"""Valley parameter fitting: per-reach cross-section least-squares on the
EXACT Stage-2 observable form, and the robust longitudinal floor fit.
Parameters map 1:1 onto MacroConfig Valley fields."""

from __future__ import annotations

from dataclasses import dataclass, asdict

import numpy as np
from scipy.optimize import least_squares
from scipy.stats import theilslopes

from . import profiles
from .transects import Reach


@dataclass
class CrossFit:
    zf: float
    inc: float
    gl: float
    gr: float
    hw: float
    ml: float
    mr: float
    r: float
    c0: float      # lateral center offset (m): model center minus transect d=0
    rms: float
    n_pts: int
    ok: bool
    reason: str


def fit_cross(reach: Reach, cfg: dict) -> CrossFit:
    d = reach.d
    z = reach.z_med
    fin = np.isfinite(z)
    k = cfg["shoulder_k_m"]

    def fail(reason, **kw):
        base = dict(zf=np.nan, inc=np.nan, gl=np.nan, gr=np.nan, hw=np.nan,
                    ml=np.nan, mr=np.nan, r=np.nan, c0=np.nan, rms=np.nan,
                    n_pts=int(fin.sum()), ok=False, reason=reason)
        base.update(kw)
        return CrossFit(**base)

    if fin.sum() < 40:
        return fail("too_few_samples")
    df, zfv = d[fin], z[fin]
    # per-side FINITE extents (window edges truncate transects asymmetrically)
    ext_l = float(-df.min()) if (df < 0).any() else 0.0
    ext_r = float(df.max()) if (df > 0).any() else 0.0
    half = min(ext_l, ext_r) if min(ext_l, ext_r) > 0 else max(ext_l, ext_r)
    if half < 60.0:
        return fail("too_truncated")

    # ---- geometric inits ----
    central = np.abs(df) <= 30.0
    if central.sum() < 5:
        return fail("no_central_samples")
    zf0 = float(np.percentile(zfv[central], 5))
    outer_l = df < -2.0 * ext_l / 3.0
    outer_r = df > 2.0 * ext_r / 3.0
    if outer_l.sum() < 5 or outer_r.sum() < 5:
        return fail("no_outer_samples")
    t0l = float(np.percentile(zfv[outer_l], 80))
    t0r = float(np.percentile(zfv[outer_r], 80))
    inc0 = float(np.clip(0.5 * (t0l + t0r) - zf0, 0.5, 100.0))
    gl0 = float(np.polyfit(df[outer_l], zfv[outer_l], 1)[0])
    gr0 = float(np.polyfit(df[outer_r], zfv[outer_r], 1)[0])
    low = fin & (z < zf0 + 0.15 * inc0)
    hw0 = float(np.clip((np.abs(d[low]).max() if low.any() else 8.0), 2.0, 100.0))

    def limb_slope(side):
        sel = fin & (np.sign(d) == side) \
            & (z > zf0 + 0.2 * inc0) & (z < zf0 + 0.8 * inc0)
        if sel.sum() < 4:
            return 0.3
        dz = np.diff(z[sel]) / np.diff(d[sel])
        dz = dz[np.isfinite(dz)]
        s = float(np.percentile(np.abs(dz), 75)) if dz.size else 0.3
        return float(np.clip(s, 0.04, 1.4))

    ml0, mr0 = limb_slope(+1), limb_slope(-1)

    hw_hi = float(min(250.0, 0.75 * half))  # wide floodplains: scale w/ transect
    p0 = np.array([zf0, inc0, gl0, gr0, hw0, ml0, mr0, 6.0, 0.0])
    lo = np.array([zf0 - 2.0, 0.3, -0.35, -0.35, 1.0, 0.03, 0.03, 1.0, -60.0])
    hi = np.array([zf0 + 3.0, 120.0, 0.35, 0.35, hw_hi, 1.5, 1.5, 20.0, 60.0])
    p0 = np.clip(p0, lo + 1e-9, hi - 1e-9)

    def model(p):
        zf, inc, gl, gr, hw, ml, mr, r, c0 = p
        return profiles.valley_cross_model(df - c0, zf, inc, gl, gr, hw,
                                           ml, mr, r, k)

    def resid(p):
        return model(p) - zfv

    try:
        sol = least_squares(resid, p0, bounds=(lo, hi), method="trf",
                            loss="soft_l1", f_scale=cfg["f_scale"],
                            max_nfev=400)
    except Exception:
        return fail("solver_error")
    zf, inc, gl, gr, hw, ml, mr, r, c0 = [float(v) for v in sol.x]
    rms = float(np.sqrt(np.mean((model(sol.x) - zfv) ** 2)))

    def pinned(v, lo_v, hi_v):
        return (v - lo_v) < 1e-6 * max(abs(hi_v - lo_v), 1.0) \
            or (hi_v - v) < 1e-6 * max(abs(hi_v - lo_v), 1.0)

    ok = True
    reason = "ok"
    if not sol.success:
        ok, reason = False, "no_convergence"
    elif inc < cfg["min_incision_m"]:
        ok, reason = False, "no_valley_expression"
    elif rms > max(cfg["rms_max_abs_m"], cfg["rms_max_frac"] * inc):
        ok, reason = False, "rms_too_high"
    elif pinned(hw, 1.0, hw_hi) or pinned(ml, 0.03, 1.5) \
            or pinned(mr, 0.03, 1.5) or pinned(c0, -60.0, 60.0):
        ok, reason = False, "bound_pinned"
    return CrossFit(zf=zf, inc=inc, gl=gl, gr=gr, hw=hw, ml=ml, mr=mr, r=r,
                    c0=c0, rms=rms, n_pts=int(fin.sum()), ok=ok, reason=reason)


@dataclass
class LongFit:
    floor_z0: float          # at s = 0 (upstream) of the station arc
    fall_raw: float
    fall_clamped: float
    resid_p95: float
    nonmonotone_rise_m: float
    n_stations: int


MIN_FALL_GRADIENT = 5e-4  # Stage-2 contract (valley.rs)


def fit_longitudinal(s: np.ndarray, z_floor: np.ndarray, cfg: dict) -> LongFit | None:
    fin = np.isfinite(z_floor) & np.isfinite(s)
    if fin.sum() < 8:
        return None
    sf, zf = s[fin], z_floor[fin]
    slope, intercept, _lo, _hi = theilslopes(zf, sf)
    fall_raw = float(-slope)
    resid = zf - (intercept + slope * sf)
    # rolling-median-5 downstream rise
    rm = _rolling_median(zf, 5)
    rise = rm - np.minimum.accumulate(rm)
    return LongFit(floor_z0=float(intercept), fall_raw=fall_raw,
                   fall_clamped=float(max(fall_raw, MIN_FALL_GRADIENT)),
                   resid_p95=float(np.percentile(np.abs(resid), 95)),
                   nonmonotone_rise_m=float(np.nanmax(rise)),
                   n_stations=int(fin.sum()))


def _rolling_median(v: np.ndarray, w: int) -> np.ndarray:
    pad = w // 2
    vp = np.pad(v, pad, mode="edge")
    return np.median(np.lib.stride_tricks.sliding_window_view(vp, w), axis=1)


def weighted_median(vals: list[float], weights: list[int]) -> float:
    v = np.asarray(vals, float)
    w = np.asarray(weights, float)
    fin = np.isfinite(v) & (w > 0)
    if not fin.any():
        return float("nan")
    order = np.argsort(v[fin])
    vv, ww = v[fin][order], w[fin][order]
    cw = np.cumsum(ww)
    return float(vv[np.searchsorted(cw, 0.5 * cw[-1])])


def build_valley(cl_ext: np.ndarray, ext_up_m: float, crossfits: list[CrossFit],
                 reaches: list[Reach], longfit: LongFit,
                 join_trunk: int | None, cfg: dict) -> tuple[dict, dict]:
    """Assemble the MacroConfig Valley object (course-local coords, the
    EXTENDED centerline as Path::Points) + the measurements sidecar."""
    from .transects import arc_length, resample_polyline
    total = arc_length(cl_ext)
    ctrl = resample_polyline(cl_ext, cfg["ctrl_spacing_m"])
    oks = [(cf, rc) for cf, rc in zip(crossfits, reaches) if cf.ok]
    knots = [[round((rc.s_mid + ext_up_m) / total, 4), round(cf.hw, 2)]
             for cf, rc in oks]
    if not knots:
        knots = [[0.0, 8.0]]
    wl = weighted_median([cf.ml for cf, _ in oks], [rc.n_used for _, rc in oks])
    wr = weighted_median([cf.mr for cf, _ in oks], [rc.n_used for _, rc in oks])
    r_med = weighted_median([cf.r for cf, _ in oks], [rc.n_used for _, rc in oks])
    # floor_z0 at s=0 of the EXTENDED spine (upstream extension rises)
    floor_z0 = longfit.floor_z0 + longfit.fall_clamped * ext_up_m
    valley = {
        "path": {"Points": [{"x": round(float(p[0]), 2),
                             "y": round(float(p[1]), 2)} for p in ctrl]},
        "floor_z0_m": round(float(floor_z0), 2),
        "fall_gradient": round(float(longfit.fall_clamped), 6),
        "floor_halfwidth": {"knots": knots},
        "wall_grad_left": round(wl, 3) if np.isfinite(wl) else 0.2,
        "wall_grad_right": round(wr, 3) if np.isfinite(wr) else 0.2,
        "floor_round_m": round(r_med, 2) if np.isfinite(r_med) else 6.0,
        "shoulder_k_m": cfg["shoulder_k_m"],
        "join_trunk": join_trunk,
    }
    meas = {
        "reaches": [dict(asdict(cf), s_mid=rc.s_mid, n_used=rc.n_used)
                    for cf, rc in zip(crossfits, reaches)],
        "longitudinal": asdict(longfit),
        "incision_med": weighted_median([cf.inc for cf, _ in oks],
                                        [rc.n_used for _, rc in oks]),
        "ext_up_m": ext_up_m,
        "arc_total_m": total,
    }
    return valley, meas
