"""Detectors on synthetic surfaces with known answers."""
import numpy as np

from dtm_atlas import config
from dtm_atlas.artifacts import detect_bridges, detect_curvature, detect_seams


def _plane(n=300, tilt=0.02):
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float64)
    return (xx * config.WORK_RES_M * tilt)


def test_curvature_flags_spike_not_plane():
    z = _plane()
    el = np.ones(z.shape, bool)
    z2 = z.copy()
    z2[150, 150] += 3.0                      # 3 m spike
    flag0, _ = detect_curvature(z, el)
    flag1, s1 = detect_curvature(z2, el)
    assert flag0.sum() == 0
    assert flag1[150, 150] and s1["curvature_cells"] >= 1


def test_curvature_spares_large_cliff_band():
    n = 700
    z = _plane(n)
    z[:, 350:] += 8.0                        # a long straight scarp (real terrain)
    el = np.ones(z.shape, bool)
    flag, stats = detect_curvature(z, el)
    # the scarp band is one huge connected component (> CURV_MAX_COMPONENT_PX)
    # -> logged as a cliff, NOT masked
    assert stats["cliff_components"] >= 1
    assert flag.sum() < n * 4


def test_seam_detector_finds_row_offset():
    rng = np.random.default_rng(1)
    z = _plane() + rng.normal(0, 0.05, (300, 300))
    z[150:, :] += 0.8                        # tile seam: constant offset
    el = np.ones(z.shape, bool)
    flag, stats = detect_seams(z, el)
    assert 149 in stats["seam_rows"] or 150 in stats["seam_rows"]
    assert not stats["seam_cols"]


def test_bridge_ribbon_over_water():
    z = np.zeros((200, 200))
    water = np.zeros((200, 200), bool)
    water[80:120, :] = True                  # a river band at z=0
    z[98:102, :] = 3.0                       # a deck crossing it, 4 px wide ribbon
    # the deck cells are NOT water (bridge over it)
    water[98:102, :] = False
    valid = np.ones_like(water)
    flag, stats = detect_bridges(z, water, valid)
    assert stats["bridge_components"] >= 1
    assert flag[100, 100]
