"""Analytic-surface tests for the v2 battery additions (workplan B4).

Same philosophy as test_metrics.py: every metric is validated on surfaces
whose right answer is known by construction, so a regression is a broken
kernel, never a changed opinion.
"""
import os
import sys

import numpy as np
import pytest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from metrics import core  # noqa: E402

CELL = 2.0
N = 300
THR = 6e4  # accum area threshold, m^2


def _tilted_valley():
    """A tilted plane with a straight central valley: one integrated channel
    draining to the south edge."""
    y, x = np.mgrid[0:N, 0:N].astype(np.float64)
    z = 0.02 * CELL * y                      # falls toward row 0 (south)
    z += 0.001 * CELL * np.abs(x - N / 2)    # gentle V toward the centreline
    return z


def _integrated():
    """A genuinely integrated dendritic surface: fill a rough tilted surface
    (fluvial integration by definition -- zero closed depressions), plus an
    epsilon tilt so the filled flats drain deterministically under D8."""
    from scipy import ndimage

    rng = np.random.default_rng(3)
    y, _x = np.mgrid[0:N, 0:N].astype(np.float64)
    rough = 0.01 * CELL * y + ndimage.gaussian_filter(
        rng.standard_normal((N, N)), 3.0) * 3.0
    return core.fill_depressions(rough, CELL, None) + 1e-6 * y


def _with_bowls(z, depth, seed=4, count=12, r=10.0):
    y, x = np.mgrid[0:N, 0:N].astype(np.float64)
    rng = np.random.default_rng(seed)
    out = z.copy()
    for _ in range(count):
        cx, cy = rng.uniform(40, N - 40, 2)
        out -= depth * np.exp(-((x - cx) ** 2 + (y - cy) ** 2) / (2 * r * r))
    return out


def _two_plane():
    """Half flat, half steep — cleanly bimodal in slope and elevation."""
    y, _x = np.mgrid[0:N, 0:N].astype(np.float64)
    z = np.where(y < N / 2, 0.001 * CELL * y, 0.001 * CELL * (N / 2) + 0.2 * CELL * (y - N / 2))
    return z


def _fbm_like(seed=0):
    """Smooth unimodal random surface (band-limited noise)."""
    rng = np.random.default_rng(seed)
    from scipy import ndimage

    z = ndimage.gaussian_filter(rng.standard_normal((N, N)), 8.0)
    return z * 10.0


# ------------------------------------------------------- network_connectivity

THR_NET = 2e4  # lower threshold so the dendritic surfaces carry a network


def test_connectivity_is_one_on_integrated_surface():
    c = core.network_connectivity(_integrated(), CELL, None, THR_NET)
    assert c == pytest.approx(1.0)


def test_connectivity_drops_on_a_kettle_field():
    # Deep closed basins swallow the network -- the heathland signature.
    c = core.network_connectivity(_with_bowls(_integrated(), 8.0), CELL, None, THR_NET)
    assert np.isfinite(c)
    assert c < 0.6, c


def test_connectivity_ignores_shallow_depressions():
    # Depressions under min_pit_depth_m (lidar-noise scale) must NOT read as
    # derangement -- the tolerance is the point.
    c = core.network_connectivity(_with_bowls(_integrated(), 0.3, seed=5), CELL,
                                  None, THR_NET)
    assert c == pytest.approx(1.0)


def test_connectivity_nan_with_no_channels():
    # A cone sheds flow radially; with a huge threshold there is no channel.
    y, x = np.mgrid[0:N, 0:N].astype(np.float64)
    cone = -np.hypot(x - N / 2, y - N / 2) * CELL * 0.05
    c = core.network_connectivity(cone, CELL, None, 1e12)
    assert np.isnan(c)


# ------------------------------------------------------------- bimodalities

def test_slope_bimodality_fires_on_two_planes_not_noise():
    d_planes = core.slope_bimodality(_two_plane(), CELL)
    d_noise = core.slope_bimodality(_fbm_like(), CELL)
    assert d_planes > 2.0, d_planes
    assert d_planes > d_noise


def test_hypsometric_bimodality_fires_on_floor_plus_terrace():
    # Floor at 0, terrace at 20 m, thin ramp between.
    y, _x = np.mgrid[0:N, 0:N].astype(np.float64)
    z = np.where(y < N * 0.45, 0.0, np.where(y > N * 0.55, 20.0, (y - N * 0.45) * 2.0))
    z = z + _fbm_like(1) * 0.02  # a whisper of texture so variances exist
    d_terr = core.hypsometric_bimodality(z, CELL)
    d_noise = core.hypsometric_bimodality(_fbm_like(2), CELL)
    assert d_terr > 2.0, d_terr
    assert d_terr > d_noise


def test_bimodality_is_deterministic():
    z = _two_plane()
    assert core.slope_bimodality(z, CELL) == core.slope_bimodality(z, CELL)


# ------------------------------------------------------------- horton_ratios

def test_horton_on_degenerate_single_stream_is_nan():
    # A plane with one carved straight ditch, exactly ONE column wide, so the
    # channel is a single order-1 thread (a Gaussian ditch is two cells wide
    # and legitimately makes order 2 out of the paired threads).
    y, _x = np.mgrid[0:N, 0:N].astype(np.float64)
    z = 0.02 * CELL * y
    z[:, N // 2] -= 5.0
    h = core.horton_ratios(z, CELL, None, THR)
    assert h["strahler_max"] <= 1 or np.isnan(h["strahler_max"])
    # One order ⇒ ratios are undefined, loudly NaN.
    assert np.isnan(h["horton_bifurcation_ratio"])


def test_horton_on_dendritic_surface_is_plausible():
    # A rough tilted surface develops a genuine dendritic hierarchy under D8.
    rng = np.random.default_rng(3)
    from scipy import ndimage

    y, _x = np.mgrid[0:N, 0:N].astype(np.float64)
    z = 0.01 * CELL * y + ndimage.gaussian_filter(rng.standard_normal((N, N)), 3.0) * 3.0
    h = core.horton_ratios(z, CELL, None, 2e4)
    assert h["strahler_max"] >= 2
    # Real-world clustering: bifurcation 3-5 typically; accept a generous band
    # for a synthetic surface, but reject the degenerate extremes.
    assert 1.5 < h["horton_bifurcation_ratio"] < 8.0, h
    if np.isfinite(h["horton_length_ratio"]):
        assert 1.05 < h["horton_length_ratio"] < 6.0, h


# ------------------------------------------------------------- features wire

def test_features_carries_the_new_columns():
    import yaml
    from metrics import features

    cfg = yaml.safe_load(open(os.path.join(os.path.dirname(__file__), "..",
                                           "metrics", "config.yaml")))
    axes = features.build_axes(cfg)
    z = _tilted_valley()
    scalars, _ = features.compute(z, CELL, np.ones_like(z, bool), cfg, axes)
    for k in ["network_connectivity", "hypsometric_bimodality", "slope_bimodality",
              "horton_bifurcation_ratio", "horton_length_ratio", "strahler_max"]:
        assert k in scalars, k
        assert k in features.SCALAR_COLUMNS, k
