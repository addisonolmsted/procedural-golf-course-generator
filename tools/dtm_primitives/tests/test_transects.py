"""Transect engine on a synthetic straight valley: known params in, exact
centering/stacking recovery out."""

import os
import sys

import numpy as np

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
from dtm_primitives import frame, profiles, transects  # noqa: E402

CELL = 2.0
N = 400  # 800 m box
ZF, INC, HW, ML, MR, R, K = 92.0, 10.0, 14.0, 0.45, 0.22, 5.0, 4.0


def synthetic_valley():
    """Straight west-east valley at y=400 on a gentle plane, row0=north."""
    ix = np.arange(N)
    x, y = np.meshgrid(CELL / 2 + ix * CELL, CELL / 2 + ix * CELL)  # [ysouth, x]
    d = y - 400.0  # signed offset; tangent = +x so +d (north) = left
    t = (ZF + INC) + 0.004 * d
    wall = np.where(d > 0, ML, MR)
    v = ZF + profiles.ramp(np.abs(d) - HW, wall, R)
    z_south = profiles.smin(t, v, K)
    return z_south[::-1]  # to row0=north (TIF orientation)


def masks_all_valid():
    z = np.ones((N, N), np.uint8)
    return {"inpaint": np.zeros_like(z), "water": np.zeros_like(z),
            "road": np.zeros_like(z), "valid": z}


CFG = {"min_valid_frac": 0.9, "inpaint_frac_max": 0.4,
       "road_floor_halfwidth_m": 20.0, "foreign_corridor_pad_m": 20.0}


def test_stack_recovers_cross_section():
    nat = synthetic_valley()
    # centerline deliberately offset 6 m north of the true floor line
    cl = np.stack([np.linspace(60, 740, 137), np.full(137, 406.0)], 1)
    st = transects.stations(cl, 25.0)
    tr = transects.sample(nat, masks_all_valid(), CELL, st, 120.0, 2.0)
    tr = transects.recenter(tr, search_m=30.0, probe_min_m=6.0)
    # centering must find the true floor 6 m south (d = −6)
    assert np.abs(np.median(tr.floor_offset) + 6.0) < 2.0 + 1e-9
    keep = transects.keep_mask(tr, CFG)
    assert keep.all()
    reaches = transects.stack(tr, keep, reach_len_m=300.0, min_per_reach=6)
    assert len(reaches) >= 2
    r0 = reaches[0]
    # median stacked profile matches the analytic cross-section
    za = profiles.valley_cross_model(r0.d, ZF, INC, 0.004, 0.004, HW, ML, MR,
                                     R, K)
    fin = np.isfinite(r0.z_med) & (np.abs(r0.d) <= 100)
    rms = float(np.sqrt(np.mean((r0.z_med[fin] - za[fin]) ** 2)))
    assert rms < 0.15, f"stacked-profile RMS {rms}"
    # floor z per station ~ ZF
    assert abs(np.nanmedian(tr.z_floor) - ZF) < 0.3


def test_discard_rules():
    nat = synthetic_valley()
    masks = masks_all_valid()
    # north-south road strip over x in [190, 220] — transects are north-south
    # too (east-west centerline), so only stations INSIDE the strip sample it
    masks["road"][:, 95:110] = 1
    cl = np.stack([np.linspace(60, 740, 137), np.full(137, 400.0)], 1)
    st = transects.stations(cl, 25.0)
    tr = transects.sample(nat, masks, CELL, st, 120.0, 2.0)
    tr = transects.recenter(tr, search_m=30.0, probe_min_m=6.0)
    keep = transects.keep_mask(tr, CFG)
    hit = (st.xy[:, 0] >= 191.0) & (st.xy[:, 0] <= 219.0)
    assert hit.any()
    assert (~keep[hit]).all()
    assert keep[~hit].all()


def test_foreign_corridor_discard():
    nat = synthetic_valley()
    cl = np.stack([np.linspace(60, 740, 137), np.full(137, 400.0)], 1)
    st = transects.stations(cl, 25.0)
    tr = transects.sample(nat, masks_all_valid(), CELL, st, 120.0, 2.0)
    tr = transects.recenter(tr, search_m=30.0, probe_min_m=6.0)
    other = np.stack([np.full(50, 400.0), np.linspace(0, 800, 50)], 1)
    keep = transects.keep_mask(tr, CFG, foreign=[(other, 30.0)])
    near = np.abs(st.xy[:, 0] - 400.0) <= 50.0
    assert (~keep[near]).all()
    assert keep[~near].all()
