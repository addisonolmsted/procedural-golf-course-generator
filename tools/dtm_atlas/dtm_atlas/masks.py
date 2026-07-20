"""Mask stack: rasterize the OSM feature classes onto the working grid, apply
metric dilations (true Euclidean via EDT), build the exterior-margin flag and
the inpaint union. Class masks are stored UNDILATED (truth); dilation radii
live in config/meta so a radius change is a cheap deterministic recompose.
"""

from __future__ import annotations

import json
import os

import numpy as np
from rasterio.features import rasterize
from scipy import ndimage

from . import config, grids, meta, osm


def _to_utm_geom(geom: dict, epsg: int) -> dict:
    """GeoJSON (lon,lat) -> same structure in UTM meters."""
    def ring_to_utm(ring):
        lons = [p[0] for p in ring]
        lats = [p[1] for p in ring]
        xs, ys = grids.ll_to_utm(lons, lats, epsg)
        return [[x, y] for x, y in zip(xs, ys)]
    if geom["type"] == "Polygon":
        return {"type": "Polygon",
                "coordinates": [ring_to_utm(r) for r in geom["coordinates"]]}
    if geom["type"] == "LineString":
        return {"type": "LineString", "coordinates": ring_to_utm(geom["coordinates"])}
    raise ValueError(geom["type"])


def _rasterize(geoms: list[dict], shape, transform, all_touched: bool) -> np.ndarray:
    if not geoms:
        return np.zeros(shape, np.uint8)
    return rasterize(
        [(g, 1) for g in geoms], out_shape=shape, transform=transform,
        fill=0, all_touched=all_touched, dtype="uint8",
    )


def dilate_m(mask: np.ndarray, radius_m: float, res_m: float) -> np.ndarray:
    """True Euclidean dilation by radius_m meters (EDT of the complement)."""
    if radius_m <= 0 or not mask.any():
        return mask.astype(bool)
    dist = ndimage.distance_transform_edt(~mask.astype(bool)) * res_m
    return dist <= radius_m


def dilate_by_width(halfwidth_map: np.ndarray, res_m: float) -> np.ndarray:
    """Variable-radius dilation: a cell joins the mask when its distance to the
    nearest center cell is <= that center cell's half-width (meters). One EDT
    with return_indices — deterministic."""
    centers = halfwidth_map > 0
    if not centers.any():
        return centers
    dist, (iy, ix) = ndimage.distance_transform_edt(
        ~centers, return_indices=True, return_distances=True)
    return dist * res_m <= halfwidth_map[iy, ix]


def _features_path(key: str) -> str:
    """Versioned features cache, falling back to the v1 file so derive stages
    stay runnable on pre-existing caches (pilot mode). meta records which."""
    p2 = os.path.join(config.FETCH_CACHE, key, config.FEATURES_FILE)
    return p2 if os.path.exists(p2) else \
        os.path.join(config.FETCH_CACHE, key, "features.json")


def stage_masks(key: str, force: bool) -> None:
    sdir = os.path.join(config.STORE, key)
    mdir = os.path.join(sdir, "masks")
    sentinel = os.path.join(mdir, "inpaint_classes.tif")
    # inpaint_nongolf.tif joins the sentinel so v1 store dirs re-run this stage
    if (os.path.exists(sentinel)
            and os.path.exists(os.path.join(mdir, "inpaint_nongolf.tif"))
            and not force):
        return
    m = meta.read_meta(os.path.join(sdir, "meta.json"))
    epsg = int(m["crs"].split(":")[1])
    tf = grids.work_transform(tuple(m["window_utm"]))
    shape = (m["grid"]["h"], m["grid"]["w"])
    raw, _tf, _e = grids.read_gtiff(os.path.join(sdir, "raw.tif"))
    valid = raw != config.NODATA

    fpath = _features_path(key)
    feats = osm.classify_features(json.load(open(fpath)))

    stats = {}
    class_masks: dict[str, np.ndarray] = {}
    road_halfwidth = np.zeros(shape, np.float32)   # meters at centerline cells
    for cls, (radius, _is_line) in config.MASK_CLASSES.items():
        f = feats.get(cls, {"polys": [], "lines": []})
        polys = [_to_utm_geom(p, epsg) for p in f["polys"]]
        mask = _rasterize(polys, shape, tf, all_touched=False)
        if cls == "road":
            # lines carry per-way half-widths; rasterize ascending so wider
            # ways win where they overlap (deterministic per-cell max)
            groups: dict[float, list] = {}
            for coords, hw in f["lines"]:
                groups.setdefault(round(float(hw), 2), []).append(coords)
            for hw in sorted(groups):
                lg = [_to_utm_geom({"type": "LineString", "coordinates": c},
                                   epsg) for c in groups[hw]]
                lm = _rasterize(lg, shape, tf, all_touched=True).astype(bool)
                mask |= lm
                road_halfwidth[lm] = hw
            # parking/polygon members use the class radius as their half-width
            road_halfwidth[mask.astype(bool) & (road_halfwidth == 0)] = radius
        else:
            lines = [_to_utm_geom({"type": "LineString", "coordinates": c},
                                  epsg) for c in f["lines"]]
            if lines:
                mask |= _rasterize(lines, shape, tf, all_touched=True)
        class_masks[cls] = mask.astype(bool)
        grids.write_gtiff(os.path.join(mdir, f"{cls}.tif"),
                          mask.astype(np.uint8), tf, epsg)
        stats[cls] = {"cells": int(mask.sum()),
                      "frac": round(float(mask.mean()), 5),
                      "dilate_m": radius}
    hw_on = road_halfwidth[road_halfwidth > 0]
    stats["road"]["halfwidth_p50"] = round(float(np.median(hw_on)), 1) if hw_on.size else 0.0
    stats["road"]["halfwidth_max"] = round(float(hw_on.max()), 1) if hw_on.size else 0.0
    grids.write_gtiff(os.path.join(mdir, "road_halfwidth.tif"),
                      road_halfwidth, tf, epsg)

    # exterior margin: outside the property boundary (flag only, never inpainted)
    bpolys = [_to_utm_geom({"type": "Polygon", "coordinates": rings}, epsg)
              for rings in m["boundary"]["coordinates"]]
    interior = _rasterize(bpolys, shape, tf, all_touched=False).astype(bool)
    exterior = (~interior) & valid
    grids.write_gtiff(os.path.join(mdir, "boundary_interior.tif"),
                      interior.astype(np.uint8), tf, epsg)
    grids.write_gtiff(os.path.join(mdir, "exterior.tif"),
                      exterior.astype(np.uint8), tf, epsg)
    grids.write_gtiff(os.path.join(mdir, "nodata.tif"),
                      (~valid).astype(np.uint8), tf, epsg)

    # inpaint union of DILATED classes (artifacts stage adds its masks later);
    # road lines dilate by per-way half-width, everything else by class radius
    dilated: dict[str, np.ndarray] = {}
    for cls in config.INPAINT_CLASSES:
        if cls == "road":
            dilated[cls] = dilate_by_width(road_halfwidth, config.WORK_RES_M)
        else:
            radius = config.MASK_CLASSES[cls][0]
            if config.DATASET == "tiles":
                # natural stream banks are signal, not berms — tiles dilate
                # water modestly (the 30 m course rationale is man-made
                # pond berms)
                radius = config.TILE_MASK_DILATE_OVERRIDES.get(cls, radius)
            dilated[cls] = dilate_m(class_masks[cls], radius,
                                    config.WORK_RES_M)
    inp = np.zeros(shape, bool)
    for cls in config.INPAINT_CLASSES:
        inp |= dilated[cls]
    inp &= valid
    grids.write_gtiff(sentinel, inp.astype(np.uint8), tf, epsg)

    # non-golf dilated union: the consolidation-density driver (green/tee/
    # bunker dilations must not read as urban fabric)
    nongolf = np.zeros(shape, bool)
    for cls in config.NONGOLF_CLASSES:
        nongolf |= dilated[cls]
    nongolf &= valid
    grids.write_gtiff(os.path.join(mdir, "inpaint_nongolf.tif"),
                      nongolf.astype(np.uint8), tf, epsg)

    m["masks"] = stats
    m["features_cache"] = os.path.basename(fpath)
    m["margin"] = {
        "exterior_frac": round(float(exterior.mean()), 4),
        "exterior_data_frac": round(
            float(valid[~interior].mean()) if (~interior).any() else 0.0, 4),
        "exterior_km2": round(
            float(exterior.sum()) * config.WORK_RES_M ** 2 / 1e6, 3),
    }
    meta.write_meta(os.path.join(sdir, "meta.json"), m)
