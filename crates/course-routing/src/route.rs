//! S6 routing — port of `tools/golf/routing.py`: the beam over green
//! orderings with router-chosen par, detail placement (tees, landing
//! zones, tee boxes, walks, bridges), and the exact rescore.
//!
//! THE SHOT MODEL (owner's design, 2026-08-30): only tees, landing zones and
//! greens need golf-ball-amicable ground (~8% grade); the terrain BETWEEN
//! shots may be wild -- carries over gullies, dunes, water. Par 3 =
//! tee->green; par 4 = tee->LZ->green; par 5 = tee->LZ1->LZ2->green, the
//! second LZ mattering on true three-shot holes. `playability.corridors` is
//! therefore NOT a hole gate here; the corridor idea survives only as walk
//! sanity.
//!
//! Stage-06 doctrine (docs/stages/stage-06-routing.md): greens-first,
//! SATURATING penalties -- a bad route is expensive, never impossible; the
//! only prunes are arithmetic impossibility (par mixes) and geometry (cannot
//! reach home). Fixed beam width x depth; total-order deterministic
//! tie-breaks; no RNG anywhere.
//!
//! Weights are hand-set and PROVISIONAL until the render-review round -- the
//! same status siting weights had before d6w calibration.
//!
//! PORT NOTES (2026-09-14). Deliberate deviations from `routing.py`, each
//! marked `DEVIATION` at the site:
//!  1. the beam's state sort replaces Python's `round(-score, 6)` float
//!     rounding tie-break with an explicit total order (score descending via
//!     `partial_cmp`, NaN last; then the green-index tuple; then the par
//!     tuple);
//!  2. the no-op loop at `routing.py:558-561` (which re-set `ok[gi] = True`
//!     on cells already true) is dropped;
//!  3. `terms["worst_clear"]` was assigned twice before the score sum and
//!     `"min_green_lz_m"` was excluded from a sum it was not yet in; here
//!     both diagnostics are added to the route terms AFTER the score is
//!     summed, so the exclusion is explicit rather than accidental;
//!  4. the joint tee x LZ retry carries an explicit iteration cap
//!     (`JOINT_RETRY_CAP`, equal to `place_tee`'s `n_options`);
//!  5. `TeeBox.axis` is the play-axis angle in radians (lib.rs) where the
//!     prototype stored the unit `(dy, dx)`.
//! There is no annealing pass in the prototype and none here.

use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::f64::consts::PI;

use rayon::prelude::*;

use crate::geom::{ch_intrusion, clearance_violation, point_to_polyline_m, polyline_crossings,
                  trapezoid};
use crate::img;
use crate::terrain::Terrain;
use crate::{Bridge, BridgeKind, Candidate, Clubhouse, Fields, Hole, Morphology, Persistence,
            Route, Siting, TeeBox, Walk, Yx, N_BEARINGS};

// --- pars, mixes, bands ----------------------------------------------------
// Allowed par-36 mixes as (n_par3, n_par4, n_par5), owner-specified.
pub const ALLOWED_MIXES: [(u8, u8, u8); 3] = [(2, 5, 2), (3, 3, 3), (1, 7, 1)];
// Hole-length bands in metres from the BACK tee. 120-230 / 300-470 / 480-610
// yards converted and trimmed to a walkable nine; the pool's distance-
// spectrum repair (greens.py) guarantees candidate pairs inside every band.
pub const PAR_BAND_3: (f64, f64) = (110.0, 210.0);
pub const PAR_BAND_4: (f64, f64) = (280.0, 430.0);
pub const PAR_BAND_5: (f64, f64) = (440.0, 560.0);
pub const TOTAL_BAND_M: (f64, f64) = (2600.0, 3200.0);

/// `PAR_BANDS[par]`.
pub fn par_band(par: u8) -> (f64, f64) {
    match par {
        3 => PAR_BAND_3,
        4 => PAR_BAND_4,
        _ => PAR_BAND_5,
    }
}

// Measured 2026-08-30 on 4,748 real green->next-tee walks (302 courses):
// length p25/50/75/90 = 49/67/97/142 m. The old free threshold (150 m) sat
// at the real p90+, and our routed medians drifted there. BACKTRACKING is
// rare in reality: the walk projected against the prior hole's closing
// direction is p50 0 m / p75 7 / p90 51, and only 13% of walks backtrack
// more than 40 m -- walks continue FORWARD (median angle 59 deg off the
// prior line of play).
pub const WALK_FREE_M: f64 = 70.0;       // real p50: costs ~nothing up to here
pub const WALK_SAT_M: f64 = 200.0;       // saturation point of the length penalty
pub const WALK_MAX_M: f64 = 300.0;       // beyond this the tee simply isn't placed (geometric)
pub const BACKTRACK_FREE_M: f64 = 10.0;
pub const BACKTRACK_SAT_M: f64 = 60.0;   // real p90 is 51 m
pub const DRIVE_R_M: (f64, f64) = (190.0, 250.0);     // tee -> LZ1 annulus
pub const SECOND_R_M: (f64, f64) = (160.0, 220.0);    // LZ1 -> LZ2 annulus (par 5)
// Real max-dogleg (9,248 hole lines): par4/5 p50 18-20 deg, p90 44-46, and
// 57-60% of holes bend >15 deg; the bend sits at t=0.63 (the LZ). Par 3s are
// dead straight (sinuosity p50 1.000). 45 deg = the real p90.
pub const DOGLEG_MAX_RAD: f64 = 45.0 * PI / 180.0;
pub const LZ_R_M: f64 = 22.0;            // target landing-zone radius
pub const LZ_ROOM_MIN_M: f64 = 15.0;     // tight-but-playable floor (penalized, never vetoed)
// Measured 2026-08-30 (196 corpus holes with >=2 mapped golf=tee): spread
// along the play axis is roughly CONSTANT across par classes -- p25/50/75 =
// 52/77/105 (par3), 48/69/93 (par4), 63/78/102 (par5). 75 m sits between the
// medians and p75; the old 57 m was short of every class median.
pub const TEE_STAGGER_M: [f64; 5] = [0.0, 18.0, 36.0, 54.0, 75.0];
pub const TEE_SIZE_M: f64 = 7.0;

pub const BEAM_W: usize = 48;
pub const EXPAND_TOP: usize = 20;
pub const N_FINAL: usize = 8;

/// `siting.CLUBHOUSE_RADIUS_M` (C3 Budgets fixture): holes 1 and 9 must
/// come within this of the clubhouse. Carried here so the router does not
/// depend on the siting port's symbol table.
pub const CLUBHOUSE_RADIUS_M: f64 = 120.0;

/// `siting.trapezoid` defaults (`ramp=0.25, tail=0.4`).
const TRAP_RAMP: f64 = 0.25;
const TRAP_TAIL: f64 = 0.4;

/// `place_tee` returns up to this many clean options and the joint tee x LZ
/// retry in `detail_route` tries at most this many (explicit cap; the
/// prototype's loop was bounded only by the option list's length).
pub const N_TEE_OPTIONS: usize = 6;
pub const JOINT_RETRY_CAP: usize = N_TEE_OPTIONS;

// --- longitudinal profile scoring (measured 2026-08-30) ---------------------
// 5,201 real holes on 307 corpus courses (OSM golf=hole lines sampled on the
// 2 m tiles, 15 m edge-replicated smoothing). The mean real hole is a shallow
// U: tee ~2-3 m above mid-hole, green climbing ~1-2 m at the end. Medians:
// net_dz -0.4 m (p5 -12.7, p25 -3.9, p75 +2.5, p95 +10.1); 44% downhill.
// Climb 1.24 m per 100 m (p90 3.35). THE HARD FACT: max height above the
// tee->green chord is p50 0.1 m, p90 1.7 m, p99 5.6 m -- real holes go
// AROUND high ground, they do not play over it; ground BELOW the chord
// (gully carries) is unconstrained, which is the shot model's whole point.
pub const PROF_NET_BAND: (f64, f64) = (-12.7, 10.1);  // p5-p95; trapezoid, ramp per siting rules
pub const PROF_CHORD_FREE_M: f64 = 1.7;               // p90: no penalty below
pub const PROF_CHORD_SAT_M: f64 = 5.6;                // p99: penalty saturates here
pub const PROF_CLIMB_100_FREE: f64 = 1.6;             // ~p60 climb per 100 m
pub const PROF_CLIMB_100_SAT: f64 = 3.4;              // p90

// --- line-of-play clearance (measured 2026-08-30) ---------------------------
// Real courses keep neighbouring lines of play apart, most strictly through
// the hole's MIDDLE: across 5,187 corpus holes with neighbours, the p10
// clearance to the nearest other line is ~50 m at t=0.3-0.7, tapering to
// ~30 m at the ends (t=0: 29 m, t=0.5: 50 m, t=1: 33 m) -- shared corridor
// mouths near tees/greens are normal, shared landing zones are not.
pub const CLEAR_END_M: f64 = 30.0;
pub const CLEAR_MID_M: f64 = 50.0;
pub const LZ_SEP_M: f64 = 50.0;          // landing zones of different holes stay this far apart
// A GREEN MUST NOT SIT IN ANOTHER HOLE'S LANDING ZONE (owner, 2026-08-30:
// seed 600125's par-3 3rd green sat on the 2nd's LZ -- an ideal drive would
// finish on the next green). Measured on 64,450 real (green, other-hole-LZ-
// region) pairs: p0.1 = 52 m, p1 = 81 m; only 0.08% of pairs fall under 50 m
// and 0.95% under 80 m. Real courses simply do not do this.
pub const GREEN_LZ_SEP_M: f64 = 80.0;
pub const LZ_APPROX_M: f64 = 220.0;      // where a drive finishes, for beam-time LZ estimates
// A GREEN MUST NOT SIT IN ANOTHER HOLE'S LINE OF PLAY (plan item 1,
// 2026-09-14). The 250-seed audit found 36 greens within 35 m of another
// hole's mid-line: `clearance_violation`'s consecutive exemption dropped
// the previous green's samples outright, and nothing else looked at a
// green against a spine. GREEN_CLEAR_M = CLEAR_MID_M, the measured
// mid-hole p10 clearance between neighbouring lines of play (5,187
// corpus holes): a green is a line-of-play object like any other and
// gets the same room. GREEN_VETO_M (owner): a green inside another
// hole's fairway width is a defect, not a penalty -- a hard tier in
// placement and a heavy flat charge in the scores. GREEN_JUNCTION_M: the
// successor's first 60 m of arc is the tee beside this green (walks p50
// 67 m), the shared corridor mouth -- the only exemption.
pub const GREEN_CLEAR_M: f64 = CLEAR_MID_M;
// GREEN_VETO_M 30 -> 35 (2026-09-15, after item 3's compaction): at 30 the
// placement tiers left every residual case at 30-35 m -- a successor's
// tee 70 m from the green playing past it 27 degrees off the line -- so
// the tier now sits at the acceptance's 35 m (half a 70 m corridor).
pub const GREEN_VETO_M: f64 = 35.0;
pub const GREEN_JUNCTION_M: f64 = 60.0;
/// Route / beam weights on `v = clip((GREEN_CLEAR_M - d) / GREEN_CLEAR_M, 0, 1)`:
/// `-GIP_W * v^2` and a flat veto charge under GREEN_VETO_M -- twice a
/// play crossing (6.0) at route level, 8 in the beam where the straight
/// tee->green line is an estimate. Calibration ladder (250 seeds,
/// 2026-09-15; cases = greens within 35 / 50 m of another hole's spine
/// outside the successor's first 60 m; "mid" = the audit's t 0.15-0.85
/// count, baseline 78 / 122, mid 36):
///   x1  (4.0)  ->  13 / 64, mid 6
///   x2  (8.0)  ->   7 / 57, mid 3   (shipped: the ladder's top rung,
///                                    acceptance mid <= 3 met)
///   x4 (16.0)  ->   4 / 42, mid 3   (off the ladder; same guards)
/// The residual at x2 is structural: five are hole 9's green beside hole
/// 1's tee (both pinned to the clubhouse disc, 30-34 m, arc 0-40 m) --
/// the loop-closure twin of the consecutive junction, which the exemption
/// does not cover; one is a successor at arc 65 m; one a tee 32 m from
/// the previous-but-one green. Every guard held on every rung.
pub const GIP_W: f64 = 16.0;

/// SOFT WINDOW, the router's side (owner, 2026-09-14; round 1, item 3):
/// holes may use the ground outside the window where it is good, but a
/// green is charged by how far it sits from the interior. `d` = the
/// green's signed distance to the window edge (positive inside);
/// `edge = -EDGE_W * clip((EDGE_FREE_M - d) / EDGE_RAMP_M, 0, 1)`: 0 at
/// >= 60 m inside, -0.3 on the line, -0.6 at >= 60 m outside. Beam cheap
/// score and detail `terms["edge"]`; the reserved loop anchor is exempt
/// (it sits where the clubhouse put it). 60 m = a green's own room plus
/// the corpus mid-hole clearance p10 (CLEAR_MID_M), so a green on the
/// line has no legal neighbour inside.
pub const EDGE_W: f64 = 0.6;

/// COVERAGE (owner, 2026-09-14; round 1, item 4): the window's ground
/// within COV_R_M of play. Beam: a bitset over a COV_CELL_M lattice on the
/// window, stamped by each new straight hole; `s_hole += COV_W_BEAM * new /
/// n_cells` -- a fresh 400 m hole in open ground earns ~+0.33 at 3.0, a
/// hole laid beside an earlier one or out in the halo nothing. Route:
/// `coverage` = the share of the window's 8 m cells within COV_R_M of any
/// placed spine, `rterms["coverage"] = COV_W_ROUTE * coverage`; the share
/// within 100 m is the audit's diagnostic. Ladder 2 / 3 / 4 on
/// COV_W_BEAM recorded in docs/calibration/routing-site-use.md. 120 m = a
/// hole's own corridor (CLEAR_MID_M) plus a par-4's lateral room.
pub const COV_CELL_M: f64 = 40.0;

/// GREEN SETTING (owner, 2026-09-14; round 1, item 5): the beam picked
/// greens in calm ground (surround relief p50 4.6 m vs the corpus's
/// 6.1-6.8 m; 12 % upland vs 25 %). `sett_raw = trapezoid(surround,
/// 3.5, 14, 0.25, 0.4) + 0.5 * clip((relief_pos - 0.35) / 0.30, 0, 1)`
/// (real greens: surround p25/p50/p75 2.9/4.9/7.9 m; relief_pos p25->p75
/// 0.31->0.67, the upland bonus spans that band) ranked within the pool
/// like the fit score (logits are uncalibrated, ranks are comparable);
/// beam cheap score and detail `terms["setting"]` at SETT_W. Ladder
/// 0.3 / 0.6 / 1.2 in docs/calibration/routing-site-use.md.
pub const SETT_W: f64 = 0.6;

/// Percentile rank of each value within the slice, stable-argsort based
/// (`pool_pct`'s body): 0 for the smallest, 1 for the largest.
fn rank01(vals: &[f64]) -> Vec<f64> {
    let n = vals.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| asc_nan_last(vals[i], vals[j]));
    let mut pct = vec![0.0; n];
    let den = (n.max(2) - 1) as f64;
    for (rank, &i) in order.iter().enumerate() {
        pct[i] = rank as f64 / den;
    }
    pct
}

/// Item 5: the setting rank of every pool candidate.
pub fn pool_setting(f: &Fields, pool: &[Candidate]) -> Vec<f64> {
    let raw: Vec<f64> = pool.iter().map(|c| {
        let k = cell_of(f, c.yx.0, c.yx.1);
        let (y, x) = (k / f.nx, k % f.nx);
        let sur = crate::greens::surround_relief(&f.z8, f.nx, f.ny, f.cell, y, x);
        trapezoid(sur, 3.5, 14.0, 0.25, 0.4) + 0.5 * clip((f.relief_pos[k] - 0.35) / 0.30, 0.0, 1.0)
    }).collect();
    rank01(&raw)
}
pub const COV_R_M: f64 = 120.0;
pub const COV_W_BEAM: f64 = 3.0;
pub const COV_W_ROUTE: f64 = 2.0;

/// Lattice dims `(rows, cols)` of the coverage bitset on `win`.
fn cov_dims(win: (f64, f64, f64, f64)) -> (usize, usize) {
    ((win.2 / COV_CELL_M).ceil() as usize, (win.3 / COV_CELL_M).ceil() as usize)
}

/// Stamp the lattice cells whose centre lies within COV_R_M of segment
/// `a`-`b` into `cov`; returns how many were newly set.
fn cov_stamp(cov: &mut [u64], win: (f64, f64, f64, f64), a: Yx, b: Yx) -> usize {
    let (nr, nc) = cov_dims(win);
    let mut new = 0;
    for r in 0..nr {
        let y = win.0 + (r as f64 + 0.5) * COV_CELL_M;
        for c in 0..nc {
            let x = win.1 + (c as f64 + 0.5) * COV_CELL_M;
            let (d, _) = point_to_polyline_m(&[a, b], (y, x));
            if d <= COV_R_M {
                let k = r * nc + c;
                let (w, bit) = (k / 64, 1u64 << (k % 64));
                if cov[w] & bit == 0 {
                    cov[w] |= bit;
                    new += 1;
                }
            }
        }
    }
    new
}

/// Share of the window's 8 m cells within `r_m` of any of `spines`.
fn coverage_share(f: &Fields, win: (f64, f64, f64, f64), spines: &[Vec<Yx>], r_m: f64) -> f64 {
    let i0 = (win.0 / f.cell).floor().max(0.0) as usize;
    let i1 = (((win.0 + win.2) / f.cell).ceil() as usize).min(f.ny);
    let j0 = (win.1 / f.cell).floor().max(0.0) as usize;
    let j1 = (((win.1 + win.3) / f.cell).ceil() as usize).min(f.nx);
    if i1 <= i0 || j1 <= j0 {
        return 0.0;
    }
    let hit: usize = (i0..i1).into_par_iter().map(|i| {
        let y = i as f64 * f.cell;
        let mut n = 0;
        for j in j0..j1 {
            let x = j as f64 * f.cell;
            if spines.iter().any(|sp| point_to_polyline_m(sp, (y, x)).0 <= r_m) {
                n += 1;
            }
        }
        n
    }).sum();
    hit as f64 / ((i1 - i0) * (j1 - j0)) as f64
}
pub const EDGE_FREE_M: f64 = 60.0;
pub const EDGE_RAMP_M: f64 = 120.0;

/// Signed distance from `p` to the window's edge, positive inside.
fn edge_distance(win: (f64, f64, f64, f64), p: Yx) -> f64 {
    let (wy, wx, h, w) = win;
    (p.0 - wy).min(wy + h - p.0).min(p.1 - wx).min(wx + w - p.1)
}

/// The edge term for a green at `p` (`reserved` greens exempt).
fn edge_term(win: (f64, f64, f64, f64), p: Yx, reserved: bool) -> f64 {
    if reserved {
        return 0.0;
    }
    -EDGE_W * clip((EDGE_FREE_M - edge_distance(win, p)) / EDGE_RAMP_M, 0.0, 1.0)
}
pub const GIP_VETO_ROUTE: f64 = 12.0;
pub const GIP_VETO_BEAM: f64 = 8.0;

// --- clubhouse keep-out ------------------------------------------------------
// Owner (2026-08-30): "the clubhouse sits right in front of the green
// locations" on some seeds. Holes 1 and 9 are REQUIRED to come within 120 m
// of the clubhouse, so proximity alone is not the crime -- a LINE OF PLAY
// passing over the clubhouse pad is. Keep-out disc CH_KEEPOUT_M (the pad
// gate is a 25-60 m buildable disc; 45 m covers it), measured against every
// spine with its endpoints trimmed 20 m so tee-1-beside-the-clubhouse and
// green-9-beside-the-clubhouse stay legal. Walks are exempt -- walking past
// the clubhouse is what a clubhouse is for.
pub const CH_KEEPOUT_M: f64 = 45.0;
/// `ch_intrusion`'s default `trim_m`.
pub const CH_TRIM_M: f64 = 20.0;
/// `polyline_crossings`' default `trim_m`.
pub const CROSS_TRIM_M: f64 = 8.0;

/// Length budget for the nine (`detail_route.BUDGET_M`). LENGTH BUDGET
/// (2026-08-30): the beam's pace term cannot bind because detail re-derives
/// lengths, and the clearance tier was buying spacing with band-top holes
/// on every seed (totals 3.5-3.6 km). Each hole's band cap now shrinks to
/// what the 3,100 m budget minus the remaining holes' band minimums allows
/// -- totals are bounded by construction.
pub const BUDGET_M: f64 = 3100.0;

// --- interest raster (plan item 6) -------------------------------------------
/// `surround` is computed on the play window plus this slab, 0 elsewhere.
pub const SURROUND_SLAB_M: f64 = 200.0;
/// `interest`'s surround trapezoid: free from 3.5 m, full to 14 m.
pub const INTEREST_SURROUND_BAND: (f64, f64) = (3.5, 14.0);
/// `interest40 = max_filter_disc(interest, 5)`: a feature within 40 m.
pub const INTEREST40_R_CELLS: usize = 5;

// --- data model ------------------------------------------------------------

/// `routing.RouteFields`.
#[derive(Clone, Debug)]
pub struct RouteFields {
    /// m of contiguous <=8%-and-dry ground
    pub lz_room: Vec<f64>,
    pub tee_ok: Vec<bool>,
    pub tee_relaxed: Vec<bool>,
    /// p90 - p10 of `z8` over the 25-90 m annulus (`greens::field_score`'s
    /// ring), on the window + `SURROUND_SLAB_M` slab; 0 outside it
    pub surround: Vec<f64>,
    /// [0, 1] "something to play against here": surround relief, |tpi200|,
    /// saddle / peak / pit; 0 on wet cells. NOT yet read by any score.
    pub interest: Vec<f64>,
    /// `interest` maxed over a 40 m disc; `place_lz` pays LZ_INTEREST_W for it.
    pub interest40: Vec<f64>,
}

/// The per-site context the router threads through the beam and detail
/// placement: the clubhouse being auditioned (`home`), the play window
/// `(min_y, min_x, h, w)` in metres, and the morphology.
#[derive(Clone, Copy, Debug)]
pub struct SiteCtx<'a> {
    pub home: Yx,
    pub win: (f64, f64, f64, f64),
    pub m: &'a Morphology,
}

/// `routing.build_route_fields(f)` plus the interest rasters: `surround`
/// on the window + 200 m slab, `interest` / `interest40` over the grid.
/// `win` is `Siting::window_m`.
pub fn build_route_fields(f: &Fields, m: &Morphology, p: &Persistence,
                          win: (f64, f64, f64, f64)) -> RouteFields {
    let lz_room = img::edt(&f.pad_fair, f.nx, f.ny, f.cell);
    let mut tee_relaxed = Vec::with_capacity(f.z8.len());
    for i in 0..f.z8.len() {
        tee_relaxed.push(f.slope[i] <= 0.10 && !f.wet8[i]);
    }
    let surround = surround_raster(f, win);
    let n = f.z8.len();
    let mut interest = vec![0.0; n];
    for i in 0..n {
        if f.wet8[i] {
            continue;
        }
        let saddle: f64 = if m.saddle[i] { 1.0 } else { 0.0 };
        let feat = saddle
            .max(clip(p.peak[i] / 4.0, 0.0, 1.0))
            .max(clip(p.pit[i] / 3.0, 0.0, 1.0));
        let v = 0.5 * trapezoid(surround[i], INTEREST_SURROUND_BAND.0, INTEREST_SURROUND_BAND.1,
                                TRAP_RAMP, TRAP_TAIL)
            + 0.3 * clip(f.tpi200[i].abs() / 1.5, 0.0, 1.0)
            + 0.2 * feat;
        interest[i] = clip(v, 0.0, 1.0);
    }
    let interest40 = img::max_filter_disc(&interest, f.nx, f.ny, INTEREST40_R_CELLS);
    RouteFields { lz_room, tee_ok: f.pad_tee.clone(), tee_relaxed, surround, interest, interest40 }
}

/// p90 - p10 of `z8` over the 25-90 m annulus, exactly `field_score`'s
/// ring (`rr0 = 3`, `rr1 = 11` cells at 8 m; `r_in` strictly between
/// `rr0^2 - 1` and `rr0^2`), evaluated on the window + `SURROUND_SLAB_M`
/// slab with an `rr1` margin so the slab's edge cells see real
/// neighbours. Full-grid array, 0 outside the slab.
fn surround_raster(f: &Fields, win: (f64, f64, f64, f64)) -> Vec<f64> {
    let (nx, ny) = (f.nx, f.ny);
    let mut out = vec![0.0; nx * ny];
    // window cells the way `greens::generate` derives them from the siting
    let i0 = (win.0 / f.cell).round_ties_even().max(0.0) as usize;
    let j0 = (win.1 / f.cell).round_ties_even().max(0.0) as usize;
    let h = (win.2 / f.cell).round_ties_even().max(0.0) as usize;
    let w = (win.3 / f.cell).round_ties_even().max(0.0) as usize;
    let slab = (SURROUND_SLAB_M / f.cell).round_ties_even() as usize;
    let a0 = i0.saturating_sub(slab).min(ny);
    let a1 = (i0 + h + slab).min(ny);
    let b0 = j0.saturating_sub(slab).min(nx);
    let b1 = (j0 + w + slab).min(nx);
    if a1 <= a0 || b1 <= b0 {
        return out;
    }
    let rr0 = (crate::greens::SURROUND_R0_M / f.cell).round_ties_even() as usize; // 3
    let rr1 = (crate::greens::SURROUND_R1_M / f.cell).round_ties_even() as usize; // 11
    let za0 = a0.saturating_sub(rr1);
    let za1 = (a1 + rr1).min(ny);
    let zb0 = b0.saturating_sub(rr1);
    let zb1 = (b1 + rr1).min(nx);
    let (zh, zw) = (za1 - za0, zb1 - zb0);
    let mut zs = Vec::with_capacity(zh * zw);
    for y in za0..za1 {
        zs.extend_from_slice(&f.z8[y * nx + zb0..y * nx + zb1]);
    }
    let r_in = ((rr0 * rr0) as f64 - 0.5).sqrt();
    let r_out = rr1 as f64;
    let p90 = img::percentile_filter_ring(&zs, zw, zh, r_in, r_out, 90.0);
    let p10 = img::percentile_filter_ring(&zs, zw, zh, r_in, r_out, 10.0);
    let oy = a0 - za0;
    let ox = b0 - zb0;
    for y in a0..a1 {
        for x in b0..b1 {
            let kz = (oy + y - a0) * zw + (ox + x - b0);
            out[y * nx + x] = p90[kz] - p10[kz];
        }
    }
    out
}

/// One back-tee option from `place_tee`: `(yx, score, graded)`.
#[derive(Clone, Copy, Debug)]
pub struct TeeOption {
    pub yx: Yx,
    pub score: f64,
    pub graded: bool,
}

/// One landing-zone pick from `place_lz`: `((y, x, r), score, clean)`.
#[derive(Clone, Copy, Debug)]
pub struct LzPick {
    pub lz: (f64, f64, f64),
    pub score: f64,
    pub clean: bool,
}

// --- small helpers ---------------------------------------------------------

fn hyp(a: Yx, b: Yx) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn clip(v: f64, lo: f64, hi: f64) -> f64 {
    if v < lo {
        lo
    } else if v > hi {
        hi
    } else {
        v
    }
}

/// `np.linspace(a, b, n)`: endpoint exact.
fn linspace(a: f64, b: f64, n: usize) -> Vec<f64> {
    let mut out = Vec::with_capacity(n);
    if n == 0 {
        return out;
    }
    if n == 1 {
        out.push(a);
        return out;
    }
    let step = (b - a) / (n - 1) as f64;
    for i in 0..n {
        out.push(a + step * i as f64);
    }
    out[n - 1] = b;
    out
}

/// `np.clip(int(v), 0, n - 1)`: truncation toward zero then clamp.
fn trunc_clip(v: f64, n: usize) -> usize {
    let i = v as i64;
    i.clamp(0, n as i64 - 1) as usize
}

fn cell_of(f: &Fields, y: f64, x: f64) -> usize {
    trunc_clip(y / f.cell, f.ny) * f.nx + trunc_clip(x / f.cell, f.nx)
}

/// Descending order on f64 with NaN last; never an equality test.
fn desc_nan_last(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => b.partial_cmp(&a).unwrap_or(Ordering::Equal),
    }
}

/// Ascending order on f64 with NaN last.
fn asc_nan_last(a: f64, b: f64) -> Ordering {
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a.partial_cmp(&b).unwrap_or(Ordering::Equal),
    }
}

/// Indices `0..n` stably sorted by `score` descending (index tie-break):
/// `np.lexsort((np.arange(n), -score))`.
fn order_desc(score: &[f64]) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..score.len()).collect();
    idx.sort_by(|&i, &j| desc_nan_last(score[i], score[j]));
    idx
}

/// `np.interp(s, xp, fp)` for monotone non-decreasing `xp`.
fn interp(s: f64, xp: &[f64], fp: &[f64]) -> f64 {
    let n = xp.len();
    if n == 1 || s <= xp[0] {
        return fp[0];
    }
    if s >= xp[n - 1] {
        return fp[n - 1];
    }
    let mut j = 0;
    while j + 1 < n - 1 && xp[j + 1] <= s {
        j += 1;
    }
    let dx = xp[j + 1] - xp[j];
    if dx <= 0.0 {
        return fp[j];
    }
    fp[j] + (s - xp[j]) * (fp[j + 1] - fp[j]) / dx
}

fn any_crossing(a: &[Yx], others: &[Vec<Yx>]) -> bool {
    for o in others {
        if !polyline_crossings(a, o, CROSS_TRIM_M).is_empty() {
            return true;
        }
    }
    false
}

fn any_walk_crossing(a: &[Yx], walks: &[[Yx; 2]]) -> bool {
    for w in walks {
        if !polyline_crossings(a, &w[..], CROSS_TRIM_M).is_empty() {
            return true;
        }
    }
    false
}

/// Plan item 1: how far `green` sits inside the spine `poly`'s line of
/// play, `(d, v)` with `v = clip((GREEN_CLEAR_M - d) / GREEN_CLEAR_M, 0, 1)`;
/// `None` when the closest point is the spine's start (the green is abeam
/// of or behind the tee), or when `junction` is set and it lies within
/// `GREEN_JUNCTION_M` of the start (the successor's tee beside this
/// green: the corridor mouth, not play over the green).
fn green_play(poly: &[Yx], green: Yx, junction: bool) -> Option<(f64, f64)> {
    let (d, s) = point_to_polyline_m(poly, green);
    // abeam of or behind the tee (the closest point is the spine's start)
    // is not in the line of play: nobody plays backwards. Added 2026-09-15
    // after item 3's compaction: 7 of 18 residual cases were greens
    // 33-34 m beside a non-adjacent hole's tee.
    if s <= 0.0 {
        return None;
    }
    if junction && s < GREEN_JUNCTION_M {
        return None;
    }
    Some((d, clip((GREEN_CLEAR_M - d) / GREEN_CLEAR_M, 0.0, 1.0)))
}

/// The worst green-in-play `v` of the legs `poly` against `avoid_greens`,
/// with `junction_idx` (an index into `avoid_greens`, the predecessor's
/// green) exempt within `GREEN_JUNCTION_M` of `poly`'s start; and whether
/// any green lies under `GREEN_VETO_M`.
fn greens_along(poly: &[Yx], avoid_greens: &[Yx], junction_idx: Option<usize>) -> (f64, bool) {
    let mut worst = 0.0_f64;
    let mut veto = false;
    for (k, &g2) in avoid_greens.iter().enumerate() {
        if let Some((d, v)) = green_play(poly, g2, junction_idx == Some(k)) {
            worst = worst.max(v);
            veto = veto || d < GREEN_VETO_M;
        }
    }
    (worst, veto)
}

// --- geometry kernels ------------------------------------------------------

/// `routing.wet_spans(wet2, cell2, a, b, kind, hole, step_m=2.0)`:
/// contiguous wet runs along segment a->b, as Bridge records.
pub fn wet_spans(t: &Terrain, a: Yx, b: Yx, kind: BridgeKind, hole: usize, step_m: f64)
    -> Vec<Bridge> {
    let cell2 = t.z2.spec.cell_size;
    let nx2 = t.z2.spec.nx as usize;
    let ny2 = t.z2.spec.ny as usize;
    let l = hyp(a, b);
    if l < step_m {
        return Vec::new();
    }
    let n = (l / step_m) as usize + 1;
    let ts = linspace(0.0, 1.0, n);
    let mut pts: Vec<Yx> = Vec::with_capacity(n);
    let mut w: Vec<bool> = Vec::with_capacity(n);
    for &s in &ts {
        let p = (a.0 + s * (b.0 - a.0), a.1 + s * (b.1 - a.1));
        let yi = trunc_clip(p.0 / cell2, ny2);
        let xi = trunc_clip(p.1 / cell2, nx2);
        pts.push(p);
        w.push(t.wet2[yi * nx2 + xi]);
    }
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        if w[i] {
            let mut j = i;
            while j + 1 < n && w[j + 1] {
                j += 1;
            }
            let span = (j - i + 1) as f64 * step_m;
            out.push(Bridge {
                a_yx: pts[i.saturating_sub(1)],
                b_yx: pts[(j + 1).min(n - 1)],
                span_m: span,
                kind,
                hole,
            });
            i = j + 1;
        } else {
            i += 1;
        }
    }
    out
}

// --- par-mix feasibility ---------------------------------------------------

/// Can `counts` (n3, n4, n5) still complete one of ALLOWED_MIXES with
/// `9 - holes_done` holes remaining?
pub fn mix_feasible(counts: (u8, u8, u8), holes_done: usize) -> bool {
    let left = 9 - holes_done as i32;
    for m in ALLOWED_MIXES {
        let need = (m.0 as i32 - counts.0 as i32, m.1 as i32 - counts.1 as i32,
                    m.2 as i32 - counts.2 as i32);
        if need.0 >= 0 && need.1 >= 0 && need.2 >= 0 && need.0 + need.1 + need.2 == left {
            return true;
        }
    }
    false
}

/// The pars (3/4/5, ascending) that keep a legal mix reachable.
pub fn legal_pars(counts: (u8, u8, u8), holes_done: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(3);
    for (p, k) in [(3u8, 0usize), (4, 1), (5, 2)] {
        let mut c = [counts.0, counts.1, counts.2];
        c[k] += 1;
        if mix_feasible((c[0], c[1], c[2]), holes_done + 1) {
            out.push(p);
        }
    }
    out
}

fn bump_counts(counts: (u8, u8, u8), par: u8) -> (u8, u8, u8) {
    match par {
        3 => (counts.0 + 1, counts.1, counts.2),
        4 => (counts.0, counts.1 + 1, counts.2),
        _ => (counts.0, counts.1, counts.2 + 1),
    }
}

// --- approach scoring ------------------------------------------------------

/// Bearing bin for an incoming play direction `v_in` (unit, (dy,dx)).
/// approach_table's bin k has the golfer standing along +u_k playing IN
/// along -u_k, so the stance side faces -v_in. Python's `round` is
/// half-to-even on the numpy float, hence `round_ties_even`.
pub fn approach_bin(v_in: Yx) -> usize {
    let th = (-v_in.0).atan2(-v_in.1).rem_euclid(2.0 * PI);
    let k = (th / (2.0 * PI / N_BEARINGS as f64)).round_ties_even() as i64;
    k.rem_euclid(N_BEARINGS as i64) as usize
}

/// Per-bin shot quality in ~[-0.3, 1]. Hand weights, deliberately NOT
/// FIT_COEF: the fitted coefficients answer "is this a green site", pooled
/// over all bearings with quadratic moderation -- not a per-bin shot
/// function. Ordering intuition kept: visibility and receptivity dominate.
pub fn score_approach(tab: &[[f64; 4]; N_BEARINGS], k: usize) -> f64 {
    let vis = tab[k][0];
    let rec = clip(tab[k][1], -0.05, 0.05) / 0.05;
    let back = clip(tab[k][2], 0.0, 4.0) / 4.0;
    let room = clip(tab[k][3], 0.0, 1.0);
    0.50 * vis + 0.30 * rec + 0.10 * back + 0.10 * room
}

// --- landing zones ---------------------------------------------------------

/// `routing._gather` at one point: `(lz_room, slope, d_water)`.
fn gather(rf: &RouteFields, f: &Fields, y: f64, x: f64) -> (f64, f64, f64, f64) {
    let i = cell_of(f, y, x);
    (rf.lz_room[i], f.slope[i], f.d_water[i], rf.interest40[i])
}

/// LANDING-ZONE SETTING (owner, 2026-09-14; round 1, item 6): `place_lz`
/// scored room 0.5, flatness 0.3 (linear from 0 % -- dead flat won), water
/// distance 0.1, remainder 0.4, dogleg -0.6: the flattest legal cell on
/// the annulus, every time (48 % of LZs under 2 %, setting std 0.18 within
/// a course). Now flatness is free to LZ_SLOPE_FREE (3 %) and 0 at the 8 %
/// fairway gate, the water trapezoid pays 0.3 for 15-60 m (a lateral
/// hazard in reach, item 8's band), and `interest40` -- a feature within
/// 40 m: surround relief, |tpi200|, a saddle / peak / pit -- earns
/// LZ_INTEREST_W. Gates and tiers untouched. Ladder 0.2 / 0.35 / 0.5 in
/// docs/calibration/routing-site-use.md.
pub const LZ_SLOPE_FREE: f64 = 0.03;
pub const LZ_WATER_BAND_M: (f64, f64) = (15.0, 60.0);
pub const LZ_INTEREST_W: f64 = 0.35;

/// Beam-time LZ existence probe: best room quality over a coarse grid of
/// plausible first-landing points. Cheap (9 gathers).
pub fn lz_probe(rf: &RouteFields, f: &Fields, tee_yx: Yx, green_yx: Yx, par: u8) -> f64 {
    if par == 3 {
        return 1.0;
    }
    let v = (green_yx.0 - tee_yx.0, green_yx.1 - tee_yx.1);
    let l = v.0.hypot(v.1);
    if l < 1e-6 {
        return 0.0;
    }
    let u = (v.0 / l, v.1 / l);
    let n = (-u.1, u.0);
    let mut best = 0.0_f64;
    for r in [DRIVE_R_M.0, (DRIVE_R_M.0 + DRIVE_R_M.1) / 2.0, DRIVE_R_M.1] {
        for off in [-30.0, 0.0, 30.0] {
            let d = r.min(0.75 * l);
            let p = (tee_yx.0 + u.0 * d + n.0 * off, tee_yx.1 + u.1 * d + n.1 * off);
            let (room, _, _, _) = gather(rf, f, p.0, p.1);
            best = best.max(clip(room / LZ_R_M, 0.0, 1.0));
        }
    }
    best
}

/// Best landing zone on an annulus around `from_yx`, bearing within
/// DOGLEG_MAX of the direct line to the green. 5 radii x 23 bearings
/// (`np.arange(-45, 46, 4)` degrees: -45, -41, ..., 43), flat index
/// `bearing * 5 + radius` as `np.meshgrid(radii, bears).ravel()` orders it.
/// `junction_idx` names the predecessor's green in `avoid_greens` (exempt
/// from the green veto within `GREEN_JUNCTION_M` of `from_yx`, the tee);
/// `None` when `from_yx` is not this hole's tee.
#[allow(clippy::too_many_arguments)]
pub fn place_lz(rf: &RouteFields, f: &Fields, from_yx: Yx, green_yx: Yx,
                r_band: (f64, f64), remainder_band: (f64, f64),
                avoid_spines: &[Vec<Yx>], avoid_walks: &[[Yx; 2]],
                avoid_lzs: &[Yx], avoid_greens: &[Yx], junction_idx: Option<usize>,
                ch_keepout_yx: Option<Yx>) -> Option<LzPick> {
    let a = from_yx;
    let g = green_yx;
    let v = (g.0 - a.0, g.1 - a.1);
    let d = v.0.hypot(v.1);
    if d < 1e-6 {
        return None;
    }
    let th0 = v.0.atan2(v.1);
    let radii = linspace(r_band.0, r_band.1, 5);
    let mut bears: Vec<f64> = Vec::with_capacity(23);
    let mut deg = -45i32;
    while deg < 46 {
        bears.push(th0 + (deg as f64) * PI / 180.0);
        deg += 4;
    }
    let n = bears.len() * radii.len();
    let mut ys = Vec::with_capacity(n);
    let mut xs = Vec::with_capacity(n);
    let mut room = Vec::with_capacity(n);
    let mut score = Vec::with_capacity(n);
    for &th in &bears {
        for &r in &radii {
            let y = a.0 + r * th.sin();
            let x = a.1 + r * th.cos();
            let (rm, slope, dwat, int40) = gather(rf, f, y, x);
            let room_q = clip(rm / LZ_R_M, 0.0, 1.0);
            let ok = rm >= LZ_ROOM_MIN_M;
            let rem = (g.0 - y).hypot(g.1 - x);
            let rem_t = trapezoid(rem, remainder_band.0, remainder_band.1, TRAP_RAMP, TRAP_TAIL);
            let dog = (th - th0).abs();
            // free to 15 deg, saturating at the real p90 of 45. First cut freed to
            // 20 deg at weight 0.3 and our routed median came out 32 deg vs the real
            // p50 of 18-20 -- the LZ room/flat rewards outbid a penalty that mild,
            // so the ramp starts at the real "most holes bend this little" point
            // and the weight doubles.
            let dog_pen = clip((dog * 180.0 / PI - 15.0) / 30.0, 0.0, 1.0);
            let mut sc = 0.5 * room_q
                + 0.3 * (1.0 - clip((slope - LZ_SLOPE_FREE) / (0.08 - LZ_SLOPE_FREE), 0.0, 1.0))
                + 0.3 * trapezoid(dwat, LZ_WATER_BAND_M.0, LZ_WATER_BAND_M.1, TRAP_RAMP, TRAP_TAIL)
                + 0.4 * rem_t - 0.6 * dog_pen
                + LZ_INTEREST_W * int40;
            if !ok {
                sc -= 1.0;     // tight LZ: penalized, never vetoed
            }
            ys.push(y);
            xs.push(x);
            room.push(rm);
            score.push(sc);
        }
    }
    let order = order_desc(&score);
    // tiered like place_tee: walk-crossing freedom is the hard rule, play
    // crossing freedom the strong preference, dirty last resort
    let mut pick = order[0];
    let mut pick_walk_clean: Option<usize> = None;
    let mut pick_play_clean: Option<usize> = None;
    let mut broke = false;
    for &k in &order {
        let legs: [Yx; 3] = [a, (ys[k], xs[k]), g];
        if any_walk_crossing(&legs, avoid_walks) {
            continue;
        }
        if pick_walk_clean.is_none() {
            pick_walk_clean = Some(k);
        }
        if any_crossing(&legs, avoid_spines) {
            continue;
        }
        if let Some(ch) = ch_keepout_yx {
            if ch_intrusion(&legs, ch, CH_TRIM_M) > 0.0 {
                continue;
            }
        }
        // GREEN VETO (plan item 1): neither leg passes within GREEN_VETO_M
        // of another hole's green -- part of the play-clean tier, so a
        // candidate that plays over a green ranks with one that crosses
        // a line of play; the predecessor's green is exempt over the
        // tee's first GREEN_JUNCTION_M (the walk junction)
        let (_, veto_in) = greens_along(&legs[..2], avoid_greens, junction_idx);
        let (_, veto_out) = greens_along(&legs[1..], avoid_greens, None);
        if veto_in || veto_out {
            continue;
        }
        if pick_play_clean.is_none() {
            pick_play_clean = Some(k);
        }
        // SHARED LZ AVOIDANCE (owner, 2026-08-30): landing zones of
        // different holes stay LZ_SEP_M apart -- the full-clean tier also
        // requires separation from every already-placed LZ
        let mut sep_ok = true;
        for &(ly, lx) in avoid_lzs {
            if (ys[k] - ly).hypot(xs[k] - lx) < LZ_SEP_M {
                sep_ok = false;
                break;
            }
        }
        if !sep_ok {
            continue;
        }
        // and never inside another hole's green (measured: real pairs are
        // under 80 m only 0.95% of the time)
        let mut green_ok = true;
        for &(gy2, gx2) in avoid_greens {
            if (ys[k] - gy2).hypot(xs[k] - gx2) < GREEN_LZ_SEP_M {
                green_ok = false;
                break;
            }
        }
        if green_ok {
            pick = k;
            broke = true;
            break;
        }
    }
    if !broke {
        pick = match (pick_play_clean, pick_walk_clean) {
            (Some(k), _) => k,
            (None, Some(k)) => k,
            (None, None) => pick,
        };
    }
    let (y, x) = (ys[pick], xs[pick]);
    let legs: [Yx; 3] = [a, (y, x), g];
    let clean = !any_crossing(&legs, avoid_spines) && !any_walk_crossing(&legs, avoid_walks);
    Some(LzPick { lz: (y, x, room[pick].min(LZ_R_M)), score: score[pick], clean })
}

// --- tees ------------------------------------------------------------------

/// Best back-tee cell: inside the walk disc of `prev_yx` (or the clubhouse
/// disc for hole 1), hole length in band, tee-grade ground preferred.
/// Returns up to `n_options` clean candidates (tiered), or the single
/// best-scored fallback; `None` when neither tee mask has a candidate.
/// `avoid_greens` are the other holes' greens, `junction_idx` the
/// predecessor's index in it (exempt within `GREEN_JUNCTION_M` of the tee).
#[allow(clippy::too_many_arguments)]
pub fn place_tee(rf: &RouteFields, f: &Fields, prev_yx: Yx, green_yx: Yx, par: u8,
                 clubhouse_yx: Option<Yx>, avoid_spines: &[Vec<Yx>], avoid_walks: &[[Yx; 2]],
                 avoid_greens: &[Yx], junction_idx: Option<usize>,
                 n_options: usize, hi_cap: Option<f64>, ch_keepout_yx: Option<Yx>)
    -> Option<Vec<TeeOption>> {
    let (lo, mut hi) = par_band(par);
    if let Some(cap) = hi_cap {
        hi = (lo + 20.0).max(hi.min(cap));
    }
    let centre = clubhouse_yx.unwrap_or(prev_yx);
    // the hole-1 disc keeps 14 m of margin: the box slide (+-10 m lateral)
    // and the 8 m cell quantization must not push the back box past 120 m
    let r_m = if clubhouse_yx.is_some() { CLUBHOUSE_RADIUS_M - 14.0 } else { WALK_MAX_M };
    let ci = (centre.0 / f.cell) as i64;
    let cj = (centre.1 / f.cell) as i64;
    let r = (r_m / f.cell) as i64 + 1;
    let i0 = (ci - r).max(0) as usize;
    let i1 = ((ci + r + 1).max(0) as usize).min(f.ny);
    let j0 = (cj - r).max(0) as usize;
    let j1 = ((cj + r + 1).max(0) as usize).min(f.nx);
    if i1 <= i0 || j1 <= j0 {
        return None;
    }
    let span = hi - lo;
    // tiers: the tee pad mask, the relaxed (graded) mask, then -- item 2,
    // 2026-09-15 -- any dry node under the slope cap, so the saturating
    // geometric fallback in detail_route (which cannot obey the cap; one
    // hole in 250 sat on a 62 % node) is reached only on a wet or wall-bound
    // disc
    for (tier, graded) in [(0, false), (1, true), (2, true)] {
        // candidates in row-major order, the lexsort's (ys, xs) tie-break
        let mut cand: Vec<(usize, usize)> = Vec::new();
        let mut sc: Vec<f64> = Vec::new();
        for i in i0..i1 {
            for j in j0..j1 {
                let idx = i * f.nx + j;
                let m = match tier {
                    0 => rf.tee_ok[idx],
                    1 => rf.tee_relaxed[idx],
                    _ => !f.wet8[idx],
                };
                // item 2: the pad masks read the 2 m ground; the back box
                // also obeys the node cap tee_boxes enforces
                if !m || f.slope[idx] > TEE_BOX_SLOPE_MAX {
                    continue;
                }
                let ym = i as f64 * f.cell;
                let xm = j as f64 * f.cell;
                let d_prev = (ym - centre.0).hypot(xm - centre.1);
                let d_green = (ym - green_yx.0).hypot(xm - green_yx.1);
                if !(d_prev <= r_m && d_green > lo && d_green < hi) {
                    continue;
                }
                let walk_t = 1.0 - clip((d_prev - WALK_FREE_M) / (WALK_SAT_M - WALK_FREE_M), 0.0, 1.0);
                let band_t = trapezoid(d_green, lo + 0.15 * span, hi - 0.15 * span, TRAP_RAMP, 0.05);
                let padq = 1.0 - clip(f.slope[idx] / 0.065, 0.0, 1.0);
                cand.push((i, j));
                sc.push(0.9 * walk_t + 1.1 * band_t + 0.5 * padq);
            }
        }
        if cand.is_empty() {
            continue;
        }
        let order = order_desc(&sc);
        // CROSSING AVOIDANCE (2026-08-30): the beam scores crossings on
        // ESTIMATED tees; detail placement moves them, so the final walk or
        // spine can cross a hole the beam thought was clear (measured: 10
        // walk-x-play crossings on the first gentle-tile test, then 1 that
        // survived every LZ option -- green 3 sat exactly ON the hole-6
        // tee->green line, so walk 4 fanned across the whole approach cone
        // and only a DIFFERENT TEE could fix it). Returns up to n_options
        // clean candidates so the caller can retry tee+LZ jointly; the last
        // entry is the best-scored fallback (possibly dirty, saturating).
        // TIERED cleanliness (2026-08-30). Walk-crossing freedom is the
        // owner's hard rule; play x play freedom is a strong preference
        // (~1% of courses). Demanding both at once starved the option list
        // and the fallback then violated the HARD rule -- measured: adding
        // the play x play test naively took the gentle tile from 0 to 5
        // walk crossings. Tier 1: fully clean. Tier 2: walk-clean only.
        // Tier 3 (last resort): best-scored, possibly dirty.
        let mut tier0: Vec<TeeOption> = Vec::new();
        let mut tier1: Vec<TeeOption> = Vec::new();
        let mut tier2: Vec<TeeOption> = Vec::new();
        let mut fallback: Option<TeeOption> = None;
        for &q in order.iter().take(200) {
            let (i, j) = cand[q];
            let cell_yx: Yx = (i as f64 * f.cell, j as f64 * f.cell);
            let entry = TeeOption { yx: cell_yx, score: sc[q], graded };
            if fallback.is_none() {
                fallback = Some(entry);
            }
            let walk_seg: [Yx; 2] = [prev_yx, cell_yx];
            let spine_seg: [Yx; 2] = [cell_yx, green_yx];
            let walk_bad = any_crossing(&walk_seg, avoid_spines)
                || any_walk_crossing(&spine_seg, avoid_walks);
            if walk_bad {
                continue;
            }
            let play_bad = any_crossing(&spine_seg, avoid_spines);
            if play_bad {
                if tier2.len() < n_options {
                    tier2.push(entry);
                }
                continue;
            }
            // clearance tier: keep >= ~2/3 of the measured radius profile.
            // A THRESHOLD inside the tier order, not a primary sort -- the
            // first attempt sorted options by clearance and starved hole 8
            // into a walk-crossing fallback (score 2.37, worst_clear 0.97).
            let mut viol = 0.0_f64;
            for sp in avoid_spines {
                viol = viol.max(clearance_violation(&spine_seg, sp, false));
            }
            if let Some(ch) = ch_keepout_yx {
                viol = viol.max(ch_intrusion(&spine_seg, ch, CH_TRIM_M));
            }
            // GREEN IN PLAY (plan item 1): the straight tee->green line
            // vs the other greens folds into the same threshold
            let (gv, _) = greens_along(&spine_seg, avoid_greens, junction_idx);
            viol = viol.max(gv);
            if viol <= 0.55 {
                tier0.push(entry);
                if tier0.len() >= n_options {
                    break;
                }
            } else if tier1.len() < n_options {
                tier1.push(entry);
            }
        }
        let opts = if !tier0.is_empty() {
            tier0
        } else if !tier1.is_empty() {
            tier1
        } else {
            tier2
        };
        if !opts.is_empty() {
            return Some(opts);
        }
        if let Some(fb) = fallback {
            return Some(vec![fb]);
        }
    }
    None
}

/// Item 2 (2026-09-15): no tee box on a cliff. TEE_BOX_SLOPE_MAX (owner,
/// provisional): a 7 m pad on 15 % ground benches without a wall. The 250
/// baseline had 39 boxes on > 30 % ground: the unchecked `(base, true)`
/// fallback when no cell of the +-10 m lateral scan passed the pad masks
/// (tee_ok <= 8 %, tee_relaxed <= 10 %), plus boxes whose floor cell was
/// gentle while the next 8 m node under the pad was a wall (z8 is a
/// stride sample, so a point's ground is the 2 x 2 nodes around it, not
/// one cell). Now: `box_slope` is the max over those four nodes, the scan
/// accepts a point only under the cap, runs at the stagger, then at 0.75x
/// and 0.5x of it, on the mask tier and then on a graded tier (any dry
/// point under the cap); failing all, a graded scan 3 m ahead of the
/// previous box; failing that, the previous box's own point (a coincident
/// pad; `length_m` stays non-increasing throughout).
pub const TEE_BOX_SLOPE_MAX: f64 = 0.15;
pub const TEE_STAGGER_RETRY: [f64; 3] = [1.0, 0.75, 0.5];
pub const TEE_FALLBACK_STEP_M: f64 = 3.0;
/// Radius of the capped-node search around detail_route's saturating tee
/// fallback point: 40 m keeps a par 3 inside its band either way.
pub const TEE_FALLBACK_SEARCH_M: f64 = 40.0;

/// The nearest dry 8 m node to `p` within `r_m` whose slope is under
/// TEE_BOX_SLOPE_MAX (ties: row-major), `None` when there is none.
fn nearest_capped_node(f: &Fields, p: Yx, r_m: f64) -> Option<Yx> {
    let r = (r_m / f.cell).ceil() as i64;
    let ci = (p.0 / f.cell).round() as i64;
    let cj = (p.1 / f.cell).round() as i64;
    let mut best: Option<(f64, Yx)> = None;
    for i in (ci - r).max(0)..=(ci + r).min(f.ny as i64 - 1) {
        for j in (cj - r).max(0)..=(cj + r).min(f.nx as i64 - 1) {
            let idx = i as usize * f.nx + j as usize;
            if f.wet8[idx] || f.slope[idx] > TEE_BOX_SLOPE_MAX {
                continue;
            }
            let q = (i as f64 * f.cell, j as f64 * f.cell);
            let d = hyp(q, p);
            if d > r_m {
                continue;
            }
            let better = match best {
                None => true,
                Some((bd, _)) => asc_nan_last(d, bd) == Ordering::Less,
            };
            if better {
                best = Some((d, q));
            }
        }
    }
    best.map(|(_, q)| q)
}

/// The steepest of the four 8 m nodes around `p` -- the ground a 7 m pad
/// at `p` actually sits on (`Fields::slope` is the strided node value).
fn box_slope(f: &Fields, p: Yx) -> f64 {
    // the nodes bracketing the point on each axis (one node when the
    // point sits on it: a pad centred on a node touches no other)
    let fy = clip(p.0 / f.cell, 0.0, (f.ny - 1) as f64);
    let fx = clip(p.1 / f.cell, 0.0, (f.nx - 1) as f64);
    let ys = [fy.floor() as usize, fy.ceil() as usize];
    let xs = [fx.floor() as usize, fx.ceil() as usize];
    let mut m = 0.0_f64;
    for &y in &ys {
        for &x in &xs {
            m = m.max(f.slope[y * f.nx + x]);
        }
    }
    m
}

/// Five boxes staggered along the play axis; forward boxes shorten the
/// hole. Lateral slide +-10 m by deterministic min-slope scan; the forward
/// three may sit on relaxed (graded) ground; see TEE_BOX_SLOPE_MAX for the
/// cap and the retries when the scan finds nothing.
pub fn tee_boxes(rf: &RouteFields, f: &Fields, back_yx: Yx, target_yx: Yx, green_yx: Yx)
    -> Vec<TeeBox> {
    let a = back_yx;
    let tv = (target_yx.0 - a.0, target_yx.1 - a.1);
    let tl = tv.0.hypot(tv.1).max(1e-9);
    let u = (tv.0 / tl, tv.1 / tl);
    let n = (-u.1, u.0);
    let g = green_yx;
    let mut out: Vec<TeeBox> = Vec::with_capacity(TEE_STAGGER_M.len());
    // one lateral scan around `base` on `tier` (0 = pad masks, 1 = graded);
    // (key = (slope, |lat|, lat), point, graded): lat is unique per step so
    // the key order is total without a float-equality test
    let scan = |base: Yx, tier: usize, bi: usize| -> Option<((f64, f64, f64), Yx, bool)> {
        let mut best: Option<((f64, f64, f64), Yx, bool)> = None;
        let mut li = 0;
        while li < 11 {
            let lat = -10.0 + 2.0 * li as f64;
            li += 1;
            let p = (base.0 + n.0 * lat, base.1 + n.1 * lat);
            let idx = cell_of(f, p.0, p.1);
            let sl = box_slope(f, p);
            let okm = if tier == 0 {
                if bi < 2 { rf.tee_ok[idx] } else { rf.tee_relaxed[idx] }
            } else {
                !f.wet8[idx]
            };
            if !okm || sl > TEE_BOX_SLOPE_MAX {
                continue;
            }
            let key = (sl, lat.abs(), lat);
            let better = match &best {
                None => true,
                Some((bk, _, _)) => asc_nan_last(key.0, bk.0)
                    .then_with(|| asc_nan_last(key.1, bk.1))
                    .then_with(|| asc_nan_last(key.2, bk.2)) == Ordering::Less,
            };
            if better {
                best = Some((key, p, !rf.tee_ok[idx]));
            }
        }
        best
    };
    for (bi, &off) in TEE_STAGGER_M.iter().enumerate() {
        let mut best: Option<((f64, f64, f64), Yx, bool)> = None;
        'search: for tier in 0..2 {
            for &scale in TEE_STAGGER_RETRY.iter() {
                let base = (a.0 + u.0 * off * scale, a.1 + u.1 * off * scale);
                best = scan(base, tier, bi);
                if best.is_some() {
                    break 'search;
                }
            }
        }
        if best.is_none() && bi > 0 {
            let prev = out[bi - 1].yx;
            let base = (prev.0 + u.0 * TEE_FALLBACK_STEP_M, prev.1 + u.1 * TEE_FALLBACK_STEP_M);
            best = scan(base, 1, bi);
        }
        let (p, graded) = match best {
            Some((_, p, gr)) => (p, gr),
            // the back tee itself for box 0 (place_tee vetted it), else a
            // pad coincident with the previous box
            None => (if bi == 0 { a } else { out[bi - 1].yx }, true),
        };
        out.push(TeeBox {
            yx: p,
            size_m: TEE_SIZE_M,
            axis: u.0.atan2(u.1),
            length_m: (g.0 - p.0).hypot(g.1 - p.1),
            graded,
            slope: box_slope(f, p),
        });
    }
    out
}

// --- beam search over (green sequence, par assignment) ---------------------

/// `routing._pool_arrays`: percentile rank of each candidate's score within
/// THIS seed's pool (fit logits are uncalibrated), stable-argsort based.
pub fn pool_pct(pool: &[Candidate]) -> Vec<f64> {
    let scores: Vec<f64> = pool.iter().map(|c| c.score).collect();
    rank01(&scores)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SegKind {
    Walk,
    Spine,
}

/// A beam state. The prototype's 10-tuple: (score, used, counts, pos, seq,
/// segs, cum, cum_mid, pdir, lzs_ap). `seq` entries are
/// `(green_idx, par, est_tee_yx)`.
///
/// state gains a running length so the beam feels PACE, not just bands:
/// without it the clearance term buys spacing with length and totals ran
/// 3.5-3.6 km (band-top holes all the way); target pace ~322 m/hole
/// state also carries approximate LZ points (par 4/5) so a GREEN is never
/// chosen on top of an earlier hole's landing zone -- greens come from the
/// beam and detail placement cannot move them.
#[derive(Clone, Debug)]
pub struct BeamState {
    pub score: f64,
    used: Vec<bool>,
    pub counts: (u8, u8, u8),
    pos: Yx,
    pub seq: Vec<(usize, u8, Yx)>,
    segs: Vec<(Yx, Yx, SegKind)>,
    cum: f64,
    cum_mid: f64,
    pdir: Option<Yx>,
    lzs_ap: Vec<Yx>,
    /// item 4: the COV_CELL_M lattice cells within COV_R_M of a spine so far
    cov: Vec<u64>,
}

/// DEVIATION (the one deliberate one in the beam): the prototype sorted
/// states by `(round(-score, 6), greens, pars)`, i.e. a float-rounding
/// tie-break. Here the order is total and explicit: score descending
/// (`partial_cmp`, NaN last), then the green-index tuple, then the par
/// tuple. Scores within 1e-6 that the prototype would have grouped and
/// ordered by green tuple are ordered by score here.
pub fn state_order(a: &BeamState, b: &BeamState) -> Ordering {
    desc_nan_last(a.score, b.score)
        .then_with(|| a.seq.iter().map(|e| e.0).cmp(b.seq.iter().map(|e| e.0)))
        .then_with(|| a.seq.iter().map(|e| e.1).cmp(b.seq.iter().map(|e| e.1)))
}

/// Expand one state at depth `h` into its successors, in the prototype's
/// generation order (par ascending, then the top-`EXPAND_TOP` greens by
/// cheap score). Pure in its inputs, so the beam may run these in parallel
/// and concatenate in state order.
#[allow(clippy::too_many_arguments)]
fn expand_state(f: &Fields, rf: &RouteFields, pool: &[Candidate], yx: &[Yx], pct: &[f64],
                sett: &[f64], site: &SiteCtx, st: &BeamState, h: usize) -> Vec<BeamState> {
    let home = site.home;
    let n = pool.len();
    let mut nxt = Vec::new();
    let pars = legal_pars(st.counts, h);
    let mut dist = Vec::with_capacity(n);
    let mut d_home = Vec::with_capacity(n);
    for i in 0..n {
        dist.push(hyp(yx[i], st.pos));
        d_home.push(hyp(yx[i], home));
    }
    for par in pars {
        let (lo, hi) = par_band(par);
        // home lookahead: remaining holes can still bring us back
        let left = (8 - h) as f64;
        let reach = PAR_BAND_5.1 * left + left * WALK_MAX_M;
        let mut idxs: Vec<usize> = Vec::new();
        let mut walk_est_all = Vec::with_capacity(n);
        for i in 0..n {
            let we = 0.0_f64.max((lo - dist[i]).max(dist[i] - hi));
            walk_est_all.push(we);
            let mut ok = we <= WALK_MAX_M;
            ok = ok && d_home[i] <= reach + hi + WALK_MAX_M;
            if h == 8 {
                ok = ok && d_home[i] <= CLUBHOUSE_RADIUS_M;
            }
            // DEVIATION: routing.py:558-561 re-set `ok[gi] = True` for
            // unused true cells -- a no-op -- dropped here.
            if ok && !st.used[i] {
                idxs.push(i);
            }
        }
        if idxs.is_empty() {
            continue;
        }
        // est tee: pull back from prev pos toward green if needed
        let m = idxs.len();
        let mut tee_est: Vec<Yx> = Vec::with_capacity(m);
        let mut hole_len: Vec<f64> = Vec::with_capacity(m);
        let mut cheap: Vec<f64> = Vec::with_capacity(m);
        let lo_i = lo + 0.15 * (hi - lo);
        let hi_i = hi - 0.15 * (hi - lo);
        let mid_par = (lo + hi) / 2.0;
        for &gi in &idxs {
            let v = (yx[gi].0 - st.pos.0, yx[gi].1 - st.pos.1);
            let dd = v.0.hypot(v.1).max(1e-9);
            let we = walk_est_all[gi];
            let tee = if we > 0.0 {
                let pull = if dist[gi] < lo { dist[gi] - lo } else { (dist[gi] - hi).max(0.0) };
                (st.pos.0 + v.0 / dd * pull, st.pos.1 + v.1 / dd * pull)
            } else {
                st.pos
            };
            let hl = hyp(yx[gi], tee);
            let pace = ((st.cum + hl) - (st.cum_mid + mid_par)).abs();
            let mut c = 1.5 * pct[gi]
                + SETT_W * sett[gi]
                + edge_term(site.win, yx[gi], pool[gi].reserved)
                + 0.9 * trapezoid(hl, lo_i, hi_i, TRAP_RAMP, 0.1)
                - 1.0 * clip(pace / 450.0, 0.0, 1.0)
                - 1.0 * clip((we - WALK_FREE_M) / (WALK_SAT_M - WALK_FREE_M), 0.0, 1.0)
                - 0.3 * clip(we / WALK_MAX_M, 0.0, 1.0);
            // back-to-back 3s/5s
            if let Some(last) = st.seq.last() {
                if (par == 3 || par == 5) && last.1 == par {
                    c -= 0.6;
                }
            }
            // MIX PRIOR, beam side (measured 2026-08-30): among 489 real
            // ref+par-tagged nines, par-36 mixes split (2,5,2) 94.2% /
            // (1,7,1) 3.1% / (3,3,3) 2.7%; our 1000-seed batch ran
            // (3,3,3) 75.4% -- par 3s dodge LZ/clearance costs and par 5s
            // buy length room, so the beam drifted to the extremes. Real
            // nines are 5/9 par-4s; nudge each par-4 pick.
            if par == 4 {
                c += 0.22;
            }
            tee_est.push(tee);
            hole_len.push(hl);
            cheap.push(c);
        }
        let order = order_desc(&cheap);
        let n_sp = st.segs.iter().filter(|s| s.2 == SegKind::Spine).count();
        for &q in order.iter().take(EXPAND_TOP) {
            let gi = idxs[q];
            let tee = tee_est[q];
            let gp = yx[gi];
            // approach quality from the estimated last-shot direction
            let from = if par == 3 {
                tee
            } else {
                (tee.0 + 0.6 * (gp.0 - tee.0), tee.1 + 0.6 * (gp.1 - tee.1))
            };
            let vin = (gp.0 - from.0, gp.1 - from.1);
            let nv = vin.0.hypot(vin.1);
            let ab = if nv > 1e-9 { approach_bin((vin.0 / nv, vin.1 / nv)) } else { 0 };
            let appr = score_approach(&pool[gi].approach, ab);
            let lzq = lz_probe(rf, f, tee, gp, par);
            let mut cov = st.cov.clone();
            let (nr, nc) = cov_dims(site.win);
            let new_cov = cov_stamp(&mut cov, site.win, tee, gp);
            let s_hole = cheap[q] + 1.0 * appr + 0.8 * lzq
                + COV_W_BEAM * new_cov as f64 / (nr * nc).max(1) as f64;
            // crossings vs accumulated straight segments
            let mut pen = 0.0_f64;
            let ns_arr: [Yx; 2] = [tee, gp];
            let nw_arr: [Yx; 2] = [st.pos, tee];
            let mut sp_i = 0usize;
            for &(a2, b2, kind2) in &st.segs {
                let seg: [Yx; 2] = [a2, b2];
                if kind2 == SegKind::Spine
                    && !polyline_crossings(&nw_arr, &seg, CROSS_TRIM_M).is_empty() {
                    pen -= 4.0;
                }
                if kind2 == SegKind::Spine {
                    if !polyline_crossings(&ns_arr, &seg, CROSS_TRIM_M).is_empty() {
                        pen -= 6.0;
                    }
                    // LINE-OF-PLAY CLEARANCE in the beam (2026-08-30):
                    // detail placement alone could not fix bunching --
                    // every final of a bunched SEQUENCE bunches, so the
                    // sequence itself must pay early. Straight-segment
                    // approximation of the measured radii; the
                    // consecutive junction (prev green = this walk's
                    // origin) is exempt as in the exact term.
                    sp_i += 1;
                    let v = clearance_violation(&ns_arr, &seg, sp_i == n_sp);
                    pen -= 2.0 * v * v;
                }
                if kind2 == SegKind::Walk
                    && !polyline_crossings(&ns_arr, &seg, CROSS_TRIM_M).is_empty() {
                    pen -= 4.0;
                }
            }
            let chi = ch_intrusion(&ns_arr, home, CH_TRIM_M);
            pen -= 2.0 * chi * chi;
            // GREEN IN PLAY, beam side (plan item 1): this hole's straight
            // line vs every earlier green (the predecessor exempt within
            // GREEN_JUNCTION_M of this tee), and this green vs every
            // earlier straight spine (no exemption: those END at their
            // greens). The exact term charges the route again.
            let n_seq = st.seq.len();
            for (k, e) in st.seq.iter().enumerate() {
                if let Some((d, v)) = green_play(&ns_arr, yx[e.0], k + 1 == n_seq) {
                    pen -= GIP_W * v * v;
                    if d < GREEN_VETO_M {
                        pen -= GIP_VETO_BEAM;
                    }
                }
            }
            // The loop junction (2026-09-15): the ninth green and the first
            // tee are both pinned to the clubhouse disc, so hole 9's green
            // beside hole 1's first GREEN_JUNCTION_M is the same corridor
            // mouth as the consecutive junction, and exempt the same way.
            let mut sp_k = 0usize;
            for &(a2, b2, kind2) in &st.segs {
                if kind2 != SegKind::Spine {
                    continue;
                }
                sp_k += 1;
                let loop_junction = h == 8 && sp_k == 1;
                if let Some((d, v)) = green_play(&[a2, b2], gp, loop_junction) {
                    pen -= GIP_W * v * v;
                    if d < GREEN_VETO_M {
                        pen -= GIP_VETO_BEAM;
                    }
                }
            }
            // this green vs earlier holes' approximate LZs
            for &(ly, lx) in &st.lzs_ap {
                let d_gl = (gp.0 - ly).hypot(gp.1 - lx);
                if d_gl < GREEN_LZ_SEP_M {
                    pen -= 5.0 * (1.0 - d_gl / GREEN_LZ_SEP_M);
                }
            }
            // this hole's approximate LZ vs earlier greens
            let mut new_lz: Option<Yx> = None;
            if par >= 4 {
                let vv = (gp.0 - tee.0, gp.1 - tee.1);
                let nv2 = vv.0.hypot(vv.1);
                if nv2 > 1e-9 {
                    let s = LZ_APPROX_M.min(0.6 * nv2);
                    let q2 = (tee.0 + vv.0 / nv2 * s, tee.1 + vv.1 / nv2 * s);
                    new_lz = Some(q2);
                    for e in &st.seq {
                        let g2 = yx[e.0];
                        let d_lg = (q2.0 - g2.0).hypot(q2.1 - g2.1);
                        if d_lg < GREEN_LZ_SEP_M {
                            pen -= 5.0 * (1.0 - d_lg / GREEN_LZ_SEP_M);
                        }
                    }
                }
            }
            if let Some(pd) = st.pdir {
                let wv = (tee.0 - st.pos.0, tee.1 - st.pos.1);
                let bt = 0.0_f64.max(-(pd.0 * wv.0 + pd.1 * wv.1));
                pen -= 0.7 * 1.0_f64.min(0.0_f64.max(
                    (bt - BACKTRACK_FREE_M) / (BACKTRACK_SAT_M - BACKTRACK_FREE_M)));
            }
            let mut used = st.used.clone();
            used[gi] = true;
            let mut seq = st.seq.clone();
            seq.push((gi, par, tee));
            let mut segs = st.segs.clone();
            segs.push((st.pos, tee, SegKind::Walk));
            segs.push((tee, gp, SegKind::Spine));
            let mut lzs_ap = st.lzs_ap.clone();
            if let Some(q2) = new_lz {
                lzs_ap.push(q2);
            }
            let dv = (gp.0 - tee.0, gp.1 - tee.1);
            let dn = dv.0.hypot(dv.1).max(1e-9);
            nxt.push(BeamState {
                score: st.score + s_hole + pen,
                used,
                counts: bump_counts(st.counts, par),
                pos: gp,
                seq,
                segs,
                cum: st.cum + hole_len[q],
                cum_mid: st.cum_mid + mid_par,
                pdir: Some((dv.0 / dn, dv.1 / dn)),
                lzs_ap,
                cov,
            });
        }
    }
    nxt
}

/// The router. Beam over green sequences with router-chosen par
/// (`routing.beam_route`); `site.home` is the clubhouse being auditioned.
pub fn beam_route(t: &Terrain, f: &Fields, rf: &RouteFields, site: &SiteCtx, pool: &[Candidate])
    -> Option<Route> {
    if pool.is_empty() {
        return None;
    }
    let home = site.home;
    let yx: Vec<Yx> = pool.iter().map(|c| c.yx).collect();
    let pct = pool_pct(pool);
    let sett = pool_setting(f, pool);
    let n = pool.len();
    let mut states = vec![BeamState {
        score: 0.0,
        used: vec![false; n],
        counts: (0, 0, 0),
        pos: home,
        seq: Vec::new(),
        segs: Vec::new(),
        cum: 0.0,
        cum_mid: 0.0,
        pdir: None,
        lzs_ap: Vec::new(),
        cov: { let (nr, nc) = cov_dims(site.win); vec![0u64; (nr * nc + 63) / 64] },
    }];
    for h in 0..9 {
        // each state's expansion is independent; rayon computes them and
        // the ordered collect concatenates in state order, so the result is
        // identical to the serial loop
        let per_state: Vec<Vec<BeamState>> = states
            .par_iter()
            .map(|st| expand_state(f, rf, pool, &yx, &pct, &sett, site, st, h))
            .collect();
        let mut nxt: Vec<BeamState> = Vec::new();
        for v in per_state {
            nxt.extend(v);
        }
        if nxt.is_empty() {
            return None;
        }
        nxt.sort_by(state_order);      // stable; see `state_order`
        nxt.truncate(BEAM_W);
        states = nxt;
    }

    let mut finals: Vec<&BeamState> = states.iter().take(N_FINAL).collect();
    // (2,5,2) ALWAYS AUDITIONS (2026-08-30): the route-level mix prior can
    // only rerank the finals, and on some seeds no (2,5,2) state survived
    // the beam -- pushing the prior weight further just shuffled the
    // minority between (3,3,3) and (1,7,1) (calibration ladder in
    // detail_route). If the finals carry no (2,5,2) state, the best one
    // from the full last generation joins them.
    if !finals.iter().any(|st| st.counts == (2, 5, 2)) {
        for st in &states {
            if st.counts == (2, 5, 2) {
                finals.push(st);
                break;
            }
        }
    }
    let mut best: Option<Route> = None;
    for st in finals {
        if let Some(r) = detail_route(t, f, rf, site, pool, &st.seq) {
            let better = match &best {
                None => true,
                Some(b) => r.score > b.score,
            };
            if better {
                best = Some(r);
            }
        }
    }
    best
}

// --- longitudinal profile ----------------------------------------------------

/// `routing.spine_profile`: `(net_dz, max_above_chord, climb_per_100m)`
/// along a spine at `step_m` (8 m).
pub fn spine_profile(f: &Fields, spine: &[Yx], step_m: f64) -> (f64, f64, f64) {
    let mut cum = Vec::with_capacity(spine.len());
    cum.push(0.0);
    let mut l = 0.0;
    for i in 1..spine.len() {
        l += hyp(spine[i], spine[i - 1]);
        cum.push(l);
    }
    if l < 40.0 {
        return (0.0, 0.0, 0.0);
    }
    let ns = ((l / step_m) as usize).max(4) + 1;
    let ss = linspace(0.0, l, ns);
    let sy: Vec<f64> = spine.iter().map(|p| p.0).collect();
    let sx: Vec<f64> = spine.iter().map(|p| p.1).collect();
    let mut z = Vec::with_capacity(ns);
    for &s in &ss {
        let y = interp(s, &cum, &sy);
        let x = interp(s, &cum, &sx);
        z.push(f.z8[cell_of(f, y, x)]);
    }
    let net_dz = z[ns - 1] - z[0];
    let chord = linspace(z[0], z[ns - 1], ns);
    let mut above = f64::NEG_INFINITY;
    for i in 0..ns {
        above = above.max(z[i] - chord[i]);
    }
    let mut climb = 0.0;
    for i in 1..ns {
        let dz = z[i] - z[i - 1];
        if dz > 0.0 {
            climb += dz;
        }
    }
    (net_dz, above, climb / l * 100.0)
}

/// The profile terms and their raw diagnostics.
#[derive(Clone, Copy, Debug)]
pub struct ProfileTerms {
    pub prof_net: f64,
    pub prof_chord: f64,
    pub prof_climb: f64,
    pub net_dz: f64,
    pub above: f64,
    pub climb100: f64,
}

/// `routing.profile_terms`.
pub fn profile_terms(f: &Fields, spine: &[Yx]) -> ProfileTerms {
    let (net_dz, above, climb100) = spine_profile(f, spine, 8.0);
    let net_t = trapezoid(net_dz, PROF_NET_BAND.0, PROF_NET_BAND.1, TRAP_RAMP, TRAP_TAIL);
    let above_pen = clip((above - PROF_CHORD_FREE_M) / (PROF_CHORD_SAT_M - PROF_CHORD_FREE_M),
                         0.0, 1.0);
    let climb_pen = clip((climb100 - PROF_CLIMB_100_FREE)
                         / (PROF_CLIMB_100_SAT - PROF_CLIMB_100_FREE), 0.0, 1.0);
    ProfileTerms {
        prof_net: 0.4 * net_t,
        prof_chord: -0.8 * above_pen,
        prof_climb: -0.4 * climb_pen,
        net_dz,
        above,
        climb100,
    }
}

// --- detail placement + exact rescore --------------------------------------

/// `routing._walk`: the straight green->tee walk with its mean grade and
/// wet spans.
pub fn walk(t: &Terrain, f: &Fields, a: Yx, b: Yx, hole: usize) -> Walk {
    let l = hyp(a, b);
    let n = ((l / f.cell) as usize).max(1);
    let ts = linspace(0.0, 1.0, n + 1);
    let mut zz = Vec::with_capacity(n + 1);
    for &s in &ts {
        let p = (a.0 + s * (b.0 - a.0), a.1 + s * (b.1 - a.1));
        zz.push(f.z8[cell_of(f, p.0, p.1)]);
    }
    let grade = if n > 1 {
        let mut acc = 0.0;
        for i in 1..zz.len() {
            acc += (zz[i] - zz[i - 1]).abs();
        }
        acc / l.max(1e-9)
    } else {
        0.0
    };
    Walk {
        path: [a, b],
        length_m: l,
        grade_mean: grade,
        bridges: wet_spans(t, a, b, BridgeKind::Walk, hole, 2.0),
    }
}

/// Metres of the walk spent going AGAINST the prior hole's closing
/// direction (real p50 is zero; golfers walk onward, not back).
fn backtrack_m(prev_dir: Option<Yx>, w: &Walk) -> f64 {
    match prev_dir {
        None => 0.0,
        Some(pd) => {
            if w.length_m < 5.0 {
                return 0.0;
            }
            let wv = (w.path[1].0 - w.path[0].0, w.path[1].1 - w.path[0].1);
            0.0_f64.max(-(pd.0 * wv.0 + pd.1 * wv.1))
        }
    }
}

fn round_to(x: f64, d: i32) -> f64 {
    let m = 10f64.powi(d);
    (x * m).round() / m
}

/// `routing.detail_route`: exact placement of tees, LZs, boxes, walks and
/// bridges along a beam state's sequence `(green_idx, par, est_tee)`, then
/// the exact rescore.
pub fn detail_route(t: &Terrain, f: &Fields, rf: &RouteFields, site: &SiteCtx,
                    pool: &[Candidate], seq: &[(usize, u8, Yx)]) -> Option<Route> {
    if seq.is_empty() {
        return None;
    }
    let home = site.home;
    let nh = seq.len();
    let pct = pool_pct(pool);
    let sett = pool_setting(f, pool);
    let mut holes: Vec<Hole> = Vec::with_capacity(nh);
    let mut spines: Vec<Vec<Yx>> = Vec::with_capacity(nh);
    let mut walks: Vec<[Yx; 2]> = Vec::with_capacity(nh);
    let mut lz_seen: Vec<Yx> = Vec::new();
    let mut prev_dir: Option<Yx> = None;    // unit closing direction of prev hole
    let mut pos = home;
    let mut total_len = 0.0;
    let mut total_walk = 0.0;
    let mut hole_scores = 0.0;

    for (h, &(gi, par, _est)) in seq.iter().enumerate() {
        let g = pool[gi].yx;
        // every OTHER hole's green is a fixed obstacle for this hole's LZs
        // and its line; `junction_idx` is the predecessor's slot in it
        let mut other_greens: Vec<Yx> = Vec::with_capacity(nh);
        let mut junction_idx: Option<usize> = None;
        for (h2, q) in seq.iter().enumerate() {
            if q.0 == gi {
                continue;
            }
            if h > 0 && h2 + 1 == h {
                junction_idx = Some(other_greens.len());
            }
            other_greens.push(pool[q.0].yx);
        }
        let mut rest_min = 0.0;
        for q in &seq[h + 1..] {
            rest_min += par_band(q.1).0;
        }
        let hi_cap = BUDGET_M - total_len - rest_min;
        let opts = place_tee(rf, f, pos, g, par, if h == 0 { Some(home) } else { None },
                             &spines, &walks, &other_greens, junction_idx, N_TEE_OPTIONS,
                             Some(hi_cap), Some(home));
        let opts = match opts {
            Some(o) => o,
            None => {
                // saturating fallback: tee at the walk-disc edge toward the green
                let v = (g.0 - pos.0, g.1 - pos.1);
                let d = v.0.hypot(v.1);
                let vn = (v.0 / d.max(1e-9), v.1 / d.max(1e-9));
                let (lo, hi) = par_band(par);
                let back_d = 0.0_f64.max(d - (lo + hi) / 2.0);
                let raw = (pos.0 + vn.0 * back_d, pos.1 + vn.1 * back_d);
                // item 2 (2026-09-15): the raw point obeys no slope cap (two
                // hole-9 par 3s in 250 sat on 35-62 % nodes); take the
                // nearest dry node under TEE_BOX_SLOPE_MAX within
                // TEE_FALLBACK_SEARCH_M instead, the point itself failing that
                vec![TeeOption {
                    yx: nearest_capped_node(f, raw, TEE_FALLBACK_SEARCH_M).unwrap_or(raw),
                    score: -0.5,
                    graded: true,
                }]
            }
        };

        // JOINT tee x LZ retry: a clean tee whose only LZs cross earlier
        // geometry is not clean at all -- try the next tee option before
        // accepting a crossing (the walk-4/hole-6 case above).
        let mut chosen: Option<(TeeOption, Vec<(f64, f64, f64)>, f64)> = None;
        for opt in opts.iter().take(JOINT_RETRY_CAP) {
            let mut lzs: Vec<(f64, f64, f64)> = Vec::new();
            let mut lz_score = 1.0;
            let mut all_clean = true;
            if par >= 4 {
                let rem = if par == 4 { (90.0, 200.0) } else { (300.0, 999.0) };
                if let Some(r1) = place_lz(rf, f, opt.yx, g, DRIVE_R_M, rem, &spines, &walks,
                                           &lz_seen, &other_greens, junction_idx, Some(home)) {
                    lzs.push(r1.lz);
                    lz_score = r1.score;
                    all_clean = all_clean && r1.clean;
                }
            }
            if par == 5 && !lzs.is_empty() {
                let from = (lzs[0].0, lzs[0].1);
                if let Some(r2) = place_lz(rf, f, from, g, SECOND_R_M, (120.0, 200.0), &spines,
                                           &walks, &lz_seen, &other_greens, None, Some(home)) {
                    lzs.push(r2.lz);
                    lz_score = 0.5 * (lz_score + r2.score);
                    all_clean = all_clean && r2.clean;
                }
            }
            if chosen.is_none() {
                chosen = Some((*opt, lzs.clone(), lz_score));
            }
            if all_clean {
                chosen = Some((*opt, lzs, lz_score));
                break;
            }
        }
        let (tee, lzs, lz_score) = chosen.expect("opts is never empty");
        let tee_yx = tee.yx;
        for &(y, x, _r) in &lzs {
            lz_seen.push((y, x));
        }

        let mut pts: Vec<Yx> = Vec::with_capacity(lzs.len() + 2);
        pts.push(tee_yx);
        for &(y, x, _r) in &lzs {
            pts.push((y, x));
        }
        pts.push(g);
        let mut length = 0.0;
        for i in 1..pts.len() {
            length += hyp(pts[i], pts[i - 1]);
        }
        let k = pts.len();
        let vin = (pts[k - 1].0 - pts[k - 2].0, pts[k - 1].1 - pts[k - 2].1);
        let nv = vin.0.hypot(vin.1).max(1e-9);
        let ab = approach_bin((vin.0 / nv, vin.1 / nv));
        let appr = score_approach(&pool[gi].approach, ab);

        let mut bridges: Vec<Bridge> = Vec::new();
        for i in 1..pts.len() {
            bridges.extend(wet_spans(t, pts[i - 1], pts[i], BridgeKind::Spine, h, 2.0));
        }
        let wk = walk(t, f, pos, tee_yx, h);

        let mut bridge_sum = 0.0;
        for b in bridges.iter().chain(wk.bridges.iter()) {
            bridge_sum += -0.3 - 0.005 * b.span_m;
        }
        let b2b = if h > 0 && (par == 3 || par == 5) && seq[h - 1].1 == par { -0.6 } else { 0.0 };
        let (blo, bhi) = par_band(par);
        let mut terms: BTreeMap<String, f64> = BTreeMap::new();
        terms.insert("green".into(), 1.5 * pct[gi]);
        terms.insert("edge".into(), edge_term(site.win, g, pool[gi].reserved));
        terms.insert("setting".into(), SETT_W * sett[gi]);
        terms.insert("approach".into(), 1.0 * appr);
        terms.insert("lz".into(), if par >= 4 { 0.8 * clip(lz_score, -1.0, 1.0) } else { 0.0 });
        terms.insert("tee".into(), 0.4 * clip(tee.score, -1.0, 1.0));
        terms.insert("length".into(), 0.5 * trapezoid(length, blo, bhi, TRAP_RAMP, TRAP_TAIL));
        terms.insert("walk".into(),
                     -1.0 * clip((wk.length_m - WALK_FREE_M) / (WALK_SAT_M - WALK_FREE_M), 0.0, 1.0)
                     - 0.3 * clip(wk.length_m / WALK_MAX_M, 0.0, 1.0));
        terms.insert("backtrack".into(),
                     -0.7 * clip((backtrack_m(prev_dir, &wk) - BACKTRACK_FREE_M)
                                 / (BACKTRACK_SAT_M - BACKTRACK_FREE_M), 0.0, 1.0));
        terms.insert("walk_grade".into(), -0.15 * clip(wk.grade_mean / 0.15, 0.0, 1.0));
        terms.insert("bridges".into(), (-1.5_f64).max(bridge_sum));
        terms.insert("b2b".into(), b2b);
        terms.insert("ch_keepout".into(), -6.0 * ch_intrusion(&pts, home, CH_TRIM_M));
        let pt = profile_terms(f, &pts);
        terms.insert("prof_net".into(), pt.prof_net);
        terms.insert("prof_chord".into(), pt.prof_chord);
        terms.insert("prof_climb".into(), pt.prof_climb);
        // the score sum runs BEFORE the diagnostics join the map
        for v in terms.values() {
            hole_scores += v;
        }
        terms.insert("net_dz_m".into(), round_to(pt.net_dz, 1));
        terms.insert("above_chord_m".into(), round_to(pt.above, 1));
        // item 6 diagnostic: the best feature within 40 m of this hole's LZs
        let lz_int = lzs.iter()
            .map(|&(ly, lx, _)| rf.interest40[cell_of(f, ly, lx)])
            .fold(0.0_f64, f64::max);
        terms.insert("lz_interest40".into(), round_to(lz_int, 2));

        let boxes = tee_boxes(rf, f, tee_yx, pts[1], g);
        holes.push(Hole {
            index: h,
            par,
            green_idx: gi,
            kind: pool[gi].kind.clone(),
            green_yx: g,
            tee_boxes: boxes,
            lzs: lzs.clone(),
            spine: pts.clone(),
            length_m: length,
            approach_bin: ab,
            bridges,
            walk_from_prev: wk.clone(),
            terms,
        });
        spines.push(pts.clone());
        walks.push(wk.path);
        total_len += length;
        total_walk += wk.length_m;
        pos = g;
        let v_close = (pts[k - 1].0 - pts[k - 2].0, pts[k - 1].1 - pts[k - 2].1);
        let n_close = v_close.0.hypot(v_close.1);
        prev_dir = if n_close > 1e-9 { Some((v_close.0 / n_close, v_close.1 / n_close)) } else { None };
    }

    // line-of-play clearance + LZ separation, exact
    let mut clear_pen = 0.0;
    let mut worst_clear = 0.0_f64;
    for i in 0..nh {
        for j in (i + 1)..nh {
            let v = clearance_violation(&spines[i], &spines[j], j == i + 1);
            worst_clear = worst_clear.max(v);
            clear_pen -= 3.0 * v * v;        // quadratic: brushing is cheap,
                                             // sharing a corridor is not
        }
    }
    // GREEN IN PLAY, exact (plan item 1): every (A's green, B's spine),
    // A != B, on the placed polylines; B = A + 1 exempt over its first
    // GREEN_JUNCTION_M of arc, and so is B = hole 1 against the last green
    // (the clubhouse loop junction). Offenders under GREEN_CLEAR_M recorded.
    let mut gip_pen = 0.0;
    let mut green_in_play: Vec<(usize, usize, f64)> = Vec::new();
    for a in 0..nh {
        for b in 0..nh {
            if a == b {
                continue;
            }
            // B = A + 1, or the loop junction (A the last hole, B the first)
            let junction = b == a + 1 || (a + 1 == nh && b == 0);
            if let Some((d, v)) = green_play(&spines[b], holes[a].green_yx, junction) {
                gip_pen -= GIP_W * v * v;
                if d < GREEN_VETO_M {
                    gip_pen -= GIP_VETO_ROUTE;
                }
                if d < GREEN_CLEAR_M {
                    green_in_play.push((a, b, d));
                }
            }
        }
    }
    let mut green_lz_pen = 0.0;
    let mut worst_glz = 999.0_f64;
    for hh in &holes {
        for &(ly, lx, _r) in &hh.lzs {
            for h2 in &holes {
                if h2.index == hh.index {
                    continue;
                }
                let d_gl = (h2.green_yx.0 - ly).hypot(h2.green_yx.1 - lx);
                worst_glz = worst_glz.min(d_gl);
                if d_gl < GREEN_LZ_SEP_M {
                    green_lz_pen -= 5.0 * (1.0 - d_gl / GREEN_LZ_SEP_M);
                }
            }
        }
    }
    let mut lz_all: Vec<(usize, f64, f64)> = Vec::new();
    for hh in &holes {
        for &(y, x, _r) in &hh.lzs {
            lz_all.push((hh.index, y, x));
        }
    }
    let mut lz_pen = 0.0;
    for ii in 0..lz_all.len() {
        for jj in (ii + 1)..lz_all.len() {
            if lz_all[ii].0 == lz_all[jj].0 {
                continue;
            }
            let dd = (lz_all[ii].1 - lz_all[jj].1).hypot(lz_all[ii].2 - lz_all[jj].2);
            if dd < LZ_SEP_M {
                // SHARED LANDING ZONES ARE AVOIDED (owner): heavy, saturating
                lz_pen -= 2.0 * (1.0 - dd / LZ_SEP_M);
            }
        }
    }

    // crossings, exact
    let mut crossings: Vec<(usize, usize, Yx)> = Vec::new();
    let mut cross_pen = 0.0_f64;
    for i in 0..nh {
        for j in (i + 1)..nh {
            for p in polyline_crossings(&spines[i], &spines[j], CROSS_TRIM_M) {
                crossings.push((i, j, p));
            }
        }
    }
    // play x play: near-veto (owner, 2026-08-30: possible but VERY rare,
    // ~1% of courses -- the first 20-seed round produced crossings on 8/20
    // at -3.0, so the price doubles and the cap loosens)
    cross_pen += (-18.0_f64).max(-6.0 * crossings.len() as f64);
    let mut walk_cross = 0usize;
    for i in 0..nh {
        for j in 0..nh {
            if j == i || j + 1 == i {
                continue;        // a walk legitimately touches its own holes
            }
            if !polyline_crossings(&walks[i], &spines[j], CROSS_TRIM_M).is_empty() {
                walk_cross += 1;
            }
        }
    }
    cross_pen += (-8.0_f64).max(-4.0 * walk_cross as f64);

    let mut kind_counts: BTreeMap<&str, usize> = BTreeMap::new();
    for hh in &holes {
        *kind_counts.entry(pool[hh.green_idx].kind.as_str()).or_insert(0) += 1;
    }
    let total_kinds = nh as f64;
    let mut entropy = 0.0;
    for &c in kind_counts.values() {
        let p = c as f64 / total_kinds;
        entropy -= p * p.ln();
    }
    entropy /= 9.0_f64.ln();
    let mut spread = 0.0;
    for (par, need) in [(3u8, 40.0), (4, 80.0), (5, 50.0)] {
        let ls: Vec<f64> = holes.iter().filter(|hh| hh.par == par).map(|hh| hh.length_m).collect();
        if ls.len() >= 2 {
            let mut mx = f64::NEG_INFINITY;
            let mut mn = f64::INFINITY;
            for &l in &ls {
                mx = mx.max(l);
                mn = mn.min(l);
            }
            if mx - mn >= need {
                spread += 0.3;
            }
        }
    }
    // weight 1.0 -> 2.5, tail 0.4 -> 0.05 (2026-08-30): adding the beam
    // clearance term taught the router to BUY spacing with length (3,603 m
    // totals); the band must push back as hard as the spacing pulls
    let tot_t = trapezoid(total_len, TOTAL_BAND_M.0, TOTAL_BAND_M.1, TRAP_RAMP, 0.05);

    // PAR-5 POSITIONAL SPREAD (owner, 2026-08-30): the length budget was
    // banking yardage for the end and the fives all landed on holes 7-9.
    // Reward fives whose indices span the round; -0.8 when they bunch in
    // any 3-hole stretch.
    // MIX PRIOR, route side: (2,5,2) is what architects build (94.2% of
    // real par-36 nines); the other two stay legal-but-exceptional.
    let n3 = holes.iter().filter(|hh| hh.par == 3).count();
    let n5 = holes.iter().filter(|hh| hh.par == 5).count();
    // calibration ladder (200 fresh seeds per rung, real = 94.2/2.7/3.1):
    //   1.2 / 0.15 / 0.0   ->  73.5 / 19.0 / 7.5
    //   2.0 / 0.0  / 0.0   ->  86.0 / 10.5 / 3.5   ((1,7,1) on target)
    //   2.0 / 0.0  / -0.6  ->  86.0 /  7.5 / 6.5   (minority reshuffled;
    //                            the real limiter is (2,5,2) missing from
    //                            the finals -- fixed structurally below)
    let mix_prior = match (n3, n5) {
        (2, 2) => 2.0,
        (1, 1) => 0.0,
        (3, 3) => -0.6,
        _ => 0.0,
    };
    let idx5: Vec<usize> = holes.iter().filter(|hh| hh.par == 5).map(|hh| hh.index).collect();
    if idx5.len() >= 2 {
        let mx = *idx5.iter().max().expect("non-empty");
        let mn = *idx5.iter().min().expect("non-empty");
        if mx - mn < 3 {
            spread -= 0.8;
        } else if mx - mn >= 4 {
            spread += 0.3;
        }
    }
    let coverage = coverage_share(f, site.win, &spines, COV_R_M);
    let coverage100 = coverage_share(f, site.win, &spines, 100.0);
    let mut rterms: BTreeMap<String, f64> = BTreeMap::new();
    rterms.insert("coverage".into(), COV_W_ROUTE * coverage);
    rterms.insert("entropy".into(), 0.6 * entropy);
    rterms.insert("spread".into(), spread);
    rterms.insert("total".into(), 2.5 * tot_t);
    rterms.insert("mix_prior".into(), mix_prior);
    rterms.insert("crossings".into(), cross_pen);
    rterms.insert("clearance".into(), clear_pen);
    rterms.insert("lz_sep".into(), lz_pen);
    rterms.insert("green_in_lz".into(), green_lz_pen);
    rterms.insert("green_in_play".into(), gip_pen);
    // DEVIATION: the prototype assigned `worst_clear = 0.0` twice and
    // excluded a not-yet-present `min_green_lz_m` from the sum. Here the
    // score is summed over the score terms only; the two diagnostics are
    // inserted afterwards and are explicitly NOT score terms.
    let mut score = hole_scores;
    for v in rterms.values() {
        score += v;
    }
    rterms.insert("worst_clear".into(), round_to(worst_clear, 2));
    rterms.insert("coverage100".into(), round_to(coverage100, 3));
    rterms.insert("min_green_lz_m".into(),
                  if worst_glz < 999.0 { round_to(worst_glz, 0) } else { -1.0 });
    let par_sequence: Vec<u8> = holes.iter().map(|hh| hh.par).collect();
    Some(Route {
        holes,
        par_sequence,
        total_length_m: total_len,
        total_walk_m: total_walk,
        clubhouse_yx: home,
        score,
        terms: rterms,
        crossings,
        green_in_play,
        coverage,
    })
}

/// `routing.run_routing(z2, cell2, wet2, sit, f, pool, pool_fn)`: one full
/// route per clubhouse alternate (the pool regenerated per alternate when
/// `pool_fn` is given), the best returned; `None` when none survives.
///
/// ROUTING PICKS THE CLUBHOUSE (owner, 2026-08-30): siting proposes up to
/// three spatially distinct clubhouse candidates; a route is auditioned
/// from each and the best-scoring route wins -- the routing is what
/// matters, the clubhouse serves it. `pool_fn(clubhouse) -> pool`
/// regenerates the pool per candidate (the pool's reserved loop anchor
/// depends on the clubhouse); when absent the given pool is reused.
/// `sit.clubhouse.alternates` is the audition list as `siting.py` builds it
/// (the primary is its first entry); when empty the primary alone runs.
pub fn run_routing(t: &Terrain, sit: &Siting, f: &Fields, m: &Morphology, p: &Persistence,
                   pool: &[Candidate], pool_fn: Option<&dyn Fn(&Clubhouse) -> Vec<Candidate>>)
    -> Option<Route> {
    // stage timing, `SAND_TIME=1` (the sandhills crate's switch; `route_batch
    // --time` sets it): the once-per-tile raster build
    let t0 = std::time::Instant::now();
    let rf = build_route_fields(f, m, p, sit.window_m);
    if std::env::var("SAND_TIME").is_ok() {
        eprintln!("  time {:14} {:7.1} ms", "route_fields", t0.elapsed().as_secs_f64() * 1e3);
    }
    let primary = std::slice::from_ref(&sit.clubhouse);
    let alts: &[Clubhouse] = if sit.clubhouse.alternates.is_empty() {
        primary
    } else {
        &sit.clubhouse.alternates
    };
    let mut best: Option<Route> = None;
    for ch in alts {
        let regenerated;
        let p_ch: &[Candidate] = match pool_fn {
            Some(pf) => {
                regenerated = pf(ch);
                &regenerated
            }
            None => pool,
        };
        let site = SiteCtx { home: ch.yx, win: sit.window_m, m };
        if let Some(r) = beam_route(t, f, &rf, &site, p_ch) {
            let better = match &best {
                None => true,
                Some(b) => r.score > b.score,
            };
            if better {
                best = Some(r);
            }
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// `test_routing.test_kernels`' par-mix gate, over every prefix of all
    /// 3^9 sequences: the incremental `legal_pars` chain admits exactly the
    /// prefixes that some ALLOWED_MIXES sequence completes.
    #[test]
    fn legal_pars_matches_brute_force() {
        let pars = [3u8, 4, 5];
        let mut legal_prefixes: HashSet<Vec<u8>> = HashSet::new();
        let mut n_legal = 0;
        for code in 0..3usize.pow(9) {
            let mut c = code;
            let mut seq = Vec::with_capacity(9);
            for _ in 0..9 {
                seq.push(pars[c % 3]);
                c /= 3;
            }
            let n3 = seq.iter().filter(|&&p| p == 3).count() as u8;
            let n4 = seq.iter().filter(|&&p| p == 4).count() as u8;
            let n5 = seq.iter().filter(|&&p| p == 5).count() as u8;
            if ALLOWED_MIXES.contains(&(n3, n4, n5)) {
                n_legal += 1;
                for l in 1..=9 {
                    legal_prefixes.insert(seq[..l].to_vec());
                }
            }
        }
        assert_eq!(n_legal, 756 + 1680 + 72);
        for code in 0..3usize.pow(9) {
            let mut c = code;
            let mut seq = Vec::with_capacity(9);
            for _ in 0..9 {
                seq.push(pars[c % 3]);
                c /= 3;
            }
            let mut counts = (0u8, 0u8, 0u8);
            for h in 0..9 {
                let p = seq[h];
                let chain_ok = legal_pars(counts, h).contains(&p);
                let brute = legal_prefixes.contains(&seq[..=h]);
                assert_eq!(chain_ok, brute, "prefix {:?}", &seq[..=h]);
                if !chain_ok {
                    break;
                }
                counts = bump_counts(counts, p);
            }
        }
    }

    #[test]
    fn mix_feasible_cases() {
        assert!(mix_feasible((0, 0, 0), 0));
        assert!(mix_feasible((2, 5, 2), 9));
        assert!(mix_feasible((3, 3, 3), 9));
        assert!(mix_feasible((1, 7, 1), 9));
        assert!(mix_feasible((2, 5, 1), 8));
        assert!(mix_feasible((0, 7, 0), 7));
        assert!(!mix_feasible((4, 0, 0), 4));
        assert!(!mix_feasible((0, 8, 0), 8));
        assert!(!mix_feasible((2, 5, 2), 8));
        assert!(!mix_feasible((3, 3, 2), 9));
        assert_eq!(legal_pars((0, 0, 0), 0), vec![3, 4, 5]);
        assert_eq!(legal_pars((2, 5, 1), 8), vec![5]);
        assert_eq!(legal_pars((3, 0, 0), 3), vec![4, 5]);
        assert_eq!(legal_pars((2, 0, 0), 2), vec![3, 4, 5]);
    }

    #[test]
    fn approach_bin_cardinals() {
        // playing toward -x: stance faces +x -> bin 0
        assert_eq!(approach_bin((0.0, -1.0)), 0);
        // playing toward -y: stance faces +y -> pi/2 -> bin 4
        assert_eq!(approach_bin((-1.0, 0.0)), 4);
        // playing toward +x: stance faces -x -> pi -> bin 8
        assert_eq!(approach_bin((0.0, 1.0)), 8);
        // playing toward +y: stance faces -y -> 3pi/2 -> bin 12
        assert_eq!(approach_bin((1.0, 0.0)), 12);
        // diagonal
        let s = std::f64::consts::FRAC_1_SQRT_2;
        assert_eq!(approach_bin((-s, -s)), 2);
        // every bearing lands in 0..16 and the bins step by one every 22.5 deg
        for k in 0..N_BEARINGS {
            let th = k as f64 * 2.0 * PI / N_BEARINGS as f64;
            let v = (-th.sin(), -th.cos());
            assert_eq!(approach_bin(v), k);
        }
    }

    #[test]
    fn score_approach_weights() {
        let mut tab = [[0.0; 4]; N_BEARINGS];
        tab[3] = [1.0, 0.05, 4.0, 1.0];
        assert!((score_approach(&tab, 3) - 1.0).abs() < 1e-12);
        tab[5] = [0.0, -0.5, 0.0, 0.0];
        assert!((score_approach(&tab, 5) + 0.3).abs() < 1e-12);
    }

    fn st(score: f64, greens: &[usize], pars: &[u8]) -> BeamState {
        BeamState {
            score,
            used: Vec::new(),
            counts: (0, 0, 0),
            pos: (0.0, 0.0),
            seq: greens.iter().zip(pars).map(|(&g, &p)| (g, p, (0.0, 0.0))).collect(),
            segs: Vec::new(),
            cum: 0.0,
            cum_mid: 0.0,
            pdir: None,
            lzs_ap: Vec::new(),
            cov: Vec::new(),
        }
    }

    #[test]
    fn state_order_is_total() {
        let a = st(2.0, &[1, 2], &[4, 4]);
        let b = st(1.0, &[0, 1], &[3, 3]);
        let c = st(1.0, &[0, 2], &[3, 3]);
        let d = st(1.0, &[0, 2], &[4, 3]);
        let e = st(1.0 + 1e-9, &[9, 9], &[5, 5]);
        let nan = st(f64::NAN, &[0, 0], &[3, 3]);
        assert_eq!(state_order(&a, &b), Ordering::Less);      // higher score first
        assert_eq!(state_order(&b, &c), Ordering::Less);      // green tuple
        assert_eq!(state_order(&c, &d), Ordering::Less);      // par tuple
        assert_eq!(state_order(&e, &b), Ordering::Less);      // 1e-9 counts, no rounding
        assert_eq!(state_order(&nan, &b), Ordering::Greater); // NaN last
        assert_eq!(state_order(&c, &c), Ordering::Equal);
        let all = [&a, &b, &c, &d, &e, &nan];
        for x in all {
            for y in all {
                assert_eq!(state_order(x, y), state_order(y, x).reverse());
                for z in all {
                    if state_order(x, y) != Ordering::Greater
                        && state_order(y, z) != Ordering::Greater {
                        assert_ne!(state_order(x, z), Ordering::Greater);
                    }
                }
            }
        }
        let mut v = vec![d.clone(), nan.clone(), c.clone(), b.clone(), e.clone(), a.clone()];
        v.sort_by(state_order);
        let got: Vec<f64> = v.iter().map(|s| s.score).collect();
        assert!(got[0] == 2.0 && got[1] > 1.0 && got[5].is_nan());
        assert_eq!(v[2].seq[0].0, 0);
        assert_eq!(v[2].seq[1].0, 1);
        assert_eq!(v[3].seq[0].1, 3);   // c: pars (3,3) before d: pars (4,3)
        assert_eq!(v[4].seq[0].1, 4);
    }

    #[test]
    fn order_desc_is_stable() {
        let s = [1.0, 3.0, 3.0, 2.0, f64::NAN, 3.0];
        assert_eq!(order_desc(&s), vec![1, 2, 5, 3, 0, 4]);
    }

    #[test]
    fn pool_pct_ranks() {
        let mk = |s: f64| Candidate {
            yx: (0.0, 0.0),
            kind: "k".into(),
            score: s,
            grad: (0.0, 0.0),
            persistence: 0.0,
            approach: [[0.0; 4]; N_BEARINGS],
            reserved: false,
            build: crate::Build::Natural,
        };
        let pool = vec![mk(0.5), mk(-1.0), mk(2.0), mk(0.5)];
        let p = pool_pct(&pool);
        assert_eq!(p, vec![1.0 / 3.0, 0.0, 1.0, 2.0 / 3.0]);
        assert_eq!(pool_pct(&pool[..1]), vec![0.0]);
    }

    #[test]
    fn interp_and_linspace() {
        let v = linspace(0.0, 1.0, 5);
        assert_eq!(v, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(linspace(2.0, 2.0, 1), vec![2.0]);
        let xp = [0.0, 10.0, 30.0];
        let fp = [0.0, 1.0, 5.0];
        assert!((interp(5.0, &xp, &fp) - 0.5).abs() < 1e-12);
        assert!((interp(20.0, &xp, &fp) - 3.0).abs() < 1e-12);
        assert!((interp(30.0, &xp, &fp) - 5.0).abs() < 1e-12);
        assert!((interp(-1.0, &xp, &fp) - 0.0).abs() < 1e-12);
    }

    fn synth_terrain(wet_cols: std::ops::Range<usize>) -> Terrain {
        use course_world::grid::{Grid, GridSpec};
        use course_world::math::Vec2;
        let spec = GridSpec::new(Vec2::new(0.0, 0.0), 1.0, 100, 100);
        let height = Grid::filled(spec, 0.0);
        let mut water = Grid::filled(spec, f64::NAN);
        for y in 0..100 {
            for x in wet_cols.clone() {
                water.set(x as u32, y as u32, 0.0);
            }
        }
        Terrain::from_grids(crate::Mode::Fluvial, &height, &water, Vec::new())
    }

    /// `test_kernels`' wet strip: one bridge, 8..=12 m over a 10-cell strip.
    #[test]
    fn wet_spans_strip() {
        let t = synth_terrain(40..50);
        let br = wet_spans(&t, (50.0, 0.0), (50.0, 99.0), BridgeKind::Spine, 0, 2.0);
        assert_eq!(br.len(), 1, "{br:?}");
        assert!(br[0].span_m >= 8.0 && br[0].span_m <= 12.0, "{br:?}");
        assert_eq!(br[0].kind, BridgeKind::Spine);
        assert_eq!(br[0].hole, 0);
        assert!(br[0].a_yx.1 < 40.0 && br[0].b_yx.1 >= 50.0);
        let dry = synth_terrain(0..0);
        assert!(wet_spans(&dry, (1.0, 1.0), (90.0, 90.0), BridgeKind::Walk, 3, 2.0).is_empty());
        assert!(wet_spans(&t, (1.0, 1.0), (1.0, 2.0), BridgeKind::Walk, 3, 2.0).is_empty());
    }

    /// The whole chain on the prototype's gentle synthetic tile
    /// (`test_routing.make_tile`); needs the img/geom kernels and the
    /// siting + greens ports.
    #[test]
    #[ignore]
    fn gentle_tile_routes() {
        use course_world::grid::{Grid, GridSpec};
        use course_world::math::Vec2;
        let n = 1504u32;
        let spec = GridSpec::new(Vec2::new(0.0, 0.0), 2.0, n, n);
        let mut height = Grid::filled(spec, 0.0);
        for y in 0..n {
            for x in 0..n {
                let xx = x as f64 * 2.0;
                let yy = y as f64 * 2.0;
                let z = 0.9 * (xx / 210.0).sin() + 0.9 * (yy / 260.0).sin() + 0.002 * xx
                    + 4.0 * (xx / 900.0).sin() * (yy / 800.0).sin();
                height.set(x, y, z);
            }
        }
        let water = Grid::filled(spec, f64::NAN);
        let t = Terrain::from_grids(crate::Mode::Fluvial, &height, &water, Vec::new());
        let (_sit, r) = crate::route_tile(&t).expect("no route on a gentle tile");
        assert_eq!(r.holes.len(), 9);
        assert_eq!(r.par_sequence.iter().map(|&p| p as u32).sum::<u32>(), 36);
        let n3 = r.par_sequence.iter().filter(|&&p| p == 3).count() as u8;
        let n4 = r.par_sequence.iter().filter(|&&p| p == 4).count() as u8;
        let n5 = r.par_sequence.iter().filter(|&&p| p == 5).count() as u8;
        assert!(ALLOWED_MIXES.contains(&(n3, n4, n5)));
        assert!(hyp(r.holes[0].tee_boxes[0].yx, r.clubhouse_yx) <= CLUBHOUSE_RADIUS_M + 1e-6);
        assert!(hyp(r.holes[8].green_yx, r.clubhouse_yx) <= CLUBHOUSE_RADIUS_M + 1e-6);
        for h in &r.holes {
            assert_eq!(h.tee_boxes.len(), 5);
            let lens: Vec<f64> = h.tee_boxes.iter().map(|b| b.length_m).collect();
            for i in 0..4 {
                assert!(lens[i] >= lens[i + 1] - 1.0, "{lens:?}");
            }
        }
        assert!(r.total_length_m >= 2200.0 && r.total_length_m <= 3400.0);
    }
}
