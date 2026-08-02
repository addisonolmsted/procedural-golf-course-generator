"""Scan-artifact detectors on the 2 m raw grid (masked-cell-aware statistics):

1. flat surfaces — plane-detrended window std ~ 0: hydro-flattened water the
   OSM masks missed (rivers/lakes/ocean) and large pavement slabs. The
   OSM-independent backstop for the F1 unmapped-water failure.
2. curvature outliers — Evans-Young profile curvature, robust z (median/MAD),
   |z| > threshold; big components are real cliffs (logged, not masked).
3. tile seams — 3DEP project tiles are axis-aligned in the requested UTM, so a
   seam is a full row/col of anomalous steps.
4. bridge decks — ribbon-shaped components elevated above their water body
   (mapped water OR detected flat surfaces, so decks over unmapped rivers are
   caught too).

After the union, a consolidation pass demotes kept islands wherever the local
NON-GOLF inpaint density is high: those cells are graded suburban lots that
would pin the Laplace fill into a "building quilt". A confidence raster
(1 - local inpaint fraction; 0 inside inpaint) records how trustworthy each
kept cell's neighborhood is — downstream metrics weight by it.

Flagged cells are written as masks and unioned into inpaint.tif (the final
inpaint mask = class dilations + artifacts + consolidation).
"""

from __future__ import annotations

import os

import numpy as np
from scipy import ndimage

from . import config, grids, meta


def profile_curvature(z: np.ndarray, res: float) -> np.ndarray:
    """Evans-Young 3x3 quadratic-fit profile curvature (1/m). NaN-safe caller."""
    k = res
    zpad = np.pad(z, 1, mode="edge")
    z1 = zpad[:-2, :-2]; z2 = zpad[:-2, 1:-1]; z3 = zpad[:-2, 2:]
    z4 = zpad[1:-1, :-2]; z6 = zpad[1:-1, 2:]
    z7 = zpad[2:, :-2]; z8 = zpad[2:, 1:-1]; z9 = zpad[2:, 2:]
    p = (z3 + z6 + z9 - z1 - z4 - z7) / (6 * k)          # dz/dx
    q = (z1 + z2 + z3 - z7 - z8 - z9) / (6 * k)          # dz/dy
    r = (z1 + z3 + z4 + z6 + z7 + z9 - 2 * (z2 + zpad[1:-1, 1:-1] + z8)) / (3 * k * k)
    s = (z3 + z7 - z1 - z9) / (4 * k * k)
    t = (z1 + z2 + z3 + z7 + z8 + z9 - 2 * (z4 + zpad[1:-1, 1:-1] + z6)) / (3 * k * k)
    g2 = p * p + q * q
    denom = g2 * np.power(1.0 + g2, 1.5) + 1e-12
    kp = -(p * p * r + 2 * p * q * s + q * q * t) / denom
    return np.where(g2 < 1e-10, 0.0, kp)


def detrended_window_std(z: np.ndarray, win: int) -> np.ndarray:
    """Std of the residual from the best-fit plane in each win x win window,
    closed-form from uniform_filter moments. Window-local x/y are uncorrelated
    and have variance (win^2-1)/12, so
        var_resid = var(z) - cov(z,x)^2/var(x) - cov(z,y)^2/var(y).
    Detrending matters: hydro-flattened rivers are planar but SLOPED — a raw
    window std misses them."""
    zz = z.astype(np.float64)
    h, w = z.shape
    ii, jj = np.mgrid[0:h, 0:w].astype(np.float64)
    mz = ndimage.uniform_filter(zz, win)
    mzz = ndimage.uniform_filter(zz * zz, win)
    # E_w[z*(coord - center)] = uniform_filter(z*coord) - coord_center*E_w[z]
    czx = ndimage.uniform_filter(zz * jj, win) - jj * mz
    czy = ndimage.uniform_filter(zz * ii, win) - ii * mz
    var_c = (win * win - 1) / 12.0
    var = mzz - mz * mz - (czx * czx + czy * czy) / var_c
    return np.sqrt(np.maximum(var, 0.0))


def detect_flat(raw: np.ndarray, valid: np.ndarray, water: np.ndarray,
                std_m: float | None = None) -> tuple[np.ndarray, dict]:
    """Dead-flat components (>= FLAT_MIN_COMPONENT_HA) in VALID cells — run on
    valid rather than eligible so a half-mapped lake forms one coherent
    component; cells already masked cost nothing. Returns the UNDILATED mask
    (truth); the inpaint contribution dilates by FLAT_DILATE_M.
    `std_m` overrides the threshold (tiles use the strict TILE_FLAT_STD_M:
    under closed canopy the DTM is TIN-interpolation-smooth, and the course
    threshold would swallow whole forests)."""
    std = detrended_window_std(raw, config.FLAT_WIN_PX)
    flat = (std < (std_m if std_m is not None
                   else config.FLAT_DETREND_STD_M)) & valid
    stats = {"flat_components": 0, "flat_total_ha": 0.0,
             "flat_max_ha": 0.0, "flat_water_overlap": 0.0}
    if not flat.any():
        return flat, stats
    min_px = max(1, round(config.FLAT_MIN_COMPONENT_HA * 1e4
                          / config.WORK_RES_M ** 2))
    lab, n = ndimage.label(flat, structure=np.ones((3, 3), np.int8))
    sizes = np.bincount(lab.ravel())
    keep = sizes >= min_px
    keep[0] = False
    flat = keep[lab]
    if flat.any():
        px_ha = config.WORK_RES_M ** 2 / 1e4
        stats = {
            "flat_components": int(keep.sum()),
            "flat_total_ha": round(float(flat.sum()) * px_ha, 2),
            "flat_max_ha": round(float(sizes[keep].max()) * px_ha, 2),
            "flat_water_overlap": round(
                float((flat & water).sum()) / float(flat.sum()), 3),
        }
    return flat, stats


def consolidate(inpaint_pre: np.ndarray, nongolf: np.ndarray,
                golf_truth: np.ndarray,
                valid: np.ndarray) -> tuple[np.ndarray, dict]:
    """Demote kept islands where the local NON-GOLF inpaint density says the
    neighborhood is urban fabric (graded lots that would pin the fill into a
    building quilt). Single pass, no fixpoint — bounded + deterministic.
    Undilated golf-class cells are never demoted (measured course ground)."""
    win = max(3, int(round(config.CONSOLIDATE_WIN_M / config.WORK_RES_M)) | 1)
    density = ndimage.uniform_filter(
        (nongolf & valid).astype(np.float64), win)
    demote = (valid & ~inpaint_pre & ~golf_truth
              & (density > config.CONSOLIDATE_DENSITY))
    return demote, {
        "consolidated_cells": int(demote.sum()),
        "consolidated_frac": round(
            float(demote.sum()) / max(int(valid.sum()), 1), 4),
    }


def confidence_raster(inpaint: np.ndarray, valid: np.ndarray) -> np.ndarray:
    """float32 in [0,1]: 1 - local inpaint fraction for kept cells; exactly 0
    inside inpaint and on nodata. Downstream metric weighting reads this."""
    win = max(3, int(round(config.CONFIDENCE_WIN_M / config.WORK_RES_M)) | 1)
    frac = ndimage.uniform_filter((inpaint & valid).astype(np.float64), win)
    conf = np.clip(1.0 - frac, 0.0, 1.0).astype(np.float32)
    conf[inpaint | ~valid] = 0.0
    return conf


def detect_curvature(raw: np.ndarray, eligible: np.ndarray) -> tuple[np.ndarray, dict]:
    kp = profile_curvature(raw, config.WORK_RES_M)
    v = kp[eligible]
    if v.size < 100:
        return np.zeros_like(eligible), {"curvature_cells": 0, "cliff_components": 0}
    med = float(np.median(v))
    # absolute floor: on graded/near-planar ground MAD -> 0 and a raw robust-z
    # would flag numeric noise; 1e-4 1/m is far below any real earthwork edge.
    mad = max(float(np.median(np.abs(v - med))) * 1.4826, 1e-4)
    flag = (np.abs(kp - med) / mad > config.CURV_Z_THRESH) & eligible
    # the 3x3 stencil is edge-padded -> border curvature is an artifact of the
    # padding, not the data; never flag the outer ring
    flag[:2, :] = flag[-2:, :] = False
    flag[:, :2] = flag[:, -2:] = False
    flag = ndimage.binary_dilation(
        flag, structure=ndimage.generate_binary_structure(2, 2),
        iterations=config.CURV_DILATE_PX)
    lab, n = ndimage.label(flag, structure=np.ones((3, 3), np.int8))
    cliffs = 0
    if n:
        sizes = np.bincount(lab.ravel())
        for li in range(1, n + 1):
            if sizes[li] > config.CURV_MAX_COMPONENT_PX:
                flag[lab == li] = False       # real terrain band: keep, log
                cliffs += 1
    return flag, {"curvature_cells": int(flag.sum()), "cliff_components": cliffs}


def detect_seams(raw: np.ndarray, eligible: np.ndarray) -> tuple[np.ndarray, dict]:
    h, w = raw.shape
    flag = np.zeros((h, w), bool)
    rows, cols = [], []

    def steps(a, b, el):
        d = np.abs(a - b)
        d[~el] = np.nan
        return d

    row_d = steps(raw[1:, :], raw[:-1, :], eligible[1:, :] & eligible[:-1, :])
    col_d = steps(raw[:, 1:], raw[:, :-1], eligible[:, 1:] & eligible[:, :-1])
    g_med = np.nanmedian(row_d) if np.isfinite(row_d).any() else 0.0
    g_p90 = np.nanpercentile(row_d, 90) if np.isfinite(row_d).any() else 0.0
    for r in range(row_d.shape[0]):
        line = row_d[r]
        f = np.isfinite(line)
        if f.sum() < 0.3 * w:
            continue
        if (np.nanmedian(line) > config.SEAM_MEDIAN_FACTOR * max(g_med, 1e-4)
                and np.nanmean(line[f] > g_p90) >= config.SEAM_P90_FRAC):
            rows.append(r)
            flag[max(0, r - config.SEAM_WIDTH_PX // 2):
                 r + config.SEAM_WIDTH_PX // 2 + 2, :] = True
    g_med_c = np.nanmedian(col_d) if np.isfinite(col_d).any() else 0.0
    g_p90_c = np.nanpercentile(col_d, 90) if np.isfinite(col_d).any() else 0.0
    for c in range(col_d.shape[1]):
        line = col_d[:, c]
        f = np.isfinite(line)
        if f.sum() < 0.3 * h:
            continue
        if (np.nanmedian(line) > config.SEAM_MEDIAN_FACTOR * max(g_med_c, 1e-4)
                and np.nanmean(line[f] > g_p90_c) >= config.SEAM_P90_FRAC):
            cols.append(c)
            flag[:, max(0, c - config.SEAM_WIDTH_PX // 2):
                 c + config.SEAM_WIDTH_PX // 2 + 2] = True
    flag &= eligible
    return flag, {"seam_rows": rows, "seam_cols": cols}


def detect_bridges(raw: np.ndarray, water: np.ndarray,
                   valid: np.ndarray) -> tuple[np.ndarray, dict]:
    flag = np.zeros_like(valid)
    n_comp = 0
    if not water.any():
        return flag, {"bridge_components": 0}
    corridor = (ndimage.distance_transform_edt(~water) * config.WORK_RES_M
                <= config.BRIDGE_CORRIDOR_M) & valid
    wlab, wn = ndimage.label(water, structure=np.ones((3, 3), np.int8))
    corridor_lab = wlab.copy()
    if wn:
        # nearest water component id for corridor cells
        _, (iy, ix) = ndimage.distance_transform_edt(
            ~water, return_indices=True, return_distances=True)
        corridor_lab = wlab[iy, ix]
    for wi in range(1, wn + 1):
        wcells = (wlab == wi) & valid
        if wcells.sum() < 10:
            continue
        level = float(np.median(raw[wcells]))
        cand = corridor & (corridor_lab == wi) & \
            (raw > level + config.BRIDGE_MIN_RISE_M)
        lab, n = ndimage.label(cand, structure=np.ones((3, 3), np.int8))
        for li in range(1, n + 1):
            comp = lab == li
            a = int(comp.sum())
            if not (config.BRIDGE_AREA_PX[0] <= a <= config.BRIDGE_AREA_PX[1]):
                continue
            ys, xs = np.nonzero(comp)
            yc = ys - ys.mean()
            xc = xs - xs.mean()
            cxx = float((xc * xc).sum()) / a + 1 / 12
            cyy = float((yc * yc).sum()) / a + 1 / 12
            cxy = float((xc * yc).sum()) / a
            tr, det = cxx + cyy, cxx * cyy - cxy * cxy
            disc = max(tr * tr / 4 - det, 0.0) ** 0.5
            l1 = max(tr / 2 + disc, 1e-9)
            l2 = max(tr / 2 - disc, 1e-9)
            if (l1 / l2) ** 0.5 >= config.BRIDGE_ASPECT:
                flag |= comp
                n_comp += 1
    return flag, {"bridge_components": n_comp}


def stage_artifacts(key: str, force: bool) -> None:
    sdir = os.path.join(config.STORE, key)
    mdir = os.path.join(sdir, "masks")
    out_path = os.path.join(mdir, "inpaint.tif")
    # confidence.tif joins the sentinel so v1 store dirs re-run this stage
    if (os.path.exists(out_path)
            and os.path.exists(os.path.join(mdir, "confidence.tif"))
            and not force):
        return
    m = meta.read_meta(os.path.join(sdir, "meta.json"))
    epsg = int(m["crs"].split(":")[1])
    tf = grids.work_transform(tuple(m["window_utm"]))
    raw, _t, _e = grids.read_gtiff(os.path.join(sdir, "raw.tif"))
    valid = raw != config.NODATA
    inp_classes, _t2, _e2 = grids.read_gtiff(
        os.path.join(mdir, "inpaint_classes.tif"))
    inp_classes = inp_classes.astype(bool)
    water, _t3, _e3 = grids.read_gtiff(os.path.join(mdir, "water.tif"))
    water = water.astype(bool)

    def _mask(name: str) -> np.ndarray:
        arr, _tt, _ee = grids.read_gtiff(os.path.join(mdir, f"{name}.tif"))
        return arr.astype(bool)

    # flat first: water surfaces have ~zero curvature and would shrink the
    # curvature MAD if left inside the detector statistics. On TILES the
    # inpaint contribution uses the strict water-planarity threshold; the
    # course-threshold fraction is recorded as a canopy-smoothness
    # diagnostic instead of masking whole forests.
    from .masks import dilate_m
    if config.DATASET == "tiles":
        flat, flat_stats = detect_flat(raw, valid, water,
                                       std_m=config.TILE_FLAT_STD_M)
        broad, _bs = detect_flat(raw, valid, water)
        flat_stats["canopy_smooth_frac"] = round(
            float(broad.sum()) / max(int(valid.sum()), 1), 4)
        flat_d = dilate_m(flat, config.TILE_FLAT_DILATE_M, config.WORK_RES_M)
    else:
        flat, flat_stats = detect_flat(raw, valid, water)
        flat_d = dilate_m(flat, config.FLAT_DILATE_M, config.WORK_RES_M)

    eligible = valid & ~inp_classes & ~flat_d   # stats unpolluted by man-made
    curv, curv_stats = detect_curvature(raw, eligible)
    seam, seam_stats = detect_seams(raw, eligible)
    # pseudo-water from the flat detector de-tethers bridge detection from OSM
    bridge, bridge_stats = detect_bridges(raw, water | flat, valid)

    for name, mask in (("artifact_flat", flat), ("artifact_curvature", curv),
                       ("artifact_seam", seam), ("artifact_bridge", bridge)):
        grids.write_gtiff(os.path.join(mdir, f"{name}.tif"),
                          mask.astype(np.uint8), tf, epsg)

    # TILES: the curvature detector and urban consolidation are COURSE
    # instruments (post-construction damage, suburban membrane quilts).
    # On natural tiles the curvature z-score is self-referential against
    # interpolation-smooth forest floor and fires on real creek banks and
    # bluff edges — the very signal the tiles exist to provide. Curvature
    # stays recorded as a diagnostic; only mapped classes, true-flat water,
    # seams, and bridge decks join the tile inpaint.
    if config.DATASET == "tiles":
        inpaint_pre = (inp_classes | seam | bridge | flat_d) & valid
        demote = np.zeros_like(inpaint_pre)
        cons_stats = {"consolidated_frac": 0.0}
    else:
        inpaint_pre = (inp_classes | curv | seam | bridge | flat_d) & valid
        # consolidation: driver = non-golf class dilations + everything the
        # detectors found; protected = undilated golf-class ground
        nongolf = _mask("inpaint_nongolf") | curv | seam | bridge | flat_d
        golf_truth = (_mask("fairway") | _mask("green") | _mask("tee")
                      | _mask("bunker"))
        demote, cons_stats = consolidate(inpaint_pre, nongolf, golf_truth,
                                         valid)
    inpaint = inpaint_pre | demote
    # seal 1-px slivers between masked features. border_value=1: the default
    # (0) ERODES mask pixels touching the array edge — harmless on courses
    # (the 500 m margin keeps water off borders) but a 3 km tile cuts through
    # streams at its edge, and eroded border water broke the excision
    # invariant.
    inpaint = ndimage.binary_closing(
        inpaint, structure=np.ones((3, 3), np.int8), border_value=1) & valid
    grids.write_gtiff(os.path.join(mdir, "consolidated.tif"),
                      (inpaint & ~inpaint_pre).astype(np.uint8), tf, epsg)
    grids.write_gtiff(out_path, inpaint.astype(np.uint8), tf, epsg)

    conf = confidence_raster(inpaint, valid)
    grids.write_gtiff(os.path.join(mdir, "confidence.tif"), conf, tf, epsg)

    interior = _mask("boundary_interior")
    kept = valid & ~inpaint
    residual_flat = flat & kept
    m["artifacts"] = {**flat_stats, **curv_stats, **seam_stats, **bridge_stats,
                      **cons_stats}
    m["inpaint"] = {
        "cells": int(inpaint.sum()),
        "frac": round(float(inpaint.sum()) / max(int(valid.sum()), 1), 4),
    }
    px_ha = config.WORK_RES_M ** 2 / 1e4
    rf_max_ha = 0.0
    if residual_flat.any():
        rl, rn = ndimage.label(residual_flat, structure=np.ones((3, 3), np.int8))
        rf_max_ha = round(float(np.bincount(rl.ravel())[1:].max()) * px_ha, 3)
    m["confidence"] = {
        "interior_kept_frac": round(
            float((kept & interior).sum()) / max(int((valid & interior).sum()), 1), 4),
        "exterior_kept_frac": round(
            float((kept & ~interior).sum()) / max(int((valid & ~interior).sum()), 1), 4),
        "residual_flat_frac": round(
            float(residual_flat.sum()) / max(int(kept.sum()), 1), 5),
        "residual_flat_ha_max": rf_max_ha,
        "calib_confidence": round(float(conf[valid].mean()), 4) if valid.any() else 0.0,
    }
    meta.write_meta(os.path.join(sdir, "meta.json"), m)
