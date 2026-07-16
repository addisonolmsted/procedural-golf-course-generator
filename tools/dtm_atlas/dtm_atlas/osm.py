"""Overpass fetches for Stage 0: the course PROPERTY BOUNDARY (full geometry,
unlike the parkland collector which discarded it) and the mask-class FEATURES.

Client etiquette mirrors tools/parkland_atlas/sources.py: endpoint rotation
across three mirrors, linear backoff, bounded retries. Raw responses are cached
verbatim by the pipeline so every derive stage is offline + deterministic.
"""

from __future__ import annotations

import json
import time

import requests

from . import config

UA = {"User-Agent": "dtm-atlas/0.1 (golf terrain research; contact: repo owner)"}


class FetchError(RuntimeError):
    pass


def overpass(query: str, tries: int = 4) -> dict:
    last = None
    for attempt in range(tries):
        ep = config.OVERPASS_ENDPOINTS[attempt % len(config.OVERPASS_ENDPOINTS)]
        try:
            r = requests.post(ep, data={"data": query}, headers=UA, timeout=120)
            if r.status_code == 200:
                return r.json()
            last = f"{r.status_code} @ {ep}"
        except requests.RequestException as e:
            last = f"{e} @ {ep}"
        time.sleep(3.0 * (attempt + 1))
    raise FetchError(f"overpass failed: {last}")


# --- queries ----------------------------------------------------------------

def boundary_query(bbox_ll: tuple[float, float, float, float]) -> str:
    """leisure=golf_course ways + relations with FULL geometry.
    bbox_ll = (south, west, north, east)."""
    b = ",".join(f"{v:.6f}" for v in bbox_ll)
    return (
        "[out:json][timeout:90];("
        f'way["leisure"="golf_course"]({b});'
        f'relation["leisure"="golf_course"]({b});'
        ");out geom tags;"
    )


def features_query(bbox_ll: tuple[float, float, float, float]) -> str:
    """Every mask-class feature in the window, full geometry."""
    b = ",".join(f"{v:.6f}" for v in bbox_ll)
    parts = []
    for sel in (
        '["golf"="green"]', '["golf"="tee"]', '["golf"="bunker"]',
        '["golf"="fairway"]', '["golf"="water_hazard"]', '["golf"="lateral_water_hazard"]',
        '["building"]', '["amenity"="parking"]',
        '["natural"="water"]', '["landuse"="reservoir"]',
    ):
        parts.append(f"way{sel}({b});relation{sel}({b});")
    for sel in ('["highway"]', '["railway"]', '["waterway"]'):
        parts.append(f"way{sel}({b});")
    return "[out:json][timeout:120];(" + "".join(parts) + ");out geom tags;"


# --- name guard (same rule as the parkland collector) ------------------------

_STOP = {"the", "golf", "club", "course", "country", "at", "and", "of", "links",
         "national", "resort", "gc", "cc", "&", "de", "la", "le", "el", "no"}


def _norm_tokens(name: str) -> set[str]:
    keep = []
    for tok in "".join(c if c.isalnum() else " " for c in (name or "").lower()).split():
        if tok not in _STOP:
            keep.append(tok)
    return set(keep)


def name_score(cand: str, osm: str) -> float:
    a, b = _norm_tokens(cand), _norm_tokens(osm)
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


# --- geometry assembly -------------------------------------------------------

def _way_coords(el: dict) -> list[list[float]]:
    """Overpass `out geom` way geometry -> [[lon, lat], ...] (GeoJSON order)."""
    return [[g["lon"], g["lat"]] for g in el.get("geometry", [])]


def assemble_rings(way_coord_lists: list[list[list[float]]]) -> list[list[list[float]]]:
    """Stitch open ways end-to-end into closed rings (exact node coordinates —
    Overpass returns identical floats for shared nodes). Deterministic: ways
    consumed in input order, forward match preferred over reversed."""
    pending = [list(w) for w in way_coord_lists if len(w) >= 2]
    rings: list[list[list[float]]] = []
    while pending:
        ring = pending.pop(0)
        progressed = True
        while progressed and ring[0] != ring[-1]:
            progressed = False
            for i, cand in enumerate(pending):
                if cand[0] == ring[-1]:
                    ring.extend(cand[1:])
                elif cand[-1] == ring[-1]:
                    ring.extend(list(reversed(cand))[1:])
                elif cand[-1] == ring[0]:
                    ring[:0] = cand[:-1]
                elif cand[0] == ring[0]:
                    ring[:0] = list(reversed(cand))[:-1]
                else:
                    continue
                pending.pop(i)
                progressed = True
                break
        if ring[0] == ring[-1] and len(ring) >= 4:
            rings.append(ring)
        # unclosed leftovers are dropped (logged by the caller via ring count)
    return rings


def element_polygons(el: dict) -> list[dict]:
    """OSM element (way or relation, `out geom`) -> list of GeoJSON Polygons
    [{"type":"Polygon","coordinates":[outer, inner...]}]. Ways: single ring.
    Relations: outer/inner member roles, rings assembled by stitching."""
    if el["type"] == "way":
        c = _way_coords(el)
        if len(c) >= 3:
            if c[0] != c[-1]:
                c = c + [c[0]]
            return [{"type": "Polygon", "coordinates": [c]}]
        return []
    if el["type"] != "relation":
        return []
    outers, inners = [], []
    for m in el.get("members", []):
        if m.get("type") != "way" or "geometry" not in m:
            continue
        coords = [[g["lon"], g["lat"]] for g in m["geometry"]]
        (outers if m.get("role", "outer") in ("outer", "") else inners).append(coords)
    out_rings = assemble_rings(outers)
    in_rings = assemble_rings(inners)
    polys = []
    for oring in out_rings:
        # attach inner rings whose first vertex falls inside this outer ring
        holes = [r for r in in_rings if _point_in_ring(r[0], oring)]
        polys.append({"type": "Polygon", "coordinates": [oring] + holes})
    return polys


def _point_in_ring(pt: list[float], ring: list[list[float]]) -> bool:
    x, y = pt
    inside = False
    for i in range(len(ring) - 1):
        x1, y1 = ring[i]
        x2, y2 = ring[i + 1]
        if (y1 > y) != (y2 > y):
            xa = x1 + (y - y1) / (y2 - y1) * (x2 - x1)
            if xa > x:
                inside = not inside
    return inside


def _bbox_overlap(a: tuple, b: tuple) -> float:
    """IoU-ish overlap of two (south, west, north, east) boxes, 0..1 of a."""
    s = max(a[0], b[0]); w = max(a[1], b[1])
    n = min(a[2], b[2]); e = min(a[3], b[3])
    if n <= s or e <= w:
        return 0.0
    inter = (n - s) * (e - w)
    area_a = (a[2] - a[0]) * (a[3] - a[1])
    return inter / area_a if area_a > 0 else 0.0


def pick_boundary(resp: dict, label: str, holes_bbox: tuple) -> dict | None:
    """Choose the course's property boundary from candidate elements: name
    Jaccard >= threshold preferred, tie-broken by overlap with the cached holes
    bbox; unnamed candidates accepted on overlap alone (>= 0.5). Returns
    {"polygons": [...GeoJSON...], "source": "osm_way:123", "name": ..., "score": ...}."""
    best = None
    for el in resp.get("elements", []):
        polys = element_polygons(el)
        if not polys:
            continue
        lats = [pt[1] for p in polys for pt in p["coordinates"][0]]
        lons = [pt[0] for p in polys for pt in p["coordinates"][0]]
        ebox = (min(lats), min(lons), max(lats), max(lons))
        overlap = _bbox_overlap(holes_bbox, ebox)
        nm = el.get("tags", {}).get("name", "")
        score = name_score(label, nm)
        ok = (score >= config.BOUNDARY_MIN_JACCARD and overlap > 0.05) or \
             (not nm and overlap >= 0.5) or (score > 0 and overlap >= 0.5)
        if not ok:
            continue
        rank = (score, overlap)
        if best is None or rank > best[0]:
            best = (rank, {
                "polygons": polys,
                "source": f"osm_{el['type']}:{el['id']}",
                "name": nm,
                "name_score": round(score, 3),
                "holes_bbox_overlap": round(overlap, 3),
            })
    return best[1] if best else None


def classify_features(resp: dict) -> dict[str, dict]:
    """Split a features response into mask classes ->
    {"polys": [GeoJSON Polygon...], "lines": [[[lon,lat],...], ...]}."""
    out = {k: {"polys": [], "lines": []} for k in config.MASK_CLASSES}
    for el in resp.get("elements", []):
        tags = el.get("tags", {})
        golf = tags.get("golf")
        if golf in ("green", "tee", "bunker", "fairway"):
            out[golf]["polys"].extend(element_polygons(el))
        elif golf in ("water_hazard", "lateral_water_hazard"):
            out["water"]["polys"].extend(element_polygons(el))
        elif "building" in tags:
            out["building"]["polys"].extend(element_polygons(el))
        elif tags.get("amenity") == "parking":
            out["road"]["polys"].extend(element_polygons(el))
        elif "highway" in tags or "railway" in tags:
            if el["type"] == "way":
                c = _way_coords(el)
                if len(c) >= 2:
                    out["road"]["lines"].append(c)
        elif tags.get("natural") == "water" or tags.get("landuse") == "reservoir":
            out["water"]["polys"].extend(element_polygons(el))
        elif "waterway" in tags:
            if tags.get("waterway") in ("riverbank", "dock"):
                out["water"]["polys"].extend(element_polygons(el))
            elif el["type"] == "way":
                c = _way_coords(el)
                if len(c) >= 2:
                    out["water"]["lines"].append(c)
    return out


def save_json(path: str, obj: dict) -> None:
    with open(path, "w") as f:
        json.dump(obj, f, separators=(",", ":"))
