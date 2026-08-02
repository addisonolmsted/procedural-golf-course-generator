"""Post-ingest linear-artifact check (B3): unmapped roads, berms, ditches
and pipeline cuts survive OSM-driven inpainting and leak into the 2 m
terrain as long, straight, narrow high-frequency stripes. Detector:

  highpass = z − gaussian(z, linear_hp_sigma_m)
  mask     = |highpass| ≥ linear_abs_m, outside water+inpaint
  components (8-conn) flagged when
     length ≥ linear_min_len_m  (extent along the PCA major axis)
     elongation ≥ linear_elong_min  (sqrt of eigenvalue ratio)
     minor-axis std ≤ 15 m          (stays in a narrow band — creeks wander)

Writes out/linear_check.parquet (tile, n_linear, longest_m, linear_frac);
`select` treats n_linear > 0 as ineligible once this exists."""

from __future__ import annotations

import json
import os

import numpy as np
import pandas as pd

from . import sconfig

MINOR_STD_MAX_M = 15.0
MIN_COMPONENT_PX = 200


def check_tile(nat: np.ndarray, masks_off: np.ndarray, cell: float,
               t: dict) -> dict:
    from scipy.ndimage import gaussian_filter, label
    z = np.where(np.isfinite(nat), nat, np.nanmedian(nat))
    hp = z - gaussian_filter(z, t["linear_hp_sigma_m"] / cell)
    m = (np.abs(hp) >= t["linear_abs_m"]) & np.isfinite(nat) & ~masks_off
    lab, n = label(m, structure=np.ones((3, 3), int))
    n_linear = 0
    longest = 0.0
    lin_px = 0
    for comp in range(1, n + 1):
        ys, xs = np.nonzero(lab == comp)
        if ys.size < MIN_COMPONENT_PX:
            continue
        pts = np.stack([xs * cell, ys * cell], 1)
        c = pts - pts.mean(0)
        cov = (c.T @ c) / len(c)
        evals, evecs = np.linalg.eigh(cov)
        minor, major = np.sqrt(max(evals[0], 1e-9)), np.sqrt(max(evals[1],
                                                                1e-9))
        along = c @ evecs[:, 1]
        length = float(along.max() - along.min())
        if length >= t["linear_min_len_m"] \
                and major / minor >= t["linear_elong_min"] \
                and minor <= MINOR_STD_MAX_M:
            n_linear += 1
            longest = max(longest, length)
            lin_px += int(ys.size)
    return {"n_linear": n_linear, "longest_m": round(longest, 1),
            "linear_frac": round(lin_px / max(nat.size, 1), 5)}


def run(cfg: dict, force: bool = False) -> int:
    from dtm_atlas import config as atlas_config
    from dtm_atlas import grids as agrids
    atlas_config.select_dataset("tiles")
    t = cfg["topup"]
    out_p = sconfig.out_path("linear_check.parquet")
    prev: dict[str, dict] = {}
    if os.path.exists(out_p) and not force:
        prev = {r["tile"]: r for r in
                pd.read_parquet(out_p).to_dict("records")}
    man = json.load(open(os.path.join(
        sconfig.repo_path(cfg["paths"]["atlas_root"]), "tiles_us.json")))
    rows = []
    todo = 0
    for key in sorted(man):
        if key in prev:
            rows.append(prev[key])
            continue
        sdir = os.path.join(atlas_config.STORE, key)
        p = os.path.join(sdir, "naturalized.tif")
        if not os.path.exists(p):
            continue
        arr, _tf, _e = agrids.read_gtiff(p)
        nat = np.where(arr == atlas_config.NODATA, np.nan, arr.astype(float))
        off = np.zeros(nat.shape, bool)
        for mname in ("water", "inpaint"):
            mp = os.path.join(sdir, "masks", f"{mname}.tif")
            if os.path.exists(mp):
                a, _t2, _e2 = agrids.read_gtiff(mp)
                off |= a > 0
        rows.append({"tile": key, **check_tile(nat, off, 2.0, t)})
        todo += 1
        if todo % 25 == 0:
            print(f"  ... {todo} checked")
    df = pd.DataFrame(rows)
    df.to_parquet(out_p)
    flagged = df[df.n_linear > 0]
    print(f"linear_check: {len(df)} tiles, {len(flagged)} flagged "
          f"(longest {flagged.longest_m.max() if len(flagged) else 0:.0f} m)"
          f" -> {out_p}")
    for r in flagged.sort_values("longest_m", ascending=False) \
            .head(12).itertuples():
        print(f"  {r.tile}: n={r.n_linear} longest={r.longest_m:.0f} m "
              f"frac={r.linear_frac}")
    return 0
