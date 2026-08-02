"""Geomorphon scale pilot (Stage 3T c2 gate): sweep lookup L x flatness t
over ~12 ingested tiles spanning regions; report class-fraction stability and
write QA class-raster PNGs next to hillshades. The production scale gets
frozen in config.yaml from this evidence."""

from __future__ import annotations

import json
import os

import numpy as np

from . import geomorphons as G

SWEEP_L = [250.0, 500.0, 1000.0]
SWEEP_T = [0.3, 1.0]
CELL10 = 10.0
COLLAR_M = 250.0

CLASS_COLORS = {
    0: (30, 30, 30), 1: (200, 200, 200), 2: (160, 30, 30), 3: (220, 90, 60),
    4: (235, 160, 90), 5: (240, 210, 120), 6: (150, 190, 120),
    7: (110, 170, 200), 8: (170, 200, 230), 9: (60, 110, 200),
    10: (40, 40, 160),
}


def _tile_10m(key: str):
    from dtm_atlas import config as atlas_config
    from dtm_atlas import grids
    arr, _tf, _e = grids.read_gtiff(
        os.path.join(atlas_config.STORE, key, "naturalized.tif"))
    z = np.where(arr == atlas_config.NODATA, np.nan, arr.astype(float))
    k = int(round(CELL10 / 2.0))
    ny, nx = z.shape[0] // k, z.shape[1] // k
    import warnings
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        return np.nanmean(z[: ny * k, : nx * k]
                          .reshape(ny, k, nx, k), axis=(1, 3))


def _pick_tiles(n: int = 12) -> list[str]:
    from dtm_atlas import config as atlas_config
    with open(atlas_config.TILES_MANIFEST) as f:
        man = json.load(f)
    stored = [k for k in sorted(man)
              if os.path.exists(os.path.join(atlas_config.STORE, k,
                                             "naturalized.tif"))]
    # spread across regions: round-robin by region
    by_region: dict[str, list[str]] = {}
    for k in stored:
        by_region.setdefault(man[k]["region"], []).append(k)
    picks: list[str] = []
    while len(picks) < n and any(by_region.values()):
        for r in sorted(by_region):
            if by_region[r] and len(picks) < n:
                picks.append(by_region[r].pop(0))
    return picks


def run(cfg: dict | None = None) -> int:
    from dtm_atlas import config as atlas_config
    from dtm_atlas import qa as atlas_qa
    atlas_config.select_dataset("tiles")
    out_dir = os.path.join(os.path.dirname(os.path.dirname(
        os.path.abspath(__file__))), "out", "geopilot")
    os.makedirs(out_dir, exist_ok=True)
    picks = _pick_tiles()
    print(f"pilot tiles ({len(picks)}): {picks}")
    collar = int(COLLAR_M / CELL10)
    rows = []
    for key in picks:
        z = _tile_10m(key)
        for L in SWEEP_L:
            for t in SWEEP_T:
                cls = G.classify(z, CELL10, lookup_m=L, flat_deg=t)
                fr = G.class_fractions(cls, collar_px=collar)
                rows.append({"key": key, "L": L, "t": t, **fr})
                if L == 500.0 and t == 1.0:
                    img = np.zeros((*cls.shape, 3), np.uint8)
                    for c, col in CLASS_COLORS.items():
                        img[cls == c] = col
                    atlas_qa._save_png(
                        os.path.join(out_dir, f"{key}_cls.png"), img)
                    hs = atlas_qa.hillshade_rgb(
                        z, np.isfinite(z), CELL10)
                    atlas_qa._save_png(
                        os.path.join(out_dir, f"{key}_hs.png"), hs)
    import pandas as pd
    df = pd.DataFrame(rows)
    df.to_csv(os.path.join(out_dir, "sweep.csv"), index=False)
    # stability: rank-corr of ridge fraction across L (at t=1.0)
    from scipy.stats import spearmanr
    base = df[(df.L == 500.0) & (df.t == 1.0)].set_index("key")["ridge"]
    print("\nridge-fraction stability vs L=500 (t=1.0):")
    for L in SWEEP_L:
        s = df[(df.L == L) & (df.t == 1.0)].set_index("key")["ridge"]
        rho = spearmanr(base, s.loc[base.index]).statistic
        print(f"  L={L:6.0f}: ridge frac p50 {s.median():.3f}  "
              f"rank-corr {rho:.3f}")
    print("\nclass fractions at L=500, t=1.0 (median over tiles):")
    med = df[(df.L == 500.0) & (df.t == 1.0)].median(numeric_only=True)
    for c in G.CLASS_NAMES.values():
        print(f"  {c:10s} {med[c]:.3f}")
    print(f"\nwrote {out_dir}/sweep.csv + QA PNGs")
    return 0
