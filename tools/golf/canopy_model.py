"""The canopy placement model, frozen into tracked source.

`tools/golf/canopy_metrics.py` fits a logistic model of tree presence from the
terrain on the 572 natural archetype tiles and writes it to
`corpus/out/canopy_by_terrain.json`. That file is a 4 MB local artifact under a
gitignored `out/`, and regenerating it needs the whole tile corpus on disk, so
the generator cannot depend on it. This module is the frozen extract: the three
fits we use, the per-tile coverage distributions we sample from, and the
measured patch statistics we are trying to match.

  python3 tools/golf/canopy_model.py          # re-freeze from the corpus artifact

`PARAMS` is not extracted from anything. It is the outcome of
`canopy_calibrate.py` and is edited by hand, the way a calibrated dial always
has been on this branch.

Archetype keys are `course_sandhills::mode::Mode::key()` verbatim, so a seed's
mode names its fit with no translation table.
"""
from __future__ import annotations

import json
import pathlib

HERE = pathlib.Path(__file__).resolve().parent
DATA_PATH = HERE / "canopy_model.json"
SOURCE = HERE / "corpus" / "out" / "canopy_by_terrain.json"

# the order the fit's coefficients were standardised in; must match
# canopy_metrics.FEATURES exactly or every z-score is applied to the wrong column
FEATURES = ("relief_pos", "tpi200", "slope", "roughness", "northness",
            "curvature", "d_water", "elev_rel")

ARCHETYPES = ("sandhills", "sandhills_nc", "sandhills_river")

_D = json.loads(DATA_PATH.read_text()) if DATA_PATH.exists() else {}
FITS = _D.get("fits", {})
COVERAGE = _D.get("coverage", {})     # archetype -> [per-real-tile treed fraction]
TILES = _D.get("tiles", {})           # archetype -> [{tree_frac, patch stats...}]
SUMMARY = _D.get("summary", {})       # archetype -> the published median row

# Which fit places the trees, and which corpus supplies the coverage. They are
# allowed to differ, and for the aeolian river seeds they do: see the round log.
PLACE_FIT = {"aeolian": "sandhills", "aeolian_river": "sandhills",
             "fluvial": "sandhills_nc", "fluvial_river": "sandhills_nc"}
COVER_FIT = {"aeolian": "sandhills", "aeolian_river": "sandhills_river",
             "fluvial": "sandhills_nc", "fluvial_river": "sandhills_nc"}

CELL = 10.0
OCTAVES_M = (10.0, 20.0, 40.0, 80.0, 160.0, 320.0, 640.0)

# a     how much of the score is terrain rather than noise
# H     fractal exponent; higher = more weight on the long octaves = bigger stands
# sm    low-pass on the model logit, metres
# grain extra weight on the 10 m octave; this is the edge-density dial
# r_edge   canopy ramps to full over this distance in from a stand's edge, metres
#
# Fitted by canopy_calibrate.py (2026-09-16) against EVERY real tile of the
# archetype, on half our seeds, and chosen on the other half. The holdout is
# not ceremony: a 16-tile search handed back a cell scoring 0.138 on its own
# tiles and 0.248 on tiles it had not seen.
#   sandhills        fit 0.161  holdout 0.066   (87 dry aeolian seeds)
#   sandhills_nc     fit 0.221  holdout 0.236   (134 fluvial)
#   sandhills_river  fit 0.204  holdout 0.108   (29 aeolian with a creek)
#
# Keyed by the archetype the tile is meant to RESEMBLE, not by the fit that
# places its trees: the aeolian river seeds are placed by the sandhills
# coefficients but scored against the sandhills_river tiles.
#
# `grain` is zero in all three. It was built as the edge-density dial and it
# works, but no cohort wanted it once each was scored against its own corpus;
# see the round log for what turning it up costs on the Carolina tiles.
PARAMS = {
    "sandhills":       dict(a=0.70, H=0.50, sm=30.0, grain=0.00, r_edge=20.0),
    "sandhills_nc":    dict(a=0.35, H=0.70, sm=50.0, grain=0.00, r_edge=20.0),
    "sandhills_river": dict(a=0.70, H=0.70, sm=20.0, grain=0.00, r_edge=20.0),
}

# Canopy percent over treed percent: the savanna signature, and the one number
# that separates a Carolina longleaf stand from a closed wood of the same
# extent. It follows the COVERAGE archetype rather than the placement fit,
# because it is a property of that country's vegetation, measured beside its
# coverage on the same tiles.
DENSITY = {'sandhills': 0.3281, 'sandhills_nc': 0.6639, 'sandhills_river': 0.2794}

# distinct per-purpose RNG tags; one stream one purpose, the rule
# crates/course-sandhills/src/rng.rs enforces for the generator proper
TAG_COVERAGE = 0x63_6F_76  # "cov"
TAG_NOISE = 0x6E_6F_69     # "noi"

PATCH_KEYS = ("n_patches", "isolated_rate", "isolated_per_km2", "p50_patch_cells",
              "p90_patch_cells", "largest_frac", "edge_density")


def freeze() -> dict:
    """Re-extract the tracked block from the corpus artifact."""
    src = json.loads(SOURCE.read_text())
    out = {"source": str(SOURCE.relative_to(HERE.parents[1])),
           "fits": {}, "coverage": {}, "tiles": {}, "summary": {}}
    for a in ARCHETYPES:
        out["fits"][a] = src["fits"][a]
        out["summary"][a] = src["summary"][a]
        rows = [r for r in src["tiles"] if r["archetype"] == a]
        out["coverage"][a] = sorted(float(r["tree_frac"]) for r in rows)
        out["tiles"][a] = [dict(tile=r["tile"], n=r["n"],
                                tree_frac=float(r["tree_frac"]),
                                tcc_mean=r["tcc_mean"],
                                **{k: r["patch"][k] for k in PATCH_KEYS})
                           for r in rows]
    return out


if __name__ == "__main__":
    d = freeze()
    DATA_PATH.write_text(json.dumps(d, indent=1))
    for a in ARCHETYPES:
        print(f"{a:16s} fit ok  {len(d['coverage'][a]):3d} tiles  "
              f"treed med {100 * d['coverage'][a][len(d['coverage'][a]) // 2]:6.2f} %")
    print(f"wrote {DATA_PATH} ({DATA_PATH.stat().st_size / 1024:.0f} kB)")
