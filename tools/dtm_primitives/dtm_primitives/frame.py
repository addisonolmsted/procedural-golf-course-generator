"""The course-local coordinate frame — the ONE module that owns row/col math.

Local frame: x = (col + 0.5)·cell eastward, y = (nrows − 1 − row + 0.5)·cell
northward, i.e. local = utm − (window_utm[0], window_utm[1]). This is
bit-identical to the Rust Grid/GridSpec pixel-center convention (origin at
cell/2, row 0 = south) and to read_hg01's row-flip. TIF arrays arrive
row 0 = north; HG01 arrays (after read_hg01) are row 0 = south.
"""

from __future__ import annotations

import numpy as np


def rc_to_local(rows, cols, cell: float, nrows: int):
    """TIF (row, col) centers -> local (x, y)."""
    rows = np.asarray(rows, dtype=np.float64)
    cols = np.asarray(cols, dtype=np.float64)
    x = (cols + 0.5) * cell
    y = (nrows - 1 - rows + 0.5) * cell
    return x, y


def local_to_rc(x, y, cell: float, nrows: int):
    """Local (x, y) -> fractional TIF (row, col); row/col of the containing
    pixel center at integer values."""
    x = np.asarray(x, dtype=np.float64)
    y = np.asarray(y, dtype=np.float64)
    col = x / cell - 0.5
    row = (nrows - 1) - (y / cell - 0.5)
    return row, col


def _bilinear(arr: np.ndarray, row: np.ndarray, col: np.ndarray) -> np.ndarray:
    """Bilinear on fractional (row, col); NaN outside or at any NaN corner."""
    h, w = arr.shape
    out = np.full(row.shape, np.nan)
    ok = (row >= 0) & (row <= h - 1) & (col >= 0) & (col <= w - 1)
    if not ok.any():
        return out
    r = np.clip(row[ok], 0, h - 1)
    c = np.clip(col[ok], 0, w - 1)
    r0 = np.floor(r).astype(np.intp)
    c0 = np.floor(c).astype(np.intp)
    r1 = np.minimum(r0 + 1, h - 1)
    c1 = np.minimum(c0 + 1, w - 1)
    fr = r - r0
    fc = c - c0
    v = (arr[r0, c0] * (1 - fr) * (1 - fc) + arr[r0, c1] * (1 - fr) * fc
         + arr[r1, c0] * fr * (1 - fc) + arr[r1, c1] * fr * fc)
    out[ok] = v
    return out


def bilinear_tif(arr: np.ndarray, x, y, cell: float) -> np.ndarray:
    """Sample a row0=north array (float; NaN = invalid) at local (x, y)."""
    row, col = local_to_rc(x, y, cell, arr.shape[0])
    return _bilinear(arr.astype(np.float64, copy=False), row, col)


def nearest_tif(arr: np.ndarray, x, y, cell: float, fill=0):
    """Nearest-neighbor sample of a row0=north array (masks) at local (x, y)."""
    row, col = local_to_rc(x, y, cell, arr.shape[0])
    h, w = arr.shape
    ri = np.rint(row).astype(np.intp)
    ci = np.rint(col).astype(np.intp)
    ok = (ri >= 0) & (ri < h) & (ci >= 0) & (ci < w)
    out = np.full(row.shape, fill, dtype=arr.dtype)
    out[ok] = arr[ri[ok], ci[ok]]
    return out


def bilinear_hg01(h_south: np.ndarray, cell: float, x, y) -> np.ndarray:
    """Sample an HG01 array AFTER read_hg01 (row 0 = south) at local (x, y):
    h[y_idx, x_idx] sits at world (cell/2 + x_idx·cell, cell/2 + y_idx·cell)."""
    x = np.asarray(x, dtype=np.float64)
    y = np.asarray(y, dtype=np.float64)
    col = x / cell - 0.5
    row = y / cell - 0.5
    return _bilinear(h_south.astype(np.float64, copy=False), row, col)


def block_index_to_local(rows_b, cols_b, block_cell: float, nrows_b: int):
    """Centers of block-mean cells (row0=north blocks) -> local (x, y)."""
    return rc_to_local(rows_b, cols_b, block_cell, nrows_b)
