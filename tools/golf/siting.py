#!/usr/bin/env python3
"""S5 siting prototype: score the core, choose the play window, site the
clubhouse. (P0-P4 of the siting plan; green candidates are `greens.py`.)

Python prototype of what ports to `crates/course-substrate` /
`crates/course-routing` — the scoring is uncalibrated until the render-review
rounds pass, and every primitive it leans on (geomorphons, surfaces, pads,
the proxy floors) lives in Python today.

Design decisions carried from the plan, with their reasons:

* The window side is PER-BIOME (`play_m`): measured 2026-08-28, a 600 m
  window fits a 9-hole route on 2/12 aeolian seeds; 1200 m reaches 11/12.
* Calm is a FLOOR, not a maximand (d6w-siting-baseline: "otherwise generated
  courses will systematically sit on duller ground than real ones").
  Banded terms are trapezoids over the accepted band; the maximand is the
  morphology mix and pad density.
* Pruned JOINT window/clubhouse search: exhaustive SAT scan, NMS, top ~30,
  clubhouse + loop-probe per survivor, best pair wins. Without NMS the top
  30 are 30 translations of one window.
* The returning-nine constraint (`holes[0].tee` / `holes[8].green` within
  clubhouse_radius_m) is a HARD GATE at clubhouse time — better siting
  refuses a clubhouse that cannot anchor a loop than routing discovers it
  after its whole budget.

Units: slope is RISE/RUN everywhere in this file. `playability.pads` wants
percent; the conversion happens at that call and nowhere else.
"""
from __future__ import annotations

import pathlib
import sys
from dataclasses import dataclass, field

import numpy as np
from scipy import ndimage

_TOOLS = pathlib.Path(__file__).resolve().parent.parent
for _p in ("macro_campaign", "metrics", "golf_proxy", "dtm_primitives", "golf"):
    sys.path.insert(0, str(_TOOLS / _p))

from dtm_primitives import geomorphons as gm          # noqa: E402
import playability as play                            # noqa: E402

# --- world geometry (docs/01-conventions.md) ------------------------------
EXTENT_M = 3000.0
CORE_MIN_M, CORE_MAX_M = 750.0, 2250.0
CLUBHOUSE_RADIUS_M = 120.0          # C3 Budgets fixture

# Accepted bands from d6w-siting-baseline (real courses, per-province p10-p90
# where measured; sandhills row used as the aeolian default).
CALM_BAND = (0.32, 0.79)            # fraction of window at slope <= 0.08
RELIEF_BAND_M = (8.0, 55.0)         # window p95-p5 (window-scale re-measure)
POS_BAND = (0.25, 0.65)             # relief_pos of the window mean

GREEN_SLOPE_RR = 0.05               # rise/run; playability GREEN_SLOPE is 5%
TEE_SLOPE_RR = 0.065
FAIRWAY_SLOPE_RR = 0.08


# --------------------------------------------------------------------------
# P0 — fields


@dataclass
class Fields:
    cell: float                     # 8.0
    z8: np.ndarray                  # 8 m height over the FULL extent
    slope: np.ndarray               # rise/run
    relief_pos: np.ndarray          # percentile rank of lowpass elevation
    tpi200: np.ndarray
    subgrid_rough: np.ndarray       # std of the 2 m heights under each cell
    wet8: np.ndarray                # bool
    d_water: np.ndarray             # m to nearest wet cell (inf if none)
    pad_green: np.ndarray           # bool, disc-eroded
    pad_tee: np.ndarray
    pad_fair: np.ndarray
    room: np.ndarray                # EDT inside pad_green, m
    flow_accum: np.ndarray | None   # m^2, from C2 when present


def block_reduce_std(z2: np.ndarray, k: int) -> np.ndarray:
    """Std of each k x k block — the 2 m information an 8 m cell threw away."""
    ny, nx = z2.shape[0] // k, z2.shape[1] // k
    b = z2[:ny * k, :nx * k].reshape(ny, k, nx, k)
    return b.std(axis=(1, 3))


def build_fields(z2: np.ndarray, cell2: float, wet2: np.ndarray,
                 flow_accum8: np.ndarray | None = None) -> Fields:
    k = int(round(8.0 / cell2))
    z8 = z2[::k, ::k].astype(np.float64)
    cell = cell2 * k
    gy, gx = np.gradient(z8, cell)
    slope = np.hypot(gx, gy)

    # relief_pos on a 400 m lowpass, stable rank (dtm_metrics convention)
    lp = ndimage.gaussian_filter(z8, 400.0 / np.pi / cell)
    order = np.argsort(lp.ravel(), kind="stable")
    rp = np.empty(lp.size)
    rp[order] = np.arange(lp.size) / max(lp.size - 1, 1)
    relief_pos = rp.reshape(lp.shape)

    tpi200 = z8 - ndimage.uniform_filter(z8, int(round(200.0 / cell)) | 1)
    rough = block_reduce_std(z2, k)
    # z8 (strided) is one cell larger than the block reduce; edge-pad to match
    py = z8.shape[0] - rough.shape[0]
    px = z8.shape[1] - rough.shape[1]
    if py > 0 or px > 0:
        rough = np.pad(rough, ((0, max(py, 0)), (0, max(px, 0))), mode="edge")
    rough = rough[:z8.shape[0], :z8.shape[1]]

    wet8 = wet2[::k, ::k][:z8.shape[0], :z8.shape[1]]
    d_water = (ndimage.distance_transform_edt(~wet8, sampling=cell)
               if wet8.any() else np.full(z8.shape, np.inf))

    # pads() takes PERCENT; convert here and nowhere else.
    pad_green = play.pads(z8, cell, wet8, play.GREEN_R,
                          GREEN_SLOPE_RR * 100.0, play.GREEN_RELIEF)
    pad_tee = play.pads(z8, cell, wet8, play.TEE_R, TEE_SLOPE_RR * 100.0)
    pad_fair = (slope <= FAIRWAY_SLOPE_RR) & ~wet8
    room = ndimage.distance_transform_edt(pad_green, sampling=cell)
    return Fields(cell, z8, slope, relief_pos, tpi200, rough, wet8, d_water,
                  pad_green, pad_tee, pad_fair, room, flow_accum8)


# --------------------------------------------------------------------------
# P1 — morphology (two scales + derived saddles)


@dataclass
class Morphology:
    cls240: np.ndarray
    cls80: np.ndarray
    saddle: np.ndarray
    fall_axis: np.ndarray           # [0, pi) open axis
    rise_axis: np.ndarray           # [0, pi) blocked axis
    collar: int                     # px inside which classes are artifacts


def build_morphology(f: Fields) -> Morphology:
    cls240, pb, mb = gm.classify(f.z8, f.cell, lookup_m=240.0, skip_m=16.0,
                                 flat_deg=1.0, return_dirs=True)
    cls80 = gm.classify(f.z8, f.cell, lookup_m=80.0, skip_m=8.0, flat_deg=1.0)
    saddle, fall, rise = gm.saddle_axes(pb, mb)
    return Morphology(cls240, cls80, saddle, fall, rise,
                      gm.collar_px(240.0, f.cell))


# --------------------------------------------------------------------------
# P2 — persistence by priority-flood (pits AND peaks), with spill bearings


def _flood_fill_level(z: np.ndarray, cell: float):
    """Priority-flood fill level per cell + label of the depression's spill.

    `fill - z` is pit persistence: exactly the depth a punchbowl holds
    before it pours over its lowest rim — threshold-free, amplitude-
    invariant, and the only instrument that ranks hollows in a biome whose
    flow accumulation is identically zero.
    """
    import heapq
    ny, nx = z.shape
    fill = np.full(z.shape, np.inf)
    spill_dir = np.zeros(z.shape)          # bearing rad toward the outlet
    seen = np.zeros(z.shape, bool)
    pq = []
    for x in range(nx):
        for y in (0, ny - 1):
            pq.append((z[y, x], y, x))
    for y in range(ny):
        for x in (0, nx - 1):
            pq.append((z[y, x], y, x))
    for lvl, y, x in pq:
        fill[y, x] = z[y, x]
    heapq.heapify(pq)
    while pq:
        lvl, y, x = heapq.heappop(pq)
        if seen[y, x]:
            continue
        seen[y, x] = True
        for dy, dx in ((-1, 0), (1, 0), (0, -1), (0, 1)):
            j, i = y + dy, x + dx
            if 0 <= j < ny and 0 <= i < nx and not seen[j, i]:
                nl = max(lvl, z[j, i])
                if nl < fill[j, i]:
                    fill[j, i] = nl
                    # water leaves toward the neighbour we flooded FROM
                    spill_dir[j, i] = np.arctan2(-dy, -dx)
                    heapq.heappush(pq, (nl, j, i))
    return fill, spill_dir


@dataclass
class Persistence:
    pit: np.ndarray                 # m of held water before the spill
    peak: np.ndarray                # prominence, m
    spill_dir: np.ndarray           # rad, the punchbowl's OPEN bearing


def build_persistence(f: Fields) -> Persistence:
    """Core-only: siting never uses persistence outside it, and the pure-
    Python flood is the prototype's slowest kernel (fine in Rust)."""
    lo = int(round(CORE_MIN_M / f.cell))
    hi = int(round(CORE_MAX_M / f.cell))
    zc = f.z8[lo:hi, lo:hi]
    pit = np.zeros(f.z8.shape)
    peak = np.zeros(f.z8.shape)
    spill = np.zeros(f.z8.shape)
    fill, sp = _flood_fill_level(zc, f.cell)
    pit[lo:hi, lo:hi] = fill - zc
    fill_inv, _ = _flood_fill_level(-zc, f.cell)
    peak[lo:hi, lo:hi] = fill_inv + zc
    spill[lo:hi, lo:hi] = sp
    return Persistence(pit, peak, spill)


# --------------------------------------------------------------------------
# P3 — SAT window scan


def _sat(a: np.ndarray) -> np.ndarray:
    s = np.zeros((a.shape[0] + 1, a.shape[1] + 1))
    np.cumsum(np.cumsum(a, 0), 1, out=s[1:, 1:])
    return s


def _win_mean(sat: np.ndarray, h: int, w: int) -> np.ndarray:
    """Mean of every h x w window; output [ny-h+1, nx-w+1]."""
    return (sat[h:, w:] - sat[:-h, w:] - sat[h:, :-w] + sat[:-h, :-w]) / (h * w)


def trapezoid(v: np.ndarray, lo: float, hi: float,
              ramp: float = 0.25, tail: float = 0.4) -> np.ndarray:
    """1.0 across [lo, hi], ramping from 0 below and to `tail` above.

    The non-zero upper tail is deliberate: a veto is a hard constraint, hard
    constraints force retries, and v2 has none. Calmer-than-the-band ground
    is dull, not illegal.
    """
    span = hi - lo
    r = ramp * span
    up = np.clip((v - (lo - r)) / r, 0.0, 1.0)
    down = 1.0 - (1.0 - tail) * np.clip((v - hi) / r, 0.0, 1.0)
    return np.minimum(up, down)


@dataclass
class WindowScan:
    score: np.ndarray               # per top-left position, 8 m grid
    ij0: tuple[int, int]            # grid offset of position (0,0)
    h: int                          # window height, cells (y)
    w: int                          # window width, cells (x)
    terms: dict = field(default_factory=dict)


def scan_windows_one(f: Fields, m: Morphology, h_m: float, w_m: float
                     ) -> WindowScan:
    """One orientation of the per-biome rectangle. The window is a RECT, not
    a square: the three real 9-hole courses measure 1517x487, 1222x304 and
    1357x628 -- ribbons -- and a square can never contain them."""
    cell = f.cell
    h = int(round(h_m / cell))
    w = int(round(w_m / cell))
    lo = int(round(CORE_MIN_M / cell))
    hi = int(round(CORE_MAX_M / cell))
    core = np.s_[lo:hi, lo:hi]

    z = f.z8[core]
    calm = _win_mean(_sat((f.slope[core] <= FAIRWAY_SLOPE_RR).astype(float)), h, w)
    posm = _win_mean(_sat(f.relief_pos[core]), h, w)
    padg = _win_mean(_sat(f.pad_green[core].astype(float)), h, w)
    padf = _win_mean(_sat(f.pad_fair[core].astype(float)), h, w)
    wat = _win_mean(_sat(f.wet8[core].astype(float)), h, w)
    rough = _win_mean(_sat(f.subgrid_rough[core]), h, w)
    # window relief via rect max/min filters (exact, cheap)
    zmax = ndimage.maximum_filter(z, size=(h, w))[h // 2:h // 2 + calm.shape[0],
                                                  w // 2:w // 2 + calm.shape[1]]
    zmin = ndimage.minimum_filter(z, size=(h, w))[h // 2:h // 2 + calm.shape[0],
                                                  w // 2:w // 2 + calm.shape[1]]
    relief = zmax - zmin

    def frac(mask):
        return _win_mean(_sat(mask[core].astype(float)), h, w)
    f_shoulder = frac(m.cls240 == 4)
    f_spur = frac(m.cls240 == 5)
    f_hollow = frac(m.cls80 == 7)
    f_saddle = frac(m.saddle)

    banded = (trapezoid(calm, *CALM_BAND)
              * trapezoid(relief, *RELIEF_BAND_M)
              * trapezoid(posm, *POS_BAND))
    # WATER IS AMENITY IN THE MEASURED BAND, not a defect (corpus 2026-08-29).
    # Softening the old -1.5*wat to -0.5 was NOT enough and the measurement
    # said so: with the softer penalty only 26% of sited windows still held
    # any water against 93% of real Carolina courses, because water suppresses
    # the score through THREE channels, not one -- the explicit term plus
    # pad_green and pad_fair, both of which exclude wet cells. A pure penalty
    # can only ever push siting away.
    # So water gets a trapezoid, the same shape as the surround-relief term
    # and for the same reason ("floors not maximands", d6w): no credit when
    # dry (38% of real Nebraska courses carry none, so dry must stay viable),
    # full credit across the real in-course band 0.5-4.5%, and a real penalty
    # only past 12% where water starts eating routable land.
    wat_t = np.where(wat < 0.005, 0.0,
             np.where(wat <= 0.045, 0.45,
              np.where(wat <= 0.12, 0.45 - 0.45 * (wat - 0.045) / 0.075,
                       -1.5 * np.clip((wat - 0.12) / 0.10, 0, 1))))
    # Grounding: real Carolina sandhills courses carry a median 1.7% water
    # inside the course polygon (p75 3.5%) and 93% hold at least one body, so
    # a window with a couple of ponds is normal ground. The fitted green score
    # puts water proximity at ~zero (d_water_band coef 0.0003) and greens ON
    # water are excluded by a hard gate elsewhere, so nothing downstream needs
    # this term to be defensive. Measured effect of the trapezoid: fluvial
    # sited windows holding any water 26% -> 76%, median 0.00% -> 0.60%.
    character = (2.0 * padg + 1.0 * padf
                 + 3.0 * (f_shoulder + f_spur + f_hollow + 2.0 * f_saddle)
                 + wat_t - 0.8 * np.clip(rough / 0.6, 0, 1))
    score = banded * (0.2 + character)
    return WindowScan(score, (lo, lo), h, w,
                      terms=dict(calm=calm, relief=relief, pos=posm,
                                 padg=padg, banded=banded))


def scan_windows(f: Fields, m: Morphology, long_m: float, short_m: float
                 ) -> list[WindowScan]:
    """Both axis-aligned orientations; S5 picks across them. Terrain grain is
    random per seed, so two orientations recover most of what free rotation
    would have bought -- rotation itself stays rejected (nesting ladder,
    deliverable format)."""
    if abs(long_m - short_m) < 1e-9:
        return [scan_windows_one(f, m, long_m, long_m)]
    return [scan_windows_one(f, m, short_m, long_m),   # landscape
            scan_windows_one(f, m, long_m, short_m)]   # portrait


def shortlist(scans: list[WindowScan], cell: float, n: int = 30,
              min_sep_m: float = 200.0) -> list[tuple[int, int, int]]:
    """Top-n (orientation, i, j) after NMS across BOTH orientations.
    Total order: (-score, orient, i, j)."""
    sep = max(1, int(round(min_sep_m / cell)))
    entries = []
    for oi, scan in enumerate(scans):
        s = scan.score
        for k in np.argsort(s.ravel(), kind="stable")[::-1][:4 * n]:
            i, j = int(k // s.shape[1]), int(k % s.shape[1])
            if np.isfinite(s[i, j]):
                entries.append((-float(s[i, j]), oi, i, j))
    entries.sort()
    kept: list[tuple[int, int, int]] = []
    centres: list[tuple[float, float]] = []
    for negs, oi, i, j in entries:
        scan = scans[oi]
        cy = (i + scan.h / 2.0) * cell
        cx = (j + scan.w / 2.0) * cell
        if any((cy - a) ** 2 + (cx - b) ** 2 < min_sep_m ** 2
               for (a, b) in centres):
            continue
        kept.append((oi, i, j))
        centres.append((cy, cx))
        if len(kept) >= n:
            break
    return kept


# --------------------------------------------------------------------------
# P4 — clubhouse + joint score


@dataclass
class Clubhouse:
    yx: tuple[float, float]         # world metres
    score: float
    reserved_green: tuple[float, float]
    reserved_tee: tuple[float, float]
    alternates: list = field(default_factory=list)   # spatially distinct picks


def _disc(f: Fields, r_m: float) -> np.ndarray:
    n = max(1, int(round(r_m / f.cell)))
    yy, xx = np.mgrid[-n:n + 1, -n:n + 1]
    return (yy * yy + xx * xx) <= n * n


@dataclass
class ClubhouseFields:
    pad_room: np.ndarray            # EDT inside the buildable mask, m
    near_green: np.ndarray
    near_tee: np.ndarray


# A clubhouse needs SOME flat dry ground, but a hard 60 m disc is over-gated:
# measured on aeolian seed 600009 it left ZERO legal cells on the whole tile
# (no 120 m circle under 8% slope exists in dune country -- real clubhouse
# pads are graded). Floor at 25 m, reward up to 60.
CH_PAD_MIN_M = 25.0
CH_PAD_GOOD_M = 60.0


def build_clubhouse_fields(f: Fields) -> ClubhouseFields:
    """Computed once, not once per shortlisted window."""
    ok = (f.slope <= FAIRWAY_SLOPE_RR) & ~f.wet8
    pad_room = ndimage.distance_transform_edt(ok, sampling=f.cell)
    rr = _disc(f, CLUBHOUSE_RADIUS_M)
    near_green = ndimage.maximum_filter(f.pad_green.astype(np.uint8),
                                        footprint=rr).astype(bool)
    near_tee = ndimage.maximum_filter(f.pad_tee.astype(np.uint8),
                                      footprint=rr).astype(bool)
    return ClubhouseFields(pad_room, near_green, near_tee)


def site_clubhouse(f: Fields, win_ij: tuple[int, int], scan: WindowScan,
                   cf: ClubhouseFields) -> Clubhouse | None:
    """Best clubhouse for one window, or None if no legal anchor exists.

    Hard gates: a 60 m buildable dry pad, and a green-grade AND tee-grade
    pad within CLUBHOUSE_RADIUS_M — the returning-nine anchor. Everything
    else is soft: prospect, edge affinity (the CENTRE is penalised — a
    3.1 km route in this window cannot spare its middle), departure spread.
    """
    cell = f.cell
    i0 = scan.ij0[0] + win_ij[0]
    j0 = scan.ij0[1] + win_ij[1]
    h, w = scan.h, scan.w
    # clubhouse may sit in the window or within 100 m outside it
    halo = int(round(100.0 / cell))
    a0, a1 = max(0, i0 - halo), min(f.z8.shape[0], i0 + h + halo)
    b0, b1 = max(0, j0 - halo), min(f.z8.shape[1], j0 + w + halo)

    cand = np.zeros(f.z8.shape, bool)
    cand[a0:a1, b0:b1] = True
    cand &= (cf.pad_room >= CH_PAD_MIN_M) & cf.near_green & cf.near_tee
    # CENTRAL MASK (owner, 2026-08-30): a clubhouse deep in the middle of
    # the property makes a returning nine hard to fit -- the route must
    # thread around its own origin twice. The soft edge term was not enough
    # (it competed against prospect/pad quality); the central 30%-box of the
    # window is now excluded outright, with a soft fallback if that empties
    # the candidate set (never a hard failure on a cramped window).
    ci_lo = i0 + int(0.35 * h)
    ci_hi = i0 + int(0.65 * h)
    cj_lo = j0 + int(0.35 * w)
    cj_hi = j0 + int(0.65 * w)
    central = np.zeros(f.z8.shape, bool)
    central[ci_lo:ci_hi, cj_lo:cj_hi] = True
    if (cand & ~central).any():
        cand &= ~central
    if not cand.any():
        return None

    ys, xs = np.where(cand)
    zwin = f.z8[i0:i0 + h, j0:j0 + w]
    zlo, zhi = zwin.min(), zwin.max()
    rel = max(zhi - zlo, 1e-9)
    cy, cx = i0 + h / 2.0, j0 + w / 2.0

    prospect = np.clip((f.z8[ys, xs] - zlo) / rel, 0, 1)
    p_term = trapezoid(prospect, 0.3, 0.8)
    d_centre = np.hypot(ys - cy, xs - cx) * cell
    half_min = 0.5 * min(h, w) * cell
    e_term = trapezoid(d_centre, 0.5 * half_min, 0.9 * half_min)
    # departure spread: fairway ground at 150-250 m in >= 3 of 8 sectors
    spread = np.zeros(len(ys))
    for k in range(8):
        th = k * np.pi / 4.0
        py = (ys + np.sin(th) * 200.0 / cell).astype(int).clip(0, f.z8.shape[0] - 1)
        px = (xs + np.cos(th) * 200.0 / cell).astype(int).clip(0, f.z8.shape[1] - 1)
        spread += f.pad_fair[py, px]
    s_term = np.clip(spread / 3.0, 0, 1)
    drain = 1.0 - np.clip(f.subgrid_rough[ys, xs] / 0.8, 0, 1)
    padq = np.clip((cf.pad_room[ys, xs] - CH_PAD_MIN_M)
                   / (CH_PAD_GOOD_M - CH_PAD_MIN_M), 0, 1)
    # ROUND CLIMAX (owner, 2026-08-30): cheat the clubhouse slightly AWAY
    # from the richest green ground so the best sites are reached mid-round
    # rather than spent on holes 1 and 9. "Richest" at siting time = the
    # peak of the pad_green density smoothed at 300 m (the pool does not
    # exist yet); the term rewards distance from that peak, saturating at
    # half the window -- a nudge, not a banishment.
    dens = ndimage.uniform_filter(f.pad_green[i0:i0 + h, j0:j0 + w]
                                  .astype(float),
                                  max(int(300.0 / cell) | 1, 3))
    pk = np.unravel_index(int(np.argmax(dens)), dens.shape)
    d_rich = np.hypot(ys - (i0 + pk[0]), xs - (j0 + pk[1])) * cell
    climax = np.clip(d_rich / (0.5 * max(h, w) * cell), 0, 1)
    total = (p_term * 0.8 + e_term * 1.0 + s_term * 1.0 + drain * 0.4
             + padq * 0.8 + climax * 0.6)

    # total order
    order = np.lexsort((xs, ys, -total))

    def nearest(mask, y, x):
        my, mx = np.where(mask & cand_window_or_halo(f, i0, j0, h, w, halo))
        if len(my) == 0:
            my, mx = np.where(mask)
        d = np.hypot(my - y, mx - x)
        k = int(np.argmin(d))
        return (my[k] * cell, mx[k] * cell)

    # ALTERNATES (owner, 2026-08-30): what matters most is the ROUTING, not
    # the clubhouse -- so siting proposes up to three spatially distinct
    # clubhouse candidates (NMS 250 m) and the router auditions a route from
    # each, keeping the best. The primary (best soft score) still decides
    # the window pair score, so window choice is unchanged.
    picks = []
    for q in order:
        y, x = int(ys[q]), int(xs[q])
        if any(np.hypot(y - py, x - px) * cell < 250.0 for (py, px) in
               [(int(c.yx[0] / cell), int(c.yx[1] / cell)) for c in picks]):
            continue
        picks.append(Clubhouse((y * cell, x * cell), float(total[q]),
                               nearest(f.pad_green, y, x),
                               nearest(f.pad_tee, y, x)))
        if len(picks) >= 3:
            break
    ch = picks[0]
    ch.alternates = picks
    return ch


def cand_window_or_halo(f, i0, j0, h, w, halo):
    m = np.zeros(f.z8.shape, bool)
    m[max(0, i0 - halo):i0 + h + halo, max(0, j0 - halo):j0 + w + halo] = True
    return m


def loop_probe(f: Fields, ch: Clubhouse, hole_lengths: np.ndarray,
               sites: np.ndarray, beam: int = 6) -> int:
    """Max hole steps closable chaining through `sites` from the clubhouse,
    return within CLUBHOUSE_RADIUS_M on the last step. The joint coupling
    term.

    BEAM, NOT GREEDY (2026-08-30). The greedy version broke on the first
    dead step and reported the PREFIX length: one unreachable par-3 band at
    h=2 printed "2/9" even when steps 3-9 were trivial -- and the observed
    2-and-5 failure modes on aeolian seeds sat exactly before the two
    par-3s. A capacity question needs a search that survives a bad branch:
    per step, every legal extension of every beam state is generated and the
    top `beam` states are kept, ranked by (holes closed, -total band error).
    Deterministic: ties break on the site index path.
    """
    if len(sites) == 0:
        return 0
    home = np.array(ch.yx)
    # state: (closed, err, path_tuple, pos)
    states = [(0, 0.0, (), home)]
    best_closed = 0
    nh = len(hole_lengths)
    for h, L in enumerate(hole_lengths):
        nxt = []
        for (closed, err, path, pos) in states:
            if closed < h:          # this state already died at an earlier step
                continue
            d = np.hypot(sites[:, 0] - pos[0], sites[:, 1] - pos[1])
            okd = (d > 0.8 * L) & (d < 1.2 * L)
            if h == nh - 1:
                dh = np.hypot(sites[:, 0] - home[0], sites[:, 1] - home[1])
                okd &= dh <= CLUBHOUSE_RADIUS_M
            okd[list(path)] = False
            for k in np.flatnonzero(okd):
                nxt.append((closed + 1, err + abs(d[k] - L),
                            path + (int(k),), sites[k]))
        if not nxt:
            break
        nxt.sort(key=lambda s: (-s[0], s[1], s[2]))
        states = nxt[:beam]
        best_closed = max(best_closed, states[0][0])
    return best_closed


def preview_sites(f: Fields, i0: int, j0: int, h: int, w: int, cap: int = 80,
                  anchor_yx: tuple[float, float] | None = None) -> np.ndarray:
    """Green-site preview inside a window+halo, for the loop probe only.

    REBUILT 2026-08-30. The old preview required `pad_green` -- the
    r=16 m / 5% / 1.4 m gate that greens.py records as passing only 28% of
    real Nebraska greens -- ranked by `room` (which burned the 40-site cap
    inside one big interdune flat), thinned at 100 m against the real pool's
    40 m, and had no halo. The probe was scoring windows against a pool the
    router will never see, and reported 2-5/9 on aeolian seeds whose ground
    the corridor packer fits 9 holes into. This preview now approximates the
    REAL pool's gates at 8 m: the GRADED confirm ceiling (slope <= 0.08
    rise/run, local relief <= 2.2 m over a ~20 m disc, dry), flattest-first,
    60 m spacing, 120 m halo, and the clubhouse's reserved green appended so
    the return anchor the clubhouse gate guaranteed is actually reachable.
    """
    halo = int(round(120.0 / f.cell))
    a0, a1 = max(0, i0 - halo), min(f.z8.shape[0], i0 + h + halo)
    b0, b1 = max(0, j0 - halo), min(f.z8.shape[1], j0 + w + halo)
    sl = np.s_[a0:a1, b0:b1]
    k = int(round(20.0 / f.cell)) | 1
    relief = (ndimage.maximum_filter(f.z8[sl], k)
              - ndimage.minimum_filter(f.z8[sl], k))
    ok = (f.slope[sl] <= 0.08) & (relief <= 2.2) & ~f.wet8[sl]
    ys, xs = np.where(ok)
    kept: list[tuple[float, float]] = []
    if len(ys):
        order = np.lexsort((xs, ys, f.slope[sl][ys, xs]))
        occ = np.zeros(ok.shape, bool)
        sep = max(1, int(round(60.0 / f.cell)))
        for q in order:
            y, x = int(ys[q]), int(xs[q])
            if occ[y, x]:
                continue
            kept.append(((a0 + y) * f.cell, (b0 + x) * f.cell))
            if len(kept) >= cap:
                break
            occ[max(0, y - sep):y + sep + 1, max(0, x - sep):x + sep + 1] = True
    if anchor_yx is not None:
        kept.append((float(anchor_yx[0]), float(anchor_yx[1])))
    return np.array(kept) if kept else np.zeros((0, 2))


@dataclass
class Siting:
    window_ij: tuple[int, int]      # top-left, 8 m indices into the full grid
    window_m: tuple[float, float, float, float]   # (min_y, min_x, h, w) world metres
    clubhouse: Clubhouse
    pair_score: float
    shortlist: list                 # [(ij, win_score, ch_score, loop_closed)]


def run_siting(z2, cell2, wet2, play_long_m: float, play_short_m: float = None,
               hole_lengths=None, flow_accum8=None) -> tuple[Siting, Fields, Morphology, Persistence]:
    if play_short_m is None:
        play_short_m = play_long_m
    if hole_lengths is None:
        hole_lengths = np.array([350., 360., 160., 480., 370., 170., 355., 490., 365.])
    f = build_fields(z2, cell2, wet2, flow_accum8)
    m = build_morphology(f)
    p = build_persistence(f)
    scans = scan_windows(f, m, play_long_m, play_short_m)
    cf = build_clubhouse_fields(f)
    cands = shortlist(scans, f.cell)

    rows = []
    best = None
    for (oi, i, j) in cands:
        scan = scans[oi]
        gi, gj = scan.ij0[0] + i, scan.ij0[1] + j
        ch = site_clubhouse(f, (i, j), scan, cf)
        if ch is None:
            rows.append(((oi, i, j), float(scan.score[i, j]), None, 0))
            continue
        sites = preview_sites(f, gi, gj, scan.h, scan.w,
                              anchor_yx=ch.reserved_green)
        closed = loop_probe(f, ch, hole_lengths, sites)
        wscore = float(scan.score[i, j])
        # Loop-term floor 0.4 -> 0.25 (2026-08-30): with the honest probe an
        # unroutable window should lose real ground, but routing is the final
        # arbiter and is not built yet -- the probe steers, it does not veto.
        pair = wscore * (0.5 + ch.score / 4.0) * (0.25 + 0.75 * closed / 9.0)
        rows.append(((oi, i, j), wscore, ch.score, closed))
        if best is None or pair > best[0]:
            best = (pair, oi, (i, j), ch)
    if best is None:
        raise RuntimeError("no window admits a clubhouse")
    pair, oi, (i, j), ch = best
    scan = scans[oi]
    gi, gj = scan.ij0[0] + i, scan.ij0[1] + j
    sit = Siting((gi, gj),
                 (gi * f.cell, gj * f.cell, scan.h * f.cell, scan.w * f.cell),
                 ch, pair, rows)
    sit.scans = scans
    return sit, f, m, p
