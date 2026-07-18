"""Distribution explorer — static HTML: per-metric histograms, a |Spearman|
correlation matrix, and an outlier table (the doc's "no unexplained outliers"
gate feeds on this). Self-contained (inline PNGs, no external assets).
"""

from __future__ import annotations

import base64
import io
import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import pandas as pd

from . import store


def _png_b64(fig) -> str:
    buf = io.BytesIO()
    fig.savefig(buf, format="png", dpi=100, bbox_inches="tight")
    plt.close(fig)
    return base64.b64encode(buf.getvalue()).decode()


def metric_columns(df: pd.DataFrame) -> list[str]:
    return [c for c in df.columns
            if c.startswith(("s1_", "s2_", "s3_")) and df[c].dtype.kind == "f"]


def run() -> int:
    path = os.path.join(store.OUT, "metrics.parquet")
    df = pd.read_parquet(path)
    ok = df[~df.get("ctx_low_confidence", False).astype(bool)]
    cols = metric_columns(df)
    parts = ["<!doctype html><meta charset='utf-8'><title>Stage-1 metrics explorer</title>",
             "<style>body{font:14px system-ui;margin:16px;background:#181818;color:#ddd}",
             "img{background:#fff;border-radius:4px;margin:4px}",
             "table{border-collapse:collapse}td,th{border:1px solid #444;padding:2px 8px;font-size:12px}",
             "h3{margin:18px 0 4px}</style>",
             f"<h2>Stage-1 metric distributions — {len(df)} courses "
             f"({len(ok)} confident)</h2>"]

    # histograms, 4 per row
    parts.append("<div>")
    for c in cols:
        v = ok[c].dropna()
        fig, ax = plt.subplots(figsize=(2.6, 1.8))
        if len(v):
            ax.hist(v, bins=24, color="#4a7", alpha=0.9)
        ax.set_title(f"{c} (n={len(v)})", fontsize=7)
        ax.tick_params(labelsize=6)
        parts.append(f"<img src='data:image/png;base64,{_png_b64(fig)}'>")
    parts.append("</div>")

    # correlation matrix
    sub = ok[cols]
    rho = sub.corr(method="spearman").abs()
    fig, ax = plt.subplots(figsize=(11, 10))
    im = ax.imshow(rho.to_numpy(), cmap="magma", vmin=0, vmax=1)
    ax.set_xticks(range(len(cols)))
    ax.set_xticklabels(cols, rotation=90, fontsize=5)
    ax.set_yticks(range(len(cols)))
    ax.set_yticklabels(cols, fontsize=5)
    fig.colorbar(im, shrink=0.6)
    parts.append("<h3>|Spearman| matrix (confident courses)</h3>")
    parts.append(f"<img src='data:image/png;base64,{_png_b64(fig)}'>")

    # outlier table: courses > 4 robust-z on any metric
    parts.append("<h3>Outliers (|robust z| > 4)</h3><table><tr><th>course</th>"
                 "<th>metric</th><th>value</th><th>z</th></tr>")
    n_out = 0
    for c in cols:
        v = ok[c].dropna()
        if len(v) < 20:
            continue
        med = v.median()
        mad = max((v - med).abs().median() * 1.4826, 1e-9)
        z = (v - med) / mad
        for key, zz in z[z.abs() > 4].items():
            parts.append(f"<tr><td>{key}</td><td>{c}</td>"
                         f"<td>{v[key]:.4g}</td><td>{zz:+.1f}</td></tr>")
            n_out += 1
    parts.append(f"</table><p>{n_out} outlier flags</p>")

    # NaN availability
    parts.append("<h3>Availability</h3><table><tr><th>metric</th><th>n finite</th></tr>")
    for c in cols:
        parts.append(f"<tr><td>{c}</td><td>{int(df[c].notna().sum())}/{len(df)}</td></tr>")
    parts.append("</table>")

    out = os.path.join(store.OUT, "explorer", "index.html")
    os.makedirs(os.path.dirname(out), exist_ok=True)
    with open(out, "w") as f:
        f.write("\n".join(parts))
    print(f"wrote {out} ({len(cols)} metrics, {n_out} outlier flags)")
    return 0
