"""landform_check — Stage 4T c9: the population closure loop.

Render a tile-population-matched fleet of SAMPLED, COMPOSED terrains (205
seeds with archetypes drawn from each calibration tile's responsibilities +
a 5x41 stratified panel), re-extract them with the SAME dtm_primitives
instruments (D8 mode) and the SAME 10 m S1 estimator, and compare the
generated population to the real tile population: whitened energy distance
+ per-field KS against a split-half baseline, variety (IQR ratio), galleries.
"""

import os
import sys

LANDFORM_CHECK_VERSION = "0.1.0"

_HERE = os.path.dirname(os.path.abspath(__file__))
_REPO = os.path.normpath(os.path.join(_HERE, "..", "..", ".."))
for _p in (os.path.join(_REPO, "tools", "dtm_primitives"),
           os.path.join(_REPO, "tools", "tile_scout"),
           os.path.join(_REPO, "tools", "dtm_atlas"),
           os.path.join(_REPO, "tools", "dtm_metrics"),
           os.path.join(_REPO, "tools", "metrics")):
    if _p not in sys.path:
        sys.path.insert(0, _p)
REPO = _REPO
OUT = os.path.normpath(os.path.join(_HERE, "..", "out"))

# fleet constants (documented in the closure report)
CELL_M = 2.5          # render resolution; k10=4 block-mean lands exactly at 10 m
K10 = 4
N_PANEL_PER_ARCH = 41
POP_SEED_SALT = b"lf4t-check:"
PANEL_SEED_BASE = 1_000_000
