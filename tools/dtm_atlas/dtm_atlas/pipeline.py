"""Per-course orchestration: stage functions + the batch driver.

Driver ergonomics mirror tools/parkland_atlas/collect.py: cache-first resume
(a stage is skipped when its outputs exist, unless --force), failures.log +
failed.json negative cache, batches never abort on one course, courtesy
sleep between networked courses. Only `fetch` touches the network.
"""

from __future__ import annotations

import base64
import datetime
import hashlib
import json
import os
import time

import numpy as np

from . import DTM_ATLAS_VERSION, config, grids, meta, osm, uscourses

DERIVE_ORDER = ["raster", "masks", "artifacts", "naturalize"]


# --- course selection --------------------------------------------------------

def select_courses(course_arg: str, limit: int) -> list[str]:
    keys = uscourses.read_list()
    if course_arg:
        want = [k.strip() for k in course_arg.split(",") if k.strip()]
        unknown = [k for k in want if k not in keys]
        if unknown:
            raise SystemExit(f"unknown course keys: {unknown}")
        keys = want
    if limit > 0:
        keys = keys[:limit]
    return keys


def _parkland_record(key: str) -> dict:
    path = os.path.join(config.PARKLAND, "out", "cache", f"{key}.json")
    with open(path) as f:
        return json.load(f)


def log_fail(key: str, stage: str, why: str) -> None:
    os.makedirs(config.OUT, exist_ok=True)
    with open(os.path.join(config.OUT, "failures.log"), "a") as f:
        f.write(f"{key}\t{stage}\t{why}\n")
    fpath = os.path.join(config.OUT, "failed.json")
    failed = set()
    if os.path.exists(fpath):
        failed = set(json.load(open(fpath)))
    failed.add(key)
    with open(fpath, "w") as f:
        json.dump(sorted(failed), f)


def _failed_set() -> set[str]:
    fpath = os.path.join(config.OUT, "failed.json")
    return set(json.load(open(fpath))) if os.path.exists(fpath) else set()


# --- fetch stage --------------------------------------------------------------

def _holes_ll(rec: dict) -> list[tuple[float, float]]:
    pts = []
    for h in rec.get("holes", []):
        pts.extend((la, lo) for la, lo in h.get("pts", []))
    return pts


def _hull_fallback_polygons(rec: dict, epsg: int) -> list[dict]:
    """Convex hull of hole points, expanded radially +100 m from the centroid —
    the flagged fallback when OSM has no acceptable property boundary."""
    from scipy.spatial import ConvexHull
    pts = _holes_ll(rec)
    lats = [p[0] for p in pts]
    lons = [p[1] for p in pts]
    xs, ys = grids.ll_to_utm(lons, lats, epsg)
    P = np.stack([xs, ys], axis=1)
    hull = P[ConvexHull(P).vertices]
    c = hull.mean(axis=0)
    d = hull - c
    n = np.hypot(d[:, 0], d[:, 1])
    grown = c + d * ((n + 100.0) / np.maximum(n, 1e-9))[:, None]
    lons2, lats2 = grids.utm_to_ll(list(grown[:, 0]), list(grown[:, 1]), epsg)
    ring = [[lo, la] for lo, la in zip(lons2, lats2)]
    ring.append(ring[0])
    return [{"type": "Polygon", "coordinates": [ring]}]


def stage_fetch(key: str, force: bool) -> None:
    import requests

    cdir = os.path.join(config.FETCH_CACHE, key)
    done = all(os.path.exists(os.path.join(cdir, f)) for f in
               ("dem_1m.tif", "boundary.json", "features.json",
                "availability.json", "provenance.json"))
    if done and not force:
        return
    os.makedirs(cdir, exist_ok=True)
    rec = _parkland_record(key)
    label = rec.get("label", key)
    la0, lo0, la1, lo1 = rec["bbox"]
    holes_bbox = (la0, lo0, la1, lo1)      # (south, west, north, east)
    lat_c = (la0 + la1) / 2.0
    lon_c = (lo0 + lo1) / 2.0
    epsg = grids.utm_epsg(lon_c, lat_c)
    sess = requests.Session()
    prov: dict = {
        "fetched_utc": datetime.datetime.now(datetime.timezone.utc)
        .strftime("%Y-%m-%dT%H:%M:%SZ"),
        "requests": [],
    }

    # 1. property boundary
    dlat = config.BOUNDARY_PAD_M / 111320.0
    dlon = config.BOUNDARY_PAD_M / (111320.0 * max(0.2, np.cos(np.radians(lat_c))))
    bquery = osm.boundary_query((la0 - dlat, lo0 - dlon, la1 + dlat, lo1 + dlon))
    bresp = osm.overpass(bquery)
    osm.save_json(os.path.join(cdir, "boundary.json"), bresp)
    prov["requests"].append({"what": "overpass_boundary",
                             "osm3s": bresp.get("osm3s", {})})
    picked = osm.pick_boundary(bresp, label, holes_bbox)
    if picked is None:
        picked = {"polygons": _hull_fallback_polygons(rec, epsg),
                  "source": "holes_hull_fallback", "name": "", "name_score": 0.0,
                  "holes_bbox_overlap": 1.0}
    # window from boundary bounds + margin, snapped
    blats = [pt[1] for p in picked["polygons"] for pt in p["coordinates"][0]]
    blons = [pt[0] for p in picked["polygons"] for pt in p["coordinates"][0]]
    xs, ys = grids.ll_to_utm(blons, blats, epsg)
    window = grids.snap_window(min(xs), min(ys), max(xs), max(ys))
    picked["epsg"] = epsg
    picked["window_utm"] = list(window)
    osm.save_json(os.path.join(cdir, "picked_boundary.json"), picked)

    # 2. mask-class features over the window (+50 m so dilations don't clip)
    x0, y0, x1, y1 = window
    corner_x = [x0 - 50, x1 + 50, x1 + 50, x0 - 50]
    corner_y = [y0 - 50, y0 - 50, y1 + 50, y1 + 50]
    lons2, lats2 = grids.utm_to_ll(corner_x, corner_y, epsg)
    fb = (min(lats2), min(lons2), max(lats2), max(lons2))
    time.sleep(config.COURTESY_SLEEP_S)
    fresp = osm.overpass(osm.features_query(fb))
    osm.save_json(os.path.join(cdir, "features.json"), fresp)
    prov["requests"].append({"what": "overpass_features",
                             "osm3s": fresp.get("osm3s", {})})

    # 3. 3DEP availability + DEM
    avail = dep3_availability(fb, sess)
    osm.save_json(os.path.join(cdir, "availability.json"), avail)
    from . import dep3
    dem = dep3.export_dem(window, epsg, config.FETCH_RES_M, sess)
    tf = grids.work_transform(window, config.FETCH_RES_M)
    grids.write_gtiff(os.path.join(cdir, "dem_1m.tif"), dem, tf, epsg,
                      nodata=config.NODATA)
    prov["requests"].append({
        "what": "dep3_export",
        "endpoint": config.DEP3_EXPORT,
        "bbox_utm": list(window),
        "epsg": epsg,
        "res_m": config.FETCH_RES_M,
        "dem_sha256": hashlib.sha256(dem.tobytes()).hexdigest(),
    })
    prov["versions"] = {"dtm_atlas": DTM_ATLAS_VERSION}
    with open(os.path.join(cdir, "provenance.json"), "w") as f:
        json.dump(prov, f, indent=1, sort_keys=True)


def dep3_availability(bbox_ll, sess) -> dict:
    from . import dep3
    return dep3.query_availability(bbox_ll, sess)


# --- raster stage --------------------------------------------------------------

def stage_raster(key: str, force: bool) -> None:
    sdir = os.path.join(config.STORE, key)
    raw_path = os.path.join(sdir, "raw.tif")
    if os.path.exists(raw_path) and not force:
        return
    cdir = os.path.join(config.FETCH_CACHE, key)
    dem, _tf1, epsg = grids.read_gtiff(os.path.join(cdir, "dem_1m.tif"))
    picked = json.load(open(os.path.join(cdir, "picked_boundary.json")))
    avail = json.load(open(os.path.join(cdir, "availability.json")))
    window = tuple(picked["window_utm"])

    work = grids.block_mean_2x2(dem)
    tf = grids.work_transform(window, config.WORK_RES_M)
    grids.write_gtiff(raw_path, work, tf, epsg, nodata=config.NODATA)

    from . import dep3
    tex = dep3.texture_frac(dem)
    best = avail.get("best", "none")
    best_res = {lbl: res for (_l, lbl, res) in config.DEP3_INDEX_LAYERS}.get(best)
    rec = _parkland_record(key)
    valid = work != config.NODATA
    h, w = work.shape
    m = {
        "schema_version": 1,
        "key": key,
        "label": rec.get("label", key),
        "source": {"provider": "usgs_3dep",
                   "service": "3DEPElevation/ImageServer",
                   "request_res_m": config.FETCH_RES_M},
        "crs": f"EPSG:{epsg}",
        "grid": {"w": int(w), "h": int(h), "res_m": config.WORK_RES_M,
                 "transform": [tf.a, tf.b, tf.c, tf.d, tf.e, tf.f]},
        "window_utm": list(window),
        "fetch": {"effective_source": best,
                  "effective_res_m": best_res,
                  "projects": avail.get("layers", {}).get(best, []),
                  "texture_frac": round(tex, 4)},
        "usable_for_calibration": bool(
            best_res is not None and best_res <= config.USABLE_SOURCE_MAX_M),
        "boundary": {"type": "MultiPolygon",
                     "coordinates": [
                         [[[round(v, 7) for v in pt] for pt in ring]
                          for ring in p["coordinates"]]
                         for p in picked["polygons"]]},
        "boundary_source": picked["source"],
        "boundary_name_score": picked.get("name_score", 0.0),
        "margin_m": config.MARGIN_M,
        "elev": {"min": round(float(work[valid].min()), 2) if valid.any() else None,
                 "max": round(float(work[valid].max()), 2) if valid.any() else None},
        "versions": {"pipeline": DTM_ATLAS_VERSION,
                     "params_hash": config.params_hash()},
    }
    meta.write_meta(os.path.join(sdir, "meta.json"), m)


# --- driver --------------------------------------------------------------------

def _stage_fn(name: str):
    if name == "fetch":
        return stage_fetch
    if name == "raster":
        return stage_raster
    if name == "masks":
        from . import masks as m
        return m.stage_masks
    if name == "artifacts":
        from . import artifacts as a
        return a.stage_artifacts
    if name == "naturalize":
        from . import naturalize as n
        return n.stage_naturalize
    raise KeyError(name)


def run(cmd: str, keys: list[str], force: bool, offline: bool) -> int:
    if cmd == "qa":
        from . import qa as qa_mod
        return qa_mod.build(keys)
    seq = [cmd] if cmd != "all" else (
        (["fetch"] if not offline else []) + DERIVE_ORDER)
    stages = {s: _stage_fn(s) for s in seq}

    failed_before = _failed_set()
    n_ok = n_fail = 0
    for i, key in enumerate(keys):
        if cmd == "fetch" and key in failed_before and not force:
            print(f"[{i + 1}/{len(keys)}] {key}: skipped (failed.json; --force to retry)")
            continue
        try:
            for stage in seq:
                stages[stage](key, force)
                if stage == "fetch" and len(keys) > 1:
                    time.sleep(config.COURTESY_SLEEP_S)
            n_ok += 1
            print(f"[{i + 1}/{len(keys)}] {key}: ok")
        except Exception as e:  # never abort the batch
            n_fail += 1
            why = f"{type(e).__name__}: {e}"
            log_fail(key, stage, why)
            print(f"[{i + 1}/{len(keys)}] {key}: FAIL at {stage}: {why}")
    print(f"done: {n_ok} ok, {n_fail} failed"
          + (" (see out/failures.log)" if n_fail else ""))
    if cmd == "all":
        from . import qa as qa_mod
        qa_mod.build(keys)
    return 0 if n_fail == 0 else 1
