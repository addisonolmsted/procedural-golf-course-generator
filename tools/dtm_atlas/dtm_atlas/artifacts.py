"""Scan-artifact detectors on the 2 m raw grid (masked-cell-aware statistics):

1. curvature outliers — Evans-Young profile curvature, robust z (median/MAD),
   |z| > threshold; big components are real cliffs (logged, not masked).
2. tile seams — 3DEP project tiles are axis-aligned in the requested UTM, so a
   seam is a full row/col of anomalous steps.
3. bridge decks — ribbon-shaped components elevated above their water body.

Flagged cells are written as masks and unioned into inpaint.tif (the final
inpaint mask = class dilations + artifacts).
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
    if os.path.exists(out_path) and not force:
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

    eligible = valid & ~inp_classes    # stats unpolluted by known man-made cells
    curv, curv_stats = detect_curvature(raw, eligible)
    seam, seam_stats = detect_seams(raw, eligible)
    bridge, bridge_stats = detect_bridges(raw, water, valid)

    for name, mask in (("artifact_curvature", curv), ("artifact_seam", seam),
                       ("artifact_bridge", bridge)):
        grids.write_gtiff(os.path.join(mdir, f"{name}.tif"),
                          mask.astype(np.uint8), tf, epsg)
    inpaint = (inp_classes | curv | seam | bridge) & valid
    grids.write_gtiff(out_path, inpaint.astype(np.uint8), tf, epsg)

    m["artifacts"] = {**curv_stats, **seam_stats, **bridge_stats}
    m["inpaint"] = {
        "cells": int(inpaint.sum()),
        "frac": round(float(inpaint.sum()) / max(int(valid.sum()), 1), 4),
    }
    meta.write_meta(os.path.join(sdir, "meta.json"), m)
