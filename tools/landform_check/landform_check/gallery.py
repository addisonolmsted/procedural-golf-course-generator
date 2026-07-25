"""Per-archetype galleries: generated composed terrain (hillshade) next to
its nearest REAL tile by whitened S1 distance — the eyeball companion to
closure_report.json."""

from __future__ import annotations

import json
import os

import numpy as np
import pandas as pd

from . import OUT, REPO
from .run import _tile_resp, fleet

PAIRS_PER_ARCH = 8


def _png(z: np.ndarray, cell: float, out_png: str, max_px: int = 700):
    from dtm_atlas.qa import hillshade_rgb
    from PIL import Image
    valid = np.isfinite(z)
    k = max(1, int(np.ceil(z.shape[0] / max_px)))
    if k > 1:
        ny, nx = z.shape[0] // k, z.shape[1] // k
        z = np.nanmean(z[: ny * k, : nx * k].reshape(ny, k, nx, k),
                       axis=(1, 3))
        valid = np.isfinite(z)
        cell = cell * k
    rgb = hillshade_rgb(z, valid, cell)
    Image.fromarray(rgb).save(out_png)


def run() -> int:
    from dtm_atlas import config as atlas_config
    from dtm_atlas import grids as agrids
    from dtm_metrics.sandbox.campaign import read_hg01
    from tile_scout import scoring
    atlas_config.select_dataset("tiles")

    tile_resp, names = _tile_resp()
    s1g = pd.read_parquet(os.path.join(OUT, "s1_gen.parquet"))
    s1r = pd.read_parquet(os.path.join(OUT, "s1_real.parquet"))
    cols = sorted(c for c in s1r.columns if c != "tile" and c in s1g.columns)
    R = s1r[cols].to_numpy(float)
    fin_r = np.isfinite(R).all(1)
    R = R[fin_r]
    real_keys = s1r.tile.to_numpy()[fin_r]
    lw = scoring.whitener(R)

    gdir = os.path.join(OUT, "gallery")
    os.makedirs(gdir, exist_ok=True)
    rows_html: list[str] = []
    pop = {it["key"]: it for it in fleet() if it["group"] == "pop"}
    for a, name in enumerate(names):
        sub = s1g[(s1g.group == "pop") & (s1g.archetype == a)] \
            .sort_values("tile").head(PAIRS_PER_ARCH)
        if sub.empty:
            continue
        rows_html.append(f"<h2>{a}: {name}</h2>")
        for _, row in sub.iterrows():
            key = row["tile"]
            g = row[cols].to_numpy(float)
            if not np.isfinite(g).all():
                continue
            d = np.linalg.norm((R - g) @ lw.T, axis=1)
            j = int(np.argmin(d))
            near = real_keys[j]
            gp = os.path.join(gdir, f"{key}.png")
            rp = os.path.join(gdir, f"real_{near}.png")
            if not os.path.exists(gp):
                h, cell = read_hg01(os.path.join(OUT, "grids",
                                                 f"{key}.hg01"))
                _png(h[::-1].astype(float), cell, gp)
            if not os.path.exists(rp):
                arr, _tf, _e = agrids.read_gtiff(os.path.join(
                    atlas_config.STORE, near, "naturalized.tif"))
                nat = np.where(arr == atlas_config.NODATA, np.nan,
                               arr.astype(float))
                _png(nat, 2.0, rp)
            it = pop.get(key, {})
            rows_html.append(
                f"<div class='pair'><figure><img src='{key}.png'>"
                f"<figcaption>{key} (seed {it.get('seed', '?')})"
                f"</figcaption></figure>"
                f"<figure><img src='real_{near}.png'>"
                f"<figcaption>real {near} (S1 d={d[j]:.2f})"
                f"</figcaption></figure></div>")
    html = ("<!doctype html><meta charset='utf-8'>"
            "<title>landform_check gallery</title><style>"
            "body{font:14px system-ui;margin:20px} "
            ".pair{display:flex;gap:8px;margin:10px 0} "
            "figure{margin:0} img{width:460px;max-width:46vw} "
            "figcaption{font-size:11px;color:#555}</style>"
            "<h1>Generated (composed) vs nearest real tile</h1>"
            + "".join(rows_html))
    with open(os.path.join(gdir, "index.html"), "w") as f:
        f.write(html)
    print(f"gallery -> {os.path.join(gdir, 'index.html')}")
    return 0
