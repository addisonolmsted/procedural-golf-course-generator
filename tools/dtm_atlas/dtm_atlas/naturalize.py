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


def _jacobi_sweeps(out: np.ndarray, comp: np.ndarray,
                   support: np.ndarray, sweeps: int) -> None:
    """In-place masked Jacobi: each comp cell becomes the mean of its defined
    (support) 4-neighbors. Padded-slice neighbors — no wrap-around. Convex
    combinations only, so the range of support values is never escaped."""
    for _ in range(sweeps):
        val = np.where(support, out, 0.0)
        cnt = support.astype(np.float64)
        s = np.zeros_like(val)
        c = np.zeros_like(cnt)
        s[1:, :] += val[:-1, :]; c[1:, :] += cnt[:-1, :]
        s[:-1, :] += val[1:, :]; c[:-1, :] += cnt[1:, :]
        s[:, 1:] += val[:, :-1]; c[:, 1:] += cnt[:, :-1]
        s[:, :-1] += val[:, 1:]; c[:, :-1] += cnt[:, 1:]
        new = np.where(c > 0, s / np.maximum(c, 1.0), val)
        out[comp] = new[comp]


def _restrict(z: np.ndarray, known: np.ndarray, comp: np.ndarray):
    """2x block-mean restriction. Coarse known = block had a known cell (and
    no comp claim); coarse comp = block had a comp cell; coarse z = mean of
    the block's known values where any, else 0 (never read)."""
    h, w = z.shape
    h2, w2 = (h + 1) // 2, (w + 1) // 2
    zp = np.pad(z, ((0, h2 * 2 - h), (0, w2 * 2 - w)), mode="edge")
    kp = np.pad(known, ((0, h2 * 2 - h), (0, w2 * 2 - w)), mode="constant")
    cp = np.pad(comp, ((0, h2 * 2 - h), (0, w2 * 2 - w)), mode="constant")
    zb = np.where(kp, zp, 0.0).reshape(h2, 2, w2, 2)
    kb = kp.reshape(h2, 2, w2, 2)
    kcnt = kb.sum(axis=(1, 3)).astype(np.float64)
    zc = zb.sum(axis=(1, 3)) / np.maximum(kcnt, 1.0)
    compc = cp.reshape(h2, 2, w2, 2).any(axis=(1, 3))
    knownc = (kcnt > 0) & ~compc
    return zc, knownc, compc


def _multiscale_fill(z: np.ndarray, comp: np.ndarray,
                     known: np.ndarray) -> np.ndarray:
    """True coarse-to-fine Laplace for oversized components: restrict until the
    component fits MG_COARSE_TARGET_CELLS, spsolve the coarsest level, then
    bilinear-prolong + fixed Jacobi sweeps per level. Deterministic: level
    count fixed by shape, fixed sweeps, no convergence branching. Every value
    is a convex combination of in-view known values (max principle holds
    against the view's known range)."""
    if int(comp.sum()) <= config.MG_COARSE_TARGET_CELLS:
        out = z.copy()
        x, _rmin, _rmax = _solve_component(out, known, comp)
        if x is None:
            out[comp] = float(np.median(z[known])) if known.any() else 0.0
        else:
            ys, xs = np.nonzero(comp)
            out[ys, xs] = x
        return out
    zc, knownc, compc = _restrict(z, known, comp)
    filled_c = _multiscale_fill(zc, compc, knownc)
    up = ndimage.zoom(filled_c, 2, order=1, mode="nearest",
                      grid_mode=True)[: z.shape[0], : z.shape[1]]
    out = z.copy()
    out[comp] = up[comp]
    _jacobi_sweeps(out, comp, known | comp, config.MG_SMOOTH_SWEEPS)
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
            filled = _multiscale_fill(zv, comp, kv)
            # max principle vs the view's known range (coarse Dirichlet draws
            # on block means of these values; slack for float accumulation)
            if kv.any():
                kmin, kmax = float(zv[kv].min()), float(zv[kv].max())
                fmin, fmax = float(filled[comp].min()), float(filled[comp].max())
                slack = 1e-4 + 1e-9 * max(abs(kmin), abs(kmax))
                if fmin < kmin - slack or fmax > kmax + slack:
                    raise AssertionError(
                        f"multigrid fill [{fmin:.3f},{fmax:.3f}] escapes known "
                        f"range [{kmin:.3f},{kmax:.3f}]")
            zv[comp] = filled[comp]
            solver = "laplace_spsolve+multigrid"
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
    # F1 gate: detected dead-flat surfaces (unmapped water/pavement) excised too
    fpath = os.path.join(mdir, "artifact_flat.tif")
    if os.path.exists(fpath):
        flat, _tf5, _e5 = grids.read_gtiff(fpath)
        unflat = flat.astype(bool) & valid & ~inpaint
        if unflat.any():
            raise AssertionError(
                f"{int(unflat.sum())} flat-artifact cells not in inpaint")

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
