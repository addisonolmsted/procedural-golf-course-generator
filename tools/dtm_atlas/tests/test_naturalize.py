"""Laplace fill: analytic recovery, invariants, determinism."""
import numpy as np

from dtm_atlas.naturalize import laplace_fill


def test_fills_hole_in_plane_exactly():
    # a plane is harmonic: the membrane fill must reproduce it (to solver tol)
    yy, xx = np.mgrid[0:80, 0:80].astype(np.float64)
    z = 0.05 * xx + 0.02 * yy
    truth = z.copy()
    inp = np.zeros(z.shape, bool)
    inp[30:50, 30:50] = True
    z[inp] = 999.0                            # corrupt the hole
    valid = np.ones(z.shape, bool)
    out, stats = laplace_fill(z, inp, valid)
    assert np.abs(out[inp] - truth[inp]).max() < 1e-6
    assert stats["components"] == 1


def test_untouched_outside_mask_and_range_bounded():
    rng = np.random.default_rng(7)
    z = rng.normal(0, 1.0, (100, 100))
    inp = np.zeros(z.shape, bool)
    inp[10:30, 60:90] = True
    inp[70:80, 10:20] = True
    valid = np.ones(z.shape, bool)
    out, stats = laplace_fill(z, inp, valid)
    assert (out[~inp] == z[~inp]).all()       # bit-exact outside
    assert stats["components"] == 2
    # maximum principle: filled values inside the surrounding data range
    assert out[inp].max() <= z[~inp].max() + 1e-6
    assert out[inp].min() >= z[~inp].min() - 1e-6


def test_deterministic():
    rng = np.random.default_rng(3)
    z = rng.normal(0, 1.0, (60, 60))
    inp = np.zeros(z.shape, bool)
    inp[20:40, 20:40] = True
    valid = np.ones(z.shape, bool)
    a, _ = laplace_fill(z, inp, valid)
    b, _ = laplace_fill(z, inp, valid)
    assert (a == b).all()


def test_nodata_edge_uses_neumann():
    # hole touching a nodata band: solve must not crash and must stay bounded
    yy, xx = np.mgrid[0:60, 0:60].astype(np.float64)
    z = 0.1 * xx
    valid = np.ones(z.shape, bool)
    valid[:, :5] = False                      # nodata strip on the west edge
    inp = np.zeros(z.shape, bool)
    inp[20:40, 5:20] = True                   # hole adjacent to the strip
    out, _ = laplace_fill(z, inp, valid)
    ring = z[~inp & valid]
    assert out[inp].max() <= ring.max() + 1e-6
    assert out[inp].min() >= ring.min() - 1e-6
