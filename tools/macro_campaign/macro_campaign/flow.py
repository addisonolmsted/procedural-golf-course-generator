"""Flow routing with flat resolution.

`metrics.core.d8_accumulation` has no flat handling: it picks the first
`_D8` direction that beats a zero baseline, and diagonals are divided by
sqrt(2), so on any surface where several neighbours are equal-elevation the
receiver is always the same cardinal direction. `fill_depressions` makes
that worse — it grades filled depressions by a 1e-4 epsilon whose direction
is an artifact of heap pop order, not of the terrain. Together they draw
dead-straight axis-aligned "channels" across every lake, pond, sinkhole and
graded flat, which is what the QA viewer showed on nearly every tile.

This module keeps the epsilon-filled surface (it is a fine way to guarantee
every cell drains) but resolves flats properly, Barnes-style: cells with no
strictly-lower neighbour drain along a BFS tree grown inward from the flat's
SPILL cells (flat cells that touch strictly-lower terrain). Flow across a
lake then converges on the real outlet instead of marching along a row.
"""

import numpy as np
from scipy import ndimage

# (dy, dx) — same 8-neighbourhood as metrics.core, order is irrelevant here
# because ties are broken by the flat BFS rather than by iteration order.
_D8 = [(-1, -1), (-1, 0), (-1, 1), (0, -1), (0, 1), (1, -1), (1, 0), (1, 1)]


def _shift(a, dy, dx, fill):
    """`a` shifted so out[y, x] == a[y + dy, x + dx]; `fill` off-grid."""
    out = np.full_like(a, fill)
    ys_dst = slice(max(0, -dy), a.shape[0] - max(0, dy))
    ys_src = slice(max(0, dy), a.shape[0] - max(0, -dy))
    xs_dst = slice(max(0, -dx), a.shape[1] - max(0, dx))
    xs_src = slice(max(0, dx), a.shape[1] - max(0, -dx))
    out[ys_dst, xs_dst] = a[ys_src, xs_src]
    return out


def receivers(zfill: np.ndarray, cell: float) -> tuple[np.ndarray, np.ndarray]:
    """Steepest-descent receiver index per cell, with flats resolved.

    Returns `(rec, slope)` where `rec` is the flat receiver index
    (`row * nx + col`, or -1 for outlets/undrained) and `slope` is the
    gradient toward it measured on `zfill` (0 across flats).
    """
    ny, nx = zfill.shape
    idx = np.arange(ny * nx, dtype=np.int64).reshape(ny, nx)
    best_slope = np.zeros(zfill.shape)
    rec = np.full(zfill.shape, -1, dtype=np.int64)

    for dy, dx in _D8:
        zn = _shift(zfill, dy, dx, np.inf)
        dist = cell * (1.4142135623730951 if dy and dx else 1.0)
        slope = (zfill - zn) / dist
        take = slope > best_slope
        best_slope = np.where(take, slope, best_slope)
        rec = np.where(take, _shift(idx, dy, dx, -1), rec)

    # --- flat resolution -------------------------------------------------
    # A cell with no strictly-lower neighbour sits on a flat. Grow a BFS
    # outward from the flat's SPILL cells — the plateau-edge cells that did
    # find a downhill neighbour — so interior cells point one step closer to
    # the real outlet instead of all picking the same cardinal direction.
    #
    # A candidate receiver must be at the same elevation or lower. Without
    # that test a flat cell can adopt an UPSLOPE neighbour that drains back
    # into it, and the pair becomes a 2-cycle that no accumulation sweep can
    # order (it silently drops the whole basin from the totals).
    flat = rec < 0
    if flat.any():
        assigned = ~flat  # cells whose receiver is already known
        while True:
            newly = np.zeros(flat.shape, bool)
            for dy, dx in _D8:
                src_ok = _shift(assigned, dy, dx, False)
                src_z = _shift(zfill, dy, dx, np.inf)
                cand = flat & ~assigned & ~newly & src_ok & (src_z <= zfill)
                if not cand.any():
                    continue
                rec = np.where(cand, _shift(idx, dy, dx, -1), rec)
                newly |= cand
            if not newly.any():
                break
            assigned |= newly

    return rec, best_slope


def accumulate(rec: np.ndarray) -> np.ndarray:
    """Number of cells draining through each cell (including itself)."""
    ny, nx = rec.shape
    flat_rec = rec.ravel()
    acc = np.ones(ny * nx, dtype=np.int64)
    # In-degree topological order: a cell can be pushed once every cell that
    # drains into it has been. Deterministic and O(n).
    indeg = np.zeros(ny * nx, dtype=np.int64)
    has = flat_rec >= 0
    np.add.at(indeg, flat_rec[has], 1)
    stack = np.nonzero(indeg == 0)[0].tolist()
    while stack:
        i = stack.pop()
        r = flat_rec[i]
        if r < 0:
            continue
        acc[r] += acc[i]
        indeg[r] -= 1
        if indeg[r] == 0:
            stack.append(r)
    return acc.reshape(ny, nx)


def drop_to_receiver(z: np.ndarray, rec: np.ndarray, cell: float) -> np.ndarray:
    """Gradient toward each cell's receiver measured on `z`, which may differ
    from the surface the receivers were routed on (raw vs depression-filled).
    NaN where there is no receiver."""
    ny, nx = z.shape
    out = np.full(z.shape, np.nan)
    has = rec >= 0
    if not has.any():
        return out
    ri = rec[has]
    ry, rx = np.divmod(ri, nx)
    yy, xx = np.nonzero(has)
    dist = np.hypot((ry - yy).astype(float), (rx - xx).astype(float)) * cell
    with np.errstate(divide="ignore", invalid="ignore"):
        out[has] = (z[yy, xx] - z[ry, rx]) / dist
    return out
