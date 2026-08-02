"""Flow routing invariants — chiefly that flats do not drain in straight
cardinal lines, which is the artifact flow.py exists to remove."""

import pathlib
import sys

import numpy as np

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))

from macro_campaign import flow  # noqa: E402

CELL = 2.0


def test_filled_lake_drains_from_every_cell():
    """The real input: a depression already flooded to its spill level, i.e.
    a flat pool (`extract` runs `mcore.fill_depressions` first). Every pool
    cell must acquire a receiver and reach the spill."""
    ny = nx = 40
    z = np.full((ny, nx), 10.0)
    pool = np.zeros(z.shape, bool)
    yy, xx = np.mgrid[0:ny, 0:nx]
    pool[((yy - 20) ** 2 + (xx - 20) ** 2) < 64] = True
    z[pool] = 9.0            # dead-level flooded pool
    z[13, 20] = 8.5          # its spill, just outside the pool rim
    z[0:13, 20] = np.linspace(6.0, 8.4, 13)  # outflow channel to the edge
    rec, _ = flow.receivers(z, CELL)
    assert (rec[pool] >= 0).all(), "flooded pool left cells undrained"
    # Trace from the pool centre: it must leave the pool.
    y, x = 20, 20
    for _ in range(4 * ny * nx):
        r = rec[y, x]
        if r < 0:
            break
        y, x = divmod(int(r), nx)
    assert not pool[y, x], f"pool centre terminated inside the pool at {(y, x)}"


def test_true_pit_stays_an_outlet():
    """A pit whose every neighbour is higher must NOT be handed a receiver:
    pointing uphill would create a cycle the accumulation sweep cannot
    order. Callers fill depressions first; an unfilled pit is a real outlet."""
    z = np.full((20, 20), 10.0)
    z[10, 10] = 5.0
    rec, _ = flow.receivers(z, CELL)
    assert rec[10, 10] < 0


def test_flat_does_not_drain_along_a_single_row():
    """A perfectly level plateau with one low outlet. The old D8 broke ties
    toward the first cardinal direction, so every plateau cell pointed the
    same way and the traced channel was a straight row. The BFS must instead
    fan out toward the outlet, using both axes."""
    ny = nx = 41
    z = np.full((ny, nx), 5.0)
    z[20, 0] = 0.0  # outlet on the west edge, mid-height
    rec, _ = flow.receivers(z, CELL)

    # Collect the receiver directions of the flat interior.
    dirs = set()
    for y in range(1, ny - 1):
        for x in range(1, nx - 1):
            r = rec[y, x]
            if r < 0:
                continue
            ry, rx = divmod(int(r), nx)
            dirs.add((ry - y, rx - x))
    assert len(dirs) > 1, f"flat drains in one direction only: {dirs}"
    # And flow must actually reach the outlet from a far corner.
    y, x = ny - 2, nx - 2
    for _ in range(4 * ny * nx):
        r = rec[y, x]
        if r < 0:
            break
        y, x = divmod(int(r), nx)
    assert (y, x) == (20, 0), f"far cell drained to {(y, x)}, not the outlet"


def test_accumulation_totals_the_grid():
    """Every cell counts itself once, so the outlets' accumulation must sum
    to the cell count — a cheap check that the topological sweep is complete
    and loop-free."""
    ny = nx = 30
    yy, xx = np.mgrid[0:ny, 0:nx]
    z = 20.0 - 0.3 * yy - 0.1 * xx
    rec, _ = flow.receivers(z, CELL)
    acc = flow.accumulate(rec)
    outlets = rec < 0
    assert acc[outlets].sum() == ny * nx
    assert acc.min() >= 1


def test_drop_measured_on_a_different_surface():
    """Receivers routed on the filled surface, gradient read on the raw one:
    a filled pit must report ~0 fall, not the epsilon the fill invented."""
    ny = nx = 20
    yy, _xx = np.mgrid[0:ny, 0:nx]
    raw = 10.0 - 0.2 * yy
    raw[10, 10] = 0.0  # a pit
    filled = raw.copy()
    filled[10, 10] = raw[10, 9] + 1e-4
    rec, _ = flow.receivers(filled, CELL)
    drop = flow.drop_to_receiver(raw, rec, CELL)
    assert np.isfinite(drop[rec >= 0]).all()
    # The pit is far below its receiver on the RAW surface -> negative fall.
    assert drop[10, 10] < 0.0
