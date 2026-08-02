"""Plots for the feature matrix: per-course small multiples, cross-course
scatters, distribution overlays, and the scalar correlation heatmap."""

from __future__ import annotations

import os

import matplotlib
matplotlib.use("Agg")
import matplotlib.pyplot as plt
import numpy as np
import yaml

from . import io

GROUP_COLOR = {"lowland": "#4C9F70", "rolling": "#D9A441", "mountain": "#B5533C", "": "#888"}


def _hillshade(z, cs, az=315.0, alt=45.0):
    gy, gx = np.gradient(z, cs)
    az_r, alt_r = np.radians(az), np.radians(alt)
    lx, ly, lz = np.cos(alt_r) * np.cos(az_r), np.cos(alt_r) * np.sin(az_r), np.sin(alt_r)
    nx, ny, nz = -gx, gy, np.ones_like(z)
    n = np.sqrt(nx * nx + ny * ny + nz * nz)
    return np.clip((nx * lx + ny * ly + nz * lz) / n, 0, 1)


def _pick_representative(df, per_group=3):
    ex = df[df["clip_variant"] == "course_extent"]
    keys = []
    for g in ("lowland", "rolling", "mountain"):
        sub = ex[ex["group"] == g].sort_values("relief_p95_p5")
        if len(sub) == 0:
            continue
        idx = np.linspace(0, len(sub) - 1, per_group).round().astype(int)
        keys += list(sub.iloc[idx]["course_id"])
    return keys


def _small_multiples(df, cfg, axes, out):
    cids = _pick_representative(df)
    paths = {os.path.basename(p)[:-5]: p for p in io.list_courses(cfg)}
    dist_dir = os.path.join(out, "distributions")
    fig, axs = plt.subplots(len(cids), 5, figsize=(15, 2.6 * len(cids)))
    if len(cids) == 1:
        axs = axs[None, :]
    for r, cid in enumerate(cids):
        c = io.load_course(paths[cid], cfg, "course_extent")
        z = np.where(c.valid, c.height, np.nan)
        axs[r, 0].imshow(_hillshade(np.nan_to_num(c.height, nan=np.nanmean(c.height)), c.cell_size),
                         cmap="gray"); axs[r, 0].set_ylabel(cid, fontsize=8)
        axs[r, 0].set_title("hillshade" if r == 0 else "", fontsize=8)
        from . import core
        sl = core.slope_deg(c.height, c.cell_size, c.valid)
        axs[r, 1].imshow(np.where(c.valid, sl, np.nan), cmap="magma", vmin=0, vmax=25)
        axs[r, 1].set_title("slope" if r == 0 else "", fontsize=8)
        D = np.load(os.path.join(dist_dir, f"{cid}_course_extent.npz"))
        axs[r, 2].loglog(axes["psd_wavelength_m"], D["radial_psd"]); axs[r, 2].set_title("PSD" if r == 0 else "", fontsize=8)
        axs[r, 3].plot(axes["variogram_lag_m"], D["variogram"]); axs[r, 3].set_title("variogram" if r == 0 else "", fontsize=8)
        axs[r, 4].plot(axes["hypsometric_fraction"], D["hypsometric_curve"]); axs[r, 4].set_ylim(0, 1)
        axs[r, 4].set_title("hypsometric" if r == 0 else "", fontsize=8)
        for k in (0, 1):
            axs[r, k].set_xticks([]); axs[r, k].set_yticks([])
    fig.tight_layout()
    fig.savefig(os.path.join(out, "report", "small_multiples.png"), dpi=cfg["report"]["dpi"])
    plt.close(fig)


def _dist_overlays(df, axes, out):
    ex = df[df["clip_variant"] == "course_extent"]
    dist_dir = os.path.join(out, "distributions")
    fig, axs = plt.subplots(1, 4, figsize=(18, 4))
    titles = ["slope_hist (deg)", "radial_psd (log-log)", "variogram (m)", "hypsometric"]
    edges = axes["slope_hist_edges_deg"]
    slc = 0.5 * (edges[:-1] + edges[1:])
    for _, r in ex.iterrows():
        p = os.path.join(dist_dir, f"{r['course_id']}_course_extent.npz")
        if not os.path.exists(p):
            continue
        D = np.load(p); col = GROUP_COLOR.get(r["group"], "#888")
        axs[0].plot(slc, D["slope_hist"], color=col, alpha=0.25, lw=0.6)
        axs[1].loglog(axes["psd_wavelength_m"], D["radial_psd"], color=col, alpha=0.25, lw=0.6)
        axs[2].plot(axes["variogram_lag_m"], D["variogram"], color=col, alpha=0.25, lw=0.6)
        axs[3].plot(axes["hypsometric_fraction"], D["hypsometric_curve"], color=col, alpha=0.25, lw=0.6)
    for a, t in zip(axs, titles):
        a.set_title(t, fontsize=10)
    axs[3].set_ylim(0, 1)
    handles = [plt.Line2D([], [], color=GROUP_COLOR[g], label=g) for g in ("lowland", "rolling", "mountain")]
    axs[0].legend(handles=handles, fontsize=8)
    fig.suptitle("Per-course distributions (course_extent), coloured by relief band", y=1.02)
    fig.tight_layout()
    fig.savefig(os.path.join(out, "report", "dist_overlays.png"), dpi=110, bbox_inches="tight")
    plt.close(fig)


def _scatters(df, out):
    ex = df[df["clip_variant"] == "course_extent"]
    col = ex["group"].map(lambda g: GROUP_COLOR.get(g, "#888"))
    pairs = [("relief_p95_p5", "variogram_range", "relief vs correlation length"),
             ("relief_p95_p5", "canopy_fraction", "relief vs canopy"),
             ("hypsometric_integral", "slope_p95", "HI vs slope p95"),
             ("relief_p95_p5", "heterogeneity_index_100", "relief vs heterogeneity")]
    fig, axs = plt.subplots(1, 4, figsize=(18, 4))
    for a, (x, y, t) in zip(axs, pairs):
        a.scatter(ex[x], ex[y], c=col, s=14, alpha=0.8, edgecolor="none")
        a.set_xlabel(x, fontsize=9); a.set_ylabel(y, fontsize=9); a.set_title(t, fontsize=10)
    fig.tight_layout()
    fig.savefig(os.path.join(out, "report", "scatters.png"), dpi=110)
    plt.close(fig)


def _corr_heatmap(df, out):
    from . import features
    ex = df[df["clip_variant"] == "course_extent"][features.SCALAR_COLUMNS]
    corr = ex.corr()
    fig, ax = plt.subplots(figsize=(13, 12))
    im = ax.imshow(corr, cmap="RdBu_r", vmin=-1, vmax=1)
    ax.set_xticks(range(len(corr))); ax.set_xticklabels(corr.columns, rotation=90, fontsize=6)
    ax.set_yticks(range(len(corr))); ax.set_yticklabels(corr.columns, fontsize=6)
    fig.colorbar(im, fraction=0.046)
    ax.set_title("Scalar feature correlation (course_extent, 200 courses)")
    fig.tight_layout()
    fig.savefig(os.path.join(out, "report", "correlation_heatmap.png"), dpi=120)
    plt.close(fig)


def generate(df, axes, cfg, out):
    os.makedirs(os.path.join(out, "report"), exist_ok=True)
    _small_multiples(df, cfg, axes, out)
    _dist_overlays(df, axes, out)
    _scatters(df, out)
    _corr_heatmap(df, out)
    print("wrote report/{small_multiples,dist_overlays,scatters,correlation_heatmap}.png")
