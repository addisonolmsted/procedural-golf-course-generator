"""Frame conventions: rc<->local round trips and sampler identities."""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from dtm_primitives import frame  # noqa: E402


def test_rc_local_roundtrip():
    cell, nrows = 2.0, 100
    rows = np.array([0, 50, 99])
    cols = np.array([0, 10, 40])
    x, y = frame.rc_to_local(rows, cols, cell, nrows)
    # row 0 = north = largest y
    assert y[0] > y[2]
    r2, c2 = frame.local_to_rc(x, y, cell, nrows)
    assert np.allclose(r2, rows)
    assert np.allclose(c2, cols)


def test_bilinear_tif_at_centers():
    cell = 2.0
    arr = np.arange(20.0).reshape(4, 5)
    rows, cols = np.meshgrid(np.arange(4), np.arange(5), indexing="ij")
    x, y = frame.rc_to_local(rows.ravel(), cols.ravel(), cell, 4)
    v = frame.bilinear_tif(arr, x, y, cell)
    assert np.allclose(v, arr.ravel())


def test_bilinear_outside_nan():
    arr = np.ones((4, 5))
    v = frame.bilinear_tif(arr, [-5.0, 100.0], [1.0, 1.0], 2.0)
    assert np.isnan(v).all()


def test_hg01_convention():
    # h row0=south: h[y, x] at world (cell/2 + x*cell, cell/2 + y*cell)
    cell = 2.0
    h = np.arange(12.0).reshape(3, 4)  # y rows south-first
    v = frame.bilinear_hg01(h, cell, [1.0, 3.0], [1.0, 5.0])
    assert v[0] == h[0, 0]
    assert v[1] == h[2, 1]


def test_nearest_tif_masks():
    arr = np.zeros((4, 4), np.uint8)
    arr[0, 3] = 1  # north-east corner
    cell = 1.0
    # local coords of row 0, col 3: x=3.5, y=3.5
    v = frame.nearest_tif(arr, [3.5, 0.5], [3.5, 0.5], cell)
    assert v[0] == 1 and v[1] == 0
