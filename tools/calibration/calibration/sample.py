"""Space-filling design over theta in transformed [0,1]^d space."""

from __future__ import annotations

import numpy as np
from scipy.stats import qmc

from . import param_schema as ps


def design(n, seed=12345, method="sobol"):
    """Return (thetas: list[dict], U: ndarray[n,d] unit design)."""
    d = len(ps.NAMES)
    if method == "sobol":
        eng = qmc.Sobol(d=d, scramble=True, seed=seed)
        U = eng.random(n)
    else:  # latin hypercube
        eng = qmc.LatinHypercube(d=d, seed=seed)
        U = eng.random(n)
    thetas = [ps.unit_to_theta(U[i]) for i in range(n)]
    return thetas, U
