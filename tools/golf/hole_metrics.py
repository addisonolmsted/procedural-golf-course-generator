"""Hole metrics computed identically on a real hole and on one of ours.

The point of this module is that there is ONE definition of each quantity.
Both sides call the same functions with the same arguments: a centreline
polyline in tile-local metres `(y, x)`, the par, and a 2 m height grid with
its cell size. Nothing here knows whether the line came from an OSM
`golf=hole` way or from our router's spine.

Stations follow the corpus convention (`corpus/shot_profiles.py`), because a
real hole's landing zones are not recorded anywhere:

    drive  at arc  min(DRIVE_M, DRIVE_FRAC * L)
    second at arc  min(drive + DRIVE_M, L - APPROACH_RESERVE_M)   (par 5)
    green  at arc  L

DOGLEG (2026-09-16). The previous measure -- the largest turn at an interior
vertex with both legs over 20 m -- is retired here, for two reasons. It reads
a sharp dogleg on a hole that is straight for 400 m and kinks in the last
40 m, although the shot that bend affects is a pitch you simply aim at. And
on our spines the vertices ARE the landing zones, two or three of them, while
on a real way they are wherever the mapper clicked, so it measured cartography
as much as architecture. The replacement is the angle that carries the
strategic content: how far off the green you must aim the shot you are about
to hit.

    drive dogleg  = angle AT THE TEE between the line to the drive station
                    and the line to the green
    second dogleg = the same angle taken at the drive station, par 5 only

Both use three points at fixed arc positions, so they are immune to vertex
placement. `lateral_offset` is the shape companion that does not care where
the bend sits: the greatest perpendicular distance of the centreline from the
tee-to-green chord.

Profile quantities use `holes.py`'s recipe verbatim -- 5 m arc resample, 15 m
boxcar -- so they are comparable with `out/hole_profiles.json`. Ground is read
by TRUNCATED index, as `holes.py`, `route_audit.py` and the router's
`trunc_clip` all do.
"""
import math

import numpy as np
from scipy.ndimage import uniform_filter1d

STEP_M = 5.0                 # arc resample for the profile
SMOOTH_M = 15.0              # boxcar over the profile
DRIVE_M = 220.0              # a full shot
DRIVE_FRAC = 0.63            # the drive never runs past 63 % of the hole
APPROACH_RESERVE_M = 60.0    # the par-5 second stops this short of the green
SECTION_HALF_M = 45.0        # cross section half width
CROSS_RUN_M = 30.0           # cross slope is measured over +-30 m
DIR_WINDOW_M = 20.0          # local play direction is taken over +-20 m
APPROACH_RUN_M = 100.0       # the approach grade is the last 100 m
REACHABLE_M = 230.0          # a par 5 is reachable in two from the drive station
PARALLEL_DEG = 20.0          # two holes are "parallel" within this bearing
PARALLEL_M = 60.0            # ... and this close
MIN_LEN_M = 60.0


# ------------------------------------------------------------------ sampling --

def ground(z2, cell, ys, xs):
    """Height by TRUNCATED index, clipped to the tile."""
    yi = np.clip((np.asarray(ys) / cell).astype(int), 0, z2.shape[0] - 1)
    xi = np.clip((np.asarray(xs) / cell).astype(int), 0, z2.shape[1] - 1)
    return z2[yi, xi].astype(float)


def arc_cum(pts):
    p = np.asarray(pts, float)
    return np.concatenate([[0.0], np.cumsum(np.hypot(*np.diff(p, axis=0).T))])


def at_arc(pts, cum, s):
    """The point(s) at arc position(s) `s` along the polyline."""
    p = np.asarray(pts, float)
    return np.stack([np.interp(s, cum, p[:, 0]), np.interp(s, cum, p[:, 1])], axis=-1)


def profile(z2, cell, pts):
    """(arc, smoothed height) along the polyline, `holes.py`'s recipe."""
    cum = arc_cum(pts)
    L = float(cum[-1])
    n = max(int(L / STEP_M), 4)
    ss = np.linspace(0.0, L, n + 1)
    q = at_arc(pts, cum, ss)
    z = ground(z2, cell, q[:, 0], q[:, 1])
    return ss, uniform_filter1d(z, max(int(SMOOTH_M / STEP_M), 1), mode="nearest")


def play_dir(pts, cum, s):
    """Unit direction of play at arc `s`, over a +-DIR_WINDOW_M window."""
    L = float(cum[-1])
    a = at_arc(pts, cum, max(0.0, s - DIR_WINDOW_M))
    b = at_arc(pts, cum, min(L, s + DIR_WINDOW_M))
    v = b - a
    n = float(np.hypot(*v))
    return v / n if n > 1e-9 else np.array([1.0, 0.0])


def angle_between(u, v):
    """Unsigned angle between two vectors, degrees."""
    nu, nv = float(np.hypot(*u)), float(np.hypot(*v))
    if nu < 1e-9 or nv < 1e-9:
        return 0.0
    return math.degrees(math.acos(float(np.clip(np.dot(u, v) / (nu * nv), -1.0, 1.0))))


# ------------------------------------------------------------------- stations --

def stations(L, par):
    """(drive arc, second arc or None) on the corpus convention."""
    if par < 4:
        return None, None
    s1 = min(DRIVE_M, DRIVE_FRAC * L)
    if par < 5:
        return s1, None
    s2 = min(s1 + DRIVE_M, L - APPROACH_RESERVE_M)
    return s1, (s2 if s2 > s1 + 1.0 else None)


# --------------------------------------------------------------- cross slope --

def cross_slope(z2, cell, pts, cum, s):
    """Cross slope at arc `s` as a percentage, plus the section's own relief.

    The SIGN is only ever used for "does the tilt change from hole to hole",
    and every reported figure is either an absolute value or a count of sign
    changes, both of which survive a global flip. Which side of the line
    `off > 0` falls on depends on whether tile y runs north or down the
    array, which is not fixed here, so the sign is deliberately not given a
    left/right meaning."""
    u = play_dir(pts, cum, s)
    n = np.array([-u[1], u[0]])            # one of the two normals in (y, x)
    c = at_arc(pts, cum, s)
    off = np.arange(-SECTION_HALF_M, SECTION_HALF_M + 2.0, 2.0)
    q = c[None, :] + n[None, :] * off[:, None]
    z = ground(z2, cell, q[:, 0], q[:, 1])
    z = uniform_filter1d(z, 7, mode="nearest")     # ~14 m, the profile's smoothing
    zl = float(np.interp(-CROSS_RUN_M, off, z))    # one side
    zr = float(np.interp(CROSS_RUN_M, off, z))     # the other
    return (zl - zr) / (2.0 * CROSS_RUN_M) * 100.0, float(z.max() - z.min())


# ------------------------------------------------------------------ one hole --

def hole_metrics(z2, cell, pts, par):
    """Every per-hole quantity, or None when the line is too short."""
    pts = np.asarray(pts, float)
    cum = arc_cum(pts)
    L = float(cum[-1])
    if L < MIN_LEN_M:
        return None
    ss, zs = profile(z2, cell, pts)
    tee, green = pts[0], at_arc(pts, cum, L)
    chord = float(np.hypot(*(green - tee)))
    s1, s2 = stations(L, par)

    # shape
    drive_dog = second_dog = None
    if s1 is not None:
        p1 = at_arc(pts, cum, s1)
        drive_dog = angle_between(p1 - tee, green - tee)
        if s2 is not None:
            p2 = at_arc(pts, cum, s2)
            second_dog = angle_between(p2 - p1, green - p1)
    # greatest perpendicular offset of the line from the tee-green chord
    if chord > 1e-6:
        d = (green - tee) / chord
        rel = pts - tee
        offs = np.abs(rel[:, 0] * d[1] - rel[:, 1] * d[0])
        lateral = float(offs.max())
    else:
        lateral = 0.0

    # profile
    ch = np.linspace(zs[0], zs[-1], len(zs))
    dz = np.diff(zs)
    i_app = int(np.searchsorted(ss, max(0.0, L - APPROACH_RUN_M)))
    run = max(L - float(ss[i_app]), 1.0)
    out = dict(
        par=int(par), length_m=L, chord_m=chord,
        sinuosity=(L / chord if chord > 1e-6 else 1.0),
        drive_dogleg_deg=drive_dog, second_dogleg_deg=second_dog,
        lateral_offset_m=lateral, lateral_offset_frac=lateral / L,
        net_dz=float(zs[-1] - zs[0]),
        total_climb=float(dz[dz > 0].sum()),
        total_drop=float(-dz[dz < 0].sum()),
        mean_abs_grade=float(np.abs(dz).sum() / L),
        above_chord=float((zs - ch).max()),
        below_chord=float(-(zs - ch).min()),
        approach_grade_pct=float((zs[-1] - zs[i_app]) / run * 100.0),
    )
    cs_g, relief_g = cross_slope(z2, cell, pts, cum, L)
    out["cross_green_pct"], out["section_relief_green_m"] = cs_g, relief_g
    if s1 is not None:
        cs_d, relief_d = cross_slope(z2, cell, pts, cum, s1)
        out["cross_drive_pct"], out["section_relief_drive_m"] = cs_d, relief_d
        out["reach_in_two"] = bool(par == 5 and float(np.hypot(*(green - at_arc(pts, cum, s1)))) <= REACHABLE_M)
    else:
        out["cross_drive_pct"] = out["section_relief_drive_m"] = None
        out["reach_in_two"] = None
    out["bearing_deg"] = math.degrees(math.atan2(green[0] - tee[0], green[1] - tee[1]))
    out["tee_yx"] = [float(tee[0]), float(tee[1])]
    out["green_yx"] = [float(green[0]), float(green[1])]
    return out


# ---------------------------------------------------------------- one course --

def seg_gap_m(a, b):
    """Smallest distance between two polylines, sampled every 10 m."""
    pa, pb = np.asarray(a, float), np.asarray(b, float)
    qa = at_arc(pa, arc_cum(pa), np.arange(0.0, arc_cum(pa)[-1] + 1e-9, 10.0))
    qb = at_arc(pb, arc_cum(pb), np.arange(0.0, arc_cum(pb)[-1] + 1e-9, 10.0))
    d = np.hypot(qa[:, None, 0] - qb[None, :, 0], qa[:, None, 1] - qb[None, :, 1])
    return float(d.min())


def course_metrics(holes, lines):
    """Routing-level quantities over a whole nine or eighteen.

    `holes` are `hole_metrics` dicts in play order; `lines` the matching
    centrelines. Length variety is reported per par as the spread between the
    same-par holes of one course, which is what a player actually notices.

    `corridor_gap_*` includes CONSECUTIVE holes, so the green-to-next-tee
    junction contributes; that is true on both sides and so stays comparable,
    but a small value is not by itself a defect."""
    n = len(holes)
    out = dict(n_holes=n, total_length_m=sum(h["length_m"] for h in holes))
    for par in (3, 4, 5):
        ls = sorted(h["length_m"] for h in holes if h["par"] == par)
        out[f"par{par}_n"] = len(ls)
        out[f"par{par}_spread_m"] = (ls[-1] - ls[0]) if len(ls) > 1 else None
        out[f"par{par}_min_m"] = ls[0] if ls else None
        out[f"par{par}_max_m"] = ls[-1] if ls else None
    # bearing change between consecutive holes, and walks
    turns, walks = [], []
    for i in range(1, n):
        d = abs(holes[i]["bearing_deg"] - holes[i - 1]["bearing_deg"]) % 360.0
        turns.append(min(d, 360.0 - d))
        walks.append(float(np.hypot(holes[i]["tee_yx"][0] - holes[i - 1]["green_yx"][0],
                                    holes[i]["tee_yx"][1] - holes[i - 1]["green_yx"][1])))
    out["bearing_change_med"] = float(np.median(turns)) if turns else None
    out["same_dir_share"] = (float(np.mean([t < 45.0 for t in turns])) if turns else None)
    out["walk_med_m"] = float(np.median(walks)) if walks else None
    # corridor spacing and near-parallel pairs
    gaps, par_pairs, pairs = [], 0, 0
    for i in range(n):
        best = math.inf
        for j in range(n):
            if i == j:
                continue
            g = seg_gap_m(lines[i], lines[j])
            best = min(best, g)
            if j > i:
                pairs += 1
                d = abs(holes[i]["bearing_deg"] - holes[j]["bearing_deg"]) % 180.0
                d = min(d, 180.0 - d)
                if d < PARALLEL_DEG and g < PARALLEL_M:
                    par_pairs += 1
        gaps.append(best)
    out["corridor_gap_med_m"] = float(np.median(gaps))
    out["corridor_gap_min_m"] = float(np.min(gaps))
    out["parallel_pair_share"] = (par_pairs / pairs) if pairs else 0.0
    # does the ground tilt the same way hole after hole?
    cs = [h["cross_drive_pct"] for h in holes if h["cross_drive_pct"] is not None]
    if len(cs) > 1:
        flips = sum(1 for a, b in zip(cs[:-1], cs[1:]) if (a > 0) != (b > 0))
        out["cross_flip_share"] = flips / (len(cs) - 1)
    else:
        out["cross_flip_share"] = None
    return out
