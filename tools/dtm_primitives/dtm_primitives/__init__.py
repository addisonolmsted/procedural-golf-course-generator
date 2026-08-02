"""dtm_primitives — terrain-v2 Stage 3: primitive parameter extraction from
real courses.

For each course in the frozen Stage-0 store: detect which landform primitives
are present (valleys+tributaries, bluffs, bowls; ridges evidence-only) and
measure their parameters directly from the naturalized DTM. The per-course
record embeds a literal, renderable golf-landform MacroConfig (course-local
frame) plus measurement diagnostics — the measured pass-through inputs for
Stage-7a calibration and the round-trip gate against the Stage-2 library.

Reuses: the frozen v1 flow kernels (tools/metrics/metrics/core.py), the
Stage-0 store readers + OSM feature cache (tools/dtm_atlas), and the Stage-1
store bridge (tools/dtm_metrics). No RNG anywhere — byte-identical re-runs.
"""

import os
import sys

DTM_PRIMITIVES_VERSION = "0.1.0"
SCHEMA = 1

_HERE = os.path.dirname(os.path.abspath(__file__))
_REPO = os.path.normpath(os.path.join(_HERE, "..", "..", ".."))

# Frozen v1 kernels, Stage-0 store readers, Stage-1 bridge.
for _p in (os.path.join(_REPO, "tools", "metrics"),
           os.path.join(_REPO, "tools", "dtm_atlas"),
           os.path.join(_REPO, "tools", "dtm_metrics")):
    if _p not in sys.path:
        sys.path.insert(0, _p)

REPO = _REPO
