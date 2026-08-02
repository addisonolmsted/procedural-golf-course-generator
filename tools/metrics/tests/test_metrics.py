"""Synthetic-surface validation of metrics.core.

Each test builds a surface with a known answer and asserts recovery within
tolerance — this is how we trust the numbers before running on real courses.
Also verifies the real/synthetic contract: every core function takes a plain
array and returns a scalar or a fixed-shape vector (no geo types).
"""
import numpy as np
import pytest

from metrics import core

CS = 5.0          # metres/cell
N = 256


def tilted_plane(tilt_deg=0.0, axis="x", n=N):
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float64)
    slope = np.tan(np.radians(tilt_deg))
    coord = xx if axis == "x" else yy
    return (coord * CS * slope)


def cone_on_disk(n=N, height=100.0):
    """z = height*(1 - r/R) on the inscribed disk; masked outside. HI == 1/3."""
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float64)
    cy = cx = (n - 1) / 2.0
    r = np.hypot(yy - cy, xx - cx)
    R = (n - 1) / 2.0
    z = height * np.clip(1.0 - r / R, 0.0, None)
    mask = r <= R
    return z, mask


def sinusoid(wavelength_m=60.0, n=N):
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float64)
    return np.sin(2.0 * np.pi * (xx * CS) / wavelength_m)


def fbm_beta(beta, n=N, seed=7):
    """Spectral synthesis of a surface with PSD ~ k^-beta (H = (beta-2)/2)."""
    rng = np.random.default_rng(seed)
    ky = np.fft.fftfreq(n)[:, None]
    kx = np.fft.fftfreq(n)[None, :]
    k = np.hypot(kx, ky)
    k[0, 0] = 1.0
    amp = k ** (-beta / 2.0)
    amp[0, 0] = 0.0
    phase = rng.uniform(0, 2 * np.pi, (n, n))
    spec = amp * np.exp(1j * phase)
    z = np.fft.ifft2(spec).real
    return (z - z.mean()) / z.std()


# --------------------------------------------------------------------------

def test_flat_plane():
    z = np.zeros((N, N))
    assert core.relief_p95_p5(z, CS) == pytest.approx(0.0, abs=1e-9)
    assert core.slope_stats(z, CS)["slope_mean"] == pytest.approx(0.0, abs=1e-6)
    r = core.rms_roughness(z, CS, 100.0)
    assert (np.isnan(r) or r == pytest.approx(0.0, abs=1e-9))
    # degenerate HI handled gracefully
    assert core.hypsometric_integral(z, CS) == pytest.approx(0.5)


def test_tilted_plane_slope_matches_tilt():
    for t in (5.0, 12.0, 25.0):
        z = tilted_plane(t)
        st = core.slope_stats(z, CS)
        assert st["slope_median"] == pytest.approx(t, abs=0.5)
        # a plane is flat after detrend -> roughness ~ 0
        assert core.rms_roughness(z, CS, 120.0) < 1e-6
        # plane hypsometric integral ~ 0.5
        assert core.hypsometric_integral(z, CS) == pytest.approx(0.5, abs=0.05)


def test_cone_hypsometric_integral():
    z, mask = cone_on_disk()
    assert core.hypsometric_integral(z, CS, mask) == pytest.approx(1.0 / 3.0, abs=0.03)
    # radially symmetric -> slope nearly constant (small CV)
    s = core.slope_deg(z, CS, mask)[mask]
    assert s.std() / s.mean() < 0.25


def test_sinusoid_psd_peak_and_variogram():
    lam = 60.0
    z = sinusoid(lam)
    wl, psd, kc = core.radial_psd(z, CS)
    good = np.isfinite(psd) & (kc > 0)
    peak_k = kc[good][np.argmax(psd[good])]
    assert (1.0 / peak_k) == pytest.approx(lam, rel=0.25)
    # variogram of a sinusoid first peaks near half a wavelength
    lags = np.linspace(0, 150, 40)
    g = core.variogram(z, CS, lags)
    first_peak = lags[1:][np.argmax(g[1:])]
    assert first_peak == pytest.approx(lam / 2.0, rel=0.4)


def test_fbm_spectral_slope_recovered():
    band = [40.0, 400.0]
    for beta_true in (2.5, 3.0, 3.5):
        z = fbm_beta(beta_true)
        wl, psd, kc = core.radial_psd(z, CS)
        beta = core.spectral_slope_beta(kc, psd, band)
        assert beta == pytest.approx(beta_true, abs=0.5)


def test_water_mask_excluded():
    # a plane with a spurious spike inside a masked "water" patch must not
    # affect slope stats when masked out.
    z = tilted_plane(8.0)
    mask = np.ones((N, N), bool)
    z[100:120, 100:120] = 9999.0
    mask[100:120, 100:120] = False
    st = core.slope_stats(z, CS, mask)
    assert st["slope_median"] == pytest.approx(8.0, abs=0.6)


def test_hydrology_runs_and_channels_form():
    # fBm terrain should yield a positive drainage density and a plausible theta
    z = fbm_beta(3.0) * 40.0
    h = core.hydrology_metrics(z, CS, accum_area_threshold_m2=5e4)
    assert h["drainage_density"] > 0
    assert np.isfinite(h["valley_spacing"])


def test_anisotropy_detects_directional_stripes():
    # elongated features along x (long correlation in x) -> ratio > 1
    yy, xx = np.mgrid[0:N, 0:N].astype(float)
    z = np.sin(2 * np.pi * (yy * CS) / 40.0)  # varies fast in y, constant in x
    a = core.anisotropy(z, CS, max_lag_m=200.0)
    assert a["anisotropy_ratio"] >= 1.0


def test_contract_shapes_and_types():
    """Every distribution fn returns a fixed-length vector; scalars are floats."""
    z = fbm_beta(3.0) * 20.0
    fr = np.linspace(0, 1, 21)
    assert core.hypsometric_curve(z, CS, fr).shape == (21,)
    edges = np.linspace(0, 45, 31)
    assert core.slope_hist(z, CS, edges).shape == (30,)
    lags = np.linspace(0, 400, 40)
    assert core.variogram(z, CS, lags).shape == (40,)
    wl, psd, kc = core.radial_psd(z, CS, n_bins=48)
    assert psd.shape == (48,) and kc.shape == (48,)
    assert isinstance(core.relief_p95_p5(z, CS), float)
    assert isinstance(core.hypsometric_integral(z, CS), float)


# ------------------------- landform organization ---------------------------

def test_local_std_field_matches_rms_roughness():
    """The refactor contract: rms_roughness == nanmean of local_std_field."""
    z = fbm_beta(3.0) * 20.0
    field, valid = core.local_std_field(z, CS, 100.0)
    good = np.isfinite(field) & valid
    assert core.rms_roughness(z, CS, 100.0) == pytest.approx(
        float(field[good].mean()), rel=1e-12)


def test_roughness_organization_concentrated_vs_uniform():
    """Noise confined to one quadrant -> high cv/top10, low-ish moran vs the
    same noise spread uniformly."""
    rng = np.random.default_rng(3)
    base = np.zeros((N, N))
    quad = base.copy()
    quad[: N // 2, : N // 2] = rng.normal(0, 1.0, (N // 2, N // 2))
    uni = rng.normal(0, 0.5, (N, N))
    oq = core.roughness_organization(quad, CS)
    ou = core.roughness_organization(uni, CS)
    assert oq["rough_cv"] > ou["rough_cv"] * 1.5
    assert oq["rough_top10"] > ou["rough_top10"]
    # uniform white noise has no spatial structure in its roughness field
    assert abs(ou["rough_moran"]) < 0.25
    # quadrant structure IS block-scale structure
    assert oq["rough_block_ratio"] > ou["rough_block_ratio"]


def test_extrema_density_counts_bumps():
    """A grid of well-separated gaussian bumps -> ~one extremum per bump;
    a flat plane -> zero."""
    n = N
    z = np.zeros((n, n))
    yy, xx = np.mgrid[0:n, 0:n].astype(np.float64)
    k = 4  # 4x4 bumps
    step = n // k
    for i in range(k):
        for j in range(k):
            cy, cx = (i + 0.5) * step, (j + 0.5) * step
            z += 5.0 * np.exp(-((yy - cy) ** 2 + (xx - cx) ** 2) / (2 * 8.0 ** 2))
    dens = core.extrema_density(z, CS, sigma_m=12.0)
    ha = n * n * CS * CS / 1e4
    n_ext = dens * ha
    # 16 peaks (+ shallow saddle pits between bumps may add a few)
    assert 16 <= n_ext <= 40
    assert core.extrema_density(np.zeros((n, n)), CS) == pytest.approx(0.0)


def test_tpi_valleys_count_and_elongation():
    """Parallel sine valleys -> valley count == wave count, high elongation."""
    lam = 320.0  # 64 cells at CS=5 -> 4 full waves across 256
    z = 10.0 * sinusoid(lam)
    t = core.tpi_landforms(z, CS, radius_m=60.0, k=0.7, min_area_m2=900.0)
    km2 = N * N * CS * CS / 1e6
    n_valleys = t["valley_count_km2"] * km2
    assert 3 <= n_valleys <= 6  # 4 troughs (edge troughs may clip)
    assert t["valley_elong"] > 4.0  # full-height stripes are very elongated
    assert t["valley_len_p90"] > 0.5 * N * CS


def test_landform_metrics_nan_safe_on_flat():
    z = np.zeros((N, N))
    ro = core.roughness_organization(z, CS)
    for v in ro.values():
        assert np.isnan(v) or np.isfinite(v)
    t = core.tpi_landforms(z, CS, 60.0)
    assert t["valley_count_km2"] == pytest.approx(0.0)
    h = core.hydrology_metrics(z, CS)
    assert "chan_len_p90" in h


def test_chan_len_p90_present_on_fbm():
    z = fbm_beta(3.0) * 40.0
    h = core.hydrology_metrics(z, CS, accum_area_threshold_m2=5e4)
    assert np.isfinite(h["chan_len_p90"]) and h["chan_len_p90"] > 0
