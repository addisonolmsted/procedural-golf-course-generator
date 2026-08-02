"""Fit-recovery (identifiability) — the strongest evidence line for "which
parameters are worth using": invert each campaign-C known-config metric vector
through the emulators and score per-parameter recovery.

Recoverable <=> Spearman(true, recovered) >= 0.6 across the 40 configs.
Confounded pairs surface as correlated recovery errors + near-identical ST
signatures.
"""

from __future__ import annotations

import os
import pickle

import numpy as np
import pandas as pd
from scipy.optimize import differential_evolution
from scipy.stats import spearmanr

from .. import store
from . import params as P

SBOX = os.path.join(store.OUT, "sandbox")


def run() -> int:
    with open(os.path.join(SBOX, "models.pkl"), "rb") as f:
        models = pickle.load(f)
    dfc = pd.read_parquet(os.path.join(SBOX, "campaign_c.parquet"))
    dfa = pd.read_parquet(os.path.join(SBOX, "campaign_a.parquet"))

    results = {}
    for mode, dims in (("flat", [n for n in P.NAMES if n not in P.FLAT_INERT]),
                       ("skel", list(P.NAMES))):
        sub = dfc[dfc["mode"] == mode]
        if sub.empty:
            continue
        # usable emulators for this mode: fitted on matching dim count
        mets = [m for m, (model, mdims, extra) in models.items()
                if mdims == dims or (mode == "skel" and len(mdims) == len(P.NAMES))]
        mets = [m for m in mets if m in sub.columns]
        if len(mets) < 5:
            print(f"{mode}: too few emulated metrics ({len(mets)}) — skipped")
            continue
        # standardization scales from campaign A spread
        sigma = {m: max(float(dfa[m].std()), 1e-9) if m in dfa else 1.0
                 for m in mets}
        agg = sub.groupby("theta_id")[mets + P.NAMES].mean()

        rec_rows = []
        for ti, row in agg.iterrows():
            target = row[mets].to_numpy(float)
            tfin = np.isfinite(target)
            if tfin.sum() < 5:
                continue

            def loss(u):
                # vectorized: u has shape (n_dims, S) -> return (S,)
                U = u.T if u.ndim == 2 else u[None, :]
                S = U.shape[0]
                tot = np.zeros(S)
                for k, m in enumerate(mets):
                    if not tfin[k]:
                        continue
                    model, mdims, extra = models[m]
                    cols = [dims.index(d) if d in dims else -1 for d in mdims]
                    X = np.where(
                        np.asarray(cols)[None, :] >= 0,
                        U[:, [max(c, 0) for c in cols]],
                        0.5)
                    if extra:
                        X = np.hstack([X, np.full((S, extra), 0.5)])
                    pred = model.predict(X)
                    tot += ((pred - target[k]) / sigma[m]) ** 2
                return tot if u.ndim == 2 else float(tot[0])

            res = differential_evolution(
                loss, bounds=[(0.0, 1.0)] * len(dims), seed=int(ti),
                maxiter=40, popsize=16, tol=1e-6, polish=False,
                vectorized=True, updating="deferred")
            rec = {f"rec_{d}": res.x[i] for i, d in enumerate(dims)}
            true = {f"true_{d}": P.value_to_unit(d, row[d]) for d in dims}
            rec_rows.append({"theta_id": ti, **true, **rec})
        rdf = pd.DataFrame(rec_rows)
        rdf.to_parquet(os.path.join(SBOX, f"recovery_{mode}.parquet"))

        rows = []
        for d in dims:
            t = rdf[f"true_{d}"]
            r = rdf[f"rec_{d}"]
            rho = float(spearmanr(t, r).statistic) if len(rdf) >= 8 else np.nan
            rows.append({"param": d, "spearman": rho,
                         "med_abs_err": float((t - r).abs().median())})
        results[mode] = pd.DataFrame(rows).set_index("param")
        print(f"{mode}: recovered {len(rdf)} configs over {len(mets)} metrics")

    out = pd.concat(results, axis=1) if results else pd.DataFrame()
    out.to_csv(os.path.join(SBOX, "recovery_summary.csv"))
    print("wrote recovery_summary.csv")
    print(out.round(3).to_string())
    return 0
