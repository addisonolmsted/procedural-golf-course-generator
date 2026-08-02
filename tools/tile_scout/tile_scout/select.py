"""F3 final selection: the calibration ~200 from the ingested tile store's
2 m metrics, by greedy facility location — minimize each course's distance to
its nearest selected tile, with per-region caps for diversity. Artifact gates
(inpaint, water, nodata, canopy-noise) decide eligibility first."""

from __future__ import annotations

import json
import os

import numpy as np
import pandas as pd

from . import sconfig
from .f2_terrain import S1_COLS

# S1 + the robust S2 axes proven in Stage 1 (fit_targets)
S2_SUBSET = ["s2_beta_char", "s2_sf_rms_100", "s2_res_extrema_ha",
             "s2_res_slope_p50", "s2_prof_curv_p90"]


def _eligible(tiles: pd.DataFrame, cfg: dict) -> pd.Series:
    f3 = cfg["f3"]
    ok = pd.Series(True, index=tiles.index)
    if "ctx_inpaint_frac" in tiles:
        ok &= tiles["ctx_inpaint_frac"] <= f3["max_inpaint_frac"]
    if "ctx_water_fraction" in tiles:
        ok &= tiles["ctx_water_fraction"] <= f3["max_water_fraction"]
    if "ctx_low_confidence" in tiles:
        ok &= ~tiles["ctx_low_confidence"].astype(bool)
    return ok


def greedy_facility(Dc: np.ndarray, regions: list[str], caps: dict,
                    k: int) -> list[int]:
    """Dc: (n_courses, n_tiles) whitened distances. Pick k tiles maximizing
    coverage gain; region caps guard diversity; distance tiebreak for spread."""
    n_t = Dc.shape[1]
    best = np.full(Dc.shape[0], np.inf)
    chosen: list[int] = []
    used = {r: 0 for r in set(regions)}
    while len(chosen) < k:
        gain = np.where(Dc < best[:, None], best[:, None] - Dc, 0.0).sum(axis=0)
        gain[chosen] = -1.0
        for t in range(n_t):
            if used[regions[t]] >= caps[regions[t]]:
                gain[t] = -1.0
        if gain.max() <= 0:
            # coverage saturated -> switch to diversity (farthest from chosen)
            if not chosen:
                break
            far = Dc[:, chosen].min(axis=0) if False else None
            rem = [t for t in range(n_t) if t not in chosen
                   and used[regions[t]] < caps[regions[t]]]
            if not rem:
                break
            # farthest-from-selected in tile space via course-distance proxy
            t_star = max(rem, key=lambda t: float(np.min(
                [np.linalg.norm(Dc[:, t] - Dc[:, c]) for c in chosen])))
        else:
            t_star = int(np.argmax(gain))
        chosen.append(t_star)
        used[regions[t_star]] += 1
        best = np.minimum(best, Dc[:, t_star])
    return chosen


def run(cfg: dict) -> int:
    from . import archetypes, regions as R, scoring
    from .f2_terrain import _zframe
    courses = pd.read_parquet(
        sconfig.repo_path(cfg["paths"]["course_metrics_parquet"]))
    tiles = pd.read_parquet(
        sconfig.repo_path("tools/dtm_metrics/out/metrics_tiles.parquet"))
    regs = {r.name: r for r in R.load(sconfig.out_path("regions.json"))}
    man = json.load(open(
        sconfig.repo_path(os.path.join(cfg["paths"]["atlas_root"],
                                       "tiles_us.json"))))
    tiles = tiles[tiles.index.isin(man)]
    tiles["region"] = [man[k]["region"] for k in tiles.index]

    elig = _eligible(tiles, cfg)
    print(f"eligible: {int(elig.sum())}/{len(tiles)} tiles pass artifact gates")
    tiles = tiles[elig]

    cols = [c for c in S1_COLS + S2_SUBSET if c in tiles and c in courses]
    cz = _zframe(cfg, courses, weighted=True)[[c for c in cols if c in
                 _zframe(cfg, courses).columns]].fillna(0.0)
    # include S2 (not in the archetype frame) via course robust-z
    extra = [c for c in S2_SUBSET if c in tiles]
    if extra:
        ce = scoring.transform(courses, extra, [])
        med, sd = scoring.robust_scales(ce)
        cz = pd.concat([cz, scoring.zscore(ce, med, sd).fillna(0.0)], axis=1)
        te = scoring.zscore(scoring.transform(tiles, extra, []), med, sd)
    tz = _zframe(cfg, tiles, weighted=True)[[c for c in cz.columns
                 if c in _zframe(cfg, tiles).columns]].fillna(0.0)
    if extra:
        tz = pd.concat([tz, te.fillna(0.0)], axis=1)
    cz, tz = cz.align(tz, axis=1, join="inner", fill_value=0.0)

    L = scoring.whitener(cz.to_numpy())
    Dc = scoring.pairwise_dist(L, cz.to_numpy(), tz.to_numpy())  # courses x tiles

    caps = {r: int(np.ceil(cfg["f3"]["region_cap_factor"] * regs[r].quota))
            for r in tiles["region"].unique()}
    k = min(cfg["f3"]["n_select"], len(tiles))
    chosen = greedy_facility(Dc, list(tiles["region"]), caps, k)
    sel_keys = list(tiles.index[chosen])

    sel = {"selected": sel_keys, "n": len(sel_keys),
           "cfg_hash": sconfig.cfg_hash(cfg)}
    sconfig.save_json(sconfig.out_path("selection.json"), sel)
    # flip selected flags in the manifest
    for k_ in man:
        man[k_]["selected"] = k_ in set(sel_keys)
    with open(sconfig.repo_path(os.path.join(cfg["paths"]["atlas_root"],
                                             "tiles_us.json")), "w") as f:
        json.dump(man, f, indent=1, sort_keys=True)
        f.write("\n")
    # coverage: each course's nearest SELECTED tile
    sub = Dc[:, chosen]
    nn = sub.min(axis=1)
    print(f"selected {len(sel_keys)} tiles; per-course nearest-tile distance "
          f"p50 {np.median(nn):.2f} p90 {np.percentile(nn, 90):.2f} "
          f"max {nn.max():.2f}")
    return 0
