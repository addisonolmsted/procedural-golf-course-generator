"""Shape-tier pass: the deep dtm_primitives transect fits.

`extract` is the fast placement tier (counts, sizes, spacing) and runs in
seconds per tile. This pass runs the full valley pipeline — flow network →
smoothed centerlines → perpendicular transects → cross-section and meander
fits — which is minutes per tile, so it is a separate subcommand and is
resumable (`shape_version` stamped per tile, `--force` re-runs).

It fills the two knobs `extract` leaves null (`meander_intensity`,
`meander_wavelength_mult`) and appends `valley_centerlines` to the tile's
regions.json so tile-lab can draw what was fitted. Cross-section shape
(wall asymmetry, floor rounding) is recorded in `extras` — those are not
knobs yet; promoting them is a prior edit plus planner wiring.
"""

import json
import pathlib
import sys

import numpy as np
import yaml

_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_ROOT / "metrics"))
sys.path.insert(0, str(_ROOT / "dtm_primitives"))

from dtm_primitives import blufffit, meanderfit, valleypipe  # noqa: E402

from . import cgrid, netstats  # noqa: E402
from .extract import (  # noqa: E402
    _dump_json,
    _network_cfg,
    _plane_grade,
    primitives_frame,
)

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"
PRIMITIVES_CFG = _ROOT / "dtm_primitives" / "dtm_primitives" / "config.yaml"

SHAPE_VERSION = 3
# A meander fit on a stub of channel is noise; require enough arc to carry
# at least a couple of bends.
MIN_CENTERLINE_M = 400.0

# Scarp gate for the bench cascade. A "bench" in the planner's sense is a
# terrace edge a hole could sit above or below: tall enough to read as a
# landform, long enough to define a tread. (The old detector binned the
# whole tile along ONE global downhill axis and called high-gradient bins
# "benches" — straight strips that could not follow a curved terrace.)
SCARP_MIN_HEIGHT_M = 4.0
# Relaxed 2026-07-31 from 300 m. On the pilot corpus 98 of 138 mountain
# candidates were rejected on length alone and NONE on height, against a
# candidate median length of 236 m — the gate, not the terrain, was setting
# the count. A terrace riser is naturally broken into shorter segments by
# the gullies cutting back into it, so requiring one long unbroken scarp
# measures continuity rather than the presence of a bench.
SCARP_MIN_LEN_M = 150.0

# Cascade geometry. A riser counts toward the cascade when it runs within
# this many degrees of the contour (perpendicular to downhill), and two
# risers are the same LEVEL when their downhill-axis positions are closer
# than LEVEL_GAP_M.
CASCADE_ANGLE_TOL_DEG = 35.0
LEVEL_GAP_M = 200.0

# Knobs and extras this pass owns. `extract` preserves them when it re-runs,
# so a cheap re-extract does not silently discard an hour of transect fits
# (nothing here depends on extract's flow routing). `shape --force` re-runs.
SHAPE_KNOBS = (
    "junction_angle_deg",
    "junction_spacing_m",
    "branch_len_m",
    "junction_area_logratio",
    "chan_hw_at_a0_m",
    "chan_hw_area_exp",
    "slope_area_theta",
    "meander_intensity",
    "meander_wavelength_mult",
    "bench_count",
    "bench_scarp_h_m",
    "bench_tread_w_m",
)
SHAPE_EXTRAS = (
    "net_n_branches",
    "net_n_junctions",
    "net_strahler_max",
    "net_depth_max",
    "net_hw_fit_rows",
    "net_fall_fit_rows",
    "net_fall_at_a0",
    "meander_sinuosity",
    "valley_wall_asym_logratio",
    "valley_floor_round_m",
    "shape_branches",
    "shape_meander_fits",
    "scarp_candidates",
    "scarp_len_m",
)


def _weighted_median(vals, weights):
    v = np.asarray(vals, dtype=float)
    w = np.asarray(weights, dtype=float)
    ok = np.isfinite(v) & np.isfinite(w) & (w > 0)
    if not ok.any():
        return float("nan")
    v, w = v[ok], w[ok]
    order = np.argsort(v)
    v, w = v[order], w[order]
    c = np.cumsum(w)
    return float(v[np.searchsorted(c, 0.5 * c[-1])])


def shape_tile(tile_path: pathlib.Path, cfg: dict) -> tuple[dict, list] | None:
    """Return (knob updates, centerline records) for one tile."""
    z, (_, _, cell) = cgrid.read_f32(tile_path)
    z = z.astype(float)
    # dtm_primitives assumes row 0 = north; campaign tiles are row 0 = south.
    # See extract.primitives_frame — returned centerlines are then already in
    # our y-up world frame.
    zp, valid = primitives_frame(z)
    grade, (gx, gy) = _plane_grade(z, cell, np.isfinite(z))
    downhill = (-gx / grade, -gy / grade) if grade > 1e-12 else None
    masks = {"valid": valid.astype(np.uint8)}
    results, net = valleypipe.extract_valleys(zp, valid, masks, cell, [], cfg)
    # The branching topology rides on results[i].branch (parent,
    # junction_frac, mouth_area_m2) and was previously discarded.
    net_knobs, net_extras = netstats.network_stats(results, cell, net)

    lines, ints, wlmults, sinus, weights = [], [], [], [], []
    wall_asym, floor_round = [], []
    for br in results:
        cl = np.asarray(br.centerline, dtype=float)
        if len(cl) < 2:
            continue
        arc = float(np.hypot(*np.diff(cl, axis=0).T).sum())
        fit = None
        if arc >= MIN_CENTERLINE_M and np.isfinite(br.top_width_m) and br.top_width_m > 0:
            fit = meanderfit.decompose(cl, br.top_width_m, cfg)
        rec = {
            "pts_m": [[float(p[0]), float(p[1])] for p in cl[:: max(len(cl) // 200, 1)]],
            "arc_m": arc,
            "top_width_m": float(br.top_width_m) if np.isfinite(br.top_width_m) else None,
            "meander": None,
        }
        if fit is not None:
            rec["meander"] = {
                "intensity": _f(fit.intensity),
                "wavelength_mult": _f(fit.wavelength_mult),
                "sinuosity": _f(fit.sinuosity),
                "amplitude_m": _f(fit.amplitude_m),
                "wavelength_m": _f(fit.wavelength_m),
                "basis": fit.basis,
            }
            if np.isfinite(fit.intensity):
                ints.append(fit.intensity)
                weights.append(arc)
            if np.isfinite(fit.wavelength_mult):
                wlmults.append(fit.wavelength_mult)
            if np.isfinite(fit.sinuosity):
                sinus.append(fit.sinuosity)
        lines.append(rec)
        # cross-section shape, for a future knob promotion
        for cf in br.crossfits:
            if not getattr(cf, "ok", False):
                continue
            ml, mr = float(cf.ml), float(cf.mr)
            if np.isfinite(ml) and np.isfinite(mr) and min(ml, mr) > 1e-6:
                wall_asym.append(np.log(max(ml, mr) / min(ml, mr)))
            if np.isfinite(cf.r):
                floor_round.append(float(cf.r))

    # --- scarps (the bench cascade), as splines --------------------------
    # Valley corridors suppress plain valley walls; blufffit's pair test
    # still keeps a genuine bluff that happens to have a river at its toe.
    corridors = [
        (np.asarray(br.centerline, dtype=float), float(br.top_width_m))
        for br in results
        if len(br.centerline) > 1 and np.isfinite(br.top_width_m)
    ]
    scarps, scarp_h, scarp_len = [], [], []
    try:
        bluffs = blufffit.detect_and_fit(zp, valid, cell, cfg, valley_corridors=corridors)
    except Exception as exc:
        bluffs = []
        print(f"    blufffit failed: {type(exc).__name__}: {exc}")
    for b in bluffs:
        cl = np.asarray(b.centerline, dtype=float)
        keep = b.height_m >= SCARP_MIN_HEIGHT_M and b.length_m >= SCARP_MIN_LEN_M
        scarps.append(
            {
                "centerline_m": [[float(p[0]), float(p[1])] for p in cl],
                "height_m": float(b.height_m),
                "face_grad": float(b.face_grad),
                "length_m": float(b.length_m),
                "raise_left": bool(b.raise_left),
                "accepted": keep,
            }
        )
        if keep:
            scarp_h.append(b.height_m)
            scarp_len.append(b.length_m)

    knobs = {
        **net_knobs,
        "meander_intensity": _weighted_median(ints, weights) if ints else float("nan"),
        "meander_wavelength_mult": float(np.median(wlmults)) if wlmults else float("nan"),
        "bench_count": _bench_levels(scarps, downhill),
        "bench_scarp_h_m": float(np.median(scarp_h)) if scarp_h else float("nan"),
        "bench_tread_w_m": _tread_width(scarps, downhill, cell * z.shape[0]),
    }
    extras = {
        **net_extras,
        "meander_sinuosity": float(np.median(sinus)) if sinus else float("nan"),
        "valley_wall_asym_logratio": float(np.median(wall_asym)) if wall_asym else float("nan"),
        "valley_floor_round_m": float(np.median(floor_round)) if floor_round else float("nan"),
        "shape_branches": float(len(lines)),
        "shape_meander_fits": float(len(ints)),
        "scarp_candidates": float(len(scarps)),
        "scarp_len_m": float(np.median(scarp_len)) if scarp_len else float("nan"),
    }
    return knobs, extras, lines, scarps


def _f(v) -> float | None:
    v = float(v)
    return v if np.isfinite(v) else None


def _cascade_positions(scarps: list, downhill) -> list[float]:
    """Accepted CONTOUR-PARALLEL risers, projected onto the downhill axis.

    A bench cascade is a sequence of risers stepping down the regional
    grade, so its scarps run along the contour — their tangents are
    perpendicular to downhill. Most detected scarps are not that: in
    dissected ground they are valley walls and spur noses pointing every
    which way, and counting those measures dissection, not terracing.
    """
    if downhill is None:
        return []
    d = np.asarray(downhill, dtype=float)
    n = float(np.hypot(*d))
    if n < 1e-9:
        return []
    d = d / n
    pos = []
    for s in scarps:
        if not s["accepted"]:
            continue
        cl = np.asarray(s["centerline_m"], dtype=float)
        if len(cl) < 2:
            continue
        v = cl[-1] - cl[0]
        L = float(np.hypot(*v))
        if L < 1e-6:
            continue
        # |tangent . downhill| small  <=>  the riser follows the contour
        if abs(float((v / L) @ d)) > np.cos(np.deg2rad(90.0 - CASCADE_ANGLE_TOL_DEG)):
            continue
        pos.append(float(np.mean(cl @ d)))
    return sorted(pos)


def _cluster(pos: list[float], gap: float) -> list[float]:
    """Group positions into levels, returning each level's mean."""
    if not pos:
        return []
    levels, cur = [], [pos[0]]
    for a, b in zip(pos[:-1], pos[1:]):
        if b - a > gap:
            levels.append(float(np.mean(cur)))
            cur = []
        cur.append(b)
    levels.append(float(np.mean(cur)))
    return levels


def _bench_levels(scarps: list, downhill) -> float:
    """Number of terrace LEVELS in the cascade — what the planner places.

    Not the scarp-segment count: one terrace edge breaks into several
    segments wherever a gully cuts back into it, and feeding ~16 segments to
    the planner would divide the same regional drop into 16 tiny risers,
    turning a bench landscape into a staircase. Segments stay in
    `extras.scarp_candidates`.
    """
    return float(len(_cluster(_cascade_positions(scarps, downhill), LEVEL_GAP_M)))


def _tread_width(scarps: list, downhill, extent_m: float) -> float:
    """Median gap between adjacent terrace levels — the flat a cascade
    leaves between two risers."""
    levels = _cluster(_cascade_positions(scarps, downhill), LEVEL_GAP_M)
    if len(levels) < 2:
        return float("nan")
    gaps = [b - a for a, b in zip(levels[:-1], levels[1:]) if 0 < b - a < extent_m]
    return float(np.median(gaps)) if gaps else float("nan")


def run(archetype: str | None = None, force: bool = False):
    cfg = _network_cfg()  # sensitivity + raised network caps (see extract.py)
    tiles_dir = OUT / "tiles"
    for arch_dir in sorted(tiles_dir.iterdir()) if tiles_dir.exists() else []:
        if not arch_dir.is_dir() or (archetype and arch_dir.name != archetype):
            continue
        ex_dir = OUT / "extract" / arch_dir.name
        for tile in sorted(arch_dir.glob("*.cgrid")):
            rec_p = ex_dir / (tile.stem + ".json")
            if not rec_p.exists():
                print(f"  [skip] {arch_dir.name}/{tile.stem}: no extract record")
                continue
            rec = json.loads(rec_p.read_text())
            if not force and rec.get("shape_version") == SHAPE_VERSION:
                print(f"  [skip] {arch_dir.name}/{tile.stem}")
                continue
            try:
                knobs, extras, lines, scarps = shape_tile(tile, cfg)
            except Exception as exc:
                print(f"  [fail] {arch_dir.name}/{tile.stem}: {type(exc).__name__}: {exc}")
                continue
            rec["knobs"].update(knobs)
            rec.setdefault("extras", {}).update(extras)
            rec["shape_version"] = SHAPE_VERSION
            _dump_json(rec_p, rec)
            reg_p = ex_dir / (tile.stem + ".regions.json")
            if reg_p.exists():
                reg = json.loads(reg_p.read_text())
                reg["valley_centerlines"] = lines
                reg["scarps"] = scarps
                # superseded by `scarps` (see SCARP_MIN_* — the old axis-band
                # detector could not represent a curved terrace edge)
                reg.pop("bench_axis", None)
                reg.pop("bench_bands", None)
                reg["shape_version"] = SHAPE_VERSION
                _dump_json(reg_p, reg)
            print(
                f"  [ok] {arch_dir.name}/{tile.stem}: {len(lines)} branches, "
                f"{int(extras['shape_meander_fits'])} meander fits, "
                f"{int(knobs['bench_count'])}/{len(scarps)} scarps"
            )
