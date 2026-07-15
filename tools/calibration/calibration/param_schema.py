"""Canonical generator parameter vector theta, bound to the REAL generator.

RECONCILIATION (important — the task prompt was written without the current
project's context). The prompt imagines a ~40-parameter generator (landform
primitives, terracing/contrast, ridged fBm, iterative warp, an erosion-model
blend, routing-exponent, m/n stream-power exponents, planation, boundary
inflow, thermal/talus knobs). The ACTUAL `golf-terrain` generator exposes far
fewer knobs, and — crucially for a continuous p(theta) — has **no categorical
switches to reparameterize**: it is already continuous. The real config is:

  TerrainParams { amplitude, base_period, octaves, lacunarity, gain,
                  warp_amp, warp_period, redistribution }          (params.rs)
  ErosionParams { intensity, deposition, diffusion, hardness_contrast }  (erosion.rs)

`generate_course_with(spec, seed, &TerrainParams, &ErosionParams, &WaterParams)`
is the explicit-param entrypoint (invoked headlessly by `xtask forward-grid`).

So theta = these 12 knobs. Mapping of the prompt's wishlist to reality:

  prompt param            -> real knob / status
  base_frequency          -> base_period (period = tile/freq); we use base_period
  octaves                 -> octaves
  lacunarity              -> lacunarity   (sampler fixes 2.0; freed here)
  gain                    -> gain
  relief_amplitude        -> amplitude    (sampler SOLVES it from target relief;
                                           here it is a free knob)
  ridged_blend            -> ABSENT (no ridged fBm in the generator)
  warp_strength/frequency -> warp_amp / warp_period
  warp_octaves/recursion  -> ABSENT (single warp, no per-octave/iterative knob)
  redistribution_exponent -> redistribution
  terrace_*, contrast_gain-> ABSENT (no terracing / contrast remap)
  landform primitives *   -> ABSENT (no authored macro primitives)
  erodibility_contrast    -> hardness_contrast (a REAL capability the sampler
                                           disables at 0; freed here so calibration
                                           can discover whether it helps coverage)
  erodibility_scale/mix   -> ABSENT as params (hardness field scale is internal)
  erosion_model_blend     -> ABSENT (fixed stream-power + graded deposition)
  routing_exponent        -> ABSENT (fixed D8 priority-flood routing)
  K_strength              -> intensity (Theta = sum K*dt)
  m_area_exp / n_slope_exp-> FIXED (m=1/2, n=1); concavity is not a free knob
  deposition_rate         -> deposition
  planation_*             -> ABSENT as separate knobs (part of graded deposition)
  iterations              -> FIXED (8 routing epochs)
  discharge_ref           -> FIXED
  boundary_inflow         -> a WaterParams.river_area scenario var — OUT OF SCOPE
                             (morphology-only; water excluded, see run/config)
  diffusion_D             -> diffusion
  talus_angle             -> FIXED (35 deg); thermal_strength ABSENT

Seed is the nuisance/replication variable (>=3 seeds per theta).

Transforms: log for scales/frequencies/rates spanning orders of magnitude;
linear for bounded exponents/fractions. Ranges bind to the sampler's clamps
plus headroom so the design can probe past where the current sampler lives.
"""

from __future__ import annotations

import hashlib
import json
import math

# name, subsystem, low, high, transform, integer, drives (hypothesized metric family)
PARAMS = [
    # 1. base fBm ---------------------------------------------------------
    # amplitude hi 120->200 (2026-07-11): the v9 fit had ZERO weight above 65 m
    # relief because the design's relief ceiling (~90 m) sat AT the real max —
    # 200 puts the real tail mid-range. base_period lo 260->200: the fit was
    # starved of short-period design points (variogram_range residual +2.2z;
    # real range min 66 m).
    ("amplitude",       "fbm",     5.0,  200.0, "log",    False, "relief_p95_p5, elev_std"),
    ("base_period",     "fbm",   200.0, 2200.0, "log",    False, "variogram_range, valley_spacing, PSD peak"),
    ("octaves",         "fbm",     3.0,    7.0, "linear", True,  "spectral_slope_beta, highfreq_energy_frac"),
    ("lacunarity",      "fbm",     1.8,    2.3, "linear", False, "spectral_slope_beta"),
    ("gain",            "fbm",     0.35,   0.75, "linear", False, "spectral_slope_beta, roughness, sharpness"),
    # 2. domain warp ------------------------------------------------------
    ("warp_amp",        "warp",    0.0,  240.0, "linear", False, "anisotropy, directional variogram, sinuosity"),
    ("warp_period",     "warp",  300.0, 2200.0, "log",    False, "warp feature scale"),
    # 3. redistribution ---------------------------------------------------
    ("redistribution",  "redist",  0.25,  4.0,  "log",    False, "hypsometric_integral, slope_skew, frac_flat, elev_kurtosis"),
    # 4. NEW fine-detail band (the primary roughness lever) ---------------
    ("detail_amp",      "detail",  0.1,   4.0,  "log",    False, "spectral_slope_beta, highfreq_energy_frac, curvature, variogram_nugget"),
    ("detail_gain",     "detail",  0.40,  0.72, "linear", False, "spectral_slope_beta (detail spectral shape)"),
    ("detail_octaves",  "detail",  2.0,   5.0,  "linear", True,  "highfreq_energy_frac, sharpness (detail spectral reach)"),
    ("nugget_amp",      "detail",  0.0,   0.35, "linear", False, "variogram_nugget, laplacian on flats"),
    # 5. NEW directional anisotropy --------------------------------------
    ("aniso_ratio",     "aniso",   1.0,   4.0,  "log",    False, "anisotropy_ratio, directional variogram"),
    # 6. fluvial erosion --------------------------------------------------
    ("intensity",       "erosion", 0.3,   6.5,  "log",    False, "drainage_density, relief change, valley incision"),
    ("deposition",      "erosion", 0.1,   3.5,  "linear", False, "frac_flat, valley-floor width, hypsometry"),
    ("hardness_contrast", "erosion", 0.0, 0.8,  "linear", False, "slope tail, cliff frequency, curvature, elev_kurtosis (reactivated)"),
    # 7. hillslope --------------------------------------------------------
    ("diffusion",       "hillslope", 0.2, 3.0,  "linear", False, "spectral_slope_beta, sharpness, slope_p95"),
    # 8. landform organization (2026-07 "dialed in" pass; appended so decode
    # indices 0..16 stay stable). Spot-check @0.6: ridged −0.6 moved top10
    # 0.224→0.294 / het200 0.363→0.625 / kurt 0.0→6.1; relief_patch 0.7 moved
    # het200 →0.440. (detail_patchiness was cut: no measurable metric response
    # in rolling OR flat regimes — the organization gaps are macro-structural.)
    ("ridged_mix",        "fbm", -0.8,  0.8,  "linear", False, "rough_top10_30, rough_moran_60, elev_kurtosis, heterogeneity, chan_len(diag)"),
    ("relief_patchiness", "fbm",  0.0,  0.8,  "linear", False, "heterogeneity_index_200, elev_kurtosis, rough_block_ratio(diag)"),
]

NAMES = [p[0] for p in PARAMS]
DRIVES = {p[0]: p[6] for p in PARAMS}
SUBSYSTEM = {p[0]: p[1] for p in PARAMS}

# TerrainParams config fields (superset of the theta terrain knobs; aniso_angle
# is a per-course nuisance the metric is invariant to — fixed 0 in calibration).
TERRAIN_FIELDS = ["amplitude", "base_period", "octaves", "lacunarity", "gain",
                  "warp_amp", "warp_period", "redistribution",
                  "detail_amp", "detail_gain", "detail_octaves", "nugget_amp",
                  "aniso_ratio", "aniso_angle",
                  "ridged_mix", "relief_patchiness"]
# (detail_patchiness exists in the generator but is NOT a theta knob — no
# measurable metric response; serde defaults it to 0.)
EROSION_FIELDS = ["intensity", "deposition", "diffusion", "hardness_contrast"]

# fixed values for the generator fields NOT in theta (documented above)
FIXED = {}   # generate_course_with takes only these structs; nothing else needed


def _to_unit(name, v):
    _, _, lo, hi, tr, _, _ = _by_name(name)
    if tr == "log":
        return (math.log(v) - math.log(lo)) / (math.log(hi) - math.log(lo))
    return (v - lo) / (hi - lo)


def _from_unit(name, u):
    _, _, lo, hi, tr, integer, _ = _by_name(name)
    u = min(1.0, max(0.0, u))
    if tr == "log":
        v = lo * (hi / lo) ** u
    else:
        v = lo + u * (hi - lo)
    if integer:
        v = float(round(v))
    return v


_INDEX = {p[0]: p for p in PARAMS}


def _by_name(name):
    return _INDEX[name]


def unit_to_theta(u):
    """u: array-like in [0,1]^d (design space) -> theta dict in real units."""
    return {name: _from_unit(name, u[i]) for i, name in enumerate(NAMES)}


def theta_to_unit(theta):
    return [_to_unit(name, theta[name]) for name in NAMES]


_INT_FIELDS = {"octaves", "detail_octaves"}


def theta_to_config(theta):
    """theta dict -> generator config dict {terrain{...}, erosion{...}}.

    Integer knobs are rounded; `aniso_angle` is not a theta knob (nuisance) so
    it is fixed at 0 here. This is exactly what `xtask forward-grid`
    deserializes into TerrainParams / ErosionParams.
    """
    def val(f):
        if f == "aniso_angle":
            return 0.0
        return int(round(theta[f])) if f in _INT_FIELDS else float(theta[f])
    terrain = {f: val(f) for f in TERRAIN_FIELDS}
    erosion = {f: float(theta[f]) for f in EROSION_FIELDS}
    return {"terrain": terrain, "erosion": erosion}


def config_to_theta(cfg):
    """Inverse of theta_to_config (real generator config -> theta dict; skips
    the aniso_angle nuisance which is not a theta knob)."""
    t = dict(cfg["terrain"]); e = dict(cfg["erosion"])
    return {**{f: float(t[f]) for f in TERRAIN_FIELDS if f in NAMES},
            **{f: float(e[f]) for f in EROSION_FIELDS}}


def config_hash(cfg):
    return hashlib.sha1(json.dumps(cfg, sort_keys=True).encode()).hexdigest()[:12]


def bounds_unit():
    """(lows, highs) in real units, ordered by NAMES — for reporting."""
    return [p[2] for p in PARAMS], [p[3] for p in PARAMS]


def dump_json(path):
    schema = [{"name": p[0], "subsystem": p[1], "low": p[2], "high": p[3],
               "transform": p[4], "integer": p[5], "drives": p[6]} for p in PARAMS]
    json.dump({"params": schema,
               "terrain_fields": TERRAIN_FIELDS, "erosion_fields": EROSION_FIELDS},
              open(path, "w"), indent=1)
