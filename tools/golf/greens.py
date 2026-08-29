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

# Pad-confirmation calibration, MEASURED against 519 OSM golf=green centroids
# on the 22 cached real-course tiles (2026-08-29). The original gate
# (r=16 m disc, 5% slope, 1.4 m relief -- playability.py's constants) passes
# only 28% of real Nebraska greens and 23% of real Carolina ones: a real
# green is a ~20 m flattish kernel inside SHAPED surrounds, not a 32 m flat
# circle. Sweep:
#     r16 5% 1.4m -> 28 / 23 %      r10 6% 1.6m -> 83 / 51 %
#     r10 8% 2.2m -> 95 / 74 %  (Carolina stays lower: crowned Pinehurst
#                                greens carry deliberate falloff)
# NATURAL sites use the middle calibration; the GRADED class uses the loose
# one and carries an earthwork cost -- "buildable with a bulldozer's help",
# the second lever behind making every seed routable.
SITE_R_M = 10.0
NAT_SLOPE_RR = 0.06
NAT_RELIEF_M = 1.6
GRADED_SLOPE_RR = 0.08
GRADED_RELIEF_M = 2.2

# SURROUND relief band (25-90 m annulus, p90-p10 of elevation), measured on
# the 519 real greens 2026-08-29: median 6.8 m (NE) / 6.1 m (NC); real
# greens essentially never sit in dead-flat surrounds (< 2 m happens 3-5% of
# the time, < 1 m almost never). Our pools were skewing dull -- 34-36% of
# candidates below the real p25 (~3.7-4.3 m). A green should be locally
# buildable but CONTEXTUALLY interesting: flat pad, non-flat world around it.
SURROUND_R0_M, SURROUND_R1_M = 25.0, 90.0
SURROUND_LO_M = 1.5      # below: dead flat, heavy penalty ramp
SURROUND_OK_M = 3.5      # real p25; full credit from here
SURROUND_HI_M = 14.0     # soft cap; a wall of dune face stops helping
W_SURROUND = 1.0         # score weight; set 0 to ablate (see overlay3.py A/B)

# FITTED SCORE -- fame-weighted logistic on the 2026-08-29 corpus:
# 429 US courses, 9119 real greens vs 36476 matched same-property controls,
# by-course 80/20 split (corpus/holdout.json). Held-out AUC 0.7928 vs 0.4716
# for the previous hand weights -- which ranked real greens WORSE than random
# ground (room/vis/recept/aroom all carried the wrong sign). Coefficients are
# raw-unit, setting-features only: the 6 construction-signature features
# (subgrid_rough, resid, slope, graded, curvatures) are deliberately EXCLUDED
# -- on a post-construction DEM they detect "already bulldozed" (rough alone
# scores 0.700) and invert under Carolina pine canopy (full model 0.464 there).
# Provenance: tools/golf/corpus/out/fitted/coeffs.json; fit.py BUILD_FEATURES.
# Per the pre-registered archetype test only sandhills_nc earned an override,
# on 3 held-out courses -- recorded as provisional, NOT shipped.
FIT_B0 = -0.1964
# Linear + QUADRATIC terms. The linear-only fit (AUC 0.7928) was right in the
# middle of its range and wrong at the top tail: every term monotone, so the
# score maximized at EXTREMES (the biggest tpi60 knob, the most convex spur)
# while real greens sit on moderate rises -- measured 2026-08-29: our top-9
# candidates hit real greens at 7% within-100 m vs 17% for RANDOM 9 points.
# Quadratics let moderation be expressed (held-out AUC 0.8319); peaks land
# near the real-green p90 (tpi60 optimum +2.5 m, surround ~15 m). Note the
# old hand design had the right SHAPE (trapezoid bands, d6w "floors not
# maximands") with wrong weights; the linear fit had right weights, wrong
# shape. Provenance: corpus/out/fitted/coeffs_quad.json.
FIT_COEF = {
    "aroom_best": -2.9673,
    "aroom_mean": -1.1311,
    "aspect_cos": 0.1579,
    "aspect_sin": 0.1231,
    "backdrop_best": -0.5791,
    "backdrop_far": 0.0661,
    "cls240_concave": -0.3858,
    "cls240_convex": 0.1913,
    "cls240_flat": -0.4549,
    "cls80_concave": -0.5068,
    "cls80_convex": 0.5415,
    "cls80_flat": 0.0673,
    "d_boundary_norm": -2.3707,
    "d_water_band": -0.0031,
    "peak": 0.7923,
    "pit": -1.2377,
    "recept_best": 7.7403,
    "relief_pos": 0.9862,
    "room": -0.1759,
    "saddle": 0.0,
    "surround": 0.0922,
    "tpi200": -0.2507,
    "tpi60": 1.2256,
    "vis_best": -1.6782,
    "vis_mean": -3.6503
}
FIT_COEF_SQ = {
    "aroom_best": 2.196199,
    "aroom_mean": 1.348049,
    "backdrop_best": 1.160808,
    "backdrop_far": -0.001514,
    "d_boundary_norm": 7.052632,
    "d_water_band": 6e-06,
    "peak": -0.376947,
    "pit": 0.739331,
    "recept_best": -8.726523,
    "relief_pos": -0.835471,
    "room": 0.002634,
    "surround": -0.003025,
    "tpi200": 0.011071,
    "tpi60": -0.240789,
    "vis_best": 1.311408,
    "vis_mean": 2.243136
}
# geomorphon 10-class groupings (corpus/features.py)
_CLS_CONVEX = {2, 3, 4, 5}
_CLS_CONCAVE = {7, 8, 9, 10}
W_GRADED = 0.5   # construction-cost term, DESIGN CHOICE not fitted: the fit
                 # measures where greens sit, not what they cost to build


def surround_relief(z8: np.ndarray, cell: float, y: int, x: int) -> float:
    n = int(SURROUND_R1_M / cell)
    if y - n < 0 or x - n < 0 or y + n >= z8.shape[0] or x + n >= z8.shape[1]:
        return 0.0
    yy, xx = np.mgrid[-n:n + 1, -n:n + 1] * cell
    d = np.hypot(yy, xx)
    m = (d >= SURROUND_R0_M) & (d <= SURROUND_R1_M)
    zz = z8[y - n:y + n + 1, x - n:x + n + 1][m]
    return float(np.percentile(zz, 90) - np.percentile(zz, 10))

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
    build: str = "natural"          # "natural" | "graded" (needs earthwork)


# --------------------------------------------------------------------------
# P5/P6 — typed seeding over window + halo


def _seed_typed(f: Fields, m: Morphology, p: Persistence,
                i0: int, j0: int, h: int, w: int) -> list[tuple[int, int, str]]:
    """(i, j, type) seeds inside the window+halo, gated hard only where
    physically necessary."""
    halo = int(round(120.0 / f.cell))
    a0, a1 = max(0, i0 - halo), min(f.z8.shape[0], i0 + h + halo)
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
    r = int(round(24.0 / cell2))
    y, x = int(round(y_m / cell2)), int(round(x_m / cell2))
    if y - r < 0 or x - r < 0 or y + r >= z2.shape[0] or x + r >= z2.shape[1]:
        return False, (0.0, 0.0), 0.0, "none"
    zc = z2[y - r:y + r, x - r:x + r]
    wc = wet2[y - r:y + r, x - r:x + r]
    n = int(round(SITE_R_M / cell2))
    yy, xx = np.mgrid[-r:r, -r:r]
    disc = (yy * yy + xx * xx) <= n * n
    if wc[disc].any():
        return False, (0.0, 0.0), 0.0, "none"
    zd = zc[disc]
    relief = zd.max() - zd.min()
    A = np.stack([yy[disc] * cell2, xx[disc] * cell2,
                  np.ones(disc.sum())], 1)
    coef, *_ = np.linalg.lstsq(A, zd, rcond=None)
    gy, gx = float(coef[0]), float(coef[1])
    grade = float(np.hypot(gy, gx))
    resid = float(np.sqrt(np.mean((zd - A @ coef) ** 2)))
    if relief <= NAT_RELIEF_M and grade <= NAT_SLOPE_RR and resid <= 0.45:
        return True, (gy, gx), resid, "natural"
    if relief <= GRADED_RELIEF_M and grade <= GRADED_SLOPE_RR and resid <= 0.65:
        # buildable with bounded cut/fill; the earthwork is roughly the
        # residual volume over the pad
        return True, (gy, gx), resid, "graded"
    return False, (gy, gx), resid, "none"


# --------------------------------------------------------------------------
# P8 — approach vectors


def _backdrop_far(f: Fields, y: int, x: int) -> float:
    """Best mean rise 80-160 m beyond the green over all bearings (fitted
    term; corpus/features.py is the reference implementation)."""
    bs = np.arange(1, int(160.0 / f.cell) + 1)
    zg = f.z8[y, x]
    best = -1e9
    for k in range(N_BEARINGS):
        th = 2.0 * np.pi * k / N_BEARINGS
        by = np.clip((y - np.sin(th) * bs).astype(int), 0, f.z8.shape[0] - 1)
        bx = np.clip((x - np.cos(th) * bs).astype(int), 0, f.z8.shape[1] - 1)
        mm = bs * f.cell >= 80.0
        best = max(best, float((f.z8[by, bx] - zg)[mm].mean()))
    return best


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
    h = int(round(sit.window_m[2] / f.cell))
    w = int(round(sit.window_m[3] / f.cell))

    seeds = _seed_typed(f, m, p, i0, j0, h, w)
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

    from scipy import ndimage as _ndi
    tpi60 = f.z8 - _ndi.uniform_filter(f.z8, int(round(60.0 / f.cell)) | 1)
    win_y0, win_x0 = sit.window_m[0], sit.window_m[1]
    win_h_m, win_w_m = sit.window_m[2], sit.window_m[3]
    win_sqrt_area = float(np.sqrt(max(win_h_m * win_w_m, 1.0)))

    cands: list[Candidate] = []
    for (y, x), (s0, kind) in ranked:
        y_m, x_m = y * f.cell, x * f.cell
        ok, grad, resid, build = confirm_2m(z2, cell2, wet2, y_m, x_m)
        if not ok:
            continue
        tab = approach_table(f, y, x, grad)
        gy, gx = grad
        gmag = float(np.hypot(gy, gx))
        c240 = int(m.cls240[y, x])
        c80 = int(m.cls80[y, x])
        # window-edge distance stands in for the fitted course-boundary term
        d_edge = min(y_m - win_y0, win_y0 + win_h_m - y_m,
                     x_m - win_x0, win_x0 + win_w_m - x_m)
        feats = {
            "vis_best": float(tab[:, 0].max()),
            "vis_mean": float(tab[:, 0].mean()),
            "recept_best": float(np.clip(tab[:, 1], -0.05, 0.05).max() / 0.05),
            "backdrop_best": float(np.clip(tab[:, 2], 0, 8).max() / 8.0),
            "backdrop_far": float(np.clip(_backdrop_far(f, y, x), -10, 10)),
            "aroom_best": float(tab[:, 3].max()),
            "aroom_mean": float(tab[:, 3].mean()),
            "tpi60": float(tpi60[y, x]),
            "tpi200": float(f.tpi200[y, x]),
            "relief_pos": float(f.relief_pos[y, x]),
            "surround": surround_relief(f.z8, f.cell, y, x),
            "room": float(min(f.room[y, x], 60.0)),
            "pit": float(np.log1p(min(p.pit[y, x], 10.0))),
            "peak": float(np.log1p(min(p.peak[y, x], 20.0))),
            "saddle": float(m.saddle[y, x]),
            "cls240_flat": float(c240 == 1),
            "cls240_convex": float(c240 in _CLS_CONVEX),
            "cls240_concave": float(c240 in _CLS_CONCAVE),
            "cls80_flat": float(c80 == 1),
            "cls80_convex": float(c80 in _CLS_CONVEX),
            "cls80_concave": float(c80 in _CLS_CONCAVE),
            "aspect_sin": float(gy / gmag) if gmag > 1e-6 else 0.0,
            "aspect_cos": float(gx / gmag) if gmag > 1e-6 else 0.0,
            "d_water_band": float(min(f.d_water[y, x], 400.0)),
            "d_boundary_norm": float(d_edge / win_sqrt_area),
        }
        score = (FIT_B0 + sum(FIT_COEF[k] * v for k, v in feats.items())
                 + sum(FIT_COEF_SQ[k] * feats[k] * feats[k]
                       for k in FIT_COEF_SQ)
                 - (W_GRADED if build == "graded" else 0.0))
        cands.append(Candidate((y_m, x_m), kind, float(score), grad,
                               float(max(p.pit[y, x], p.peak[y, x])), tab,
                               build=build))

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
            ok, grad, resid, build = confirm_2m(z2, cell2, wet2, gy, gx)
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
