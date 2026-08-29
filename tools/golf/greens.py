#!/usr/bin/env python3
"""Green-site candidate generation with approach-vector scoring. (P5-P9.)

Consumes `siting.run_siting`'s outputs; emits 75-125 typed, scored candidates
with a (16-bearing x term) approach table each. Prototype of
`crates/course-routing/src/greens.rs`, which may read ONLY contract C2 —
so everything here derives from height + water + flow + the play window.

The typed families exist because "morphological diversity" needs a handle:
without per-type quotas, whichever type scores highest on a seed swamps the
pool and the course becomes nine punchbowls. Types G (valley/interdune flat)
and I (generic pad) are deliberate additions to the reviewer's list — most
real greens sit on unremarkable ground, and without G the aeolian pool is
empty (interdune greens are geomorphon `flat` at low relief_pos, matching
none of the exotic classes).

Hard gates are only the physically impossible (water under the pad, a
channel through it, inside the geomorphon collar); everything else is a
score. Hard constraints force retries, and this pipeline has none.
"""
from __future__ import annotations

import pathlib
import sys
from dataclasses import dataclass, field

import numpy as np
from scipy import ndimage

_TOOLS = pathlib.Path(__file__).resolve().parent.parent
for _p in ("macro_campaign", "golf",):
    sys.path.insert(0, str(_TOOLS / _p))

import playability as play                            # noqa: E402
from siting import (Fields, Morphology, Persistence,  # noqa: E402
                    Siting, GREEN_SLOPE_RR, CLUBHOUSE_RADIUS_M, trapezoid)

N_BEARINGS = 16
APPROACH_LO_M, APPROACH_HI_M = 60.0, 200.0
EYE_M = 1.6

TYPES = ("bench", "spur", "saddle", "punchbowl", "plateau", "dell",
         "valley_flat", "knoll", "generic")


@dataclass
class Candidate:
    yx: tuple[float, float]         # world metres (y, x)
    kind: str
    score: float
    grad: tuple[float, float]       # pad plane gradient (gy, gx), rise/run
    persistence: float
    approach: np.ndarray            # (N_BEARINGS, 4): visible, recept, backdrop, room
    reserved: bool = False          # the loop-closure anchor slot


# --------------------------------------------------------------------------
# P5/P6 — typed seeding over window + halo


def _seed_typed(f: Fields, m: Morphology, p: Persistence,
                i0: int, j0: int, w: int) -> list[tuple[int, int, str]]:
    """(i, j, type) seeds inside the window+halo, gated hard only where
    physically necessary."""
    halo = int(round(120.0 / f.cell))
    a0, a1 = max(0, i0 - halo), min(f.z8.shape[0], i0 + w + halo)
    b0, b1 = max(0, j0 - halo), min(f.z8.shape[1], j0 + w + halo)
    box = np.zeros(f.z8.shape, bool)
    box[a0:a1, b0:b1] = True

    # the geomorphon collar: classes there are artifacts of the array edge
    c = m.collar
    inner = np.zeros(f.z8.shape, bool)
    inner[c:-c, c:-c] = True
    box &= inner

    # hard gates
    ok = box & ~f.wet8
    if f.flow_accum is not None:
        ok &= f.flow_accum < 2.0e4          # ~2 ha contributing area
    lowrough = f.subgrid_rough < 0.55       # a 2 m-rough cell is not a green

    high = f.relief_pos > 0.55
    low = f.relief_pos < 0.45
    edt_break = ndimage.distance_transform_edt(
        f.slope <= GREEN_SLOPE_RR, sampling=f.cell)

    seeds: list[tuple[int, int, str]] = []

    def take(mask, kind, cap=600):
        ys, xs = np.where(mask & ok)
        if len(ys) > cap:
            # strided, not truncated: a raster-order cut takes only the
            # window's south edge for any type with more cells than the cap
            step = len(ys) // cap + 1
            ys, xs = ys[::step], xs[::step]
        for y, x in zip(ys.tolist(), xs.tolist()):
            seeds.append((y, x, kind))

    take((m.cls240 == 4) & f.pad_green, "bench")
    take((m.cls240 == 5) & f.pad_green, "spur")
    take(m.saddle & f.pad_green, "saddle")
    take(np.isin(m.cls80, (7, 10)) & (p.pit > 0.5) & (p.pit < 3.0)
         & lowrough, "punchbowl")
    take((m.cls240 == 1) & high & (edt_break < 60.0) & f.pad_green, "plateau")
    # dell: hollow at the 240 m scale with a partial rim — approximated as
    # hollow-class ground whose pit persistence is small (drains, not a sump)
    take((m.cls240 == 7) & (p.pit < 0.5) & f.pad_green, "dell")
    take((m.cls240 == 1) & low & f.pad_green, "valley_flat")
    take((p.peak > 1.0) & (p.peak < 4.0) & f.pad_green, "knoll")
    take(f.pad_green, "generic")
    return seeds


# --------------------------------------------------------------------------
# P7 — 2 m confirmation on survivors


def confirm_2m(z2: np.ndarray, cell2: float, wet2: np.ndarray,
               y_m: float, x_m: float) -> tuple[bool, tuple[float, float], float]:
    """Exact pad test + plane fit on a 64 m crop of the 2 m grid.

    Returns (passes, plane gradient (gy, gx), residual RMS). The 8 m
    prefilter over-admits (its slope is smoothed); this is the authoritative
    gate, run only on survivors — 400 x 32^2 cells instead of 1501^2.
    """
    r = int(round(32.0 / cell2))
    y, x = int(round(y_m / cell2)), int(round(x_m / cell2))
    if y - r < 0 or x - r < 0 or y + r >= z2.shape[0] or x + r >= z2.shape[1]:
        return False, (0.0, 0.0), 0.0
    zc = z2[y - r:y + r, x - r:x + r]
    wc = wet2[y - r:y + r, x - r:x + r]
    n = int(round(play.GREEN_R / cell2))
    yy, xx = np.mgrid[-r:r, -r:r]
    disc = (yy * yy + xx * xx) <= n * n
    if wc[disc].any():
        return False, (0.0, 0.0), 0.0
    zd = zc[disc]
    if zd.max() - zd.min() > play.GREEN_RELIEF:
        return False, (0.0, 0.0), 0.0
    # plane fit over the disc
    A = np.stack([yy[disc] * cell2, xx[disc] * cell2,
                  np.ones(disc.sum())], 1)
    coef, *_ = np.linalg.lstsq(A, zd, rcond=None)
    gy, gx = float(coef[0]), float(coef[1])
    if np.hypot(gy, gx) > GREEN_SLOPE_RR:
        return False, (gy, gx), 0.0
    resid = float(np.sqrt(np.mean((zd - A @ coef) ** 2)))
    if resid > 0.45:
        return False, (gy, gx), resid
    return True, (gy, gx), resid


# --------------------------------------------------------------------------
# P8 — approach vectors


def approach_table(f: Fields, y: int, x: int,
                   grad: tuple[float, float]) -> np.ndarray:
    """(N_BEARINGS, 4): visible_frac, receptivity, backdrop, corridor room.

    Bearing k points FROM the approach TOWARD the green — a golfer standing
    on bearing k+pi plays IN along -u_k. `visible_frac`: fraction of stances
    at 60-200 m that can see the putting surface (eye 1.6 m, target the
    SURFACE, not a flag — the question is judging the shot, not seeing a
    stick). Receptivity: the pad tilting toward the golfer holds the ball;
    away, it releases — the other, independent half of "benched from one
    side, blind from the other".
    """
    cell = f.cell
    steps = np.arange(1, int(APPROACH_HI_M / cell) + 1)
    out = np.zeros((N_BEARINGS, 4))
    zg = f.z8[y, x]
    gy, gx = grad
    for k in range(N_BEARINGS):
        th = 2.0 * np.pi * k / N_BEARINGS
        uy, ux = np.sin(th), np.cos(th)
        py = np.clip((y + uy * steps).astype(int), 0, f.z8.shape[0] - 1)
        px = np.clip((x + ux * steps).astype(int), 0, f.z8.shape[1] - 1)
        prof = f.z8[py, px]
        d = steps * cell
        lo = d >= APPROACH_LO_M
        # blind test: from stance s, the max angle subtended by any
        # intermediate t < s must not exceed the angle to the green surface
        eye = prof + EYE_M
        vis = np.ones(len(d), bool)
        for si in np.flatnonzero(lo):
            dd = d[si] - d[:si]
            if si and ((prof[:si] - eye[si]) / dd > (zg - eye[si]) / d[si]).any():
                vis[si] = False
        out[k, 0] = float(vis[lo].mean()) if lo.any() else 0.0
        # receptivity: green tilts toward the incoming golfer (+)
        out[k, 1] = float(-(gy * uy + gx * ux))
        # backdrop: mean rise 30-80 m beyond the green along -u
        bs = np.arange(1, int(80.0 / cell) + 1)
        by = np.clip((y - uy * bs).astype(int), 0, f.z8.shape[0] - 1)
        bx = np.clip((x - ux * bs).astype(int), 0, f.z8.shape[1] - 1)
        far = bs * cell >= 30.0
        out[k, 2] = float((f.z8[by, bx] - zg)[far].mean()) if far.any() else 0.0
        # corridor room at the landing zone (150-250 m out)
        land = (d >= 150.0) & (d <= 250.0)
        out[k, 3] = float(f.pad_fair[py, px][land].mean()) if land.any() else 0.0
    return out


# --------------------------------------------------------------------------
# P9 — thinning


def _stamp_thin(cands: list[Candidate], cell: float, shape,
                min_sep_m: float, cap: int) -> list[Candidate]:
    """Greedy best-first with a raster-stamp occupancy grid — the fast,
    exactly-equivalent form of playability.peak_sites' O(n*k) loop."""
    occ = np.zeros(shape, bool)
    sep = max(1, int(round(min_sep_m / cell)))
    out = []
    for c in sorted(cands, key=lambda c: (-c.reserved, -c.score,
                                          c.yx[0], c.yx[1])):
        y, x = int(round(c.yx[0] / cell)), int(round(c.yx[1] / cell))
        if occ[y, x] and not c.reserved:
            continue
        out.append(c)
        if len(out) >= cap:
            break
        occ[max(0, y - sep):y + sep + 1, max(0, x - sep):x + sep + 1] = True
    return out


def generate(z2, cell2, wet2, sit: Siting, f: Fields, m: Morphology,
             p: Persistence, n_target: int = 100,
             hole_lengths=None) -> list[Candidate]:
    if hole_lengths is None:
        hole_lengths = np.array([350., 360., 160., 480., 370., 170., 355., 490., 365.])
    i0, j0 = sit.window_ij
    w = int(round(sit.window_m[2] / f.cell))

    seeds = _seed_typed(f, m, p, i0, j0, w)
    if not seeds:
        return []

    # score seeds cheaply at 8 m: room + type prior. SPREAD BEFORE CONFIRM:
    # rank-truncating globally by room takes the 400 roomiest cells, which
    # all sit in the interiors of the few biggest pads -- measured, the 50 m
    # thinning then collapsed 400 confirmed candidates to 29. Stamp-thin per
    # type at 40 m first, so what reaches the 2 m gate is already spatially
    # diverse, then confirm.
    pre: dict[tuple[int, int], tuple[float, str]] = {}
    for (y, x, kind) in seeds:
        s = f.room[y, x] / 40.0
        if kind != "generic":
            s += 0.6                        # exotic ground earns its look
        old = pre.get((y, x))
        if old is None or s > old[0]:
            pre[(y, x)] = (s, kind)
    by_kind: dict[str, list] = {}
    for (y, x), (s, kind) in pre.items():
        by_kind.setdefault(kind, []).append((s, y, x))
    ranked = []
    sep = max(1, int(round(40.0 / f.cell)))
    for kind, lst in sorted(by_kind.items()):
        occ = np.zeros(f.z8.shape, bool)
        n_took = 0
        for s, y, x in sorted(lst, key=lambda t: (-t[0], t[1], t[2])):
            if occ[y, x]:
                continue
            ranked.append(((y, x), (s, kind)))
            occ[max(0, y - sep):y + sep + 1, max(0, x - sep):x + sep + 1] = True
            n_took += 1
            if n_took >= 90:
                break

    cands: list[Candidate] = []
    for (y, x), (s0, kind) in ranked:
        y_m, x_m = y * f.cell, x * f.cell
        ok, grad, resid = confirm_2m(z2, cell2, wet2, y_m, x_m)
        if not ok:
            continue
        tab = approach_table(f, y, x, grad)
        vis = tab[:, 0].max()
        recept = np.clip(tab[:, 1], -0.05, 0.05).max() / 0.05
        score = (s0 + 1.2 * vis + 0.8 * recept
                 + 0.4 * np.clip(tab[:, 2], 0, 8).max() / 8.0
                 + 0.6 * tab[:, 3].max()
                 - 2.0 * resid)
        cands.append(Candidate((y_m, x_m), kind, float(score), grad,
                               float(max(p.pit[y, x], p.peak[y, x])), tab))

    # reserve the loop anchor: the candidate nearest the clubhouse's
    # reserved green, protected from thinning
    if cands:
        gy, gx = sit.clubhouse.reserved_green
        d = [np.hypot(c.yx[0] - gy, c.yx[1] - gx) for c in cands]
        k = int(np.argmin(d))
        if d[k] <= CLUBHOUSE_RADIUS_M * 1.5:
            cands[k].reserved = True
        else:
            # no confirmed candidate near home: force-add the reserved pad
            ok, grad, resid = confirm_2m(z2, cell2, wet2, gy, gx)
            tab = approach_table(f, int(round(gy / f.cell)),
                                 int(round(gx / f.cell)), grad)
            cands.append(Candidate((gy, gx), "generic", 0.5, grad, 0.0,
                                   tab, reserved=True))

    # type quotas: floor of 5 per present type, remainder by score
    by_type: dict[str, list[Candidate]] = {}
    for c in cands:
        by_type.setdefault(c.kind, []).append(c)
    pool: list[Candidate] = []
    for t, lst in by_type.items():
        lst.sort(key=lambda c: (-c.reserved, -c.score, c.yx[0], c.yx[1]))
        pool.extend(lst[:5])
    rest = [c for c in cands if c not in pool]
    rest.sort(key=lambda c: (-c.score, c.yx[0], c.yx[1]))
    pool.extend(rest)

    thinned = _stamp_thin(pool, f.cell, f.z8.shape, 50.0, n_target + 25)

    # distance-spectrum repair: every hole-length band must be coverable
    pts = np.array([c.yx for c in thinned])
    if len(pts) >= 2:
        dm = np.hypot(pts[:, None, 0] - pts[None, :, 0],
                      pts[:, None, 1] - pts[None, :, 1])
        for L in sorted(set(np.round(hole_lengths, -1))):
            band = (dm > 0.8 * L) & (dm < 1.2 * L)
            if band.any():
                continue
            # back-fill from the rejected list with anything that lands a pair
            for c in rest:
                if any(0.8 * L < np.hypot(c.yx[0] - q.yx[0],
                                          c.yx[1] - q.yx[1]) < 1.2 * L
                       for q in thinned):
                    thinned.append(c)
                    break
    return thinned[:max(n_target + 25, 75)]
