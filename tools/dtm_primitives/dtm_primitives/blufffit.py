"""Bluff detection + fitting: sustained step edges on a dedicated 70 m
low-pass (L=200 would smear a 15 m face), componentized and fitted with the
exact Stage-2 sigmoid observable (profiles.bluff_cross_model). OSM
natural=cliff lines seed/validate. Valley walls are excluded — they are
modeled by the valley records."""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
from scipy import ndimage

from . import frame, profiles
from .network import SIGMA_PER_L
from .transects import Stations, resample_polyline, stations as make_stations


@dataclass
class BluffRecord:
    centerline: np.ndarray
    height_m: float
    face_grad: float
    raise_left: bool
    length_m: float
    rms: float
    height_knots: list = field(default_factory=list)
    osm_cliff_near: bool = False


def detect_and_fit(z_nan: np.ndarray, valid: np.ndarray, cell: float,
                   cfg: dict, valley_corridors: list = (),
                   cliff_lines: list = ()) -> list[BluffRecord]:
    bcfg = cfg["bluff"]
    sigma = SIGMA_PER_L * bcfg["lowpass_m"] / cell
    zf = np.where(valid, z_nan, np.nan)
    fill = np.where(valid, z_nan, float(np.nanmedian(zf)))
    lp = ndimage.gaussian_filter(fill, sigma)
    gy, gx = np.gradient(lp, cell)
    slope = np.hypot(gx, gy)
    cand = (slope >= bcfg["slope_thresh"]) & valid

    lab, n = ndimage.label(cand, structure=np.ones((3, 3), np.int8))
    out: list[BluffRecord] = []
    px_area = cell * cell
    nrows = z_nan.shape[0]
    for comp in range(1, n + 1):
        ys, xs = np.nonzero(lab == comp)
        if ys.size * px_area < bcfg["min_area_m2"]:
            continue
        x_loc, y_loc = frame.rc_to_local(ys, xs, cell, nrows)
        P = np.stack([x_loc, y_loc], axis=1)
        c = P.mean(axis=0)
        d = P - c
        cov = d.T @ d / len(d)
        w_eig, v_eig = np.linalg.eigh(cov)
        elong = float(np.sqrt(max(w_eig[1], 1e-9) / max(w_eig[0], 1e-9)))
        if elong < bcfg["elong_min"]:
            continue
        axis = v_eig[:, 1]
        t = d @ axis
        span = float(t.max() - t.min())
        if span < bcfg["min_len_m"]:
            continue
        # valley-wall exclusion with the PAIR test: valley walls flank the
        # centerline on BOTH sides; a bluff with a river at its toe (the
        # classic pairing) is one-sided — its far bank stays low. Distance
        # alone would wrongly exclude every toe-river bluff.
        skip = False
        for (cl_v, topw) in valley_corridors:
            _u, dist, _s, _t = profiles.polyline_project(
                cl_v, P[::9, 0], P[::9, 1])
            if np.median(dist) >= topw + bcfg["valley_pad_m"]:
                continue
            if _both_sides_rise(z_nan, cell, cl_v, topw):
                skip = True
                break
        if skip:
            continue
        # polyline: slope^2-weighted centroids in 20 m bins along the axis
        wts = slope[ys, xs] ** 2
        bins = np.floor((t - t.min()) / 20.0).astype(int)
        pts = []
        for b in range(bins.max() + 1):
            sel = bins == b
            if sel.sum() < 2:
                continue
            wsum = wts[sel].sum()
            pts.append((P[sel] * wts[sel, None]).sum(axis=0) / max(wsum, 1e-9))
        if len(pts) < 3:
            continue
        cl = resample_polyline(np.array(pts), 20.0)
        # 3-point moving average to calm bin noise
        if len(cl) >= 5:
            k = np.array([0.25, 0.5, 0.25])
            cl = np.stack([np.convolve(cl[:, 0], k, mode="same"),
                           np.convolve(cl[:, 1], k, mode="same")], axis=1)
            cl[0] = pts[0]
            cl[-1] = pts[-1]
        rec = _fit_bluff(z_nan, cell, cl, cfg)
        if rec is None:
            continue
        if cliff_lines:
            from scipy.spatial import cKDTree
            allc = np.concatenate([resample_polyline(np.asarray(c_), 20.0)
                                   for c_ in cliff_lines if len(c_) >= 2]) \
                if any(len(c_) >= 2 for c_ in cliff_lines) else None
            if allc is not None and len(allc):
                tree = cKDTree(allc)
                dd, _ = tree.query(rec.centerline[::2])
                rec.osm_cliff_near = bool(np.median(dd) < 60.0)
        out.append(rec)
    return out


def _both_sides_rise(z_nan: np.ndarray, cell: float, cl_v: np.ndarray,
                     topw: float) -> bool:
    """Does the valley rise on BOTH flanks (true valley) or one (bluff+toe
    river)? Sampled at 7 stations: floor vs uplands at ±1.3 top width."""
    st = make_stations(resample_polyline(cl_v, 25.0), max(cl_v.shape[0], 2)
                       * 25.0 / 7.0)
    d_up = 1.3 * max(topw, 40.0)
    zl = frame.bilinear_tif(z_nan, st.xy[:, 0] + st.normal[:, 0] * d_up,
                            st.xy[:, 1] + st.normal[:, 1] * d_up, cell)
    zr = frame.bilinear_tif(z_nan, st.xy[:, 0] - st.normal[:, 0] * d_up,
                            st.xy[:, 1] - st.normal[:, 1] * d_up, cell)
    z0 = frame.bilinear_tif(z_nan, st.xy[:, 0], st.xy[:, 1], cell)
    rl = np.nanmedian(zl - z0)
    rr = np.nanmedian(zr - z0)
    if not (np.isfinite(rl) and np.isfinite(rr)):
        return True   # unknown -> conservative exclude
    lo, hi = sorted([rl, rr])
    return bool(hi > 1.0 and lo >= 0.4 * hi)


def _fit_bluff(z_nan: np.ndarray, cell: float, cl: np.ndarray,
               cfg: dict) -> BluffRecord | None:
    bcfg = cfg["bluff"]
    st: Stations = make_stations(cl, 25.0)
    if len(st.s) < 4:
        return None
    # probe height/side at +-60 m to seed the fit
    d_probe = 60.0
    zl = frame.bilinear_tif(z_nan, st.xy[:, 0] + st.normal[:, 0] * d_probe,
                            st.xy[:, 1] + st.normal[:, 1] * d_probe, cell)
    zr = frame.bilinear_tif(z_nan, st.xy[:, 0] - st.normal[:, 0] * d_probe,
                            st.xy[:, 1] - st.normal[:, 1] * d_probe, cell)
    dz = np.nanmedian(zl - zr)
    if not np.isfinite(dz):
        return None
    raise_left = bool(dz > 0)
    h0 = float(np.clip(abs(dz), 2.0, 120.0))
    halflen = max(80.0, 2.0 * h0 / 0.3)
    dgrid = np.arange(-halflen, halflen + 1.0, 2.0)
    px = st.xy[:, 0][:, None] + st.normal[:, 0][:, None] * dgrid[None, :]
    py = st.xy[:, 1][:, None] + st.normal[:, 1][:, None] * dgrid[None, :]
    z = frame.bilinear_tif(z_nan, px.ravel(), py.ravel(), cell) \
        .reshape(px.shape)
    import warnings
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        zmed = np.nanmedian(z, axis=0)
    fin = np.isfinite(zmed)
    if fin.sum() < 30:
        return None
    dd, zz = dgrid[fin], zmed[fin]
    sign = 1.0 if raise_left else -1.0

    from scipy.optimize import least_squares

    def resid(p):
        base, g, h, fg = p
        return profiles.bluff_cross_model(dd, base, g, h, fg, sign) - zz

    p0 = [float(np.nanmedian(zz) - sign * 0.0 - h0 / 2), 0.0, h0, 0.5]
    lo = [p0[0] - 60.0, -0.35, 2.0, 0.05]
    hi = [p0[0] + 60.0, 0.35, 120.0, 2.0]
    try:
        sol = least_squares(resid, np.clip(p0, lo, hi), bounds=(lo, hi),
                            method="trf", loss="soft_l1", f_scale=0.5,
                            max_nfev=300)
    except Exception:
        return None
    base, g, h, fg = [float(v) for v in sol.x]
    rms = float(np.sqrt(np.mean(resid(sol.x) ** 2)))
    if not sol.success or rms > bcfg["rms_max_frac"] * max(h, 2.0):
        return None
    # per-100 m height knots (interior; ends excluded — taper handled by
    # the library's taper_frac)
    knots = []
    total = st.s[-1]
    if total > 200.0:
        for s_mid in np.arange(100.0, total - 99.0, 100.0):
            sel = np.abs(st.s - s_mid) <= 50.0
            if sel.sum() < 2:
                continue
            dzk = np.nanmedian(zl[sel] - zr[sel]) * sign
            if np.isfinite(dzk) and h > 0:
                knots.append([round(float(s_mid / total), 3),
                              round(float(np.clip(dzk / h, 0.0, 4.0)), 2)])
    from .transects import arc_length
    return BluffRecord(centerline=cl, height_m=round(h, 2),
                       face_grad=round(fg, 3), raise_left=raise_left,
                       length_m=round(float(arc_length(cl)), 1),
                       rms=round(rms, 3), height_knots=knots)
