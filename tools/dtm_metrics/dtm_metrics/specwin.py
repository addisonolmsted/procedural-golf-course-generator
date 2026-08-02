"""Clean-window spectral estimation.

On a median course ~half the cells are Laplace-inpainted, and the fill has
near-zero high-frequency content — a global FFT under-reports every fine band.
So character bands come from Hann-tapered periodograms of WINDOWS that are
>= 90% clean, per-window planar-detrended (the implicit high-pass), averaged
with window weights = mean cell weight. Band power is a Parseval annulus sum,
so band RMS is in meters of surface amplitude (unit-tested against white and
power-law fields).
"""

from __future__ import annotations

import numpy as np


def _hann2(shape: tuple[int, int]) -> np.ndarray:
    return np.outer(np.hanning(shape[0]), np.hanning(shape[1]))


def _planar_detrend(x: np.ndarray) -> np.ndarray:
    ny, nx = x.shape
    yy, xx = np.mgrid[0:ny, 0:nx].astype(np.float64)
    A = np.stack([np.ones(ny * nx), xx.ravel() - xx.mean(), yy.ravel() - yy.mean()], 1)
    coef, *_ = np.linalg.lstsq(A, x.ravel(), rcond=None)
    return x - (A @ coef).reshape(ny, nx)


def window_periodogram(x: np.ndarray, cell: float, pre_detrended: bool = False):
    """(P, f) — 2-D periodogram normalized so sum(P) ~= var(detrended x)
    (Welch normalization: |FFT(h*x)|^2 / (N * mean(h^2))), and the radial
    frequency magnitude grid (cycles/m). Rectangular grids supported; DC
    zeroed. `pre_detrended=True` skips the planar detrend (caller already
    removed a trend, e.g. the global quadratic for macro bands)."""
    ny, nx = x.shape
    h = _hann2((ny, nx))
    d = (x if pre_detrended else _planar_detrend(x)) * h
    F = np.fft.fft2(d)
    P = (np.abs(F) ** 2) / ((ny * nx) * (h * h).mean() * (ny * nx))
    P[0, 0] = 0.0
    fy = np.fft.fftfreq(ny, d=cell)
    fx = np.fft.fftfreq(nx, d=cell)
    f = np.hypot(fy[:, None], fx[None, :])
    return P, f


def band_variances(P: np.ndarray, f: np.ndarray, bands_m) -> list[float]:
    """Variance (m^2) in each wavelength band [(lo_m, hi_m), ...]; hi may be inf."""
    out = []
    for lo_m, hi_m in bands_m:
        f_lo = 0.0 if np.isinf(hi_m) else 1.0 / hi_m
        f_hi = 1.0 / lo_m
        sel = (f > f_lo) & (f <= f_hi)
        out.append(float(P[sel].sum()))
    return out


def beta_from_periodogram(P: np.ndarray, f: np.ndarray, band_m, n_bins: int = 24) -> float:
    """Log-log radial slope over the wavelength band (PSD ~ f^-beta)."""
    lo_m, hi_m = band_m
    sel = (f >= 1.0 / hi_m) & (f <= 1.0 / lo_m)
    if sel.sum() < 32:
        return float("nan")
    ff = f[sel]
    pp = P[sel]
    edges = np.logspace(np.log10(ff.min()), np.log10(ff.max()), n_bins + 1)
    idx = np.clip(np.searchsorted(edges, ff) - 1, 0, n_bins - 1)
    fs, ps = [], []
    for b in range(n_bins):
        m = idx == b
        if m.sum() >= 4:
            fs.append(np.log10(ff[m].mean()))
            ps.append(np.log10(pp[m].mean() + 1e-30))
    if len(fs) < 6:
        return float("nan")
    coef = np.polyfit(fs, ps, 1)
    return float(-coef[0])


def iter_windows(shape: tuple[int, int], win_px: int, stride_px: int):
    ny, nx = shape
    for y0 in range(0, max(ny - win_px, 0) + 1, stride_px):
        for x0 in range(0, max(nx - win_px, 0) + 1, stride_px):
            yield y0, x0


def clean_window_bands(nat: np.ndarray, clean: np.ndarray, weightmap: np.ndarray,
                       cell: float, bands_m, beta_band_m, window_cascade_m,
                       stride_frac: float, min_clean_frac: float,
                       min_windows: int) -> dict:
    """Coverage-weighted clean-window band RMS values + beta, over a WINDOW
    CASCADE: suburban courses' clean areas are shredded by cart-path masks, so
    a course may support 128 m windows but no 512 m ones. Each band takes its
    estimate from the LARGEST window size that (a) fits >= min_windows clean
    windows and (b) can hold the band (window >= 2x the band's long-wavelength
    edge). Long bands honestly go NaN on shredded courses instead of dragging
    every band to NaN. beta needs window >= 2x its long edge too.

    Returns {"bands": [rms...], "beta", "n_windows" (largest tier used),
    "window_m" (largest tier with windows)}.
    """
    tiers = []
    for wm in window_cascade_m:
        win = int(round(wm / cell))
        if win < 32 or win > min(nat.shape):
            continue
        stride = max(1, int(round(win * stride_frac)))
        var_sums = None
        beta_sum = 0.0
        beta_w = 0.0
        wsum = 0.0
        n_win = 0
        for y0, x0 in iter_windows(nat.shape, win, stride):
            cfrac = clean[y0:y0 + win, x0:x0 + win].mean()
            if cfrac < min_clean_frac:
                continue
            w = float(weightmap[y0:y0 + win, x0:x0 + win].mean())
            P, f = window_periodogram(nat[y0:y0 + win, x0:x0 + win], cell)
            bv = band_variances(P, f, bands_m)
            if var_sums is None:
                var_sums = np.zeros(len(bv))
            var_sums += w * np.asarray(bv)
            b = beta_from_periodogram(P, f, beta_band_m)
            if np.isfinite(b):
                beta_sum += w * b
                beta_w += w
            wsum += w
            n_win += 1
        if n_win >= min_windows:
            tiers.append({
                "window_m": wm,
                "rms": np.sqrt(var_sums / wsum),
                "beta": float(beta_sum / beta_w) if beta_w > 0 else float("nan"),
                "n": n_win,
            })
    out_bands = []
    for bi, (_lo, hi) in enumerate(bands_m):
        need = 2.0 * (hi if np.isfinite(hi) else 1e9)
        val = float("nan")
        for t in tiers:  # cascade ordered largest-first
            if t["window_m"] >= need:
                val = float(t["rms"][bi])
                break
        out_bands.append(val)
    beta = float("nan")
    for t in tiers:
        if t["window_m"] >= 2.0 * beta_band_m[1] and np.isfinite(t["beta"]):
            beta = t["beta"]
            break
    return {
        "bands": out_bands,
        "beta": beta,
        "n_windows": tiers[0]["n"] if tiers else 0,
        "window_m": tiers[0]["window_m"] if tiers else 0.0,
    }
