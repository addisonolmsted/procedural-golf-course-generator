"""Coverage relevance: emulated leave-param-fixed reachability.

For each parameter: draw 4096 emulated metric vectors with the param FREE vs
FIXED at mid-box; a param is coverage-critical when fixing it drops any kept
metric's real-range reachability by >= 0.05 (real ranges from metrics.parquet,
confident courses).
"""

from __future__ import annotations

import os
import pickle

import numpy as np
import pandas as pd
from scipy.stats import qmc

from .. import store
from . import params as P

SBOX = os.path.join(store.OUT, "sandbox")


def _reach(pred: np.ndarray, real: np.ndarray) -> float:
    lo, hi = np.percentile(pred, [1, 99])
    fin = real[np.isfinite(real)]
    if fin.size == 0:
        return np.nan
    return float(((fin >= lo) & (fin <= hi)).mean())


def run() -> int:
    with open(os.path.join(SBOX, "models.pkl"), "rb") as f:
        models = pickle.load(f)
    real = pd.read_parquet(os.path.join(store.OUT, "metrics.parquet"))
    real = real[~real["ctx_low_confidence"].astype(bool)]

    mets = [m for m in models if m in real.columns and m.startswith("s2_")]
    eng = qmc.Sobol(d=len(P.NAMES), scramble=True, seed=99)
    U = eng.random_base2(m=12)  # 4096

    def predict(m, Umat):
        model, mdims, extra = models[m]
        cols = [P.NAMES.index(d) for d in mdims]
        X = Umat[:, cols]
        if extra:
            X = np.hstack([X, np.full((len(X), extra), 0.5)])
        return model.predict(X)

    base_reach = {m: _reach(predict(m, U), real[m].to_numpy(float)) for m in mets}
    rows = []
    for pi, pname in enumerate(P.NAMES):
        Ufix = U.copy()
        Ufix[:, pi] = 0.5
        worst = 0.0
        worst_m = "-"
        for m in mets:
            r = _reach(predict(m, Ufix), real[m].to_numpy(float))
            d = (base_reach[m] or 0.0) - (r or 0.0)
            if np.isfinite(d) and d > worst:
                worst, worst_m = d, m
        rows.append({"param": pname, "worst_reach_drop": round(worst, 4),
                     "on_metric": worst_m})
    out = pd.DataFrame(rows).set_index("param")
    out.to_csv(os.path.join(SBOX, "coverage_relevance.csv"))
    print(out.to_string())
    base = pd.Series(base_reach, name="base_reach")
    base.to_csv(os.path.join(SBOX, "base_reachability.csv"))
    print("wrote coverage_relevance.csv / base_reachability.csv")
    return 0
