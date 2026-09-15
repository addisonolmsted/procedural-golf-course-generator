//! The green pool — port of `tools/golf/greens.py` (P5-P9): typed
//! detectors, the fitted-logistic field score, 2 m confirmation, approach
//! tables, quotas and thinning.
//!
//! Consumes `siting::run_siting`'s outputs; emits 75-125 typed, scored
//! candidates with a (16-bearing x term) approach table each.
//!
//! The typed families exist because "morphological diversity" needs a
//! handle: without per-type quotas, whichever type scores highest on a seed
//! swamps the pool and the course becomes nine punchbowls. Types G
//! (valley/interdune flat) and I (generic pad) are deliberate additions to
//! the reviewer's list — most real greens sit on unremarkable ground, and
//! without G the aeolian pool is empty (interdune greens are geomorphon
//! `flat` at low relief_pos, matching none of the exotic classes).
//!
//! Hard gates are only the physically impossible (water under the pad, a
//! channel through it, inside the geomorphon collar); everything else is a
//! score. Hard constraints force retries, and this pipeline has none.
//!
//! Porting rules (lib.rs): no random draws; every sort is stable with the
//! prototype's key and falls through to the original index; numpy
//! broadcasting is written out as loops; iteration order is kept wherever
//! it decides which candidate survives a thinning pass.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;

use crate::img;
use crate::terrain::Terrain;
use crate::{Build, Candidate, Fields, Morphology, Persistence, Siting, N_BEARINGS};

/// `greens.generate(..., n_target=100)`.
pub const N_TARGET: usize = 100;

/// Approach stances are sampled 60-200 m out (`APPROACH_LO_M`,
/// `APPROACH_HI_M`); the golfer's eye is 1.6 m above the ground (`EYE_M`).
pub const APPROACH_LO_M: f64 = 60.0;
pub const APPROACH_HI_M: f64 = 200.0;
pub const EYE_M: f64 = 1.6;

/// `siting.GREEN_SLOPE_RR` (rise/run; playability's GREEN_SLOPE is 5 %):
/// the `edt_break` gate of the plateau detector. Mirrored here because
/// `siting.rs` does not export it (the sibling port is concurrent).
pub const GREEN_SLOPE_RR: f64 = 0.05;
/// `siting.CLUBHOUSE_RADIUS_M` (the C3 Budgets fixture): the loop anchor
/// is the confirmed candidate within 1.5 x this of the reserved green.
pub const CLUBHOUSE_RADIUS_M: f64 = 120.0;

// Pad-confirmation calibration, MEASURED against 519 OSM golf=green
// centroids on the 22 cached real-course tiles (2026-08-29). The original
// gate (r=16 m disc, 5 % slope, 1.4 m relief -- playability.py's constants)
// passes only 28 % of real Nebraska greens and 23 % of real Carolina ones:
// a real green is a ~20 m flattish kernel inside SHAPED surrounds, not a
// 32 m flat circle. Sweep:
//     r16 5% 1.4m -> 28 / 23 %      r10 6% 1.6m -> 83 / 51 %
//     r10 8% 2.2m -> 95 / 74 %  (Carolina stays lower: crowned Pinehurst
//                                greens carry deliberate falloff)
// NATURAL sites use the middle calibration; the GRADED class uses the loose
// one and carries an earthwork cost -- "buildable with a bulldozer's help",
// the second lever behind making every seed routable.
pub const SITE_R_M: f64 = 10.0;
pub const NAT_SLOPE_RR: f64 = 0.06;
pub const NAT_RELIEF_M: f64 = 1.6;
pub const GRADED_SLOPE_RR: f64 = 0.08;
pub const GRADED_RELIEF_M: f64 = 2.2;
/// Plane-fit residual RMS ceilings of the two classes (`confirm_2m`).
pub const NAT_RESID_M: f64 = 0.45;
pub const GRADED_RESID_M: f64 = 0.65;

// SURROUND relief band (25-90 m annulus, p90-p10 of elevation), measured
// on the 519 real greens 2026-08-29: median 6.8 m (NE) / 6.1 m (NC); real
// greens essentially never sit in dead-flat surrounds (< 2 m happens 3-5 %
// of the time, < 1 m almost never). Our pools were skewing dull -- 34-36 %
// of candidates below the real p25 (~3.7-4.3 m). A green should be locally
// buildable but CONTEXTUALLY interesting: flat pad, non-flat world around
// it. Only the annulus radii feed the fitted score today; the band
// thresholds and `W_SURROUND` are the pre-fit hand design, kept for the
// overlay3.py A/B (set 0 to ablate).
pub const SURROUND_R0_M: f64 = 25.0;
pub const SURROUND_R1_M: f64 = 90.0;
/// below: dead flat, heavy penalty ramp
pub const SURROUND_LO_M: f64 = 1.5;
/// real p25; full credit from here
pub const SURROUND_OK_M: f64 = 3.5;
/// soft cap; a wall of dune face stops helping
pub const SURROUND_HI_M: f64 = 14.0;
/// score weight; set 0 to ablate (see overlay3.py A/B)
pub const W_SURROUND: f64 = 1.0;

// FITTED SCORE -- fame-weighted logistic on the 2026-08-29 corpus:
// 429 US courses, 9119 real greens vs 36476 matched same-property controls,
// by-course 80/20 split (corpus/holdout.json). Held-out AUC 0.7928 vs
// 0.4716 for the previous hand weights -- which ranked real greens WORSE
// than random ground (room/vis/recept/aroom all carried the wrong sign).
// Coefficients are raw-unit, setting-features only: the 6
// construction-signature features (subgrid_rough, resid, slope, graded,
// curvatures) are deliberately EXCLUDED -- on a post-construction DEM they
// detect "already bulldozed" (rough alone scores 0.700) and invert under
// Carolina pine canopy (full model 0.464 there). Provenance:
// tools/golf/corpus/out/fitted/coeffs.json; fit.py BUILD_FEATURES. Per the
// pre-registered archetype test only sandhills_nc earned an override, on 3
// held-out courses -- recorded as provisional, NOT shipped.
//
// Linear + QUADRATIC terms. The linear-only fit (AUC 0.7928) was right in
// the middle of its range and wrong at the top tail: every term monotone,
// so the score maximized at EXTREMES (the biggest tpi60 knob, the most
// convex spur) while real greens sit on moderate rises -- measured
// 2026-08-29: our top-9 candidates hit real greens at 7 % within-100 m vs
// 17 % for RANDOM 9 points. Quadratics let moderation be expressed
// (held-out AUC 0.8319); peaks land near the real-green p90 (tpi60 optimum
// +2.5 m, surround ~15 m). Note the old hand design had the right SHAPE
// (trapezoid bands, d6w "floors not maximands") with wrong weights; the
// linear fit had right weights, wrong shape.
// Provenance: tools/golf/corpus/out/fitted/coeffs_quad.json (the
// `features` list there is the order below; `auc_held` 0.8319). The
// VALUES are the prototype's `greens.py` transcription -- linear terms
// rounded to 4 decimals, quadratics to 6 -- because the Python reference
// that the batch parity test is measured against uses those, not the
// full-precision JSON.
pub const FIT_B0: f64 = -0.1964;

/// Number of linear features (the JSON's 25, `saddle` included with a
/// zero coefficient).
pub const N_FEATURES: usize = 25;

/// Feature indices, in `coeffs_quad.json`'s `features` order.
pub const F_RELIEF_POS: usize = 0;
pub const F_TPI60: usize = 1;
pub const F_TPI200: usize = 2;
pub const F_ASPECT_SIN: usize = 3;
pub const F_ASPECT_COS: usize = 4;
pub const F_SURROUND: usize = 5;
pub const F_ROOM: usize = 6;
pub const F_D_WATER_BAND: usize = 7;
pub const F_D_BOUNDARY_NORM: usize = 8;
pub const F_PIT: usize = 9;
pub const F_PEAK: usize = 10;
pub const F_SADDLE: usize = 11;
pub const F_CLS240_FLAT: usize = 12;
pub const F_CLS240_CONVEX: usize = 13;
pub const F_CLS240_CONCAVE: usize = 14;
pub const F_CLS80_FLAT: usize = 15;
pub const F_CLS80_CONVEX: usize = 16;
pub const F_CLS80_CONCAVE: usize = 17;
pub const F_VIS_BEST: usize = 18;
pub const F_VIS_MEAN: usize = 19;
pub const F_RECEPT_BEST: usize = 20;
pub const F_BACKDROP_BEST: usize = 21;
pub const F_BACKDROP_FAR: usize = 22;
pub const F_AROOM_BEST: usize = 23;
pub const F_AROOM_MEAN: usize = 24;

/// Feature names, same order (for dumps and tests).
pub const FEATURE_NAMES: [&str; N_FEATURES] = [
    "relief_pos", "tpi60", "tpi200", "aspect_sin", "aspect_cos", "surround",
    "room", "d_water_band", "d_boundary_norm", "pit", "peak", "saddle",
    "cls240_flat", "cls240_convex", "cls240_concave", "cls80_flat",
    "cls80_convex", "cls80_concave", "vis_best", "vis_mean", "recept_best",
    "backdrop_best", "backdrop_far", "aroom_best", "aroom_mean",
];

/// Linear coefficients, `FEATURE_NAMES` order.
///
/// DEVIATION (routing round 1, item 3, 2026-09-15): `d_boundary_norm`'s
/// pair (-2.3707 linear, +7.0526 quadratic) is set to zero. The feature was
/// a MATCHED design variable in the corpus fit (`corpus/controls.py`: real
/// greens 0.115 vs controls 0.116), so its fitted parabola is not a
/// setting signal; on our windows its floor sat ~197 m inside the edge and
/// the 120 m halo OUTSIDE the window scored +0.3 logit above the
/// mid-band, so `hot_seeds` filled from the rim (57 % of greens within
/// 60 m of the edge, 45 % outside). The feature is still computed for
/// the dumps; the window is enforced by the router's edge term instead
/// (`route::EDGE_W`).
pub const FIT_COEF: [f64; N_FEATURES] = [
    0.9862,   // relief_pos
    1.2256,   // tpi60
    -0.2507,  // tpi200
    0.1231,   // aspect_sin
    0.1579,   // aspect_cos
    0.0922,   // surround
    -0.1759,  // room
    -0.0031,  // d_water_band
    0.0,      // d_boundary_norm -- fitted -2.3707; zeroed 2026-09-15 (see below)
    -1.2377,  // pit
    0.7923,   // peak
    0.0,      // saddle
    -0.4549,  // cls240_flat
    0.1913,   // cls240_convex
    -0.3858,  // cls240_concave
    0.0673,   // cls80_flat
    0.5415,   // cls80_convex
    -0.5068,  // cls80_concave
    -1.6782,  // vis_best
    -3.6503,  // vis_mean
    7.7403,   // recept_best
    -0.5791,  // backdrop_best
    0.0661,   // backdrop_far
    -2.9673,  // aroom_best
    -1.1311,  // aroom_mean
];

/// Quadratic coefficients as `(feature index, coefficient)`, in the JSON's
/// `*__sq` order. Sixteen of the 25 features carry one.
pub const FIT_COEF_SQ: [(usize, f64); 16] = [
    (F_TPI60, -0.240789),
    (F_TPI200, 0.011071),
    (F_SURROUND, -0.003025),
    (F_BACKDROP_BEST, 1.160808),
    (F_BACKDROP_FAR, -0.001514),
    (F_VIS_BEST, 1.311408),
    (F_VIS_MEAN, 2.243136),
    (F_RECEPT_BEST, -8.726523),
    (F_AROOM_BEST, 2.196199),
    (F_AROOM_MEAN, 1.348049),
    (F_RELIEF_POS, -0.835471),
    (F_PIT, 0.739331),
    (F_PEAK, -0.376947),
    (F_D_WATER_BAND, 6e-06),
    (F_D_BOUNDARY_NORM, 0.0),   // fitted 7.052632; zeroed 2026-09-15 (see below)
    (F_ROOM, 0.002634),
];

/// Construction-cost term, DESIGN CHOICE not fitted: the fit measures where
/// greens sit, not what they cost to build.
pub const W_GRADED: f64 = 0.5;

/// The prototype's `TYPES` tuple, detector order; `"hot"` (the field
/// score's own top cells) is the tenth kind string the pool can carry.
pub const TYPES: [&str; 9] = [
    "bench", "spur", "saddle", "punchbowl", "plateau", "dell", "valley_flat",
    "knoll", "generic",
];

/// geomorphon 10-class groupings (corpus/features.py)
fn cls_convex(c: u8) -> bool {
    matches!(c, 2 | 3 | 4 | 5)
}
fn cls_concave(c: u8) -> bool {
    matches!(c, 7 | 8 | 9 | 10)
}

// --------------------------------------------------------------------------
// numpy / Python semantics helpers

/// Python `round()` / `np.rint`: half to even.
fn round_half_even(v: f64) -> i64 {
    v.round_ties_even() as i64
}

/// `np.clip(arr.astype(int), 0, n - 1)`: truncation toward zero, then clamp.
fn clip_idx(v: f64, n: usize) -> usize {
    (v as i64).clamp(0, n as i64 - 1) as usize
}

/// Python float ordering (`-0.0 == 0.0`; NaN never compares, treated as a
/// tie so the stable sort keeps input order).
fn cmp_f(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

/// The fitted logistic (pre-sigmoid) on a full feature vector:
/// `FIT_B0 + sum(coef * v) + sum(coef_sq * v * v)`. Features the caller
/// cannot compute are passed as 0 and contribute nothing (both terms).
pub fn fitted_score(feats: &[f64; N_FEATURES]) -> f64 {
    let mut acc = FIT_B0;
    for i in 0..N_FEATURES {
        acc += FIT_COEF[i] * feats[i];
    }
    for &(i, c) in FIT_COEF_SQ.iter() {
        acc += c * feats[i] * feats[i];
    }
    acc
}

/// `greens.surround_relief(z8, cell, y, x)`: p90 - p10 of the 8 m heights
/// in the 25-90 m annulus (world-metre radii), 0 when the annulus leaves
/// the grid.
pub fn surround_relief(z8: &[f64], nx: usize, ny: usize, cell: f64, y: usize, x: usize) -> f64 {
    let n = (SURROUND_R1_M / cell) as usize; // int(): truncation, 11 at 8 m
    if y < n || x < n || y + n >= ny || x + n >= nx {
        return 0.0;
    }
    let mut zz = Vec::with_capacity((2 * n + 1) * (2 * n + 1));
    for dy in -(n as i64)..=(n as i64) {
        for dx in -(n as i64)..=(n as i64) {
            let d = (dy as f64 * cell).hypot(dx as f64 * cell);
            if d >= SURROUND_R0_M && d <= SURROUND_R1_M {
                let yy = (y as i64 + dy) as usize;
                let xx = (x as i64 + dx) as usize;
                zz.push(z8[yy * nx + xx]);
            }
        }
    }
    img::percentile(&zz, 90.0) - img::percentile(&zz, 10.0)
}

// --------------------------------------------------------------------------
// P5/P6 — typed seeding over window + halo

/// `greens._seed_typed`: `(i, j, kind)` seeds inside the window + 120 m
/// halo, gated hard only where physically necessary. Detector order and
/// the raster order inside each detector are the tie rule that decides a
/// cell's kind downstream (first claim wins in `generate`'s `pre` map).
///
/// The prototype's `flow_accum < 2 ha` gate is absent: `Fields` carries no
/// flow accumulation on this branch (the reference batch never passed one
/// either -- `build_fields(..., flow_accum8=None)`).
pub fn seed_typed(f: &Fields, m: &Morphology, p: &Persistence,
                  i0: usize, j0: usize, h: usize, w: usize) -> Vec<(usize, usize, &'static str)> {
    let (nx, ny) = (f.nx, f.ny);
    let halo = round_half_even(120.0 / f.cell) as usize;
    let a0 = i0.saturating_sub(halo);
    let a1 = (i0 + h + halo).min(ny);
    let b0 = j0.saturating_sub(halo);
    let b1 = (j0 + w + halo).min(nx);

    // the geomorphon collar: classes there are artifacts of the array edge.
    // `inner[c:-c, c:-c]` -- with c == 0 numpy's `[0:-0]` is EMPTY, so a
    // zero collar seeds nothing (it is 30 cells in practice).
    let c = m.collar;
    let n = nx * ny;
    let mut ok = vec![false; n];
    for y in a0..a1 {
        for x in b0..b1 {
            let k = y * nx + x;
            let inner = c > 0 && y >= c && y + c < ny && x >= c && x + c < nx;
            // hard gates
            ok[k] = inner && !f.wet8[k];
        }
    }

    let slope_ok: Vec<bool> = f.slope.iter().map(|&s| s <= GREEN_SLOPE_RR).collect();
    let edt_break = img::edt(&slope_ok, nx, ny, f.cell);

    let mut seeds: Vec<(usize, usize, &'static str)> = Vec::new();
    let mut take = |pred: &dyn Fn(usize) -> bool, kind: &'static str| {
        const CAP: usize = 600;
        let mut ks: Vec<usize> = Vec::new();
        for k in 0..n {
            if ok[k] && pred(k) {
                ks.push(k);
            }
        }
        // strided, not truncated: a raster-order cut takes only the
        // window's south edge for any type with more cells than the cap
        let step = if ks.len() > CAP { ks.len() / CAP + 1 } else { 1 };
        for &k in ks.iter().step_by(step) {
            seeds.push((k / nx, k % nx, kind));
        }
    };

    take(&|k| m.cls240[k] == 4 && f.pad_green[k], "bench");
    take(&|k| m.cls240[k] == 5 && f.pad_green[k], "spur");
    take(&|k| m.saddle[k] && f.pad_green[k], "saddle");
    // a 2 m-rough cell is not a green
    take(&|k| (m.cls80[k] == 7 || m.cls80[k] == 10) && p.pit[k] > 0.5 && p.pit[k] < 3.0
              && f.subgrid_rough[k] < 0.55, "punchbowl");
    take(&|k| m.cls240[k] == 1 && f.relief_pos[k] > 0.55 && edt_break[k] < 60.0
              && f.pad_green[k], "plateau");
    // dell: hollow at the 240 m scale with a partial rim — approximated as
    // hollow-class ground whose pit persistence is small (drains, not a sump)
    take(&|k| m.cls240[k] == 7 && p.pit[k] < 0.5 && f.pad_green[k], "dell");
    take(&|k| m.cls240[k] == 1 && f.relief_pos[k] < 0.45 && f.pad_green[k], "valley_flat");
    take(&|k| p.peak[k] > 1.0 && p.peak[k] < 4.0 && f.pad_green[k], "knoll");
    take(&|k| f.pad_green[k], "generic");
    seeds
}

// --------------------------------------------------------------------------
// P7 — 2 m confirmation on survivors

/// `greens.confirm_2m(z2, cell2, wet2, y_m, x_m)`: exact pad test + plane
/// fit on a 48 m crop of the 2 m grid. Returns `(class, plane gradient
/// (gy, gx), residual RMS)`; `class` is `None` when the pad fails. The 8 m
/// prefilter over-admits (its slope is smoothed); this is the authoritative
/// gate, run only on survivors — 400 x 24^2 cells instead of 1501^2.
///
/// The crop is `[y - r, y + r)` on both axes (the prototype's half-open
/// slice), the disc `yy^2 + xx^2 <= n^2` in 2 m cells with `n` =
/// `SITE_R_M / cell2`; the plane is `z = a + gy*y + gx*x` by least squares
/// (`img::plane_fit`); the gradient is returned even on a failed fit
/// (the forced loop anchor uses it).
pub fn confirm_2m(t: &Terrain, y_m: f64, x_m: f64) -> (Option<Build>, (f64, f64), f64) {
    let cell2 = t.z2.spec.cell_size;
    let nx2 = t.z2.spec.nx as usize;
    let ny2 = t.z2.spec.ny as usize;
    let r = round_half_even(24.0 / cell2);
    let y = round_half_even(y_m / cell2);
    let x = round_half_even(x_m / cell2);
    if y - r < 0 || x - r < 0 || y + r >= ny2 as i64 || x + r >= nx2 as i64 {
        return (None, (0.0, 0.0), 0.0);
    }
    let n = round_half_even(SITE_R_M / cell2);
    let n2 = n * n;
    let mut ys = Vec::new();
    let mut xs = Vec::new();
    let mut zs = Vec::new();
    for yy in -r..r {
        for xx in -r..r {
            if yy * yy + xx * xx > n2 {
                continue;
            }
            let k = (y + yy) as usize * nx2 + (x + xx) as usize;
            if t.wet2[k] {
                return (None, (0.0, 0.0), 0.0);
            }
            ys.push(yy as f64 * cell2);
            xs.push(xx as f64 * cell2);
            zs.push(t.z2.data[k]);
        }
    }
    let zmax = zs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let zmin = zs.iter().copied().fold(f64::INFINITY, f64::min);
    let relief = zmax - zmin;
    let (a, gy, gx) = img::plane_fit(&ys, &xs, &zs);
    let grade = gy.hypot(gx);
    let mut ss = 0.0;
    for i in 0..zs.len() {
        let e = zs[i] - (a + gy * ys[i] + gx * xs[i]);
        ss += e * e;
    }
    let resid = (ss / zs.len() as f64).sqrt();
    if relief <= NAT_RELIEF_M && grade <= NAT_SLOPE_RR && resid <= NAT_RESID_M {
        return (Some(Build::Natural), (gy, gx), resid);
    }
    if relief <= GRADED_RELIEF_M && grade <= GRADED_SLOPE_RR && resid <= GRADED_RESID_M {
        // buildable with bounded cut/fill; the earthwork is roughly the
        // residual volume over the pad
        return (Some(Build::Graded), (gy, gx), resid);
    }
    (None, (gy, gx), resid)
}

// --------------------------------------------------------------------------
// P8 — approach vectors

/// Bearing `k` of `N_BEARINGS`: `th = 2 pi k / N`, unit `(uy, ux) =
/// (sin th, cos th)` -- the prototype's expression order, kept so the
/// truncated sample indices agree bit for bit.
fn bearing_unit(k: usize) -> (f64, f64) {
    let th = 2.0 * PI * k as f64 / N_BEARINGS as f64;
    (th.sin(), th.cos())
}

/// `greens._backdrop_far(f, y, x)`: best mean rise 80-160 m beyond the
/// green over all bearings (fitted term; corpus/features.py is the
/// reference implementation).
pub fn backdrop_far(f: &Fields, y: usize, x: usize) -> f64 {
    let nb = (160.0 / f.cell) as usize; // int(): 20
    let zg = f.z8[y * f.nx + x];
    let mut best = -1e9_f64;
    for k in 0..N_BEARINGS {
        let (uy, ux) = bearing_unit(k);
        let mut sum = 0.0;
        let mut cnt = 0usize;
        for b in 1..=nb {
            if b as f64 * f.cell < 80.0 {
                continue;
            }
            let by = clip_idx(y as f64 - uy * b as f64, f.ny);
            let bx = clip_idx(x as f64 - ux * b as f64, f.nx);
            sum += f.z8[by * f.nx + bx] - zg;
            cnt += 1;
        }
        let mean = sum / cnt as f64;
        if mean > best {
            best = mean;
        }
    }
    best
}

/// `greens.approach_table(f, y, x, grad)` -> `[[visible_frac, receptivity,
/// backdrop, corridor_room]; N_BEARINGS]`.
///
/// Bearing k points FROM the approach TOWARD the green — a golfer standing
/// on bearing k+pi plays IN along -u_k. `visible_frac`: fraction of stances
/// at 60-200 m that can see the putting surface (eye 1.6 m, target the
/// SURFACE, not a flag — the question is judging the shot, not seeing a
/// stick). Receptivity: the pad tilting toward the golfer holds the ball;
/// away, it releases — the other, independent half of "benched from one
/// side, blind from the other".
pub fn approach_table(f: &Fields, y: usize, x: usize, grad: (f64, f64)) -> [[f64; 4]; N_BEARINGS] {
    let cell = f.cell;
    let (nx, ny) = (f.nx, f.ny);
    let nsteps = (APPROACH_HI_M / cell) as usize; // int(): 25
    let nb = (80.0 / cell) as usize; // int(): 10
    let mut out = [[0.0f64; 4]; N_BEARINGS];
    let zg = f.z8[y * nx + x];
    let (gy, gx) = grad;
    let mut prof = vec![0.0f64; nsteps];
    let mut d = vec![0.0f64; nsteps];
    let mut fair = vec![false; nsteps];
    for (k, row) in out.iter_mut().enumerate() {
        let (uy, ux) = bearing_unit(k);
        for s in 1..=nsteps {
            let py = clip_idx(y as f64 + uy * s as f64, ny);
            let px = clip_idx(x as f64 + ux * s as f64, nx);
            prof[s - 1] = f.z8[py * nx + px];
            fair[s - 1] = f.pad_fair[py * nx + px];
            d[s - 1] = s as f64 * cell;
        }
        // blind test: from stance s, the max angle subtended by any
        // intermediate t < s must not exceed the angle to the green surface
        let mut n_lo = 0usize;
        let mut n_vis = 0usize;
        for si in 0..nsteps {
            if d[si] < APPROACH_LO_M {
                continue;
            }
            n_lo += 1;
            let eye = prof[si] + EYE_M;
            let to_green = (zg - eye) / d[si];
            let mut blocked = false;
            if si > 0 {
                for t in 0..si {
                    if (prof[t] - eye) / (d[si] - d[t]) > to_green {
                        blocked = true;
                        break;
                    }
                }
            }
            if !blocked {
                n_vis += 1;
            }
        }
        row[0] = if n_lo > 0 { n_vis as f64 / n_lo as f64 } else { 0.0 };
        // receptivity: green tilts toward the incoming golfer (+)
        row[1] = -(gy * uy + gx * ux);
        // backdrop: mean rise 30-80 m beyond the green along -u
        let mut sum = 0.0;
        let mut cnt = 0usize;
        for b in 1..=nb {
            if b as f64 * cell < 30.0 {
                continue;
            }
            let by = clip_idx(y as f64 - uy * b as f64, ny);
            let bx = clip_idx(x as f64 - ux * b as f64, nx);
            sum += f.z8[by * nx + bx] - zg;
            cnt += 1;
        }
        row[2] = if cnt > 0 { sum / cnt as f64 } else { 0.0 };
        // corridor room at the landing zone (150-250 m out)
        let mut nl = 0usize;
        let mut nf = 0usize;
        for s in 0..nsteps {
            if d[s] >= 150.0 && d[s] <= 250.0 {
                nl += 1;
                if fair[s] {
                    nf += 1;
                }
            }
        }
        row[3] = if nl > 0 { nf as f64 / nl as f64 } else { 0.0 };
    }
    out
}

// --------------------------------------------------------------------------
// P9 — thinning

/// `greens._stamp_thin(cands, cell, shape, min_sep_m, cap)`: greedy
/// best-first with a raster-stamp occupancy grid — the fast, exactly
/// equivalent form of playability.peak_sites' O(n*k) loop. Order:
/// reserved first, then score descending, then `(y, x)` ascending, then
/// input order (the prototype's stable `sorted`). A reserved candidate is
/// never blocked by the stamp; the stamp is a `(2 sep + 1)^2` square.
pub fn stamp_thin(cands: &[Candidate], cell: f64, nx: usize, ny: usize,
                  min_sep_m: f64, cap: usize) -> Vec<Candidate> {
    let mut order: Vec<usize> = (0..cands.len()).collect();
    order.sort_by(|&a, &b| {
        let (ca, cb) = (&cands[a], &cands[b]);
        cb.reserved.cmp(&ca.reserved)
            .then(cmp_f(cb.score, ca.score))
            .then(cmp_f(ca.yx.0, cb.yx.0))
            .then(cmp_f(ca.yx.1, cb.yx.1))
            .then(a.cmp(&b))
    });
    let mut occ = vec![false; nx * ny];
    let sep = (round_half_even(min_sep_m / cell).max(1)) as usize;
    let mut out: Vec<Candidate> = Vec::new();
    for i in order {
        let c = &cands[i];
        let y = clamp_round(c.yx.0 / cell, ny);
        let x = clamp_round(c.yx.1 / cell, nx);
        if occ[y * nx + x] && !c.reserved {
            continue;
        }
        out.push(c.clone());
        if out.len() >= cap {
            break;
        }
        stamp(&mut occ, nx, ny, y, x, sep);
    }
    out
}

/// `int(round(v))` clamped into `[0, n)` (the prototype indexes the raw
/// value; every candidate lies inside the grid, the clamp is defensive).
fn clamp_round(v: f64, n: usize) -> usize {
    round_half_even(v).clamp(0, n as i64 - 1) as usize
}

/// `occ[max(0, y - sep):y + sep + 1, max(0, x - sep):x + sep + 1] = True`
/// (numpy clips the upper bound).
fn stamp(occ: &mut [bool], nx: usize, ny: usize, y: usize, x: usize, sep: usize) {
    let y0 = y.saturating_sub(sep);
    let y1 = (y + sep + 1).min(ny);
    let x0 = x.saturating_sub(sep);
    let x1 = (x + sep + 1).min(nx);
    for yy in y0..y1 {
        for xx in x0..x1 {
            occ[yy * nx + xx] = true;
        }
    }
}

/// The setting features every cell can supply from the 8 m stack (shared
/// by `field_score` and the final score): tpi, relief, room, persistence,
/// saddle, the six class indicators, the water band, the boundary term.
fn setting_feats(f: &Fields, m: &Morphology, p: &Persistence, tpi60: &[f64], k: usize,
                 d_edge: f64, sqa: f64, feats: &mut [f64; N_FEATURES]) {
    feats[F_TPI60] = tpi60[k];
    feats[F_TPI200] = f.tpi200[k];
    feats[F_RELIEF_POS] = f.relief_pos[k];
    feats[F_ROOM] = f.room[k].min(60.0);
    feats[F_PIT] = p.pit[k].min(10.0).ln_1p();
    feats[F_PEAK] = p.peak[k].min(20.0).ln_1p();
    feats[F_SADDLE] = if m.saddle[k] { 1.0 } else { 0.0 };
    let c240 = m.cls240[k];
    let c80 = m.cls80[k];
    feats[F_CLS240_FLAT] = if c240 == 1 { 1.0 } else { 0.0 };
    feats[F_CLS240_CONVEX] = if cls_convex(c240) { 1.0 } else { 0.0 };
    feats[F_CLS240_CONCAVE] = if cls_concave(c240) { 1.0 } else { 0.0 };
    feats[F_CLS80_FLAT] = if c80 == 1 { 1.0 } else { 0.0 };
    feats[F_CLS80_CONVEX] = if cls_convex(c80) { 1.0 } else { 0.0 };
    feats[F_CLS80_CONCAVE] = if cls_concave(c80) { 1.0 } else { 0.0 };
    // `min(inf, 400)` = 400 on a dry tile
    feats[F_D_WATER_BAND] = f.d_water[k].min(400.0);
    feats[F_D_BOUNDARY_NORM] = d_edge / sqa;
}

/// `greens.field_score(f, m, p, sit, tpi60, i0, i1, j0, j1)`: the fitted
/// score restricted to its FIELD-computable terms, evaluated over
/// `[i0:i1, j0:j1]` at 8 m. This is the PROPOSER (2026-08-29): measured on
/// held-out courses, real greens score at the 100th percentile of the pool
/// the typed detectors used to propose -- the score knew where greens
/// belong and seeding never offered those cells. Approach-table terms
/// (vis/recept/backdrop/aroom) are omitted here and refine the survivors.
/// Aspect uses the 8 m gradient OF THE SLAB (one-sided at the slab's own
/// edges, as `np.gradient(f.z8[sl])` was) as a stand-in for the
/// confirm-plane fit. Surround is the p90 - p10 percentile filter over a
/// CELL-radius ring (`3 <= rho <= 11` cells, not the world-metre annulus
/// of `surround_relief`) on the halo-padded slab with `nearest` edges.
/// Returns the full-grid array, `-inf` outside the slab, so callers can
/// argmax over the full shape.
pub fn field_score(f: &Fields, m: &Morphology, p: &Persistence, sit: &Siting, tpi60: &[f64],
                   i0: usize, i1: usize, j0: usize, j1: usize) -> Vec<f64> {
    let (nx, ny) = (f.nx, f.ny);
    let mut out = vec![f64::NEG_INFINITY; nx * ny];
    let sh = i1 - i0;
    let sw = j1 - j0;
    if sh == 0 || sw == 0 {
        return out;
    }
    // surround relief: p90-p10 of z8 over the 25-90 m annulus, exact
    let rr0 = round_half_even(SURROUND_R0_M / f.cell) as usize; // 3
    let rr1 = round_half_even(SURROUND_R1_M / f.cell) as usize; // 11
    let za0 = i0.saturating_sub(rr1);
    let za1 = (i1 + rr1).min(ny);
    let zb0 = j0.saturating_sub(rr1);
    let zb1 = (j1 + rr1).min(nx);
    let (zh, zw) = (za1 - za0, zb1 - zb0);
    let mut zs = Vec::with_capacity(zh * zw);
    for y in za0..za1 {
        zs.extend_from_slice(&f.z8[y * nx + zb0..y * nx + zb1]);
    }
    // ring `rr0^2 <= rho^2 <= rr1^2` in cells; the kernel's footprint is
    // `r_in < rho <= r_out`, so r_in sits strictly between the integer
    // radii-squared `rr0^2 - 1` and `rr0^2`
    let r_in = ((rr0 * rr0) as f64 - 0.5).sqrt();
    let r_out = rr1 as f64;
    let p90 = img::percentile_filter_ring(&zs, zw, zh, r_in, r_out, 90.0);
    let p10 = img::percentile_filter_ring(&zs, zw, zh, r_in, r_out, 10.0);
    let oy = i0 - za0;
    let ox = j0 - zb0;

    // the slab's own gradient
    let mut slab = Vec::with_capacity(sh * sw);
    for y in i0..i1 {
        slab.extend_from_slice(&f.z8[y * nx + j0..y * nx + j1]);
    }
    let (gy8, gx8) = img::gradient(&slab, sw, sh, f.cell);

    let (win_y0, win_x0, win_h, win_w) = sit.window_m;
    let sqa = (win_h * win_w).max(1.0).sqrt();

    for r in 0..sh {
        let ys = (i0 + r) as f64 * f.cell;
        for c in 0..sw {
            let xs = (j0 + c) as f64 * f.cell;
            let k = (i0 + r) * nx + (j0 + c);
            let d_edge = (ys - win_y0).min(win_y0 + win_h - ys)
                .min((xs - win_x0).min(win_x0 + win_w - xs));
            let mut feats = [0.0f64; N_FEATURES];
            setting_feats(f, m, p, tpi60, k, d_edge, sqa, &mut feats);
            let kz = (oy + r) * zw + (ox + c);
            feats[F_SURROUND] = p90[kz] - p10[kz];
            let ks = r * sw + c;
            let mut gm = gy8[ks].hypot(gx8[ks]);
            if gm < 1e-6 {
                gm = 1.0;
            }
            feats[F_ASPECT_SIN] = gy8[ks] / gm;
            feats[F_ASPECT_COS] = gx8[ks] / gm;
            out[k] = fitted_score(&feats);
        }
    }
    out
}

/// The "hot" seeds of `generate`: the field score's top cells of the
/// window+halo slab regardless of type, 40 m stamp-thinned, at most
/// `n_max`. The order is `np.argsort(fs, kind="stable")[::-1]` -- score
/// descending and, among EQUAL scores, the later raster index first (the
/// reversal of a stable ascending sort). Non-finite cells end the scan.
/// Returns slab-local `(yy, xx)`.
pub fn hot_seeds(fs: &[f64], sw: usize, sh: usize, sep_h: usize, n_max: usize) -> Vec<(usize, usize)> {
    let mut order: Vec<usize> = (0..sw * sh).collect();
    order.sort_by(|&a, &b| cmp_f(fs[a], fs[b]).then(a.cmp(&b)));
    order.reverse();
    let mut occ = vec![false; sw * sh];
    let mut out = Vec::new();
    for idx in order {
        if out.len() >= n_max || !fs[idx].is_finite() {
            break;
        }
        let (yy, xx) = (idx / sw, idx % sw);
        if occ[idx] {
            continue;
        }
        stamp(&mut occ, sw, sh, yy, xx, sep_h);
        out.push((yy, xx));
    }
    out
}

/// The distance-spectrum repair of `generate`: every hole-length band
/// (`0.8 L < d < 1.2 L` for each distinct `L` = length rounded to tens,
/// ascending) must be coverable by some pair of the thinned pool; a band
/// nobody covers is back-filled with the first candidate of `rest` (score
/// order) that lands a pair with the CURRENT pool. The pair matrix is the
/// prototype's: built once from the pool as it stood before the loop, so a
/// candidate back-filled for one band does not count toward the next
/// band's test (only toward its back-fill search).
pub fn spectrum_repair(thinned: &mut Vec<Candidate>, rest: &[Candidate], hole_lengths: &[f64]) {
    if thinned.len() < 2 {
        return;
    }
    let n0 = thinned.len();
    let mut dm = vec![0.0f64; n0 * n0];
    for i in 0..n0 {
        for j in 0..n0 {
            dm[i * n0 + j] = (thinned[i].yx.0 - thinned[j].yx.0).hypot(thinned[i].yx.1 - thinned[j].yx.1);
        }
    }
    // `sorted(set(np.round(hole_lengths, -1)))`: rint(L / 10) * 10, half
    // to even, distinct, ascending
    let mut bands: Vec<i64> = hole_lengths.iter()
        .map(|&l| round_half_even(l / 10.0) * 10).collect();
    bands.sort_unstable();
    bands.dedup();
    for lb in bands {
        let l = lb as f64;
        let (lo, hi) = (0.8 * l, 1.2 * l);
        let covered = dm.iter().any(|&d| d > lo && d < hi);
        if covered {
            continue;
        }
        // back-fill from the rejected list with anything that lands a pair
        for c in rest {
            let lands = thinned.iter().any(|q| {
                let d = (c.yx.0 - q.yx.0).hypot(c.yx.1 - q.yx.1);
                d > lo && d < hi
            });
            if lands {
                thinned.push(c.clone());
                break;
            }
        }
    }
}

/// `greens.generate(z2, cell2, wet2, sit, f, m, p, n_target, hole_lengths)`.
///
/// Stages, in the prototype's order: typed seeds over window + halo; the
/// field score's hot seeds; one kind per cell (first claim wins); per-kind
/// 40 m stamp thinning (kinds in name order, cap 90 each); 2 m
/// confirmation; approach tables and the full fitted score (minus
/// `W_GRADED` for graded pads); the loop anchor near the clubhouse's
/// reserved green (force-added if nothing confirmed within 180 m); per-type
/// quotas (floor of 5, remainder by score); 50 m stamp thinning to
/// `n_target + 25`; the distance-spectrum repair; the final cap at
/// `max(n_target + 25, 75)`.
pub fn generate(t: &Terrain, sit: &Siting, f: &Fields, m: &Morphology, p: &Persistence,
                n_target: usize, hole_lengths: Option<&[f64]>) -> Vec<Candidate> {
    const DEFAULT_LENGTHS: [f64; 9] = [350., 360., 160., 480., 370., 170., 355., 490., 365.];
    let hole_lengths: &[f64] = hole_lengths.unwrap_or(&DEFAULT_LENGTHS);
    let (nx, ny) = (f.nx, f.ny);
    let (i0, j0) = sit.window_ij;
    let h = round_half_even(sit.window_m.2 / f.cell) as usize;
    let w = round_half_even(sit.window_m.3 / f.cell) as usize;

    let mut seeds = seed_typed(f, m, p, i0, j0, h, w);

    // PROPOSER (2026-08-29): the typed detectors alone never offered the
    // cells the fitted score rates highest (real greens sat at the pool's
    // 100th score percentile). The field score now (a) contributes "hot"
    // seeds -- its top cells regardless of type -- and (b) replaces the old
    // room/40 heuristic as the pre-confirm thinning key.
    let halo = round_half_even(120.0 / f.cell) as usize;
    let a0 = i0.saturating_sub(halo);
    let a1 = (i0 + h + halo).min(ny);
    let b0 = j0.saturating_sub(halo);
    let b1 = (j0 + w + halo).min(nx);
    let size60 = (round_half_even(60.0 / f.cell) as usize) | 1; // 9
    let lp60 = img::uniform_filter(&f.z8, nx, ny, size60);
    let tpi60: Vec<f64> = f.z8.iter().zip(&lp60).map(|(z, l)| z - l).collect();
    let fscore = field_score(f, m, p, sit, &tpi60, a0, a1, b0, b1);
    let (sh, sw) = (a1 - a0, b1 - b0);
    let mut fs = vec![f64::NEG_INFINITY; sh * sw];
    for yy in 0..sh {
        for xx in 0..sw {
            let k = (a0 + yy) * nx + (b0 + xx);
            fs[yy * sw + xx] = if f.wet8[k] { f64::NEG_INFINITY } else { fscore[k] };
        }
    }
    let sep_h = round_half_even(40.0 / f.cell).max(1) as usize;
    for (yy, xx) in hot_seeds(&fs, sw, sh, sep_h, 150) {
        seeds.push((a0 + yy, b0 + xx, "hot"));
    }
    if seeds.is_empty() {
        return Vec::new();
    }

    // score seeds cheaply at 8 m: room + type prior. SPREAD BEFORE CONFIRM:
    // rank-truncating globally by room takes the 400 roomiest cells, which
    // all sit in the interiors of the few biggest pads -- measured, the 50 m
    // thinning then collapsed 400 confirmed candidates to 29. Stamp-thin per
    // type at 40 m first, so what reaches the 2 m gate is already spatially
    // diverse, then confirm.
    //
    // `pre`: one entry per cell in first-seen order; a later seed replaces
    // it only on a STRICTLY higher score, so the first detector to claim a
    // cell names its kind.
    let mut pre: Vec<(usize, usize, f64, &'static str)> = Vec::new();
    let mut pre_idx: HashMap<(usize, usize), usize> = HashMap::new();
    for &(y, x, kind) in &seeds {
        let v = fscore[y * nx + x];
        let s = if v.is_finite() { v } else { -1e9 };
        match pre_idx.get(&(y, x)) {
            Some(&i) => {
                if s > pre[i].2 {
                    pre[i] = (y, x, s, kind);
                }
            }
            None => {
                pre_idx.insert((y, x), pre.len());
                pre.push((y, x, s, kind));
            }
        }
    }
    // kinds in name order (`sorted(by_kind.items())`), each list in `pre`
    // order, thinned by (score desc, y, x) stable at 40 m, at most 90
    let mut by_kind: BTreeMap<&'static str, Vec<(f64, usize, usize)>> = BTreeMap::new();
    for &(y, x, s, kind) in &pre {
        by_kind.entry(kind).or_default().push((s, y, x));
    }
    let mut ranked: Vec<(usize, usize, f64, &'static str)> = Vec::new();
    let sep = round_half_even(40.0 / f.cell).max(1) as usize;
    for (kind, lst) in by_kind.iter() {
        let mut order: Vec<usize> = (0..lst.len()).collect();
        order.sort_by(|&a, &b| {
            cmp_f(lst[b].0, lst[a].0)
                .then(lst[a].1.cmp(&lst[b].1))
                .then(lst[a].2.cmp(&lst[b].2))
                .then(a.cmp(&b))
        });
        let mut occ = vec![false; nx * ny];
        let mut n_took = 0usize;
        for i in order {
            let (s, y, x) = lst[i];
            if occ[y * nx + x] {
                continue;
            }
            ranked.push((y, x, s, kind));
            stamp(&mut occ, nx, ny, y, x, sep);
            n_took += 1;
            if n_took >= 90 {
                break;
            }
        }
    }

    let (win_y0, win_x0, win_h_m, win_w_m) = sit.window_m;
    let win_sqrt_area = (win_h_m * win_w_m).max(1.0).sqrt();

    let mut cands: Vec<Candidate> = Vec::new();
    for &(y, x, _s0, kind) in &ranked {
        let y_m = y as f64 * f.cell;
        let x_m = x as f64 * f.cell;
        let (class, grad, _resid) = confirm_2m(t, y_m, x_m);
        let build = match class {
            Some(b) => b,
            None => continue,
        };
        let tab = approach_table(f, y, x, grad);
        let k = y * nx + x;
        let (gy, gx) = grad;
        let gmag = gy.hypot(gx);
        // window-edge distance stands in for the fitted course-boundary term
        let d_edge = (y_m - win_y0).min(win_y0 + win_h_m - y_m)
            .min(x_m - win_x0).min(win_x0 + win_w_m - x_m);
        let mut feats = [0.0f64; N_FEATURES];
        setting_feats(f, m, p, &tpi60, k, d_edge, win_sqrt_area, &mut feats);
        let col = |i: usize| tab.iter().map(move |r| r[i]);
        feats[F_VIS_BEST] = col(0).fold(f64::NEG_INFINITY, f64::max);
        feats[F_VIS_MEAN] = col(0).sum::<f64>() / N_BEARINGS as f64;
        feats[F_RECEPT_BEST] = col(1).map(|v| v.clamp(-0.05, 0.05))
            .fold(f64::NEG_INFINITY, f64::max) / 0.05;
        feats[F_BACKDROP_BEST] = col(2).map(|v| v.clamp(0.0, 8.0))
            .fold(f64::NEG_INFINITY, f64::max) / 8.0;
        feats[F_BACKDROP_FAR] = backdrop_far(f, y, x).clamp(-10.0, 10.0);
        feats[F_AROOM_BEST] = col(3).fold(f64::NEG_INFINITY, f64::max);
        feats[F_AROOM_MEAN] = col(3).sum::<f64>() / N_BEARINGS as f64;
        feats[F_SURROUND] = surround_relief(&f.z8, nx, ny, f.cell, y, x);
        feats[F_ASPECT_SIN] = if gmag > 1e-6 { gy / gmag } else { 0.0 };
        feats[F_ASPECT_COS] = if gmag > 1e-6 { gx / gmag } else { 0.0 };
        let score = fitted_score(&feats) - if build == Build::Graded { W_GRADED } else { 0.0 };
        cands.push(Candidate {
            yx: (y_m, x_m),
            kind: kind.to_string(),
            score,
            grad,
            persistence: p.pit[k].max(p.peak[k]),
            approach: tab,
            reserved: false,
            build,
        });
    }

    // reserve the loop anchor: the candidate nearest the clubhouse's
    // reserved green, protected from thinning
    if !cands.is_empty() {
        let (gy, gx) = sit.clubhouse.reserved_green;
        let mut best = 0usize;
        let mut best_d = f64::INFINITY;
        for (i, c) in cands.iter().enumerate() {
            let d = (c.yx.0 - gy).hypot(c.yx.1 - gx);
            if d < best_d {
                best_d = d;
                best = i;
            }
        }
        if best_d <= CLUBHOUSE_RADIUS_M * 1.5 {
            cands[best].reserved = true;
        } else {
            // no confirmed candidate near home: force-add the reserved pad
            // (the prototype keeps the plane gradient whatever the pad's
            // verdict, and the default `natural` build)
            let (_class, grad, _resid) = confirm_2m(t, gy, gx);
            let ry = clamp_round(gy / f.cell, ny);
            let rx = clamp_round(gx / f.cell, nx);
            let tab = approach_table(f, ry, rx, grad);
            cands.push(Candidate {
                yx: (gy, gx),
                kind: "generic".to_string(),
                score: 0.5,
                grad,
                persistence: 0.0,
                approach: tab,
                reserved: true,
                build: Build::Natural,
            });
        }
    }

    // type quotas: floor of 5 per present type, remainder by score. Types
    // in first-appearance order (name order from `ranked`, a forced
    // "generic" anchor joining or founding its list at the end).
    let mut by_type: Vec<(&str, Vec<usize>)> = Vec::new();
    for (i, c) in cands.iter().enumerate() {
        match by_type.iter_mut().find(|(k, _)| *k == c.kind.as_str()) {
            Some((_, v)) => v.push(i),
            None => by_type.push((c.kind.as_str(), vec![i])),
        }
    }
    let key_desc = |a: usize, b: usize| {
        let (ca, cb) = (&cands[a], &cands[b]);
        cb.reserved.cmp(&ca.reserved)
            .then(cmp_f(cb.score, ca.score))
            .then(cmp_f(ca.yx.0, cb.yx.0))
            .then(cmp_f(ca.yx.1, cb.yx.1))
            .then(a.cmp(&b))
    };
    let mut pool_idx: Vec<usize> = Vec::new();
    let mut in_pool = vec![false; cands.len()];
    for (_, lst) in by_type.iter_mut() {
        lst.sort_by(|&a, &b| key_desc(a, b));
        for &i in lst.iter().take(5) {
            pool_idx.push(i);
            in_pool[i] = true;
        }
    }
    let mut rest_idx: Vec<usize> = (0..cands.len()).filter(|&i| !in_pool[i]).collect();
    rest_idx.sort_by(|&a, &b| {
        let (ca, cb) = (&cands[a], &cands[b]);
        cmp_f(cb.score, ca.score)
            .then(cmp_f(ca.yx.0, cb.yx.0))
            .then(cmp_f(ca.yx.1, cb.yx.1))
            .then(a.cmp(&b))
    });
    let rest: Vec<Candidate> = rest_idx.iter().map(|&i| cands[i].clone()).collect();
    let mut pool: Vec<Candidate> = pool_idx.iter().map(|&i| cands[i].clone()).collect();
    pool.extend(rest.iter().cloned());

    let mut thinned = stamp_thin(&pool, f.cell, nx, ny, 50.0, n_target + 25);

    // distance-spectrum repair: every hole-length band must be coverable
    spectrum_repair(&mut thinned, &rest, hole_lengths);
    thinned.truncate((n_target + 25).max(75));
    thinned
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(y: f64, x: f64, score: f64, reserved: bool) -> Candidate {
        Candidate {
            yx: (y, x),
            kind: "generic".to_string(),
            score,
            grad: (0.0, 0.0),
            persistence: 0.0,
            approach: [[0.0; 4]; N_BEARINGS],
            reserved,
            build: Build::Natural,
        }
    }

    fn synth_fields(nx: usize, ny: usize, z8: Vec<f64>) -> Fields {
        let n = nx * ny;
        Fields {
            cell: 8.0,
            nx,
            ny,
            z8,
            slope: vec![0.0; n],
            relief_pos: vec![0.5; n],
            tpi200: vec![0.0; n],
            subgrid_rough: vec![0.0; n],
            wet8: vec![false; n],
            d_water: vec![f64::INFINITY; n],
            pad_green: vec![true; n],
            pad_tee: vec![true; n],
            pad_fair: vec![true; n],
            room: vec![100.0; n],
        }
    }

    /// The logistic on a hand-built feature vector, against a Python
    /// evaluation of `greens.py`'s own `FIT_B0 + sum(FIT_COEF[k] * v) +
    /// sum(FIT_COEF_SQ[k] * v * v)` on the same values (run 2026-09-14,
    /// numpy 2.1.2). The vector is `0.1 * (i + 1)` scaled per feature so
    /// every coefficient, linear and quadratic, is exercised at a distinct
    /// value; an order mistake in either table moves the sum by >> 1e-9.
    #[test]
    fn fitted_score_matches_python() {
        let mut feats = [0.0f64; N_FEATURES];
        for (i, v) in feats.iter_mut().enumerate() {
            *v = 0.1 * (i as f64 + 1.0) * if i % 2 == 0 { 1.0 } else { -0.5 };
        }
        let got = fitted_score(&feats);
        let expected = PYTHON_EXPECTED;
        assert!((got - expected).abs() < 1e-9, "got {got:.12}, python {expected:.12}");
        // the JSON's full-precision coefficients agree to the transcription's
        // rounding (4 / 6 decimals); the prototype is the reference
        assert!((got - PYTHON_EXPECTED_JSON).abs() < 2e-3);
    }
    // the prototype's values less d_boundary_norm's zeroed pair on the
    // vector's feats[8] = 0.9: -2.3707 * 0.9 + 7.052632 * 0.81 = 3.57900192
    const PYTHON_EXPECTED: f64 = 4.133595839999993 - 3.57900192;
    const PYTHON_EXPECTED_JSON: f64 = 4.13342945899071 - 3.57900192;

    #[test]
    fn feature_tables_are_consistent() {
        assert_eq!(FEATURE_NAMES[F_RELIEF_POS], "relief_pos");
        assert_eq!(FEATURE_NAMES[F_AROOM_MEAN], "aroom_mean");
        assert_eq!(FEATURE_NAMES[F_SADDLE], "saddle");
        assert_eq!(FIT_COEF[F_SADDLE], 0.0);
        assert_eq!(FIT_COEF[F_RECEPT_BEST], 7.7403);
        let mut seen = [false; N_FEATURES];
        for &(i, _) in FIT_COEF_SQ.iter() {
            assert!(!seen[i], "duplicate quadratic on {}", FEATURE_NAMES[i]);
            seen[i] = true;
        }
        // the nine features without a quadratic
        for name in ["aspect_sin", "aspect_cos", "saddle", "cls240_flat", "cls240_convex",
                     "cls240_concave", "cls80_flat", "cls80_convex", "cls80_concave"] {
            let i = FEATURE_NAMES.iter().position(|n| *n == name).unwrap();
            assert!(!seen[i], "{name} should carry no quadratic");
        }
    }

    #[test]
    fn approach_bearing_geometry() {
        // bearing k: u = (sin th, cos th); k = 0 east, 4 north (+y), 8
        // west, 12 south (-y)
        let (uy, ux) = bearing_unit(0);
        assert!((ux - 1.0).abs() < 1e-15 && uy.abs() < 1e-15);
        let (uy, ux) = bearing_unit(4);
        assert!((uy - 1.0).abs() < 1e-15 && ux.abs() < 1e-15);
        let (uy, _) = bearing_unit(12);
        assert!((uy + 1.0).abs() < 1e-15);

        // flat ground, all fairway: every stance sees the surface, no
        // backdrop, full corridor room; the pad tilting up toward +y
        // releases the golfer approaching from +y (k = 4) and holds the one
        // from -y (k = 12)
        let (nx, ny) = (80, 80);
        let f = synth_fields(nx, ny, vec![0.0; nx * ny]);
        let tab = approach_table(&f, 40, 40, (0.02, 0.0));
        for row in tab.iter() {
            assert_eq!(row[0], 1.0);
            assert_eq!(row[2], 0.0);
            assert_eq!(row[3], 1.0);
        }
        assert!((tab[4][1] + 0.02).abs() < 1e-12);
        assert!((tab[12][1] - 0.02).abs() < 1e-12);
        assert!(tab[0][1].abs() < 1e-12);

        // a 10 m wall five cells north of the green (five cells wide, so
        // the oblique bearings miss it) blinds the northern approach and is
        // the southern approach's backdrop (rows 4..=10 of the 30-80 m
        // band, one of the seven cells on the wall)
        let mut z = vec![0.0; nx * ny];
        for x in 38..=42 {
            z[45 * nx + x] = 10.0;
        }
        let f = synth_fields(nx, ny, z);
        let tab = approach_table(&f, 40, 40, (0.0, 0.0));
        assert_eq!(tab[4][0], 0.0);
        assert_eq!(tab[12][0], 1.0);
        assert_eq!(tab[0][0], 1.0);
        assert!((tab[12][2] - 10.0 / 7.0).abs() < 1e-12);
        assert_eq!(tab[4][2], 0.0);
        // backdrop_far: 80-160 m beyond along -u, bearing 4 looks south
        // (nothing), bearing 12 looks north through the wall at 40 m --
        // outside its 80 m floor, so nothing either
        assert!(backdrop_far(&f, 40, 40).abs() < 1e-12);
        let mut z = vec![0.0; nx * ny];
        for x in 38..=42 {
            z[52 * nx + x] = 11.0; // 96 m north: inside the 80-160 m band
        }
        let f = synth_fields(nx, ny, z);
        assert!((backdrop_far(&f, 40, 40) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn clip_idx_truncates_then_clamps() {
        assert_eq!(clip_idx(-0.4, 10), 0);
        assert_eq!(clip_idx(-3.7, 10), 0);
        assert_eq!(clip_idx(3.9, 10), 3);
        assert_eq!(clip_idx(12.0, 10), 9);
        assert_eq!(round_half_even(2.5), 2);
        assert_eq!(round_half_even(3.5), 4);
    }

    #[test]
    fn stamp_thin_greedy_order_and_reserved() {
        // 8 m cells, 50 m separation -> sep 6 cells: the stamp covers
        // |dy|, |dx| <= 6 cells (48 m). Candidates at 0, 40 m (blocked by
        // the best), 56 m (clear) and a reserved low scorer that comes out
        // first regardless of score.
        let cands = vec![
            cand(0.0, 0.0, 1.0, false),
            cand(0.0, 40.0, 0.9, false),
            cand(0.0, 56.0, 0.8, false),
            cand(400.0, 400.0, -5.0, true),
            cand(200.0, 200.0, 0.85, false),
        ];
        let out = stamp_thin(&cands, 8.0, 64, 64, 50.0, 100);
        let ys: Vec<(f64, f64)> = out.iter().map(|c| c.yx).collect();
        assert_eq!(ys, vec![(400.0, 400.0), (0.0, 0.0), (200.0, 200.0), (0.0, 56.0)]);
        // a reserved candidate stamps too, and is itself never blocked
        let cands = vec![
            cand(0.0, 0.0, 1.0, false),
            cand(8.0, 8.0, -5.0, true),
            cand(16.0, 16.0, -6.0, true),
        ];
        let out = stamp_thin(&cands, 8.0, 64, 64, 50.0, 100);
        let ys: Vec<(f64, f64)> = out.iter().map(|c| c.yx).collect();
        assert_eq!(ys, vec![(8.0, 8.0), (16.0, 16.0)]);
        // cap stops the scan
        let out = stamp_thin(&cands, 8.0, 64, 64, 50.0, 2);
        assert_eq!(out.len(), 2);
        // equal scores: (y, x) ascending, then input order
        let cands = vec![
            cand(80.0, 0.0, 0.5, false),
            cand(0.0, 80.0, 0.5, false),
            cand(0.0, 0.0, 0.5, false),
        ];
        let out = stamp_thin(&cands, 8.0, 64, 64, 50.0, 100);
        let ys: Vec<(f64, f64)> = out.iter().map(|c| c.yx).collect();
        assert_eq!(ys, vec![(0.0, 0.0), (0.0, 80.0), (80.0, 0.0)]);
    }

    #[test]
    fn hot_seeds_prefer_the_later_index_on_ties() {
        // argsort(stable)[::-1]: equal scores come out later-index first
        let fs = vec![1.0, 2.0, 2.0, f64::NEG_INFINITY, 0.5, 2.0];
        let out = hot_seeds(&fs, 3, 2, 0, 10);
        assert_eq!(out, vec![(1, 2), (0, 2), (0, 1), (0, 0), (1, 1)]);
        // the stamp blocks the 8-neighbourhood, the scan stops at -inf
        let out = hot_seeds(&fs, 3, 2, 1, 10);
        assert_eq!(out, vec![(1, 2), (0, 0)]);
        let out = hot_seeds(&fs, 3, 2, 0, 2);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn spectrum_repair_backfills_uncovered_bands() {
        let mut thinned = vec![cand(0.0, 0.0, 1.0, false), cand(0.0, 100.0, 0.9, false)];
        let rest = vec![cand(500.0, 500.0, 0.5, false), cand(0.0, 360.0, 0.4, false)];
        // 355 rounds (half to even) to 360: band (288, 432); only the second
        // rest candidate lands a pair; 100 m band (80, 120) is covered
        spectrum_repair(&mut thinned, &rest, &[355.0, 100.0]);
        assert_eq!(thinned.len(), 3);
        assert_eq!(thinned[2].yx, (0.0, 360.0));
        // nothing in rest lands: the pool is unchanged
        let mut thinned = vec![cand(0.0, 0.0, 1.0, false), cand(0.0, 100.0, 0.9, false)];
        spectrum_repair(&mut thinned, &rest, &[1000.0]);
        assert_eq!(thinned.len(), 2);
    }

    #[test]
    fn hole_length_rounding_is_half_even() {
        let r: Vec<i64> = [350., 360., 160., 480., 370., 170., 355., 490., 365.]
            .iter().map(|&l| round_half_even(l / 10.0) * 10).collect();
        assert_eq!(r, vec![350, 360, 160, 480, 370, 170, 360, 490, 360]);
    }

    fn synth_terrain(nx: usize, ny: usize, zf: impl Fn(f64, f64) -> f64) -> Terrain {
        use course_world::grid::{Grid, GridSpec};
        use course_world::math::Vec2;
        let spec = GridSpec::new(Vec2::new(0.0, 0.0), 2.0, nx as u32, ny as u32);
        let mut z = Grid::filled(spec, 0.0);
        for y in 0..ny {
            for x in 0..nx {
                z.data[y * nx + x] = zf(y as f64 * 2.0, x as f64 * 2.0);
            }
        }
        Terrain { mode: crate::Mode::Aeolian, z2: z, wet2: vec![false; nx * ny], lines: Vec::new() }
    }

    #[test]
    #[ignore = "needs img::plane_fit"]
    fn confirm_2m_classifies_planes() {
        // a 1 % plane is natural with its gradient recovered
        let t = synth_terrain(64, 64, |y, _x| 0.01 * y);
        let (class, (gy, gx), resid) = confirm_2m(&t, 64.0, 64.0);
        assert_eq!(class, Some(Build::Natural));
        assert!((gy - 0.01).abs() < 1e-9 && gx.abs() < 1e-9 && resid < 1e-9);
        // a 7 % plane is graded (relief over the 10 m disc ~1.4 m)
        let t = synth_terrain(64, 64, |y, _x| 0.07 * y);
        let (class, _, _) = confirm_2m(&t, 64.0, 64.0);
        assert_eq!(class, Some(Build::Graded));
        // 9 % fails
        let t = synth_terrain(64, 64, |y, _x| 0.09 * y);
        let (class, (gy, _), _) = confirm_2m(&t, 64.0, 64.0);
        assert_eq!(class, None);
        assert!((gy - 0.09).abs() < 1e-9);
        // out of bounds and water fail with a zero gradient
        assert_eq!(confirm_2m(&t, 10.0, 64.0), (None, (0.0, 0.0), 0.0));
        let mut t = synth_terrain(64, 64, |_, _| 0.0);
        t.wet2[32 * 64 + 32] = true;
        assert_eq!(confirm_2m(&t, 64.0, 64.0), (None, (0.0, 0.0), 0.0));
    }

    #[test]
    #[ignore = "needs img::percentile"]
    fn surround_relief_of_a_plane() {
        // z = 0.05 * x over 8 m cells: the 25-90 m annulus spans +-90 m in x,
        // so p90 - p10 is about 0.8 * 2 * 4.5 m; the edge returns 0
        let (nx, ny) = (60, 60);
        let mut z = vec![0.0; nx * ny];
        for y in 0..ny {
            for x in 0..nx {
                z[y * nx + x] = 0.05 * x as f64 * 8.0;
            }
        }
        let s = surround_relief(&z, nx, ny, 8.0, 30, 30);
        assert!(s > 6.0 && s < 8.0, "{s}");
        assert_eq!(surround_relief(&z, nx, ny, 8.0, 5, 30), 0.0);
    }
}
