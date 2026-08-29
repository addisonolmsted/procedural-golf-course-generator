"""Fetch OSM golf=green centroids for the cached real-course tiles, projected
into each tile's local metres. Writes real_greens.json."""
import sys, json, pathlib, math
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
for p in ("macro_campaign", "dtm_atlas"):
    sys.path.insert(0, str(ROOT / "tools" / p))
from macro_campaign.utm import to_utm
from macro_campaign.fetch import TILE_M, CELL_M
from dtm_atlas import osm

OUT = ROOT / "tools/macro_campaign/out"
SETS = {
    "nebraska": (OUT / "courses_v2/sandhills", json.load(open(OUT / "courses_v2/summary.json"))["sandhills"]),
    "carolina": (OUT / "courses", json.load(open(OUT / "courses/sandhills.json"))),
}
res = {}
for region, (tiledir, courses) in SETS.items():
    if region == "carolina":
        tiledir = OUT / "courses/sandhills"
    for c in courses:
        key = f"{round(c['lat']*1000):06d}_{round(-c['lon']*1000):06d}"
        tile = tiledir / f"{key}.cgrid"
        if not tile.exists():
            continue
        zone = int((c["lon"] + 180.0) / 6.0) + 1
        e, n = to_utm(c["lat"], c["lon"], zone)
        e0 = round((e - TILE_M / 2) / CELL_M) * CELL_M
        n0 = round((n - TILE_M / 2) / CELL_M) * CELL_M
        half_deg = 0.02
        q = ("[out:json][timeout:90];("
             f'way["golf"="green"]({c["lat"]-half_deg},{c["lon"]-half_deg},'
             f'{c["lat"]+half_deg},{c["lon"]+half_deg});'
             ");out geom;")
        try:
            els = osm.overpass(q).get("elements", [])
        except Exception as exc:
            print(f"  [fail] {c['name']}: {exc}")
            continue
        pts = []
        for el in els:
            g = el.get("geometry") or []
            if not g:
                continue
            la = sum(p["lat"] for p in g) / len(g)
            lo = sum(p["lon"] for p in g) / len(g)
            ee, nn = to_utm(la, lo, zone)
            x, y = ee - e0, nn - n0
            if 0 <= x <= TILE_M and 0 <= y <= TILE_M:
                pts.append([round(y, 1), round(x, 1)])
        res.setdefault(region, []).append(
            dict(name=c["name"], key=key, tile=str(tile), greens=pts))
        print(f"  {region} {c['name'][:34]:36s} {len(pts):3d} greens")
import os
json.dump(res, open(os.environ["SCRATCH"] + "/real_greens.json", "w"), indent=1)
