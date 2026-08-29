"""Fetch OSM greens and assign each to its course polygon.

Assignment rule: smallest-area CONTAINING course polygon (resort umbrella
relations are larger than the individual course ways inside them, so smallest
picks the specific course). Unassigned greens adopt the nearest boundary
within GREEN_ASSIGN_NEAR_M, else drop with a logged reason.

Status after this pass:
  keeper                >= MIN_GREENS_KEEP greens, plausibly one course
  multi_course_unsplit  > MULTI_COURSE_GREENS greens and no sub-polygons —
                        greens still feed the point scorer; the course is
                        excluded from window fitting (the NC-56% confound)
  few_greens            below the floor; kept for boundary shape stats only
"""

from __future__ import annotations

import json

import numpy as np

from . import config, geo, net, registry


def _greens_query(s, w, n, e) -> str:
    return ("[out:json][timeout:180];("
            f'way["golf"="green"]({s},{w},{n},{e});'
            f'relation["golf"="green"]({s},{w},{n},{e});'
            ");out geom tags;")


def _green_rings(data: dict) -> list[tuple[int, np.ndarray]]:
    out = []
    for el in data.get("elements", []):
        if el.get("type") == "way" and el.get("geometry"):
            ring = np.array([[g["lat"], g["lon"]] for g in el["geometry"]], float)
        elif el.get("type") == "relation" and el.get("members"):
            rings = [np.array([[g["lat"], g["lon"]] for g in m.get("geometry", [])], float)
                     for m in el["members"] if m.get("role") in ("outer", "")]
            rings = [r for r in rings if len(r) >= 4]
            if not rings:
                continue
            ring = max(rings, key=len)
        else:
            continue
        if len(ring) >= 4:
            out.append((el["id"], ring))
    return out


def fetch_greens() -> list[tuple[int, np.ndarray]]:
    """Bulk per search box + per-course for curated records outside boxes."""
    greens: dict[int, np.ndarray] = {}
    failures = 0
    # quadrant-split the box queries: golf=green over a 2.6-degree box is a
    # heavy query and the mirrors 504 it under load
    for (tag, la, lo, h) in config.SEARCH_BOXES:
        for (qa, qo) in ((la - h / 2, lo - h / 2), (la - h / 2, lo + h / 2),
                         (la + h / 2, lo - h / 2), (la + h / 2, lo + h / 2)):
            try:
                data = net.overpass(_greens_query(qa - h / 2, qo - h / 2,
                                                  qa + h / 2, qo + h / 2))
            except net.FetchError as ex:
                print(f"  [net] greens {tag} quad: {ex}")
                failures += 1
                continue
            for gid, ring in _green_rings(data):
                greens[gid] = ring
    for r in registry.all_records():
        if r["region_tag"] != "other":
            continue
        ring = np.array(r["osm"]["ring_ll"], float)
        pad = 0.01
        s, w = ring[:, 0].min() - pad, ring[:, 1].min() - pad
        n, e = ring[:, 0].max() + pad, ring[:, 1].max() + pad
        try:
            data = net.overpass(_greens_query(s, w, n, e))
        except net.FetchError as ex:
            print(f"  [net] greens {r['course_id']}: {ex}")
            failures += 1
            continue
        for gid, gr in _green_rings(data):
            greens[gid] = gr
    print(f"  greens fetched: {len(greens)} ({failures} query failures)")
    return sorted(greens.items())


def run() -> dict:
    recs = registry.all_records()
    # course polygons in their own UTM + lat/lon bboxes for prefilter
    courses = []
    for r in recs:
        ring = np.array(r["osm"]["ring_ll"], float)
        courses.append(dict(
            rec=r, ring_ll=ring,
            bbox=(ring[:, 0].min(), ring[:, 1].min(),
                  ring[:, 0].max(), ring[:, 1].max()),
            ring_m=geo.ll_to_m(ring[:, 0], ring[:, 1], r["utm"]["zone"]),
            area=r["osm"]["area_m2"]))
    assigned: dict[str, list] = {r["course_id"]: [] for r in recs}
    dropped = {"no_polygon": 0, "assigned_near": 0, "contained": 0}
    for gid, gring in fetch_greens():
        glat, glon = float(gring[:, 0].mean()), float(gring[:, 1].mean())
        # containment: smallest containing polygon
        best = None
        near = None
        for c in courses:
            b = c["bbox"]
            if not (b[0] - 0.005 <= glat <= b[2] + 0.005
                    and b[1] - 0.005 <= glon <= b[3] + 0.005):
                continue
            zone = c["rec"]["utm"]["zone"]
            gm = geo.ll_to_m(gring[:, 0], gring[:, 1], zone)
            garea, gcen = geo.shoelace(gm)
            if geo.point_in_poly(gcen, c["ring_m"]):
                if best is None or c["area"] < best[0]["area"]:
                    best = (c, gcen, abs(garea))
            else:
                d = geo.dist_to_ring(gcen, c["ring_m"])
                if d <= config.GREEN_ASSIGN_NEAR_M and (near is None or d < near[3]):
                    near = (c, gcen, abs(garea), d)
        if best is not None:
            c, gcen, garea = best
            assigned[c["rec"]["course_id"]].append(
                dict(osm_id=gid, yx_utm=[float(gcen[0]), float(gcen[1])],
                     area_m2=garea, assign="contained"))
            dropped["contained"] += 1
        elif near is not None:
            c, gcen, garea, d = near
            assigned[c["rec"]["course_id"]].append(
                dict(osm_id=gid, yx_utm=[float(gcen[0]), float(gcen[1])],
                     area_m2=garea, assign=f"nearest-{d:.0f}m"))
            dropped["assigned_near"] += 1
        else:
            dropped["no_polygon"] += 1

    # sub-polygon map: which courses sit inside another course polygon
    contains_subs = set()
    for i, a in enumerate(courses):
        for j, b in enumerate(courses):
            if i == j or a["area"] <= b["area"]:
                continue
            _, cen = geo.shoelace(b["ring_m"])
            if abs(a["rec"]["centroid_ll"][0] - b["rec"]["centroid_ll"][0]) < 0.05 \
               and geo.point_in_poly(cen, a["ring_m"]):
                contains_subs.add(a["rec"]["course_id"])

    stats = {"keeper": 0, "few_greens": 0, "multi_course_unsplit": 0}
    from .discover import region_for
    for c in courses:
        r = c["rec"]
        # keep tags consistent when SEARCH_BOXES change between runs
        r["region_tag"] = region_for(*r["centroid_ll"])
        gs = sorted(assigned[r["course_id"]], key=lambda g: g["osm_id"])
        r["greens"] = gs
        n = len(gs)
        # thresholds, not nearest-multiple: practice greens pad every count
        # (Riviera 24 = 18 holes + 6 practice), so 24 must read 18, not 27
        r["holes_estimate"] = (0 if n == 0 else 9 if n <= 13 else
                               18 if n <= 26 else 27 if n <= 31 else 36)
        if n > config.MULTI_COURSE_GREENS and r["course_id"] not in contains_subs:
            r["status"] = "multi_course_unsplit"
        elif n >= config.MIN_GREENS_KEEP:
            r["status"] = "keeper"
        else:
            r["status"] = "few_greens"
        stats[r["status"]] += 1
        registry.write(r)
    registry.write_index()
    out = dict(stats=stats, greens=dropped)
    (config.OUT / "assign_report.json").write_text(json.dumps(out, indent=1))
    return out
