"""dtm_metrics — terrain-v2 Stage 1: metric extraction + noise-parameter
identifiability study over the Stage-0 DTM store.

Three metric families organized by the primitive-awareness principle
(docs/terrain-v2-plan.md + the Stage-1 plan):
  S1 macro   — the landform primitives' domain (validation/feasibility only),
  S2 char    — the texture character the Stage-4 noise layer must reproduce,
  S3 modctr  — the modulation contract (texture conditioned on landform position).

Reuses the pure metric kernels of tools/metrics/metrics/core.py (v1, frozen)
and adapts the v1 calibration patterns (Sobol design, HistGBT+SALib emulator,
reachability) to the new schema. See tools/dtm_metrics/README.md.
"""

import os
import sys

DTM_METRICS_VERSION = "0.1.0"

_HERE = os.path.dirname(os.path.abspath(__file__))
_REPO = os.path.normpath(os.path.join(_HERE, "..", "..", ".."))

# Make the frozen v1 kernels + the Stage-0 store readers importable.
for _p in (os.path.join(_REPO, "tools", "metrics"),
           os.path.join(_REPO, "tools", "dtm_atlas")):
    if _p not in sys.path:
        sys.path.insert(0, _p)
