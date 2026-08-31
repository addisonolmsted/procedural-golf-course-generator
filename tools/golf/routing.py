"""S6 routing prototype: shot-based tee -> landing zones -> green chains.

THE SHOT MODEL (owner's design, 2026-08-30): only tees, landing zones and
greens need golf-ball-amicable ground (~8% grade); the terrain BETWEEN shots
may be wild -- carries over gullies, dunes, water. Par 3 = tee->green; par 4
= tee->LZ->green; par 5 = tee->LZ1->LZ2->green, the second LZ mattering on
true three-shot holes. `playability.corridors` is therefore NOT a hole gate
here; the corridor idea survives only as walk sanity.

Stage-06 doctrine (docs/stages/stage-06-routing.md): greens-first, SATURATING
penalties -- a bad route is expensive, never impossible; the only prunes are
arithmetic impossibility (par mixes) and geometry (cannot reach home). Fixed
beam width x depth; total-order deterministic tie-breaks; no RNG anywhere.

PORT NOTES (do not act here): (a) contracts' PAR_SEQUENCE becomes router
OUTPUT -- C2/C3 change at port time; (b) C3's anticipated per-hole carry span
list is exactly our `bridges`; (c) five tee boxes vs C3's single tee NodeId
is stage-06 open question 4, resolved here as "S6 places all five" because
tee position determines hole length, which the par search needs.

Weights are hand-set and PROVISIONAL until the render-review round -- the
same status siting weights had before d6w calibration.
"""

from __future__ import annotations

from dataclasses import dataclass, field

import numpy as np
from scipy import ndimage

from siting import (CLUBHOUSE_RADIUS_M, Fields, Siting, trapezoid)

# --- pars, mixes, bands ----------------------------------------------------
# Allowed par-36 mixes as (n_par3, n_par4, n_par5), owner-specified.
ALLOWED_MIXES = ((2, 5, 2), (3, 3, 3), (1, 7, 1))
# Hole-length bands in metres from the BACK tee. 120-230 / 300-470 / 480-610
# yards converted and trimmed to a walkable nine; the pool's distance-
# spectrum repair (greens.py) guarantees candidate pairs inside every band.
PAR_BANDS = {3: (110.0, 210.0), 4: (280.0, 430.0), 5: (440.0, 560.0)}
TOTAL_BAND_M = (2600.0, 3200.0)

# Measured 2026-08-30 on 4,748 real green->next-tee walks (302 courses):
# length p25/50/75/90 = 49/67/97/142 m. The old free threshold (150 m) sat
# at the real p90+, and our routed medians drifted there. BACKTRACKING is
# rare in reality: the walk projected against the prior hole's closing
# direction is p50 0 m / p75 7 / p90 51, and only 13% of walks backtrack
# more than 40 m -- walks continue FORWARD (median angle 59 deg off the
# prior line of play).
WALK_FREE_M = 70.0       # real p50: costs ~nothing up to here
WALK_SAT_M = 200.0       # saturation point of the length penalty
WALK_MAX_M = 300.0       # beyond this the tee simply isn't placed (geometric)
BACKTRACK_FREE_M = 10.0
BACKTRACK_SAT_M = 60.0   # real p90 is 51 m
DRIVE_R_M = (190.0, 250.0)     # tee -> LZ1 annulus
SECOND_R_M = (160.0, 220.0)    # LZ1 -> LZ2 annulus (par 5)
# Real max-dogleg (9,248 hole lines): par4/5 p50 18-20 deg, p90 44-46, and
# 57-60% of holes bend >15 deg; the bend sits at t=0.63 (the LZ). Par 3s are
# dead straight (sinuosity p50 1.000). 45 deg = the real p90.
DOGLEG_MAX_RAD = np.radians(45.0)
LZ_R_M = 22.0            # target landing-zone radius
LZ_ROOM_MIN_M = 15.0     # tight-but-playable floor (penalized, never vetoed)
# Measured 2026-08-30 (196 corpus holes with >=2 mapped golf=tee): spread
# along the play axis is roughly CONSTANT across par classes -- p25/50/75 =
# 52/77/105 (par3), 48/69/93 (par4), 63/78/102 (par5). 75 m sits between the
# medians and p75; the old 57 m was short of every class median.
TEE_STAGGER_M = (0.0, 18.0, 36.0, 54.0, 75.0)
TEE_SIZE_M = 7.0

BEAM_W = 48
EXPAND_TOP = 20
N_FINAL = 8


# --- data model ------------------------------------------------------------

@dataclass
class TeeBox:
    yx: tuple[float, float]
    size_m: float
    axis: tuple[float, float]        # unit (dy, dx) toward the first target
    length_m: float                  # this box -> green along the spine
    graded: bool


@dataclass
class Bridge:
    a_yx: tuple[float, float]
    b_yx: tuple[float, float]
    span_m: float
    kind: str                        # "walk" | "spine"
    hole: int


@dataclass
class Walk:
    path: np.ndarray                 # (2,2) [[y0,x0],[y1,x1]]
    length_m: float
    grade_mean: float
    bridges: list[Bridge] = field(default_factory=list)


@dataclass
class Hole:
    index: int
    par: int
    green_idx: int
    green_yx: tuple[float, float]
    tee_boxes: list[TeeBox]
    lzs: list[tuple[float, float, float]]     # (y, x, r_m)
    spine: np.ndarray                # (k,2) back tee -> LZs -> green
    length_m: float
    approach_bin: int
    bridges: list[Bridge]
    walk_from_prev: Walk
    terms: dict[str, float]


@dataclass
class Route:
    holes: list[Hole]
    par_sequence: list[int]
    total_length_m: float
    total_walk_m: float
    clubhouse_yx: tuple[float, float]
    score: float
    terms: dict[str, float]
    crossings: list[tuple[int, int, tuple[float, float]]]


@dataclass
class RouteFields:
    lz_room: np.ndarray              # m of contiguous <=8%-and-dry ground
    tee_ok: np.ndarray
    tee_relaxed: np.ndarray


def build_route_fields(f: Fields) -> RouteFields:
    lz_room = ndimage.distance_transform_edt(f.pad_fair, sampling=f.cell)
    tee_relaxed = (f.slope <= 0.10) & ~f.wet8
    return RouteFields(lz_room, f.pad_tee, tee_relaxed)


# --- geometry kernels ------------------------------------------------------

def seg_intersect(p, q, r, s) -> bool:
    """Proper intersection of segments pq and rs (2D orientation signs)."""
    def o(a, b, c):
        v = (b[1] - a[1]) * (c[0] - a[0]) - (b[0] - a[0]) * (c[1] - a[1])
        return 0 if abs(v) < 1e-9 else (1 if v > 0 else -1)
    o1, o2 = o(p, q, r), o(p, q, s)
    o3, o4 = o(r, s, p), o(r, s, q)
    return o1 != o2 and o3 != o4 and 0 not in (o1, o2, o3, o4)


def _seg_point(p, q, r, s):
    """Intersection point of the (assumed crossing) lines pq, rs."""
    d1y, d1x = q[0] - p[0], q[1] - p[1]
    d2y, d2x = s[0] - r[0], s[1] - r[1]
    den = d1x * d2y - d1y * d2x
    if abs(den) < 1e-12:
        return ((p[0] + q[0]) / 2, (p[1] + q[1]) / 2)
    t = ((r[1] - p[1]) * d2y - (r[0] - p[0]) * d2x) / den
    return (p[0] + t * d1y, p[1] + t * d1x)


def _trim(poly: np.ndarray, trim_m: float) -> np.ndarray:
    """Shrink a polyline's first and last segment by trim_m at the ends so
    legitimately-shared endpoints (walk meets spine at the green) don't
    count as crossings."""
    p = poly.astype(float).copy()
    if len(p) < 2:
        return p
    for (i, j) in ((0, 1), (len(p) - 1, len(p) - 2)):
        v = p[j] - p[i]
        n = np.hypot(*v)
        if n > 2 * trim_m:
            p[i] = p[i] + v / n * trim_m
    return p


def polyline_crossings(A: np.ndarray, B: np.ndarray,
                       trim_m: float = 8.0) -> list[tuple[float, float]]:
    a, b = _trim(A, trim_m), _trim(B, trim_m)
    out = []
    for i in range(len(a) - 1):
        for j in range(len(b) - 1):
            if seg_intersect(a[i], a[i + 1], b[j], b[j + 1]):
                out.append(_seg_point(a[i], a[i + 1], b[j], b[j + 1]))
    return out


def wet_spans(wet2: np.ndarray, cell2: float, a_yx, b_yx, kind: str,
              hole: int, step_m: float = 2.0) -> list[Bridge]:
    """Contiguous wet runs along segment a->b, as Bridge records."""
    a = np.asarray(a_yx, float)
    b = np.asarray(b_yx, float)
    L = float(np.hypot(*(b - a)))
    if L < step_m:
        return []
    n = int(L / step_m) + 1
    ts = np.linspace(0.0, 1.0, n)
    pts = a[None, :] + ts[:, None] * (b - a)[None, :]
    yi = np.clip((pts[:, 0] / cell2).astype(int), 0, wet2.shape[0] - 1)
    xi = np.clip((pts[:, 1] / cell2).astype(int), 0, wet2.shape[1] - 1)
    w = wet2[yi, xi]
    out = []
    i = 0
    while i < n:
        if w[i]:
            j = i
            while j + 1 < n and w[j + 1]:
                j += 1
            span = (j - i + 1) * step_m
            out.append(Bridge(tuple(pts[max(i - 1, 0)]),
                              tuple(pts[min(j + 1, n - 1)]),
                              float(span), kind, hole))
            i = j + 1
        else:
            i += 1
    return out


# --- par-mix feasibility ---------------------------------------------------

def mix_feasible(counts: tuple[int, int, int], holes_done: int) -> bool:
    """Can `counts` (n3, n4, n5) still complete one of ALLOWED_MIXES with
    9 - holes_done holes remaining?"""
    left = 9 - holes_done
    for m in ALLOWED_MIXES:
        need = (m[0] - counts[0], m[1] - counts[1], m[2] - counts[2])
        if min(need) >= 0 and sum(need) == left:
            return True
    return False


def legal_pars(counts: tuple[int, int, int], holes_done: int) -> list[int]:
    out = []
    for p, k in ((3, 0), (4, 1), (5, 2)):
        c = list(counts)
        c[k] += 1
        if mix_feasible(tuple(c), holes_done + 1):
            out.append(p)
    return out


# --- approach scoring ------------------------------------------------------

def approach_bin(v_in: np.ndarray) -> int:
    """Bearing bin for an incoming play direction v_in (unit, (dy,dx)).
    approach_table's bin k has the golfer standing along +u_k playing IN
    along -u_k, so the stance side faces -v_in."""
    th = np.arctan2(-v_in[0], -v_in[1]) % (2 * np.pi)
    return int(round(th / (2 * np.pi / 16))) % 16


def score_approach(tab: np.ndarray, k: int) -> float:
    """Per-bin shot quality in ~[-0.3, 1]. Hand weights, deliberately NOT
    FIT_COEF: the fitted coefficients answer "is this a green site", pooled
    over all bearings with quadratic moderation -- not a per-bin shot
    function. Ordering intuition kept: visibility and receptivity dominate."""
    vis = float(tab[k, 0])
    rec = float(np.clip(tab[k, 1], -0.05, 0.05) / 0.05)
    back = float(np.clip(tab[k, 2], 0.0, 4.0) / 4.0)
    room = float(np.clip(tab[k, 3], 0.0, 1.0))
    return 0.50 * vis + 0.30 * rec + 0.10 * back + 0.10 * room


# --- landing zones ---------------------------------------------------------

def _gather(rf: RouteFields, f: Fields, ys: np.ndarray, xs: np.ndarray):
    yi = np.clip((ys / f.cell).astype(int), 0, rf.lz_room.shape[0] - 1)
    xi = np.clip((xs / f.cell).astype(int), 0, rf.lz_room.shape[1] - 1)
    return rf.lz_room[yi, xi], f.slope[yi, xi], f.d_water[yi, xi]


def lz_probe(rf: RouteFields, f: Fields, tee_yx, green_yx, par: int) -> float:
    """Beam-time LZ existence probe: best room quality over a coarse grid of
    plausible first-landing points. Cheap (9 gathers)."""
    if par == 3:
        return 1.0
    a = np.asarray(tee_yx, float)
    b = np.asarray(green_yx, float)
    v = b - a
    L = np.hypot(*v)
    if L < 1e-6:
        return 0.0
    u = v / L
    n = np.array([-u[1], u[0]])
    best = 0.0
    for r in (DRIVE_R_M[0], np.mean(DRIVE_R_M), DRIVE_R_M[1]):
        for off in (-30.0, 0.0, 30.0):
            p = a + u * min(r, 0.75 * L) + n * off
            room, _, _ = _gather(rf, f, np.array([p[0]]), np.array([p[1]]))
            best = max(best, float(np.clip(room[0] / LZ_R_M, 0, 1)))
    return best


def place_lz(rf: RouteFields, f: Fields, from_yx, green_yx,
             r_band: tuple[float, float], remainder_band: tuple[float, float],
             avoid_spines=(), avoid_walks=(), avoid_lzs=(), avoid_greens=(),
             ch_keepout_yx=None) -> tuple[tuple[float, float, float], float] | None:
    """Best landing zone on an annulus around from_yx, bearing within
    DOGLEG_MAX of the direct line to the green. Returns ((y,x,r), score)."""
    a = np.asarray(from_yx, float)
    g = np.asarray(green_yx, float)
    v = g - a
    D = np.hypot(*v)
    if D < 1e-6:
        return None
    th0 = np.arctan2(v[0], v[1])
    radii = np.linspace(r_band[0], r_band[1], 5)
    bears = th0 + np.radians(np.arange(-45, 46, 4))
    R, TH = np.meshgrid(radii, bears)
    ys = a[0] + R * np.sin(TH)
    xs = a[1] + R * np.cos(TH)
    room, slope, dwat = _gather(rf, f, ys.ravel(), xs.ravel())
    room_q = np.clip(room / LZ_R_M, 0, 1)
    ok = room >= LZ_ROOM_MIN_M
    rem = np.hypot(g[0] - ys.ravel(), g[1] - xs.ravel())
    rem_t = trapezoid(rem, remainder_band[0], remainder_band[1])
    dog = np.abs((TH - th0).ravel())
    # free to 15 deg, saturating at the real p90 of 45. First cut freed to
    # 20 deg at weight 0.3 and our routed median came out 32 deg vs the real
    # p50 of 18-20 -- the LZ room/flat rewards outbid a penalty that mild,
    # so the ramp starts at the real "most holes bend this little" point
    # and the weight doubles.
    dog_pen = np.clip((np.degrees(dog) - 15.0) / 30.0, 0, 1)
    score = (0.5 * room_q + 0.3 * (1 - np.clip(slope / 0.08, 0, 1))
             + 0.1 * trapezoid(dwat, 20.0, 80.0)
             + 0.4 * rem_t - 0.6 * dog_pen)
    score = np.where(ok, score, score - 1.0)     # tight LZ: penalized, never vetoed
    order = np.lexsort((np.arange(score.size), -score))
    yr, xr = ys.ravel(), xs.ravel()
    # tiered like place_tee: walk-crossing freedom is the hard rule, play
    # crossing freedom the strong preference, dirty last resort
    pick = int(order[0])
    pick_walk_clean = None
    pick_play_clean = None
    for k in order:
        k = int(k)
        legs = np.array([[a[0], a[1]], [yr[k], xr[k]], [g[0], g[1]]])
        if any(polyline_crossings(legs, wk) for wk in avoid_walks):
            continue
        if pick_walk_clean is None:
            pick_walk_clean = k
        if any(polyline_crossings(legs, sp) for sp in avoid_spines):
            continue
        if ch_keepout_yx is not None and \
                ch_intrusion(legs, ch_keepout_yx) > 0.0:
            continue
        if pick_play_clean is None:
            pick_play_clean = k
        # SHARED LZ AVOIDANCE (owner, 2026-08-30): landing zones of
        # different holes stay LZ_SEP_M apart -- the full-clean tier also
        # requires separation from every already-placed LZ
        if not all(np.hypot(yr[k] - ly, xr[k] - lx) >= LZ_SEP_M
                   for (ly, lx) in avoid_lzs):
            continue
        # and never inside another hole's green (measured: real pairs are
        # under 80 m only 0.95% of the time)
        if all(np.hypot(yr[k] - gy2, xr[k] - gx2) >= GREEN_LZ_SEP_M
               for (gy2, gx2) in avoid_greens):
            pick = k
            break
    else:
        pick = (pick_play_clean if pick_play_clean is not None
                else pick_walk_clean if pick_walk_clean is not None
                else pick)
    y, x = float(yr[pick]), float(xr[pick])
    legs = np.array([[a[0], a[1]], [y, x], [g[0], g[1]]])
    clean = not any(polyline_crossings(legs, sp) for sp in avoid_spines) \
        and not any(polyline_crossings(legs, wk) for wk in avoid_walks)
    return ((y, x, float(min(room[pick], LZ_R_M))), float(score[pick]), clean)


# --- tees ------------------------------------------------------------------

def place_tee(rf: RouteFields, f: Fields, prev_yx, green_yx, par: int,
              clubhouse_yx=None, avoid_spines=(), avoid_walks=(),
              n_options: int = 6, hi_cap: float | None = None,
              ch_keepout_yx=None) -> list | None:
    """Best back-tee cell: inside the walk disc of prev_yx (or the clubhouse
    disc for hole 1), hole length in band, tee-grade ground preferred."""
    lo, hi = PAR_BANDS[par]
    if hi_cap is not None:
        hi = max(lo + 20.0, min(hi, hi_cap))
    centre = np.asarray(clubhouse_yx if clubhouse_yx is not None else prev_yx,
                        float)
    # the hole-1 disc keeps 14 m of margin: the box slide (+-10 m lateral)
    # and the 8 m cell quantization must not push the back box past 120 m
    r_m = (CLUBHOUSE_RADIUS_M - 14.0) if clubhouse_yx is not None else WALK_MAX_M
    ci, cj = int(centre[0] / f.cell), int(centre[1] / f.cell)
    r = int(r_m / f.cell) + 1
    i0, i1 = max(0, ci - r), min(f.z8.shape[0], ci + r + 1)
    j0, j1 = max(0, cj - r), min(f.z8.shape[1], cj + r + 1)
    yy, xx = np.mgrid[i0:i1, j0:j1]
    ym, xm = yy * f.cell, xx * f.cell
    d_prev = np.hypot(ym - centre[0], xm - centre[1])
    d_green = np.hypot(ym - np.asarray(green_yx)[0], xm - np.asarray(green_yx)[1])
    in_disc = d_prev <= r_m
    in_band = (d_green > lo) & (d_green < hi)
    for mask, graded in ((rf.tee_ok[i0:i1, j0:j1], False),
                         (rf.tee_relaxed[i0:i1, j0:j1], True)):
        cand = mask & in_disc & in_band
        if not cand.any():
            continue
        walk_t = 1.0 - np.clip((d_prev - WALK_FREE_M) / (WALK_SAT_M - WALK_FREE_M),
                               0, 1)
        band_t = trapezoid(d_green, lo + 0.15 * (hi - lo), hi - 0.15 * (hi - lo),
                           tail=0.05)
        padq = 1.0 - np.clip(f.slope[i0:i1, j0:j1] / 0.065, 0, 1)
        sc = np.where(cand, 0.9 * walk_t + 1.1 * band_t + 0.5 * padq, -1e9)
        ys, xs = np.where(cand)
        order = np.lexsort((xs, ys, -sc[ys, xs]))
        # CROSSING AVOIDANCE (2026-08-30): the beam scores crossings on
        # ESTIMATED tees; detail placement moves them, so the final walk or
        # spine can cross a hole the beam thought was clear (measured: 10
        # walk-x-play crossings on the first gentle-tile test, then 1 that
        # survived every LZ option -- green 3 sat exactly ON the hole-6
        # tee->green line, so walk 4 fanned across the whole approach cone
        # and only a DIFFERENT TEE could fix it). Returns up to n_options
        # clean candidates so the caller can retry tee+LZ jointly; the last
        # entry is the best-scored fallback (possibly dirty, saturating).
        # TIERED cleanliness (2026-08-30). Walk-crossing freedom is the
        # owner's hard rule; play x play freedom is a strong preference
        # (~1% of courses). Demanding both at once starved the option list
        # and the fallback then violated the HARD rule -- measured: adding
        # the play x play test naively took the gentle tile from 0 to 5
        # walk crossings. Tier 1: fully clean. Tier 2: walk-clean only.
        # Tier 3 (last resort): best-scored, possibly dirty.
        tier0, tier1, tier2 = [], [], []
        fallback = None
        for q in order[:200]:
            y, x = int(ys[q]), int(xs[q])
            cell_yx = np.array([(i0 + y) * f.cell, (j0 + x) * f.cell])
            if fallback is None:
                fallback = ((float(cell_yx[0]), float(cell_yx[1])),
                            float(sc[y, x]), graded)
            walk_seg = np.array([np.asarray(prev_yx, float), cell_yx])
            spine_seg = np.array([cell_yx, np.asarray(green_yx, float)])
            walk_bad = any(polyline_crossings(walk_seg, sp)
                           for sp in avoid_spines)
            walk_bad = walk_bad or any(polyline_crossings(spine_seg, wk)
                                       for wk in avoid_walks)
            if walk_bad:
                continue
            play_bad = any(polyline_crossings(spine_seg, sp)
                           for sp in avoid_spines)
            entry = ((float(cell_yx[0]), float(cell_yx[1])),
                     float(sc[y, x]), graded)
            if play_bad:
                if len(tier2) < n_options:
                    tier2.append(entry)
                continue
            # clearance tier: keep >= ~2/3 of the measured radius profile.
            # A THRESHOLD inside the tier order, not a primary sort -- the
            # first attempt sorted options by clearance and starved hole 8
            # into a walk-crossing fallback (score 2.37, worst_clear 0.97).
            viol = max((clearance_violation(spine_seg, sp)
                        for sp in avoid_spines), default=0.0)
            if ch_keepout_yx is not None:
                viol = max(viol, ch_intrusion(spine_seg, ch_keepout_yx))
            if viol <= 0.55:
                tier0.append(entry)
                if len(tier0) >= n_options:
                    break
            elif len(tier1) < n_options:
                tier1.append(entry)
        opts = tier0 if tier0 else (tier1 if tier1 else tier2)
        if opts or fallback:
            return opts if opts else [fallback]
    return None


def tee_boxes(rf: RouteFields, f: Fields, back_yx, target_yx,
              green_yx) -> list[TeeBox]:
    """Five boxes staggered along the play axis; forward boxes shorten the
    hole. Lateral slide +-10 m by deterministic min-slope scan; the forward
    three may sit on relaxed (graded) ground."""
    a = np.asarray(back_yx, float)
    t = np.asarray(target_yx, float)
    u = t - a
    u = u / max(np.hypot(*u), 1e-9)
    n = np.array([-u[1], u[0]])
    g = np.asarray(green_yx, float)
    out = []
    for bi, off in enumerate(TEE_STAGGER_M):
        base = a + u * off
        best = None
        for lat in np.arange(-10.0, 10.1, 2.0):
            p = base + n * lat
            yi = int(np.clip(p[0] / f.cell, 0, f.z8.shape[0] - 1))
            xi = int(np.clip(p[1] / f.cell, 0, f.z8.shape[1] - 1))
            okm = rf.tee_ok if bi < 2 else rf.tee_relaxed
            if not okm[yi, xi]:
                continue
            key = (float(f.slope[yi, xi]), abs(lat), lat)
            if best is None or key < best[0]:
                best = (key, p, not rf.tee_ok[yi, xi])
        if best is None:
            best = ((0.0, 0.0, 0.0), base, True)
        _, p, graded = best
        out.append(TeeBox((float(p[0]), float(p[1])), TEE_SIZE_M,
                          (float(u[0]), float(u[1])),
                          float(np.hypot(*(g - p))), graded))
    return out


# --- beam search over (green sequence, par assignment) ---------------------

def _pool_arrays(pool):
    yx = np.array([c.yx for c in pool], float)
    score = np.array([c.score for c in pool], float)
    # percentile rank within THIS seed's pool: fit logits are uncalibrated
    order = np.argsort(score, kind="stable")
    pct = np.empty(len(pool))
    pct[order] = np.arange(len(pool)) / max(len(pool) - 1, 1)
    return yx, pct


def beam_route(f: Fields, rf: RouteFields, sit: Siting, pool,
               wet2: np.ndarray, cell2: float) -> Route | None:
    """The router. Beam over green sequences with router-chosen par."""
    if not pool:
        return None
    yx, pct = _pool_arrays(pool)
    home = np.asarray(sit.clubhouse.yx, float)
    n = len(pool)

    # state: (score, used frozenset, (n3,n4,n5), pos, seq tuple, segs tuple)
    # seq entries: (green_idx, par, est_tee_yx)
    # state gains a running length so the beam feels PACE, not just bands:
    # without it the clearance term buys spacing with length and totals ran
    # 3.5-3.6 km (band-top holes all the way); target pace ~322 m/hole
    # state also carries approximate LZ points (par 4/5) so a GREEN is never
    # chosen on top of an earlier hole's landing zone -- greens come from the
    # beam and detail placement cannot move them.
    states = [(0.0, frozenset(), (0, 0, 0), home, (), (), 0.0, 0.0, None, ())]
    for h in range(9):
        nxt = []
        for (sc, used, counts, pos, seq, segs, cum, cum_mid, pdir,
             lzs_ap) in states:
            pars = legal_pars(counts, h)
            D = np.hypot(yx[:, 0] - pos[0], yx[:, 1] - pos[1])
            d_home = np.hypot(yx[:, 0] - home[0], yx[:, 1] - home[1])
            for par in pars:
                lo, hi = PAR_BANDS[par]
                walk_est = np.maximum(0.0, np.maximum(lo - D, D - hi))
                ok = walk_est <= WALK_MAX_M
                # home lookahead: remaining holes can still bring us back
                left = 8 - h
                reach = sum(sorted(PAR_BANDS[p][1] for p in (3, 4, 5))[-1:]
                            ) * left + left * WALK_MAX_M
                ok &= d_home <= reach + hi + WALK_MAX_M
                if h == 8:
                    ok &= d_home <= CLUBHOUSE_RADIUS_M
                for gi in np.flatnonzero(ok):
                    if gi in used:
                        continue
                    ok[gi] = True
                idxs = np.flatnonzero(ok)
                idxs = np.array([i for i in idxs if i not in used], int)
                if not len(idxs):
                    continue
                # est tee: pull back from prev pos toward green if needed
                v = yx[idxs] - pos[None, :]
                dd = np.maximum(np.hypot(v[:, 0], v[:, 1]), 1e-9)
                tee_est = np.where(
                    (walk_est[idxs] > 0)[:, None],
                    pos[None, :] + v / dd[:, None]
                    * np.where(D[idxs] < lo, D[idxs] - lo,
                               np.clip(D[idxs] - hi, 0, None))[:, None],
                    pos[None, :])
                hole_len = np.hypot(yx[idxs][:, 0] - tee_est[:, 0],
                                    yx[idxs][:, 1] - tee_est[:, 1])
                lo_i = lo + 0.15 * (hi - lo)
                hi_i = hi - 0.15 * (hi - lo)
                mid_par = (PAR_BANDS[par][0] + PAR_BANDS[par][1]) / 2.0
                pace = np.abs((cum + hole_len) - (cum_mid + mid_par))
                cheap = (1.5 * pct[idxs]
                         + 0.9 * trapezoid(hole_len, lo_i, hi_i, tail=0.1)
                         - 1.0 * np.clip(pace / 450.0, 0, 1)
                         - 1.0 * np.clip((walk_est[idxs] - WALK_FREE_M)
                                         / (WALK_SAT_M - WALK_FREE_M), 0, 1)
                         - 0.3 * np.clip(walk_est[idxs] / WALK_MAX_M, 0, 1))
                # back-to-back 3s/5s
                if seq and par in (3, 5) and seq[-1][1] == par:
                    cheap = cheap - 0.6
                # MIX PRIOR, beam side (measured 2026-08-30): among 489 real
                # ref+par-tagged nines, par-36 mixes split (2,5,2) 94.2% /
                # (1,7,1) 3.1% / (3,3,3) 2.7%; our 1000-seed batch ran
                # (3,3,3) 75.4% -- par 3s dodge LZ/clearance costs and par 5s
                # buy length room, so the beam drifted to the extremes. Real
                # nines are 5/9 par-4s; nudge each par-4 pick.
                if par == 4:
                    cheap = cheap + 0.22
                top = idxs[np.argsort(-cheap, kind="stable")[:EXPAND_TOP]]
                for gi in top:
                    gi = int(gi)
                    tee = tee_est[list(idxs).index(gi)]
                    # approach quality from the estimated last-shot direction
                    vin = yx[gi] - (tee if par == 3 else
                                    tee + 0.6 * (yx[gi] - tee))
                    nv = np.hypot(*vin)
                    ab = approach_bin(vin / nv) if nv > 1e-9 else 0
                    appr = score_approach(pool[gi].approach, ab)
                    lzq = lz_probe(rf, f, tee, yx[gi], par)
                    s_hole = float(cheap[list(idxs).index(gi)]
                                   + 1.0 * appr + 0.8 * lzq)
                    # crossings vs accumulated straight segments
                    pen = 0.0
                    new_spine = (tuple(tee), tuple(yx[gi]), "spine")
                    new_walk = (tuple(pos), tuple(tee), "walk")
                    ns_arr = np.array([new_spine[0], new_spine[1]])
                    n_sp = sum(1 for sg in segs if sg[2] == "spine")
                    sp_i = 0
                    for (a2, b2, kind2) in segs:
                        A = np.array([a2, b2])
                        if kind2 == "spine" and polyline_crossings(
                                np.array([new_walk[0], new_walk[1]]), A):
                            pen -= 4.0
                        if kind2 == "spine":
                            if polyline_crossings(ns_arr, A):
                                pen -= 6.0
                            # LINE-OF-PLAY CLEARANCE in the beam (2026-08-30):
                            # detail placement alone could not fix bunching --
                            # every final of a bunched SEQUENCE bunches, so the
                            # sequence itself must pay early. Straight-segment
                            # approximation of the measured radii; the
                            # consecutive junction (prev green = this walk's
                            # origin) is exempt as in the exact term.
                            sp_i += 1
                            v = clearance_violation(
                                ns_arr, A, consecutive=(sp_i == n_sp))
                            pen -= 2.0 * v * v
                        if kind2 == "walk" and polyline_crossings(
                                np.array([new_spine[0], new_spine[1]]), A):
                            pen -= 4.0
                    pen -= 2.0 * ch_intrusion(ns_arr, home) ** 2
                    # this green vs earlier holes' approximate LZs
                    gp = yx[gi]
                    for (ly, lx) in lzs_ap:
                        d_gl = np.hypot(gp[0] - ly, gp[1] - lx)
                        if d_gl < GREEN_LZ_SEP_M:
                            pen -= 5.0 * (1.0 - d_gl / GREEN_LZ_SEP_M)
                    # this hole's approximate LZ vs earlier greens
                    new_lz = ()
                    if par >= 4:
                        vv = gp - np.asarray(tee)
                        nv2 = np.hypot(*vv)
                        if nv2 > 1e-9:
                            q = (np.asarray(tee)
                                 + vv / nv2 * min(LZ_APPROX_M, 0.6 * nv2))
                            new_lz = ((float(q[0]), float(q[1])),)
                            for (_g2, _p2, _e2), gidx in zip(seq,
                                                             [e[0] for e in seq]):
                                gy2, gx2 = yx[gidx]
                                d_lg = np.hypot(q[0] - gy2, q[1] - gx2)
                                if d_lg < GREEN_LZ_SEP_M:
                                    pen -= 5.0 * (1.0 - d_lg / GREEN_LZ_SEP_M)
                    if pdir is not None:
                        wv = np.asarray(tee) - pos
                        bt = max(0.0, -float(np.dot(pdir, wv)))
                        pen -= 0.7 * min(1.0, max(0.0, (bt - BACKTRACK_FREE_M)
                                                  / (BACKTRACK_SAT_M
                                                     - BACKTRACK_FREE_M)))
                    c2 = list(counts)
                    c2[{3: 0, 4: 1, 5: 2}[par]] += 1
                    nxt.append((sc + s_hole + pen, used | {gi}, tuple(c2),
                                yx[gi], seq + ((gi, par, tuple(tee)),),
                                segs + (new_walk, new_spine),
                                cum + float(hole_len[list(idxs).index(gi)]),
                                cum_mid + mid_par,
                                (yx[gi] - np.asarray(tee))
                                / max(np.hypot(*(yx[gi] - np.asarray(tee))),
                                      1e-9),
                                lzs_ap + new_lz))
        if not nxt:
            return None
        nxt.sort(key=lambda s: (round(-s[0], 6),
                                tuple(q[0] for q in s[4]),
                                tuple(q[1] for q in s[4])))
        states = nxt[:BEAM_W]

    finals = states[:N_FINAL]
    # (2,5,2) ALWAYS AUDITIONS (2026-08-30): the route-level mix prior can
    # only rerank the finals, and on some seeds no (2,5,2) state survived
    # the beam -- pushing the prior weight further just shuffled the
    # minority between (3,3,3) and (1,7,1) (calibration ladder in
    # detail_route). If the finals carry no (2,5,2) state, the best one
    # from the full last generation joins them.
    if not any(st[2] == (2, 5, 2) for st in finals):
        for st in states:
            if st[2] == (2, 5, 2):
                finals = finals + [st]
                break
    best = None
    for st in finals:
        r = detail_route(f, rf, sit, pool, wet2, cell2, st)
        if r is not None and (best is None or r.score > best.score):
            best = r
    return best


# --- longitudinal profile scoring (measured 2026-08-30) ---------------------
# 5,201 real holes on 307 corpus courses (OSM golf=hole lines sampled on the
# 2 m tiles, 15 m edge-replicated smoothing). The mean real hole is a shallow
# U: tee ~2-3 m above mid-hole, green climbing ~1-2 m at the end. Medians:
# net_dz -0.4 m (p5 -12.7, p25 -3.9, p75 +2.5, p95 +10.1); 44% downhill.
# Climb 1.24 m per 100 m (p90 3.35). THE HARD FACT: max height above the
# tee->green chord is p50 0.1 m, p90 1.7 m, p99 5.6 m -- real holes go
# AROUND high ground, they do not play over it; ground BELOW the chord
# (gully carries) is unconstrained, which is the shot model's whole point.
PROF_NET_BAND = (-12.7, 10.1)      # p5-p95; trapezoid, ramp per siting rules
PROF_CHORD_FREE_M = 1.7            # p90: no penalty below
PROF_CHORD_SAT_M = 5.6             # p99: penalty saturates here
PROF_CLIMB_100_FREE = 1.6          # ~p60 climb per 100 m
PROF_CLIMB_100_SAT = 3.4           # p90


def spine_profile(f: Fields, spine: np.ndarray, step_m: float = 8.0):
    """net_dz, max_above_chord, climb_per_100m along a spine at 8 m."""
    segs = np.diff(spine, axis=0)
    seglen = np.hypot(segs[:, 0], segs[:, 1])
    L = float(seglen.sum())
    if L < 40.0:
        return 0.0, 0.0, 0.0
    cum = np.r_[0.0, np.cumsum(seglen)]
    ss = np.linspace(0.0, L, max(int(L / step_m), 4) + 1)
    ys = np.interp(ss, cum, spine[:, 0])
    xs = np.interp(ss, cum, spine[:, 1])
    yi = np.clip((ys / f.cell).astype(int), 0, f.z8.shape[0] - 1)
    xi = np.clip((xs / f.cell).astype(int), 0, f.z8.shape[1] - 1)
    z = f.z8[yi, xi]
    net_dz = float(z[-1] - z[0])
    chord = np.linspace(z[0], z[-1], len(z))
    above = float((z - chord).max())
    dz = np.diff(z)
    climb100 = float(dz[dz > 0].sum() / L * 100.0)
    return net_dz, above, climb100


def profile_terms(f: Fields, spine: np.ndarray) -> dict[str, float]:
    net_dz, above, climb100 = spine_profile(f, spine)
    net_t = float(trapezoid(np.array([net_dz]), *PROF_NET_BAND)[0])
    above_pen = float(np.clip((above - PROF_CHORD_FREE_M)
                              / (PROF_CHORD_SAT_M - PROF_CHORD_FREE_M), 0, 1))
    climb_pen = float(np.clip((climb100 - PROF_CLIMB_100_FREE)
                              / (PROF_CLIMB_100_SAT - PROF_CLIMB_100_FREE),
                              0, 1))
    return dict(prof_net=0.4 * net_t,
                prof_chord=-0.8 * above_pen,
                prof_climb=-0.4 * climb_pen,
                _net_dz=net_dz, _above=above, _climb100=climb100)


# --- line-of-play clearance (measured 2026-08-30) ---------------------------
# Real courses keep neighbouring lines of play apart, most strictly through
# the hole's MIDDLE: across 5,187 corpus holes with neighbours, the p10
# clearance to the nearest other line is ~50 m at t=0.3-0.7, tapering to
# ~30 m at the ends (t=0: 29 m, t=0.5: 50 m, t=1: 33 m) -- shared corridor
# mouths near tees/greens are normal, shared landing zones are not.
CLEAR_END_M = 30.0
CLEAR_MID_M = 50.0
LZ_SEP_M = 50.0          # landing zones of different holes stay this far apart
# A GREEN MUST NOT SIT IN ANOTHER HOLE'S LANDING ZONE (owner, 2026-08-30:
# seed 600125's par-3 3rd green sat on the 2nd's LZ -- an ideal drive would
# finish on the next green). Measured on 64,450 real (green, other-hole-LZ-
# region) pairs: p0.1 = 52 m, p1 = 81 m; only 0.08% of pairs fall under 50 m
# and 0.95% under 80 m. Real courses simply do not do this.
GREEN_LZ_SEP_M = 80.0
LZ_APPROX_M = 220.0      # where a drive finishes, for beam-time LZ estimates


def _clear_radius(t: np.ndarray) -> np.ndarray:
    """The measured clearance profile: 30 m at the ends, 50 m mid-hole."""
    ramp = np.clip(np.minimum(t, 1.0 - t) / 0.3, 0.0, 1.0)
    return CLEAR_END_M + (CLEAR_MID_M - CLEAR_END_M) * ramp


def _resample_t(poly: np.ndarray, step_m: float = 15.0):
    segs = np.diff(poly, axis=0)
    sl = np.hypot(segs[:, 0], segs[:, 1])
    L = float(sl.sum())
    if L < 2 * step_m:
        return None, None
    cum = np.r_[0.0, np.cumsum(sl)]
    ss = np.linspace(0.0, L, max(int(L / step_m), 3) + 1)
    pts = np.stack([np.interp(ss, cum, poly[:, 0]),
                    np.interp(ss, cum, poly[:, 1])], 1)
    return pts, ss / L


def clearance_violation(A: np.ndarray, B: np.ndarray,
                        consecutive: bool = False) -> float:
    """Worst fractional intrusion of polylines A and B into each other's
    measured clearance radius, weighted by BOTH holes' along-hole position
    (an end-of-A point brushing the MIDDLE of B counts at B's radius).

    consecutive=True exempts the junction: the end of A (green) and the
    start of B (next tee) legitimately share ground -- that adjacency IS
    walk minimization. Without the exemption the gentle-tile score fell
    34 -> 4.9 purely on tee-beside-previous-green pairs."""
    pa, ta = _resample_t(A)
    pb, tb = _resample_t(B)
    if pa is None or pb is None:
        return 0.0
    if consecutive:
        keep_a = ta <= 0.82
        keep_b = tb >= 0.18
        pa, ta = pa[keep_a], ta[keep_a]
        pb, tb = pb[keep_b], tb[keep_b]
        if not len(pa) or not len(pb):
            return 0.0
    d = np.sqrt(((pa[:, None, :] - pb[None, :, :]) ** 2).sum(-1))
    r = np.maximum(_clear_radius(ta)[:, None], _clear_radius(tb)[None, :])
    v = np.clip((r - d) / r, 0.0, 1.0)
    return float(v.max())


# --- clubhouse keep-out ------------------------------------------------------
# Owner (2026-08-30): "the clubhouse sits right in front of the green
# locations" on some seeds. Holes 1 and 9 are REQUIRED to come within 120 m
# of the clubhouse, so proximity alone is not the crime -- a LINE OF PLAY
# passing over the clubhouse pad is. Keep-out disc CH_KEEPOUT_M (the pad
# gate is a 25-60 m buildable disc; 45 m covers it), measured against every
# spine with its endpoints trimmed 20 m so tee-1-beside-the-clubhouse and
# green-9-beside-the-clubhouse stay legal. Walks are exempt -- walking past
# the clubhouse is what a clubhouse is for.
CH_KEEPOUT_M = 45.0


def ch_intrusion(spine: np.ndarray, ch_yx, trim_m: float = 20.0) -> float:
    """Fractional intrusion of the clubhouse point into the trimmed spine's
    keep-out corridor: 0 = line stays >= CH_KEEPOUT_M away, 1 = dead over."""
    sp = _trim(np.asarray(spine, float), trim_m)
    c = np.asarray(ch_yx, float)
    best = np.inf
    for i in range(len(sp) - 1):
        a2 = sp[i]
        b2 = sp[i + 1]
        ab = b2 - a2
        t = np.clip(np.dot(c - a2, ab) / max(np.dot(ab, ab), 1e-9), 0, 1)
        best = min(best, float(np.hypot(*(a2 + t * ab - c))))
    return float(np.clip((CH_KEEPOUT_M - best) / CH_KEEPOUT_M, 0, 1))


# --- detail placement + exact rescore --------------------------------------

def _walk(f: Fields, a_yx, b_yx, wet2, cell2, hole: int) -> Walk:
    a = np.asarray(a_yx, float)
    b = np.asarray(b_yx, float)
    L = float(np.hypot(*(b - a)))
    n = max(int(L / f.cell), 1)
    ts = np.linspace(0, 1, n + 1)
    pts = a[None, :] + ts[:, None] * (b - a)[None, :]
    yi = np.clip((pts[:, 0] / f.cell).astype(int), 0, f.z8.shape[0] - 1)
    xi = np.clip((pts[:, 1] / f.cell).astype(int), 0, f.z8.shape[1] - 1)
    zz = f.z8[yi, xi]
    grade = float(np.abs(np.diff(zz)).sum() / max(L, 1e-9)) if n > 1 else 0.0
    return Walk(np.array([a, b]), L, grade,
                wet_spans(wet2, cell2, a, b, "walk", hole))


def _backtrack_m(prev_dir, walk: Walk) -> float:
    """Metres of the walk spent going AGAINST the prior hole's closing
    direction (real p50 is zero; golfers walk onward, not back)."""
    if prev_dir is None or walk.length_m < 5.0:
        return 0.0
    wv = walk.path[1] - walk.path[0]
    return float(max(0.0, -np.dot(prev_dir, wv)))


def detail_route(f: Fields, rf: RouteFields, sit: Siting, pool,
                 wet2: np.ndarray, cell2: float, state) -> Route | None:
    _, _, counts, _, seq, _, _cum, _cm, _pd, _lz = state
    home = np.asarray(sit.clubhouse.yx, float)
    holes: list[Hole] = []
    spines: list[np.ndarray] = []
    walks: list[np.ndarray] = []
    lz_seen: list[tuple[float, float]] = []
    prev_dir = None                       # unit closing direction of prev hole
    pos = home
    total_len = total_walk = 0.0
    hole_scores = 0.0
    yx_all, pct = _pool_arrays(pool)

    # LENGTH BUDGET (2026-08-30): the beam's pace term cannot bind because
    # detail re-derives lengths, and the clearance tier was buying spacing
    # with band-top holes on every seed (totals 3.5-3.6 km). Each hole's
    # band cap now shrinks to what the 3,100 m budget minus the remaining
    # holes' band minimums allows -- totals are bounded by construction.
    BUDGET_M = 3100.0
    for h, (gi, par, _est) in enumerate(seq):
        g = np.asarray(pool[gi].yx, float)
        # every OTHER hole's green is a fixed obstacle for this hole's LZs
        other_greens = [tuple(pool[q[0]].yx) for q in seq if q[0] != gi]
        rest_min = sum(PAR_BANDS[pp][0] for (_g2, pp, _e2) in seq[h + 1:])
        hi_cap = BUDGET_M - total_len - rest_min
        opts = place_tee(rf, f, pos, g, par,
                         clubhouse_yx=home if h == 0 else None,
                         avoid_spines=spines, avoid_walks=walks,
                         hi_cap=hi_cap, ch_keepout_yx=home)
        if not opts:
            # saturating fallback: tee at the walk-disc edge toward the green
            v = g - pos
            v = v / max(np.hypot(*v), 1e-9)
            lo, hi = PAR_BANDS[par]
            d = np.hypot(*(g - pos))
            back = pos + v * max(0.0, d - (lo + hi) / 2)
            opts = [(tuple(back), -0.5, True)]

        # JOINT tee x LZ retry: a clean tee whose only LZs cross earlier
        # geometry is not clean at all -- try the next tee option before
        # accepting a crossing (the walk-4/hole-6 case above).
        chosen = None
        for (tee_yx, tee_q, tee_graded) in opts:
            lzs = []
            lz_score = 1.0
            all_clean = True
            if par >= 4:
                r1 = place_lz(rf, f, tee_yx, g, DRIVE_R_M,
                              (90.0, 200.0) if par == 4 else (300.0, 999.0),
                              avoid_spines=spines, avoid_walks=walks,
                              avoid_lzs=lz_seen, avoid_greens=other_greens,
                              ch_keepout_yx=home)
                if r1 is not None:
                    lzs.append(r1[0])
                    lz_score = r1[1]
                    all_clean &= r1[2]
            if par == 5 and lzs:
                r2 = place_lz(rf, f, (lzs[0][0], lzs[0][1]), g, SECOND_R_M,
                              (120.0, 200.0),
                              avoid_spines=spines, avoid_walks=walks,
                              avoid_lzs=lz_seen, avoid_greens=other_greens,
                              ch_keepout_yx=home)
                if r2 is not None:
                    lzs.append(r2[0])
                    lz_score = 0.5 * (lz_score + r2[1])
                    all_clean &= r2[2]
            if chosen is None:
                chosen = (tee_yx, tee_q, tee_graded, lzs, lz_score)
            if all_clean:
                chosen = (tee_yx, tee_q, tee_graded, lzs, lz_score)
                break
        tee_yx, tee_q, tee_graded, lzs, lz_score = chosen
        lz_seen += [(y, x) for (y, x, _r) in lzs]

        pts = [np.asarray(tee_yx, float)] + \
              [np.array([y, x]) for (y, x, _r) in lzs] + [g]
        spine = np.array(pts)
        length = float(sum(np.hypot(*(pts[i + 1] - pts[i]))
                           for i in range(len(pts) - 1)))
        vin = pts[-1] - pts[-2]
        nv = max(np.hypot(*vin), 1e-9)
        ab = approach_bin(vin / nv)
        appr = score_approach(pool[gi].approach, ab)

        bridges = []
        for i in range(len(pts) - 1):
            bridges += wet_spans(wet2, cell2, pts[i], pts[i + 1], "spine", h)
        walk = _walk(f, pos, tee_yx, wet2, cell2, h)

        terms = dict(
            green=1.5 * float(pct[gi]),
            approach=1.0 * appr,
            lz=0.8 * float(np.clip(lz_score, -1, 1)) if par >= 4 else 0.0,
            tee=0.4 * float(np.clip(tee_q, -1, 1)),
            length=0.5 * float(trapezoid(np.array([length]),
                                         *PAR_BANDS[par])[0]),
            walk=-1.0 * float(np.clip((walk.length_m - WALK_FREE_M)
                                      / (WALK_SAT_M - WALK_FREE_M), 0, 1))
                 - 0.3 * float(np.clip(walk.length_m / WALK_MAX_M, 0, 1)),
            backtrack=-0.7 * float(np.clip(
                (_backtrack_m(prev_dir, walk) - BACKTRACK_FREE_M)
                / (BACKTRACK_SAT_M - BACKTRACK_FREE_M), 0, 1)),
            walk_grade=-0.15 * float(np.clip(walk.grade_mean / 0.15, 0, 1)),
            bridges=max(-1.5, sum(-0.3 - 0.005 * b.span_m
                                  for b in bridges + walk.bridges)),
            b2b=-0.6 if (h and par in (3, 5) and seq[h - 1][1] == par) else 0.0,
        )
        terms["ch_keepout"] = -6.0 * ch_intrusion(spine, home)
        pt = profile_terms(f, spine)
        terms.update({k: v for k, v in pt.items() if not k.startswith("_")})
        terms["net_dz_m"] = round(pt["_net_dz"], 1)
        terms["above_chord_m"] = round(pt["_above"], 1)
        hole_scores += sum(v for k, v in terms.items()
                           if k not in ("net_dz_m", "above_chord_m"))
        holes.append(Hole(h, par, int(gi), (float(g[0]), float(g[1])),
                          tee_boxes(rf, f, tee_yx, pts[1], g), lzs, spine,
                          length, ab, bridges, walk, terms))
        spines.append(spine)
        walks.append(walk.path)
        total_len += length
        total_walk += walk.length_m
        pos = g
        v_close = pts[-1] - pts[-2]
        n_close = np.hypot(*v_close)
        prev_dir = v_close / n_close if n_close > 1e-9 else None

    # line-of-play clearance + LZ separation, exact
    clear_pen = 0.0
    worst_clear = 0.0
    for i in range(9):
        for j in range(i + 1, 9):
            v = clearance_violation(spines[i], spines[j],
                                    consecutive=(j == i + 1))
            worst_clear = max(worst_clear, v)
            clear_pen -= 3.0 * v * v          # quadratic: brushing is cheap,
                                              # sharing a corridor is not
    green_lz_pen = 0.0
    worst_glz = 999.0
    for hh in holes:
        for (ly, lx, _r) in hh.lzs:
            for h2 in holes:
                if h2.index == hh.index:
                    continue
                d_gl = float(np.hypot(h2.green_yx[0] - ly,
                                      h2.green_yx[1] - lx))
                worst_glz = min(worst_glz, d_gl)
                if d_gl < GREEN_LZ_SEP_M:
                    green_lz_pen -= 5.0 * (1.0 - d_gl / GREEN_LZ_SEP_M)
    lz_all = [(hh.index, y, x) for hh in holes for (y, x, _r) in hh.lzs]
    lz_pen = 0.0
    for ii in range(len(lz_all)):
        for jj in range(ii + 1, len(lz_all)):
            if lz_all[ii][0] == lz_all[jj][0]:
                continue
            dd = np.hypot(lz_all[ii][1] - lz_all[jj][1],
                          lz_all[ii][2] - lz_all[jj][2])
            if dd < LZ_SEP_M:
                # SHARED LANDING ZONES ARE AVOIDED (owner): heavy, saturating
                lz_pen -= 2.0 * (1.0 - dd / LZ_SEP_M)

    # crossings, exact
    crossings = []
    cross_pen = 0.0
    for i in range(9):
        for j in range(i + 1, 9):
            for pt in polyline_crossings(spines[i], spines[j]):
                crossings.append((i, j, (float(pt[0]), float(pt[1]))))
    # play x play: near-veto (owner, 2026-08-30: possible but VERY rare,
    # ~1% of courses -- the first 20-seed round produced crossings on 8/20
    # at -3.0, so the price doubles and the cap loosens)
    cross_pen += max(-18.0, -6.0 * len(crossings))
    walk_cross = 0
    for i in range(9):
        for j in range(9):
            if j in (i, i - 1):
                continue        # a walk legitimately touches its own holes
            if polyline_crossings(walks[i], spines[j]):
                walk_cross += 1
    cross_pen += max(-8.0, -4.0 * walk_cross)

    kinds = [pool[hh.green_idx].kind for hh in holes]
    from collections import Counter
    cnt = np.array(list(Counter(kinds).values()), float)
    p = cnt / cnt.sum()
    entropy = float(-(p * np.log(p)).sum() / np.log(9))
    spread = 0.0
    for par, need in ((3, 40.0), (4, 80.0), (5, 50.0)):
        ls = [hh.length_m for hh in holes if hh.par == par]
        if len(ls) >= 2 and max(ls) - min(ls) >= need:
            spread += 0.3
    # weight 1.0 -> 2.5, tail 0.4 -> 0.05 (2026-08-30): adding the beam
    # clearance term taught the router to BUY spacing with length (3,603 m
    # totals); the band must push back as hard as the spacing pulls
    tot_t = float(trapezoid(np.array([total_len]), *TOTAL_BAND_M,
                            tail=0.05)[0])

    # PAR-5 POSITIONAL SPREAD (owner, 2026-08-30): the length budget was
    # banking yardage for the end and the fives all landed on holes 7-9.
    # Reward fives whose indices span the round; -0.8 when they bunch in
    # any 3-hole stretch.
    # MIX PRIOR, route side: (2,5,2) is what architects build (94.2% of
    # real par-36 nines); the other two stay legal-but-exceptional.
    n3 = sum(1 for hh in holes if hh.par == 3)
    n5 = sum(1 for hh in holes if hh.par == 5)
    # calibration ladder (200 fresh seeds per rung, real = 94.2/2.7/3.1):
    #   1.2 / 0.15 / 0.0   ->  73.5 / 19.0 / 7.5
    #   2.0 / 0.0  / 0.0   ->  86.0 / 10.5 / 3.5   ((1,7,1) on target)
    #   2.0 / 0.0  / -0.6  ->  86.0 /  7.5 / 6.5   (minority reshuffled;
    #                            the real limiter is (2,5,2) missing from
    #                            the finals -- fixed structurally below)
    mix_prior = {(2, 2): 2.0, (1, 1): 0.0, (3, 3): -0.6}.get((n3, n5), 0.0)
    idx5 = [hh.index for hh in holes if hh.par == 5]
    if len(idx5) >= 2 and (max(idx5) - min(idx5)) < 3:
        spread -= 0.8
    elif len(idx5) >= 2 and (max(idx5) - min(idx5)) >= 4:
        spread += 0.3
    rterms = dict(entropy=0.6 * entropy, spread=spread, total=2.5 * tot_t,
                  mix_prior=mix_prior,
                  crossings=cross_pen, clearance=clear_pen, lz_sep=lz_pen,
                  green_in_lz=green_lz_pen,
                  worst_clear=0.0)
    rterms["worst_clear"] = 0.0        # diagnostic below, not a score term
    score = hole_scores + sum(v for k, v in rterms.items()
                              if k not in ("worst_clear", "min_green_lz_m"))
    rterms["worst_clear"] = round(worst_clear, 2)
    rterms["min_green_lz_m"] = round(worst_glz, 0) if worst_glz < 999 else -1
    return Route(holes, [hh.par for hh in holes], total_len, total_walk,
                 (float(home[0]), float(home[1])), float(score),
                 rterms, crossings)


def run_routing(z2, cell2, wet2, sit: Siting, f: Fields, pool,
                pool_fn=None) -> Route | None:
    """Entry point: route nine holes on an already-sited window + pool.

    ROUTING PICKS THE CLUBHOUSE (owner, 2026-08-30): siting proposes up to
    three spatially distinct clubhouse candidates; a route is auditioned
    from each and the best-scoring route wins -- the routing is what
    matters, the clubhouse serves it. `pool_fn(clubhouse) -> pool`
    regenerates the pool per candidate (the pool's reserved loop anchor
    depends on the clubhouse); when absent the given pool is reused.
    """
    rf = build_route_fields(f)
    alts = getattr(sit.clubhouse, "alternates", None) or [sit.clubhouse]
    best = None
    primary = sit.clubhouse
    for ch in alts:
        sit.clubhouse = ch
        p_ch = pool_fn(ch) if pool_fn is not None else pool
        r = beam_route(f, rf, sit, p_ch, wet2, cell2)
        if r is not None and (best is None or r.score > best.score):
            best = r
    sit.clubhouse = primary
    return best
