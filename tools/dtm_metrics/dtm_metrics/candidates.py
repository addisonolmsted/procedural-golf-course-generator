"""The Stage-1 candidate metric battery.

Families (the primitive-awareness split — see stage1_metrics.md for the full
per-metric rationale):
  s1_*  macro form      — primitives' domain; validation/feasibility, never
                          noise-fit targets (double-counting guard).
  s2_*  character       — what the Stage-4 noise layer must reproduce; the
                          Stage-7b objective candidates.
  s3_*  modulation      — texture conditioned on landform position; measures
                          Stage 4's floor_damp/slope_gain/grain_align contract.
  ctx_* context         — never fit (water, canopy, masks, confidence).

S1 metrics are emitted twice: unsuffixed (all_valid weighting — inpainted
macro form is legitimate) and `__clean` (clean-cells-only; the G2 stability
comparison). S2/S3 are clean-only by construction.
"""

from __future__ import annotations

import numpy as np
from scipy import ndimage, stats

from metrics import core as v1core

from . import specwin, surfaces
from .weights import weighted_iqr, weighted_moments, weighted_percentile
from .store import Course

import json
import os

_PARKLAND_CACHE = os.path.normpath(os.path.join(
    os.path.dirname(os.path.abspath(__file__)),
    "..", "..", "parkland_atlas", "out", "cache"))


def _parkland_treepct(key: str) -> float:
    try:
        with open(os.path.join(_PARKLAND_CACHE, f"{key}.json")) as f:
            return float(json.load(f).get("treepct", float("nan")))
    except (OSError, ValueError):
        return float("nan")

FAMILY_TAGS = ("s1_", "s2_", "s3_", "ctx_")


def _hyps_quantiles(z: np.ndarray, w: np.ndarray, qs=(0.10, 0.25, 0.50, 0.75, 0.90)) -> dict:
    """Area fraction below q of (robust) relief — weighted."""
    lo, hi = weighted_percentile(z, w, [1.0, 99.0])
    span = max(hi - lo, 1e-6)
    zr = np.clip((z - lo) / span, 0.0, 1.0)
    sw = w.sum()
    out = {}
    for q in qs:
        out[f"s1_hyps_q{int(q * 100)}"] = float((w * (zr < q)).sum() / sw)
    return out


def _block_mean(a: np.ndarray, k: int) -> np.ndarray:
    ny, nx = a.shape[0] // k, a.shape[1] // k
    return a[: ny * k, : nx * k].reshape(ny, k, nx, k).mean(axis=(1, 3))


def _tilt_grade(z: np.ndarray, w: np.ndarray, valid: np.ndarray, cell: float) -> float:
    """Weighted planar fit over valid cells -> gradient magnitude."""
    ys, xs = np.nonzero(valid)
    if ys.size < 100:
        return float("nan")
    step = max(1, ys.size // 200_000)      # subsample huge grids
    ys, xs = ys[::step], xs[::step]
    ww = w[ys, xs]
    A = np.stack([np.ones(ys.size), xs * cell, ys * cell], 1)
    Aw = A * ww[:, None]
    coef, *_ = np.linalg.lstsq(Aw, z[ys, xs] * ww, rcond=None)
    return float(np.hypot(coef[1], coef[2]))


def _s1_macro(c: Course, s: surfaces.Surfaces, wmap: np.ndarray,
              sel: np.ndarray, cfg: dict, suffix: str = "") -> dict:
    """Macro family over the selection mask (all_valid or clean)."""
    S: dict = {}
    v = s.s1[sel]
    w = wmap[sel]
    p = weighted_percentile(v, w, [1.0, 5.0, 95.0, 99.0])
    S[f"s1_relief_p95_p5{suffix}"] = float(p[2] - p[1])
    S[f"s1_relief_p99_p1{suffix}"] = float(p[3] - p[0])
    ls = s.lp_slope[sel]
    S[f"s1_lp_slope_p50{suffix}"] = float(weighted_percentile(ls, w, [50.0])[0])
    S[f"s1_lp_slope_iqr{suffix}"] = weighted_iqr(ls, w)
    S[f"s1_lp_slope_p90{suffix}"] = float(weighted_percentile(ls, w, [90.0])[0])
    for k, val in _hyps_quantiles(v, w).items():
        S[f"{k}{suffix}"] = val
    return S


def compute_course(c: Course, cfg: dict) -> dict:
    scfg = cfg["scales"]
    bands = cfg["bands"]
    s = surfaces.build(c.naturalized, c.valid, c.cell,
                       cfg["surfaces"]["lowpass_L_m"],
                       cfg["surfaces"]["tpi_window_m"])
    wmap = surfaces.weight_map(c.valid, c.clean, c.exterior,
                               c.exterior_inpaint_frac,
                               cfg["weights"]["margin_bonus"])
    S: dict = {"key": c.key}

    # ---------------- S1 macro (all_valid + __clean variants) ----------------
    S.update(_s1_macro(c, s, wmap, c.valid, cfg))
    S.update(_s1_macro(c, s, wmap, c.clean, cfg, suffix="__clean"))

    # global macro band powers (quadratic detrend then Parseval annuli)
    resid, _tr = v1core.detrend(s.s1, c.cell, c.valid, "quadratic")
    P, f = specwin.window_periodogram(resid, c.cell, pre_detrended=True)
    macro_rms = np.sqrt(specwin.band_variances(P, f, bands["macro_m"]))
    for (lo, hi), v in zip(bands["macro_m"], macro_rms):
        name = f"s1_bp_{int(lo)}p" if np.isinf(hi) else f"s1_bp_{int(lo)}_{int(hi)}"
        S[name] = float(v)

    # coarse drainage density on 8 m block-means
    k = max(1, int(round(scfg["drainage_block_m"] / c.cell)))
    zb = _block_mean(s.s1, k)
    vb = _block_mean(c.valid.astype(np.float64), k) > 0.5
    hyd = v1core.hydrology_metrics(zb, c.cell * k, vb, scfg["drainage_thresh_m2"])
    S["s1_drainage_density_coarse"] = hyd["drainage_density"]

    tl = v1core.tpi_landforms(s.s1, c.cell, scfg["tpi_valley_radius_m"], c.valid)
    S["s1_valley_count_km2_200"] = tl["valley_count_km2"]
    S["s1_valley_elong_200"] = tl["valley_elong"]

    an = v1core.anisotropy(s.s1, c.cell, c.valid, "quadratic",
                           max_lag_m=scfg["aniso"]["macro_max_lag_m"],
                           n_dir=scfg["aniso"]["n_dir"])
    S["s1_aniso_ratio_macro"] = an["anisotropy_ratio"]
    S["s1_tilt_grade"] = _tilt_grade(s.s1, wmap, c.valid, c.cell)

    # ---------------- S2 character (clean-only) ------------------------------
    sw = cfg["specwin"]
    cw = specwin.clean_window_bands(
        s.s1, c.clean, wmap, c.cell,
        bands["char_m"] + bands["qa_m"], bands["beta_char_m"],
        sw["window_cascade_m"], sw["stride_frac"],
        sw["min_clean_frac"], sw["min_windows"])
    for (lo, hi), v in zip(bands["char_m"], cw["bands"][: len(bands["char_m"])]):
        S[f"s2_cw_bp_{int(lo)}_{int(hi)}"] = float(v)
    (qlo, qhi) = bands["qa_m"][0]
    S[f"s2_cw_bp_{int(qlo)}_{int(qhi)}"] = float(cw["bands"][len(bands["char_m"])])
    S["s2_beta_char"] = cw["beta"]
    S["ctx_n_windows"] = cw["n_windows"]
    S["ctx_window_m"] = cw["window_m"]

    # structure-function amplitudes at fixed lags (valid-pair, mask-unbiased)
    lags = np.asarray(scfg["sf_lags_m"], np.float64)
    g = v1core.variogram(s.s2, c.cell, lags, c.clean, "planar")
    for lag, gg in zip(scfg["sf_lags_m"], g):
        S[f"s2_sf_rms_{int(lag)}"] = float(np.sqrt(gg)) if np.isfinite(gg) else float("nan")

    for w_m in scfg["res_rms_windows_m"]:
        S[f"s2_res_rms_{int(w_m)}"] = v1core.rms_roughness(s.s2, c.cell, w_m, c.clean, "planar")

    ro = v1core.roughness_organization(
        s.s2, c.cell, c.clean, scfg["rough_org"]["window_m"],
        scfg["rough_org"]["moran_lag_m"], scfg["rough_org"]["block_m"], "planar")
    S["s2_res_rough_cv_25"] = ro["rough_cv"]
    S["s2_res_rough_top10_25"] = ro["rough_top10"]
    S["s2_res_rough_moran_50"] = ro["rough_moran"]

    S["s2_res_extrema_ha"] = v1core.extrema_density(
        s.s2, c.cell, c.clean, scfg["extrema_sigma_m"], "planar")

    gy, gx = np.gradient(s.s2, c.cell)
    rslope = np.hypot(gx, gy)[c.clean]
    rw = wmap[c.clean]
    S["s2_res_slope_p50"] = float(weighted_percentile(rslope, rw, [50.0])[0])
    S["s2_res_slope_p90"] = float(weighted_percentile(rslope, rw, [90.0])[0])

    cs = v1core.curvature_stats(s.s1, c.cell, c.clean)
    S["s2_prof_curv_mean"] = cs["mean_abs_profile_curv"]
    S["s2_prof_curv_p90"] = cs["profile_curv_p90"]
    S["s2_laplacian_p95"] = v1core.laplacian_stats(s.s1, c.cell, c.clean)["laplacian_abs_p95"]

    vgc = scfg["variogram"]
    vlags = np.linspace(vgc["min_lag_m"], vgc["max_lag_m"], vgc["n_lags"])
    gv = v1core.variogram(s.s2, c.cell, vlags, c.clean, "planar")
    vf = v1core.variogram_fit(vlags, gv, "exponential")
    S["s2_vg_nugget"] = vf["variogram_nugget"]
    S["s2_vg_range_char"] = vf["variogram_range"]

    an2 = v1core.anisotropy(s.s2, c.cell, c.clean, "planar",
                            max_lag_m=scfg["aniso"]["char_max_lag_m"],
                            n_dir=scfg["aniso"]["n_dir"])
    S["s2_res_aniso_ratio"] = an2["anisotropy_ratio"]

    mo = weighted_moments(s.s2[c.clean], rw)
    S["s2_res_skew"] = mo["skew"]
    S["s2_res_kurt"] = mo["kurtosis"]

    # ---------------- S3 modulation contract ---------------------------------
    S.update(modulation_metrics(s, c.clean, c.cell, cfg))

    # ---------------- context ------------------------------------------------
    S["ctx_water_fraction"] = float((c.water & c.valid).sum() / max(c.valid.sum(), 1))
    lab, nlab = ndimage.label(c.water, structure=np.ones((3, 3), np.int8))
    if nlab:
        areas = np.bincount(lab.ravel())[1:] * c.cell * c.cell
        areas = areas[areas > 0]
        S["ctx_water_body_count"] = int(len(areas))
        S["ctx_water_area_p50"] = float(np.percentile(areas, 50))
        S["ctx_water_area_p90"] = float(np.percentile(areas, 90))
    else:
        S["ctx_water_body_count"] = 0
        S["ctx_water_area_p50"] = float("nan")
        S["ctx_water_area_p90"] = float("nan")
    for cls, st in c.meta.get("masks", {}).items():
        S[f"ctx_mask_{cls}_frac"] = st.get("frac", float("nan"))
    S["ctx_inpaint_frac"] = float(c.meta.get("inpaint", {}).get("frac", float("nan")))
    S["ctx_exterior_inpaint_frac"] = c.exterior_inpaint_frac
    S["ctx_clean_frac"] = c.clean_frac
    S["ctx_canopy_pct"] = _parkland_treepct(c.key)
    S["ctx_grid_w"] = int(c.meta["grid"]["w"])
    S["ctx_grid_h"] = int(c.meta["grid"]["h"])
    S["ctx_low_confidence"] = bool(
        c.clean_frac < cfg["flags"]["low_confidence_clean_frac"]
        or cw["n_windows"] < sw["min_windows"])
    return S


def modulation_metrics(s: surfaces.Surfaces, clean: np.ndarray, cell: float,
                       cfg: dict, prefix: str = "s3_") -> dict:
    """The modulation contract, computable against ANY macro predictor set
    (lowpass proxy here; true skeleton fields in the sandbox validation)."""
    scfg = cfg["scales"]
    S: dict = {}
    R, _valid = v1core.local_std_field(s.s2, cell, scfg["rough_org"]["window_m"],
                                       clean, "planar")
    good = np.isfinite(R) & clean

    # floor vs upland texture ratio (log), TPI classes at +-0.5 sigma
    thr = 0.5 * s.tpi_sigma
    floor = good & (s.tpi < -thr)
    upland = good & (s.tpi > thr)
    if floor.sum() >= 500 and upland.sum() >= 500:
        S[f"{prefix}floor_upland_log_rms_ratio"] = float(
            np.log(R[floor].mean() + 1e-9) - np.log(R[upland].mean() + 1e-9))
    else:
        S[f"{prefix}floor_upland_log_rms_ratio"] = float("nan")

    # texture-vs-macro-slope exponent over slope deciles
    ls = s.lp_slope[good]
    rr = R[good]
    if ls.size > 2000:
        qs = np.quantile(ls, np.linspace(0.05, 0.95, 10))
        xs_, ys_ = [], []
        for i in range(len(qs) - 1):
            m = (ls >= qs[i]) & (ls < qs[i + 1])
            if m.sum() >= 200:
                xs_.append(np.log(1.0 + ls[m].mean()))
                ys_.append(np.log(rr[m].mean() + 1e-9))
        if len(xs_) >= 5:
            coef = np.polyfit(xs_, ys_, 1)
            S[f"{prefix}slope_texture_exponent"] = float(coef[0])
        else:
            S[f"{prefix}slope_texture_exponent"] = float("nan")
    else:
        S[f"{prefix}slope_texture_exponent"] = float("nan")

    # grain alignment: directional semivariance across vs along macro aspect
    S[f"{prefix}grain_alignment"] = _grain_alignment(
        s, clean, cell, scfg["grain_lag_m"])

    # relief-position correlation (subsampled Spearman)
    sub = good.copy()
    sub[::2, :] = False
    sub[:, ::2] = False
    if sub.sum() > 2000:
        rho = stats.spearmanr(R[sub], s.relief_pos[sub]).statistic
        S[f"{prefix}relief_pos_texture_corr"] = float(rho)
    else:
        S[f"{prefix}relief_pos_texture_corr"] = float("nan")
    return S


def _grain_alignment(s: surfaces.Surfaces, clean: np.ndarray, cell: float,
                     lag_m: float, win_m: float = 512.0,
                     min_slope: float = 0.01) -> float:
    """Window-mean log ratio of S2 semivariance ACROSS vs ALONG the local macro
    aspect at ``lag_m``. > 0 = texture grain follows the macro slope direction
    (across-slope differences larger). Windows need a defined aspect (mean
    macro slope >= min_slope)."""
    lag = max(1, int(round(lag_m / cell)))
    win = int(round(win_m / cell))
    if win > min(s.s2.shape) or win < 32:
        win = max(32, min(s.s2.shape) // 3)
    vals, ws = [], []
    x = np.where(clean, s.s2, np.nan)
    for y0, x0 in specwin.iter_windows(s.s2.shape, win, max(1, win // 2)):
        cw = clean[y0:y0 + win, x0:x0 + win]
        if cw.mean() < 0.5:
            continue
        mslope = s.lp_slope[y0:y0 + win, x0:x0 + win].mean()
        if mslope < min_slope:
            continue
        a = s.lp_aspect[y0:y0 + win, x0:x0 + win]
        ux, uy = np.cos(a).mean(), np.sin(a).mean()
        norm = np.hypot(ux, uy)
        if norm < 0.3:      # incoherent aspect in the window
            continue
        ux, uy = ux / norm, uy / norm
        dx_a, dy_a = int(round(ux * lag)), int(round(uy * lag))       # along
        dx_c, dy_c = int(round(-uy * lag)), int(round(ux * lag))      # across
        wx = x[y0:y0 + win, x0:x0 + win]

        def semiv(dy: int, dx: int) -> float:
            if dy == 0 and dx == 0:
                return float("nan")
            aa, bb = v1core._shift_pair(wx, dy, dx)
            d = aa - bb
            d = d[np.isfinite(d)]
            return 0.5 * float(np.mean(d * d)) if d.size > 200 else float("nan")

        va = semiv(dy_a, dx_a)
        vc = semiv(dy_c, dx_c)
        if np.isfinite(va) and np.isfinite(vc) and va > 0:
            vals.append(np.log(vc / va))
            ws.append(mslope)
    if not vals:
        return float("nan")
    vals = np.asarray(vals)
    ws = np.asarray(ws)
    return float((vals * ws).sum() / ws.sum())
