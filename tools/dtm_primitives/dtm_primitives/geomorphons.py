"""Geomorphons (Jasiewicz & Stepinski 2013): 10-class landform
classification by 8-direction line-of-sight ternary patterns. Pure numpy,
deterministic, resolution-agnostic — run on our own clean tiles (the global
90 m product is far coarser than our data).

Per direction: zenith angle A = max elevation angle to any cell within the
lookup distance, nadir angle B = max depression angle. Ternary element:
+1 if B − A > t (land falls away — cell locally HIGH toward that direction),
−1 if A − B > t (land rises — cell locally LOW), else 0. The (n_plus,
n_minus) counts index the class lookup table. Classes:
1 FL flat, 2 PK peak, 3 RI ridge, 4 SH shoulder, 5 SP spur, 6 SL slope,
7 HL hollow, 8 FS footslope, 9 VL valley, 10 PT pit.
"""

from __future__ import annotations

import numpy as np

CLASS_NAMES = {1: "flat", 2: "peak", 3: "ridge", 4: "shoulder", 5: "spur",
               6: "slope", 7: "hollow", 8: "footslope", 9: "valley",
               10: "pit"}

# lookup[n_plus][n_minus] — Jasiewicz & Stepinski Table 1 orientation:
# n_plus = directions where the cell is locally HIGH (land falls away),
# n_minus = directions where the cell is locally LOW (land rises).
# Verified by the analytic tests (cone->peak, pit, tent->ridge, V->valley).
_T = [
    # n_minus:  0   1   2   3   4   5   6   7   8      n_plus:
    [1, 1, 1, 8, 8, 9, 9, 9, 10],   # 0
    [1, 1, 8, 8, 8, 9, 9, 9, 0],    # 1
    [1, 4, 6, 6, 7, 7, 9, 0, 0],    # 2
    [4, 4, 6, 6, 6, 7, 0, 0, 0],    # 3
    [4, 4, 5, 6, 6, 0, 0, 0, 0],    # 4
    [3, 3, 5, 5, 0, 0, 0, 0, 0],    # 5
    [3, 3, 3, 0, 0, 0, 0, 0, 0],    # 6
    [3, 3, 0, 0, 0, 0, 0, 0, 0],    # 7
    [2, 0, 0, 0, 0, 0, 0, 0, 0],    # 8
]
LOOKUP = np.zeros((9, 9), np.uint8)
for _p in range(9):
    for _m in range(9):
        LOOKUP[_p, _m] = _T[_p][_m]

_DIRS = [(-1, 0), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1)]


def _shift(a: np.ndarray, dy: int, dx: int) -> np.ndarray:
    """Shift with NaN fill (line-of-sight beyond the array sees nothing)."""
    out = np.full_like(a, np.nan)
    h, w = a.shape
    ys0, ys1 = max(dy, 0), min(h + dy, h)
    xs0, xs1 = max(dx, 0), min(w + dx, w)
    out[ys0:ys1, xs0:xs1] = a[ys0 - dy:ys1 - dy, xs0 - dx:xs1 - dx]
    return out


def classify(z: np.ndarray, cell: float, lookup_m: float = 500.0,
             skip_m: float = 20.0, flat_deg: float = 1.0) -> np.ndarray:
    """Geomorphon class raster (uint8, 0 = undefined/nodata edge)."""
    z = z.astype(np.float64)
    t = np.tan(np.radians(flat_deg))
    n_steps_max = max(int(round(lookup_m / cell)), 2)
    step0 = max(int(round(skip_m / cell)), 1)
    n_plus = np.zeros(z.shape, np.uint8)
    n_minus = np.zeros(z.shape, np.uint8)
    valid = np.isfinite(z)
    for dy, dx in _DIRS:
        diag = np.hypot(dy, dx)
        A = np.full(z.shape, -np.inf)   # max elevation tangent
        B = np.full(z.shape, -np.inf)   # max depression tangent
        for s in range(step0, n_steps_max + 1):
            zs = _shift(z, dy * s, dx * s)
            with np.errstate(invalid="ignore"):
                tanang = (zs - z) / (s * cell * diag)
            fin = np.isfinite(tanang)
            np.maximum(A, np.where(fin, tanang, -np.inf), out=A)
            np.maximum(B, np.where(fin, -tanang, -np.inf), out=B)
        seen = np.isfinite(A) & np.isfinite(B)
        Af = np.where(seen, A, 0.0)
        Bf = np.where(seen, B, 0.0)
        plus = seen & ((Bf - Af) > t)   # land falls away -> locally high
        minus = seen & ((Af - Bf) > t)  # land rises -> locally low
        n_plus += plus.astype(np.uint8)
        n_minus += minus.astype(np.uint8)
    cls = LOOKUP[n_plus.ravel(), n_minus.ravel()].reshape(z.shape)
    cls[~valid] = 0
    return cls


def class_fractions(cls: np.ndarray, collar_px: int = 0) -> dict:
    """Per-class area fractions, optionally excluding an edge collar."""
    if collar_px > 0:
        cls = cls[collar_px:-collar_px, collar_px:-collar_px]
    ok = cls > 0
    tot = max(int(ok.sum()), 1)
    return {CLASS_NAMES[c]: round(float((cls == c).sum()) / tot, 4)
            for c in CLASS_NAMES}


RIDGE_CLASSES = (2, 3, 4, 5)   # peak, ridge, shoulder, spur
VALLEY_CLASSES = (7, 9, 10)    # hollow, valley, pit


def ridge_mask(cls: np.ndarray) -> np.ndarray:
    return np.isin(cls, RIDGE_CLASSES)
