"""Emulators + sensitivity: per-metric HistGBT surrogate theta -> E_seed[metric]
with 4-fold CV R2, SALib Sobol total-effect ST, theta-vs-seed variance shares
(the sandbox side of gate G3), and the HIT/SURPRISE intent report against the
hypothesized targets (v1 pattern).

Campaign A drives the base params x s1/s2 metrics; campaign B (when present)
drives the full 19 dims x s3/s3t metrics (skeleton one-hots appended as
features, ST computed over the param dims only).
"""

from __future__ import annotations

import os
import pickle

import numpy as np
import pandas as pd
from SALib.analyze import sobol as salib_sobol
from SALib.sample import saltelli
from sklearn.ensemble import HistGradientBoostingRegressor
from sklearn.model_selection import cross_val_score

from .. import store
from . import params as P

SBOX = os.path.join(store.OUT, "sandbox")


def metric_cols(df: pd.DataFrame, prefixes=("s1_", "s2_", "s3_")) -> list[str]:
    return [c for c in df.columns
            if c.startswith(prefixes) and df[c].dtype.kind == "f"
            and not c.endswith("__clean")]


def theta_share(df: pd.DataFrame, cols: list[str]) -> pd.Series:
    """Between-theta variance share (theta-driven vs seed-noise)."""
    out = {}
    for m in cols:
        g = df.groupby("theta_id")[m]
        between = g.mean().var()
        within = g.var().mean()
        tot = (between or 0.0) + (within or 0.0)
        out[m] = float(between / tot) if tot and np.isfinite(tot) and tot > 0 else np.nan
    return pd.Series(out, name="theta_share")


def _fit_one(X: np.ndarray, y: np.ndarray):
    model = HistGradientBoostingRegressor(max_depth=4, max_iter=300,
                                          learning_rate=0.06, random_state=0)
    finite = np.isfinite(y)
    if finite.sum() < 40:
        return None, np.nan
    Xf, yf = X[finite], y[finite]
    r2 = float(np.mean(cross_val_score(model, Xf, yf, cv=4, scoring="r2")))
    model.fit(Xf, yf)
    return model, r2


def _sobol_st(model, dims: list[str], n: int, extra_cols: int = 0) -> dict:
    problem = {"num_vars": len(dims), "names": dims,
               "bounds": [[0.0, 1.0]] * len(dims)}
    Xs = saltelli.sample(problem, n, calc_second_order=False)
    if extra_cols:
        Xs = np.hstack([Xs, np.full((len(Xs), extra_cols), 0.5)])
    y = model.predict(Xs)
    if np.std(y) < 1e-12:
        return {d: 0.0 for d in dims}
    S = salib_sobol.analyze(problem, y, calc_second_order=False,
                            print_to_console=False)
    return {d: float(max(v, 0.0)) for d, v in zip(dims, S["ST"])}


def run() -> int:
    cfg = store.load_config()
    ecfg = cfg["campaigns"]["emulator"]
    os.makedirs(SBOX, exist_ok=True)

    dfa = pd.read_parquet(os.path.join(SBOX, "campaign_a.parquet"))
    dims_a = [n for n in P.NAMES if n not in P.FLAT_INERT]
    cols_a = metric_cols(dfa, ("s1_", "s2_"))
    share_a = theta_share(dfa, cols_a)

    # per-theta seed means; unit-space X
    agg = dfa.groupby("theta_id")[cols_a + P.NAMES].mean()
    X = np.stack([[P.value_to_unit(n, v) for v in agg[n]] for n in dims_a], 1)

    models: dict = {}
    r2s: dict = {}
    st_rows: dict = {}
    for m in cols_a:
        model, r2 = _fit_one(X, agg[m].to_numpy())
        r2s[m] = r2
        if model is not None and np.isfinite(r2) and r2 >= ecfg["r2_gate"]:
            models[m] = (model, dims_a, 0)
            st_rows[m] = _sobol_st(model, dims_a, ecfg["saltelli_n"])
    print(f"campaign A: {len(cols_a)} metrics, "
          f"{sum(1 for v in r2s.values() if np.isfinite(v) and v >= ecfg['r2_gate'])} pass R2 gate")

    # campaign B: modulation params on skeletons (s3 + s3t metrics)
    bpath = os.path.join(SBOX, "campaign_b.parquet")
    share_b = pd.Series(dtype=float)
    if os.path.exists(bpath):
        dfb = pd.read_parquet(bpath)
        cols_b = metric_cols(dfb, ("s3_",)) + [c for c in dfb.columns
                                               if c.startswith("s3t_")]
        share_b = theta_share(dfb, cols_b)
        skels = sorted(dfb["skeleton"].unique())
        aggb = dfb.groupby(["skeleton", "theta_id"])[cols_b + P.NAMES].mean().reset_index()
        Xb = np.stack([[P.value_to_unit(n, v) for v in aggb[n]] for n in P.NAMES], 1)
        onehot = np.stack([(aggb["skeleton"] == s).to_numpy(float) for s in skels[1:]], 1) \
            if len(skels) > 1 else np.zeros((len(aggb), 0))
        Xb_full = np.hstack([Xb, onehot])
        for m in cols_b:
            model, r2 = _fit_one(Xb_full, aggb[m].to_numpy())
            r2s[m] = r2
            if model is not None and np.isfinite(r2) and r2 >= ecfg["r2_gate"]:
                models[m] = (model, list(P.NAMES), onehot.shape[1])
                st_rows[m] = _sobol_st(model, list(P.NAMES), ecfg["saltelli_n"],
                                       extra_cols=onehot.shape[1])
        print(f"campaign B: {len(cols_b)} s3/s3t metrics fitted")

    st = pd.DataFrame(st_rows).T  # metrics x params
    st.to_csv(os.path.join(SBOX, "sobol_st.csv"))
    pd.Series(r2s, name="cv_r2").to_csv(os.path.join(SBOX, "emulator_r2.csv"))
    pd.concat([share_a, share_b]).to_csv(os.path.join(SBOX, "theta_share.csv"))
    with open(os.path.join(SBOX, "models.pkl"), "wb") as f:
        pickle.dump(models, f)

    # intent report: HIT if a param's hypothesized metric ranks it top-3
    lines = ["# Sensitivity intent report (HIT = hypothesized target ranks the "
             "param in its top-3 by ST)", ""]
    for pname in P.NAMES:
        if pname == "dummy" or pname not in st.columns:
            continue
        hyp = P.TARGETS[pname]
        hits = []
        for m in st.index:
            top3 = st.loc[m].nlargest(3).index
            if pname in top3 and any(t in m for t in hyp.split()):
                hits.append(m)
        best_m = st[pname].idxmax() if st[pname].max() > 0 else "-"
        tag = "HIT" if hits else "SURPRISE"
        lines.append(f"- {pname}: max ST {st[pname].max():.3f} on `{best_m}` "
                     f"[{tag}] (hypothesis: {hyp})")
    dummy_ceiling = float(st.get("dummy", pd.Series([0.0])).max())
    lines.append(f"\ndummy ST ceiling (noise floor): {dummy_ceiling:.4f}")
    with open(os.path.join(SBOX, "intent_report.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    print(f"wrote sobol_st.csv / emulator_r2.csv / theta_share.csv / "
          f"intent_report.md (dummy ceiling {dummy_ceiling:.4f})")
    return 0
