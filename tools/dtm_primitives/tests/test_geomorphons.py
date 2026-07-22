"""Geomorphons on analytic surfaces — the class-table orientation pin."""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from dtm_primitives import geomorphons as G  # noqa: E402

CELL = 10.0
N = 101  # 1 km


def _grid():
    ix = np.arange(N) * CELL
    x, y = np.meshgrid(ix, ix)
    return x, y


def test_plane_is_flat_and_gentle_slope_is_slope():
    x, y = _grid()
    flat = np.zeros((N, N))
    cls = G.classify(flat, CELL, lookup_m=300)
    c = cls[30:-30, 30:-30]
    assert (c == 1).mean() > 0.99          # flat
    tilted = 0.08 * x                       # 8% grade > 1 deg
    cls2 = G.classify(tilted, CELL, lookup_m=300)
    c2 = cls2[30:-30, 30:-30]
    assert (c2 == 6).mean() > 0.95         # slope


def test_cone_peak_and_pit():
    x, y = _grid()
    r = np.hypot(x - 500, y - 500)
    cone = 60.0 - 0.12 * r                  # peak at center
    cls = G.classify(cone, CELL, lookup_m=300)
    assert cls[50, 50] == 2                 # peak
    cls_inv = G.classify(-cone, CELL, lookup_m=300)
    assert cls_inv[50, 50] == 10            # pit


def test_tent_ridge_and_v_valley():
    x, y = _grid()
    tent = -0.15 * np.abs(y - 500)          # crest line along x at y=500
    cls = G.classify(tent, CELL, lookup_m=300)
    crest = cls[50, 30:-30]
    assert (crest == 3).mean() > 0.9        # ridge
    vee = 0.15 * np.abs(y - 500)            # V-valley floor along x
    cls2 = G.classify(vee, CELL, lookup_m=300)
    floor = cls2[50, 30:-30]
    assert (floor == 9).mean() > 0.9        # valley


def test_step_shoulder_footslope():
    x, y = _grid()
    step = 20.0 / (1.0 + np.exp(-(y - 500) / 40.0))   # smooth step rising north
    cls = G.classify(step, CELL, lookup_m=400)
    col = cls[:, 50]
    # above the break: shoulder-ish (locally high on one side); below: footslope-ish
    upper = col[62:72]
    lower = col[30:40]
    assert np.isin(upper, (3, 4, 5)).mean() > 0.5
    assert np.isin(lower, (7, 8, 9)).mean() > 0.5


def test_class_fractions_and_masks():
    x, y = _grid()
    tent = -0.15 * np.abs(y - 500)
    cls = G.classify(tent, CELL, lookup_m=300)
    fr = G.class_fractions(cls, collar_px=10)
    assert abs(sum(fr.values()) - 1.0) < 0.01
    assert G.ridge_mask(cls)[50, 50]
