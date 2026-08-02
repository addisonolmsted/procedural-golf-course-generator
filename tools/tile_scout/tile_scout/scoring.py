"""Shared metric-space machinery: transforms, robust z-scales, shrunk
Mahalanobis. All scales derive from the 135 COURSES (the matching target) and
are saved once — candidates are always projected into the course frame."""

from __future__ import annotations

import numpy as np
import pandas as pd


def transform(df: pd.DataFrame, cols: list[str], log1p_cols: list[str]
              ) -> pd.DataFrame:
    out = df[cols].astype(float).copy()
    for c in cols:
        if c in log1p_cols:
            out[c] = np.log1p(np.maximum(out[c], 0.0))
    return out


def robust_scales(t: pd.DataFrame) -> tuple[pd.Series, pd.Series]:
    """(center, scale) per column: median / 1.4826*MAD (floored)."""
    med = t.median()
    mad = (t - med).abs().median() * 1.4826
    sd = mad.where(mad > 1e-9, t.std().where(t.std() > 1e-9, 1.0))
    return med, sd


def zscore(t: pd.DataFrame, med: pd.Series, sd: pd.Series) -> pd.DataFrame:
    return (t - med) / sd


def whitener(zc: np.ndarray) -> np.ndarray:
    """Ledoit-Wolf-shrunk covariance -> Cholesky whitening matrix L so that
    ||L (a - b)|| is the shrunk Mahalanobis distance."""
    from sklearn.covariance import LedoitWolf
    cov = LedoitWolf().fit(zc).covariance_
    return np.linalg.cholesky(np.linalg.inv(cov)).T


def pairwise_dist(L: np.ndarray, A: np.ndarray, B: np.ndarray) -> np.ndarray:
    """(len(A), len(B)) whitened euclidean distances."""
    wa = A @ L.T
    wb = B @ L.T
    from scipy.spatial.distance import cdist
    return cdist(wa, wb)


def energy_distance(A: np.ndarray, B: np.ndarray) -> float:
    """Multivariate energy distance (pattern of calibration/validation.py)."""
    from scipy.spatial.distance import cdist
    return float(2.0 * cdist(A, B).mean() - cdist(A, A).mean()
                 - cdist(B, B).mean())
