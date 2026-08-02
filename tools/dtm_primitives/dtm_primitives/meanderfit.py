"""Meander decomposition: centerline = smooth trend + lateral displacement.
Reports amplitude, dominant wavelength, sinuosity, and the SELF-CONSISTENT
MeanderSpec parameterization (width basis = fitted valley top width W;
wavelength_mult = lambda/W; intensity = A / A_max(lambda, W) with the exact
Stage-2 curvature-clamp authority A_max = lambda^2 / (4 pi^2 * 2.5 W)).
As long as extraction and the sampler share this W basis, a MeanderSpec
built from these values reproduces the observed geometry."""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
from scipy.ndimage import gaussian_filter1d

from .network import SIGMA_PER_L
from .transects import arc_length, resample_polyline


@dataclass
class MeanderFit:
    amplitude_m: float          # sqrt(2)*std of trimmed displacement
    amplitude_p90_m: float
    wavelength_m: float         # 2 x median zero-crossing spacing (nan if <3)
    sinuosity: float            # arc / trend-arc
    n_crossings: int
    intensity: float            # A / A_max(lambda, W)  (nan without lambda)
    wavelength_mult: float      # lambda / W
    trend_len_m: float
    basis: str = "gauss"        # gauss | chord (which trend measured it)


def _band(pts: np.ndarray, trend: np.ndarray, step: float,
          smooth_e_m: float, min_crossings: int):
    """(amplitude, amp_p90, wavelength, n_crossings, trend_len) of the
    displacement of pts from one trend polyline."""
    from .profiles import polyline_project
    _u, d, side, t_len = polyline_project(trend, pts[:, 0], pts[:, 1])
    e = d * side
    e = e - np.median(e)
    es = gaussian_filter1d(e, SIGMA_PER_L * smooth_e_m / step,
                           mode="nearest")
    mad = np.median(np.abs(e)) * 1.4826
    keep = np.abs(e) <= 3.0 * max(mad, 1e-9)
    amp = float(np.sqrt(2.0) * e[keep].std()) if keep.sum() > 4 else 0.0
    amp90 = float(np.percentile(np.abs(e), 90))
    sgn = np.sign(es)
    nz = sgn != 0
    crossings = np.where(np.diff(sgn[nz]) != 0)[0]
    idx_nz = np.where(nz)[0]
    n_cross = int(len(crossings))
    if n_cross >= min_crossings and len(crossings) >= 2:
        cross_s = idx_nz[crossings] * step
        lam = float(2.0 * np.median(np.diff(cross_s)))
    else:
        lam = float("nan")
    return amp, amp90, lam, n_cross, float(t_len)


def decompose(centerline: np.ndarray, top_width_m: float,
              cfg: dict) -> MeanderFit | None:
    mcfg = cfg["meander"]
    step = 5.0
    pts = resample_polyline(centerline, step)
    if len(pts) < 12:
        return None

    # TWO trend bases: the 600 m Gaussian measures creek-scale meanders but
    # ABSORBS long-wave ones (a 2.9 km floodplain meander lives inside the
    # trend); the straight chord catches those. The decisively larger
    # amplitude picks the basis (2x margin so valley curvature can't steal
    # creek-scale fits).
    sigma = SIGMA_PER_L * mcfg["trend_lowpass_m"] / step
    tx = gaussian_filter1d(pts[:, 0], sigma, mode="nearest")
    ty = gaussian_filter1d(pts[:, 1], sigma, mode="nearest")
    g = _band(pts, np.stack([tx, ty], 1), step, mcfg["smooth_e_m"],
              mcfg["min_crossings"])
    chord = np.stack([pts[0], pts[-1]])
    c = _band(pts, chord, step, mcfg["smooth_e_m"] * 4.0,
              mcfg["min_crossings"])
    if np.isfinite(c[2]) and c[0] > 2.0 * g[0]:
        amp, amp90, lam, n_cross, t_len = c
        basis = "chord"
    else:
        amp, amp90, lam, n_cross, t_len = g
        basis = "gauss"

    arc = arc_length(pts)
    sinuosity = float(arc / max(t_len, 1e-9))

    W = max(float(top_width_m), 1.0)
    if np.isfinite(lam) and lam > 0:
        a_max = lam ** 2 / (4.0 * np.pi ** 2
                            * cfg["meander"]["min_radius_widths"] * W)
        intensity = float(np.clip(amp / max(a_max, 1e-9), 0.0, 1.0))
        wl_mult = float(lam / W)
    else:
        intensity = float("nan")
        wl_mult = float("nan")
    return MeanderFit(amplitude_m=round(amp, 2),
                      amplitude_p90_m=round(amp90, 2),
                      wavelength_m=round(lam, 1) if np.isfinite(lam) else lam,
                      sinuosity=round(sinuosity, 4),
                      n_crossings=n_cross,
                      intensity=round(intensity, 3)
                      if np.isfinite(intensity) else intensity,
                      wavelength_mult=round(wl_mult, 2)
                      if np.isfinite(wl_mult) else wl_mult,
                      trend_len_m=round(float(t_len), 1),
                      basis=basis)
