"""Forward map theta -> generated terrain -> metric vector x.

Generation is batched through `xtask forward-grid` (explicit params, dry water =
pure morphology). Metrics REUSE the metrics package unchanged: identical
`features.compute` under the identical preprocessing config (resample cell,
detrend policy), clipped as `course_extent` == whole generated tile (all land;
water is empty because the forward pass is dry). This is what puts real and
synthetic terrain in one feature space.
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor

import numpy as np
import pandas as pd
import yaml

from . import param_schema as ps

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "..", "..", ".."))
METRICS_DIR = os.path.join(REPO, "tools", "metrics")
sys.path.insert(0, METRICS_DIR)
from metrics import features                      # noqa: E402  (reuse, unchanged)
from metrics.synthetic import read_heightgrid     # noqa: E402

OUT = os.path.normpath(os.path.join(HERE, "..", "out"))

# morphology objective = all scalar metrics EXCEPT land cover (canopy/water) —
# the generator's vegetation/water is calibrated separately (morphology-only).
LANDCOVER = {"canopy_fraction", "canopy_fraction_clip", "water_fraction"}
# Landform metrics that FAILED their acceptance gates (2026-07-11, all-200 real
# vs 240 v9 grids) — kept in the parquet as diagnostics, excluded from matching:
# rough_cv_30 is 0.989-redundant with rough_top10_30 (kept the better-AUC one);
# block_ratio/valley_count/valley_len showed no real-vs-synth signal (|AUC-0.5|
# < 0.10: 0.507, 0.466, 0.425); chan_len_p90 discriminates strongly (AUC 0.135)
# but is SEED-dominated (between-theta variance share 0.25 < 0.5, campaign-1
# gate) — matching would chase seed noise, so it stays diagnostic (its
# systematic reach gap still motivated the ridged_mix mechanism).
DIAGNOSTIC_ONLY = {"rough_cv_30", "rough_block_ratio_350",
                   "valley_count_km2_60", "valley_len_p90_120", "chan_len_p90"}
MORPH_METRICS = [c for c in features.SCALAR_COLUMNS
                 if c not in LANDCOVER and c not in DIAGNOSTIC_ONLY]


def metrics_cfg():
    return yaml.safe_load(open(os.path.join(METRICS_DIR, "metrics", "config.yaml")))


def _score_grid(args):
    """Worker: (grid_path, mcfg, axes) -> dict(scalars) or None on failure."""
    grid_path, mcfg, axes = args
    try:
        h, cs, w = read_heightgrid(grid_path)
    except Exception:
        return None
    valid = np.ones(h.shape, bool)
    if mcfg["mask"]["exclude_water"]:
        valid &= ~w
    S, _ = features.compute(h, cs, valid, mcfg, axes, tree=None, water=w)
    return S


def run_manifest(manifest, tag):
    """Write the manifest, batch-generate in Rust, score every grid in parallel.
    Returns the list of scalar dicts (or None) aligned to `manifest`."""
    mcfg = metrics_cfg()
    axes = features.build_axes(mcfg)
    grid_dir = os.path.join(OUT, "grids", tag)
    os.makedirs(grid_dir, exist_ok=True)
    mpath = os.path.join(grid_dir, "manifest.json")
    json.dump(manifest, open(mpath, "w"))
    print(f"  generating {len(manifest)} grids via xtask forward-grid ...")
    subprocess.run(["cargo", "run", "-q", "-p", "xtask", "--release", "--",
                    "forward-grid", mpath], cwd=REPO, check=True)
    tasks = [(it["out"], mcfg, axes) for it in manifest]
    scored = [None] * len(tasks)
    with ProcessPoolExecutor(max_workers=max(1, (os.cpu_count() or 2) - 1)) as ex:
        for i, S in enumerate(ex.map(_score_grid, tasks, chunksize=4)):
            scored[i] = S
    return scored


def forward_cell_extent(cfg):
    mcfg = metrics_cfg()
    return float(mcfg["resample"]["common_cell_size_m"]), float(cfg["forward"]["extent_m"])


def run_forward(thetas, cfg, tag="forward"):
    """Generate + score every (theta, seed); return a DataFrame of rows."""
    cell, extent = forward_cell_extent(cfg)
    seeds = list(cfg["forward"]["seeds"])
    grid_dir = os.path.join(OUT, "grids", tag)

    manifest, index = [], []
    for ti, th in enumerate(thetas):
        conf = ps.theta_to_config(th)
        chash = ps.config_hash(conf)
        for seed in seeds:
            out = os.path.join(grid_dir, f"t{ti:05d}_s{seed}.bin")
            manifest.append({"out": out, "seed": int(seed), "cell": cell,
                             "extent": extent, **conf})
            index.append((ti, th, seed, conf, chash, out))
    scored = run_manifest(manifest, tag)

    rows = []
    for (ti, th, seed, conf, chash, out), S in zip(index, scored):
        row = {"theta_id": ti, "seed": int(seed), "config_hash": chash}
        row.update({k: th[k] for k in ps.NAMES})
        if S is None:
            row["status"] = "read_fail"
        else:
            relief = S.get("relief_p95_p5", float("nan"))
            beta = S.get("spectral_slope_beta", float("nan"))
            bad = (not np.isfinite(relief)) or relief < 0.5 or (not np.isfinite(beta))
            row["status"] = "degenerate_flat" if (np.isfinite(relief) and relief < 0.5) \
                else ("nan" if bad else "ok")
            row.update({k: S.get(k, float("nan")) for k in features.SCALAR_COLUMNS})
        rows.append(row)
    df = pd.DataFrame(rows)
    n_ok = int((df["status"] == "ok").sum())
    print(f"  scored {len(df)} rows: {n_ok} ok, "
          f"{int((df.status=='degenerate_flat').sum())} flat, "
          f"{int(df.status.isin(['nan','read_fail']).sum())} bad")
    return df
