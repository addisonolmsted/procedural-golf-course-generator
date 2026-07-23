"""Bowl detection + fitting: pre-fill closed-depression analysis (the
depression EXISTS before priority-flood fills it), spill-level boundary via
marching squares, radial median-profile fit on the exact Stage-2 inside form
(rim − smin(ramp(d_in, inner_grad, rim_round), depth, floor_k)), outlet
classification (mapped-water lakes vs spillway at the pour point). Blob
parameters (radius/wobble/cycles) are measurement-side priors only — perlin
phase is unrecoverable, so emitted boundaries are Points."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
from scipy import ndimage

from metrics import core as v1core

from . import frame, profiles


@dataclass
class BowlRecord:
    boundary: np.ndarray            # (N,2) local, closed not required
    center: tuple
    rim_z: float
    depth_m: float
    inner_grad: float
    rim_round_m: float
    radius_m: float                 # mean boundary radius
    wobble: float
    cycles: int
    is_lake: bool
    spill_at_s: float               # normalized boundary arc of the pour point
    water_frac: float
    inpaint_frac: float
    rms: float


def _block(a: np.ndarray, k: int, reduce=np.nanmean) -> np.ndarray:
    ny, nx = a.shape[0] // k, a.shape[1] // k
    import warnings
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        return reduce(a[: ny * k, : nx * k].reshape(ny, k, nx, k),
                      axis=(1, 3))


def detect_and_fit(z_nan: np.ndarray, valid: np.ndarray, cell: float,
                   cfg: dict, water: np.ndarray | None = None,
                   inpaint: np.ndarray | None = None,
                   valley_corridors: list = ()) -> list[BowlRecord]:
    bcfg = cfg["bowl"]
    k = max(int(round(bcfg["block_m"] / cell)), 1)
    zb = _block(np.where(valid, z_nan, np.nan), k)
    vb = np.isfinite(zb)
    cellb = cell * k
    filled = v1core.fill_depressions(zb, cellb, vb)
    depth = np.where(vb, filled - np.nan_to_num(zb), 0.0)
    core_mask = depth >= bcfg["depth_floor_m"]
    lab, n = ndimage.label(core_mask, structure=np.ones((3, 3), np.int8))
    out: list[BowlRecord] = []
    nrows_b = zb.shape[0]
    for comp in range(1, n + 1):
        sel = lab == comp
        dmax = float(depth[sel].max())
        if dmax < bcfg["min_depth_m"]:
            continue
        core = sel & (depth >= 0.5 * dmax)
        if core.sum() * cellb * cellb < bcfg["min_area_m2"]:
            continue
        ys, xs = np.nonzero(sel)
        # elongated depressions overlapping a valley corridor are dammed
        # valley segments, not bowls
        x_loc, y_loc = frame.rc_to_local(ys, xs, cellb, nrows_b)
        P = np.stack([x_loc, y_loc], axis=1)
        d = P - P.mean(axis=0)
        w_eig = np.linalg.eigvalsh(d.T @ d / len(d))
        elong = float(np.sqrt(max(w_eig[1], 1e-9) / max(w_eig[0], 1e-9)))
        near_valley = False
        for (cl_v, topw) in valley_corridors:
            _u, dist, _s, _t = profiles.polyline_project(
                cl_v, P[::9, 0], P[::9, 1])
            if np.median(dist) < topw + 30.0:
                near_valley = True
                break
        if near_valley and elong >= 3.0:
            continue

        # spill level + boundary contour around the depth maximum
        z_spill = float(np.nanmedian(filled[sel]))
        iy, ix = np.unravel_index(int(np.argmax(np.where(sel, depth, 0.0))),
                                  depth.shape)
        pad = 8
        y0b, y1b = max(iy - 1, 0), min(iy + 2, zb.shape[0])
        bb_ys, bb_xs = np.nonzero(sel)
        r0 = max(bb_ys.min() - pad, 0)
        r1 = min(bb_ys.max() + pad + 1, zb.shape[0])
        c0 = max(bb_xs.min() - pad, 0)
        c1 = min(bb_xs.max() + pad + 1, zb.shape[1])
        zc = np.nan_to_num(zb[r0:r1, c0:c1], nan=z_spill + 100.0)
        from skimage import measure
        conts = measure.find_contours(zc, z_spill)
        boundary = None
        for cont in sorted(conts, key=len, reverse=True):
            # contour rows/cols (crop frame) -> does it enclose the pit?
            from matplotlib.path import Path as MplPath
            if MplPath(cont).contains_point((iy - r0, ix - c0)):
                boundary = cont
                break
        if boundary is None or len(boundary) < 8:
            continue
        b_rows = boundary[:, 0] + r0
        b_cols = boundary[:, 1] + c0
        bx, by = frame.rc_to_local(b_rows, b_cols, cellb, nrows_b)
        bnd = np.stack([bx, by], axis=1)
        if len(bnd) > bcfg["boundary_max_pts"]:
            step = len(bnd) // bcfg["boundary_max_pts"] + 1
            bnd = bnd[::step]

        cx, cy = float(bx.mean()), float(by.mean())
        radii = np.hypot(bnd[:, 0] - cx, bnd[:, 1] - cy)
        radius = float(radii.mean())
        wobble = float(np.sqrt(2.0) * radii.std() / max(radius, 1e-9))
        # dominant harmonic k>=2 (k=1 is centroid offset)
        f = np.abs(np.fft.rfft(radii - radii.mean()))
        cycles = int(np.argmax(f[2:7]) + 2) if len(f) > 7 else 2

        # rim CREST per boundary normal: the spill-level boundary of a
        # spillway bowl sits on the inner wall well below the rim, so a
        # fixed ring underestimates it — scan outward and take the max
        # (for lakes the crest is at the boundary; the scan is a no-op)
        nrm = (bnd - np.array([cx, cy])) / np.maximum(radii, 1e-9)[:, None]
        scan = np.arange(bcfg["ring_offset_m"], 80.0 + 1e-9, 5.0)
        rx = bnd[:, 0][:, None] + nrm[:, 0][:, None] * scan[None, :]
        ry = bnd[:, 1][:, None] + nrm[:, 1][:, None] * scan[None, :]
        rz = frame.bilinear_tif(z_nan, rx.ravel(), ry.ravel(), cell) \
            .reshape(rx.shape)
        import warnings as _w
        with _w.catch_warnings():
            _w.simplefilter("ignore", RuntimeWarning)
            rim_samples = np.nanmax(rz, axis=1)
        rim_z = float(np.nanpercentile(rim_samples, 75))
        inside_frac = _mask_frac_inside(water, sel, k) if water is not None \
            else 0.0
        inp_frac = _mask_frac_inside(inpaint, sel, k) if inpaint is not None \
            else 0.0

        # radial median profile from the centroid, fitted with the Stage-2
        # inside form
        rays = cfg["bowl"]["n_rays"]
        prof_d = np.arange(0.0, radius * 1.2, cellb)
        angs = np.linspace(0, 2 * np.pi, rays, endpoint=False)
        px = cx + np.cos(angs)[:, None] * prof_d[None, :]
        py = cy + np.sin(angs)[:, None] * prof_d[None, :]
        zz = frame.bilinear_tif(z_nan, px.ravel(), py.ravel(), cell) \
            .reshape(rays, -1)
        import warnings
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", RuntimeWarning)
            zprof = np.nanmedian(zz, axis=0)
        fin = np.isfinite(zprof)
        if fin.sum() < 8:
            continue
        d_in = radius - prof_d[fin]          # distance INSIDE the boundary
        zv = zprof[fin]
        depth0 = float(np.clip(rim_z - np.nanmin(zv), 0.5, 60.0))

        from scipy.optimize import least_squares

        def model(p):
            rimz, dep, ig, rr = p
            return rimz - profiles.smin(
                profiles.ramp(d_in, ig, rr), dep, 3.0)

        def resid(p):
            return model(p) - zv

        p0 = [rim_z, depth0, 0.25, 8.0]
        lo = [rim_z - 10.0, 0.5, 0.02, 1.0]
        hi = [rim_z + 10.0, 60.0, 1.5, 25.0]
        try:
            sol = least_squares(resid, np.clip(p0, lo, hi), bounds=(lo, hi),
                                method="trf", loss="soft_l1", f_scale=0.5,
                                max_nfev=300)
        except Exception:
            continue
        rimf, depf, igf, rrf = [float(v) for v in sol.x]
        rms = float(np.sqrt(np.mean(resid(sol.x) ** 2)))
        if not sol.success or rms > max(0.6, 0.15 * depf):
            continue
        # DIRECT depth: rim crest − spill level + pre-fill depth below spill.
        # The radial fit's boundary is the SPILL contour, so its depth reads
        # pond-depth-below-spill for spillway bowls; the identity below is
        # registration-free (lake: spill ≈ rim low point, so it degrades to
        # crest − floor as expected).
        depth_direct = float(np.clip(rim_z - z_spill + dmax, 0.5, 80.0))

        is_lake = inside_frac >= bcfg["lake_water_frac"]
        # pour point: boundary arc position of the lowest outward-ring sample
        j = int(np.nanargmin(rim_samples))
        seg = np.hypot(*np.diff(bnd, axis=0).T)
        cum = np.concatenate([[0.0], np.cumsum(seg)])
        at_s = float(cum[min(j, len(cum) - 1)] / max(cum[-1], 1e-9))

        out.append(BowlRecord(
            boundary=bnd, center=(round(cx, 1), round(cy, 1)),
            rim_z=round(rim_z, 2), depth_m=round(depth_direct, 2),
            inner_grad=round(igf, 3), rim_round_m=round(rrf, 2),
            radius_m=round(radius, 1), wobble=round(wobble, 3),
            cycles=cycles, is_lake=bool(is_lake),
            spill_at_s=round(at_s, 3), water_frac=round(inside_frac, 3),
            inpaint_frac=round(inp_frac, 3), rms=round(rms, 3)))
    return out


def _mask_frac_inside(mask: np.ndarray, sel_block: np.ndarray,
                      k: int) -> float:
    mb = _block(mask.astype(float), k) > 0.5
    return float(mb[sel_block].mean()) if sel_block.any() else 0.0
