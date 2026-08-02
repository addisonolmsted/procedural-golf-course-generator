"""What relief does a REAL golf course actually sit on?

`landform.core_relief_cap_m` bounds the relief step 03 may leave inside the
1.5 km routable core, and it has always been an invented number (3 m
florida, 25 m piedmont, 40 m mountain). That matters more than it looks:
the cap is the one thing that decides how dissected a generated course is
allowed to be, and real piedmont landscape carries ~52 m in the same window
— so an invented cap, not the terrain model, could be what makes generated
cores read flat.

Real courses settle it empirically. A built course is routable by
definition, so the relief its architects accepted IS the constraint. This
module finds golf courses via OSM, fetches the 3DEP surface over each, and
reports the relief distribution over a core-sized window.

It reuses the campaign's own fetch and Overpass paths, so the measurement
is the same instrument used everywhere else. Results cache under
`out/courses/`; `out/report/course_relief.md` is the deliverable.
"""

import json
import pathlib
import sys

import numpy as np

_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_ROOT / "dtm_atlas"))

from dtm_atlas import osm  # noqa: E402

from . import cgrid, fetch  # noqa: E402
from .regions import CELL_M  # noqa: E402

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"
COURSES = OUT / "courses"

# Fetch the campaign's standard 3 km tile and measure the central 1.5 km,
# so the window is byte-for-byte the same geometry `structure.py` uses for
# `core_relief_m` on exemplar tiles — the comparison is then apples to
# apples with no second code path.
CORE_LO_M, CORE_HI_M = 750.0, 2250.0
# Courses smaller than this are pitch-and-putt / driving ranges, not the
# 9-18 hole layouts the generator targets.
MIN_COURSE_AREA_M2 = 2.0e5

# Search boxes, one per archetype region, so the sample spans the same
# landform provinces the exemplar tiles come from. (lat, lon) centre and a
# half-span in degrees.
#
# The boxes MUST NOT OVERLAP. The first pair did: piedmont was centred at
# (35.6, -79.8) with a 0.6 deg half-span and sandhills at (35.2, -79.45) with
# 0.5, which share the Pinehurst / Southern Pines corner — and since courses
# are ranked by area, all SIX piedmont courses came back as the same NC
# sandhills courses sandhills had already claimed. "Piedmont" measured 30.5 m
# of core relief that was not piedmont at all. `run()` now dedupes across
# archetypes as a backstop, but non-overlapping boxes are the real fix.
SEARCH_BOXES = {
    # Uwharrie / Yadkin piedmont — the province the exemplar tiles sit in
    # (Uwharrie NF, 35.2-35.5 N, 79.8-80.1 W), pushed west of the sandhills.
    "piedmont": (35.60, -80.45, 0.42),
    "florida_lowland": (28.0, -81.6, 0.6),
    "glacial_moraine": (43.3, -88.2, 0.6),
    "mountain_bench": (36.1, -84.6, 0.6),
    "sandhills": (35.2, -79.45, 0.5),  # NC sandhills golf province
}


def find_courses(archetype: str, limit: int = 12) -> list[dict]:
    """Golf-course polygons in an archetype's region, as centroids."""
    lat, lon, half = SEARCH_BOXES[archetype]
    bbox = (lat - half, lon - half, lat + half, lon + half)
    query = (
        "[out:json][timeout:120];("
        f'way["leisure"="golf_course"]({bbox[0]},{bbox[1]},{bbox[2]},{bbox[3]});'
        f'relation["leisure"="golf_course"]({bbox[0]},{bbox[1]},{bbox[2]},{bbox[3]});'
        ");out geom;"
    )
    resp = osm.overpass(query)
    out = []
    for el in resp.get("elements", []):
        polys = osm.element_polygons(el)
        if not polys:
            continue
        ring = polys[0]["coordinates"][0]
        xs = [c[0] for c in ring]
        ys = [c[1] for c in ring]
        clat, clon = float(np.mean(ys)), float(np.mean(xs))
        # crude m^2 via degree scaling — only used to drop tiny facilities
        w = (max(xs) - min(xs)) * 111_320.0 * np.cos(np.radians(clat))
        h = (max(ys) - min(ys)) * 110_540.0
        if w * h < MIN_COURSE_AREA_M2:
            continue
        out.append({
            "name": (el.get("tags") or {}).get("name", f"osm{el.get('id')}"),
            "lat": clat, "lon": clon, "approx_area_m2": float(w * h),
        })
    out.sort(key=lambda c: (-c["approx_area_m2"], c["name"]))
    return out[:limit]


def measure_course(archetype: str, c: dict) -> dict | None:
    """Relief over a core-sized window centred on the course."""
    COURSES.mkdir(parents=True, exist_ok=True)
    dest_dir = COURSES / archetype
    dest_dir.mkdir(parents=True, exist_ok=True)
    key = f"{round(c['lat']*1000):06d}_{round(-c['lon']*1000):06d}"
    tile = dest_dir / f"{key}.cgrid"
    if not tile.exists():
        zone = int((c["lon"] + 180.0) / 6.0) + 1
        epsg = 26900 + zone  # NAD83 / UTM north
        try:
            fetch.fetch_tile(c["lat"], c["lon"], epsg, zone, tile)
        except Exception as exc:
            print(f"    [fail] {c['name']}: {type(exc).__name__}: {exc}")
            return None
    z, (_, _, cell) = cgrid.read_f32(tile)
    z = z.astype(float)
    if np.isfinite(z).mean() < 0.9:
        return None
    c0, c1 = int(CORE_LO_M / cell), int(CORE_HI_M / cell)
    core = z[c0:c1, c0:c1]
    fin = core[np.isfinite(core)]
    if fin.size < 0.9 * core.size:
        return None
    relief = float(np.percentile(fin, 99) - np.percentile(fin, 1))
    gy, gx = np.gradient(core, cell)
    return {
        "name": c["name"], "lat": c["lat"], "lon": c["lon"],
        "core_relief_m": relief,
        "slope_p50": float(np.nanmedian(np.hypot(gx, gy))),
        "slope_p90": float(np.nanpercentile(np.hypot(gx, gy), 90)),
    }


def _assert_boxes_disjoint() -> None:
    """Overlapping boxes silently hand one archetype another's courses, and
    the result looks perfectly reasonable — piedmont reported a 30.5 m cap
    that was six Pinehurst courses. Fail loudly instead."""
    items = sorted(SEARCH_BOXES.items())
    for i, (a, (la, lo_a, ha)) in enumerate(items):
        for b, (lb, lo_b, hb) in items[i + 1:]:
            if abs(la - lb) < ha + hb and abs(lo_a - lo_b) < ha + hb:
                raise SystemExit(
                    f"courses.SEARCH_BOXES overlap: {a} and {b} share area — "
                    "the larger-area ranking will give both the same courses"
                )


def run(archetype: str | None = None, limit: int = 12):
    _assert_boxes_disjoint()
    report = {}
    # Backstop for the boxes: a course measured for one archetype can never
    # be counted for another, whatever the geometry says.
    claimed: dict[str, str] = {}
    for arch in sorted(SEARCH_BOXES):
        if archetype and arch != archetype:
            continue
        cache = COURSES / f"{arch}.json"
        if cache.exists():
            rows = json.loads(cache.read_text())
        else:
            rows = []
            for c in find_courses(arch, limit):
                m = measure_course(arch, c)
                if m:
                    rows.append(m)
                    print(f"  [ok] {arch}/{m['name']}: core relief {m['core_relief_m']:.1f} m")
            cache.parent.mkdir(parents=True, exist_ok=True)
            cache.write_text(json.dumps(rows, indent=1))
        kept = []
        for r in rows:
            key = f"{r['lat']:.4f},{r['lon']:.4f}"
            if key in claimed:
                print(f"  [dup] {arch}/{r['name']} already counted for {claimed[key]}")
                continue
            claimed[key] = arch
            kept.append(r)
        report[arch] = kept

    lines = ["# Real golf-course core relief", "",
             "Relief (p99-p1) over a 1.5 km window — the same window as the "
             "routable core — centred on OSM `leisure=golf_course` polygons, "
             "measured from 3DEP with the campaign's own fetch.",
             "",
             "A built course is routable by definition, so this is the "
             "empirical answer to what `landform.core_relief_cap_m` should be.",
             "",
             "| region | n | core relief p25 / p50 / p75 / p90 (m) | current cap |",
             "|---|---|---|---|"]
    caps = {"florida_lowland": 3, "glacial_moraine": 22, "mountain_bench": 40,
            "piedmont": 25, "sandhills": 15}
    for arch, rows in sorted(report.items()):
        v = [r["core_relief_m"] for r in rows]
        if not v:
            continue
        q = np.percentile(v, [25, 50, 75, 90])
        lines.append(f"| {arch} | {len(v)} | {q[0]:.1f} / **{q[1]:.1f}** / "
                     f"{q[2]:.1f} / {q[3]:.1f} | {caps.get(arch, '—')} |")
    (OUT / "report").mkdir(parents=True, exist_ok=True)
    path = OUT / "report" / "course_relief.md"
    path.write_text("\n".join(lines) + "\n")
    print("\n" + "\n".join(lines[7:]))
    print(f"\nwrote {path}")
    return report
