"""Continuum distribution matching (label-free) via DENSITY-RATIO WEIGHTING.

Train a calibrated classifier to discriminate real vs synthetic metric vectors;
its odds give r(x) = p_real(x)/p_synth(x). Weight each design theta by the mean
r over its seeds. p(theta) is then the weighted ensemble of design points — a
continuous density with no terrain-class structure imposed. `sample_courses(n)`
resamples it (with small unit-space jitter) into real generator configs.
"""

from __future__ import annotations

import json
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

from . import param_schema as ps

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "out"))


def _standardize(real, synth, metrics):
    mu = real[metrics].mean().to_numpy(float)
    sd = real[metrics].std().to_numpy(float); sd[sd == 0] = 1.0
    Xr = np.nan_to_num((real[metrics].to_numpy(float) - mu) / sd, nan=0.0)
    Xs = np.nan_to_num((synth[metrics].to_numpy(float) - mu) / sd, nan=0.0)
    return Xr, Xs, mu, sd


def density_ratio(real, synth_rows, metrics, cfg):
    """r(x) for each synth row = p_real/p_synth via a calibrated classifier.

    Regularised (sigmoid-calibrated logistic in standardized metric space) so
    partial coverage doesn't produce runaway odds on the few near-real points.
    """
    from sklearn.linear_model import LogisticRegression
    from sklearn.calibration import CalibratedClassifierCV
    Xr, Xs, mu, sd = _standardize(real, synth_rows, metrics)
    X = np.vstack([Xr, Xs])
    y = np.r_[np.ones(len(Xr)), np.zeros(len(Xs))]
    base = LogisticRegression(C=1.0, max_iter=2000, class_weight="balanced")
    clf = CalibratedClassifierCV(base, method="sigmoid", cv=4).fit(X, y)
    p = np.clip(clf.predict_proba(Xs)[:, 1], 1e-3, 1 - 1e-3)
    r = (p / (1 - p)) * (len(Xs) / len(Xr))      # correct for class sizes
    return r, (clf, mu, sd)


def fit_ptheta(real, synth, metrics, cfg, out=OUT):
    """Fit p(theta) weights over the design points; write p_theta/*.

    Density-ratio weights are TEMPERED (w = r^temper) and clipped so the fitted
    p(theta) stays a usable, diverse sampler despite partial metric coverage —
    the unmatched directions are documented as capability gaps, not forced.
    """
    os.makedirs(os.path.join(out, "p_theta"), exist_ok=True)
    inv = cfg["inverse"]
    ok = synth[synth.status == "ok"].copy()
    r, model = density_ratio(real, ok, metrics, cfg)
    ok["r"] = r
    w = ok.groupby("theta_id")["r"].mean() ** float(inv.get("temper", 0.5))
    # clip the top weights so no single theta dominates (keeps ESS usable)
    cap = np.percentile(w.to_numpy(), float(inv.get("weight_clip_pct", 99)))
    w = np.minimum(w, cap)
    w = w / w.sum()
    design = ok.groupby("theta_id")[ps.NAMES].first()
    wdf = design.copy()
    wdf["weight"] = w
    wdf = wdf.reset_index()
    wdf.to_parquet(os.path.join(out, "p_theta", "weights.parquet"))

    ess = float(1.0 / np.sum(w.to_numpy() ** 2))  # effective sample size
    with open(os.path.join(out, "p_theta", "summary.txt"), "w") as f:
        f.write("CONTINUUM p(theta) via density-ratio weighting\n")
        f.write(f"design points: {len(w)}   effective sample size: {ess:.0f} "
                f"({100*ess/len(w):.0f}% of design)\n")
        f.write("(low ESS => real distribution is reachable only from a narrow theta region)\n\n")
        f.write("weighted-mean theta (the centre of p(theta)):\n")
        wm = (design.to_numpy() * w.to_numpy()[:, None]).sum(0)
        for n, v in zip(ps.NAMES, wm):
            f.write(f"  {n:20s} {v:.3f}\n")

    # weight-vs-beta sanity: matching should up-weight rougher (lower-beta) theta
    fig, ax = plt.subplots(1, 2, figsize=(11, 4))
    top = wdf.sort_values("weight", ascending=False)
    ax[0].bar(range(len(wdf)), np.sort(w.to_numpy())[::-1])
    ax[0].set_title(f"sorted theta weights (ESS {ess:.0f}/{len(w)})"); ax[0].set_xlabel("design rank")
    ax[1].scatter(design["gain"], w.to_numpy(), s=10, c="#444")
    ax[1].set_xlabel("gain (theta)"); ax[1].set_ylabel("p(theta) weight")
    ax[1].set_title("weight vs gain")
    fig.tight_layout(); fig.savefig(os.path.join(out, "p_theta", "weights.png"), dpi=110)
    plt.close(fig)
    print(f"inverse: p(theta) fit; ESS {ess:.0f}/{len(w)} -> p_theta/weights.parquet")
    return wdf, ess, model


# ---------------------------------------------------------------------------
# public sampler entrypoint
# ---------------------------------------------------------------------------

def sample_courses(n, out=OUT, jitter=0.03, seed=0, as_config=True):
    """Draw n courses from the fitted p(theta): resample design thetas by weight,
    jitter in unit space for continuity, return real generator configs.

    Returns list of {"terrain": {...}, "erosion": {...}} (feed `xtask forward-grid`
    or the generator directly), unless as_config=False (then theta dicts).
    """
    wdf = pd.read_parquet(os.path.join(out, "p_theta", "weights.parquet"))
    rng = np.random.default_rng(seed)
    w = wdf["weight"].to_numpy(float); w = w / w.sum()
    idx = rng.choice(len(wdf), size=n, replace=True, p=w)
    thetas = wdf.iloc[idx][ps.NAMES]
    outp = []
    for _, row in thetas.iterrows():
        u = np.array([ps._to_unit(nm, row[nm]) for nm in ps.NAMES])
        u = np.clip(u + rng.normal(0, jitter, size=u.shape), 0, 1)
        th = ps.unit_to_theta(u)
        outp.append(ps.theta_to_config(th) if as_config else th)
    return outp
