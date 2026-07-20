"""tile_scout — Stage 0T: find ~200 natural 3 km x 3 km DTM tiles whose
terrain character matches the collected golf courses, in less-developed land
near them.

The campaign is a multi-resolution funnel: soft archetypes over the course
S1 metric vectors -> region discs around member courses -> F0 regional 60 m
DEM screen -> F1 batched OSM development screen -> F2 per-candidate 10 m
full-S1 match -> ingestion through dtm_atlas (--dataset tiles) -> final
greedy facility-location selection on 2 m metrics. Courses are the matching
TARGET; the tile store replaces them as the calibration dataset.

Every stage is cache-first and byte-stable; the only RNG is the seeded GMM.
"""

import os
import sys

TILE_SCOUT_VERSION = "0.1.0"

_HERE = os.path.dirname(os.path.abspath(__file__))
_REPO = os.path.normpath(os.path.join(_HERE, "..", "..", ".."))

for _p in (os.path.join(_REPO, "tools", "metrics"),
           os.path.join(_REPO, "tools", "dtm_atlas"),
           os.path.join(_REPO, "tools", "dtm_metrics")):
    if _p not in sys.path:
        sys.path.insert(0, _p)

REPO = _REPO
OUT = os.path.normpath(os.path.join(_HERE, "..", "out"))
