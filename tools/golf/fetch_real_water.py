#!/usr/bin/env python3
"""Fetch OSM water polygons for the cached real-course tiles and rasterise
them into tile-local masks.

Why this exists: 3DEP bare-earth DEMs carry no water mask, so the real-tile
study ran the generator with `wet = zeros` -- every water gate disabled.
Reviewer caught the consequence on Bayside Golf Club (Lake McConaughy),
where the tile is 24.8% water by flat-plane detection and the generator was
free to propose greens on the lake surface.

Writes `<tile>.water.npy` (bool, same shape as the tile) beside each cgrid.

    python3 tools/golf/fetch_real_water.py
"""
import json
import os
import pathlib
import sys

import numpy as np
from PIL import Image, ImageDraw

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
for p in ("macro_campaign", "dtm_atlas"):
    sys.path.insert(0, str(ROOT / "tools" / p))
from macro_campaign.utm import to_utm                 # noqa: E402
from macro_campaign.fetch import TILE_M, CELL_M       # noqa: E402
from macro_campaign import cgrid                      # noqa: E402
from dtm_atlas import osm                             # noqa: E402

OUT = ROOT / "tools/macro_campaign/out"
# `natural=water` covers lakes/ponds; `waterway=riverbank` the wide rivers;
# reservoirs are tagged either way depending on the mapper.
QUERY = ('[out:json][timeout:120];('
         'way["natural"="water"]({b});relation["natural"="water"]({b});'
         'way["waterway"="riverbank"]({b});relation["waterway"="riverbank"]({b});'
         'way["landuse"="reservoir"]({b});relation["landuse"="reservoir"]({b});'
         ');out geom;')


def tile_water(lat, lon, tile_path, shape, cell):
    zone = int((lon + 180.0) / 6.0) + 1
    e, n = to_utm(lat, lon, zone)
    e0 = round((e - TILE_M / 2) / CELL_M) * CELL_M
    n0 = round((n - TILE_M / 2) / CELL_M) * CELL_M
    half_deg = 0.022
    bbox = f"{lat-half_deg},{lon-half_deg},{lat+half_deg},{lon+half_deg}"
    els = osm.overpass(QUERY.format(b=bbox)).get("elements", [])
    im = Image.new("1", (shape[1], shape[0]), 0)
    dr = ImageDraw.Draw(im)
    n_poly = 0
    for el in els:
        for poly in osm.element_polygons(el):
            rings = poly["coordinates"]
            for ri, ring in enumerate(rings):
                pts = []
                for (lo, la) in ring:
                    ee, nn = to_utm(la, lo, zone)
                    x, y = (ee - e0) / cell, (nn - n0) / cell
                    pts.append((x, y))          # row 0 = SOUTH, matches cgrid
                if len(pts) >= 3:
                    dr.polygon(pts, fill=(0 if ri else 1))
                    n_poly += ri == 0
    return np.array(im, bool), n_poly


def flat_water(z, cell):
    """Lakes are PLANES in a bare-earth DEM, and they sit in depressions.

    OSM alone misses the big ones -- Lake McConaughy at Bayside is a relation
    whose member ways fall outside any sane bbox, so the query returned 0%
    where the lake covers a quarter of the tile. Flatness alone would flag
    genuinely flat LAND (a graded fairway, a plains tile), so require both:
    near-zero local relief over a 2 ha+ body, AND that body sitting below the
    ring of ground around it.
    """
    from scipy import ndimage
    mx = ndimage.maximum_filter(z, size=9)
    mn = ndimage.minimum_filter(z, size=9)
    flat = ndimage.binary_opening((mx - mn) < 0.25, np.ones((5, 5), bool))
    lab, n = ndimage.label(flat)
    out = np.zeros(z.shape, bool)
    for k in range(1, n + 1):
        m = lab == k
        if m.sum() * cell * cell < 2e4:          # < 2 ha is not a lake
            continue
        ring = ndimage.binary_dilation(m, iterations=int(30.0 / cell)) & ~m
        if not ring.any():
            continue
        # a lake is BELOW its shore; a flat upland is not
        if np.median(z[ring]) - np.median(z[m]) > 0.4:
            out |= m
    return out


def main():
    sets = [("nebraska", OUT / "courses_v2/sandhills",
             json.load(open(OUT / "courses_v2/summary.json"))["sandhills"]),
            ("carolina", OUT / "courses/sandhills",
             json.load(open(OUT / "courses/sandhills.json")))]
    for region, tiledir, courses in sets:
        for c in courses:
            key = f"{round(c['lat']*1000):06d}_{round(-c['lon']*1000):06d}"
            tile = tiledir / f"{key}.cgrid"
            if not tile.exists():
                continue
            dest = tile.with_suffix(".water.npy")
            if dest.exists():
                continue
            z, (_, _, cell) = cgrid.read_f32(tile)
            try:
                mask, n_poly = tile_water(c["lat"], c["lon"], tile, z.shape, cell)
            except Exception as exc:
                print(f"  [fail] {c['name']}: {exc}")
                continue
            z = np.where(np.isfinite(z), z, np.nanmean(z)).astype(np.float64)
            fw = flat_water(z, cell)
            union = mask | fw
            np.save(dest, union)
            print(f"  {region[:2]} {c['name'][:32]:34s} {n_poly:3d} polys  "
                  f"osm {mask.mean()*100:5.1f}%  flat {fw.mean()*100:5.1f}%  "
                  f"-> {union.mean()*100:5.1f}%")


if __name__ == "__main__":
    main()
