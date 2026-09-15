//! S5 siting: the 8 m fields, the window scan, the clubhouse, the loop
//! probe — port of `tools/golf/siting.py` (see lib.rs).
//!
//! Design decisions carried from the prototype, with their reasons:
//!
//! * The window side is PER-BIOME (`dims_for`): measured 2026-08-28, a
//!   600 m window fits a 9-hole route on 2/12 aeolian seeds; 1200 m
//!   reaches 11/12.
//! * Calm is a FLOOR, not a maximand (d6w-siting-baseline: "otherwise
//!   generated courses will systematically sit on duller ground than real
//!   ones"). Banded terms are trapezoids over the accepted band; the
//!   maximand is the morphology mix and pad density.
//! * Pruned JOINT window/clubhouse search: exhaustive SAT scan, NMS, top
//!   ~30, clubhouse + loop-probe per survivor, best pair wins. Without NMS
//!   the top 30 are 30 translations of one window.
//! * The returning-nine constraint (`holes[0].tee` / `holes[8].green`
//!   within `CLUBHOUSE_RADIUS_M`) is a HARD GATE at clubhouse time —
//!   better siting refuses a clubhouse that cannot anchor a loop than
//!   routing discovers it after its whole budget.
//!
//! Units: slope is RISE/RUN everywhere in this file.
//!
//! Determinism: no random draws; every sort is stable with an index
//! tie-break; float keys are compared with `partial_cmp` (all finite) and
//! never used as an equality tie-break.

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::cmp::Reverse;

use crate::fields::{build_fields, py_round, FAIRWAY_SLOPE_RR};
use crate::geom::trapezoid;
use crate::img;
use crate::morph::build_morphology;
use crate::terrain::Terrain;
use crate::{Clubhouse, Fields, Mode, Morphology, Persistence, Siting, Yx};

// --- world geometry (docs/01-conventions.md) ------------------------------
pub const EXTENT_M: f64 = 3000.0;
pub const CORE_MIN_M: f64 = 750.0;
pub const CORE_MAX_M: f64 = 2250.0;
/// C3 Budgets fixture.
pub const CLUBHOUSE_RADIUS_M: f64 = 120.0;

// Accepted bands from d6w-siting-baseline (real courses, per-province
// p10-p90 where measured; sandhills row used as the aeolian default).
/// fraction of window at slope <= 0.08
pub const CALM_BAND: (f64, f64) = (0.32, 0.79);
/// window p95-p5 (window-scale re-measure)
pub const RELIEF_BAND_M: (f64, f64) = (8.0, 55.0);
/// relief_pos of the window mean
pub const POS_BAND: (f64, f64) = (0.25, 0.65);

// A clubhouse needs SOME flat dry ground, but a hard 60 m disc is
// over-gated: measured on aeolian seed 600009 it left ZERO legal cells on
// the whole tile (no 120 m circle under 8% slope exists in dune country --
// real clubhouse pads are graded). Floor at 25 m, reward up to 60.
pub const CH_PAD_MIN_M: f64 = 25.0;
pub const CH_PAD_GOOD_M: f64 = 60.0;

/// `run_siting`'s default `hole_lengths` (metres, nine holes).
pub const DEFAULT_HOLE_LENGTHS: [f64; 9] =
    [350., 360., 160., 480., 370., 170., 355., 490., 365.];

/// The per-archetype play window `(play_long_m, play_short_m)`: aeolian
/// 1450 x 950, fluvial 1000 x 700 (`routing_sheet.py`, `batch_route.py`).
pub fn dims_for(mode: Mode) -> (f64, f64) {
    match mode {
        Mode::Aeolian => (1450.0, 950.0),
        Mode::Fluvial => (1000.0, 700.0),
    }
}

/// The core's 8 m index range `[lo, hi)` on both axes.
pub fn core_range(cell: f64) -> (usize, usize) {
    (py_round(CORE_MIN_M / cell) as usize, py_round(CORE_MAX_M / cell) as usize)
}

/// Sub-array `a[y0..y1, x0..x1]` of a row-major grid, row-major.
fn sub<T: Copy>(a: &[T], nx: usize, y0: usize, y1: usize, x0: usize, x1: usize) -> Vec<T> {
    let mut out = Vec::with_capacity((y1 - y0) * (x1 - x0));
    for y in y0..y1 {
        out.extend_from_slice(&a[y * nx + x0..y * nx + x1]);
    }
    out
}

fn clip01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

// --------------------------------------------------------------------------
// P2 — persistence by priority-flood (pits AND peaks), with spill bearings

/// Total-ordered f64 heap key (all inputs finite).
#[derive(PartialEq)]
struct Ord64(f64);
impl Eq for Ord64 {}
impl PartialOrd for Ord64 {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Ord64 {
    fn cmp(&self, other: &Self) -> Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// `siting._flood_fill_level(z, cell)`: priority-flood fill level per cell
/// + bearing of the depression's spill.
///
/// `fill - z` is pit persistence: exactly the depth a punchbowl holds
/// before it pours over its lowest rim — threshold-free, amplitude-
/// invariant, and the only instrument that ranks hollows in a biome whose
/// flow accumulation is identically zero.
///
/// Ported rather than routed through `course_world::flow::fill_depressions`:
/// that one is D8 with an epsilon grade and no spill bearing; this is the
/// prototype's D4, ungraded, `heapq` on `(level, y, x)`, with the
/// Dijkstra-style relaxation that re-points `spill_dir` on each improvement.
pub fn flood_fill_level(z: &[f64], nx: usize, ny: usize) -> (Vec<f64>, Vec<f64>) {
    let n = nx * ny;
    let mut fill = vec![f64::INFINITY; n];
    let mut spill_dir = vec![0.0; n];         // bearing rad toward the outlet
    let mut seen = vec![false; n];
    // heapq tuple order (lvl, y, x); `+ 0.0` folds -0.0 into 0.0 so the
    // float key compares as Python's == does
    let mut pq: BinaryHeap<Reverse<(Ord64, usize, usize)>> = BinaryHeap::new();
    for x in 0..nx {
        for y in [0, ny - 1] {
            pq.push(Reverse((Ord64(z[y * nx + x] + 0.0), y, x)));
            fill[y * nx + x] = z[y * nx + x];
        }
    }
    for y in 0..ny {
        for x in [0, nx - 1] {
            pq.push(Reverse((Ord64(z[y * nx + x] + 0.0), y, x)));
            fill[y * nx + x] = z[y * nx + x];
        }
    }
    const D4: [(i64, i64); 4] = [(-1, 0), (1, 0), (0, -1), (0, 1)];
    while let Some(Reverse((Ord64(lvl), y, x))) = pq.pop() {
        let idx = y * nx + x;
        if seen[idx] {
            continue;
        }
        seen[idx] = true;
        for (dy, dx) in D4 {
            let j = y as i64 + dy;
            let i = x as i64 + dx;
            if j < 0 || i < 0 || j >= ny as i64 || i >= nx as i64 {
                continue;
            }
            let nidx = j as usize * nx + i as usize;
            if seen[nidx] {
                continue;
            }
            let nl = lvl.max(z[nidx]);
            if nl < fill[nidx] {
                fill[nidx] = nl;
                // water leaves toward the neighbour we flooded FROM
                spill_dir[nidx] = ((-dy) as f64).atan2((-dx) as f64);
                pq.push(Reverse((Ord64(nl + 0.0), j as usize, i as usize)));
            }
        }
    }
    (fill, spill_dir)
}

/// `siting.build_persistence(f)`. Core-only: siting never uses persistence
/// outside it, and the pure-Python flood is the prototype's slowest kernel
/// (fine in Rust). Zero outside the core.
pub fn build_persistence(f: &Fields) -> Persistence {
    let (lo, hi) = core_range(f.cell);
    let nc = hi - lo;
    let zc = sub(&f.z8, f.nx, lo, hi, lo, hi);
    let n = f.nx * f.ny;
    let mut pit = vec![0.0; n];
    let mut peak = vec![0.0; n];
    let mut spill = vec![0.0; n];
    let (fill, sp) = flood_fill_level(&zc, nc, nc);
    let zneg: Vec<f64> = zc.iter().map(|v| -v).collect();
    let (fill_inv, _) = flood_fill_level(&zneg, nc, nc);
    for y in 0..nc {
        for x in 0..nc {
            let c = y * nc + x;
            let g = (lo + y) * f.nx + lo + x;
            pit[g] = fill[c] - zc[c];
            peak[g] = fill_inv[c] + zc[c];
            spill[g] = sp[c];
        }
    }
    Persistence { pit, peak, spill_dir: spill }
}

// --------------------------------------------------------------------------
// P3 — SAT window scan

/// One orientation's window scan (`siting.WindowScan`). `score` and every
/// term are `ns_y x ns_x` row-major over top-left positions `(i, j)`
/// relative to the core origin `ij0`.
#[derive(Clone, Debug)]
pub struct WindowScan {
    /// per top-left position, 8 m grid
    pub score: Vec<f64>,
    /// positions along y (rows) and x (cols): `nc - h + 1`, `nc - w + 1`
    pub ns_y: usize,
    pub ns_x: usize,
    /// grid offset of position (0, 0)
    pub ij0: (usize, usize),
    /// window height, cells (y)
    pub h: usize,
    /// window width, cells (x)
    pub w: usize,
    // the `terms` dict
    pub calm: Vec<f64>,
    pub relief: Vec<f64>,
    pub pos: Vec<f64>,
    pub padg: Vec<f64>,
    pub banded: Vec<f64>,
}

impl WindowScan {
    /// `scan.score[i, j]`.
    pub fn score_at(&self, i: usize, j: usize) -> f64 {
        self.score[i * self.ns_x + j]
    }
}

/// `_win_mean(_sat(a), h, w)` over an `nc x nc` array: the mean of every
/// `h x w` window, output `(nc-h+1) x (nc-w+1)`.
fn win_means(a: &[f64], nc: usize, h: usize, w: usize) -> Vec<f64> {
    let s = img::sat(a, nc, nc);
    let ns_y = nc + 1 - h;
    let ns_x = nc + 1 - w;
    let mut out = Vec::with_capacity(ns_y * ns_x);
    for y0 in 0..ns_y {
        for x0 in 0..ns_x {
            out.push(img::win_mean(&s, nc, y0, x0, h, w));
        }
    }
    out
}

fn as_f64(m: &[bool]) -> Vec<f64> {
    m.iter().map(|b| if *b { 1.0 } else { 0.0 }).collect()
}

/// `siting.scan_windows_one(f, m, h_m, w_m)`: one orientation of the
/// per-biome rectangle. The window is a RECT, not a square: the three real
/// 9-hole courses measure 1517x487, 1222x304 and 1357x628 -- ribbons --
/// and a square can never contain them.
pub fn scan_windows_one(f: &Fields, m: &Morphology, h_m: f64, w_m: f64) -> WindowScan {
    let cell = f.cell;
    let h = py_round(h_m / cell) as usize;
    let w = py_round(w_m / cell) as usize;
    let (lo, hi) = core_range(cell);
    let nc = hi - lo;
    let core = |a: &[f64]| sub(a, f.nx, lo, hi, lo, hi);
    let core_b = |a: &[bool]| as_f64(&sub(a, f.nx, lo, hi, lo, hi));

    let z = core(&f.z8);
    let calm_in: Vec<f64> = core(&f.slope).iter()
        .map(|s| if *s <= FAIRWAY_SLOPE_RR { 1.0 } else { 0.0 }).collect();
    let calm = win_means(&calm_in, nc, h, w);
    let posm = win_means(&core(&f.relief_pos), nc, h, w);
    let padg = win_means(&core_b(&f.pad_green), nc, h, w);
    let padf = win_means(&core_b(&f.pad_fair), nc, h, w);
    let wat = win_means(&core_b(&f.wet8), nc, h, w);
    let rough = win_means(&core(&f.subgrid_rough), nc, h, w);
    let ns_y = nc + 1 - h;
    let ns_x = nc + 1 - w;
    // window relief via rect max/min filters (exact, cheap): the filter is
    // centred, so position (i, j) reads cell (i + h//2, j + w//2)
    let zmax = img::max_filter_rect(&z, nc, nc, h, w);
    let zmin = img::min_filter_rect(&z, nc, nc, h, w);
    let mut relief = Vec::with_capacity(ns_y * ns_x);
    for i in 0..ns_y {
        for j in 0..ns_x {
            let c = (i + h / 2) * nc + (j + w / 2);
            relief.push(zmax[c] - zmin[c]);
        }
    }

    let frac = |mask: Vec<bool>| win_means(&core_b(&mask), nc, h, w);
    let f_shoulder = frac(m.cls240.iter().map(|c| *c == 4).collect());
    let f_spur = frac(m.cls240.iter().map(|c| *c == 5).collect());
    let f_hollow = frac(m.cls80.iter().map(|c| *c == 7).collect());
    let f_saddle = frac(m.saddle.clone());

    let np = ns_y * ns_x;
    let mut banded = Vec::with_capacity(np);
    let mut score = Vec::with_capacity(np);
    for k in 0..np {
        let b = trapezoid(calm[k], CALM_BAND.0, CALM_BAND.1, 0.25, 0.4)
            * trapezoid(relief[k], RELIEF_BAND_M.0, RELIEF_BAND_M.1, 0.25, 0.4)
            * trapezoid(posm[k], POS_BAND.0, POS_BAND.1, 0.25, 0.4);
        // WATER IS AMENITY IN THE MEASURED BAND, not a defect (corpus
        // 2026-08-29). Softening the old -1.5*wat to -0.5 was NOT enough
        // and the measurement said so: with the softer penalty only 26% of
        // sited windows still held any water against 93% of real Carolina
        // courses, because water suppresses the score through THREE
        // channels, not one -- the explicit term plus pad_green and
        // pad_fair, both of which exclude wet cells. A pure penalty can
        // only ever push siting away.
        // So water gets a trapezoid, the same shape as the surround-relief
        // term and for the same reason ("floors not maximands", d6w): no
        // credit when dry (38% of real Nebraska courses carry none, so dry
        // must stay viable), full credit across the real in-course band
        // 0.5-4.5%, and a real penalty only past 12% where water starts
        // eating routable land.
        let wv = wat[k];
        let wat_t = if wv < 0.005 {
            0.0
        } else if wv <= 0.045 {
            0.45
        } else if wv <= 0.12 {
            0.45 - 0.45 * (wv - 0.045) / 0.075
        } else {
            -1.5 * clip01((wv - 0.12) / 0.10)
        };
        // Grounding: real Carolina sandhills courses carry a median 1.7%
        // water inside the course polygon (p75 3.5%) and 93% hold at least
        // one body, so a window with a couple of ponds is normal ground.
        // The fitted green score puts water proximity at ~zero
        // (d_water_band coef 0.0003) and greens ON water are excluded by a
        // hard gate elsewhere, so nothing downstream needs this term to be
        // defensive. Measured effect of the trapezoid: fluvial sited
        // windows holding any water 26% -> 76%, median 0.00% -> 0.60%.
        let character = 2.0 * padg[k] + 1.0 * padf[k]
            + 3.0 * (f_shoulder[k] + f_spur[k] + f_hollow[k] + 2.0 * f_saddle[k])
            + wat_t - 0.8 * clip01(rough[k] / 0.6);
        banded.push(b);
        score.push(b * (0.2 + character));
    }
    WindowScan {
        score, ns_y, ns_x, ij0: (lo, lo), h, w,
        calm, relief, pos: posm, padg, banded,
    }
}

/// `siting.scan_windows(f, m, long_m, short_m)`: both axis-aligned
/// orientations; S5 picks across them. Terrain grain is random per seed,
/// so two orientations recover most of what free rotation would have
/// bought -- rotation itself stays rejected (nesting ladder, deliverable
/// format). Landscape (h = short, w = long) first, then portrait.
pub fn scan_windows(f: &Fields, m: &Morphology, long_m: f64, short_m: f64) -> Vec<WindowScan> {
    if (long_m - short_m).abs() < 1e-9 {
        return vec![scan_windows_one(f, m, long_m, long_m)];
    }
    vec![scan_windows_one(f, m, short_m, long_m),   // landscape
         scan_windows_one(f, m, long_m, short_m)]   // portrait
}

/// `siting.shortlist(scans, cell, n=30, min_sep_m=200)`: top-n
/// `(orientation, i, j)` after NMS across BOTH orientations. Total order:
/// `(-score, orient, i, j)`. Per scan, the `4n` highest positions enter
/// (`argsort(stable)[::-1][:4n]`, so among equal scores the LATER index
/// enters first, exactly as the reversed stable argsort does).
pub fn shortlist(scans: &[WindowScan], cell: f64, n: usize, min_sep_m: f64)
    -> Vec<(usize, usize, usize)> {
    let mut entries: Vec<(f64, usize, usize, usize)> = Vec::new();
    for (oi, scan) in scans.iter().enumerate() {
        let s = &scan.score;
        let mut idx: Vec<usize> = (0..s.len()).collect();
        idx.sort_by(|&a, &b| s[a].partial_cmp(&s[b]).unwrap_or(Ordering::Equal));
        for &k in idx.iter().rev().take(4 * n) {
            let (i, j) = (k / scan.ns_x, k % scan.ns_x);
            if s[k].is_finite() {
                entries.push((s[k], oi, i, j));
            }
        }
    }
    entries.sort_by(|a, b| {
        b.0.partial_cmp(&a.0).unwrap_or(Ordering::Equal)
            .then(a.1.cmp(&b.1)).then(a.2.cmp(&b.2)).then(a.3.cmp(&b.3))
    });
    let mut kept: Vec<(usize, usize, usize)> = Vec::new();
    let mut centres: Vec<Yx> = Vec::new();
    for (_s, oi, i, j) in entries {
        let scan = &scans[oi];
        let cy = (i as f64 + scan.h as f64 / 2.0) * cell;
        let cx = (j as f64 + scan.w as f64 / 2.0) * cell;
        if centres.iter().any(|(a, b)| (cy - a).powi(2) + (cx - b).powi(2) < min_sep_m * min_sep_m) {
            continue;
        }
        kept.push((oi, i, j));
        centres.push((cy, cx));
        if kept.len() >= n {
            break;
        }
    }
    kept
}

// --------------------------------------------------------------------------
// P4 — clubhouse + joint score

/// `siting._disc(f, r_m)`'s radius in cells: `max(1, round(r_m / cell))`;
/// the footprint itself is `yy^2 + xx^2 <= n^2`, which is what the
/// `img::*_disc` kernels take.
pub fn disc_radius(cell: f64, r_m: f64) -> usize {
    py_round(r_m / cell).max(1) as usize
}

/// `siting.ClubhouseFields`, computed once, not once per shortlisted window.
#[derive(Clone, Debug)]
pub struct ClubhouseFields {
    /// EDT inside the buildable mask, m
    pub pad_room: Vec<f64>,
    pub near_green: Vec<bool>,
    pub near_tee: Vec<bool>,
    /// corpus-calibrated green-buildable mask
    pub green_grade: Vec<bool>,
}

/// `siting.build_clubhouse_fields(f)`.
pub fn build_clubhouse_fields(f: &Fields) -> ClubhouseFields {
    let n = f.nx * f.ny;
    let ok: Vec<bool> = (0..n).map(|i| f.slope[i] <= FAIRWAY_SLOPE_RR && !f.wet8[i]).collect();
    let pad_room = img::edt(&ok, f.nx, f.ny, f.cell);
    let rr = disc_radius(f.cell, CLUBHOUSE_RADIUS_M);
    // GREEN-GRADE, not pad_green (2026-08-30). pad_green is the r=16m/5%/
    // 1.4m gate that greens.py records as passing only 28% of real Nebraska
    // greens; the corpus-calibrated criterion is the GRADED confirm ceiling
    // (slope <= 0.08, relief <= 2.2 m over a ~20 m disc, dry). Using the
    // old gate here made the clubhouse's near_green hard gate
    // unsatisfiable on steep dune tiles -- seed 650084 (56 m relief) had
    // pad_green on 0.00% of its core and raised "no window admits a
    // clubhouse", the last hard veto in the pipeline. Same bug class as
    // the loop probe's preview pool.
    let k20 = (py_round(20.0 / f.cell) | 1) as usize;
    let zhi = img::max_filter_rect(&f.z8, f.nx, f.ny, k20, k20);
    let zlo = img::min_filter_rect(&f.z8, f.nx, f.ny, k20, k20);
    let green_grade: Vec<bool> = (0..n)
        .map(|i| f.slope[i] <= 0.08 && (zhi[i] - zlo[i]) <= 2.2 && !f.wet8[i]).collect();
    // maximum_filter over the disc footprint on a 0/1 raster = dilation
    let near_green: Vec<bool> = img::max_filter_disc(&as_f64(&green_grade), f.nx, f.ny, rr)
        .iter().map(|v| *v > 0.5).collect();
    let near_tee: Vec<bool> = img::max_filter_disc(&as_f64(&f.pad_tee), f.nx, f.ny, rr)
        .iter().map(|v| *v > 0.5).collect();
    ClubhouseFields { pad_room, near_green, near_tee, green_grade }
}

/// `siting.site_clubhouse(f, win_ij, scan, cf, pad_min_m=None)`: best
/// clubhouse for one window (`win_ij` relative to `scan.ij0`), or `None`
/// if no legal anchor exists.
///
/// Hard gates: a buildable dry pad (`CH_PAD_MIN_M`, or `pad_min_m`), and a
/// green-grade AND tee-grade pad within `CLUBHOUSE_RADIUS_M` — the
/// returning-nine anchor. Everything else is soft: prospect, edge affinity
/// (the CENTRE is penalised — a 3.1 km route in this window cannot spare
/// its middle), departure spread.
/// SOFT WINDOW (owner, 2026-09-14; routing round 1, item 3): the 250-seed
/// audit found holes hugging the window edge with an empty middle, and
/// the clubhouse's explicit edge affinity (`e_term`, weight 1.0, peaking
/// 0.5-0.9 x half-min from the centre), its 30 %-box central exclusion
/// and its 100 m outside halo all pushed the origin -- and the loop that
/// returns to it -- to the rim. Prototype values 1.0 / (0.35, 0.65) /
/// 100 m; now 0.3 / (0.42, 0.58) / 40 m: the loop still needs an origin
/// off dead centre, but not on the boundary.
pub const CH_EDGE_W: f64 = 0.3;
pub const CH_CENTRAL_BOX: (f64, f64) = (0.42, 0.58);
pub const CH_HALO_M: f64 = 40.0;

pub fn site_clubhouse(f: &Fields, win_ij: (usize, usize), scan: &WindowScan,
                      cf: &ClubhouseFields, pad_min_m: Option<f64>) -> Option<Clubhouse> {
    let cell = f.cell;
    let (nx, ny) = (f.nx, f.ny);
    let i0 = scan.ij0.0 + win_ij.0;
    let j0 = scan.ij0.1 + win_ij.1;
    let (h, w) = (scan.h, scan.w);
    // clubhouse may sit in the window or within CH_HALO_M outside it
    let halo = py_round(CH_HALO_M / cell) as usize;
    let a0 = i0.saturating_sub(halo);
    let a1 = (i0 + h + halo).min(ny);
    let b0 = j0.saturating_sub(halo);
    let b1 = (j0 + w + halo).min(nx);
    let pad_min = pad_min_m.unwrap_or(CH_PAD_MIN_M);

    // CENTRAL MASK (owner, 2026-08-30): a clubhouse deep in the middle of
    // the property makes a returning nine hard to fit -- the route must
    // thread around its own origin twice. The soft edge term was not
    // enough (it competed against prospect/pad quality); the central
    // 30%-box of the window is now excluded outright, with a soft fallback
    // if that empties the candidate set (never a hard failure on a cramped
    // window).
    let ci_lo = i0 + (CH_CENTRAL_BOX.0 * h as f64) as usize;
    let ci_hi = i0 + (CH_CENTRAL_BOX.1 * h as f64) as usize;
    let cj_lo = j0 + (CH_CENTRAL_BOX.0 * w as f64) as usize;
    let cj_hi = j0 + (CH_CENTRAL_BOX.1 * w as f64) as usize;
    let central = |y: usize, x: usize| y >= ci_lo && y < ci_hi && x >= cj_lo && x < cj_hi;

    // np.where(cand): row-major over the box
    let mut cand_all: Vec<(usize, usize)> = Vec::new();
    for y in a0..a1 {
        for x in b0..b1 {
            let i = y * nx + x;
            if cf.pad_room[i] >= pad_min && cf.near_green[i] && cf.near_tee[i] {
                cand_all.push((y, x));
            }
        }
    }
    let non_central: Vec<(usize, usize)> =
        cand_all.iter().copied().filter(|&(y, x)| !central(y, x)).collect();
    let cand = if !non_central.is_empty() { non_central } else { cand_all };
    if cand.is_empty() {
        return None;
    }

    let zwin = sub(&f.z8, nx, i0, i0 + h, j0, j0 + w);
    let zlo = zwin.iter().copied().fold(f64::INFINITY, f64::min);
    let zhi = zwin.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let rel = (zhi - zlo).max(1e-9);
    let cy = i0 as f64 + h as f64 / 2.0;
    let cx = j0 as f64 + w as f64 / 2.0;
    let half_min = 0.5 * (h.min(w) as f64) * cell;

    // ROUND CLIMAX (owner, 2026-08-30): cheat the clubhouse slightly AWAY
    // from the richest green ground so the best sites are reached mid-
    // round rather than spent on holes 1 and 9. "Richest" at siting time =
    // the peak of the pad_green density smoothed at 300 m (the pool does
    // not exist yet); the term rewards distance from that peak, saturating
    // at half the window -- a nudge, not a banishment.
    let pg_win = as_f64(&sub(&f.pad_green, nx, i0, i0 + h, j0, j0 + w));
    let ksz = (((300.0 / cell) as usize) | 1).max(3);
    let dens = img::uniform_filter(&pg_win, w, h, ksz);
    let mut pk = 0usize;
    for (q, v) in dens.iter().enumerate() {
        if *v > dens[pk] {          // np.argmax: first occurrence
            pk = q;
        }
    }
    let (pk_y, pk_x) = (pk / w, pk % w);
    let half_max = 0.5 * (h.max(w) as f64) * cell;

    let mut total = Vec::with_capacity(cand.len());
    for &(y, x) in &cand {
        let i = y * nx + x;
        let prospect = clip01((f.z8[i] - zlo) / rel);
        let p_term = trapezoid(prospect, 0.3, 0.8, 0.25, 0.4);
        let d_centre = (y as f64 - cy).hypot(x as f64 - cx) * cell;
        let e_term = trapezoid(d_centre, 0.5 * half_min, 0.9 * half_min, 0.25, 0.4);
        // departure spread: fairway ground at 150-250 m in >= 3 of 8 sectors
        let mut spread = 0.0;
        for k in 0..8 {
            let th = k as f64 * std::f64::consts::PI / 4.0;
            // astype(int) truncates toward zero, then clip
            let py = ((y as f64 + th.sin() * 200.0 / cell) as i64).clamp(0, ny as i64 - 1) as usize;
            let px = ((x as f64 + th.cos() * 200.0 / cell) as i64).clamp(0, nx as i64 - 1) as usize;
            if f.pad_fair[py * nx + px] {
                spread += 1.0;
            }
        }
        let s_term = clip01(spread / 3.0);
        let drain = 1.0 - clip01(f.subgrid_rough[i] / 0.8);
        let padq = clip01((cf.pad_room[i] - CH_PAD_MIN_M) / (CH_PAD_GOOD_M - CH_PAD_MIN_M));
        let d_rich = (y as f64 - (i0 + pk_y) as f64).hypot(x as f64 - (j0 + pk_x) as f64) * cell;
        let climax = clip01(d_rich / half_max);
        total.push(p_term * 0.8 + e_term * CH_EDGE_W + s_term * 1.0 + drain * 0.4
                   + padq * 0.8 + climax * 0.6);
    }

    // total order: np.lexsort((xs, ys, -total)) -- total desc, then (y, x),
    // which is the candidates' own (row-major) index order
    let mut order: Vec<usize> = (0..cand.len()).collect();
    order.sort_by(|&a, &b| total[b].partial_cmp(&total[a]).unwrap_or(Ordering::Equal)
                  .then(a.cmp(&b)));

    // `nearest(mask, y, x)`: nearest TRUE cell of `mask` inside the
    // window+halo box (`cand_window_or_halo`), else anywhere; first
    // occurrence in row-major order on ties. Both gates guarantee a hit.
    let nearest = |mask: &[bool], y: usize, x: usize| -> Yx {
        let mut best: Option<(f64, usize, usize)> = None;
        let scan_box = |y0: usize, y1: usize, x0: usize, x1: usize, best: &mut Option<(f64, usize, usize)>| {
            for my in y0..y1 {
                for mx in x0..x1 {
                    if !mask[my * nx + mx] {
                        continue;
                    }
                    let d = (my as f64 - y as f64).hypot(mx as f64 - x as f64);
                    if best.is_none_or(|b| d < b.0) {
                        *best = Some((d, my, mx));
                    }
                }
            }
        };
        scan_box(a0, a1, b0, b1, &mut best);
        if best.is_none() {
            scan_box(0, ny, 0, nx, &mut best);
        }
        match best {
            Some((_, my, mx)) => (my as f64 * cell, mx as f64 * cell),
            // unreachable: near_green / near_tee gated every candidate
            None => (y as f64 * cell, x as f64 * cell),
        }
    };

    // ALTERNATES (owner, 2026-08-30): what matters most is the ROUTING, not
    // the clubhouse -- so siting proposes up to three spatially distinct
    // clubhouse candidates (NMS 250 m) and the router auditions a route
    // from each, keeping the best. The primary (best soft score) still
    // decides the window pair score, so window choice is unchanged.
    let mut picks: Vec<Clubhouse> = Vec::new();
    let mut pick_cells: Vec<(usize, usize)> = Vec::new();
    for q in order {
        let (y, x) = cand[q];
        if pick_cells.iter().any(|&(py, px)| {
            (y as f64 - py as f64).hypot(x as f64 - px as f64) * cell < 250.0
        }) {
            continue;
        }
        picks.push(Clubhouse {
            yx: (y as f64 * cell, x as f64 * cell),
            score: total[q],
            reserved_green: nearest(&cf.green_grade, y, x),
            reserved_tee: nearest(&f.pad_tee, y, x),
            alternates: Vec::new(),
        });
        pick_cells.push((y, x));
        if picks.len() >= 3 {
            break;
        }
    }
    // the prototype sets `ch.alternates = picks` (picks[0] is ch itself);
    // here alternates[0] is a copy of the primary with empty alternates
    let mut ch = picks[0].clone();
    ch.alternates = picks;
    Some(ch)
}

/// One beam state of `loop_probe`.
#[derive(Clone, Debug)]
struct ProbeState {
    closed: usize,
    err: f64,
    path: Vec<usize>,
    pos: Yx,
}

/// `siting.loop_probe(f, ch, hole_lengths, sites, beam=6)`: max hole steps
/// closable chaining through `sites` from the clubhouse, return within
/// `CLUBHOUSE_RADIUS_M` on the last step. The joint coupling term.
///
/// BEAM, NOT GREEDY (2026-08-30). The greedy version broke on the first
/// dead step and reported the PREFIX length: one unreachable par-3 band
/// at h=2 printed "2/9" even when steps 3-9 were trivial -- and the
/// observed 2-and-5 failure modes on aeolian seeds sat exactly before the
/// two par-3s. A capacity question needs a search that survives a bad
/// branch: per step, every legal extension of every beam state is
/// generated and the top `beam` states are kept, ranked by (holes closed,
/// -total band error). Deterministic: ties break on the site index path.
pub fn loop_probe(ch: &Clubhouse, hole_lengths: &[f64], sites: &[Yx], beam: usize) -> usize {
    if sites.is_empty() {
        return 0;
    }
    let home = ch.yx;
    let mut states = vec![ProbeState { closed: 0, err: 0.0, path: Vec::new(), pos: home }];
    let mut best_closed = 0;
    let nh = hole_lengths.len();
    for (h, &l) in hole_lengths.iter().enumerate() {
        let mut nxt: Vec<ProbeState> = Vec::new();
        for st in &states {
            if st.closed < h {      // this state already died at an earlier step
                continue;
            }
            for (k, &(sy, sx)) in sites.iter().enumerate() {
                let d = (sy - st.pos.0).hypot(sx - st.pos.1);
                let mut okd = d > 0.8 * l && d < 1.2 * l;
                if h == nh - 1 {
                    let dh = (sy - home.0).hypot(sx - home.1);
                    okd = okd && dh <= CLUBHOUSE_RADIUS_M;
                }
                if st.path.contains(&k) {
                    okd = false;
                }
                if okd {
                    let mut path = st.path.clone();
                    path.push(k);
                    nxt.push(ProbeState {
                        closed: st.closed + 1,
                        err: st.err + (d - l).abs(),
                        path,
                        pos: (sy, sx),
                    });
                }
            }
        }
        if nxt.is_empty() {
            break;
        }
        // key (-closed, err, path): paths are distinct, so the order is total
        nxt.sort_by(|a, b| b.closed.cmp(&a.closed)
                    .then(a.err.partial_cmp(&b.err).unwrap_or(Ordering::Equal))
                    .then(a.path.cmp(&b.path)));
        nxt.truncate(beam);
        states = nxt;
        best_closed = best_closed.max(states[0].closed);
    }
    best_closed
}

/// `siting.preview_sites(f, i0, j0, h, w, cap=80, anchor_yx=None)`:
/// green-site preview inside a window+halo, for the loop probe only.
///
/// REBUILT 2026-08-30. The old preview required `pad_green` -- the
/// r=16 m / 5% / 1.4 m gate that greens.py records as passing only 28% of
/// real Nebraska greens -- ranked by `room` (which burned the 40-site cap
/// inside one big interdune flat), thinned at 100 m against the real
/// pool's 40 m, and had no halo. The probe was scoring windows against a
/// pool the router will never see, and reported 2-5/9 on aeolian seeds
/// whose ground the corridor packer fits 9 holes into. This preview now
/// approximates the REAL pool's gates at 8 m: the GRADED confirm ceiling
/// (slope <= 0.08 rise/run, local relief <= 2.2 m over a ~20 m disc,
/// dry), flattest-first, 60 m spacing, 120 m halo, and the clubhouse's
/// reserved green appended so the return anchor the clubhouse gate
/// guaranteed is actually reachable.
pub fn preview_sites(f: &Fields, i0: usize, j0: usize, h: usize, w: usize, cap: usize,
                     anchor_yx: Option<Yx>) -> Vec<Yx> {
    let cell = f.cell;
    let halo = py_round(120.0 / cell) as usize;
    let a0 = i0.saturating_sub(halo);
    let a1 = (i0 + h + halo).min(f.ny);
    let b0 = j0.saturating_sub(halo);
    let b1 = (j0 + w + halo).min(f.nx);
    let (nys, nxs) = (a1 - a0, b1 - b0);
    let k = (py_round(20.0 / cell) | 1) as usize;
    // the relief filters run on the SLICE (reflect at the slice's edges)
    let zs = sub(&f.z8, f.nx, a0, a1, b0, b1);
    let zhi = img::max_filter_rect(&zs, nxs, nys, k, k);
    let zlo = img::min_filter_rect(&zs, nxs, nys, k, k);
    let mut cells: Vec<(usize, usize, f64)> = Vec::new();   // (y, x, slope) local
    for y in 0..nys {
        for x in 0..nxs {
            let g = (a0 + y) * f.nx + b0 + x;
            let s = f.slope[g];
            if s <= 0.08 && (zhi[y * nxs + x] - zlo[y * nxs + x]) <= 2.2 && !f.wet8[g] {
                cells.push((y, x, s));
            }
        }
    }
    let mut kept: Vec<Yx> = Vec::new();
    if !cells.is_empty() {
        // np.lexsort((xs, ys, slope)): slope asc, then (y, x) = index order
        let mut order: Vec<usize> = (0..cells.len()).collect();
        order.sort_by(|&a, &b| cells[a].2.partial_cmp(&cells[b].2).unwrap_or(Ordering::Equal)
                      .then(a.cmp(&b)));
        let mut occ = vec![false; nys * nxs];
        let sep = py_round(60.0 / cell).max(1) as usize;
        for q in order {
            let (y, x, _) = cells[q];
            if occ[y * nxs + x] {
                continue;
            }
            kept.push(((a0 + y) as f64 * cell, (b0 + x) as f64 * cell));
            if kept.len() >= cap {
                break;
            }
            for yy in y.saturating_sub(sep)..(y + sep + 1).min(nys) {
                for xx in x.saturating_sub(sep)..(x + sep + 1).min(nxs) {
                    occ[yy * nxs + xx] = true;
                }
            }
        }
    }
    if let Some(a) = anchor_yx {
        kept.push(a);
    }
    kept
}

/// Loop-term floor 0.4 -> 0.25 (2026-08-30): with the honest probe an
/// unroutable window should lose real ground, but routing is the final
/// arbiter and is not built yet -- the probe steers, it does not veto.
fn pair_score(wscore: f64, ch_score: f64, closed: usize) -> f64 {
    wscore * (0.5 + ch_score / 4.0) * (0.25 + 0.75 * closed as f64 / 9.0)
}

/// `siting.run_siting(z2, cell2, wet2, play_long_m, play_short_m, hole_lengths)`.
/// `dims` is `(play_long_m, play_short_m)`; `hole_lengths` defaults to
/// `DEFAULT_HOLE_LENGTHS`.
///
/// `Siting.shortlist` rows carry the window's GLOBAL top-left `(gi, gj)`
/// (the prototype stored `(orient, i, j)`), and `ch_score` is NaN where
/// the prototype stored `None`.
///
/// Panics with "no window admits a clubhouse" only if every shortlisted
/// window still refuses a clubhouse at a 6 m pad floor (the prototype's
/// `RuntimeError`).
pub fn run_siting(t: &Terrain, dims: (f64, f64), hole_lengths: Option<&[f64]>)
    -> (Siting, Fields, Morphology, Persistence) {
    let (play_long_m, play_short_m) = dims;
    let hole_lengths: &[f64] = hole_lengths.unwrap_or(&DEFAULT_HOLE_LENGTHS);
    let f = build_fields(t);
    let m = build_morphology(&f);
    let p = build_persistence(&f);
    let scans = scan_windows(&f, &m, play_long_m, play_short_m);
    let cf = build_clubhouse_fields(&f);
    let cands = shortlist(&scans, f.cell, 30, 200.0);

    let mut rows: Vec<((usize, usize), f64, f64, usize)> = Vec::new();
    // (pair, oi, (i, j), ch)
    let mut best: Option<(f64, usize, (usize, usize), Clubhouse)> = None;
    for &(oi, i, j) in &cands {
        let scan = &scans[oi];
        let (gi, gj) = (scan.ij0.0 + i, scan.ij0.1 + j);
        let wscore = scan.score_at(i, j);
        let Some(ch) = site_clubhouse(&f, (i, j), scan, &cf, None) else {
            rows.push(((gi, gj), wscore, f64::NAN, 0));
            continue;
        };
        let sites = preview_sites(&f, gi, gj, scan.h, scan.w, 80, Some(ch.reserved_green));
        let closed = loop_probe(&ch, hole_lengths, &sites, 6);
        let pair = pair_score(wscore, ch.score, closed);
        rows.push(((gi, gj), wscore, ch.score, closed));
        if best.as_ref().is_none_or(|b| pair > b.0) {
            best = Some((pair, oi, (i, j), ch));
        }
    }
    if best.is_none() {
        // NO HARD VETO (stage-06: "a bad route is expensive, never
        // impossible"). If every shortlisted window refuses a clubhouse,
        // relax the pad-room floor progressively rather than failing the
        // seed -- a cramped tile gets a graded clubhouse, not no course.
        for floor_m in [18.0, 12.0, 6.0] {
            for &(oi, i, j) in &cands {
                let scan = &scans[oi];
                let Some(ch) = site_clubhouse(&f, (i, j), scan, &cf, Some(floor_m)) else {
                    continue;
                };
                let (gi2, gj2) = (scan.ij0.0 + i, scan.ij0.1 + j);
                let sites = preview_sites(&f, gi2, gj2, scan.h, scan.w, 80,
                                          Some(ch.reserved_green));
                let closed = loop_probe(&ch, hole_lengths, &sites, 6);
                let wscore = scan.score_at(i, j);
                let pair = pair_score(wscore, ch.score, closed);
                if best.as_ref().is_none_or(|b| pair > b.0) {
                    best = Some((pair, oi, (i, j), ch));
                }
            }
            if best.is_some() {
                break;
            }
        }
    }
    let (pair, oi, (i, j), ch) = best.expect("no window admits a clubhouse");
    let scan = &scans[oi];
    let (gi, gj) = (scan.ij0.0 + i, scan.ij0.1 + j);
    let sit = Siting {
        window_ij: (gi, gj),
        window_m: (gi as f64 * f.cell, gj as f64 * f.cell,
                   scan.h as f64 * f.cell, scan.w as f64 * f.cell),
        clubhouse: ch,
        pair_score: pair,
        shortlist: rows,
    };
    (sit, f, m, p)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scan_of(score: Vec<f64>, ns_y: usize, ns_x: usize, h: usize, w: usize) -> WindowScan {
        let n = score.len();
        WindowScan {
            score, ns_y, ns_x, ij0: (0, 0), h, w,
            calm: vec![0.0; n], relief: vec![0.0; n], pos: vec![0.0; n],
            padg: vec![0.0; n], banded: vec![0.0; n],
        }
    }

    #[test]
    fn flood_fill_pit_depth_and_spill_bearing() {
        // 5x5 bowl: rim at 10, one low rim cell (5) on the east edge of row 2,
        // centre at 0 -> fills to 5, spill points east.
        let nx = 5;
        let mut z = vec![10.0; 25];
        z[2 * nx + 2] = 0.0;
        z[2 * nx + 4] = 5.0;   // border cell, the outlet
        z[2 * nx + 3] = 3.0;   // between centre and outlet
        let (fill, sp) = flood_fill_level(&z, nx, 5);
        assert_eq!(fill[2 * nx + 2], 5.0);
        assert_eq!(fill[2 * nx + 3], 5.0);
        // the centre was flooded from (2,3): dx=+1 -> spill toward -dx = west?
        // No: spill_dir = atan2(-dy, -dx) with (dy, dx) the step FROM the
        // popped cell (2,3) TO (2,2), i.e. (0,-1), so atan2(0, 1) = 0 (east).
        assert_eq!(sp[2 * nx + 2], 0.0);
        // a border cell keeps its own level
        assert_eq!(fill[2 * nx + 4], 5.0);
        assert_eq!(fill[0], 10.0);
    }

    #[test]
    fn flood_fill_of_a_dome_is_identity() {
        let nx = 7;
        let mut z = vec![0.0; 49];
        for y in 0..7 {
            for x in 0..7 {
                z[y * nx + x] = -((y as f64 - 3.0).powi(2) + (x as f64 - 3.0).powi(2));
            }
        }
        let (fill, _) = flood_fill_level(&z, nx, 7);
        assert_eq!(fill, z);
    }

    #[test]
    fn shortlist_orders_by_score_then_orient_then_index_and_nms() {
        // two scans, 3 x 3 positions each, window 2 x 2 cells at cell 8 m:
        // position (i, j) has its centre at ((i + 1) * 8, (j + 1) * 8)
        let a = scan_of(vec![1.0, 5.0, 1.0,
                             1.0, 1.0, 1.0,
                             1.0, 1.0, 5.0], 3, 3, 2, 2);
        let b = scan_of(vec![5.0, 1.0, 1.0,
                             1.0, 1.0, 1.0,
                             1.0, 1.0, 1.0], 3, 3, 2, 2);
        // no NMS: the full total order (-score, orient, i, j)
        let k = shortlist(&[a.clone(), b.clone()], 8.0, 30, 0.0);
        let mut want = vec![(0, 0, 1), (0, 2, 2), (1, 0, 0)];
        for oi in 0..2 {
            for i in 0..3 {
                for j in 0..3 {
                    if !want.contains(&(oi, i, j)) {
                        want.push((oi, i, j));
                    }
                }
            }
        }
        assert_eq!(k, want);
        // NMS at 15 m (225 m^2): (1,0,0) at (8,8) is 64 from (0,0,1) at
        // (8,16) -> suppressed; (0,2,2) at (24,24) is 320 away -> kept;
        // of the score-1 ties only (0,2,0) at (24,8) clears both
        let k = shortlist(&[a.clone(), b.clone()], 8.0, 30, 15.0);
        assert_eq!(k, vec![(0, 0, 1), (0, 2, 2), (0, 2, 0)]);
        // n caps the list
        assert_eq!(shortlist(&[a, b], 8.0, 2, 0.0), vec![(0, 0, 1), (0, 2, 2)]);
    }

    fn ch_at(y: f64, x: f64) -> Clubhouse {
        Clubhouse { yx: (y, x), score: 0.0, reserved_green: (y, x), reserved_tee: (y, x),
                    alternates: Vec::new() }
    }

    #[test]
    fn loop_probe_par_arithmetic() {
        // a ring of sites 400 m apart around the clubhouse closes a 9-hole
        // loop of 400 m holes when the last site lands within 120 m of home
        let home = (1000.0, 1000.0);
        let ch = ch_at(home.0, home.1);
        let r = 400.0 / (2.0 * (std::f64::consts::PI / 9.0).sin());   // 9-gon side 400
        let sites: Vec<Yx> = (1..=9).map(|k| {
            let th = k as f64 * 2.0 * std::f64::consts::PI / 9.0;
            (home.0 + r * th.sin() - r * 0.0, home.1 + r * th.cos() - r)
        }).collect();
        // site 9 (th = 2 pi) sits at (home.y, home.x + r - r) = home exactly
        let lens = [400.0; 9];
        assert_eq!(loop_probe(&ch, &lens, &sites, 6), 9);
        // a 160 m par-3 band no site satisfies kills the chain at that step
        let lens3 = [400.0, 400.0, 160.0, 400.0, 400.0, 400.0, 400.0, 400.0, 400.0];
        assert_eq!(loop_probe(&ch, &lens3, &sites, 6), 2);
        // band edges are open: d must be strictly inside (0.8 L, 1.2 L)
        let one = vec![(1000.0, 1320.0)];
        assert_eq!(loop_probe(&ch, &[400.0], &one, 6), 0);   // 320 = 0.8 * 400 exactly
        let one = vec![(1000.0, 1100.0)];
        assert_eq!(loop_probe(&ch, &[100.0], &one, 6), 1);   // returns within 120 m
        assert_eq!(loop_probe(&ch, &[100.0], &[], 6), 0);
        // no site may be reused: two 400 m holes need two sites
        let two = vec![(1000.0, 1400.0)];
        assert_eq!(loop_probe(&ch, &[400.0, 400.0], &two, 6), 1);
    }

    #[test]
    fn loop_probe_beam_survives_a_bad_branch() {
        // Step 1 (L = 300, band 240..360) admits A (err 0) and B (err 30).
        // Step 2 (L = 280, band 224..336, must return within 120 m) admits
        // C only from B (230 m) -- from A it is 200 m, under the band. A
        // greedy / beam-1 probe keeps A and dies at 1; the beam-6 probe
        // carries B and closes 2.
        let ch = ch_at(0.0, 0.0);
        let sites = vec![(0.0, 300.0), (0.0, 330.0), (0.0, 100.0)];
        assert_eq!(loop_probe(&ch, &[300.0, 280.0], &sites, 1), 1);
        assert_eq!(loop_probe(&ch, &[300.0, 280.0], &sites, 6), 2);
    }

    #[test]
    fn pair_score_floor() {
        assert!((pair_score(1.0, 0.0, 0) - 0.5 * 0.25).abs() < 1e-12);
        assert!((pair_score(1.0, 4.0, 9) - 1.5 * 1.0).abs() < 1e-12);
    }

    #[test]
    fn core_range_matches_prototype() {
        assert_eq!(core_range(8.0), (94, 281));
        assert_eq!(disc_radius(8.0, CLUBHOUSE_RADIUS_M), 15);
    }

    #[test]
    #[ignore = "needs the img kernels (sat/win_mean, rect and disc filters, edt, uniform), still being filled by the img agent"]
    fn run_siting_end_to_end_on_a_synthetic_tile() {
        let t = crate::fields::tests::synthetic_terrain(true);
        let (sit, f, m, p) = run_siting(&t, dims_for(Mode::Aeolian), None);
        let (lo, hi) = core_range(f.cell);
        // the window lies inside the core, in one of the two orientations
        let (gi, gj) = sit.window_ij;
        let (_, _, hm, wm) = sit.window_m;
        let (h, w) = ((hm / f.cell) as usize, (wm / f.cell) as usize);
        assert!(gi >= lo && gj >= lo && gi + h <= hi && gj + w <= hi);
        assert!((h, w) == (119, 181) || (h, w) == (181, 119));
        // clubhouse within the window + 100 m halo, on gated ground
        let (cy, cx) = sit.clubhouse.yx;
        assert!(cy >= (gi as f64 - 12.0) * f.cell && cy < (gi + h + 12) as f64 * f.cell);
        assert!(cx >= (gj as f64 - 12.0) * f.cell && cx < (gj + w + 12) as f64 * f.cell);
        let (rg, rt) = (sit.clubhouse.reserved_green, sit.clubhouse.reserved_tee);
        assert!((rg.0 - cy).hypot(rg.1 - cx) <= CLUBHOUSE_RADIUS_M + f.cell);
        assert!((rt.0 - cy).hypot(rt.1 - cx) <= CLUBHOUSE_RADIUS_M + f.cell);
        assert!(!sit.clubhouse.alternates.is_empty() && sit.clubhouse.alternates.len() <= 3);
        assert!(sit.shortlist.len() <= 30 && !sit.shortlist.is_empty());
        assert!(sit.shortlist.iter().all(|r| r.3 <= 9));
        // morphology and persistence shapes; persistence zero outside the core
        assert_eq!(m.cls240.len(), f.nx * f.ny);
        assert_eq!(m.collar, 30);
        assert!(p.pit.iter().all(|v| *v >= 0.0) && p.peak.iter().all(|v| *v >= 0.0));
        assert_eq!(p.pit[0], 0.0);
        assert_eq!(p.peak[(lo - 1) * f.nx + lo], 0.0);
        // determinism: a second run is bit-identical
        let (sit2, _, _, _) = run_siting(&t, dims_for(Mode::Aeolian), None);
        assert_eq!(sit2.window_ij, sit.window_ij);
        assert_eq!(sit2.clubhouse.yx, sit.clubhouse.yx);
        assert_eq!(sit2.pair_score, sit.pair_score);
    }
}
