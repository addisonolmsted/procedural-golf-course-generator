"""Load a reference course into the (height, cell_size, mask, meta) contract.

The atlas is NOT GeoTIFFs — each course is a 256x256 elevation grid packed over
its survey extent by tools/parkland_atlas (base64 `b64` = little-endian u16
decimetres above `emin`, `tb64`/`wb64` = MSB-first tree/water bit masks, row 0 =
north). This module decodes that, resamples every course to the common square
cell size, builds the two clip masks from the OSM hole centrelines, and returns
a plain numpy surface + validity mask so `core.py` never sees a geo type.
"""

from __future__ import annotations

import base64
import json
import math
import os
from dataclasses import dataclass, field

import numpy as np
from scipy import ndimage

GRID_N = 256
DEG_M = 111320.0


@dataclass
class Course:
    course_id: str
    label: str
    clip: str
    height: np.ndarray          # resampled elevation (m), row 0 = north
    cell_size: float            # common cell size (m)
    valid: np.ndarray           # bool: land + in-clip (metrics' mask)
    water: np.ndarray           # bool water mask (resampled)
    tree: np.ndarray            # bool tree mask (resampled)
    meta: dict = field(default_factory=dict)


# ---------------------------------------------------------------------------
# decode packed atlas payloads
# ---------------------------------------------------------------------------

def _decode_elev(b64, emin):
    raw = base64.b64decode(b64)
    dm = np.frombuffer(raw, dtype="<u2").astype(np.float64)
    return (emin + dm / 10.0).reshape(GRID_N, GRID_N)


def _decode_mask(b64):
    raw = np.frombuffer(base64.b64decode(b64), dtype=np.uint8)
    bits = np.unpackbits(raw, bitorder="big")[: GRID_N * GRID_N]
    return bits.reshape(GRID_N, GRID_N).astype(bool)


# ---------------------------------------------------------------------------
# resample native (non-square) grid to a common square cell size
# ---------------------------------------------------------------------------

def _resample(grid, out_ny, out_nx, order):
    # sample cell centres of the target grid from the source's [0..GRID_N] span
    sy = (np.arange(out_ny) + 0.5) / out_ny * GRID_N - 0.5
    sx = (np.arange(out_nx) + 0.5) / out_nx * GRID_N - 0.5
    YY, XX = np.meshgrid(sy, sx, indexing="ij")
    return ndimage.map_coordinates(grid.astype(np.float64), [YY, XX],
                                   order=order, mode="nearest")


# ---------------------------------------------------------------------------
# clip geometry from hole centrelines
# ---------------------------------------------------------------------------

def _holes_to_px(holes, bbox, ny, nx):
    """Hole polylines (lat/lon) -> lists of (row, col) float pixel coords."""
    la0, lo0, la1, lo1 = bbox
    dlat = (la1 - la0) or 1e-9
    dlon = (lo1 - lo0) or 1e-9
    out = []
    for h in holes:
        pts = []
        for lat, lon in h["pts"]:
            col = (lon - lo0) / dlon * nx
            row = (la1 - lat) / dlat * ny        # row 0 = north
            pts.append((row, col))
        if len(pts) >= 2:
            out.append(np.array(pts))
    return out


def _corridor_mask(shape, holes_px, buffer_cells):
    """Cells within buffer of any hole centreline."""
    from skimage.draw import line
    ny, nx = shape
    lines = np.zeros(shape, bool)
    for pts in holes_px:
        for a, b in zip(pts[:-1], pts[1:]):
            rr, cc = line(int(round(a[0])), int(round(a[1])),
                          int(round(b[0])), int(round(b[1])))
            ok = (rr >= 0) & (rr < ny) & (cc >= 0) & (cc < nx)
            lines[rr[ok], cc[ok]] = True
    if not lines.any():
        return np.zeros(shape, bool)
    dist = ndimage.distance_transform_edt(~lines)
    return dist <= buffer_cells


def _extent_mask(shape, holes_px, buffer_cells):
    """Convex hull of all hole vertices, dilated by the extent buffer."""
    from skimage.draw import polygon
    from scipy.spatial import ConvexHull
    ny, nx = shape
    pts = np.concatenate(holes_px, axis=0) if holes_px else np.zeros((0, 2))
    m = np.zeros(shape, bool)
    if len(pts) >= 3:
        try:
            hull = ConvexHull(pts[:, ::-1])  # (x=col, y=row)
            hp = pts[hull.vertices]
            rr, cc = polygon(hp[:, 0], hp[:, 1], shape)
            m[rr, cc] = True
        except Exception:
            m[:] = True
    else:
        m[:] = True
    if buffer_cells >= 1:
        m = ndimage.binary_dilation(m, iterations=int(round(buffer_cells)))
    return m


# ---------------------------------------------------------------------------
# public loader
# ---------------------------------------------------------------------------

def load_course(path, cfg, clip):
    """Return a `Course` for one clip variant, or None if that clip is N/A."""
    with open(path) as f:
        rec = json.load(f)
    cs = float(cfg["resample"]["common_cell_size_m"])
    wm, hm = float(rec["wm"]), float(rec["hm"])
    nx = max(8, int(round(wm / cs)))
    ny = max(8, int(round(hm / cs)))

    elev = _resample(_decode_elev(rec["b64"], rec["emin"]), ny, nx, cfg["resample"]["order"])
    tree = _resample(_decode_mask(rec["tb64"]).astype(np.float64), ny, nx,
                     cfg["resample"]["mask_order"]) > 0.5
    water = _resample(_decode_mask(rec["wb64"]).astype(np.float64), ny, nx,
                      cfg["resample"]["mask_order"]) > 0.5

    holes_px = _holes_to_px(rec.get("holes", []), rec["bbox"], ny, nx)
    corridor_missing = len(holes_px) == 0
    flags = []

    if clip == "fairway_corridor":
        if corridor_missing:
            return None
        clip_mask = _corridor_mask((ny, nx), holes_px,
                                   cfg["clip"]["corridor_buffer_m"] / cs)
    else:  # course_extent
        if corridor_missing:
            clip_mask = np.ones((ny, nx), bool)
            flags.append("no_holes_full_extent")
        else:
            clip_mask = _extent_mask((ny, nx), holes_px,
                                     cfg["clip"]["extent_buffer_m"] / cs)

    valid = clip_mask.copy()
    if cfg["mask"]["exclude_water"]:
        valid &= ~water
    if valid.sum() < 64:
        flags.append("few_valid_cells")

    group = rec.get("group")
    if not group:                       # reference courses carry no group field
        r = rec["emax"] - rec["emin"]
        group = "lowland" if r < 30 else "rolling" if r < 80 else "mountain"
    meta = {
        "course_id": rec["key"],
        "label": rec.get("label", rec["key"]),
        "arch": rec.get("arch", ""),
        "group": group,
        "clip_variant": clip,
        "native_cell_x_m": round(wm / GRID_N, 3),
        "native_cell_y_m": round(hm / GRID_N, 3),
        "common_cell_m": cs,
        "survey_area_m2": round(wm * hm, 1),
        "n_valid_cells": int(valid.sum()),
        "valid_area_m2": round(int(valid.sum()) * cs * cs, 1),
        "grid_ny": ny, "grid_nx": nx,
        "n_holes": len(rec.get("holes", [])),
        "treepct_atlas": rec.get("treepct"),
        "waterpct_atlas": rec.get("waterpct"),
        "emin": rec["emin"], "emax": rec["emax"],
        "flags": ";".join(flags),
    }
    return Course(rec["key"], meta["label"], clip, elev.astype(np.float64),
                  cs, valid, water, tree, meta)


def list_courses(cfg):
    base = os.path.join(os.path.dirname(os.path.abspath(__file__)),
                        cfg["data"]["cache_dir"])
    base = os.path.normpath(base)
    return sorted(os.path.join(base, f) for f in os.listdir(base) if f.endswith(".json"))
