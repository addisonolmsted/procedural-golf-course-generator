"""Fast surrogate x ~ g(theta) per metric + variance-based (Sobol) sensitivity.

Emulates the theta -> E_seed[metric] map (seed noise averaged out). Reports CV
R^2 (gates which metrics are trustworthy) and the theta x metric Sobol total-
effect matrix, compared against the hypothesized `drives` intent map.
"""

from __future__ import annotations

import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
from sklearn.ensemble import HistGradientBoostingRegressor
from sklearn.model_selection import cross_val_score

from . import param_schema as ps
from .forward import MORPH_METRICS


def _mentions(drives_text, metric):
    """True if the schema's drives-text plausibly names this metric."""
    t = drives_text.lower()
    key = metric.split("_")[0]
    aliases = {"relief": "relief", "elev": "elev", "slope": "slope", "frac": "flat",
               "spectral": "beta", "hurst": "beta", "highfreq": "high-freq",
               "variogram": "variogram", "rms": "rough", "mean": "curv",
               "profile": "curv", "plan": "curv", "laplacian": "sharp",
               "local": "relief", "heterogeneity": "heterogeneity",
               "drainage": "drainage", "valley": "valley", "concavity": "concavity",
               "anisotropy": "anisotropy"}
    return metric in t or key in t or aliases.get(key, "\0") in t


def _theta_level(synth):
    """Mean metric over seeds per theta -> (Theta[n,d], Y dataframe)."""
    ok = synth[synth.status == "ok"]
    g = ok.groupby("theta_id")
    Theta = g[ps.NAMES].first().to_numpy(float)
    Y = g[MORPH_METRICS].mean()
    return Theta, Y


def fit(synth, cfg, out):
    os.makedirs(os.path.join(out, "emulator"), exist_ok=True)
    Theta, Y = _theta_level(synth)
    r2 = {}
    models = {}
    for m in MORPH_METRICS:
        y = Y[m].to_numpy(float)
        good = np.isfinite(y)
        if good.sum() < 30 or np.nanstd(y[good]) < 1e-9:
            r2[m] = float("nan"); continue
        model = HistGradientBoostingRegressor(max_depth=4, max_iter=300,
                                              learning_rate=0.06, random_state=0)
        sc = cross_val_score(model, Theta[good], y[good], cv=cfg["emulator"]["cv_folds"],
                             scoring="r2")
        r2[m] = float(np.mean(sc))
        models[m] = model.fit(Theta[good], y[good])
    r2s = pd.Series(r2, name="cv_r2").sort_values(ascending=False)
    r2s.to_csv(os.path.join(out, "emulator", "cv_r2.csv"))

    # Sobol total-effect via SALib on the emulator (sample in unit space) ------
    from SALib.sample import saltelli
    from SALib.analyze import sobol
    d = len(ps.NAMES)
    problem = {"num_vars": d, "names": ps.NAMES, "bounds": [[0.0, 1.0]] * d}
    U = saltelli.sample(problem, cfg["sensitivity"]["n_sobol"], calc_second_order=False)
    Th = np.array([[ps._from_unit(n, U[i, j]) for j, n in enumerate(ps.NAMES)]
                   for i in range(len(U))])
    ST = {}
    gate = cfg["emulator"]["r2_gate"]
    for m in MORPH_METRICS:
        if m not in models or not np.isfinite(r2[m]) or r2[m] < gate:
            continue
        yhat = models[m].predict(Th)
        Si = sobol.analyze(problem, yhat, calc_second_order=False, print_to_console=False)
        ST[m] = np.clip(Si["ST"], 0, None)
    STdf = pd.DataFrame(ST, index=ps.NAMES).T  # rows=metric, cols=theta
    STdf.to_csv(os.path.join(out, "emulator", "sobol_ST.csv"))

    # heatmap
    if len(STdf):
        fig, ax = plt.subplots(figsize=(0.5 * d + 3, 0.32 * len(STdf) + 2))
        im = ax.imshow(STdf.to_numpy(), aspect="auto", cmap="viridis", vmin=0, vmax=1)
        ax.set_xticks(range(d)); ax.set_xticklabels(ps.NAMES, rotation=90, fontsize=7)
        ax.set_yticks(range(len(STdf))); ax.set_yticklabels(STdf.index, fontsize=7)
        fig.colorbar(im, fraction=0.03, label="Sobol total effect")
        ax.set_title("theta x metric sensitivity (emulated, R2-gated metrics)")
        fig.tight_layout(); fig.savefig(os.path.join(out, "emulator", "sensitivity_heatmap.png"), dpi=120)
        plt.close(fig)

    # intent-map comparison ---------------------------------------------------
    # reverse the schema's `drives` text: for each metric, which params CLAIM it
    hypo = {m: [p for p in ps.NAMES if _mentions(ps.DRIVES[p], m)] for m in MORPH_METRICS}
    with open(os.path.join(out, "emulator", "intent_comparison.txt"), "w") as f:
        f.write("EMULATOR CV R2 (gate %.2f) and measured Sobol drivers vs hypothesized intent.\n"
                "'HIT' = the top measured driver was hypothesized; 'SURPRISE' = it was not.\n\n" % gate)
        for m in MORPH_METRICS:
            tag = f"R2={r2.get(m, float('nan')):.2f}"
            if m in STdf.index:
                top = STdf.loc[m].sort_values(ascending=False).head(3)
                drv = ", ".join(f"{k}({v:.2f})" for k, v in top.items())
                h = hypo[m]
                verdict = "HIT" if (h and top.index[0] in h) else ("SURPRISE" if h else "(no hypothesis)")
                f.write(f"{m:24s} {tag:9s} top: {drv:42s} intent[{','.join(h) or '-'}]  {verdict}\n")
            else:
                f.write(f"{m:24s} {tag:9s} (below R2 gate — not trusted / weakly driven)\n")
    n_pass = sum(1 for m in MORPH_METRICS if np.isfinite(r2.get(m, np.nan)) and r2[m] >= gate)
    print(f"emulator: {n_pass}/{len(MORPH_METRICS)} metrics pass R2>={gate}; "
          f"sobol_ST + heatmap + intent_comparison written")
    return models, r2, STdf
