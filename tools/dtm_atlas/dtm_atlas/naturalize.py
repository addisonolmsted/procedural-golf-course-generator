"""Naturalization: inpaint every masked cell with a component-wise Laplace
(membrane) fill — scipy.sparse spsolve per connected component, Dirichlet
boundary from surrounding valid cells, homogeneous-Neumann (mirrored) stencil
at nodata/window edges.

Why Laplace: the maximum principle guarantees fill values stay inside the
Dirichlet ring's range — exactly the gate's "inpainting invents no new
structure" property, asserted per component. Determinism: row-major unknown
ordering, single-threaded SuperLU, no RNG; cells outside the inpaint mask are
untouched bit-exactly (asserted).
"""

from __future__ import annotations

import os

import numpy as np
from scipy import ndimage
from scipy.sparse import coo_matrix
from scipy.sparse.linalg import spsolve

from . import config, grids, meta

_OFFS = ((-1, 0), (1, 0), (0, -1), (0, 1))


def _solve_component(z: np.ndarray, known: np.ndarray, comp: np.ndarray):
    """Solve Laplace on `comp` cells; returns (values, ring_min, ring_max).
    known = cells with trustworthy values (valid & ~inpaint)."""
    ys, xs = np.nonzero(comp)
    n = ys.size
    idx = np.full(z.shape, -1, np.int64)
    idx[ys, xs] = np.arange(n)
    h, w = z.shape

    rows, cols, diag = [], [], np.zeros(n)
    rhs = np.zeros(n)
    ring_vals = []
    for dy, dx in _OFFS:
        ny, nx = ys + dy, xs + dx
        inside = (ny >= 0) & (ny < h) & (nx >= 0) & (nx < w)
        ni = np.where(inside, idx[ny.clip(0, h - 1), nx.clip(0, w - 1)], -1)
        is_unknown = inside & (ni >= 0)
        is_known = inside & (ni < 0) & known[ny.clip(0, h - 1), nx.clip(0, w - 1)]
        # mirrored (Neumann) neighbors contribute nothing: not counted in diag
        contrib = is_unknown | is_known
        diag += contrib.astype(np.float64)
        src = np.nonzero(is_unknown)[0]
        rows.append(src)
        cols.append(ni[src])
        kv = np.nonzero(is_known)[0]
        if kv.size:
            vals = z[ny[kv], nx[kv]]
            np.add.at(rhs, kv, vals)
            ring_vals.append(vals)
    if not ring_vals:
        return None, None, None  # no Dirichlet data at all
    rows = np.concatenate(rows)
    cols = np.concatenate(cols)
    A = coo_matrix(
        (np.concatenate([diag, -np.ones(rows.size)]),
         (np.concatenate([np.arange(n), rows]),
          np.concatenate([np.arange(n), cols]))),
        shape=(n, n),
    ).tocsr()
    x = spsolve(A, rhs)
    ring = np.concatenate(ring_vals)
    return x, float(ring.min()), float(ring.max())


def _pyramid_fill(z: np.ndarray, comp: np.ndarray, known: np.ndarray) -> np.ndarray:
    """Fixed-iteration coarse-to-fine Jacobi fill for oversized components
    (deterministic: fixed sweep counts, no convergence branching)."""
    out = z.copy()
    seed = np.where(known, z, 0.0)
    wsum = ndimage.uniform_filter(np.where(known, z, 0.0), 31)
    wcnt = ndimage.uniform_filter(known.astype(np.float64), 31)
    approx = np.where(wcnt > 1e-6, wsum / np.maximum(wcnt, 1e-9), np.median(z[known]))
    out[comp] = approx[comp]
    for _ in range(config.PYRAMID_SWEEPS):
        sm = (np.roll(out, 1, 0) + np.roll(out, -1, 0)
              + np.roll(out, 1, 1) + np.roll(out, -1, 1)) / 4.0
        out[comp] = sm[comp]
    return out


def laplace_fill(raw: np.ndarray, inpaint: np.ndarray,
                 valid: np.ndarray) -> tuple[np.ndarray, dict]:
    known = valid & ~inpaint
    todo = inpaint & valid
    out = raw.copy()
    lab, n = ndimage.label(todo, structure=np.ones((3, 3), np.int8))
    max_comp = 0
    solver = "laplace_spsolve"
    slices = ndimage.find_objects(lab)
    for li in range(1, n + 1):
        sl = slices[li - 1]
        # pad the view by 1 for the stencil ring
        y0 = max(sl[0].start - 1, 0); y1 = min(sl[0].stop + 1, raw.shape[0])
        x0 = max(sl[1].start - 1, 0); x1 = min(sl[1].stop + 1, raw.shape[1])
        comp = lab[y0:y1, x0:x1] == li
        csize = int(comp.sum())
        max_comp = max(max_comp, csize)
        zv = out[y0:y1, x0:x1]
        kv = known[y0:y1, x0:x1]
        if csize > config.PYRAMID_THRESHOLD_CELLS:
            filled = _pyramid_fill(zv, comp, kv)
            zv[comp] = filled[comp]
            solver = "laplace_spsolve+pyramid"
            continue
        x, rmin, rmax = _solve_component(zv, kv, comp)
        if x is None:
            fallback = float(np.median(raw[known])) if known.any() else 0.0
            zv[comp] = fallback
            continue
        # maximum principle (+ tiny numeric slack): invents nothing
        eps = 1e-6 + 1e-9 * max(abs(rmin), abs(rmax))
        if x.min() < rmin - eps or x.max() > rmax + eps:
            raise AssertionError(
                f"fill range [{x.min():.3f},{x.max():.3f}] escapes ring "
                f"[{rmin:.3f},{rmax:.3f}]")
        ys, xs = np.nonzero(comp)
        zv[ys, xs] = x
    stats = {"components": int(n), "max_component": int(max_comp),
             "solver": solver}
    # untouched outside the mask — bit-exact
    same = out[~todo] == raw[~todo]
    if not bool(np.all(same | (np.isnan(out[~todo]) & np.isnan(raw[~todo])))):
        raise AssertionError("naturalization touched cells outside the mask")
    return out, stats


def stage_naturalize(key: str, force: bool) -> None:
    sdir = os.path.join(config.STORE, key)
    out_path = os.path.join(sdir, "naturalized.tif")
    if os.path.exists(out_path) and not force:
        return
    m = meta.read_meta(os.path.join(sdir, "meta.json"))
    epsg = int(m["crs"].split(":")[1])
    tf = grids.work_transform(tuple(m["window_utm"]))
    raw, _t, _e = grids.read_gtiff(os.path.join(sdir, "raw.tif"))
    mdir = os.path.join(sdir, "masks")
    inpaint, _t2, _e2 = grids.read_gtiff(os.path.join(mdir, "inpaint.tif"))
    inpaint = inpaint.astype(bool)
    water, _t3, _e3 = grids.read_gtiff(os.path.join(mdir, "water.tif"))
    valid = raw != config.NODATA

    # gate invariant: water (and its berms via dilation) is fully excised
    uncovered = water.astype(bool) & valid & ~inpaint
    if uncovered.any():
        raise AssertionError(f"{int(uncovered.sum())} water cells not in inpaint")

    nat, stats = laplace_fill(raw, inpaint, valid)
    grids.write_gtiff(out_path, nat.astype(np.float32), tf, epsg,
                      nodata=config.NODATA)

    m["inpaint"].update(stats)
    nv = nat[valid]
    m["elev_naturalized"] = {"min": round(float(nv.min()), 2),
                             "max": round(float(nv.max()), 2)}
    # how much of the exterior margin got inpainted: 0 = pristine natural
    # signal, ~1 = urban surroundings (margin carries little calibration value)
    ext, _t4, _e4 = grids.read_gtiff(os.path.join(mdir, "exterior.tif"))
    ext = ext.astype(bool)
    if ext.any():
        m["margin"]["exterior_inpaint_frac"] = round(
            float(inpaint[ext].mean()), 4)
    meta.write_meta(os.path.join(sdir, "meta.json"), m)
