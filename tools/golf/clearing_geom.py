"""Where a hole's clearing reaches, as pure geometry.

No randomness lives here, so every function is testable on its own and both
sides of the comparison -- the corpus measurement and our generator -- use one
definition of "20 m off the line in the landing zone".

Two different rasterisers, deliberately, and the difference matters:

`corridor_zone_masks` stamps offsets along NORMALS FROM STATIONS, which is what
`corridor_width.profile_for_hole` does, so the retention and patch statistics
measured on real ground and on ours come off the same geometry. On a bend that
double-counts the inside of the turn, which is a known wart of the instrument
and is why it must be used identically on both sides rather than corrected on
one.

`offsets_for_hole` is an exact nearest-point projection onto the polyline, used
to build the clearing field itself. A real corridor is cut at a true distance
from the line, not at a station normal, and the projection has no inside-of-bend
bias -- which matters because the corpus says trees left in play show no
preference for the inside of a dogleg, so a biased rasteriser is the one place a
spurious preference could creep in.
"""
from __future__ import annotations

import numpy as np

CELL = 10.0
# corridor_width.SEGMENTS verbatim: the boundary belongs to the EARLIER segment
# (`>= a` and `< b`). canopy_sheet.CORRIDOR gives it to the later one. One
# station in forty changes hands; the instrument's convention wins.
SEGMENTS = (("tee", 0.0, 0.25), ("landing", 0.25, 0.7), ("green", 0.7, 1.0))
HALF_W = {"tee": 18.0, "landing": 30.0, "green": 35.0}
STAMP_MAX_M = 35.0          # canopy_sheet.corridor_mask's offset span, matched


def arc_cum(p):
    d = np.hypot(np.diff(p[:, 0]), np.diff(p[:, 1]))
    return np.concatenate([[0.0], np.cumsum(d)])


def at_arc(p, cum, s):
    s = np.atleast_1d(np.asarray(s, float))
    return np.stack([np.interp(s, cum, p[:, 0]), np.interp(s, cum, p[:, 1])], -1)


def corridor_zone_masks(spine, shape, cell=CELL):
    """The tapered corridor, split by segment, stamped the instrument's way.

    Mirrors `canopy_sheet.corridor_mask` step for step -- same half-step arc
    sampling, same +-35 m offset span, same forward-difference normal -- and
    only adds the per-segment split the retention statistics need.
    """
    p = np.asarray(spine, float)
    out = {k: np.zeros(shape, bool) for k, _a, _b in SEGMENTS}
    if len(p) < 2:
        return out
    cum = arc_cum(p)
    L = float(cum[-1])
    if L < 50.0:
        return out
    ss = np.arange(0.0, L + 1e-9, cell / 2.0)
    q = at_arc(p, cum, ss)
    fwd = at_arc(p, cum, np.clip(ss + 10, 0, L)) - at_arc(p, cum, np.clip(ss - 10, 0, L))
    nrm = np.stack([-fwd[:, 1], fwd[:, 0]], -1)
    nrm /= np.maximum(np.hypot(nrm[:, 0], nrm[:, 1]), 1e-9)[:, None]
    frac = ss / max(L, 1.0)
    offs = np.arange(-STAMP_MAX_M, STAMP_MAX_M + 1e-9, cell / 2.0)
    for name, a, b in SEGMENTS:
        zsel = (frac >= a) & (frac < b) if b < 1.0 else (frac >= a)
        if not zsel.any():
            continue
        hw = HALF_W[name]
        for o in offs:
            if abs(o) > hw:
                continue
            ys = (q[zsel, 0] + nrm[zsel, 0] * o) / cell
            xs = (q[zsel, 1] + nrm[zsel, 1] * o) / cell
            yi = np.clip(ys.astype(int), 0, shape[0] - 1)
            xi = np.clip(xs.astype(int), 0, shape[1] - 1)
            out[name][yi, xi] = True
    # a cell claimed by two segments belongs to the earlier one, as above
    out["landing"] &= ~out["tee"]
    out["green"] &= ~(out["tee"] | out["landing"])
    return out


def project_polyline(p, ys, xs):
    """Exact nearest point on the polyline for every (y, x): distance, side, arc.

    Cells beyond either end project onto the endpoint, which gives the corridor
    a rounded cap -- the same shape the tee and green discs have, so the three
    terms meet without a seam.
    """
    p = np.asarray(p, float)
    cum = arc_cum(p)
    best_d = np.full(ys.shape, np.inf)
    best_s = np.zeros(ys.shape)
    best_c = np.zeros(ys.shape)
    for i in range(len(p) - 1):
        a, b = p[i], p[i + 1]
        d = b - a
        L2 = float(d[0] * d[0] + d[1] * d[1])
        if L2 < 1e-12:
            continue
        vy, vx = ys - a[0], xs - a[1]
        t = np.clip((vy * d[0] + vx * d[1]) / L2, 0.0, 1.0)
        py, px = a[0] + t * d[0], a[1] + t * d[1]
        dist = np.hypot(ys - py, xs - px)
        cross = d[0] * vx - d[1] * vy          # >0 on one side, <0 on the other
        take = dist < best_d
        best_d = np.where(take, dist, best_d)
        best_s = np.where(take, cum[i] + t * np.sqrt(L2), best_s)
        best_c = np.where(take, np.sign(cross), best_c)
    return best_d, best_c, best_s / max(float(cum[-1]), 1e-9)


def seg_index(frac):
    """Segment id per cell, following SEGMENTS' boundary convention."""
    out = np.full(frac.shape, 2, np.int8)
    out[frac < SEGMENTS[1][2]] = 1
    out[frac < SEGMENTS[0][2]] = 0
    return out


def ramp(table_x, table_y, d):
    """Linear interpolation of a measured profile, flat at 1.0 past its end."""
    return np.interp(d, table_x, table_y, left=table_y[0], right=1.0)


def cell_centres(shape, cell=CELL):
    yy, xx = np.mgrid[0:shape[0], 0:shape[1]]
    return yy * cell + cell / 2.0, xx * cell + cell / 2.0


def hole_keep(hole, shape, model, cell=CELL):
    """Keep probability for one hole: one profile of distance to its centre line.

    No green disc and no tee disc. The projection caps the polyline at both
    ends, so a cell past the green measures its distance to the green and a cell
    behind the tee measures its distance to the tee -- the caps ARE the discs,
    and they carry the green and tee segments' own measured profiles.
    """
    p = np.asarray(hole["spine"], float)
    ys, xs = cell_centres(shape, cell)
    d, side, frac = project_polyline(p, ys, xs)
    seg = seg_index(frac)
    par = int(hole["par"])
    keep = np.ones(shape, float)
    L = float(arc_cum(p)[-1])
    # the approach is read a little further out than it is, which pulls the
    # corridor in; the green surround (last 15 m and the cap past the green) is
    # not shifted, it already matched
    narrow = getattr(model, "APPROACH_NARROW_M", 0.0)
    approach = (seg == 2) & (frac * L < L - getattr(model, "APPROACH_KEEP_END_M", 15.0))
    d_eff = np.where(approach, d + narrow, d)
    for zi, z in enumerate(("tee", "landing", "green")):
        curve = model.shape_for(par, z)
        if curve is None:
            continue
        keep = np.where(seg == zi, ramp(model.SHAPE_X, curve, d_eff), keep)
    return keep, d, side, frac
