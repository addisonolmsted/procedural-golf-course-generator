"""Top-up campaign (screener v2): hunt pristine analogs for the courses the
clean subset under-serves. Two candidate sources: (a) DEEPEN existing regions
(the F0 rank window beyond the original keep), (b) EXTENDED discs (radius
ladder) around starved courses — the 120 km leash was the scarcity, not the
country. Funnel: F0 stats -> F1 counts -> verify2 GEOMETRY gates (building
area / paved length / protected preference) -> F2 10 m match -> targeted
finalize (append to the tile index, flagged topup)."""

from __future__ import annotations

import json
import os

import numpy as np
import pandas as pd

from . import TILE_SCOUT_VERSION, sconfig
from .regions import Region, haversine_km


def starved_courses(cfg: dict) -> pd.DataFrame:
    best = pd.read_csv(sconfig.out_path("reports", "best_match.csv"))
    return best[best["distance"] > cfg["topup"]["starved_dist"]] \
        .sort_values("distance", ascending=False)


def extended_regions(starved: pd.DataFrame, cfg: dict,
                     radius_km: float) -> list[Region]:
    """Group starved courses geographically -> extended-radius regions."""
    from .regions import course_centers
    centers = course_centers(cfg)
    todo = [c for c in starved["course"] if c in centers]
    regions: list[Region] = []
    used: set[str] = set()
    for seed in todo:
        if seed in used:
            continue
        members = [c for c in todo if c not in used
                   and haversine_km(*centers[seed], *centers[c]) <= 120.0]
        used.update(members)
        la = float(np.mean([centers[m][0] for m in members]))
        lo = float(np.mean([centers[m][1] for m in members]))
        from dtm_atlas import grids
        regions.append(Region(
            name=f"rx_{seed}", seed=seed, members=members,
            center_ll=(round(la, 5), round(lo, 5)),
            epsg=grids.utm_epsg(lo, la),
            quota=8 * len(members), arch_weights=[]))
    return regions


def _f0_extended(regions: list[Region], cfg: dict,
                 radius_km: float) -> pd.DataFrame:
    """Mini-F0 over extended discs (per-region radius override)."""
    import requests
    from . import f0_coarse as F0
    from . import lattice as L
    cfg2 = json.loads(json.dumps(cfg))
    cfg2["regions"]["search_radius_km"] = radius_km
    targets = F0.course_targets(cfg)
    med = targets.median()
    mad = ((targets - med).abs().median() * 1.4826).where(
        lambda s: s > 1e-9, targets.std().fillna(1.0)).fillna(1.0)
    side = cfg["tile"]["side_m"]
    res = cfg["f0"]["res_m"]
    n50 = int(side / res)
    sess = requests.Session()
    frames = []
    for reg in regions:
        try:
            z, window = F0.region_dem(reg, cfg2, sess)
        except Exception as e:  # transient 3DEP outages must not kill the run
            print(f"  {reg.name}: SKIPPED ({type(e).__name__}: {e}) — "
                  f"rerun topup to retry")
            continue
        rx0, ry0, rx1, ry1 = window
        cands = L.candidates(reg, cfg2)
        stats = []
        for row in cands.itertuples():
            c0 = int(round((row.x0 - rx0) / res))
            r0 = int(round((ry1 - (row.y0 + side)) / res))
            if r0 < 0 or c0 < 0 or r0 + n50 > z.shape[0] \
                    or c0 + n50 > z.shape[1]:
                stats.append({k: np.nan for k in F0.F0_STATS}
                             | {"f0_nodata_frac": 1.0})
                continue
            stats.append(F0.f0_stats(z[r0:r0 + n50, c0:c0 + n50], res,
                                     cfg["f0"]["flat_slope"]))
        df = pd.concat([cands.reset_index(drop=True),
                        pd.DataFrame(stats)], axis=1)
        from dtm_atlas import grids
        lon0, lat0 = grids.utm_to_ll(list(df["x0"]), list(df["y0"]), reg.epsg)
        lon1, lat1 = grids.utm_to_ll(list(df["x0"] + side),
                                     list(df["y0"] + side), reg.epsg)
        df["bb_s"] = np.round(np.minimum(lat0, lat1), 6)
        df["bb_n"] = np.round(np.maximum(lat0, lat1), 6)
        df["bb_w"] = np.round(np.minimum(lon0, lon1), 6)
        df["bb_e"] = np.round(np.maximum(lon0, lon1), 6)
        # rank vs the region's STARVED members only
        f0c = cfg["f0"]
        cz = (df[F0.F0_STATS] - med[F0.F0_STATS]) / mad[F0.F0_STATS]
        d = np.full(len(df), np.inf)
        best = np.array([""] * len(df), dtype=object)
        for m in reg.members:
            tz = (targets.loc[m] - med) / mad
            dm = np.sqrt(((cz - tz[F0.F0_STATS]) ** 2).sum(axis=1))
            take = dm.to_numpy() < d
            d[take] = dm.to_numpy()[take]
            best[take] = m
        df["f0_dist"] = np.round(d, 4)
        df["f0_best_course"] = best
        gate = (df["f0_nodata_frac"] <= f0c["max_nodata_frac"]) \
            & (df["f0_flat_frac"] <= f0c["max_flat_frac"]) \
            & np.isfinite(df["f0_dist"])
        for s in F0.ENVELOPE_STATS:
            lo_v = targets.loc[reg.members, s].min() \
                - f0c["z_window_sd"] * mad[s]
            hi_v = targets.loc[reg.members, s].max() \
                + f0c["z_window_sd"] * mad[s]
            gate &= df[s].between(lo_v, hi_v)
        df = df[gate].sort_values(["f0_dist", "key"]) \
            .head(10 * reg.quota)
        df["region"] = reg.name
        frames.append(df)
        print(f"  {reg.name}: {len(df)} extended candidates "
              f"(radius {radius_km:.0f} km)")
    return pd.concat(frames, ignore_index=True) if frames else pd.DataFrame()


def _deepened_pool(cfg: dict, starved: pd.DataFrame) -> pd.DataFrame:
    """Re-mine existing regions beyond the original F0 keep window: regions
    of starved courses + regions with thin clean-tile counts."""
    from . import regions as R
    man = json.load(open(os.path.join(
        sconfig.repo_path(cfg["paths"]["atlas_root"]), "tiles_us.json")))
    sel_by_region: dict[str, int] = {}
    for k, v in man.items():
        if v.get("selected"):
            sel_by_region[v["region"]] = sel_by_region.get(v["region"], 0) + 1
    regs = {r.name: r for r in R.load(sconfig.out_path("regions.json"))}
    starved_regions = set(starved["region"]) \
        | {n for n, r in regs.items()
           if sel_by_region.get(n, 0) < 2}
    cands = pd.read_parquet(sconfig.out_path("f0_candidates.parquet"))
    f1 = pd.read_parquet(sconfig.out_path("f1_survivors.parquet"))
    tried = set(f1["key"])
    parts = []
    for rname in sorted(starved_regions):
        if rname not in regs:
            continue
        sub = cands[(cands["region"] == rname) & cands["f0_pass"]
                    & ~cands["key"].isin(tried)]
        window = cfg["topup"]["deepen_rank_max"] * regs[rname].quota
        parts.append(sub.sort_values(["f0_dist", "key"]).head(window))
    out = pd.concat(parts, ignore_index=True) if parts else pd.DataFrame()
    print(f"  deepened pool: {len(out)} candidates from "
          f"{len(starved_regions)} thin/starved regions")
    return out


def run(cfg: dict) -> int:
    from dtm_atlas import osm
    from . import f1_osm, verify2
    from . import f2_terrain as F2
    from . import manifest as M
    t = cfg["topup"]
    starved = starved_courses(cfg)
    print(f"starved courses ({len(starved)}): "
          f"{list(starved['course'].head(12))}")

    pool_deep = _deepened_pool(cfg, starved)
    ext_regs = extended_regions(starved, cfg, t["radius_ladder_km"][0])
    pool_ext = _f0_extended(ext_regs, cfg, t["radius_ladder_km"][0])
    pool = pd.concat([pool_deep, pool_ext], ignore_index=True) \
        .drop_duplicates("key").sort_values("key").reset_index(drop=True)
    # never re-ingest existing tiles
    man_p = os.path.join(sconfig.repo_path(cfg["paths"]["atlas_root"]),
                         "tiles_us.json")
    man = json.load(open(man_p))
    pool = pool[~pool["key"].isin(man)]
    print(f"pool: {len(pool)} candidates")

    # F1 counts (batched, cached)
    import time
    f1cfg = cfg["f1"]
    pending = [r for r in pool.itertuples()
               if not os.path.exists(
                   sconfig.out_path("cache", "f1", f"{r.key}.json"))]
    print(f"F1 counts: {len(pending)} to fetch")
    skipped = 0
    for i in range(0, len(pending), f1cfg["batch_size"]):
        rows = pending[i:i + f1cfg["batch_size"]]
        try:
            resp = osm.overpass(f1_osm.batch_query(rows))
            parsed = f1_osm.parse_counts(resp, len(rows))
        except Exception as e:
            skipped += len(rows)
            print(f"  batch @{i}: SKIPPED ({type(e).__name__}) — "
                  f"rerun topup to retry")
            continue
        for r, counts in zip(rows, parsed):
            with open(sconfig.out_path("cache", "f1", f"{r.key}.json"),
                      "w") as f:
                json.dump({"query_sha": "topup", "counts": counts}, f)
        if (i // f1cfg["batch_size"]) % 10 == 0:
            print(f"  ... {i + len(rows)}/{len(pending)}")
        time.sleep(f1cfg["courtesy_sleep_s"])
    if skipped:
        print(f"F1: {skipped} candidates skipped on fetch errors")
    keep1 = []
    for r in pool.itertuples():
        cp = sconfig.out_path("cache", "f1", f"{r.key}.json")
        if not os.path.exists(cp):
            continue
        with open(cp) as f:
            counts = json.load(f)["counts"]
        if f1_osm.gates(counts, f1cfg) \
                or f1_osm.gates_relaxed(counts, f1cfg):
            keep1.append(r.key)
    pool1 = pool[pool["key"].isin(keep1)]
    print(f"F1 pass: {len(pool1)}")

    # verify2 geometry gates + protected preference
    meas_rows = []
    for r in pool1.itertuples():
        try:
            m = verify2.measure(r, cfg)
        except Exception as e:
            print(f"  verify2 {r.key}: SKIPPED ({type(e).__name__})")
            continue
        if verify2.gates(m, cfg):
            meas_rows.append({"key": r.key, **m})
    v2 = pd.DataFrame(meas_rows)
    pool2 = pool1[pool1["key"].isin(v2["key"] if len(v2) else [])]
    print(f"verify2 pass: {len(pool2)}")
    if pool2.empty:
        print("no candidates survived — consider the next radius rung")
        return 1

    # F2 fine match
    import requests
    sess = requests.Session()
    targets = F2.course_targets(cfg)
    tz = F2._zframe(cfg, targets, weighted=True).fillna(0.0)
    from . import scoring
    L = scoring.whitener(tz.to_numpy())
    recs = []
    for row in pool2.itertuples():
        try:
            z10 = F2.fetch10(row, cfg, sess)
            vec = F2.s1_vector(z10, cfg["f2"]["res_m"])
        except Exception as e:
            print(f"  f2 {row.key}: SKIPPED ({type(e).__name__})")
            continue
        recs.append({"key": row.key, **vec})
    pool2 = pool2[pool2["key"].isin([r["key"] for r in recs])]
    vz = F2._zframe(cfg, pd.DataFrame(recs).set_index("key"),
                    weighted=True).fillna(0.0)
    dists = scoring.pairwise_dist(L, tz.to_numpy(), vz.to_numpy())
    best_j = dists.argmin(axis=0)
    out = pool2.reset_index(drop=True).copy()
    out["f2_dist"] = np.round(dists.min(axis=0), 4)
    out["f2_best_course"] = [targets.index[j] for j in best_j]
    out = out.merge(v2, on="key")
    out["score"] = out["f2_dist"] - t["protected_bonus"] \
        * out["protected_frac"]

    # targeted pick: starved courses first, then global best; empirical 1m
    picked: list = []
    starved_set = list(starved["course"])
    per_course = max(2, t["n_new_max"] // max(len(starved_set), 1))
    for course in starved_set:
        subs = out[(out["f2_best_course"] == course)
                   & ~out["key"].isin([p.key for p in picked])] \
            .sort_values(["score", "key"]).head(per_course)
        picked += list(subs.itertuples())
    rest = out[~out["key"].isin([p.key for p in picked])] \
        .sort_values(["score", "key"])
    for r in rest.itertuples():
        if len(picked) >= t["n_new_max"]:
            break
        picked.append(r)
    picked = picked[:t["n_new_max"]]
    print(f"targeted pick: {len(picked)} candidates -> 1 m probes")
    final = []
    for r in picked:
        try:
            if M._avail_1m(r, sess):
                final.append(r)
        except Exception as e:
            print(f"  probe {r.key}: SKIPPED ({type(e).__name__})")
    print(f"1 m confirmed: {len(final)}")

    # append to the tile index (flagged topup)
    for r in final:
        man[r.key] = {
            "center_ll": [round(float(r.lat), 6), round(float(r.lon), 6)],
            "side_m": cfg["tile"]["side_m"],
            "label": f"topup analog of {r.f2_best_course}",
            "region": r.region,
            "best_course": r.f2_best_course,
            "f2_dist": round(float(r.f2_dist), 4),
            "protected_frac": float(r.protected_frac),
            "topup": True,
            "selected": False,
            "tile_scout": TILE_SCOUT_VERSION,
            "cfg_hash": sconfig.cfg_hash(cfg),
        }
    with open(man_p, "w") as f:
        json.dump(man, f, indent=1, sort_keys=True)
        f.write("\n")
    txt_p = os.path.join(sconfig.repo_path(cfg["paths"]["atlas_root"]),
                         "tiles_us.txt")
    with open(txt_p, "w") as f:
        f.write("# Stage-0T natural-tile analogs — generated by tile_scout"
                " (finalize + topup).\n# One key per line; details in"
                " tiles_us.json.\n")
        for k in sorted(man):
            f.write(k + "\n")
    print(f"index: {len(man)} tiles ({len(final)} new topup)")
    return 0
