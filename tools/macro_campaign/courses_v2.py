"""D6-W ground truth: what windows do REAL courses in the v2 provinces sit
on?

Finds golf courses (OSM) in a search box around each v2 biome's exemplar
province, fetches the campaign-standard 3 km / 2 m 3DEP tile centred on
each course, and measures the course's OWN 600 m window with the same
metrics `site_window_audit.py` uses on random land and generated seeds:
calm fraction (slope <= 8%, the fairway grade target), window relief, and
terrain position (window mean elevation as a percentile of the tile's
p5-p95 range: 0 = valley floor, 1 = hilltop). Also records the tile's
best-anywhere window — what the architect had available locally.

Caches under out/courses_v2/<biome>/. Real courses are graded ground, so
these tiles NEVER feed texture harvesting — measurement only.
"""
import json
import pathlib
import sys

import numpy as np

_ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(_ROOT))
sys.path.insert(0, str(_ROOT.parent / "dtm_atlas"))

from dtm_atlas import osm  # noqa: E402
from macro_campaign import cgrid, fetch  # noqa: E402

OUT = _ROOT / "out"
COURSES = OUT / "courses_v2"
MIN_COURSE_AREA_M2 = 2.0e5

# Non-overlapping boxes around each v2 biome's exemplar province. Courses
# sit on private land NEAR the protected exemplar regions, so each box is
# the surrounding province, not the refuge itself.
SEARCH_BOXES = {
    "piedmont": (35.35, -80.35, 0.65),      # Uwharrie piedmont, W of the NC sandhills
    "sandhills": (42.20, -100.90, 1.30),    # Nebraska Sandhills (Mullen/Valentine)
    "great_plains": (36.35, -102.90, 1.20), # Kiowa/Rita Blanca shortgrass plains
    "river_valley": (32.70, -91.60, 0.90),  # Tensas/Ouachita bottomland (Monroe LA)
    "hill_country": (36.40, -92.20, 0.90),  # Ozark uplands (Mountain Home AR)
    "heathland": (44.40, -85.60, 0.85),     # Michigan kettled outwash (Cadillac belt)
}

W_M, TILE_M = 600.0, 3000.0


def _assert_boxes_disjoint():
    items = sorted(SEARCH_BOXES.items())
    for i, (a, (la, lo_a, ha)) in enumerate(items):
        for b, (lb, lo_b, hb) in items[i + 1:]:
            if abs(la - lb) < ha + hb and abs(lo_a - lo_b) < ha + hb:
                raise SystemExit(f"SEARCH_BOXES overlap: {a} / {b}")


def find_courses(biome, limit=10):
    lat, lon, half = SEARCH_BOXES[biome]
    bbox = (lat - half, lon - half, lat + half, lon + half)
    query = (
        "[out:json][timeout:120];("
        f'way["leisure"="golf_course"]({bbox[0]},{bbox[1]},{bbox[2]},{bbox[3]});'
        f'relation["leisure"="golf_course"]({bbox[0]},{bbox[1]},{bbox[2]},{bbox[3]});'
        ");out geom;"
    )
    out = []
    for el in osm.overpass(query).get("elements", []):
        polys = osm.element_polygons(el)
        if not polys:
            continue
        ring = polys[0]["coordinates"][0]
        xs = [c[0] for c in ring]
        ys = [c[1] for c in ring]
        clat, clon = float(np.mean(ys)), float(np.mean(xs))
        w = (max(xs) - min(xs)) * 111_320.0 * np.cos(np.radians(clat))
        h = (max(ys) - min(ys)) * 110_540.0
        if w * h < MIN_COURSE_AREA_M2:
            continue
        out.append({"name": (el.get("tags") or {}).get("name", f"osm{el.get('id')}"),
                    "lat": clat, "lon": clon, "approx_area_m2": float(w * h)})
    out.sort(key=lambda c: (-c["approx_area_m2"], c["name"]))
    return out[:limit]


def window_metrics(z8, y, x, w, zlo, zhi):
    win = z8[y:y + w, x:x + w]
    gy, gx = np.gradient(win, 8.0)
    calm = float((np.hypot(gx, gy) <= 0.08).mean())
    rel = float(np.percentile(win, 95) - np.percentile(win, 5))
    pos = float((win.mean() - zlo) / max(zhi - zlo, 1e-9))
    return calm, rel, pos


def measure(biome, c):
    dest = COURSES / biome
    dest.mkdir(parents=True, exist_ok=True)
    key = f"{round(c['lat']*1000):06d}_{round(-c['lon']*1000):06d}"
    tile = dest / f"{key}.cgrid"
    if not tile.exists():
        zone = int((c["lon"] + 180.0) / 6.0) + 1
        try:
            fetch.fetch_tile(c["lat"], c["lon"], 26900 + zone, zone, tile)
        except Exception as exc:
            print(f"    [fail] {c['name']}: {type(exc).__name__}: {exc}", flush=True)
            return None
    z, (_, _, cell) = cgrid.read_f32(tile)
    z = z.astype(np.float64)
    m = np.isfinite(z)
    if m.mean() < 0.9:
        return None
    z = np.where(m, z, np.nanmean(z[m]))
    z8 = z[::4, ::4]
    n = z8.shape[0]
    w = int(W_M / 8.0)
    zlo, zhi = np.percentile(z8, 5), np.percentile(z8, 95)
    # the COURSE window: centred on the centroid (tile centre)
    c0 = (n - w) // 2
    calm, rel, pos = window_metrics(z8, c0, c0, w, zlo, zhi)
    # the best window ANYWHERE in the tile — the architect's local menu
    best = (0.0, 0.0, 0.0)
    for y in range(0, n - w + 1, 12):
        for x in range(0, n - w + 1, 12):
            mm = window_metrics(z8, y, x, w, zlo, zhi)
            if mm[0] > best[0]:
                best = mm
    return {"name": c["name"], "lat": c["lat"], "lon": c["lon"],
            "course_calm": calm, "course_relief_m": rel, "course_pos": pos,
            "best_calm": best[0], "best_pos": best[2]}


def main():
    _assert_boxes_disjoint()
    results = {}
    for biome in SEARCH_BOXES:
        print(f"[{biome}]", flush=True)
        rows = []
        for c in find_courses(biome):
            r = measure(biome, c)
            if r:
                print(f"    {r['name'][:34]:34s} calm {r['course_calm']:.2f} "
                      f"relief {r['course_relief_m']:5.1f} m pos {r['course_pos']:.2f}",
                      flush=True)
                rows.append(r)
        results[biome] = rows
    (COURSES / "summary.json").write_text(json.dumps(results, indent=1))
    print("\nsummary ->", COURSES / "summary.json")


if __name__ == "__main__":
    main()
