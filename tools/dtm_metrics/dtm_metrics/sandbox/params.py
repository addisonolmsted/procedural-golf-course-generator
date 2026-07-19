"""The broad noise-parameter battery (19 assessed dims + seed + nuisance).

Each row: (name, lo, hi, transform, hypothesized targets). The hypothesized
targets drive the emulator's HIT/SURPRISE intent report (v1 pattern). `dummy`
is inert by construction — the sensitivity noise floor / overfit tripwire.
Modulation params (floor_damp/slope_gain/grain_align) are inert on flat-base
campaigns and studied on the skeleton campaigns.
"""

from __future__ import annotations

import math

# name, lo, hi, transform, int, hypothesized targets
PARAMS = [
    ("base_amp",        2.0,   60.0, "log",    False, "s2_cw_bp_100_200 s2_res_rms_100 s2_sf_rms_100"),
    ("base_wavelength", 100.0, 800.0, "log",   False, "s2_vg_range_char band-ratios"),
    ("octaves",         3.0,    8.0, "linear", True,  "s2_beta_char s2_cw_bp_8_20"),
    ("gain",            0.35,  0.75, "linear", False, "s2_beta_char s2_res_slope_p90"),
    ("lacunarity",      1.8,    2.4, "linear", False, "s2_beta_char (merge-candidate vs gain)"),
    ("warp_amp",        0.0,  300.0, "linear", False, "s2_res_aniso_ratio s2_res_rough_moran_50"),
    ("warp_wavelength", 150.0, 1500.0, "log",  False, "warp feature scale"),
    ("warp2_amp",       0.0,  120.0, "linear", False, "s2_res_extrema_ha fine tortuosity"),
    ("ridged_mix",      -1.0,   1.0, "linear", False, "s2_res_skew s2_res_kurt s2_res_rough_top10_25"),
    ("redistribution",  0.3,    3.0, "log",    False, "s2_res_skew hyps-of-residual"),
    ("tex_amp",         0.05,   3.0, "log",    False, "s2_cw_bp_8_20 s2_res_rms_25 s2_sf_rms_10"),
    ("tex_wavelength",  8.0,   60.0, "log",    False, "s2_cw_bp_8_20/s2_cw_bp_20_50 ratio"),
    ("tex_gain",        0.4,   0.72, "linear", False, "fine beta s2_laplacian_p95"),
    ("nugget_amp",      0.0,    0.5, "linear", False, "s2_vg_nugget s2_cw_bp_4_8"),
    ("aniso_ratio",     1.0,    4.0, "log",    False, "s2_res_aniso_ratio"),
    ("floor_damp",      0.05,   1.0, "linear", False, "s3_floor_upland_log_rms_ratio"),
    ("slope_gain",      -0.5,   1.0, "linear", False, "s3_slope_texture_exponent"),
    ("grain_align",     0.0,    1.0, "linear", False, "s3_grain_alignment"),
    ("dummy",           0.0,    1.0, "linear", False, "(inert noise floor)"),
]

NAMES = [p[0] for p in PARAMS]
TARGETS = {p[0]: p[5] for p in PARAMS}
# flat-base campaigns keep the modulation trio inert at these values
FLAT_INERT = {"floor_damp": 1.0, "slope_gain": 0.0, "grain_align": 0.0}


def unit_to_value(name: str, u: float) -> float:
    _, lo, hi, tr, is_int, _ = next(p for p in PARAMS if p[0] == name)
    u = min(1.0, max(0.0, u))
    v = lo * (hi / lo) ** u if tr == "log" else lo + u * (hi - lo)
    return float(round(v)) if is_int else float(v)


def value_to_unit(name: str, v: float) -> float:
    _, lo, hi, tr, _is_int, _ = next(p for p in PARAMS if p[0] == name)
    if tr == "log":
        return (math.log(v) - math.log(lo)) / (math.log(hi) - math.log(lo))
    return (v - lo) / (hi - lo)


def unit_row_to_config(u_row, seed: int, active: list[str]) -> dict:
    """Unit-box design row -> NoiseLabConfig dict. Params not in `active` take
    their flat-inert value (modulation trio) or midpoint."""
    cfg = {"seed": int(seed)}
    ui = {n: u for n, u in zip(active, u_row)}
    for name in NAMES:
        if name in ui:
            v = unit_to_value(name, ui[name])
        elif name in FLAT_INERT:
            v = FLAT_INERT[name]
        else:
            v = unit_to_value(name, 0.5)
        if name == "octaves":
            cfg[name] = int(v)
        else:
            cfg[name] = v
    return cfg
