"""Assemble the full feature set for one tile.

`compute()` is the single place that turns a (height, cell_size, valid[, tree,
water]) tile into (scalar dict, distribution dict). BOTH the real-course loader
(`run.py`) and the generator adapter (`synthetic.py`) call it, so real and
synthetic terrain are scored by identical code on identical shared axes.
"""

from __future__ import annotations

import numpy as np

from . import core


def build_axes(cfg):
    """Shared axes for the distribution vectors (identical across all tiles)."""
    s = cfg["slope"]
    edges = np.arange(s["hist_min_deg"], s["hist_max_deg"] + 1e-9, s["hist_step_deg"])
    sp = cfg["spectrum"]
    psd_wl = np.logspace(np.log10(sp["psd_wavelength_m"][0]),
                         np.log10(sp["psd_wavelength_m"][1]), sp["psd_n_axis"])
    vg = cfg["variogram"]
    lags = np.linspace(0.0, vg["max_lag_m"], vg["n_lags"])
    fr = np.linspace(0.0, 1.0, cfg["hypsometric"]["n_curve"])
    return {"slope_hist_edges_deg": edges,
            "psd_wavelength_m": psd_wl,
            "variogram_lag_m": lags,
            "hypsometric_fraction": fr}


def compute(height, cell_size, valid, cfg, axes, tree=None, water=None):
    """Return (scalars: dict, dists: dict[str, np.ndarray])."""
    dm = cfg["detrend"]["method"]
    S = {}
    D = {}

    # A. elevation & relief (raw) --------------------------------------------
    S["relief_p95_p5"] = core.relief_p95_p5(height, cell_size, valid)
    S.update(core.elev_moments(height, cell_size, valid))
    S["hypsometric_integral"] = core.hypsometric_integral(height, cell_size, valid)
    D["hypsometric_curve"] = core.hypsometric_curve(
        height, cell_size, axes["hypsometric_fraction"], valid)

    # B. slope (raw) ---------------------------------------------------------
    S.update(core.slope_stats(height, cell_size, valid,
                              cfg["slope"]["flat_deg"], cfg["slope"]["steep_deg"]))
    D["slope_hist"] = core.slope_hist(height, cell_size, axes["slope_hist_edges_deg"], valid)

    # C. spectrum & roughness (detrended) ------------------------------------
    wl, psd, kc = core.radial_psd(height, cell_size, valid, dm, cfg["spectrum"]["psd_n_axis"])
    D["radial_psd"] = core.resample_psd(wl, psd, kc, axes["psd_wavelength_m"])
    S["spectral_slope_beta"] = core.spectral_slope_beta(
        kc, psd, cfg["spectrum"]["midband_wavelength_m"])
    S["hurst_from_beta"] = (S["spectral_slope_beta"] - 2.0) / 2.0 \
        if np.isfinite(S["spectral_slope_beta"]) else float("nan")
    S["highfreq_energy_frac"] = core.highfreq_energy_frac(
        kc, psd, cfg["spectrum"]["highfreq_cutoff_m"], cfg["spectrum"]["reliable_wavelength_min_m"])

    g = core.variogram(height, cell_size, axes["variogram_lag_m"], valid, dm)
    D["variogram"] = g
    S.update(core.variogram_fit(axes["variogram_lag_m"], g, cfg["variogram"]["model"]))
    rw = cfg["roughness_windows_m"]
    S["rms_roughness_small"] = core.rms_roughness(height, cell_size, rw["small"], valid, dm)
    S["rms_roughness_mid"] = core.rms_roughness(height, cell_size, rw["mid"], valid, dm)
    S["rms_roughness_large"] = core.rms_roughness(height, cell_size, rw["large"], valid, dm)

    # D. curvature & sharpness -----------------------------------------------
    S.update(core.curvature_stats(height, cell_size, valid))
    S.update(core.laplacian_stats(height, cell_size, valid))

    # E. spatial organization ------------------------------------------------
    for w in cfg["local_relief_windows_m"]:
        lr = core.local_relief(height, cell_size, w, valid)
        tag = int(round(w))
        S[f"local_relief_mean_{tag}"] = lr["local_relief_mean"]
        S[f"heterogeneity_index_{tag}"] = lr["heterogeneity_index"]
    S["slope_autocorr_length"] = core.slope_autocorr_length(height, cell_size, valid)

    # E2. landform organization (where the roughness lives, not how much) -----
    lf = cfg["landform"]
    ro = core.roughness_organization(
        height, cell_size, valid, lf["rough_window_m"], lf["moran_lag_m"],
        lf["block_m"], dm)
    rtag = int(round(lf["rough_window_m"]))
    S[f"rough_cv_{rtag}"] = ro["rough_cv"]
    S[f"rough_top10_{rtag}"] = ro["rough_top10"]
    S[f"rough_moran_{int(round(lf['moran_lag_m']))}"] = ro["rough_moran"]
    S[f"rough_block_ratio_{int(round(lf['block_m']))}"] = ro["rough_block_ratio"]
    S["extrema_per_ha"] = core.extrema_density(
        height, cell_size, valid, lf["extrema_sigma_m"], dm)
    r_count, r_shape = lf["tpi_radius_m"]
    tl_c = core.tpi_landforms(height, cell_size, r_count, valid,
                              lf["tpi_k"], lf["tpi_min_area_m2"])
    tl_s = core.tpi_landforms(height, cell_size, r_shape, valid,
                              lf["tpi_k"], lf["tpi_min_area_m2"])
    S[f"valley_count_km2_{int(round(r_count))}"] = tl_c["valley_count_km2"]
    S[f"valley_elong_{int(round(r_shape))}"] = tl_s["valley_elong"]
    S[f"valley_len_p90_{int(round(r_shape))}"] = tl_s["valley_len_p90"]

    # F. hydrology (raw; priority-flood + D8) --------------------------------
    S.update(core.hydrology_metrics(height, cell_size, valid,
                                    cfg["hydrology"]["accum_area_threshold_m2"]))

    # G. anisotropy ----------------------------------------------------------
    S.update(core.anisotropy(height, cell_size, valid, dm,
                             max_lag_m=min(cfg["variogram"]["max_lag_m"], 400.0)))

    # H. land cover (full survey extent — clip-invariant, matches atlas %) ----
    in_bounds = np.ones(height.shape, bool)
    S["water_fraction"] = core.fraction(water, in_bounds) if water is not None else float("nan")
    S["canopy_fraction"] = core.fraction(tree, in_bounds) if tree is not None else float("nan")
    # secondary: canopy within the clip (playing ground only)
    S["canopy_fraction_clip"] = core.fraction(tree, valid) if tree is not None else float("nan")

    return S, D


# The ordered scalar feature columns (stable schema for the parquet table).
SCALAR_COLUMNS = [
    "relief_p95_p5", "elev_std", "elev_skew", "elev_kurtosis", "hypsometric_integral",
    "slope_mean", "slope_median", "slope_p90", "slope_p95", "slope_skew",
    "frac_flat", "frac_steep",
    "spectral_slope_beta", "hurst_from_beta", "highfreq_energy_frac",
    "variogram_nugget", "variogram_sill", "variogram_range",
    "rms_roughness_small", "rms_roughness_mid", "rms_roughness_large",
    "mean_abs_profile_curv", "profile_curv_p90", "mean_abs_plan_curv", "plan_curv_p90",
    "laplacian_abs_mean", "laplacian_abs_p95",
    "local_relief_mean_100", "heterogeneity_index_100",
    "local_relief_mean_200", "heterogeneity_index_200",
    "slope_autocorr_length",
    "rough_cv_30", "rough_top10_30", "rough_moran_60", "rough_block_ratio_350",
    "extrema_per_ha",
    "valley_count_km2_60", "valley_elong_120", "valley_len_p90_120",
    "drainage_density", "valley_spacing", "concavity_theta", "chan_len_p90",
    "anisotropy_ratio", "anisotropy_orientation_deg",
    "canopy_fraction", "water_fraction", "canopy_fraction_clip",
]

DIST_KEYS = ["slope_hist", "radial_psd", "variogram", "hypsometric_curve"]
