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
             skip_m: float = 20.0, flat_deg: float = 1.0,
             return_dirs: bool = False):
    """Geomorphon class raster (uint8, 0 = undefined/nodata edge).

    With `return_dirs`, also returns the per-direction ternary as two uint8
    bitmasks `(plus_bits, minus_bits)`, bit k = direction `_DIRS[k]`. The
    class lookup indexes only the COUNTS and throws the arrangement away —
    but the arrangement is where saddles live (two opposed rising directions
    crossing two opposed falling ones), and it is a free by-product here.

    EDGE CAUTION: rays that leave the array see nothing, so cells within
    `lookup_m` of the edge classify toward `flat` — an artifact of the mask,
    not the terrain. Use `collar_px(lookup_m, cell)` to exclude them.
    """
    z = z.astype(np.float64)
    t = np.tan(np.radians(flat_deg))
    n_steps_max = max(int(round(lookup_m / cell)), 2)
    step0 = max(int(round(skip_m / cell)), 1)
    n_plus = np.zeros(z.shape, np.uint8)
    n_minus = np.zeros(z.shape, np.uint8)
    plus_bits = np.zeros(z.shape, np.uint8)
    minus_bits = np.zeros(z.shape, np.uint8)
    valid = np.isfinite(z)
    for k, (dy, dx) in enumerate(_DIRS):
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
        plus_bits |= (plus.astype(np.uint8) << k)
        minus_bits |= (minus.astype(np.uint8) << k)
    cls = LOOKUP[n_plus.ravel(), n_minus.ravel()].reshape(z.shape)
    cls[~valid] = 0
    if return_dirs:
        return cls, plus_bits, minus_bits
    return cls


def collar_px(lookup_m: float, cell: float) -> int:
    """Edge collar inside which classes are mask artifacts (see `classify`)."""
    return int(np.ceil(lookup_m / cell))


# Direction angles for `_DIRS[k]`, radians, y NORTH-up world convention:
# _DIRS is (dy, dx) in ARRAY rows, so world bearing = atan2(-dy, dx) if row 0
# is north. Our grids are row 0 = SOUTH (origin SW), so bearing = atan2(dy, dx).
DIR_ANGLES = np.array([np.arctan2(dy, dx) for dy, dx in _DIRS])


def saddle_axes(plus_bits: np.ndarray, minus_bits: np.ndarray,
                axial_min: float = 0.8):
    """Saddles from the per-direction ternary, with their two axes.

    A saddle is >= 2 rising directions and >= 2 falling directions, each set
    axially aligned (antipodal pairs), the two axes roughly perpendicular.
    Doubling the angle makes antipodal directions coincide, so axial
    alignment is just the resultant length of `e^{i 2 theta}` over the set.

    Returns `(saddle_mask, fall_axis_rad, rise_axis_rad)`; the axes are in
    `[0, pi)` and are the OPEN (fall-away) and BLOCKED (ridge) axes — the
    free approach-bearing prior.
    """
    e2 = np.exp(2j * DIR_ANGLES)
    shape = plus_bits.shape
    P = np.zeros(shape, np.complex128)
    M = np.zeros(shape, np.complex128)
    n_p = np.zeros(shape, np.int16)
    n_m = np.zeros(shape, np.int16)
    for k in range(8):
        pb = (plus_bits >> k) & 1
        mb = (minus_bits >> k) & 1
        P += pb * e2[k]
        M += mb * e2[k]
        n_p += pb
        n_m += mb
    with np.errstate(invalid="ignore", divide="ignore"):
        ax_p = np.abs(P) / np.maximum(n_p, 1)
        ax_m = np.abs(M) / np.maximum(n_m, 1)
    fall = np.mod(np.angle(P) / 2.0, np.pi)
    rise = np.mod(np.angle(M) / 2.0, np.pi)
    # perpendicularity of the two axes, in axial space
    d = np.mod(fall - rise, np.pi)
    perp = np.minimum(d, np.pi - d)          # [0, pi/2]
    saddle = ((n_p >= 2) & (n_m >= 2)
              & (ax_p > axial_min) & (ax_m > axial_min)
              & (perp > np.pi / 2 - np.pi / 6))   # within 30 deg of square
    return saddle, fall, rise


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
