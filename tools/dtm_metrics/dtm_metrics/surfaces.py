"""Surface decomposition — the primitive-awareness split.

S1 = the naturalized surface (macro form; primitives' domain).
S2 = naturalized − Gaussian low-pass at L (the "primitive proxy": until Stage 3
     measures real primitives, everything below L is assumed primitive-explained;
     the residual is what the noise layer must reproduce).
S3 = macro predictors evaluated ON the low-pass surface (slope, aspect, TPI,
     relief position) — the conditioning fields for the modulation-contract
     metrics. The macro provider is PLUGGABLE: Stage 3 swaps the low-pass for
     the fitted-primitive surface without touching metric code.
"""

from __future__ import annotations

from dataclasses import dataclass

import numpy as np
from scipy import ndimage

from metrics import core as v1core  # frozen v1 kernels (path set in __init__)

# Gaussian sigma for half-amplitude response at wavelength L:
# exp(-2 pi^2 sigma^2 / L^2) = 0.5  ->  sigma = L * sqrt(ln 2 / (2 pi^2))
SIGMA_PER_L = float(np.sqrt(np.log(2.0) / (2.0 * np.pi ** 2)))  # ~0.18739


@dataclass
class Surfaces:
    cell: float
    L: float
    s1: np.ndarray          # naturalized (nearest-filled outside data)
    lowpass: np.ndarray     # the macro surface (primitive proxy)
    s2: np.ndarray          # s1 - lowpass
    lp_slope: np.ndarray    # slope magnitude of the macro surface (rise/run)
    lp_aspect: np.ndarray   # aspect of the macro surface, radians
    tpi: np.ndarray         # macro TPI (z_lp - mean window), meters
    tpi_sigma: float        # std of tpi over valid (for class thresholds)
    relief_pos: np.ndarray  # percentile rank of lowpass elevation within valid


def build(nat: np.ndarray, valid: np.ndarray, cell: float, L: float,
          tpi_window_m: float, macro_override: np.ndarray | None = None) -> Surfaces:
    """Decompose. `macro_override` is the Stage-3 plug point (fitted-primitive
    surface instead of the low-pass)."""
    filled = v1core._fill_nearest(nat, valid).astype(np.float64)
    if macro_override is not None:
        lowpass = macro_override.astype(np.float64)
    else:
        lowpass = ndimage.gaussian_filter(filled, SIGMA_PER_L * L / cell)
    s2 = filled - lowpass

    gy, gx = np.gradient(lowpass, cell)
    lp_slope = np.hypot(gx, gy)
    lp_aspect = np.arctan2(gy, gx)

    w = max(3, int(round(tpi_window_m / cell)) | 1)
    tpi = lowpass - ndimage.uniform_filter(lowpass, w)
    tv = tpi[valid]
    tpi_sigma = float(tv.std()) if tv.size else 0.0

    # relief position: rank of the macro elevation within the valid window
    rp = np.zeros_like(lowpass)
    lv = lowpass[valid]
    if lv.size:
        order = np.argsort(lv, kind="stable")
        ranks = np.empty_like(order, dtype=np.float64)
        ranks[order] = np.linspace(0.0, 1.0, lv.size)
        rp[valid] = ranks
    return Surfaces(cell=cell, L=L, s1=filled, lowpass=lowpass, s2=s2,
                    lp_slope=lp_slope, lp_aspect=lp_aspect, tpi=tpi,
                    tpi_sigma=tpi_sigma, relief_pos=rp)


def weight_map(valid: np.ndarray, clean: np.ndarray, exterior: np.ndarray,
               exterior_inpaint_frac: float, bonus: float = 1.0) -> np.ndarray:
    """Cell weights: 1.0 on valid; exterior CLEAN cells get the margin bonus
    scaled by how pristine the margin is (doc's 2x rule, contamination-aware)."""
    w = valid.astype(np.float64)
    w[exterior & clean] += bonus * max(0.0, 1.0 - exterior_inpaint_frac)
    return w
