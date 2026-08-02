"""landform_prior — Stage 3T c6: fit the conditional landform-parameter
prior from the tile extraction campaign (params/networks/branches parquets)
and emit landform_prior.json — the versioned, deterministic sampling product
the Rust sampler (Stage 4T) consumes.

Representation: per-archetype 11-point quantile tables + log-log hydraulic
regressions on drainage area (physics in the STRUCTURE, not the density
estimator) + a Gaussian copula over reach-level residuals + shrinkage toward
the pooled population for thin archetypes."""

import os
import sys

LANDFORM_PRIOR_VERSION = "0.1.0"

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
