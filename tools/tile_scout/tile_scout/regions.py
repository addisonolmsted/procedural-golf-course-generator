"""Search regions: greedy disc cover over course centers (single-linkage
chains the whole BosWash corridor; disc cover keeps regions compact), one
region per seed course, quotas proportional to member counts."""

from __future__ import annotations

import json
import os
from dataclasses import dataclass, field

import numpy as np

from . import sconfig

EARTH_KM = 6371.0


def haversine_km(lat0, lon0, lat1, lon1) -> float:
    p0, p1 = np.radians(lat0), np.radians(lat1)
    dp = p1 - p0
    dl = np.radians(lon1) - np.radians(lon0)
    a = np.sin(dp / 2) ** 2 + np.cos(p0) * np.cos(p1) * np.sin(dl / 2) ** 2
    return float(2 * EARTH_KM * np.arcsin(np.sqrt(a)))


@dataclass
class Region:
    name: str
    seed: str
    members: list[str]
    center_ll: tuple[float, float]
    epsg: int
    quota: int
    arch_weights: list[float] = field(default_factory=list)


def course_centers(cfg: dict) -> dict[str, tuple[float, float]]:
    """Course centroids from the parkland cache bboxes, restricted to the
    keys present in the metrics parquet (the actual matching targets)."""
    import pandas as pd
    keys = list(pd.read_parquet(
        sconfig.repo_path(cfg["paths"]["course_metrics_parquet"])).index)
    cache = sconfig.repo_path(cfg["paths"]["parkland_cache"])
    out = {}
    for k in keys:
        with open(os.path.join(cache, f"{k}.json")) as f:
            rec = json.load(f)
        la0, lo0, la1, lo1 = rec["bbox"]
        out[k] = ((la0 + la1) / 2.0, (lo0 + lo1) / 2.0)
    return out


def build(centers: dict, resp: np.ndarray, keys: list[str],
          cfg: dict) -> list[Region]:
    rcfg = cfg["regions"]
    radius = rcfg["group_radius_km"]
    uncovered = set(centers)
    regions: list[Region] = []
    key_idx = {k: i for i, k in enumerate(keys)}

    def covers(seed: str) -> list[str]:
        la, lo = centers[seed]
        return sorted(k for k in uncovered
                      if haversine_km(la, lo, *centers[k]) <= radius)

    while uncovered:
        # the course covering the most uncovered courses; ties lexicographic
        seed = max(sorted(uncovered), key=lambda k: len(covers(k)))
        members = covers(seed)
        uncovered -= set(members)
        la = float(np.mean([centers[m][0] for m in members]))
        lo = float(np.mean([centers[m][1] for m in members]))
        from dtm_atlas import grids
        quota = max(rcfg["min_quota"],
                    int(np.ceil(rcfg["tiles_per_course"] * len(members))))
        aw = np.mean([resp[key_idx[m]] for m in members], axis=0) \
            if len(resp) else np.array([])
        regions.append(Region(
            name=f"r_{seed}", seed=seed, members=members,
            center_ll=(round(la, 5), round(lo, 5)),
            epsg=grids.utm_epsg(lo, la), quota=quota,
            arch_weights=[round(float(v), 4) for v in aw]))
    regions.sort(key=lambda r: r.name)
    return regions


def save(regions: list[Region], path: str) -> None:
    sconfig.save_json(path, {
        r.name: {"seed": r.seed, "members": r.members,
                 "center_ll": list(r.center_ll), "epsg": r.epsg,
                 "quota": r.quota, "arch_weights": r.arch_weights}
        for r in regions})


def load(path: str) -> list[Region]:
    with open(path) as f:
        d = json.load(f)
    return [Region(name=n, seed=v["seed"], members=v["members"],
                   center_ll=tuple(v["center_ll"]), epsg=v["epsg"],
                   quota=v["quota"], arch_weights=v["arch_weights"])
            for n, v in sorted(d.items())]


def run(cfg: dict) -> int:
    from . import archetypes
    a = archetypes.load(sconfig.out_path("archetypes.json"))
    centers = course_centers(cfg)
    regions = build(centers, a.resp, a.keys, cfg)
    out = sconfig.out_path("regions.json")
    save(regions, out)
    total_quota = sum(r.quota for r in regions)
    print(f"{len(regions)} regions, total quota {total_quota}")
    for r in sorted(regions, key=lambda r: -len(r.members))[:8]:
        print(f"  {r.name:24s} members {len(r.members):3d} quota {r.quota}")
    print(f"wrote {out}")
    return 0
