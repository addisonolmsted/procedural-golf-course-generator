"""What a golf course leaves standing, frozen into tracked source.

`clearing_profile.py` measures it off 3,170 holes on 280 wooded courses and
writes `corpus/out/clearing_profile.json`. That lives under a gitignored `out/`
and needs the whole 651-tile corpus to rebuild, so the generator cannot depend
on it. This is the frozen extract, the same arrangement `canopy_model.py` uses.

  python3 tools/golf/clearing_model.py        # re-freeze from the corpus artifact

Everything here is a KEEP PROBABILITY -- the share of whatever trees are there
that a course leaves standing -- never an absolute tree rate. That is the
round's central measured finding: binning courses by their own tree cover, the
absolute in-corridor rate varies 2.4x while the normalised profile barely
moves. A course on heavy ground leaves proportionally the same trees. So the
local tree rate cancels out of the rule and never has to be estimated.

The shape is a function of ONE distance: how far a cell is from the nearest
hole's centre line. There is no separate green disc or tee disc, because the
projection already caps the polyline at both ends -- past the green, distance to
the line IS distance to the green. And there is no property-wide term, because
there is no property-wide effect: measured against ground more than 500 m from
any hole, cover between 100 m and 500 m out sits within 3 % of untouched. A golf
course is a set of corridors, not an opened-up estate.

`PARAMS` is not extracted from anything: it is the outcome of
`clearing_calibrate.py` plus the owner's one named departure, and is edited by
hand the way a calibrated dial always has been on this branch.
"""
from __future__ import annotations

import json
import pathlib

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
DATA_PATH = HERE / "clearing_model.json"
SOURCE = HERE / "corpus" / "out" / "clearing_profile.json"

CELL = 10.0
ZONES = ("tee", "landing", "green")
DIST = np.arange(0.0, 85.0, 5.0)            # 17 bin edges -> 16 bins
SHAPE_X = DIST[:-1] + 2.5                   # bin centres, where the values sit
UNTOUCHED_M = 500.0                         # beyond this nothing has touched it

_D = json.loads(DATA_PATH.read_text()) if DATA_PATH.exists() else {}
SHAPE = _D.get("shape", {})            # par -> zone -> keep prob at SHAPE_X
RETENTION = _D.get("retention", {})    # par -> zone -> sorted per-hole sample
UNTOUCHED = _D.get("untouched", 0.6)   # the corpus reference, for the record
ZONE_CORR = _D.get("zone_corr", [[1, .65, .47], [.65, 1, .61], [.47, .61, 1]])
ALL_BARE = _D.get("all_zones_bare", 0.216)
PATCH = _D.get("patch", {})            # target patch structure in the corridor
SIDE = _D.get("side", {})              # heavier-side / all-one-side / no-tree
META = _D.get("meta", {})

# H_CLEAR       fractal exponent of the retention field; higher = bigger clumps
# SIDE_SIGMA    per-hole one-sided bias, in field standard deviations
# TEE_OPEN_EXP  quantile warp on TEE retention only -- the owner's one departure
#               from measurement, asked for as "some tees in tight corridors and
#               some open, err more open than the measurement". A monotone
#               reweighting of quantiles, so it keeps the measured support (the
#               tightest tee stays as tight) and only moves the mass.
# Fitted by clearing_calibrate.py (2026-09-17) on half our fluvial seeds and
# chosen on the other half: fit 0.156, holdout 0.165. SIDE_SIGMA earns its keep
# -- a correlated field alone put only 68 % of a corridor's survivors on the
# heavier flank against a measured 82 %, and 12 % of holes all-on-one-side
# against 34 %. Real corridors are cut one-sided and the field does not do that
# by itself.
PARAMS = dict(H_CLEAR=0.5, SIDE_SIGMA=1.0, TEE_OPEN_EXP=1.40)

# APPROACH_NARROW_M  reads the approach corridor (the green segment short of
#     the last 15 m) this much further out, pulling each flank in by as much.
#     Left at ZERO, with the reason recorded: the approach first read 90 m wide
#     against 75 m on the 24 Carolina courses, but that instrument steps by
#     5 m a side. Interpolating the crossing, the approach is 80.1 m against
#     Carolina's 69.8, the drive landing zone 75.7 against 71.7, and the
#     stretch between 77.6 against 68.1 -- already the owner's "Carolina plus
#     about ten yards" without any shift. The dial stays for the day that
#     changes.
APPROACH_NARROW_M = 0.0
APPROACH_KEEP_END_M = 15.0
# OPEN_GROUND_FILLS_ATOM  a hole whose corridor holds no natural tree at all
#     takes one of the distribution's "bare" slots instead of drawing its own.
#     The corpus's 20 % of bare corridors includes holes routed over open
#     ground; on our tiles those are known before the draw (6.5 % of holes),
#     and letting them draw again double-counted them: 28 % bare.
OPEN_GROUND_FILLS_ATOM = True

# one stream, one purpose -- the rule crates/course-sandhills/src/rng.rs
# enforces for the generator proper. TAG_CLEAR must NOT be canopy_gen.TAG_NOISE:
# sharing it would correlate retention with placement and leave trees standing
# preferentially in the densest part of a stand.
TAG_CLEAR = 0x63_6C_72     # "clr", the retention field
TAG_ZONE = 0x7A_6F_6E      # "zon", the per-hole-per-zone retention draw
TAG_SIDE = 0x73_69_64      # "sid", the one-sided bias


def shape_for(par, zone):
    """Keep probability against |offset|, falling back off par when unmeasured."""
    s = SHAPE.get(str(par)) or SHAPE.get("all") or {}
    return s.get(zone) or (SHAPE.get("all") or {}).get(zone)


def retention_for(par, zone):
    r = RETENTION.get(str(par)) or {}
    v = r.get(zone)
    if v and len(v) >= 40:
        return v
    return (RETENTION.get("all") or {}).get(zone) or [0.3]


_MEAN_RET = {}


def mean_retention(par, zone):
    """The UNWARPED cohort mean of a zone's retention sample.

    The divisor that turns a drawn retention into a multiplier on the profile's
    own anchor. Unwarped on purpose: normalising the tee by its warped mean
    would cancel the warp and hand the owner back the measured tees.
    """
    k = (par, zone)
    if k not in _MEAN_RET:
        _MEAN_RET[k] = max(float(np.mean(retention_for(par, zone))), 1e-6)
    return _MEAN_RET[k]


def freeze() -> dict:
    src = json.loads(SOURCE.read_text())
    return {"meta": {"source": str(SOURCE.relative_to(HERE.parents[1])),
                     "n_holes": src["n_holes"], "n_courses": src["n_courses"],
                     "shape_x": SHAPE_X.tolist()},
            "untouched": src["untouched"],
            "shape": {par: {z: [float(min(v, 1.0)) for v in c] for z, c in zs.items()}
                      for par, zs in src["shape"].items()},
            "retention": src["retention"], "patch": src["patch"],
            "side": src["side"], "zone_corr": src.get("zone_corr"),
            "all_zones_bare": src.get("all_zones_bare")}


if __name__ == "__main__":
    d = freeze()
    DATA_PATH.write_text(json.dumps(d, indent=1))
    print(f"{d['meta']['n_holes']} holes on {d['meta']['n_courses']} courses, "
          f"untouched reference {100 * d['untouched']:.1f} %")
    for par in sorted(d["shape"]):
        for z in ZONES:
            c = d["shape"][par].get(z)
            if c:
                print(f"  par {par:3s} {z:8s} " + " ".join(f"{v:.3f}" for v in c[::2]))
    print(f"wrote {DATA_PATH} ({DATA_PATH.stat().st_size / 1024:.0f} kB)")
