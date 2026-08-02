"""Per-tile landform.* knob measurements + QA overlay artifacts.

Placement-tier only (counts, sizes, gradients, spacing) — measured with the
ported analysis stack (tools/metrics core + dtm_primitives geomorphons) so
the same code that validates generated terrain measures the real exemplars.
Shape-tier deep fits (meander intensity, wall asymmetry, floor rounding)
are null here; the `shape` subcommand runs the full tools/dtm_primitives
transect pipeline for them, and fit_knobs keeps provisional tables for any
knob with too few samples.

v2 additionally persists what the detectors SAW, for tile-lab QA:
  <id>.classes.cgrid — u8 bit-flag raster @2 m (CLASS_* bits below)
  <id>.regions.json  — vector features in world metres, y-up
Both are versioned via `extract_version`; `run(force=True)` re-extracts.
All JSON is NaN-free (null instead) so Rust serde can read it.
"""

import json
import math
import pathlib
import sys

import numpy as np
import yaml
from scipy import ndimage

_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_ROOT / "metrics"))
sys.path.insert(0, str(_ROOT / "dtm_primitives"))

from metrics import core as mcore  # noqa: E402
from dtm_primitives import geomorphons, ridgepipe  # noqa: E402

from . import cgrid, develop, flow, structure  # noqa: E402

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"
PRIMITIVES_CFG = _ROOT / "dtm_primitives" / "dtm_primitives" / "config.yaml"
CHANNEL_AREA_M2 = 6.0e4
MIN_VALID = 0.97

EXTRACT_VERSION = 5

# Accepted-basin gate. Depth was 1.5 m (matching dtm_primitives bowlfit);
# QA review judged that too harsh — real kettles and blowouts that read
# clearly on the hillshade were being rejected. Diameter stays at the
# Bowl-primitive scale so interdune dimples still don't count.
BASIN_MIN_DEPTH_M = 1.0
BASIN_MIN_DIAG_M = 80.0

# --- dune-wavelength gate (v3) ---------------------------------------------
# Two failures compound in v2's `argmax(psd/wl²)` over `mcore.radial_psd`:
#   1. that PSD bins linearly in WAVENUMBER, so the whole 100-600 m dune
#      band lands in just two bins (384 m and 128 m) — nothing to find a
#      peak in, which is why every archetype returned exactly 384 m;
#   2. a red-noise landscape's power rises monotonically toward long
#      wavelengths, so the argmax is the band edge regardless of dunes.
# v3 measures a purpose-built DIRECTIONAL spectrum instead: log-spaced
# wavelength bins along the single strongest direction, and the wavelength
# is only reported when a peak stands above the fitted background there.
DUNE_BAND_M = (100.0, 600.0)
DUNE_FIT_BAND_M = (60.0, 900.0)  # background power-law fit range
DUNE_BINS = 28  # log-spaced over DUNE_FIT_BAND_M
DUNE_SECTOR_DEG = 30.0  # directional wedge half-width
DUNE_MIN_EXCESS = 1.6  # peak power / fitted background at that wavelength
DUNE_MIN_DIRECTIONALITY = 1.25  # best sector power / all-direction mean

# --- major-ridge gate (v3) -------------------------------------------------
# `ridge_count`/`ridge_len_m` mean MAJOR placed primitives (the planner emits
# a handful), not every geomorphon spur — v2 counted ~25/tile even in
# Florida. dtm_primitives' ridgepipe traces real crests with measured
# prominence; keep the ones that would be worth placing.
# "Major" is defined from the domain, not tuned to match a prior: a ridge
# worth PLACING is one a hole could be routed along. Relaxed 2026-07-31 —
# the first cut ("carries a hole or two", >=600 m) was stricter than the
# game needs: a 400 m ridge carries a single hole, and 7% of tile relief is
# still a landform you would notice standing on it.
RIDGE_MIN_PROMINENCE_M = 4.0
RIDGE_PROMINENCE_REL = 0.07
RIDGE_MIN_LEN_M = 400.0

_CFG_CACHE = None


# Campaign-side sensitivity overrides on the shared dtm_primitives config.
# Applied here rather than edited into config.yaml because that file is the
# atlas pipeline's tuning too — this is what THIS corpus needs, not a
# retune of the shared stack.
#
# Both families were under-detecting for the same underlying reason: the
# defaults are tuned to find a few big features per COURSE, while a 3 km
# landscape tile holds many. Measured on the pilot corpus before the change:
# ridgepipe traced only 0.1-4.2 crests/tile (so the acceptance gate was
# irrelevant — the network never offered the ridges), and blufffit's
# candidates had a median length of 236 m.
CFG_OVERRIDES = {
    # Terrace risers are gentler and more broken-up than the coastal and
    # river bluffs the defaults target.
    "bluff": {
        "slope_thresh": 0.18,           # was 0.25 (~14deg) — catch gentle risers
        "min_area_m2": 2000.0,          # was 3000
        "elong_min": 2.0,               # was 2.5 — terraces are less linear
        "min_len_m": 90.0,              # was 120
    },
}

# Ridge tracing runs the whole valley pipeline on the INVERTED surface, and
# it needs different transect geometry from real valleys — which is why
# these are scoped to the ridge call instead of the shared config.
#
# Measured cause: `ridgepipe` was returning 0-4 crests/tile, and the funnel
# showed branches arriving with ZERO usable reaches (five of seven on one
# mountain tile), so there were no cross-sections to fit and the branch was
# dropped. Transects default to a 300 m half-length, tuned for a valley
# with a floodplain; interfluves sit only 200-600 m apart, so a 300 m
# ridge transect runs across the neighbouring corridor and gets discarded.
# Shortening it to 120 m (with proportionally shorter reaches) took the
# same three tiles from 4 ridges to 11, and the two that produced NONE now
# produce 2 and 4.
#
# Valleys keep the defaults: a floodplain genuinely is that wide, and the
# looser channel threshold below would turn every gully into a "creek",
# polluting the valley cross-section statistics.
RIDGE_CFG_OVERRIDES = {
    "network": {
        "accum_min_area_m2": 20000.0,   # was 60000 — admit shorter divides
        "trib_min_area_m2": 20000.0,
        "trib_min_len_m": 120.0,        # was 150
        "max_depth": 3,                 # was 2
        "max_tributaries": 14,          # was 6 — the hard cap on branch count
    },
    "transects": {
        "halflen_m": 120.0,             # was 300 — stay on the ridge's flanks
        "halflen_max_m": 200.0,         # was 600
        "reach_len_m": 200.0,           # was 300
        "min_transects_per_reach": 4,   # was 6
        "spacing_m": 20.0,              # was 25 — keep the count per reach up
    },
    "ridgefit": {
        "min_len_m": 150.0,             # was 200
        "confirm_frac": 0.5,            # was 0.6 geomorphon agreement
    },
}

# The shape pass traces the drainage network for statistics, and the
# defaults truncate it hard: `max_depth: 2` with `max_tributaries: 6` means
# a 3 km tile can never report more than a depth-1 star plus one level, so
# the low-order tributaries that carry the space-filling signal are invisible
# to measurement. Raised for the campaign only.
NETWORK_CFG_OVERRIDES = {
    "network": {
        "max_depth": 5,             # was 2
        "max_tributaries": 40,      # was 6
        "trib_min_area_m2": 20000.0,
        "trib_min_len_m": 120.0,
    },
    # Same geometry problem the ridge pass had, for the same reason: with
    # the caps raised the network resolves ~34 branches at ~170 m junction
    # spacing, and a 300 m transect half-length (tuned for a trunk with a
    # floodplain) then reaches across the neighbouring corridor and is
    # discarded. Measured: 34 branches yielded 29 cross-sections of which 3
    # were usable. A first-order reach genuinely has a narrow cross-section.
    "transects": {
        "halflen_m": 120.0,
        "halflen_max_m": 250.0,
        "reach_len_m": 200.0,
        "min_transects_per_reach": 4,
        "spacing_m": 20.0,
    },
}

_RIDGE_CFG_CACHE = None
_NETWORK_CFG_CACHE = None


def _deep_merge(base: dict, over: dict) -> dict:
    out = dict(base)
    for k, v in over.items():
        out[k] = _deep_merge(base[k], v) if isinstance(v, dict) and k in base else v
    return out


def _primitives_cfg():
    """dtm_primitives config with the campaign's shared overrides (valleys,
    bluffs)."""
    global _CFG_CACHE
    if _CFG_CACHE is None:
        raw = yaml.safe_load(PRIMITIVES_CFG.read_text())
        _CFG_CACHE = _deep_merge(raw, CFG_OVERRIDES)
    return _CFG_CACHE


def _network_cfg():
    """Config for the network-statistics pass — see NETWORK_CFG_OVERRIDES."""
    global _NETWORK_CFG_CACHE
    if _NETWORK_CFG_CACHE is None:
        _NETWORK_CFG_CACHE = _deep_merge(_primitives_cfg(), NETWORK_CFG_OVERRIDES)
    return _NETWORK_CFG_CACHE


def _ridge_cfg():
    """Config for the ridge pass only — see RIDGE_CFG_OVERRIDES."""
    global _RIDGE_CFG_CACHE
    if _RIDGE_CFG_CACHE is None:
        _RIDGE_CFG_CACHE = _deep_merge(_primitives_cfg(), RIDGE_CFG_OVERRIDES)
    return _RIDGE_CFG_CACHE


def primitives_frame(z: np.ndarray) -> tuple[np.ndarray, np.ndarray]:
    """Adapt a campaign tile to the dtm_primitives array convention.

    **Every call into dtm_primitives must go through this.** Campaign tiles
    are row 0 = SOUTH (`fetch.py` flips the north-up 3DEP export so the CGRID
    row order matches the pipeline's y-up world). dtm_primitives came from
    the atlas GeoTIFF store and hard-codes row 0 = NORTH in
    `frame.rc_to_local` (`y = (nrows-1-row+0.5)*cell`), which it uses for
    every world coordinate it emits.

    Handing it a flipped array makes that formula collapse to
    `y = (row_south + 0.5)*cell` — our own convention — so returned
    centerlines are correct VERBATIM, with no post-transform, and handedness
    (`flank_l/r`, `raise_left`) is world-consistent too. Sampling round-trips
    through the same inverse, so scalar fits are unaffected either way.

    Without this the exported geometry is mirrored `y -> H - y`: measured on
    the pilot corpus, traced ridge centerlines fell on the geomorphon ridge
    mask 39% of the time (chance, given 35% coverage) unflipped versus 90%
    flipped.
    """
    return np.flipud(z), np.flipud(np.isfinite(z))


def _dune_spectrum(z: np.ndarray, cell: float):
    """Directional spectrum over the dune band.

    Returns `(wavelengths, power_along_best_direction, directionality,
    orientation_deg)`. `orientation_deg` is the direction the waves TRAVEL
    (perpendicular to the crest lines); `directionality` is that sector's
    band power over the all-direction mean — 1 for an isotropic field.
    """
    resid, _ = mcore.detrend(z, cell, None, "quadratic")
    ny, nx = resid.shape
    win = np.hanning(ny)[:, None] * np.hanning(nx)[None, :]
    f = np.fft.fftshift(np.fft.fft2(resid * win))
    power = (np.abs(f) ** 2) / (win**2).sum()
    ky = np.fft.fftshift(np.fft.fftfreq(ny, d=cell))[:, None]
    kx = np.fft.fftshift(np.fft.fftfreq(nx, d=cell))[None, :]
    kr = np.hypot(kx, ky)
    with np.errstate(divide="ignore"):
        lam = np.where(kr > 0, 1.0 / np.maximum(kr, 1e-12), np.inf)
    # Undirected angle of the wave vector, [0, 180)
    ang = np.degrees(np.arctan2(ky + 0.0 * kx, kx + 0.0 * ky)) % 180.0

    inband = (lam >= DUNE_BAND_M[0]) & (lam <= DUNE_BAND_M[1])
    if inband.sum() < 20:
        return None
    # Strongest direction: total in-band power per angular wedge.
    centers = np.arange(0.0, 180.0, 5.0)
    sector_power = []
    for c in centers:
        d = np.abs((ang - c + 90.0) % 180.0 - 90.0)
        sel = inband & (d <= DUNE_SECTOR_DEG)
        sector_power.append(power[sel].mean() if sel.any() else 0.0)
    sector_power = np.asarray(sector_power)
    best_i = int(np.argmax(sector_power))
    mean_p = sector_power.mean()
    directionality = float(sector_power[best_i] / mean_p) if mean_p > 0 else 0.0
    orient = float(centers[best_i])

    # Power vs wavelength along that direction, log-spaced (the resolution
    # mcore.radial_psd cannot provide at these wavelengths).
    d = np.abs((ang - orient + 90.0) % 180.0 - 90.0)
    wedge = d <= DUNE_SECTOR_DEG
    edges = np.geomspace(DUNE_FIT_BAND_M[0], DUNE_FIT_BAND_M[1], DUNE_BINS + 1)
    wls, pw = [], []
    for lo, hi in zip(edges[:-1], edges[1:]):
        sel = wedge & (lam >= lo) & (lam < hi)
        if sel.sum() >= 4:
            wls.append(np.sqrt(lo * hi))
            pw.append(power[sel].mean())
    if len(wls) < 8:
        return None
    return np.asarray(wls), np.asarray(pw), directionality, orient


def _dune_probe(z: np.ndarray, cell: float, x: dict) -> None:
    """Record the directional-periodicity probe into `extras`.

    NOT a knob measurement — see `dune_wavelength_m` in fit_knobs.QUARANTINED.
    The probe fixes v2's instrumentation (log-spaced bins along the strongest
    direction, peak measured against a fitted background) but a falsification
    test on the pilot corpus showed it does not isolate DUNES: orientation
    coherence across the 5 Nebraska Sandhills tiles (R=0.41) came out LOWER
    than across the 6 Cumberland Plateau tiles (R=0.75). A real dune field
    shares one regional wind orientation, so a dune-sensitive statistic would
    rank sandhills highest. This one ranks structural grain highest instead,
    i.e. it measures generic landscape directionality — piedmont valley
    spacing scores like a dune train. Kept as extras so the numbers
    accumulate for a future, properly-validated estimator.
    """
    spec = _dune_spectrum(z, cell)
    if spec is None:
        return
    wl, pw, directionality, orient = spec
    x["dune_directionality"] = directionality
    x["dune_orientation_deg"] = orient
    if directionality < DUNE_MIN_DIRECTIONALITY:
        return
    b, a = np.polyfit(np.log(wl), np.log(pw), 1)
    excess = pw / np.exp(a + b * np.log(wl))
    band = np.nonzero((wl >= DUNE_BAND_M[0]) & (wl <= DUNE_BAND_M[1]))[0]
    band = band[(band > 0) & (band < len(wl) - 1)]
    if band.size == 0:
        return
    best = band[np.argmax(excess[band])]
    x["dune_excess"] = float(excess[best])
    if excess[best] < DUNE_MIN_EXCESS:
        return
    if excess[best] >= excess[best - 1] and excess[best] >= excess[best + 1]:
        x["dune_peak_wavelength_m"] = float(wl[best])

# classes.cgrid bit flags (OR-able)
CLASS_NODATA = 1  # pre-fill invalid cells
CLASS_CHANNEL = 2  # flow accumulation >= CHANNEL_AREA_M2
CLASS_RIDGE_RAW = 4  # geomorphon ridge mask (8 m, block-replicated to 2 m)
CLASS_BASIN = 8  # accepted closed basin (>=1.5 m deep, >=80 m across)
CLASS_BASIN_REJ = 16  # basin candidate failing the depth gate
CLASS_FILL_FLAT = 32  # depression-fill raised the surface here (lakes/pits)
CLASS_RIDGE_ACCEPTED = 64  # major-ridge footprint (estimator repair, M3)
CLASS_DEVELOPED = 128  # OSM roads/buildings/quarries (see develop.py)


def _nan_to_none(v):
    """Recursively replace non-finite floats with None (JSON null)."""
    if isinstance(v, dict):
        return {k: _nan_to_none(x) for k, x in v.items()}
    if isinstance(v, (list, tuple)):
        return [_nan_to_none(x) for x in v]
    if isinstance(v, float) and not math.isfinite(v):
        return None
    return v


def _dump_json(path: pathlib.Path, obj):
    path.write_text(
        json.dumps(_nan_to_none(obj), indent=1, sort_keys=True, allow_nan=False)
    )


def _plane_grade(z: np.ndarray, cell: float, valid: np.ndarray):
    ys, xs = np.nonzero(valid[::8, ::8])
    zz = z[::8, ::8][valid[::8, ::8]]
    a = np.column_stack([xs * cell * 8, ys * cell * 8, np.ones_like(xs, dtype=float)])
    sol, *_ = np.linalg.lstsq(a, zz, rcond=None)
    gx, gy = sol[0], sol[1]
    return float(np.hypot(gx, gy)), (float(gx), float(gy))


def _components(mask: np.ndarray, cell: float, min_diag_m: float = 0.0):
    lab, n = ndimage.label(mask, structure=np.ones((3, 3)))
    comps = []
    for sl in ndimage.find_objects(lab):
        if sl is None:
            continue
        h = (sl[0].stop - sl[0].start) * cell
        w = (sl[1].stop - sl[1].start) * cell
        diag = float(np.hypot(h, w))
        if diag >= min_diag_m:
            comps.append((sl, diag))
    return lab, comps


# A crest halfwidth can fit hundreds of metres wide on a broad dune or
# hummock; stamping that as a QA corridor swamps the render, so cap the
# drawn width. The measured value still rides in regions.json untouched.
STAMP_MAX_HALFWIDTH_M = 60.0


def _stamp_polyline(classes: np.ndarray, pts: np.ndarray, cell: float, halfwidth_m: float, bit: int):
    """OR `bit` into cells within `halfwidth_m` of a world-metre polyline.

    Stamps a DISC per sample, not a square: square stamps leave axis-aligned
    rectangles wherever the corridor is wide, which read as detector
    artifacts in the QA viewer rather than as a corridor.
    """
    if pts.size == 0:
        return
    ny, nx = classes.shape
    hw = min(float(halfwidth_m), STAMP_MAX_HALFWIDTH_M)
    rad = max(int(round(hw / cell)), 1)
    oy, ox = np.ogrid[-rad : rad + 1, -rad : rad + 1]
    disc = (oy * oy + ox * ox) <= rad * rad
    step = max(cell * 0.5, 1.0)
    for a, b in zip(pts[:-1], pts[1:]):
        seg = np.hypot(b[0] - a[0], b[1] - a[1])
        for t in np.linspace(0.0, 1.0, max(int(seg / step), 2)):
            wx, wy = a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t
            cx, cy = int(wx / cell), int(wy / cell)
            y0, y1 = max(cy - rad, 0), min(cy + rad + 1, ny)
            x0, x1 = max(cx - rad, 0), min(cx + rad + 1, nx)
            if y0 < y1 and x0 < x1:
                sub = disc[y0 - (cy - rad) : y1 - (cy - rad), x0 - (cx - rad) : x1 - (cx - rad)]
                classes[y0:y1, x0:x1] |= np.where(sub, bit, 0).astype(np.uint8)


def _bbox_m(sl, cell: float) -> list[float]:
    """[x0, y0, x1, y1] world metres of a (row, col) slice pair."""
    return [
        sl[1].start * cell,
        sl[0].start * cell,
        sl[1].stop * cell,
        sl[0].stop * cell,
    ]


def measure_tile(path: pathlib.Path, developed: np.ndarray | None = None):
    """Return (record, classes u8 [ny,nx], regions dict) or None on bad data.

    `developed` is the OSM-derived man-made mask (develop.py). Where it is
    supplied, graded ground is kept out of the detectors: road ditches are
    not channels, borrow pits are not basins. Without it every measurement
    still runs — the screen is advisory, not required."""
    z, (_, _, cell) = cgrid.read_f32(path)
    z = z.astype(float)
    valid = np.isfinite(z)
    if valid.mean() < MIN_VALID:
        return None
    classes = np.zeros(z.shape, dtype=np.uint8)
    classes[~valid] |= CLASS_NODATA
    if developed is not None and developed.shape == z.shape:
        classes[developed] |= CLASS_DEVELOPED
    else:
        developed = None
    # Nearest-fill the few gaps so flow routing is closed.
    if not valid.all():
        idx = ndimage.distance_transform_edt(
            ~valid, return_distances=False, return_indices=True
        )
        z = z[tuple(idx)]

    k = {}
    x = {}  # extras: recorded, not (yet) knobs
    # metrics: how the surface is ORGANIZED. Never fitted into the prior —
    # these are the acceptance checks `compare` gates on (structure.py).
    metrics = {}
    regions = {
        "version": 1,
        "extract_version": EXTRACT_VERSION,
        "cell_m": cell,
        "channels": [],
        "ridges": [],
        "basins": [],
        "scarps": [],  # filled by the shape pass (blufffit)
        "transects": [],
        "valley_centerlines": [],  # filled by the shape pass
    }

    # relief / tilt / core reference
    k["relief_amp_m"] = float(np.percentile(z, 99) - np.percentile(z, 1))
    c0, c1 = int(750 / cell), int(2250 / cell)
    zc = z[c0:c1, c0:c1]
    k["core_relief_cap_m"] = float(np.percentile(zc, 99) - np.percentile(zc, 1))
    grade, (gx, gy) = _plane_grade(z, cell, np.isfinite(z))
    k["tilt_grade"] = grade
    if grade > 1e-12:
        regions["tilt"] = {
            "grade": grade,
            "downhill_xy": [-gx / grade, -gy / grade],
        }
    else:
        regions["tilt"] = {"grade": grade, "downhill_xy": None}

    # grain anisotropy + dune wavelength
    aniso = mcore.anisotropy(z, cell)
    x["aniso_ratio"] = aniso["anisotropy_ratio"]
    x["aniso_orientation_deg"] = aniso["anisotropy_orientation_deg"]
    # Dune wavelength stays unmeasured (quarantined) — the probe records
    # what the spectrum says without letting it reach a knob.
    _dune_probe(z, cell, x)
    k["dune_wavelength_m"] = float("nan")

    # hydrology: depression fill + FLAT-RESOLVED D8 accumulation.
    # `mcore.d8_accumulation` breaks elevation ties toward the first cardinal
    # direction, so every filled lake/pond/flat drained along a dead-straight
    # axis-aligned line — the artifact QA found on nearly every tile. flow.py
    # resolves flats with a BFS from their spill cells instead.
    zfill = mcore.fill_depressions(z, cell)
    rec, _slope = flow.receivers(zfill, cell)
    acc = flow.accumulate(rec).astype(float) * cell * cell  # drained area, m²
    filled = zfill > z + 0.01
    channels = acc >= CHANNEL_AREA_M2
    # A lake BODY is not a channel: flow genuinely concentrates along the
    # thalweg crossing it, but painting the whole filled pool as channel
    # inflates drainage density and the valley-component count. Keep the
    # thalweg (high accumulation), drop the pool.
    channels &= ~filled | (acc >= 4.0 * CHANNEL_AREA_M2)
    if developed is not None:
        # A road ditch drains like a channel and is dead straight — exactly
        # the artifact that made section-line country look like drainage.
        channels &= ~developed
    classes[channels] |= CLASS_CHANNEL
    classes[filled] |= CLASS_FILL_FLAT
    metrics.update(
        structure.structure_metrics(z, cell, acc, channels, CHANNEL_AREA_M2)
    )
    x["drainage_density_km_per_km2"] = float(
        channels.sum() * cell / 1000.0 / ((z.size * cell * cell) / 1e6)
    )
    _, ch_comps = _components(channels, cell, min_diag_m=800.0)
    k["valley_count"] = float(len(ch_comps))
    for i, (sl, diag) in enumerate(ch_comps):
        regions["channels"].append(
            {"id": i, "diag_m": diag, "bbox_m": _bbox_m(sl, cell), "fall_grad": None}
        )

    if ch_comps:
        # Route on the FILLED surface (connectivity) but measure the drop on
        # the raw one, excluding cells the fill raised: v2 measured on zfill,
        # where lake/pit flats are dead level, dragging the median to ~1e-4 —
        # below the Valley primitive's 5e-4 monotone floor.
        raw_drop = flow.drop_to_receiver(z, rec, cell)
        unfilled = ~filled
        has_rec = rec >= 0
        sample = raw_drop[channels & unfilled & has_rec]
        sample = sample[np.isfinite(sample) & (sample > 0)]
        k["valley_fall_grad"] = float(np.median(sample)) if sample.size >= 30 else float("nan")
        # per-component fall, for the QA overlay
        ch_lab, _ = ndimage.label(channels, structure=np.ones((3, 3)))
        for crec, (sl, _diag) in zip(regions["channels"], ch_comps):
            m = (ch_lab[sl] > 0) & unfilled[sl] & has_rec[sl]
            vals = raw_drop[sl][m]
            vals = vals[np.isfinite(vals) & (vals > 0)]
            crec["fall_grad"] = float(np.median(vals)) if vals.size >= 10 else None

        # cross-sections at the highest-accumulation cells (skip lake bodies:
        # a transect across still water measures the pool, not a valley)
        rank = np.where(filled, -1.0, acc)
        top = np.argsort(rank, axis=None)[-4000:]
        ys, xs = np.unravel_index(top[:: max(1, len(top) // 40)], z.shape)
        hw_s, wall_s = [], []
        for cy, cx in zip(ys, xs):
            ri = rec[cy, cx]
            if ri < 0 or filled[cy, cx]:
                continue
            dy, dx = divmod(int(ri), z.shape[1])
            dy, dx = dy - cy, dx - cx
            if dy == 0 and dx == 0:
                continue
            # transect perpendicular to flow
            px, py = -dy, dx
            norm = np.hypot(px, py)
            px, py = px / norm, py / norm
            for sgn in (-1.0, 1.0):
                prof = []
                for i in range(1, int(150 / cell)):
                    ty = int(round(cy + sgn * py * i))
                    tx = int(round(cx + sgn * px * i))
                    if not (0 <= ty < z.shape[0] and 0 <= tx < z.shape[1]):
                        break
                    prof.append(z[ty, tx] - z[cy, cx])
                prof = np.array(prof)
                if prof.size < 10:
                    continue
                shoulder = np.percentile(prof, 90)
                if shoulder < 1.0:
                    continue
                above1 = np.nonzero(prof > 1.0)[0]
                if above1.size == 0:
                    continue
                hw = (above1[0] + 1) * cell
                half_h = shoulder / 2.0
                abovehh = np.nonzero(prof > half_h)[0]
                if abovehh.size == 0:
                    continue
                d_hh = (abovehh[0] + 1) * cell
                wall = None
                if d_hh > hw:
                    wall = (half_h - 1.0) / (d_hh - hw)
                    wall_s.append(wall)
                hw_s.append(hw)
                regions["transects"].append(
                    {
                        "center_m": [(float(cx) + 0.5) * cell, (float(cy) + 0.5) * cell],
                        "perp_xy": [sgn * px, sgn * py],
                        "hw_m": float(hw),
                        "wall_grade": float(wall) if wall is not None else None,
                    }
                )
        k["valley_halfwidth_m"] = float(np.median(hw_s)) if hw_s else float("nan")
        k["valley_wall_grade"] = float(np.median(wall_s)) if wall_s else float("nan")
    else:
        k["valley_fall_grad"] = float("nan")
        k["valley_halfwidth_m"] = float("nan")
        k["valley_wall_grade"] = float("nan")

    # ridges — the geomorphon mask is the QA backdrop (it flags every spur,
    # ~25% of cells); the KNOB counts major traced crests (see the gate).
    step8 = int(8 / cell)
    z8 = z[::step8, ::step8]
    cls = geomorphons.classify(z8, 8.0, lookup_m=500.0, flat_deg=1.0)
    rmask = geomorphons.ridge_mask(cls)
    _, r_comps = _components(rmask, 8.0, min_diag_m=300.0)
    x["geomorphon_ridge_components"] = float(len(r_comps))
    metrics.update(structure.ridge_metrics(cls, 8.0, float(len(r_comps))))
    for i, (sl, diag) in enumerate(r_comps):
        regions["ridges"].append(
            {"id": i, "kind": "geomorphon", "diag_m": diag, "bbox_m": _bbox_m(sl, 8.0)}
        )
    # block-replicate the 8 m mask back to the 2 m grid for the class raster
    rmask_up = np.kron(rmask, np.ones((step8, step8), dtype=bool))
    classes[rmask_up[: classes.shape[0], : classes.shape[1]]] |= CLASS_RIDGE_RAW

    major, prom_s, hw_s2, round_s, flank_s = [], [], [], [], []
    try:
        traced, _ = ridgepipe.extract_ridges(
            *primitives_frame(z), cell, _ridge_cfg()
        )
    except Exception as exc:  # a fit failure must not lose the whole tile
        traced = []
        x["ridge_trace_error"] = float("nan")
        print(f"    ridgepipe failed: {type(exc).__name__}: {exc}")
    prom_gate = max(RIDGE_MIN_PROMINENCE_M, RIDGE_PROMINENCE_REL * k["relief_amp_m"])
    for r in traced:
        keep = r.prominence_med >= prom_gate and r.length_m >= RIDGE_MIN_LEN_M
        cl = np.asarray(r.centerline, dtype=float)
        regions["ridges"].append(
            {
                "id": len(regions["ridges"]),
                "kind": "accepted" if keep else "traced",
                "centerline_m": [[float(p[0]), float(p[1])] for p in cl],
                "prominence_m": float(r.prominence_med),
                "len_m": float(r.length_m),
                "crest_hw_m": float(r.crest_hw_med),
                "crest_round_m": float(r.crest_round),
                "flank_l": float(r.flank_l),
                "flank_r": float(r.flank_r),
            }
        )
        if not keep:
            continue
        major.append(r)
        prom_s.append(r.prominence_med)
        hw_s2.append(r.crest_hw_med)
        round_s.append(r.crest_round)
        flank_s.append(0.5 * (r.flank_l + r.flank_r))
        _stamp_polyline(classes, cl, cell, max(r.crest_hw_med, 3.0 * cell), CLASS_RIDGE_ACCEPTED)
    k["ridge_count"] = float(len(major))
    k["ridge_len_m"] = float(np.median([r.length_m for r in major])) if major else float("nan")
    # extras feed the planner's ridge shaping (steps/03 de-mesa work)
    k["ridge_crest_hw_m"] = float(np.median(hw_s2)) if hw_s2 else float("nan")
    x["ridge_prominence_m"] = float(np.median(prom_s)) if prom_s else float("nan")
    x["ridge_crest_round_m"] = float(np.median(round_s)) if round_s else float("nan")
    x["ridge_flank_grade"] = float(np.median(flank_s)) if flank_s else float("nan")

    # closed basins — gated at the Bowl-primitive scale (see BASIN_MIN_*),
    # so interdune dimples and puddle-scale pits don't inflate the count.
    depth = zfill - z
    lab, b_comps = _components(depth > 0.5, cell, min_diag_m=BASIN_MIN_DIAG_M)
    radii, depths, cents, eccs = [], [], [], []
    for ci, (sl, _diag) in enumerate(b_comps):
        m = lab[sl] > 0
        dmax = float(depth[sl][m].max())
        area = m.sum() * cell * cell
        ys, xs = np.nonzero(m)
        cy_m = (sl[0].start + float(ys.mean()) + 0.5) * cell
        cx_m = (sl[1].start + float(xs.mean()) + 0.5) * cell
        ecc = None
        if ys.size > 8:
            cov = np.cov(np.vstack([xs.astype(float), ys.astype(float)]))
            ev = np.sort(np.linalg.eigvalsh(cov))
            if ev[1] > 0:
                ecc = 1.0 - np.sqrt(max(ev[0], 0.0) / ev[1])
        dev_frac = float(developed[sl][m].mean()) if developed is not None else 0.0
        accepted = dmax >= BASIN_MIN_DEPTH_M and dev_frac <= 0.30
        rows = sl[0].start + ys
        cols = sl[1].start + xs
        classes[rows, cols] |= CLASS_BASIN if accepted else CLASS_BASIN_REJ
        regions["basins"].append(
            {
                "center_m": [cx_m, cy_m],
                "radius_m": float(np.sqrt(area / np.pi)),
                "depth_m": dmax,
                "ecc": float(ecc) if ecc is not None else None,
                "accepted": accepted,
                "developed_frac": dev_frac,
            }
        )
        if not accepted:
            continue
        radii.append(np.sqrt(area / np.pi))
        depths.append(dmax)
        cents.append(((sl[0].start + ys.mean()) * cell, (sl[1].start + xs.mean()) * cell))
        if ecc is not None:
            eccs.append(ecc)
    k["basin_count"] = float(len(depths))
    if depths:
        k["basin_radius_m"] = float(np.median(radii))
        k["basin_depth_m"] = float(np.median(depths))
        k["blowout_eccentricity"] = float(np.median(eccs)) if eccs else float("nan")
        if len(cents) >= 2:
            pts = np.array(cents)
            d2 = np.sqrt(((pts[:, None, :] - pts[None, :, :]) ** 2).sum(-1))
            np.fill_diagonal(d2, np.inf)
            k["basin_spacing_m"] = float(np.median(d2.min(axis=1)))
        else:
            k["basin_spacing_m"] = float("nan")
    else:
        for kk in ("basin_radius_m", "basin_depth_m", "basin_spacing_m", "blowout_eccentricity"):
            k[kk] = float("nan")

    # Benches/scarps are measured by the SHAPE pass (blufffit traces real
    # scarp centerlines). The old detector here binned the whole tile along
    # one global downhill axis, so it could only ever produce straight bands
    # and mislabeled any curved terrace edge.
    k["bench_count"] = float("nan")
    k["bench_scarp_h_m"] = float("nan")
    k["bench_tread_w_m"] = float("nan")

    # shape-tier: filled by the `shape` subcommand (dtm_primitives pass)
    k["meander_intensity"] = float("nan")
    k["meander_wavelength_mult"] = float("nan")

    if developed is not None:
        x["developed_frac"] = float(developed.mean())

    rec = {
        "extract_version": EXTRACT_VERSION,
        "knobs": k,
        "extras": x,
        "metrics": metrics,
        "valid_frac": float(valid.mean()),
    }
    return rec, classes, regions


def _carry_shape(dest: pathlib.Path, rec: dict, reg_dest: pathlib.Path, regions: dict):
    """Carry a previous `shape` pass through a re-extract.

    The two passes own disjoint fields and shape costs minutes per tile
    against seconds for extract, so re-running extract must not throw its
    results away. Re-run `shape --force` when the shape code itself changes.
    """
    from .shape import SHAPE_EXTRAS, SHAPE_KNOBS

    if not dest.exists():
        return
    try:
        old = json.loads(dest.read_text())
    except (json.JSONDecodeError, OSError):
        return
    if old.get("shape_version") is None:
        return
    for key in SHAPE_KNOBS:
        if key in old.get("knobs", {}):
            rec["knobs"][key] = old["knobs"][key]
    for key in SHAPE_EXTRAS:
        if key in old.get("extras", {}):
            rec["extras"][key] = old["extras"][key]
    rec["shape_version"] = old["shape_version"]
    if reg_dest.exists():
        try:
            old_reg = json.loads(reg_dest.read_text())
        except (json.JSONDecodeError, OSError):
            return
        for key in ("valley_centerlines", "scarps"):
            if old_reg.get(key):
                regions[key] = old_reg[key]
        regions["shape_version"] = old["shape_version"]


def _skip(dest: pathlib.Path, force: bool) -> bool:
    if force or not dest.exists():
        return False
    try:
        return json.loads(dest.read_text()).get("extract_version") == EXTRACT_VERSION
    except (json.JSONDecodeError, OSError):
        return False


def run(archetype: str | None = None, force: bool = False):
    tiles_dir = OUT / "tiles"
    for arch_dir in sorted(tiles_dir.iterdir()) if tiles_dir.exists() else []:
        if not arch_dir.is_dir():
            continue
        if archetype and arch_dir.name != archetype:
            continue
        out_dir = OUT / "extract" / arch_dir.name
        out_dir.mkdir(parents=True, exist_ok=True)
        for tile in sorted(arch_dir.glob("*.cgrid")):
            dest = out_dir / (tile.stem + ".json")
            if _skip(dest, force):
                print(f"  [skip] {arch_dir.name}/{tile.stem}")
                continue
            result = measure_tile(tile, develop.load_mask(arch_dir.name, tile.stem))
            if result is None:
                print(f"  [drop] {arch_dir.name}/{tile.stem}: too little valid data")
                continue
            rec, classes, regions = result
            rec["archetype"] = arch_dir.name
            rec["tile"] = tile.stem
            _carry_shape(dest, rec, out_dir / (tile.stem + ".regions.json"), regions)
            regions["archetype"] = arch_dir.name
            regions["tile"] = tile.stem
            _dump_json(dest, rec)
            cgrid.write_u8(out_dir / (tile.stem + ".classes.cgrid"), classes, 0.0, 0.0, 2.0)
            _dump_json(out_dir / (tile.stem + ".regions.json"), regions)
            print(f"  [ok] {arch_dir.name}/{tile.stem}")
