"""Score the procedural generator's own terrain with the SAME metrics.

Reads the `HG01` grid written by `xtask dump-heightgrid` (composed heightfield
+ water mask), builds the (height, cell_size, valid) contract, and runs
`features.compute` — the exact function used on real courses. Then z-scores the
result with the standardization saved from the real run, so synthetic terrain
lands in the identical feature space (the whole point of the architecture).

    # generate + score a few seeds (needs the xtask binary built):
    python3 -m metrics.synthetic 2024 7 42

Outputs out/features_synthetic.parquet (+ _z) using out/standardization.json.
"""

from __future__ import annotations

import json
import os
import struct
import subprocess
import sys

import numpy as np
import pandas as pd
import yaml

from . import features

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "out"))
REPO = os.path.normpath(os.path.join(HERE, "..", "..", ".."))


def read_heightgrid(path):
    """Parse an HG01 grid -> (height float64 [ny,nx], cell_size, water bool)."""
    with open(path, "rb") as f:
        magic = f.read(4)
        if magic != b"HG01":
            raise ValueError(f"{path}: not an HG01 grid")
        ny, nx = struct.unpack("<II", f.read(8))
        (cell,) = struct.unpack("<f", f.read(4))
        h = np.frombuffer(f.read(4 * ny * nx), dtype="<f4").astype(np.float64).reshape(ny, nx)
        w = np.frombuffer(f.read(ny * nx), dtype=np.uint8).reshape(ny, nx).astype(bool)
    return h, float(cell), w


def dump_seed(seed, cfg, out_dir):
    """Invoke xtask to write a generated grid for `seed`; return its path."""
    cell = float(cfg["resample"]["common_cell_size_m"])
    extent = 1800.0
    path = os.path.join(out_dir, f"seed{seed}.bin")
    subprocess.run(
        ["cargo", "run", "-q", "-p", "xtask", "--release", "--",
         "dump-heightgrid", path, str(seed), str(cell), str(extent)],
        cwd=REPO, check=True)
    return path


def score_grid(height, cell_size, water, cfg, axes):
    """Run the shared feature computation on a synthetic grid (no tree layer)."""
    valid = np.ones(height.shape, bool)
    if cfg["mask"]["exclude_water"]:
        valid &= ~water
    S, D = features.compute(height, cell_size, valid, cfg, axes, tree=None, water=water)
    return S, D


def main(seeds):
    cfg = yaml.safe_load(open(os.path.join(HERE, "config.yaml")))
    axes = features.build_axes(cfg)
    syn_dir = os.path.join(OUT, "synthetic")
    os.makedirs(syn_dir, exist_ok=True)
    dist_dir = os.path.join(OUT, "distributions")
    os.makedirs(dist_dir, exist_ok=True)

    rows = []
    for seed in seeds:
        path = os.path.join(syn_dir, f"seed{seed}.bin")
        if not os.path.exists(path):
            path = dump_seed(seed, cfg, syn_dir)
        h, cs, w = read_heightgrid(path)
        S, D = score_grid(h, cs, w, cfg, axes)
        row = {"course_id": f"synthetic_{seed}", "label": f"synthetic seed {seed}",
               "clip_variant": "synthetic", "group": "synthetic",
               "n_valid_cells": int((~w).sum()), "common_cell_m": cs}
        row.update({k: S.get(k, float("nan")) for k in features.SCALAR_COLUMNS})
        rows.append(row)
        np.savez_compressed(os.path.join(dist_dir, f"synthetic_{seed}_synthetic.npz"), **D)
        print(f"  scored synthetic seed {seed}: relief {S['relief_p95_p5']:.1f} "
              f"beta {S['spectral_slope_beta']:.2f} drainage {S['drainage_density']*1000:.2f}/km")

    df = pd.DataFrame(rows)
    df.to_parquet(os.path.join(OUT, "features_synthetic.parquet"))

    # z-score with the REAL run's standardization (course_extent params)
    spath = os.path.join(OUT, "standardization.json")
    if os.path.exists(spath):
        std = json.load(open(spath)).get("course_extent", {})
        dz = df.copy()
        for col in features.SCALAR_COLUMNS:
            if col in std and std[col]["std"] > 0:
                dz[col] = (df[col] - std[col]["mean"]) / std[col]["std"]
        dz.to_parquet(os.path.join(OUT, "features_synthetic_z.parquet"))
        print("wrote features_synthetic[_z].parquet (z-scored to the real atlas)")
        _overlay_pca(dz)
    else:
        print("wrote features_synthetic.parquet (run run.py first for standardization)")


def _overlay_pca(syn_z):
    """Project synthetic seeds into the real atlas's PCA plane (the shared
    feature space) so we can see where generated terrain sits vs real courses."""
    real_path = os.path.join(OUT, "features_scalar_z.parquet")
    if not os.path.exists(real_path):
        return
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    from sklearn.decomposition import PCA
    real = pd.read_parquet(real_path)
    real = real[real["clip_variant"] == "course_extent"]
    cols = [c for c in features.SCALAR_COLUMNS
            if c in real and not real[c].isna().all() and c in syn_z]
    Xr = real[cols].to_numpy(float)
    Xr = np.where(np.isnan(Xr), 0.0, Xr)          # already z-scored: mean 0
    pca = PCA(2).fit(Xr)
    sr = pca.transform(Xr)
    Xs = np.where(np.isnan(syn_z[cols].to_numpy(float)), 0.0, syn_z[cols].to_numpy(float))
    ss = pca.transform(Xs)
    cmap = {"lowland": "#4C9F70", "rolling": "#D9A441", "mountain": "#B5533C"}
    fig, ax = plt.subplots(figsize=(7, 6))
    for g in ("lowland", "rolling", "mountain"):
        m = real["group"] == g
        ax.scatter(sr[m.to_numpy(), 0], sr[m.to_numpy(), 1], s=14, color=cmap[g], alpha=0.6, label=g)
    ax.scatter(ss[:, 0], ss[:, 1], marker="*", s=240, color="k", edgecolor="w",
               label="synthetic (generator)", zorder=5)
    for i, cid in enumerate(syn_z["course_id"]):
        ax.annotate(cid.replace("synthetic_", "seed "), (ss[i, 0], ss[i, 1]),
                    fontsize=7, xytext=(4, 4), textcoords="offset points")
    ax.set_xlabel("PC1 (ruggedness)"); ax.set_ylabel("PC2 (sharpness)")
    ax.legend(fontsize=8); ax.set_title("Generator vs real atlas in the shared feature space")
    fig.tight_layout()
    os.makedirs(os.path.join(OUT, "report"), exist_ok=True)
    fig.savefig(os.path.join(OUT, "report", "synthetic_vs_real.png"), dpi=110)
    plt.close(fig)
    print("wrote report/synthetic_vs_real.png")


if __name__ == "__main__":
    seeds = [int(s) for s in sys.argv[1:]] or [2024, 7, 42]
    main(seeds)
