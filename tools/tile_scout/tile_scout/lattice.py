"""Candidate tile lattice: axis-aligned 3 km tiles at half-overlap spacing in
each region's UTM zone, deduplicated across overlapping regions."""

from __future__ import annotations

import numpy as np
import pandas as pd

from .regions import Region, haversine_km


def tile_key(epsg: int, x0: float, y0: float) -> str:
    return f"t{epsg}_{int(x0 // 1000):05d}_{int(y0 // 1000):05d}"


def candidates(region: Region, cfg: dict) -> pd.DataFrame:
    """Lattice of tile WINDOW ORIGINS (x0, y0) whose centers lie within the
    search radius of the region center."""
    from dtm_atlas import grids
    side = cfg["tile"]["side_m"]
    step = cfg["lattice"]["spacing_m"]
    radius_m = cfg["regions"]["search_radius_km"] * 1000.0
    la, lo = region.center_ll
    xs, ys = grids.ll_to_utm([lo], [la], region.epsg)
    xc, yc = float(np.asarray(xs)[0]), float(np.asarray(ys)[0])
    # snap the lattice origin so every tile origin is SNAP-aligned
    n = int(np.ceil(radius_m / step))
    off = np.arange(-n, n + 1) * step
    gx, gy = np.meshgrid(xc + off, yc + off)
    cx, cy = gx.ravel(), gy.ravel()
    keep = np.hypot(cx - xc, cy - yc) <= radius_m
    cx, cy = cx[keep], cy[keep]
    x0 = np.round((cx - side / 2.0) / 2.0) * 2.0   # SNAP_M = 2
    y0 = np.round((cy - side / 2.0) / 2.0) * 2.0
    lon, lat = grids.utm_to_ll(list(x0 + side / 2.0), list(y0 + side / 2.0),
                               region.epsg)
    df = pd.DataFrame({
        "key": [tile_key(region.epsg, a, b) for a, b in zip(x0, y0)],
        "region": region.name,
        "epsg": region.epsg,
        "x0": x0, "y0": y0,
        "lat": np.round(lat, 6), "lon": np.round(lon, 6),
    })
    df = df.drop_duplicates("key").sort_values("key").reset_index(drop=True)
    return df


def dedup(frames: list[pd.DataFrame], regions: list[Region],
          cfg: dict) -> pd.DataFrame:
    """Cross-region dedup: tiles whose centers sit closer than dedup_min_km
    keep only the copy whose region center is nearer."""
    all_df = pd.concat(frames, ignore_index=True)
    rc = {r.name: r.center_ll for r in regions}
    d_region = np.array([
        haversine_km(row.lat, row.lon, *rc[row.region])
        for row in all_df.itertuples()])
    all_df = all_df.assign(_dr=d_region)
    # approximate planar coords for neighbor search
    lat0 = float(all_df["lat"].mean())
    kx = 111.32 * np.cos(np.radians(lat0))
    P = np.stack([all_df["lon"].to_numpy() * kx,
                  all_df["lat"].to_numpy() * 111.32], axis=1)
    from scipy.spatial import cKDTree
    tree = cKDTree(P)
    pairs = tree.query_pairs(cfg["lattice"]["dedup_min_km"])
    drop = set()
    for i, j in sorted(pairs):
        if i in drop or j in drop:
            continue
        a, b = all_df.iloc[i], all_df.iloc[j]
        if a.region == b.region:
            continue  # same-region overlap is the deliberate half-overlap
        loser = i if (a._dr, a.key) > (b._dr, b.key) else j
        drop.add(loser)
    out = all_df.drop(index=sorted(drop)).drop(columns="_dr")
    return out.sort_values("key").reset_index(drop=True)
