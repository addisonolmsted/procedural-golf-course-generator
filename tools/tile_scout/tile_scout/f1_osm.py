"""F1 — OSM development screen. Batched Overpass count queries (8 tiles per
request, 7 counted groups per tile), cache-first and resumable; gates are
re-evaluated offline from cached counts so threshold tuning never re-fetches."""

from __future__ import annotations

import hashlib
import json
import os
import time

import pandas as pd

from . import sconfig

GROUPS = ["bld", "big", "rail", "res", "loc", "dev", "agr"]

_TILE_QL = """way["building"]({s},{w},{n},{e});out count;
way["highway"~"^(motorway|trunk|primary|secondary)"]({s},{w},{n},{e});out count;
way["railway"="rail"]({s},{w},{n},{e});out count;
way["highway"="residential"]({s},{w},{n},{e});out count;
way["highway"~"^(unclassified|tertiary)$"]({s},{w},{n},{e});out count;
way["landuse"~"^(residential|commercial|industrial|retail|quarry|landfill|military)$"]({s},{w},{n},{e});out count;
way["landuse"~"^(farmland|orchard|vineyard|farmyard)$"]({s},{w},{n},{e});out count;
"""


def batch_query(rows: list) -> str:
    parts = ["[out:json][timeout:180];"]
    for r in rows:
        parts.append(_TILE_QL.format(s=r.bb_s, w=r.bb_w, n=r.bb_n, e=r.bb_e))
    return "\n".join(parts)


def parse_counts(resp: dict, n_tiles: int) -> list[dict]:
    counts = [e for e in resp.get("elements", [])
              if e.get("type") == "count"]
    if len(counts) != len(GROUPS) * n_tiles:
        raise ValueError(f"expected {len(GROUPS) * n_tiles} counts, "
                         f"got {len(counts)}")
    out = []
    for i in range(n_tiles):
        chunk = counts[i * len(GROUPS):(i + 1) * len(GROUPS)]
        out.append({g: int(c["tags"]["total"])
                    for g, c in zip(GROUPS, chunk)})
    return out


def gates(counts: dict, f1: dict) -> bool:
    return (counts["bld"] <= f1["max_buildings"]
            and counts["big"] + counts["rail"] <= f1["max_big_road_rail"]
            and counts["res"] <= f1["max_residential"]
            and counts["loc"] <= f1["max_local_roads"]
            and counts["dev"] <= f1["max_dev_landuse"]
            and counts["agr"] <= f1["max_agriculture"])


def _cache_path(key: str) -> str:
    return sconfig.out_path("cache", "f1", f"{key}.json")


def run(cfg: dict, only_region: str = "", force: bool = False) -> int:
    from dtm_atlas import osm
    f1 = cfg["f1"]
    cands = pd.read_parquet(sconfig.out_path("f0_candidates.parquet"))
    todo = cands[cands["f0_keep"]]
    if only_region:
        todo = todo[todo["region"] == only_region]
    todo = todo.sort_values("key")

    fetched = 0
    pending = []
    for row in todo.itertuples():
        cp = _cache_path(row.key)
        if os.path.exists(cp) and not force:
            continue
        pending.append(row)
    print(f"f1: {len(todo)} kept candidates, {len(pending)} to fetch")

    for i in range(0, len(pending), f1["batch_size"]):
        rows = pending[i:i + f1["batch_size"]]
        q = batch_query(rows)
        qh = hashlib.sha256(_TILE_QL.encode()).hexdigest()[:12]
        resp = osm.overpass(q)
        parsed = parse_counts(resp, len(rows))
        for r, counts in zip(rows, parsed):
            with open(_cache_path(r.key), "w") as f:
                json.dump({"query_sha": qh, "counts": counts},
                          f, sort_keys=True)
        fetched += len(rows)
        if fetched % 80 == 0:
            print(f"  ... {fetched}/{len(pending)}")
        time.sleep(f1["courtesy_sleep_s"])

    # offline gate evaluation over ALL kept candidates
    recs = []
    for row in todo.itertuples():
        cp = _cache_path(row.key)
        if not os.path.exists(cp):
            continue
        with open(cp) as f:
            counts = json.load(f)["counts"]
        recs.append({"key": row.key, **{f"f1_{g}": counts[g] for g in GROUPS},
                     "f1_pass": gates(counts, f1),
                     "f1_penalty": f1["agri_penalty"] * counts["agr"]})
    out = pd.DataFrame(recs).sort_values("key").reset_index(drop=True)
    out_p = sconfig.out_path("f1_survivors.parquet")
    if only_region and os.path.exists(out_p):
        prev = pd.read_parquet(out_p)
        prev = prev[~prev["key"].isin(out["key"])]
        out = pd.concat([prev, out], ignore_index=True) \
            .sort_values("key").reset_index(drop=True)
    out.to_parquet(out_p)
    n_pass = int(out["f1_pass"].sum())
    print(f"wrote {out_p}: {n_pass}/{len(out)} pass the development gates")
    return 0
