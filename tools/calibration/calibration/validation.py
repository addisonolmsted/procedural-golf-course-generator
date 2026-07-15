"""Round-trip validation: sample theta ~ p(theta), generate, measure with the
SAME metrics, and compare the resulting metric distribution to HELD-OUT real
courses. Also compares against the current seed sampler (baseline) to show
whether the fitted p(theta) matches the atlas better.
"""

from __future__ import annotations

import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd
from scipy import stats

from . import forward, inverse
from . import param_schema as ps


def energy_distance(A, B):
    """Multivariate energy distance between standardized samples A, B."""
    from scipy.spatial.distance import cdist
    aa = cdist(A, A).mean(); bb = cdist(B, B).mean(); ab = cdist(A, B).mean()
    return float(2 * ab - aa - bb)


def _score_configs(configs, cfg, tag, seeds):
    cell, extent = forward.forward_cell_extent(cfg)
    gdir = os.path.join(forward.OUT, "grids", tag)
    manifest = []
    for i, conf in enumerate(configs):
        for s in seeds:
            manifest.append({"out": os.path.join(gdir, f"c{i:05d}_s{s}.bin"),
                             "seed": int(s), "cell": cell, "extent": extent, **conf})
    scored = forward.run_manifest(manifest, tag)
    return pd.DataFrame([S for S in scored if S is not None])


def _baseline(n, cfg, tag, seeds):
    """Current seed sampler (no explicit params) -> metric rows."""
    cell, extent = forward.forward_cell_extent(cfg)
    gdir = os.path.join(forward.OUT, "grids", tag)
    manifest = [{"out": os.path.join(gdir, f"b{i:05d}.bin"),
                 "seed": int(1000 + i), "cell": cell, "extent": extent}
                for i in range(n)]
    scored = forward.run_manifest(manifest, tag)
    return pd.DataFrame([S for S in scored if S is not None])


def roundtrip(real_holdout, metrics, cfg, out):
    os.makedirs(os.path.join(out, "validation"), exist_ok=True)
    n = cfg["validation"]["n_roundtrip"]
    seeds = cfg["validation"]["roundtrip_seeds"]

    configs = inverse.sample_courses(n, out=out, seed=1)
    matched = _score_configs(configs, cfg, "roundtrip", seeds)
    base = _baseline(n, cfg, "baseline", seeds)

    mu = real_holdout[metrics].mean().to_numpy(float)
    sd = real_holdout[metrics].std().to_numpy(float); sd[sd == 0] = 1.0
    def z(df):
        return np.nan_to_num((df[metrics].to_numpy(float) - mu) / sd, nan=0.0)
    Zr, Zm, Zb = z(real_holdout), z(matched), z(base)

    ed_m = energy_distance(Zr, Zm)
    ed_b = energy_distance(Zr, Zb)

    # per-metric KS (matched vs holdout, baseline vs holdout)
    rows = []
    for m in metrics:
        r = real_holdout[m].dropna().to_numpy(float)
        mm = matched[m].dropna().to_numpy(float)
        bb = base[m].dropna().to_numpy(float)
        ks_m = stats.ks_2samp(r, mm).statistic if len(mm) else np.nan
        ks_b = stats.ks_2samp(r, bb).statistic if len(bb) else np.nan
        rows.append({"metric": m, "ks_matched": round(ks_m, 3), "ks_baseline": round(ks_b, 3),
                     "real_mean": np.mean(r), "matched_mean": np.mean(mm) if len(mm) else np.nan,
                     "resid_z": round((np.mean(mm) - np.mean(r)) / sd[metrics.index(m)], 2) if len(mm) else np.nan})
    per = pd.DataFrame(rows).sort_values("ks_matched", ascending=False)
    per.to_csv(os.path.join(out, "validation", "per_metric.csv"), index=False)

    # PCA overlap (fit on holdout real)
    from sklearn.decomposition import PCA
    pca = PCA(2).fit(Zr)
    fig, ax = plt.subplots(figsize=(7, 6))
    ax.scatter(*pca.transform(Zb).T, s=10, c="#bbb", alpha=0.5, label="baseline (current sampler)")
    ax.scatter(*pca.transform(Zm).T, s=12, c="#3B7", alpha=0.6, label="matched p(theta)")
    ax.scatter(*pca.transform(Zr).T, s=22, c="#B5533C", alpha=0.9, label="real (held-out)")
    ax.legend(); ax.set_xlabel("PC1"); ax.set_ylabel("PC2")
    ax.set_title("Round-trip: p(theta) vs baseline vs held-out real")
    fig.tight_layout(); fig.savefig(os.path.join(out, "validation", "pca_overlap.png"), dpi=120)
    plt.close(fig)

    with open(os.path.join(out, "validation", "report.txt"), "w") as f:
        f.write("ROUND-TRIP VALIDATION (metric distribution vs held-out real)\n\n")
        f.write(f"held-out real courses: {len(real_holdout)}   matched draws: {len(matched)}   "
                f"baseline draws: {len(base)}\n")
        f.write(f"energy distance to held-out real (standardized, lower=better):\n")
        f.write(f"  p(theta) matched : {ed_m:.4f}\n")
        f.write(f"  baseline sampler : {ed_b:.4f}\n")
        f.write((f"  improvement      : {100*(ed_b-ed_m)/ed_b:+.1f}%\n\n") if ed_b else "\n")
        f.write("per-metric KS (matched vs held-out real), worst first:\n")
        for _, r in per.head(12).iterrows():
            f.write(f"  {r.metric:24s} KS {r.ks_matched:.2f} (baseline {r.ks_baseline:.2f})  "
                    f"resid {r.resid_z:+.2f}z\n")
        f.write("\nlargest residuals are the metrics the generator still can't match "
                "(see coverage gaps).\n")
    print(f"validation: energy dist matched {ed_m:.3f} vs baseline {ed_b:.3f} "
          f"({100*(ed_b-ed_m)/ed_b:+.0f}%) -> validation/report.txt")
    return {"energy_matched": ed_m, "energy_baseline": ed_b}
