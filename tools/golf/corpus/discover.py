"""Course discovery: curated seed (parkland_atlas US list) + regional boxes.

Two sources, one output — registry stub records with boundary polygons in
course-local UTM metres, status "polygon" (greens not yet assigned):

  curated:    geocode string -> Nominatim -> Overpass boundary near the point,
              picked by name token-Jaccard >= 0.34 (parkland/dtm_atlas guard)
  discovered: one Overpass polygon query per SEARCH_BOX (disjointness asserted;
              macro_campaign/courses.py:50-57 records the Pinehurst
              double-count bug overlapping boxes caused)
"""

from __future__ import annotations

import pathlib
import sys

import numpy as np

from . import config, geo, net, registry

_TOOLS = pathlib.Path(__file__).resolve().parents[2]

# The 18 REFERENCE_US keys carry no geocode string in parkland_atlas
# (dtm_atlas/uscourses.py REFERENCE_US); geocode queries curated here.
REFERENCE_GEOCODE = {
    "augusta": "Augusta National Golf Club, Augusta, Georgia, USA",
    "bayhill": "Bay Hill Club and Lodge, Orlando, Florida, USA",
    "bethpage": "Bethpage State Park Golf Course, Farmingdale, New York, USA",
    "brookline": "The Country Club, Brookline, Massachusetts, USA",
    "cherryhills": "Cherry Hills Country Club, Cherry Hills Village, Colorado, USA",
    "colonial": "Colonial Country Club, Fort Worth, Texas, USA",
    "congressional": "Congressional Country Club, Bethesda, Maryland, USA",
    "doral": "Trump National Doral, Miami, Florida, USA",
    "eastlake": "East Lake Golf Club, Atlanta, Georgia, USA",
    "firestone": "Firestone Country Club, Akron, Ohio, USA",
    "greenbrier": "The Greenbrier, White Sulphur Springs, West Virginia, USA",
    "oakmont": "Oakmont Country Club, Oakmont, Pennsylvania, USA",
    "olympia": "Olympia Fields Country Club, Olympia Fields, Illinois, USA",
    "quail": "Quail Hollow Club, Charlotte, North Carolina, USA",
    "riviera": "Riviera Country Club, Pacific Palisades, California, USA",
    "sawgrass": "TPC Sawgrass, Ponte Vedra Beach, Florida, USA",
    "valhalla": "Valhalla Golf Club, Louisville, Kentucky, USA",
    "wingedfoot": "Winged Foot Golf Club, Mamaroneck, New York, USA",
}


def assert_disjoint() -> None:
    bs = config.SEARCH_BOXES
    for i in range(len(bs)):
        for j in range(i + 1, len(bs)):
            _, la, lo, h = bs[i]
            _, lb, ob, g = bs[j]
            if abs(la - lb) < h + g and abs(lo - ob) < h + g:
                raise AssertionError(f"boxes overlap: {bs[i][0]} {bs[j][0]}")


def curated_list() -> list[tuple[str, str, str, str]]:
    """(key, label, geocode, architect) for the US subset."""
    keys = [ln.strip() for ln in
            (_TOOLS.parent / "tools/dtm_atlas/courses_us.txt").read_text().splitlines()
            if ln.strip() and not ln.startswith("#")]
    sys.path.insert(0, str(_TOOLS.parent / "tools/parkland_atlas"))
    try:
        import courses as pk  # type: ignore
    finally:
        sys.path.remove(str(_TOOLS.parent / "tools/parkland_atlas"))
    cand = {k: (lab, q, arch) for (k, lab, q, arch) in pk.CANDIDATES}
    out = []
    for k in keys:
        if k in cand:
            lab, q, arch = cand[k]
            out.append((k, lab, q, arch))
        elif k in REFERENCE_GEOCODE:
            out.append((k, REFERENCE_GEOCODE[k].split(",")[0], REFERENCE_GEOCODE[k], ""))
    return out


def _jaccard(a: str, b: str) -> float:
    STOP = {"golf", "club", "course", "country", "the", "gc", "cc", "links", "national"}
    ta = {w for w in a.lower().replace("&", " ").split() if w not in STOP}
    tb = {w for w in b.lower().replace("&", " ").split() if w not in STOP}
    if not ta or not tb:
        # stopword-only names ("The Country Club") -- exact-string fallback
        return 1.0 if a.lower().strip() == b.lower().strip() and a.strip() else 0.0
    return len(ta & tb) / len(ta | tb)


def _polys_from_overpass(data: dict) -> list[dict]:
    """Overpass elements -> [{osm_type, osm_id, name, tags, ring_ll}] where
    ring_ll is the outer ring [[lat, lon], ...]. Relations use their largest
    outer member ring (good enough for boundary shape; holes ignored)."""
    out = []
    for el in data.get("elements", []):
        t = el.get("type")
        tags = el.get("tags", {})
        if t == "way" and el.get("geometry"):
            ring = [[g["lat"], g["lon"]] for g in el["geometry"]]
        elif t == "relation" and el.get("members"):
            rings = [[[g["lat"], g["lon"]] for g in m.get("geometry", [])]
                     for m in el["members"] if m.get("role") in ("outer", "")]
            rings = [r for r in rings if len(r) >= 4]
            if not rings:
                continue
            ring = max(rings, key=len)
        else:
            continue
        if len(ring) < 4:
            continue
        out.append(dict(osm_type=t, osm_id=el["id"],
                        name=tags.get("name", ""), tags=tags, ring_ll=ring))
    return out


def _stub(poly: dict, source: str, region_tag: str) -> dict | None:
    ring = np.array(poly["ring_ll"], float)
    lat, lon = float(ring[:, 0].mean()), float(ring[:, 1].mean())
    zone = geo.utm_zone(lon)
    ring_m = geo.ll_to_m(ring[:, 0], ring[:, 1], zone)
    area, cen = geo.shoelace(ring_m)
    area = abs(area)
    if area < config.MIN_COURSE_AREA_M2:
        return None
    long_m, short_m, axis = geo.pca_box(ring_m)
    cid = registry.course_id(poly["name"], poly["osm_type"], poly["osm_id"])
    return dict(
        course_id=cid, name=poly["name"] or cid, source=source,
        fame_tier=1, status="polygon", region_tag=region_tag,
        centroid_ll=[lat, lon],
        osm=dict(type=poly["osm_type"], id=poly["osm_id"],
                 area_m2=area, pca_long_m=long_m, pca_short_m=short_m,
                 pca_axis_deg=axis,
                 compactness=geo.compactness(area, ring_m),
                 ring_ll=ring.tolist()),
        utm=dict(zone=zone),
        greens=[], holes_estimate=None, tile=None, water_mask=None,
        params_hash=config.params_hash(),
    )


def region_for(lat: float, lon: float) -> str:
    for (tag, la, lo, h) in config.SEARCH_BOXES:
        if abs(lat - la) <= h and abs(lon - lo) <= h:
            return tag
    return "other"


def run_curated(limit: int | None = None) -> tuple[int, int]:
    got = miss = 0
    for (key, label, geocode, arch) in curated_list()[:limit]:
        hits = net.nominatim(geocode)
        if not hits:
            # over-specific strings miss ("Bay Hill Club and Lodge, Orlando,
            # Florida, USA"); retry name + last two locality tokens
            parts = [t.strip() for t in geocode.split(",")]
            if len(parts) >= 3:
                hits = net.nominatim(", ".join([parts[0]] + parts[-2:]))
        if not hits and "," in geocode:
            hits = net.nominatim(geocode.split(",")[0] + ", USA")
        if not hits:
            miss += 1
            continue
        lat, lon = float(hits[0]["lat"]), float(hits[0]["lon"])
        pad = 0.03
        q = ("[out:json][timeout:90];("
             f'way["leisure"="golf_course"]({lat-pad},{lon-pad},{lat+pad},{lon+pad});'
             f'relation["leisure"="golf_course"]({lat-pad},{lon-pad},{lat+pad},{lon+pad});'
             ");out geom tags;")
        polys = _polys_from_overpass(net.overpass(q))
        best = None
        for p in polys:
            j = _jaccard(p["name"], label)
            if j >= 0.34 and (best is None or j > best[0]):
                best = (j, p)
        if best is None and len(polys) == 1:
            best = (0.0, polys[0])   # unnamed sole polygon at the geocode point
        if best is None:
            miss += 1
            continue
        rec = _stub(best[1], "curated", region_for(lat, lon))
        if rec is None:
            miss += 1
            continue
        rec["name"] = label
        rec["architect"] = arch
        rec["curated_key"] = key
        registry.write(rec)
        got += 1
    return got, miss


def run_boxes() -> int:
    assert_disjoint()
    # dedupe by OSM id, not course_id — curated records carry the parkland
    # label as their name, so the slug half of the id differs across sources
    seen = {(r["osm"]["type"], r["osm"]["id"]) for r in registry.all_records()}
    n = 0
    for (tag, la, lo, h) in config.SEARCH_BOXES:
        q = ("[out:json][timeout:180];("
             f'way["leisure"="golf_course"]({la-h},{lo-h},{la+h},{lo+h});'
             f'relation["leisure"="golf_course"]({la-h},{lo-h},{la+h},{lo+h});'
             ");out geom tags;")
        for p in _polys_from_overpass(net.overpass(q)):
            if (p["osm_type"], p["osm_id"]) in seen:
                continue
            rec = _stub(p, "discovered", tag)
            if rec is None:
                continue
            seen.add((p["osm_type"], p["osm_id"]))
            registry.write(rec)
            n += 1
    return n
