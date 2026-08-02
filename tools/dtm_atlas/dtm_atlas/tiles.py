"""Stage-0T tile index: natural 3 km x 3 km analog tiles selected by
tools/tile_scout. tiles_us.txt = bare keys (courses_us.txt format);
tiles_us.json = per-key manifest {center_ll: [lat, lon], side_m, label,
region, best_course, ...} written by `python3 -m tile_scout finalize`."""

from __future__ import annotations

import json
import os

import numpy as np

from . import config, grids


def grids_snap() -> float:
    return config.SNAP_M


def read_list() -> list[str]:
    with open(config.TILES_US) as f:
        return [ln.strip() for ln in f
                if ln.strip() and not ln.startswith("#")]


def load_manifest() -> dict:
    with open(config.TILES_MANIFEST) as f:
        return json.load(f)


def record(key: str) -> dict:
    man = load_manifest()
    if key not in man:
        raise SystemExit(f"tile key {key} not in {config.TILES_MANIFEST}")
    return man[key]


def tile_window_and_ring(lat_c: float, lon_c: float, side_m: float,
                         margin_m: float) -> tuple[tuple, int, list]:
    """The synthetic square boundary: snapped UTM window (side + 2*margin)
    centered on (lat, lon) + the closed lon/lat ring of the side_m square.
    Pure math — unit-testable without network."""
    epsg = grids.utm_epsg(lon_c, lat_c)
    xs, ys = grids.ll_to_utm([lon_c], [lat_c], epsg)
    # snap the CENTER to the grid first: snapping the corners of a
    # fractional-center window grows it by one snap step (1501 px tiles)
    xc = round(float(np.asarray(xs)[0]) / grids_snap()) * grids_snap()
    yc = round(float(np.asarray(ys)[0]) / grids_snap()) * grids_snap()
    h = side_m / 2.0
    window = grids.snap_window(xc - h, yc - h, xc + h, yc + h,
                               margin_m=margin_m)
    cx = [xc - h, xc + h, xc + h, xc - h]
    cy = [yc - h, yc - h, yc + h, yc + h]
    lons, lats = grids.utm_to_ll(cx, cy, epsg)
    ring = [[float(lo), float(la)] for lo, la in zip(lons, lats)]
    ring.append(ring[0])
    return window, epsg, ring
