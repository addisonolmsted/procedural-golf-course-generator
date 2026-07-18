"""Weighted statistics — the mechanism behind the doc's "weight exterior-margin
cells 2x when present" rule (degrading with margin contamination). Weighted
moments and true weighted percentiles (sort + cumulative weight interpolation);
no sample duplication.
"""

from __future__ import annotations

import numpy as np


def weighted_mean(v: np.ndarray, w: np.ndarray) -> float:
    sw = w.sum()
    return float((v * w).sum() / sw) if sw > 0 else float("nan")


def weighted_moments(v: np.ndarray, w: np.ndarray) -> dict:
    """dict(mean, std, skew, kurtosis[excess]) with reliability weights."""
    sw = w.sum()
    if sw <= 0 or v.size < 4:
        return {k: float("nan") for k in ("mean", "std", "skew", "kurtosis")}
    m = (v * w).sum() / sw
    d = v - m
    m2 = (w * d * d).sum() / sw
    if m2 <= 0:
        return {"mean": float(m), "std": 0.0, "skew": float("nan"),
                "kurtosis": float("nan")}
    m3 = (w * d ** 3).sum() / sw
    m4 = (w * d ** 4).sum() / sw
    return {
        "mean": float(m),
        "std": float(np.sqrt(m2)),
        "skew": float(m3 / m2 ** 1.5),
        "kurtosis": float(m4 / (m2 * m2) - 3.0),
    }


def weighted_percentile(v: np.ndarray, w: np.ndarray, qs) -> np.ndarray:
    """Weighted percentiles (qs in [0,100]); linear interpolation on the
    cumulative-weight CDF (C=0.5 convention). Deterministic (stable sort)."""
    qs = np.atleast_1d(np.asarray(qs, np.float64))
    if v.size == 0 or w.sum() <= 0:
        return np.full(qs.shape, np.nan)
    order = np.argsort(v, kind="stable")
    vs = v[order]
    ws = w[order].astype(np.float64)
    cw = np.cumsum(ws) - 0.5 * ws
    cw /= ws.sum()
    return np.interp(qs / 100.0, cw, vs)


def weighted_iqr(v: np.ndarray, w: np.ndarray) -> float:
    p = weighted_percentile(v, w, [25.0, 75.0])
    return float(p[1] - p[0])
