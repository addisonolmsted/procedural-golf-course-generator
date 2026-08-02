"""Pure terrain metrics.

Every public function takes a plain elevation array + cell size (+ an optional
boolean validity mask) and returns a scalar or a 1-D distribution vector. There
is NO file I/O, no raster/geo type, and no "course" concept in here — that is
exactly what lets the SAME functions score the procedural generator's own
output (see ``synthetic.py``). Distribution functions take their shared axis as
an argument so the vectors are directly comparable across courses.

Conventions
-----------
* ``height``: 2-D float array, north-up not required (metrics are orientation
  aware only where noted). NaNs are not expected — mask instead.
* ``cell_size``: metres per cell (square cells; callers resample to square).
* ``mask``: bool array, ``True`` = VALID (land, in-bounds). ``None`` = all valid.
  Water / out-of-bounds cells are excluded from aggregate statistics; for
  operations that need a filled array (gradient, FFT) masked cells are filled
  from their nearest valid neighbour (elevation) or set to 0 (detrended
  residual), which is stated per function.
* Angles in degrees; curvature in 1/m; wavenumber in cycles/m; wavelength in m.
"""

from __future__ import annotations

import heapq
import numpy as np
from scipy import ndimage, optimize

SQRT2 = np.sqrt(2.0)


# ---------------------------------------------------------------------------
# helpers
# ---------------------------------------------------------------------------

def _validmask(height, mask):
    if mask is None:
        return np.ones(height.shape, bool)
    return mask.astype(bool)


def _fill_nearest(height, valid):
    """Fill invalid cells from the nearest valid neighbour (for gradient/FFT)."""
    if valid.all():
        return height
    idx = ndimage.distance_transform_edt(~valid, return_distances=False,
                                          return_indices=True)
    return height[tuple(idx)]


def detrend(height, cell_size, mask=None, method="quadratic"):
    """Remove a low-order polynomial trend fit on the valid cells.

    Returns ``(residual, trend)``. Used before spectrum / variogram / roughness
    so they capture local texture, not the regional tilt. ``planar`` leaves
    curvature untouched; ``quadratic`` also removes the mean curvature.
    """
    valid = _validmask(height, mask)
    ny, nx = height.shape
    yy, xx = np.mgrid[0:ny, 0:nx].astype(np.float64)
    xc = (xx - xx.mean()) / max(nx, 1)
    yc = (yy - yy.mean()) / max(ny, 1)
    if method == "planar":
        cols = [np.ones_like(xc), xc, yc]
    else:
        cols = [np.ones_like(xc), xc, yc, xc * xc, xc * yc, yc * yc]
    A = np.stack([c.ravel() for c in cols], axis=1)
    v = valid.ravel()
    if v.sum() < A.shape[1] + 1:
        return height - np.nanmean(height[valid]), np.full_like(height, np.nanmean(height[valid]))
    coef, *_ = np.linalg.lstsq(A[v], height.ravel()[v].astype(np.float64), rcond=None)
    trend = (A @ coef).reshape(height.shape)
    return height - trend, trend


def _gradient(height, cell_size, mask=None):
    """(gy, gx) via central differences on the nearest-filled surface."""
    valid = _validmask(height, mask)
    filled = _fill_nearest(height, valid)
    gy, gx = np.gradient(filled.astype(np.float64), cell_size)
    return gy, gx


# ---------------------------------------------------------------------------
# A. elevation & relief  (raw surface)
# ---------------------------------------------------------------------------

def relief_p95_p5(height, cell_size, mask=None):
    v = height[_validmask(height, mask)]
    if v.size < 2:
        return float("nan")
    return float(np.percentile(v, 95) - np.percentile(v, 5))


def elev_moments(height, cell_size, mask=None):
    """dict(std, skew, kurtosis) of the elevation distribution (excess kurt)."""
    from scipy import stats
    v = height[_validmask(height, mask)].astype(np.float64)
    if v.size < 4:
        return {"elev_std": float("nan"), "elev_skew": float("nan"), "elev_kurtosis": float("nan")}
    return {"elev_std": float(v.std()),
            "elev_skew": float(stats.skew(v)),
            "elev_kurtosis": float(stats.kurtosis(v))}


def hypsometric_integral(height, cell_size, mask=None):
    v = height[_validmask(height, mask)]
    if v.size < 2:
        return float("nan")
    lo, hi = float(v.min()), float(v.max())
    if hi - lo < 1e-9:
        return 0.5
    return float((v.mean() - lo) / (hi - lo))


def hypsometric_curve(height, cell_size, fractions, mask=None):
    """Normalized area-above-elevation at the shared ``fractions`` in [0,1].

    Curve[i] = fraction of land area with normalized elevation >= fractions[i].
    Monotone decreasing 1..0; its integral is the hypsometric integral.
    """
    v = height[_validmask(height, mask)]
    fr = np.asarray(fractions, float)
    if v.size < 2:
        return np.full(fr.shape, np.nan)
    lo, hi = float(v.min()), float(v.max())
    if hi - lo < 1e-9:
        return np.where(fr <= 0.5, 1.0, 0.0)
    hn = (v - lo) / (hi - lo)
    return np.array([float(np.mean(hn >= a)) for a in fr])


# ---------------------------------------------------------------------------
# B. slope  (raw surface)
# ---------------------------------------------------------------------------

def slope_deg(height, cell_size, mask=None):
    """Slope in degrees per cell (nearest-filled gradient)."""
    gy, gx = _gradient(height, cell_size, mask)
    return np.degrees(np.arctan(np.hypot(gx, gy)))


def slope_stats(height, cell_size, mask=None, flat_deg=3.0, steep_deg=15.0):
    from scipy import stats
    s = slope_deg(height, cell_size, mask)[_validmask(height, mask)]
    if s.size < 2:
        return {k: float("nan") for k in
                ("slope_mean", "slope_median", "slope_p90", "slope_p95",
                 "slope_skew", "frac_flat", "frac_steep")}
    return {"slope_mean": float(s.mean()),
            "slope_median": float(np.median(s)),
            "slope_p90": float(np.percentile(s, 90)),
            "slope_p95": float(np.percentile(s, 95)),
            "slope_skew": float(stats.skew(s)),
            "frac_flat": float(np.mean(s < flat_deg)),
            "frac_steep": float(np.mean(s > steep_deg))}


def slope_hist(height, cell_size, bin_edges_deg, mask=None):
    """Normalized (density=area-fraction) slope histogram over shared bins."""
    s = slope_deg(height, cell_size, mask)[_validmask(height, mask)]
    edges = np.asarray(bin_edges_deg, float)
    if s.size == 0:
        return np.full(len(edges) - 1, np.nan)
    h, _ = np.histogram(np.clip(s, edges[0], edges[-1] - 1e-9), bins=edges)
    tot = h.sum()
    return (h / tot).astype(np.float64) if tot else h.astype(np.float64)


# ---------------------------------------------------------------------------
# C. spectrum & multiscale roughness  (detrended)
# ---------------------------------------------------------------------------

def radial_psd(height, cell_size, mask=None, detrend_method="quadratic", n_bins=48):
    """Radially-averaged power spectral density.

    Returns ``(wavelength_m, power)`` where wavelength = 1/k (descending k), on
    the array's native wavenumber support. A Hann window and quadratic detrend
    reduce edge leakage / regional tilt. Masked residual cells set to 0.
    """
    valid = _validmask(height, mask)
    resid, _ = detrend(height, cell_size, mask, detrend_method)
    resid = np.where(valid, resid, 0.0)
    ny, nx = resid.shape
    wy = np.hanning(ny)[:, None]
    wx = np.hanning(nx)[None, :]
    win = wy * wx
    resid = resid * win
    F = np.fft.fftshift(np.fft.fft2(resid))
    power = (np.abs(F) ** 2) / (np.sum(win ** 2) + 1e-12)
    ky = np.fft.fftshift(np.fft.fftfreq(ny, d=cell_size))[:, None]
    kx = np.fft.fftshift(np.fft.fftfreq(nx, d=cell_size))[None, :]
    kr = np.hypot(np.ones_like(ky) * kx, ky * np.ones_like(kx))
    kmax = min(np.abs(kx).max(), np.abs(ky).max())
    kbins = np.linspace(0, kmax, n_bins + 1)
    which = np.digitize(kr.ravel(), kbins) - 1
    p = power.ravel()
    psd = np.array([p[which == i].mean() if np.any(which == i) else np.nan
                    for i in range(n_bins)])
    kc = 0.5 * (kbins[:-1] + kbins[1:])
    ok = kc > 0
    wl = np.full_like(kc, np.nan)
    wl[ok] = 1.0 / kc[ok]
    return wl, psd, kc


def resample_psd(wavelength_native, psd_native, kc_native, wavelength_axis):
    """Interpolate a native PSD onto the shared wavelength axis (log-log)."""
    wl = np.asarray(wavelength_native)
    good = np.isfinite(wl) & np.isfinite(psd_native) & (psd_native > 0)
    out = np.full(len(wavelength_axis), np.nan)
    if good.sum() < 2:
        return out
    order = np.argsort(wl[good])
    lx = np.log10(wl[good][order])
    ly = np.log10(psd_native[good][order])
    q = np.log10(np.asarray(wavelength_axis, float))
    inside = (q >= lx.min()) & (q <= lx.max())
    out[inside] = 10.0 ** np.interp(q[inside], lx, ly)
    return out


def spectral_slope_beta(kc, psd, band_wavelength_m):
    """PSD ~ k^-beta; fit beta over the mid-band (given as a wavelength range)."""
    k_hi = 1.0 / band_wavelength_m[0]   # short wavelength -> high k
    k_lo = 1.0 / band_wavelength_m[1]
    good = np.isfinite(psd) & (psd > 0) & (kc >= k_lo) & (kc <= k_hi)
    if good.sum() < 3:
        return float("nan")
    coef = np.polyfit(np.log10(kc[good]), np.log10(psd[good]), 1)
    return float(-coef[0])


def highfreq_energy_frac(kc, psd, cutoff_wavelength_m, reliable_wavelength_min_m):
    """Fraction of (reliable-band) spectral power at wavelength < cutoff.

    Power is area-weighted by the annulus 2*pi*k*dk of each radial bin.
    """
    k_cut = 1.0 / cutoff_wavelength_m
    k_max = 1.0 / reliable_wavelength_min_m
    good = np.isfinite(psd) & (psd > 0) & (kc > 0) & (kc <= k_max)
    if good.sum() < 2:
        return float("nan")
    k = kc[good]
    w = psd[good] * (2.0 * np.pi * k)      # radial power density weight
    tot = w.sum()
    if tot <= 0:
        return float("nan")
    return float(w[k >= k_cut].sum() / tot)


def variogram(height, cell_size, lags_m, mask=None, detrend_method="quadratic"):
    """Omnidirectional semivariance gamma(lag) at the shared lags.

    Directional-averaged over axis + diagonal offsets chosen to match each
    physical lag. Detrended surface; only valid–valid pairs counted.
    """
    valid = _validmask(height, mask)
    resid, _ = detrend(height, cell_size, mask, detrend_method)
    resid = np.where(valid, resid, np.nan)
    out = np.full(len(lags_m), np.nan)
    for i, lag in enumerate(lags_m):
        if lag <= 0:
            out[i] = 0.0
            continue
        da = max(1, int(round(lag / cell_size)))
        dd = max(1, int(round(lag / (cell_size * SQRT2))))
        sq = []
        for (dy, dx) in [(0, da), (da, 0), (dd, dd), (dd, -dd)]:
            a, b = _shift_pair(resid, dy, dx)
            d = a - b
            d = d[np.isfinite(d)]
            if d.size:
                sq.append(d * d)
        if sq:
            allsq = np.concatenate(sq)
            out[i] = 0.5 * float(np.mean(allsq)) if allsq.size else np.nan
    return out


def _shift_pair(arr, dy, dx):
    """Overlapping views of arr and arr shifted by (dy,dx)."""
    ny, nx = arr.shape
    y0a, y1a = max(0, dy), ny - max(0, -dy)
    x0a, x1a = max(0, dx), nx - max(0, -dx)
    a = arr[y0a:y1a, x0a:x1a]
    b = arr[y0a - dy:y1a - dy, x0a - dx:x1a - dx]
    return a, b


def variogram_fit(lags_m, gamma, model="exponential"):
    """Fit nugget + sill*(1-exp/-sph) to gamma(lag). Returns dict."""
    lags = np.asarray(lags_m, float)
    g = np.asarray(gamma, float)
    ok = np.isfinite(g) & (lags > 0)
    if ok.sum() < 4:
        return {"variogram_nugget": float("nan"), "variogram_sill": float("nan"),
                "variogram_range": float("nan")}
    L, G = lags[ok], g[ok]
    sill0 = float(np.nanmax(G))
    rng0 = float(L[np.searchsorted(np.cumsum(G) / np.sum(G), 0.63)]) if np.sum(G) > 0 else L.mean()
    rng0 = max(rng0, L[1])

    def expo(h, nug, sill, rng):
        return nug + sill * (1.0 - np.exp(-3.0 * h / max(rng, 1e-6)))

    def spher(h, nug, sill, rng):
        r = np.clip(h / max(rng, 1e-6), 0, 1)
        return nug + sill * (1.5 * r - 0.5 * r ** 3)

    f = spher if model == "spherical" else expo
    try:
        p, _ = optimize.curve_fit(f, L, G, p0=[0.0, sill0, rng0],
                                  bounds=([0, 0, L[1]], [sill0 * 2 + 1e-6, sill0 * 3 + 1e-6, L[-1] * 3]),
                                  maxfev=5000)
        return {"variogram_nugget": float(p[0]), "variogram_sill": float(p[1]),
                "variogram_range": float(p[2])}
    except Exception:
        return {"variogram_nugget": float("nan"), "variogram_sill": sill0,
                "variogram_range": rng0}


def local_std_field(height, cell_size, window_m, mask=None, detrend_method="quadratic"):
    """The local-roughness FIELD: std of the detrended surface in ``window_m``
    windows (NaN where under-covered). Returns ``(field, valid)``. Shared by
    ``rms_roughness`` (its mean) and the roughness-organization metrics (its
    spatial arrangement)."""
    valid = _validmask(height, mask)
    resid, _ = detrend(height, cell_size, mask, detrend_method)
    w = max(1, int(round(window_m / cell_size)))
    m = valid.astype(np.float64)
    x = np.where(valid, resid, 0.0)
    cnt = ndimage.uniform_filter(m, w) * (w * w)
    s1 = ndimage.uniform_filter(x, w) * (w * w)
    s2 = ndimage.uniform_filter(x * x, w) * (w * w)
    with np.errstate(invalid="ignore", divide="ignore"):
        mean = s1 / cnt
        var = s2 / cnt - mean * mean
    var = np.where((cnt >= 0.5 * w * w) & (var > 0), var, np.nan)
    return np.sqrt(var), valid


def rms_roughness(height, cell_size, window_m, mask=None, detrend_method="quadratic"):
    """Mean local std of the detrended surface in windows of ``window_m``."""
    local_std, valid = local_std_field(height, cell_size, window_m, mask, detrend_method)
    good = np.isfinite(local_std) & valid
    return float(local_std[good].mean()) if good.any() else float("nan")


# ---------------------------------------------------------------------------
# D. curvature & sharpness  (raw 2nd derivatives == planar-detrended)
# ---------------------------------------------------------------------------

def _second_derivs(height, cell_size, mask=None):
    valid = _validmask(height, mask)
    z = _fill_nearest(height, valid).astype(np.float64)
    fy, fx = np.gradient(z, cell_size)
    fyy, fyx = np.gradient(fy, cell_size)
    fxy, fxx = np.gradient(fx, cell_size)
    return fx, fy, fxx, fyy, 0.5 * (fxy + fyx)


def curvature_stats(height, cell_size, mask=None):
    """Profile & plan curvature magnitudes (1/m): mean_abs and p90 of each."""
    valid = _validmask(height, mask)
    fx, fy, fxx, fyy, fxy = _second_derivs(height, cell_size, mask)
    p = fx * fx + fy * fy
    eps = 1e-6
    denom_prof = (p * np.power(1.0 + p, 1.5)) + eps
    denom_plan = np.power(p, 1.5) + eps
    prof = -(fxx * fx * fx + 2 * fxy * fx * fy + fyy * fy * fy) / denom_prof
    plan = (fxx * fy * fy - 2 * fxy * fx * fy + fyy * fx * fx) / denom_plan
    # near-flat cells: profile/plan curvature is 0/0 -> report 0
    flat = p < 1e-8
    prof = np.where(flat, 0.0, prof)
    plan = np.where(flat, 0.0, plan)
    ap, al = np.abs(prof[valid]), np.abs(plan[valid])
    return {"mean_abs_profile_curv": float(ap.mean()) if ap.size else float("nan"),
            "profile_curv_p90": float(np.percentile(ap, 90)) if ap.size else float("nan"),
            "mean_abs_plan_curv": float(al.mean()) if al.size else float("nan"),
            "plan_curv_p90": float(np.percentile(al, 90)) if al.size else float("nan")}


def laplacian_stats(height, cell_size, mask=None):
    """|Laplacian| = |fxx+fyy| (1/m): mean and p95 over valid cells."""
    valid = _validmask(height, mask)
    _, _, fxx, fyy, _ = _second_derivs(height, cell_size, mask)
    lap = np.abs((fxx + fyy)[valid])
    if lap.size == 0:
        return {"laplacian_abs_mean": float("nan"), "laplacian_abs_p95": float("nan")}
    return {"laplacian_abs_mean": float(lap.mean()),
            "laplacian_abs_p95": float(np.percentile(lap, 95))}


# ---------------------------------------------------------------------------
# E. spatial organization & heterogeneity  (raw surface)
# ---------------------------------------------------------------------------

def local_relief(height, cell_size, window_m, mask=None):
    """dict(local_relief_mean, heterogeneity_index) at a window size.

    local relief = (max - min) in a window; heterogeneity = CV (std/mean) of the
    local-relief field over the tile — the discriminator for juxtaposed
    flat-and-steep regimes vs uniform rolling.
    """
    valid = _validmask(height, mask)
    z = _fill_nearest(height, valid).astype(np.float64)
    w = max(1, int(round(window_m / cell_size)))
    lo = ndimage.minimum_filter(z, w)
    hi = ndimage.maximum_filter(z, w)
    lr = (hi - lo)[valid]
    if lr.size == 0 or lr.mean() <= 0:
        return {"local_relief_mean": float("nan"), "heterogeneity_index": float("nan")}
    return {"local_relief_mean": float(lr.mean()),
            "heterogeneity_index": float(lr.std() / lr.mean())}


def slope_autocorr_length(height, cell_size, mask=None):
    """Lag (m) at which the radial autocorrelation of the slope field hits 1/e."""
    valid = _validmask(height, mask)
    s = slope_deg(height, cell_size, mask)
    s = np.where(valid, s, np.nan)
    m = np.isfinite(s)
    if m.sum() < 16:
        return float("nan")
    s0 = s - np.nanmean(s)
    s0 = np.where(m, s0, 0.0)
    F = np.fft.fft2(s0)
    ac = np.fft.ifft2(F * np.conj(F)).real
    ac = np.fft.fftshift(ac)
    ac /= ac.max() + 1e-12
    ny, nx = ac.shape
    cy, cx = ny // 2, nx // 2
    yy, xx = np.mgrid[0:ny, 0:nx]
    r = np.hypot((yy - cy) * cell_size, (xx - cx) * cell_size)
    rbins = np.arange(0, r.max(), cell_size)
    prof = np.array([ac[(r >= rbins[i]) & (r < rbins[i + 1])].mean()
                     if np.any((r >= rbins[i]) & (r < rbins[i + 1])) else np.nan
                     for i in range(len(rbins) - 1)])
    rc = 0.5 * (rbins[:-1] + rbins[1:])
    below = np.where(prof < 1.0 / np.e)[0]
    if below.size == 0:
        return float(rc[-1])
    return float(rc[below[0]])


def _component_shape(ys, xs, cell_size):
    """(elongation, length_m) of a labeled component from its coordinate
    covariance: elongation = sqrt(l1/l2), length = 4*sqrt(l1)*cell (major axis
    of the equivalent ellipse). A 1/12-cell self-variance keeps 1-cell-wide
    lines from degenerating to a zero minor axis."""
    yc = ys - ys.mean()
    xc = xs - xs.mean()
    n = ys.size
    cxx = float((xc * xc).sum()) / n + 1.0 / 12.0
    cyy = float((yc * yc).sum()) / n + 1.0 / 12.0
    cxy = float((xc * yc).sum()) / n
    tr, det = cxx + cyy, cxx * cyy - cxy * cxy
    disc = max(tr * tr / 4.0 - det, 0.0) ** 0.5
    l1 = max(tr / 2.0 + disc, 1e-9)
    l2 = max(tr / 2.0 - disc, 1e-9)
    return float(np.sqrt(l1 / l2)), float(4.0 * np.sqrt(l1) * cell_size)


def roughness_organization(height, cell_size, mask=None, window_m=30.0,
                           moran_lag_m=60.0, block_m=350.0,
                           detrend_method="quadratic"):
    """Spatial ORGANIZATION of the local-roughness field R (std in ``window_m``
    windows of the detrended surface) — amounts are covered by rms_roughness;
    these ask WHERE the roughness lives:

    - ``rough_cv``: std/mean of R — unevenness of texture across the tile.
    - ``rough_top10``: share of total roughness carried by the roughest decile
      of cells — concentration (graded courses are broadly smooth with
      roughness in banks/ravines -> HIGH).
    - ``rough_moran``: Moran's I of log R at ``moran_lag_m`` — coarse blobs of
      texture (HIGH) vs fine-grained/thin structures (LOW).
    - ``rough_block_ratio``: variance of ``block_m``-block means of log R over
      total variance — texture variation living at the block scale.
    """
    out = {"rough_cv": float("nan"), "rough_top10": float("nan"),
           "rough_moran": float("nan"), "rough_block_ratio": float("nan")}
    R, valid = local_std_field(height, cell_size, window_m, mask, detrend_method)
    good = np.isfinite(R) & valid
    if good.sum() < 64:
        return out
    r = R[good]
    mu = float(r.mean())
    if mu > 0:
        out["rough_cv"] = float(r.std() / mu)
    k = max(1, int(round(0.1 * r.size)))
    part = np.partition(r, r.size - k)
    out["rough_top10"] = float(part[r.size - k:].sum() / (r.sum() + 1e-12))
    # Moran's I of log R at one lag: 4 unique offsets == symmetric 8-neighbour
    # weights at distance d (each unordered pair once; the pair-count and
    # cross-product double together, so I is unchanged).
    x = np.where(good, np.log(R + 1e-6), np.nan)
    x0 = x - np.nanmean(x)
    d = max(1, int(round(moran_lag_m / cell_size)))
    num, wsum = 0.0, 0
    for dy, dx in ((0, d), (d, 0), (d, d), (d, -d)):
        a, b = _shift_pair(x0, dy, dx)
        pair = np.isfinite(a) & np.isfinite(b)
        num += float((a[pair] * b[pair]).sum())
        wsum += int(pair.sum())
    den = float(np.nansum(x0 * x0))
    nfin = int(np.isfinite(x0).sum())
    if wsum > 0 and den > 0:
        out["rough_moran"] = float((nfin / wsum) * (num / den))
    # block-scale variance share of log R
    bs = max(2, int(round(block_m / cell_size)))
    ny, nx = x.shape
    means = []
    for by in range(0, ny - bs + 1, bs):
        for bx in range(0, nx - bs + 1, bs):
            blk = x[by:by + bs, bx:bx + bs]
            f = np.isfinite(blk)
            if f.sum() >= 0.5 * bs * bs:
                means.append(float(blk[f].mean()))
    tot = float(np.nanvar(x))
    if len(means) >= 6 and tot > 0:
        out["rough_block_ratio"] = float(np.var(means) / tot)
    return out


def extrema_density(height, cell_size, mask=None, sigma_m=12.0,
                    detrend_method="quadratic"):
    """Local extrema (peaks + pits) per hectare after gaussian smoothing at
    ``sigma_m`` — a direct landform-granularity count. The smoothing removes
    sub-sigma stipple, so the count reflects the mid-scale (>~2 sigma) forms a
    golfer would actually see; many small extrema = choppy, few large = graded.
    """
    valid = _validmask(height, mask)
    if valid.sum() < 64:
        return float("nan")
    resid, _ = detrend(height, cell_size, mask, detrend_method)
    z = ndimage.gaussian_filter(_fill_nearest(resid, valid), sigma_m / cell_size)
    mx = ndimage.maximum_filter(z, 3)
    mn = ndimage.minimum_filter(z, 3)
    eps = 1e-12
    peaks = (z >= mx) & (z - mn > eps)   # strict: excludes flat neighbourhoods
    pits = (z <= mn) & (mx - z > eps)
    ext = (peaks | pits) & valid
    ext[0, :] = ext[-1, :] = False       # border cells: filters clamp there
    ext[:, 0] = ext[:, -1] = False
    ha = valid.sum() * cell_size * cell_size / 1e4
    return float(ext.sum() / ha) if ha > 0 else float("nan")


def tpi_landforms(height, cell_size, radius_m, mask=None, k=0.7,
                  min_area_m2=900.0):
    """Valley-landform coherence at one TPI scale.

    TPI = z − mean(z in a ~2*radius window) on the nearest-filled surface;
    valley class = TPI < −k·std(TPI) (relative threshold -> amplitude-invariant,
    pure organization). 8-connected components >= ``min_area_m2``:

    - ``valley_count_km2``: components per km² of valid area (fragmentation).
    - ``valley_elong``: area-weighted mean elongation (trough shape).
    - ``valley_len_p90``: p90 of component major-axis lengths, metres.
    """
    out = {"valley_count_km2": float("nan"), "valley_elong": float("nan"),
           "valley_len_p90": float("nan")}
    valid = _validmask(height, mask)
    if valid.sum() < 64:
        return out
    z = _fill_nearest(height, valid).astype(np.float64)
    w = max(3, 2 * int(round(radius_m / cell_size)) + 1)
    tpi = z - ndimage.uniform_filter(z, w)
    sd = float(tpi[valid].std())
    km2 = valid.sum() * cell_size * cell_size / 1e6
    if sd <= 0 or km2 <= 0:
        out["valley_count_km2"] = 0.0
        return out
    vall = (tpi < -k * sd) & valid
    lab, n = ndimage.label(vall, structure=np.ones((3, 3), np.int8))
    min_cells = max(1, int(round(min_area_m2 / (cell_size * cell_size))))
    elongs, lens, areas = [], [], []
    if n:
        for li, sl in enumerate(ndimage.find_objects(lab), start=1):
            ys, xs = np.nonzero(lab[sl] == li)
            if ys.size < min_cells:
                continue
            e, length = _component_shape(ys, xs, cell_size)
            elongs.append(e)
            lens.append(length)
            areas.append(ys.size)
    if not lens:
        out["valley_count_km2"] = 0.0
        return out
    aw = np.array(areas, np.float64)
    aw /= aw.sum()
    out["valley_count_km2"] = float(len(lens) / km2)
    out["valley_elong"] = float((np.array(elongs) * aw).sum())
    out["valley_len_p90"] = float(np.percentile(lens, 90))
    return out


# ---------------------------------------------------------------------------
# F. drainage & hydrology  (raw surface; priority-flood + D8)
# ---------------------------------------------------------------------------

_D8 = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)]


def fill_depressions(height, cell_size, mask=None):
    """Priority-flood depression filling (Barnes et al.).

    Returns a filled surface where every cell drains monotonically to the tile
    edge (a tiny epsilon gradient is added so flat fills still route). Invalid
    cells are treated as very low (open) boundaries.
    """
    valid = _validmask(height, mask)
    z = _fill_nearest(height, valid).astype(np.float64)
    ny, nx = z.shape
    eps = 1e-4 * cell_size
    filled = np.full_like(z, np.inf)
    closed = np.zeros(z.shape, bool)
    pq = []
    order = 0
    # seed the border (and any invalid cells) as open outlets
    for y in range(ny):
        for x in (0, nx - 1):
            heapq.heappush(pq, (z[y, x], order, y, x)); order += 1
            closed[y, x] = True
    for x in range(nx):
        for y in (0, ny - 1):
            if not closed[y, x]:
                heapq.heappush(pq, (z[y, x], order, y, x)); order += 1
                closed[y, x] = True
    while pq:
        zc, _, y, x = heapq.heappop(pq)
        filled[y, x] = zc
        for dy, dx in _D8:
            ny2, nx2 = y + dy, x + dx
            if 0 <= ny2 < ny and 0 <= nx2 < nx and not closed[ny2, nx2]:
                closed[ny2, nx2] = True
                zn = max(z[ny2, nx2], zc + eps)
                heapq.heappush(pq, (zn, order, ny2, nx2)); order += 1
    return filled


def _shift_full(a, dy, dx, fill):
    """a shifted by (dy,dx) with out-of-frame filled by ``fill`` (same shape)."""
    out = np.full_like(a, fill)
    ny, nx = a.shape
    ya, yb = max(0, -dy), ny - max(0, dy)
    xa, xb = max(0, -dx), nx - max(0, dx)
    out[ya:yb, xa:xb] = a[ya + dy:yb + dy, xa + dx:xb + dx]
    return out


def d8_accumulation(filled, cell_size):
    """D8 steepest-descent flow accumulation (cell counts). Returns (accum, rec).

    accum[i] = number of cells draining through i (incl. itself). Receiver
    computation is vectorised over the 8 directions; the accumulation scatter
    runs once in topological (high->low) order.
    """
    ny, nx = filled.shape
    fz = filled.astype(np.float64)
    best_slope = np.zeros((ny, nx))
    recdir = np.full((ny, nx), -1, np.int8)
    for k, (dy, dx) in enumerate(_D8):
        neigh = _shift_full(fz, dy, dx, np.inf)   # neighbour elevation
        dist = cell_size * (SQRT2 if (dy and dx) else 1.0)
        slope = (fz - neigh) / dist
        take = slope > best_slope
        best_slope[take] = slope[take]
        recdir[take] = k
    # receiver linear index (-1 = pit/outlet)
    yy, xx = np.mgrid[0:ny, 0:nx]
    rec = np.full((ny, nx), -1, np.int64)
    for k, (dy, dx) in enumerate(_D8):
        m = recdir == k
        rec[m] = ((yy[m] + dy) * nx + (xx[m] + dx))
    accum = np.ones(ny * nx, np.float64)
    recf = rec.ravel()
    order = np.argsort(-fz.ravel())      # process high -> low
    for i in order:
        j = recf[i]
        if j >= 0:
            accum[j] += accum[i]
    return accum.reshape(ny, nx), rec


def hydrology_metrics(height, cell_size, mask=None, accum_area_threshold_m2=6e4):
    """dict(drainage_density, valley_spacing, concavity_theta, chan_len_p90).

    Channels = cells whose upslope drainage area >= threshold. Drainage density
    = channel length / area; valley spacing ~ 1/density; concavity theta from
    S = k * A^-theta over the channel network; chan_len_p90 = p90 major-axis
    length of connected channel components (network coherence: long connected
    channels vs fragmented pieces).
    """
    valid = _validmask(height, mask)
    filled = fill_depressions(height, cell_size, mask)
    accum_cells, rec = d8_accumulation(filled, cell_size)
    cell_area = cell_size * cell_size
    area = accum_cells * cell_area
    chan = (area >= accum_area_threshold_m2) & valid
    ncell = int(valid.sum())
    tile_area = ncell * cell_area
    if ncell == 0 or chan.sum() < 3:
        return {"drainage_density": 0.0 if ncell else float("nan"),
                "valley_spacing": float("nan"), "concavity_theta": float("nan"),
                "chan_len_p90": float("nan")}
    # channel length: sum of link lengths from each channel cell to its receiver
    ny, nx = height.shape
    length = 0.0
    ys, xs = np.where(chan)
    for y, x in zip(ys, xs):
        j = rec[y, x]
        if j >= 0:
            jy, jx = divmod(int(j), nx)
            if chan[jy, jx]:
                length += cell_size * (SQRT2 if (jy != y and jx != x) else 1.0)
    dens = length / tile_area                     # 1/m
    spacing = (1.0 / dens) if dens > 0 else float("nan")
    # concavity: S vs A on channel cells (S = downslope grad on filled surface)
    S = []
    A = []
    fz = filled
    for y, x in zip(ys, xs):
        j = rec[y, x]
        if j < 0:
            continue
        jy, jx = divmod(int(j), nx)
        dist = cell_size * (SQRT2 if (jy != y and jx != x) else 1.0)
        s = (fz[y, x] - fz[jy, jx]) / dist
        if s > 1e-6:
            S.append(s); A.append(area[y, x])
    theta = float("nan")
    if len(S) >= 8:
        coef = np.polyfit(np.log10(np.array(A)), np.log10(np.array(S)), 1)
        theta = float(-coef[0])
    # network coherence: p90 major-axis length of connected channel components
    chan_len_p90 = float("nan")
    lab, n = ndimage.label(chan, structure=np.ones((3, 3), np.int8))
    if n:
        lens = []
        for li, sl in enumerate(ndimage.find_objects(lab), start=1):
            cys, cxs = np.nonzero(lab[sl] == li)
            if cys.size < 3:
                continue
            _, length = _component_shape(cys, cxs, cell_size)
            lens.append(length)
        if lens:
            chan_len_p90 = float(np.percentile(lens, 90))
    return {"drainage_density": float(dens),
            "valley_spacing": float(spacing),
            "concavity_theta": theta,
            "chan_len_p90": chan_len_p90}


# ---------------------------------------------------------------------------
# G. anisotropy  (directional variograms)
# ---------------------------------------------------------------------------

def anisotropy(height, cell_size, mask=None, detrend_method="quadratic",
               max_lag_m=400.0, n_dir=8):
    """dict(anisotropy_ratio, anisotropy_orientation_deg).

    Directional variograms at ``n_dir`` orientations; the range per direction is
    the lag reaching 0.63 of that direction's sill. Ratio = max/min range;
    orientation = the max-continuity axis (degrees, 0 = +x / east).
    """
    valid = _validmask(height, mask)
    resid, _ = detrend(height, cell_size, mask, detrend_method)
    resid = np.where(valid, resid, np.nan)
    lags = np.linspace(cell_size, max_lag_m, 24)
    ranges = []
    angles = np.linspace(0, np.pi, n_dir, endpoint=False)
    for th in angles:
        uy, ux = np.sin(th), np.cos(th)
        g = []
        for lag in lags:
            dy = int(round(lag / cell_size * uy))
            dx = int(round(lag / cell_size * ux))
            if dy == 0 and dx == 0:
                g.append(0.0); continue
            a, b = _shift_pair(resid, dy, dx)
            d = (a - b)
            d = d[np.isfinite(d)]
            g.append(0.5 * float(np.mean(d * d)) if d.size else np.nan)
        g = np.array(g)
        if np.all(~np.isfinite(g)):
            ranges.append(np.nan); continue
        sill = np.nanmax(g)
        idx = np.where(g >= 0.63 * sill)[0]
        ranges.append(float(lags[idx[0]]) if idx.size else float(lags[-1]))
    ranges = np.array(ranges)
    if np.all(~np.isfinite(ranges)) or np.nanmin(ranges) <= 0:
        return {"anisotropy_ratio": float("nan"), "anisotropy_orientation_deg": float("nan")}
    imax = int(np.nanargmax(ranges))
    ratio = float(np.nanmax(ranges) / np.nanmin(ranges))
    return {"anisotropy_ratio": ratio,
            "anisotropy_orientation_deg": float(np.degrees(angles[imax]))}


# ---------------------------------------------------------------------------
# H. land cover  (mask fractions)
# ---------------------------------------------------------------------------

def fraction(layer_mask, valid_mask=None):
    """Fraction of valid cells where ``layer_mask`` is set (e.g. canopy/water)."""
    lm = layer_mask.astype(bool)
    if valid_mask is None:
        denom = lm.size
        num = int(lm.sum())
    else:
        vm = valid_mask.astype(bool)
        denom = int(vm.sum())
        num = int((lm & vm).sum())
    return float(num / denom) if denom else float("nan")
