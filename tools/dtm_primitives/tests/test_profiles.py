"""Analytic identities for the mirrored Stage-2 forms (these mirror the
in-crate tests of golf-landform/src/ease.rs)."""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from dtm_primitives import profiles as P  # noqa: E402


def test_smin_exact_outside_band():
    assert P.smin(0.0, 10.0, 2.0) == 0.0
    assert P.smin(10.0, 0.0, 2.0) == 0.0
    assert P.smin(1.0, 1.1, 2.0) < 1.0  # inside the band: pulled below both


def test_smin_c1_across_band():
    k = 1.0
    xs = np.arange(-2.0, 2.0, 0.01)
    d = (P.smin(xs + 1e-6, 0.0, k) - P.smin(xs - 1e-6, 0.0, k)) / 2e-6
    assert np.abs(np.diff(d)).max() < 0.05


def test_ramp_matches_asymptote_and_c1():
    m, r = 0.8, 4.0
    assert P.ramp(-1.0, m, r) == 0.0
    x = 10.0
    assert abs(P.ramp(x, m, r) - m * (x - 0.5 * r)) < 1e-12
    d_in = (P.ramp(r - 1e-7, m, r) - P.ramp(r - 2e-7, m, r)) / 1e-7
    d_out = (P.ramp(r + 2e-7, m, r) - P.ramp(r + 1e-7, m, r)) / 1e-7
    assert abs(d_in - d_out) < 1e-4


def test_smoothstep_is_cubic():
    # the ease.rs implementation is t^2(3-2t) — pin the midpoint value
    assert P.smoothstep(0.0, 1.0, 0.5) == 0.5
    assert P.smoothstep(0.0, 1.0, 0.25) == 0.25 * 0.25 * (3 - 0.5)


def test_end_taper_anchors_ends():
    assert P.end_taper(0.0, 0.2) == 0.0
    assert P.end_taper(1.0, 0.2) == 0.0
    assert P.end_taper(0.5, 0.2) == 1.0


def test_profile_sample_clamps_and_interps():
    knots = [(0.0, 10.0), (0.5, 18.0), (1.0, 26.0)]
    assert P.profile_sample(knots, -1.0) == 10.0
    assert P.profile_sample(knots, 2.0) == 26.0
    assert abs(P.profile_sample(knots, 0.25) - 14.0) < 1e-12


def test_polyline_project_straight():
    pts = np.array([[0.0, 0.0], [100.0, 0.0]])
    u, d, side, total = P.polyline_project(pts, [50.0, 25.0, -10.0],
                                           [5.0, -3.0, 0.0])
    assert total == 100.0
    assert np.allclose(u, [0.5, 0.25, 0.0])
    assert np.allclose(d, [5.0, 3.0, 10.0])
    assert side[0] > 0 and side[1] < 0  # +y is left of +x tangent


def test_valley_cross_model_shape():
    d = np.linspace(-120, 120, 241)
    z = P.valley_cross_model(d, zf=90.0, inc=12.0, gl=-0.01, gr=0.01,
                             hw=15.0, ml=0.4, mr=0.25, r=5.0, k=4.0)
    # floor flat at zf across |d| < hw − ε
    assert abs(z[120] - 90.0) < 1e-9          # d = 0
    assert abs(z[np.argmin(np.abs(d - 10))] - 90.0) < 1e-9
    # walls asymmetric: left (d>0) steeper -> reaches upland sooner
    left_arm = z[d > 0]
    right_arm = z[d < 0][::-1]
    assert left_arm[60] > right_arm[60]
    # far field ~ upland trend
    assert abs(z[-1] - (102.0 + 0.01 * 120.0)) < 0.75


def test_bluff_cross_model():
    d = np.linspace(-100, 100, 201)
    z = P.bluff_cross_model(d, base=50.0, g=0.0, h=10.0, face_grad=0.5,
                            sign=1.0)
    assert abs(z[0] - 50.0) < 1e-9
    assert abs(z[-1] - 60.0) < 1e-9
    mid = z[100]
    assert abs(mid - 55.0) < 1e-9  # smoothstep midpoint
