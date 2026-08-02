"""Stage-0T tile ingestion: dataset switch + synthetic square boundary."""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from dtm_atlas import config, grids, tiles  # noqa: E402


def test_select_dataset_switches_roots():
    try:
        config.select_dataset("tiles")
        assert config.STORE.endswith("store_tiles")
        assert config.QA.endswith("qa_tiles")
        assert config.DATASET == "tiles"
    finally:
        config.select_dataset("courses")
    assert config.STORE.endswith(os.path.join("out", "store"))
    assert config.QA.endswith(os.path.join("out", "qa"))


def test_dataset_paths_excluded_from_params_hash():
    h0 = config.params_hash()
    try:
        config.select_dataset("tiles")
        h1 = config.params_hash()
    finally:
        config.select_dataset("courses")
    assert h0 == h1  # switching datasets must not look like a param change


def test_tile_window_exact_side():
    # sandhills-ish center; margin 0 -> window is exactly the snapped side
    window, epsg, ring = tiles.tile_window_and_ring(
        35.19, -79.47, config.TILE_SIDE_M, 0.0)
    x0, y0, x1, y1 = window
    assert 26900 < epsg < 27000
    assert abs((x1 - x0) - config.TILE_SIDE_M) <= config.SNAP_M
    assert abs((y1 - y0) - config.TILE_SIDE_M) <= config.SNAP_M
    assert x0 % config.SNAP_M == 0 and y0 % config.SNAP_M == 0


def test_tile_ring_closed_and_centered():
    lat, lon = 35.19, -79.47
    window, epsg, ring = tiles.tile_window_and_ring(lat, lon, 3000.0, 0.0)
    assert len(ring) == 5 and ring[0] == ring[-1]
    lons = [p[0] for p in ring[:4]]
    lats = [p[1] for p in ring[:4]]
    assert abs(float(np.mean(lons)) - lon) < 1e-3
    assert abs(float(np.mean(lats)) - lat) < 1e-3
    # ring spans ~3 km in both axes
    xs, ys = grids.ll_to_utm(lons, lats, epsg)
    assert abs((max(xs) - min(xs)) - 3000.0) < 1.0
    assert abs((max(ys) - min(ys)) - 3000.0) < 1.0


def test_margin_variant_grows_window():
    w0, _e, _r = tiles.tile_window_and_ring(35.19, -79.47, 3000.0, 0.0)
    w5, _e, _r = tiles.tile_window_and_ring(35.19, -79.47, 3000.0, 500.0)
    assert abs((w5[2] - w5[0]) - ((w0[2] - w0[0]) + 1000.0)) <= 2 * config.SNAP_M
