"""NHDPlus HR truth channel for Stage 3T: per-tile flowline + waterbody
vectors with Value-Added Attributes (stream order, TotDASqKm drainage area,
reach slope), fetched keyless from the National Map ArcGIS REST — the same
service family and discipline as dep3.py (retries, UA, verbatim caches,
courtesy sleeps). Piggybacks the OSM natural=cliff fetch for bluff seeding.

Caches (raw, unfiltered — FCode policy is applied at CONSUMPTION so
re-filtering never re-fetches):
  out/store_tiles/{key}/nhd/flowlines.geojson
  out/store_tiles/{key}/nhd/waterbody.geojson
  out/store_tiles/{key}/nhd/nhd_meta.json
  out/store_tiles/{key}/nhd/features_cliff.json
"""

from __future__ import annotations

import datetime
import json
import os
import time

import numpy as np

from . import config, grids, osm, tiles

NHD_ROOT = ("https://hydro.nationalmap.gov/arcgis/rest/services/"
            "NHDPlus_HR/MapServer")
# layer NAMES are stable across service renumbering; IDs are discovered
LAYER_WANT = {"flowlines": "networknhdflowline", "waterbody": "nhdwaterbody"}
FETCH_BUFFER_M = 500.0     # junction completeness near tile edges
PAGE_TRIES = 4
# the live service returns lowercase property names; probe case-insensitively
VAA_WANTED = ("streamorde", "totdasqkm", "slope", "lengthkm", "reachcode",
              "levelpathi", "maxelevsmo", "minelevsmo", "hydroseq",
              "dnhydroseq", "fromnode", "tonode")


def prop(props: dict, name: str, default=None):
    """Case-insensitive property getter (service schemas vary in case)."""
    if name in props:
        return props[name]
    low = name.lower()
    for k, v in props.items():
        if k.lower() == low:
            return v
    return default

# --- FCode consumption policy (shared with dtm_primitives.nhdnet) ----------
# FType 460 StreamRiver (46000/46003 intermittent/46006 perennial/46007
# ephemeral) and 558 ArtificialPath (carries the network through waterbodies).
KEEP_FTYPES = (460, 558)
INTERMITTENT_FCODES = (46003, 46007)


def flowline_policy(props: dict) -> dict:
    """{keep, ftype, intermittent, artificial} for one flowline's properties
    (canals/pipelines/connectors are dropped here)."""
    fcode = int(prop(props, "fcode", 0) or 0)
    ftype = fcode // 100
    return {
        "keep": ftype in KEEP_FTYPES,
        "ftype": ftype,
        "intermittent": fcode in INTERMITTENT_FCODES,
        "artificial": ftype == 558,
    }


# --- service access ---------------------------------------------------------

def discover_layers(sess) -> dict:
    """Layer name -> id from the MapServer root, cached once per campaign."""
    cache = os.path.join(config.OUT, "nhd_layers.json")
    if os.path.exists(cache):
        with open(cache) as f:
            return json.load(f)
    r = sess.get(f"{NHD_ROOT}?f=json", headers=osm.UA, timeout=60)
    r.raise_for_status()
    layers = {}
    for lyr in r.json().get("layers", []):
        name = str(lyr.get("name", "")).lower().replace(" ", "")
        for want, pat in LAYER_WANT.items():
            if pat in name and want not in layers:
                layers[want] = int(lyr["id"])
    if set(layers) != set(LAYER_WANT):
        raise RuntimeError(f"NHD layer discovery incomplete: {layers}")
    with open(cache, "w") as f:
        json.dump(layers, f, sort_keys=True)
    return layers


def _query_page(sess, layer_id: int, env: dict, offset: int):
    params = {
        "f": "geojson",
        "where": "1=1",
        "geometry": json.dumps(env),
        "geometryType": "esriGeometryEnvelope",
        "inSR": 4326,
        "outSR": 4326,
        "outFields": "*",
        "returnGeometry": "true",
        "resultOffset": offset,
    }
    last = ""
    for attempt in range(PAGE_TRIES):
        try:
            r = sess.get(f"{NHD_ROOT}/{layer_id}/query", params=params,
                         headers=osm.UA, timeout=120)
            if r.status_code == 200:
                return r.json()
            last = f"{r.status_code} {r.text[:120]!r}"
        except Exception as e:  # noqa: BLE001 — retried
            last = repr(e)
        time.sleep(3.0 * (attempt + 1))
    raise RuntimeError(f"NHD query failed after {PAGE_TRIES} tries: {last}")


def query_layer(sess, layer_id: int, env: dict) -> dict:
    """Paginated envelope query -> one merged GeoJSON FeatureCollection."""
    feats: list = []
    offset = 0
    while True:
        page = _query_page(sess, layer_id, env, offset)
        got = page.get("features", [])
        feats.extend(got)
        more = page.get("exceededTransferLimit") \
            or page.get("properties", {}).get("exceededTransferLimit")
        if not got or not more:
            break
        offset += len(got)
    return {"type": "FeatureCollection", "features": feats}


# --- per-tile fetch ---------------------------------------------------------

def _tile_envelope_ll(key: str) -> dict:
    """Buffered lon/lat envelope of the tile window (store meta if ingested,
    else the committed manifest — so fetches can run before ingestion)."""
    meta_p = os.path.join(config.STORE, key, "meta.json")
    if os.path.exists(meta_p):
        with open(meta_p) as f:
            m = json.load(f)
        x0, y0, x1, y1 = m["window_utm"]
        epsg = int(str(m["crs"]).split(":")[1])
    else:
        rec = tiles.record(key)
        lat_c, lon_c = rec["center_ll"]
        window, epsg, _ring = tiles.tile_window_and_ring(
            lat_c, lon_c, float(rec.get("side_m", config.TILE_SIDE_M)),
            config.TILE_MARGIN_M)
        x0, y0, x1, y1 = window
    b = FETCH_BUFFER_M
    xs = [x0 - b, x1 + b, x1 + b, x0 - b]
    ys = [y0 - b, y0 - b, y1 + b, y1 + b]
    lons, lats = grids.utm_to_ll(xs, ys, epsg)
    return {"xmin": float(np.min(lons)), "ymin": float(np.min(lats)),
            "xmax": float(np.max(lons)), "ymax": float(np.max(lats)),
            "spatialReference": {"wkid": 4326}}


def fetch_tile(key: str, sess, layers: dict, force: bool = False) -> bool:
    """Fetch flowlines + waterbodies + OSM cliffs for one tile. Returns
    True when the network was touched (drives the courtesy sleep)."""
    ndir = os.path.join(config.STORE, key, "nhd")
    meta_p = os.path.join(ndir, "nhd_meta.json")
    cliff_p = os.path.join(ndir, "features_cliff.json")
    have_nhd = os.path.exists(meta_p)
    have_cliff = os.path.exists(cliff_p)
    if have_nhd and have_cliff and not force:
        return False
    os.makedirs(ndir, exist_ok=True)
    env = _tile_envelope_ll(key)
    touched = False

    if not have_nhd or force:
        fl = query_layer(sess, layers["flowlines"], env)
        wb = query_layer(sess, layers["waterbody"], env)
        osm.save_json(os.path.join(ndir, "flowlines.geojson"), fl)
        osm.save_json(os.path.join(ndir, "waterbody.geojson"), wb)
        props0 = (fl["features"][0]["properties"] if fl["features"] else {})
        kept = sum(flowline_policy(f.get("properties", {}))["keep"]
                   for f in fl["features"])
        meta = {
            "fetched_utc": datetime.datetime.now(datetime.timezone.utc)
            .strftime("%Y-%m-%dT%H:%M:%SZ"),
            "envelope_ll": env,
            "layer_ids": layers,
            "n_flowlines": len(fl["features"]),
            "n_flowlines_kept_policy": int(kept),
            "n_waterbody": len(wb["features"]),
            "fields_present": sorted(props0.keys()),
            "vaa_missing": [f for f in VAA_WANTED
                            if prop(props0, f) is None]
            if props0 else list(VAA_WANTED),
        }
        osm.save_json(meta_p, meta)
        touched = True

    if not have_cliff or force:
        s, w = env["ymin"], env["xmin"]
        n, e = env["ymax"], env["xmax"]
        q = (f'[out:json][timeout:60];'
             f'way["natural"="cliff"]({s},{w},{n},{e});out geom tags;')
        resp = osm.overpass(q)
        osm.save_json(cliff_p, resp)
        touched = True
    return touched


def run(keys: list[str] | None = None, force: bool = False) -> int:
    import requests
    if config.DATASET != "tiles":
        raise SystemExit("nhd: run with --dataset tiles")
    keys = keys or tiles.read_list()
    sess = requests.Session()
    layers = discover_layers(sess)
    ok = 0
    fail = 0
    for i, key in enumerate(sorted(keys)):
        try:
            touched = fetch_tile(key, sess, layers, force=force)
            ok += 1
            if touched:
                time.sleep(1.0)
        except Exception as e:  # noqa: BLE001 — per-tile isolation
            fail += 1
            print(f"[{i + 1}/{len(keys)}] {key}: FAIL {e}")
            continue
        if (i + 1) % 25 == 0:
            print(f"[{i + 1}/{len(keys)}] ...")
    print(f"nhd: {ok} ok, {fail} failed (layers {layers})")
    return 0 if fail == 0 else 1
