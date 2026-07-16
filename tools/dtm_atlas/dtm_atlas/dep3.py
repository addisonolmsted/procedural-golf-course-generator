"""USGS 3DEP fetches: DEM export (GeoTIFF, arbitrary bbox/SR/size, keyless)
plus the availability index that names the best source product under a window.

exportImage is a dynamic mosaic — requesting the course's NAD83 UTM zone at
1 m means the server mosaics native-1 m lidar projects with no datum change.
The service caps exports at 8000x8000 px; our windows top out ~4200, but a
deterministic row-major tiling fallback exists (with an overlap assertion).
"""

from __future__ import annotations

import io
import json
import time

import numpy as np
import requests
import rasterio
from rasterio.io import MemoryFile

from . import config
from .osm import UA, FetchError


def _export_once(bbox_utm, epsg: int, size_px, sess, tries: int = 4) -> np.ndarray:
    params = {
        "bbox": ",".join(str(int(v)) for v in bbox_utm),
        "bboxSR": str(epsg),
        "imageSR": str(epsg),
        "size": f"{size_px[0]},{size_px[1]}",
        "format": "tiff",
        "pixelType": "F32",
        "noData": str(int(config.NODATA)),
        "interpolation": "RSP_BilinearInterpolation",
        "renderingRule": json.dumps({"rasterFunction": "None"}),
        "f": "image",
    }
    last = None
    for attempt in range(tries):
        try:
            r = sess.get(config.DEP3_EXPORT, params=params, headers=UA, timeout=300)
            if r.status_code == 200 and r.content[:2] in (b"II", b"MM"):
                with MemoryFile(io.BytesIO(r.content).read()) as mf:
                    with mf.open() as ds:
                        arr = ds.read(1).astype(np.float32)
                if arr.shape != (size_px[1], size_px[0]):
                    last = f"shape {arr.shape} != {size_px}"
                else:
                    nodata_frac = float(np.mean(arr == config.NODATA))
                    if nodata_frac > 0.999:
                        last = "all nodata"
                    else:
                        return arr
            else:
                body = r.content[:120]
                last = f"{r.status_code} {body!r}"
        except (requests.RequestException, rasterio.errors.RasterioIOError) as e:
            last = f"{e}"
        time.sleep(3.0 * (attempt + 1))
    raise FetchError(f"3DEP export failed: {last}")


def plan_tiles(w: int, h: int, max_px: int = config.DEP3_MAX_PX):
    """Row-major tile plan covering (w, h): [(ox, oy, pw, ph), ...]."""
    tiles = []
    ny = (h + max_px - 1) // max_px
    nx = (w + max_px - 1) // max_px
    for ty in range(ny):
        for tx in range(nx):
            ox, oy = tx * max_px, ty * max_px
            tiles.append((ox, oy, min(max_px, w - ox), min(max_px, h - oy)))
    return tiles


def export_dem(bbox_utm, epsg: int, res_m: float, sess) -> np.ndarray:
    """Fetch the DEM over bbox_utm=(x0, y0, x1, y1) at res_m, tiling if needed.
    Returns float32 (h, w), row 0 = north, NODATA where the service has none."""
    x0, y0, x1, y1 = bbox_utm
    w = round((x1 - x0) / res_m)
    h = round((y1 - y0) / res_m)
    tiles = plan_tiles(w, h)
    if len(tiles) == 1:
        return _export_once(bbox_utm, epsg, (w, h), sess)
    out = np.full((h, w), config.NODATA, np.float32)
    prev = None
    for (ox, oy, pw, ph) in tiles:
        bb = (x0 + ox * res_m, y1 - (oy + ph) * res_m,
              x0 + (ox + pw) * res_m, y1 - oy * res_m)
        part = _export_once(bb, epsg, (pw, ph), sess)
        out[oy:oy + ph, ox:ox + pw] = part
        if prev is not None and prev[0] == oy and ox > 0:
            # 16-px seam sanity: re-request a thin overlap strip and compare
            strip_bb = (x0 + (ox - 8) * res_m, bb[1], x0 + (ox + 8) * res_m, bb[3])
            strip = _export_once(strip_bb, epsg, (16, ph), sess)
            got = out[oy:oy + ph, ox - 8:ox + 8]
            if not np.allclose(strip, got, atol=0.05, equal_nan=True):
                raise FetchError("tile seam mismatch beyond 5 cm")
        prev = (oy, ox)
        time.sleep(1.0)
    return out


def query_availability(bbox_ll, sess) -> dict:
    """3DEPElevationIndex: which product tiers intersect the window.
    bbox_ll = (south, west, north, east). Returns {"best": "1m"|"19as"|"13as"|
    "none", "layers": {label: [project names]}}."""
    out = {"best": "none", "layers": {}}
    for layer, label, _res in config.DEP3_INDEX_LAYERS:
        url = f"{config.DEP3_INDEX}/{layer}/query"
        params = {
            "geometry": f"{bbox_ll[1]},{bbox_ll[0]},{bbox_ll[3]},{bbox_ll[2]}",
            "geometryType": "esriGeometryEnvelope",
            "inSR": "4326",
            "spatialRel": "esriSpatialRelIntersects",
            # named fields vary per layer and unknown ones 400 — ask for all
            "outFields": "*",
            "returnGeometry": "false",
            "f": "json",
        }
        try:
            r = sess.get(url, params=params, headers=UA, timeout=60)
            feats = r.json().get("features", []) if r.status_code == 200 else []
        except (requests.RequestException, ValueError):
            feats = []
        names = set()
        for f in feats:
            attrs = f.get("attributes") or {}
            name = next((str(v) for k, v in sorted(attrs.items())
                         if v and ("project" in k.lower() or "workunit" in k.lower())),
                        None)
            names.add(name or f"n={len(feats)}")
        out["layers"][label] = sorted(names)
        if feats and out["best"] == "none":
            out["best"] = label
    return out


def texture_frac(dem_1m: np.ndarray) -> float:
    """Empirical sub-2 m information test: fraction of interior 2x2 blocks with
    nonzero variance. Upsampled 10 m sources read ~smooth at this scale."""
    v = dem_1m[: dem_1m.shape[0] // 2 * 2, : dem_1m.shape[1] // 2 * 2]
    valid = v != config.NODATA
    b = v.reshape(v.shape[0] // 2, 2, v.shape[1] // 2, 2)
    bv = valid.reshape(v.shape[0] // 2, 2, v.shape[1] // 2, 2).all(axis=(1, 3))
    if not bv.any():
        return 0.0
    var = b.var(axis=(1, 3))
    return float((var[bv] > 1e-6).mean())
