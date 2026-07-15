"""Coverage gate: can the generator reach where the real courses live in metric
space? If not, no p(theta) can match — it's a generator capability gap.
"""

from __future__ import annotations

import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

from .forward import MORPH_METRICS


def _clean_metrics(real, synth):
    """Metrics finite in both and non-degenerate in synth (drop near-constant)."""
    ok = []
    for m in MORPH_METRICS:
        if m not in real or m not in synth:
            continue
        r, s = real[m].to_numpy(float), synth[m].to_numpy(float)
        if not (np.isfinite(r).any() and np.isfinite(s).any()):
            continue
        if np.nanstd(s) < 1e-9 * (abs(np.nanmean(s)) + 1e-9):
            continue           # generator cannot vary this metric -> useless axis
        ok.append(m)
    return ok


def _standardize(real, synth, metrics):
    mu = real[metrics].mean().to_numpy(float)
    sd = real[metrics].std().to_numpy(float); sd[sd == 0] = 1.0
    Xr = (real[metrics].to_numpy(float) - mu) / sd
    Xs = (synth[metrics].to_numpy(float) - mu) / sd
    Xr = np.nan_to_num(Xr, nan=0.0); Xs = np.nan_to_num(Xs, nan=0.0)
    return Xr, Xs, mu, sd


def run(real, synth, out):
    """real: course_extent rows; synth: ok forward rows. Writes coverage/*."""
    os.makedirs(os.path.join(out, "coverage"), exist_ok=True)
    metrics = _clean_metrics(real, synth)

    # per-metric reachability -------------------------------------------------
    rows = []
    for m in MORPH_METRICS:
        if m not in real or m not in synth:
            continue
        r = real[m].to_numpy(float); s = synth[m].to_numpy(float)
        r = r[np.isfinite(r)]; s = s[np.isfinite(s)]
        if r.size == 0 or s.size == 0:
            continue
        s_lo, s_hi = np.percentile(s, 1), np.percentile(s, 99)
        inside = float(np.mean((r >= s_lo) & (r <= s_hi)))
        rows.append({"metric": m,
                     "real_min": r.min(), "real_max": r.max(),
                     "synth_p1": s_lo, "synth_p99": s_hi,
                     "frac_real_reachable": round(inside, 3),
                     "gap_low": r.min() < s_lo, "gap_high": r.max() > s_hi,
                     "used": m in metrics})
    reach = pd.DataFrame(rows).sort_values("frac_real_reachable")
    reach.to_csv(os.path.join(out, "coverage", "reachability.csv"), index=False)

    # PCA overlap -------------------------------------------------------------
    from sklearn.decomposition import PCA
    Xr, Xs, _, _ = _standardize(real, synth, metrics)
    pca = PCA(2).fit(np.vstack([Xr, Xs]))
    pr, psn = pca.transform(Xr), pca.transform(Xs)
    fig, ax = plt.subplots(figsize=(7, 6))
    ax.scatter(psn[:, 0], psn[:, 1], s=8, c="#888", alpha=0.35, label="synthetic (theta design)")
    ax.scatter(pr[:, 0], pr[:, 1], s=18, c="#B5533C", alpha=0.85, label="real courses")
    ax.set_xlabel("PC1"); ax.set_ylabel("PC2"); ax.legend()
    ax.set_title("Metric-space coverage: real vs generator envelope")
    fig.tight_layout(); fig.savefig(os.path.join(out, "coverage", "overlap.png"), dpi=120)
    plt.close(fig)

    # fraction of real courses inside the synthetic convex hull (PCA plane) ---
    from scipy.spatial import ConvexHull, Delaunay
    try:
        hull = Delaunay(psn[ConvexHull(psn).vertices])
        inside_hull = float(np.mean(hull.find_simplex(pr) >= 0))
    except Exception:
        inside_hull = float("nan")

    gaps = reach[(reach.frac_real_reachable < 0.9) & reach.used]
    summary = {
        "n_metrics_used": len(metrics),
        "n_metrics_degenerate": len(MORPH_METRICS) - len(metrics),
        "frac_real_inside_synth_hull_pca": round(inside_hull, 3),
        "metrics_with_reach_gap": list(gaps.metric),
    }
    with open(os.path.join(out, "coverage", "summary.txt"), "w") as f:
        f.write("COVERAGE GATE\n")
        f.write(f"metrics used for matching: {len(metrics)} "
                f"(dropped {summary['n_metrics_degenerate']} degenerate: "
                f"{[m for m in MORPH_METRICS if m not in metrics]})\n")
        f.write(f"real courses inside synthetic hull (PCA plane): {inside_hull:.2f}\n\n")
        f.write("Reachability gaps (real range the generator can't reach):\n")
        for _, r in reach[reach.frac_real_reachable < 0.9].iterrows():
            f.write(f"  {r.metric:24s} reachable {r.frac_real_reachable:.2f}  "
                    f"real[{r.real_min:.2f},{r.real_max:.2f}] synth[{r.synth_p1:.2f},{r.synth_p99:.2f}]"
                    f"{'  LOW' if r.gap_low else ''}{'  HIGH' if r.gap_high else ''}\n")
    print(f"coverage: {len(metrics)} metrics usable, real-in-hull {inside_hull:.2f}, "
          f"{len(gaps)} reach gaps -> coverage/summary.txt")
    return metrics, summary, reach
