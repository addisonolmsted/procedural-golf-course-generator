"""Store + OSM access for Stage 3 — wraps the Stage-1 bridge (which wraps the
frozen Stage-0 readers) and re-parses the fetch-cache OSM features for
waterway centerlines (they survive only there; the store rasterized them)."""

from __future__ import annotations

import json
import os

import numpy as np
import yaml

from dtm_atlas import config as atlas_config
from dtm_atlas import grids as atlas_grids
from dtm_atlas import osm as atlas_osm
from dtm_metrics.store import (Course, course_keys, load_course,  # noqa: F401
                               verify_store_or_die)

_HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(_HERE, "..", "out"))


def load_config() -> dict:
    with open(os.path.join(_HERE, "config.yaml")) as f:
        return yaml.safe_load(f)


def origin_utm(meta: dict) -> tuple[float, float]:
    x0, y0, _x1, _y1 = meta["window_utm"]
    return float(x0), float(y0)


def epsg_of(meta: dict) -> int:
    return int(str(meta["crs"]).split(":")[1])


def load_waterways(key: str, meta: dict) -> list[np.ndarray]:
    """OSM waterway centerlines in the course-local frame, (N,2) float arrays.
    Source: fetch_cache features.json — classify_features puts waterway=* ways
    (and only those) into the 'water' class as `lines`."""
    path = os.path.join(atlas_config.FETCH_CACHE, key, "features.json")
    if not os.path.exists(path):
        return []
    with open(path) as f:
        feats = atlas_osm.classify_features(json.load(f))
    lines = feats.get("water", {}).get("lines", [])
    x0, y0 = origin_utm(meta)
    epsg = epsg_of(meta)
    out = []
    for line in lines:
        if len(line) < 2:
            continue
        lons = [c[0] for c in line]
        lats = [c[1] for c in line]
        xs, ys = atlas_grids.ll_to_utm(lons, lats, epsg)
        pts = np.stack([np.asarray(xs, float) - x0,
                        np.asarray(ys, float) - y0], axis=1)
        out.append(pts)
    return out
