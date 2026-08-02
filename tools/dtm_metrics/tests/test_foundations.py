"""Known-answer tests for the Stage-1 numeric foundations."""
import numpy as np

from dtm_metrics import specwin, surfaces, weights


def test_weighted_percentile_matches_numpy_on_uniform_weights():
    rng = np.random.default_rng(1)
    v = rng.normal(0, 5, 4000)
    w = np.ones_like(v)
    ours = weights.weighted_percentile(v, w, [5, 25, 50, 75, 95])
    ref = np.percentile(v, [5, 25, 50, 75, 95])
    assert np.abs(ours - ref).max() < 0.05


def test_weighted_percentile_respects_weights():
    v = np.array([0.0, 10.0])
    w = np.array([3.0, 1.0])
    med = weights.weighted_percentile(v, w, [50.0])[0]
    assert med < 5.0  # mass concentrated at 0


def test_weighted_moments_gaussian():
    rng = np.random.default_rng(2)
    v = rng.normal(3.0, 2.0, 20000)
    m = weights.weighted_moments(v, np.ones_like(v))
    assert abs(m["mean"] - 3.0) < 0.05
    assert abs(m["std"] - 2.0) < 0.05
    assert abs(m["skew"]) < 0.1
    assert abs(m["kurtosis"]) < 0.15


def test_parseval_band_closure_white_noise():
    """Sum of band variances over the full wavelength range ~= field variance."""
    rng = np.random.default_rng(3)
    n, cell = 256, 2.0
    x = rng.normal(0, 1.0, (n, n))
    P, f = specwin.window_periodogram(x, cell)
    # full range: everything above 2*cell (Nyquist) — one band
    total = specwin.band_variances(P, f, [(2 * cell, np.inf)])[0]
    # Hann tapering + detrend cost a bit of variance; ~within 15%
    assert 0.7 < total / x.var() < 1.1


def test_band_split_power_law_field():
    """A field with energy only above 64 m wavelength puts ~nothing in 8-32."""
    rng = np.random.default_rng(4)
    n, cell = 256, 2.0
    f1 = np.fft.fftfreq(n, d=cell)
    fmag = np.hypot(f1[:, None], f1[None, :])
    amp = np.where((fmag > 0) & (fmag < 1.0 / 64.0), 1.0, 0.0)
    spec = amp * np.exp(1j * rng.uniform(0, 2 * np.pi, (n, n)))
    x = np.fft.ifft2(spec).real
    x /= x.std()
    P, f = specwin.window_periodogram(x, cell)
    lo, hi = specwin.band_variances(P, f, [(8.0, 32.0), (64.0, np.inf)])
    assert hi > 20 * lo


def test_surface_split_identity_and_lowpass_dc():
    rng = np.random.default_rng(5)
    n = 200
    nat = rng.normal(100.0, 3.0, (n, n)).cumsum(axis=1) / 10 + 100
    valid = np.ones((n, n), bool)
    s = surfaces.build(nat, valid, cell=2.0, L=200.0, tpi_window_m=300.0)
    # identity: s1 = lowpass + s2 exactly
    assert np.allclose(s.s1, s.lowpass + s.s2, atol=1e-9)
    # the lowpass preserves the mean (DC)
    assert abs(s.lowpass.mean() - s.s1.mean()) < 1e-6
    # relief_pos is a proper rank in [0,1]
    assert 0.0 <= s.relief_pos[valid].min() and s.relief_pos[valid].max() <= 1.0


def test_lowpass_half_amplitude_at_L():
    """A pure sinusoid at wavelength L keeps ~half its amplitude in the lowpass."""
    n, cell, L = 512, 2.0, 200.0
    xx = np.arange(n) * cell
    nat = np.tile(np.sin(2 * np.pi * xx / L), (n, 1))
    s = surfaces.build(nat, np.ones((n, n), bool), cell, L, 300.0)
    interior = s.lowpass[n // 4: -n // 4, n // 4: -n // 4]
    ratio = interior.std() / nat[n // 4: -n // 4, n // 4: -n // 4].std()
    assert 0.42 < ratio < 0.58


def test_weight_map_margin_bonus():
    valid = np.ones((10, 10), bool)
    clean = valid.copy()
    ext = np.zeros((10, 10), bool)
    ext[:2] = True
    w = surfaces.weight_map(valid, clean, ext, exterior_inpaint_frac=0.0, bonus=1.0)
    assert w[0, 0] == 2.0 and w[5, 5] == 1.0
    w2 = surfaces.weight_map(valid, clean, ext, exterior_inpaint_frac=0.75, bonus=1.0)
    assert abs(w2[0, 0] - 1.25) < 1e-12
