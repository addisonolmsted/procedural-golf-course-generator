"""G1 — the doc's robustness harness: inject synthetic contamination on clean
naturalized DTMs (threat model: man-made features Stage 0 MISSED — masks stay
unchanged), recompute the battery, and report per-metric worst-case movement.

Gate: S1 macro metrics <= 2% (IQR-relative; both denominators reported).
S2/S3 reported against a softer 0.10-population-std gate; violators get
redesigned or demoted to diagnostic (decided in gates.py).
"""

from __future__ import annotations

import dataclasses
import os
from concurrent.futures import ProcessPoolExecutor

import numpy as np
import pandas as pd

from . import candidates, store


def inject(nat: np.ndarray, ok: np.ndarray, cell: float, cfg: dict,
           seed: int) -> np.ndarray:
    """All four contaminations on a copy of the naturalized surface."""
    rng = np.random.default_rng(seed)
    z = nat.copy()
    icfg = cfg["injections"]
    ys, xs = np.nonzero(ok)

    def disc(cy, cx, radius_m, height_m):
        r = int(np.ceil(radius_m / cell))
        y0, y1 = max(cy - r, 0), min(cy + r + 1, z.shape[0])
        x0, x1 = max(cx - r, 0), min(cx + r + 1, z.shape[1])
        yy, xx = np.mgrid[y0:y1, x0:x1]
        d = np.hypot(yy - cy, xx - cx) * cell
        bump = np.where(d < radius_m,
                        height_m * 0.5 * (1.0 + np.cos(np.pi * d / radius_m)),
                        0.0)
        z[y0:y1, x0:x1] += bump

    if ys.size:
        p = icfg["pads"]
        for _ in range(p["count"]):
            i = rng.integers(ys.size)
            disc(ys[i], xs[i], p["diameter_m"] / 2.0, p["height_m"])
        q = icfg["ponds"]
        for _ in range(q["count"]):
            i = rng.integers(ys.size)
            disc(ys[i], xs[i], q["diameter_m"] / 2.0, q["depth_m"])
        s = icfg["spikes"]
        for k in range(s["count"]):
            i = rng.integers(ys.size)
            z[ys[i], xs[i]] += s["height_m"] * (1.0 if k % 2 == 0 else -1.0)
    z[z.shape[0] // 2:, :] += icfg["seam_step_m"]
    return z


def _worker(args) -> dict:
    key, cfg = args
    c = store.load_course(key)
    base = candidates.compute_course(c, cfg)
    zin = inject(c.naturalized, c.clean & c.interior, c.cell, cfg,
                 cfg["injections"]["seed"] + hash(key) % 100000)
    cin = dataclasses.replace(c, naturalized=zin)
    injected = candidates.compute_course(cin, cfg)
    out = {"key": key}
    for k, v in base.items():
        if k.startswith(("s1_", "s2_", "s3_")) and isinstance(v, float):
            out[f"{k}__base"] = v
            out[f"{k}__inj"] = injected.get(k, float("nan"))
    return out


def run(workers: int) -> int:
    cfg = store.load_config()
    mdf = pd.read_parquet(os.path.join(store.OUT, "metrics.parquet"))
    elig = mdf[mdf["ctx_clean_frac"] >= cfg["gates"]["g1_min_clean_frac"]]
    keys = list(elig.sort_values("ctx_clean_frac", ascending=False)
                .index[: cfg["gates"]["g1_n_courses"]])
    print(f"G1 robustness on {len(keys)} cleanest courses: {keys}")

    rows = []
    with ProcessPoolExecutor(max_workers=workers) as ex:
        for row in ex.map(_worker, [(k, cfg) for k in keys]):
            rows.append(row)
            print(f"  {row['key']}: done", flush=True)
    df = pd.DataFrame(rows).set_index("key")

    metrics = sorted({c[: -len("__base")] for c in df.columns if c.endswith("__base")})
    rep = []
    for m in metrics:
        b = df[f"{m}__base"]
        i = df[f"{m}__inj"]
        d = (i - b).abs()
        pop = mdf[m].dropna()
        iqr = max(float(pop.quantile(0.75) - pop.quantile(0.25)), 1e-9)
        std = max(float(pop.std()), 1e-9)
        rep.append({
            "metric": m,
            "family": m.split("_")[0],
            "worst_abs": float(d.max()),
            "worst_rel_value": float((d / b.abs().clip(lower=1e-9)).max()),
            "worst_rel_iqr": float((d / iqr).max()),
            "worst_rel_popstd": float((d / std).max()),
        })
    out = pd.DataFrame(rep).set_index("metric").sort_values("worst_rel_iqr",
                                                            ascending=False)
    os.makedirs(os.path.join(store.OUT, "robustness"), exist_ok=True)
    out.to_parquet(os.path.join(store.OUT, "robustness", "g1.parquet"))
    df.to_parquet(os.path.join(store.OUT, "robustness", "g1_raw.parquet"))

    g_macro = cfg["gates"]["g1_macro_iqr_rel"]
    g_char = cfg["gates"]["g1_char_popstd"]
    lines = ["# G1 robustness report", "",
             f"courses: {', '.join(keys)}", "",
             "| metric | worst |d| | /IQR | /popstd | gate | verdict |",
             "|---|---|---|---|---|---|"]
    for m, r in out.iterrows():
        is_macro = m.startswith("s1_")
        gate = g_macro if is_macro else g_char
        val = r["worst_rel_iqr"] if is_macro else r["worst_rel_popstd"]
        verdict = "PASS" if val <= gate else "FAIL"
        lines.append(f"| {m} | {r['worst_abs']:.4g} | {r['worst_rel_iqr']:.3f} "
                     f"| {r['worst_rel_popstd']:.3f} | {gate} | {verdict} |")
    rp = os.path.join(store.OUT, "reports")
    os.makedirs(rp, exist_ok=True)
    with open(os.path.join(rp, "robustness_report.md"), "w") as f:
        f.write("\n".join(lines) + "\n")
    n_fail = sum("FAIL" in ln for ln in lines)
    print(f"G1 done: {len(metrics)} metrics, {n_fail} gate failures "
          f"-> reports/robustness_report.md")
    return 0
