"""Screener v2 — geometry-level development verification. The v1 screen
counted OSM ways (a warehouse = 1 way) and could not see unmapped area
context; v2 fetches GEOMETRY per candidate and gates on:
  - total building FOOTPRINT AREA (m2)
  - total PAVED-ROAD length (motorway..residential+service+unclassified;
    track/path/footway are welcome — that's hiking country)
and scores protected-lands overlap (nature reserves, protected areas,
national/state forest boundaries) as a strong preference. One Overpass
geometry query per candidate, cached + resumable."""

from __future__ import annotations

import json
import os
import time

import numpy as np

from . import sconfig

PAVED = ("motorway", "trunk", "primary", "secondary", "tertiary",
         "residential", "unclassified", "service", "living_street")

_Q = """[out:json][timeout:90];
(
  way["building"]({s},{w},{n},{e});
  way["highway"~"^({paved})$"]({s},{w},{n},{e});
  way["boundary"~"^(protected_area|national_park)$"]({s},{w},{n},{e});
  way["leisure"="nature_reserve"]({s},{w},{n},{e});
  relation["boundary"~"^(protected_area|national_park)$"]({s},{w},{n},{e});
  relation["leisure"="nature_reserve"]({s},{w},{n},{e});
);
out geom;"""


def _poly_area_m2(coords_ll: list, lat0: float) -> float:
    """Shoelace on lon/lat with local metric scaling."""
    kx = 111320.0 * np.cos(np.radians(lat0))
    ky = 111320.0
    xs = np.array([c[0] for c in coords_ll]) * kx
    ys = np.array([c[1] for c in coords_ll]) * ky
    return float(abs(np.dot(xs, np.roll(ys, 1))
                     - np.dot(ys, np.roll(xs, 1))) / 2.0)


def _line_len_m(coords_ll: list, lat0: float) -> float:
    kx = 111320.0 * np.cos(np.radians(lat0))
    ky = 111320.0
    xs = np.array([c[0] for c in coords_ll]) * kx
    ys = np.array([c[1] for c in coords_ll]) * ky
    return float(np.hypot(np.diff(xs), np.diff(ys)).sum())


def _point_in_ring(px: float, py: float, ring: list) -> bool:
    inside = False
    n = len(ring)
    j = n - 1
    for i in range(n):
        xi, yi = ring[i][0], ring[i][1]
        xj, yj = ring[j][0], ring[j][1]
        if (yi > py) != (yj > py) \
                and px < (xj - xi) * (py - yi) / (yj - yi + 1e-30) + xi:
            inside = not inside
        j = i
    return inside


def measure(row, cfg: dict) -> dict:
    """Geometry measurements for one candidate tile (bb_* lon/lat bbox)."""
    from dtm_atlas import osm
    cache = sconfig.out_path("cache", "v2", f"{row.key}.json")
    if os.path.exists(cache):
        with open(cache) as f:
            return json.load(f)
    q = _Q.format(s=row.bb_s, w=row.bb_w, n=row.bb_n, e=row.bb_e,
                  paved="|".join(PAVED))
    resp = osm.overpass(q)
    lat0 = (row.bb_s + row.bb_n) / 2.0
    bld_area = 0.0
    paved_len = 0.0
    protected_rings: list[list] = []
    for el in resp.get("elements", []):
        tags = el.get("tags", {})
        geom = el.get("geometry") or []
        coords = [(g["lon"], g["lat"]) for g in geom]
        if el.get("type") == "way" and len(coords) >= 2:
            if "building" in tags and len(coords) >= 4:
                bld_area += _poly_area_m2(coords, lat0)
            elif tags.get("highway") in PAVED:
                paved_len += _line_len_m(coords, lat0)
            elif (tags.get("boundary") in ("protected_area", "national_park")
                  or tags.get("leisure") == "nature_reserve") \
                    and len(coords) >= 4:
                protected_rings.append(coords)
        elif el.get("type") == "relation":
            for m in el.get("members", []):
                g = m.get("geometry") or []
                if m.get("role") == "outer" and len(g) >= 4:
                    protected_rings.append([(p["lon"], p["lat"]) for p in g])
    # protected fraction via a 5x5 sample-point grid
    prot = 0
    pts = 0
    for fy in np.linspace(0.1, 0.9, 5):
        for fx in np.linspace(0.1, 0.9, 5):
            px = row.bb_w + fx * (row.bb_e - row.bb_w)
            py = row.bb_s + fy * (row.bb_n - row.bb_s)
            pts += 1
            if any(_point_in_ring(px, py, ring)
                   for ring in protected_rings):
                prot += 1
    out = {"bld_area_m2": round(bld_area, 1),
           "paved_len_m": round(paved_len, 1),
           "protected_frac": round(prot / max(pts, 1), 2)}
    with open(cache, "w") as f:
        json.dump(out, f, sort_keys=True)
    time.sleep(cfg["topup"]["courtesy_sleep_s"])
    return out


def gates(meas: dict, cfg: dict) -> bool:
    t = cfg["topup"]
    return (meas["bld_area_m2"] <= t["max_building_area_m2"]
            and meas["paved_len_m"] <= t["max_paved_len_m"])
