//! The creek planform, GROWN rather than authored.
//!
//! ## Why this replaces `meander.rs`
//!
//! The retired module's header closed with: *"it keeps the existing
//! offset-from-centre-line formulation (the creek must stay in its valley — a
//! free path cannot be constrained that way)."* That sentence is why options
//! A+B were chosen (Kinoshita skew and flatness, modulated amplitude,
//! 783f004), and it is also why they could not work. An offset from a smooth
//! base line is a CLOSED-FORM PERIODIC FUNCTION of arc length, and every dial
//! we had modulated that function without making it aperiodic. The eye reads
//! the carrier, not the modulation. Review, twice: "still looks artificial ...
//! just due to the sinusoidal nature".
//!
//! The answer to that sentence is not "so keep the offset". It is that **a
//! free path CAN be constrained — against a field**. Both callers have one, or
//! can retain one: the same normalised valley coordinate the cut itself uses
//! to decide where the floor ends.
//!
//! The repo learned the same lesson one stage over, in
//! `course-skeleton/src/fluvial/carve.rs`: *"The paradigm was the bug:
//! geometry authored in path space has to be argued into consistency with the
//! terrain it sits on."* The old creek was exactly that — an authored offset
//! defended by `limit_curvature`, a swing-fraction clamp and `amp_cap_scale`.
//!
//! ## Why this is not the LEM `docs/sandhills/README.md:16` rejected
//!
//! That objection was to an unbounded terrain simulation: *"a LEM cannot be
//! bounded in a sub-10 s no-retry budget."* This is a different animal. It is
//! FIXED-ITERATION (`ITERS`, never "until converged"), runs on a ~300-node
//! polyline rather than a 1.5 M-cell grid, draws NOTHING inside the loop, and
//! costs ~3 M operations — under 10 ms against a 1.6 s tile build. The budget
//! objection does not reach it.
//!
//! ## The model
//!
//! Ikeda–Parker–Sawai (1981) linearised bend theory with the Howard–Knutson
//! (1984) reduction: bank migration rate is a weighted sum of the LOCAL
//! curvature and an exponentially-weighted integral of the UPSTREAM curvature.
//! Hickin–Nanson nonlinearity damps migration for very tight bends, so loops
//! stop growing instead of running away. A paired diffusive term keeps the
//! discretisation honest — `carve.rs` records that stream power alone "etches
//! a rectilinear comb ... this is that missing half of the erosion law", and
//! the same holds here: migration alone grows a node-scale zigzag.
//!
//! Everything is written in CHANNEL WIDTHS, which is what lets one set of
//! constants serve a 4 m Carolina blackwater creek and a 10 m Nebraska trunk.

use course_world::math::{self, Vec2};
use course_world::noise;

// --- numerics --------------------------------------------------------------

/// Fixed iteration count. Never "until converged" — stage-02's budget rule,
/// stated at `carve.rs:51`.
pub const ITERS: usize = 120;
/// Node spacing, in channel widths.
pub const DS_WIDTHS: f64 = 0.5;
/// Per-iteration displacement ceiling, in channel widths. The stability
/// analogue of `carve.rs`'s `STEP_CLAMP_M = 0.45`: one step can never move a
/// node far enough to invert its neighbours.
pub const STEP_CLAMP_W: f64 = 0.08;
/// Migration length per iteration at unit rate, in channel widths. PHYSICAL,
/// not per-node: the first version used `ds*0.5`, which made the step -- and
/// therefore the emergent wavelength -- track the node spacing (measured
/// lambda 120/133/171 m at ds = W/4, W/2, W).
/// Calibrated on the ds-sweep (`probe` in this module's history), which is
/// the only dial that matters for how meandering the result is:
///     MIG   sinuosity at ds = W/6, W/4, W/3
///     0.09  1.018 1.018 1.017   (barely bends -- the first shipped guess)
///     0.25  1.070 1.077 1.070
///     0.50  1.289 1.364 1.294
///     0.70  1.188 1.229 1.136  (SHIPPED -- lambda 51-68 m = 8.5-11 W)
///     1.00  1.659 1.716 1.676   (implausibly serpentine)
/// Real creeks run 1.15-1.30, so 0.45. Note sinuosity is FLAT in ds at every
/// setting -- that resolution-independence is the evidence the instability is
/// physical rather than a grid mode.
pub const MIG_W_PER_ITER: f64 = 0.7;
/// Diffusivity per iteration, in units of W^2. Also physical: a fixed
/// node-Laplacian weight is a diffusivity of `beta*ds^2`, which is the other
/// half of the same ds-dependence. Applied as `n` sub-passes of weight
/// `D/(n*ds^2)` with `n` chosen to keep each pass under the 0.25 stability
/// ceiling (`carve.rs`'s CREEP_ALPHA_MAX is the same idea).
pub const DIFF_W2_PER_ITER: f64 = 0.030;
pub const DIFF_ALPHA_MAX: f64 = 0.25;
/// Curvature smoothing half-width, in CHANNEL WIDTHS. Curvature is a second
/// derivative; raw, it turns node jitter into a checkerboard that the
/// migration law then amplifies. Physical, not per-node: expressed in nodes
/// this filter's real width scales with `ds`, which selects a different
/// wavelength at every resolution -- the last hiding place of the
/// ds-dependence that test `wavelength_does_not_track_node_spacing` exists
/// to catch (it survived the migration/diffusion fixes: 66/109/133 m).
pub const CURV_SMOOTH_W: f64 = 1.5;
/// Ceiling on node count as a multiple of the initial count, so a lengthening
/// planform cannot grow without bound.
pub const N_MAX_MULT: usize = 3;

// --- physics ---------------------------------------------------------------

/// Friction coefficient. Standard sand-bed value.
pub const CF: f64 = 0.01;
/// Local-curvature weight in the Howard-Knutson sum.
pub const OMEGA: f64 = 0.15;
/// Upstream-memory weight.
pub const GAMMA: f64 = 2.5;
/// Memory kernel decay length, in channel widths: the `2*Cf/D` of the IPS
/// equation expressed in the units the model is written in.
pub const KERNEL_W: f64 = 12.0;
/// Hickin-Nanson peak: migration is fastest near this radius/width ratio and
/// damps on both sides. This inherits the retired `MEANDER_RATIO`'s job --
/// bounding the bank angle -- in units the model actually has.
pub const RC_PEAK: f64 = 2.8;

/// Bank erodibility varies IN SPACE, and that is the engine of irregular bend
/// trains. Real banks are not uniform -- clay plugs from old cutoffs, tree
/// roots, gravel lenses -- so one bend grows fast while its neighbour stalls,
/// and a reach ends up with bends of genuinely different SIZE rather than
/// merely different phase.
///
/// Anchored to WORLD POSITION (perlin2), not arc length: bank material does
/// not travel with the channel. That is also what distinguishes this from the
/// retired AMP_MOD, which multiplied the output of a closed-form curve by a
/// function of arc length -- decoration. This multiplies the RATE of a growth
/// process at a fixed place, so the planform still selects its own wavelength
/// locally; some bends simply get there sooner.
///
/// Measured 2026-09-01 on 55,281 OSM `waterway=stream` reaches over the corpus
/// regions (10 m resample, 30 m smoothing, bends >= 25 m): real bend-length CV
/// is 0.38/0.46/0.55 (p25/50/75) against 0.15-0.20 for every setting of the
/// re-injection dial. Re-injection varies bend AGE and barely moves the CV;
/// erodibility varies bend SIZE, which is what the statistic sees.
pub const ERODE_VAR: f64 = 0.55;
/// Correlation length of the erodibility field, in channel widths.
pub const ERODE_L_W: f64 = 9.0;
/// Room reference: the floor half-width at which migration runs at its
/// nominal rate. Below it the creek is pinched and straightens; above it the
/// creek is free to swing. Set near the fluvial trunk's typical floor
/// half-width so a normal reach is unchanged and the modulation is genuinely
/// two-sided rather than a global slowdown.
pub const ROOM_REF_M: f64 = 55.0;
pub const ROOM_MIN_K: f64 = 0.25;
pub const ROOM_MAX_K: f64 = 2.2;

/// RESISTANT PATCHES, and why the field is gated rather than merely varied.
///
/// A smooth +-55% variation means every reach migrates SOMEWHAT and nothing
/// is ever pinned, so the planform bends continuously. Real creeks do not:
/// measured 2026-09-01 on 115,154 OSM stream reaches, 59.7% of a real creek's
/// length runs straighter than R = 150 m and its typical longest straight run
/// is 118 m (66 m at R > 300), against 53.1% and 83 m (30 m) for the ungated
/// model. The reviewer saw this before it was measured: "real creeks have
/// more straight sections than what we model".
///
/// The physical reading is resistant banks -- clay plugs left by old cutoffs,
/// bedrock, root-bound sections -- which do not migrate at all rather than
/// migrating a little. So the field is GATED: below the threshold erodibility
/// drops to a floor and that reach is effectively pinned, which is what puts
/// a straight passage between two mobile bends.
pub const ERODE_FLOOR: f64 = 0.10;
pub const ERODE_GATE: (f64, f64) = (-0.22, 0.14);

// --- seeding ---------------------------------------------------------------

/// Symmetry-breaking amplitude, in channel widths.
pub const PERTURB_W: f64 = 0.06;
/// FOUR octaves at deliberately non-harmonic scales, in channel widths. This
/// is the one place the old bug could re-enter: a linear instability amplifies
/// whatever it is seeded with, so a single sine ships the sine again through
/// the back door. A broadband seed lets the INSTABILITY select the
/// wavelength, which is what makes the planform aperiodic. Near-prime ratios
/// for the reason the retired AMP_MOD_M already recorded: sharing a scale
/// makes features coincide, and coincidence is its own kind of regular.
pub const PERTURB_L_W: [f64; 4] = [3.0, 7.0, 17.0, 37.0];
pub const PERTURB_A: [f64; 4] = [1.00, 0.70, 0.50, 0.35];
/// Re-inject a fresh perturbation this often. A single seed gives every bend
/// the same age, and the same age gives the same amplitude -- which is the
/// regularity the review is complaining about. Real reaches carry bends of
/// MIXED age: a tight mature loop beside a barely-started inflection. This is
/// the process-level version of what the retired AMP_MOD faked.
pub const REINJECT_EVERY: usize = 20;
pub const REINJECT_FRAC: f64 = 0.30;

// --- cutoffs ---------------------------------------------------------------

/// Neck contact distance, in channel widths.
///
/// HARD CONSTRAINT, not a taste dial: this distance must exceed the width at
/// which two limbs' WET RIBBONS merge in the raster, or the carve welds two
/// limbs sitting at different bed elevations into one water body and the
/// geodesic audit correctly reports the difference as an impounded climb.
/// `water::cut_creek` wets `hw + cell*0.32` either side, so the merge width is
/// about `2*hw + 1.3 m`; at the aeolian trunk's `hw ~ 3 m` that is ~7.3 m, and
/// 2.5 widths at W = 6 m gives 15 m of margin.
pub const NECK_WIDTHS: f64 = 2.5;
/// Below this arc separation a close pair is a tight bend, not a neck. A
/// circular loop of radius `RC_PEAK*W` has arc `2*pi*RC_PEAK*W ~ 17.6 W`, so
/// 6 W is clear of any legitimate bend while catching every real neck.
pub const CUT_ARC_WIDTHS: f64 = 6.0;
/// Cutoff detection starts late. Before this, a close pair is a seeding
/// artefact and excising it removes noise the instability has not yet had a
/// chance to amplify (`carve.rs:805-813`, late injection).
pub const CUTOFF_FROM_FRAC: f64 = 1.0 / 3.0;
pub const CUTOFFS_PER_ITER: usize = 2;

// --- confinement -----------------------------------------------------------

/// Outward migration starts damping here and is fully damped at `u_hard`.
pub const U_SOFT_FRAC: f64 = 0.65;
/// Structural backstop: a node may never travel further than this many
/// channel widths from where it started.
pub const MAX_EXCURSION_W: f64 = 14.0;

/// Draws consumed by the model itself: none. All randomness enters as
/// `perlin1(s, salt)` with salts XOR-derived from ONE `u32` drawn by the
/// caller — the `gorge.rs:472` / `water.rs:1479` idiom. Zero draws inside the
/// loop is what makes the model reorderable and immune to a change in node
/// count shifting a transcript.
pub const DRAWS: usize = 0;

/// An abandoned meander loop. Recorded, not yet turned into water: emitting
/// oxbow ponds touches `lake_frac`, `clear_lakes_near`, blowout avoidance and
/// the `small_ponds_never_dominate_the_count` assert, and a second coupled
/// change would make a bad render impossible to attribute.
#[derive(Clone, Debug)]
pub struct Oxbow {
    pub pts: Vec<Vec2>,
    /// Bed level at the neck — the elevation the loop was cut off at.
    pub bed_z: f64,
    /// Which iteration abandoned it; an old scar is shallower and vegetated.
    pub iter: usize,
}

/// A migrating planform.
///
/// INDEX ORDER IS FLOW ORDER: 0 = upstream head, n-1 = mouth. The
/// Howard-Knutson kernel integrates curvature UPSTREAM of a node, so a
/// reversed polyline grows bends that lean the wrong way — and that failure
/// passes a sinuosity test, which is why the contract is stated here and
/// asserted at the callers' boundary.
pub struct Planform {
    pub p: Vec<Vec2>,
    /// Position along the INITIALISING centre-line, [0,1], non-decreasing in
    /// `i` by construction. This is what keeps the bed, the drawdown and the
    /// valley coordinate keyed to the right station once the path has stopped
    /// being a graph over that centre-line.
    pub t: Vec<f64>,
    pub oxbows: Vec<Oxbow>,
}

fn sub(a: Vec2, b: Vec2) -> Vec2 {
    Vec2::new(a.x - b.x, a.y - b.y)
}

/// Signed Menger curvature at each interior node, 1/m. Endpoints are zero.
fn curvature(p: &[Vec2]) -> Vec<f64> {
    let n = p.len();
    let mut c = vec![0.0; n];
    for i in 1..n.saturating_sub(1) {
        let v1 = sub(p[i], p[i - 1]);
        let v2 = sub(p[i + 1], p[i]);
        let l1 = v1.length();
        let l2 = v2.length();
        let s = Vec2::new(v1.x + v2.x, v1.y + v2.y).length();
        if l1 < 1e-9 || l2 < 1e-9 || s < 1e-9 {
            continue;
        }
        c[i] = 2.0 * (v1.x * v2.y - v1.y * v2.x) / (l1 * l2 * s);
    }
    c
}

/// Smooth curvature over a PHYSICAL half-width (see `CURV_SMOOTH_W`).
fn smooth_curv(c: &mut [f64], ds: f64, w: f64) {
    let half = ((CURV_SMOOTH_W * w / ds.max(1e-9)).round() as usize).max(1);
    let src = c.to_vec();
    let n = c.len();
    for i in 0..n {
        let lo = i.saturating_sub(half);
        let hi = (i + half + 1).min(n);
        c[i] = src[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
    }
}

/// Howard-Knutson migration rate, as an O(n) IIR recursion.
///
/// The reference form is `R1_i = OMEGA*R0_i + GAMMA * sum_{j<i} R0_j * G(s_i -
/// s_j)` with `G(x) = exp(-x / (KERNEL_W*W))`, normalised by the kernel's
/// discrete sum. Evaluated directly that is O(n^2); because the kernel is a
/// pure exponential the partial sum obeys
/// `acc_i = acc_{i-1} * exp(-ds/L) + R0_i`, which is exact, not an
/// approximation — `iir_matches_truncated_sum` pins it.
fn migration_rate(r0: &[f64], ds: &[f64], w: f64, kernel_w: f64) -> Vec<f64> {
    let n = r0.len();
    let l = kernel_w * w;
    let mut out = vec![0.0; n];
    let mut acc = 0.0;
    let mut norm = 0.0;
    for i in 0..n {
        let decay = if i == 0 { 1.0 } else { math::exp(-ds[i - 1] / l) };
        acc = acc * decay + r0[i];
        norm = norm * decay + 1.0;
        out[i] = OMEGA * r0[i] + GAMMA * (acc / norm.max(1e-9));
    }
    out
}

/// Hickin-Nanson damping: migration peaks near `RC_PEAK` widths of curvature
/// radius and falls away on both sides. Tight bends stop growing, which is
/// what keeps loops bounded without a curvature clamp bolted on afterwards.
fn hickin_nanson(c: f64, w: f64) -> f64 {
    let rc_w = if c.abs() < 1e-12 { 1e9 } else { 1.0 / (c.abs() * w) };
    let x = rc_w / RC_PEAK;
    // 1 for gentle bends, -> 0 as the bend tightens past RC_PEAK.
    //
    // The first version peaked at x = 1 and decayed for LARGE x as well,
    // i.e. it damped nearly-straight reaches -- but `r0 = W*c` already
    // carries that dependence, so the two multiplied and the instability
    // could never leave the seed amplitude (measured: 0.41 m of growth from
    // a 0.36 m seed over 120 iterations). Hickin-Nanson bounds the TIGHT
    // side; the straight side is the curvature's job.
    (x * x) / (1.0 + x * x)
}

fn arc_steps(p: &[Vec2]) -> Vec<f64> {
    (0..p.len().saturating_sub(1))
        .map(|i| p[i].distance(p[i + 1]))
        .collect()
}

fn perturb(s: f64, w: f64, salt: u32, amp: f64) -> f64 {
    perturb_sp(s, w, salt, amp, 1.0)
}

/// `spread` widens the octave scales: >1 makes the seed more broadband, which
/// lets the instability select LOCALLY and so varies bend size along the reach.
fn perturb_sp(s: f64, w: f64, salt: u32, amp: f64, spread: f64) -> f64 {
    let mut e = 0.0;
    for k in 0..4 {
        let l = PERTURB_L_W[k] * w * spread.powf(k as f64 - 1.5);
        e += PERTURB_A[k] * noise::perlin1(s / l,
                salt ^ (0x9E37_79B9u32.wrapping_mul(k as u32 + 1)));
    }
    amp * w * e
}

/// Keep node spacing inside `[ds/2, 2*ds]` by inserting midpoints and dropping
/// crowded nodes. `t` is interpolated so it stays non-decreasing.
fn resample_nodes(pf: &mut Planform, ds: f64, n_max: usize) {
    let mut p = Vec::with_capacity(pf.p.len() + 8);
    let mut t = Vec::with_capacity(pf.t.len() + 8);
    p.push(pf.p[0]);
    t.push(pf.t[0]);
    for i in 1..pf.p.len() {
        let d = pf.p[i].distance(*p.last().unwrap());
        if d > 2.0 * ds && p.len() + 1 < n_max {
            let a = *p.last().unwrap();
            let ta = *t.last().unwrap();
            p.push(Vec2::new((a.x + pf.p[i].x) * 0.5, (a.y + pf.p[i].y) * 0.5));
            t.push((ta + pf.t[i]) * 0.5);
        } else if d < 0.5 * ds && i + 1 < pf.p.len() {
            continue; // crowded: drop this node
        }
        p.push(pf.p[i]);
        t.push(pf.t[i]);
    }
    pf.p = p;
    pf.t = t;
}

/// Neck-cutoff detection on a uniform bucket grid, O(n) with a small constant.
/// Deterministic: among qualifying pairs the smallest `i`, then smallest `j`.
fn find_neck(p: &[Vec2], cum: &[f64], w: f64) -> Option<(usize, usize)> {
    let r = NECK_WIDTHS * w;
    let arc_min = CUT_ARC_WIDTHS * w;
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, q) in p.iter().enumerate() {
        grid.entry(((q.x / r).floor() as i64, (q.y / r).floor() as i64))
            .or_default()
            .push(i);
    }
    let mut best: Option<(usize, usize)> = None;
    for (i, q) in p.iter().enumerate() {
        let (gx, gy) = ((q.x / r).floor() as i64, (q.y / r).floor() as i64);
        for dx in -1..=1 {
            for dy in -1..=1 {
                let Some(cell) = grid.get(&(gx + dx, gy + dy)) else { continue };
                for &j in cell {
                    if j <= i + 1 {
                        continue;
                    }
                    if (cum[j] - cum[i]).abs() < arc_min {
                        continue;
                    }
                    if q.distance(p[j]) > r {
                        continue;
                    }
                    if best.is_none_or(|(bi, bj)| (i, j) < (bi, bj)) {
                        best = Some((i, j));
                    }
                }
            }
        }
    }
    best
}

/// Grow a planform from an initialising centre-line.
///
/// `base` is the centre-line in FLOW ORDER; `w` the channel width; `u_at` a
/// closure returning the normalised valley coordinate (0 on the axis, 1 where
/// the floor ends) at a world point; `u_hard` the value the path may never
/// exceed; `salt` one drawn u32.
pub fn grow<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32, vigour: f64,
               u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    grow_tuned(base, w, u_hard, salt, vigour, MIG_W_PER_ITER, u_at)
}

/// `grow` with the migration dial exposed — the calibration ladder drives
/// this; production calls `grow`.
pub fn grow_tuned<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32, vigour: f64,
                     mig_w: f64, mut u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    grow_at(base, w, u_hard, salt, vigour, mig_w, DS_WIDTHS, u_at)
}

/// Sweep entry with the ITERATION budget exposed. Bends must MATURE before
/// their limbs touch, so cutoff frequency is as much a function of how long
/// the model runs as of how fast it migrates.
#[allow(clippy::too_many_arguments)]
pub fn grow_sweep_iters<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32,
                           vigour: f64, mig_w: f64, reinject: usize, erode: f64,
                           kernel_w: f64, iters_mult: f64, u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    grow_full_iters(base, w, u_hard, salt, vigour, mig_w, DS_WIDTHS, reinject,
                    1.0, 1.0, erode, kernel_w,
                    ((ITERS as f64 * iters_mult) as usize).max(10), u_at)
}

/// Full sweep entry for the calibration ladder.
#[allow(clippy::too_many_arguments)]
pub fn grow_sweep<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32, vigour: f64,
                     mig_w: f64, reinject: usize, erode: f64, kernel_w: f64,
                     u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    grow_full(base, w, u_hard, salt, vigour, mig_w, DS_WIDTHS, reinject, 1.0,
              1.0, erode, kernel_w, u_at)
}

/// `grow` with the three IRREGULARITY dials exposed, for the calibration
/// ladder. `reinject` is the re-injection period (bend AGE spread), `spread`
/// multiplies the seeding octave scales (how broadband the seed is), and
/// `diff` multiplies the diffusivity (how much small-scale variety survives).
/// Production calls `grow`.
#[allow(clippy::too_many_arguments)]
pub fn grow_irregular<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32,
                         vigour: f64, reinject: usize, spread: f64, diff: f64,
                         u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    grow_full(base, w, u_hard, salt, vigour, MIG_W_PER_ITER, DS_WIDTHS,
              reinject, spread, diff, 1.0, KERNEL_W, u_at)
}

/// `grow_tuned` with the WORKING node spacing exposed. The resolution test
/// must drive this: `grow` resamples to `DS_WIDTHS*w` internally, so passing a
/// base at a different spacing does not change the working resolution -- an
/// early version of the test swept the initial spacing only and every run
/// converged to the same internal ds, which made a resolution artefact
/// indistinguishable from a physical result.
#[allow(clippy::too_many_arguments)]
pub fn grow_at<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32, vigour: f64,
                  mig_w: f64, ds_widths: f64, u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    grow_full(base, w, u_hard, salt, vigour, mig_w, ds_widths,
              REINJECT_EVERY, 1.0, 1.0, 1.0, KERNEL_W, u_at)
}

#[allow(clippy::too_many_arguments)]
fn grow_full<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32, vigour: f64,
                mig_w: f64, ds_widths: f64, reinject: usize, spread: f64,
                diff: f64, erode: f64, kernel_w: f64, u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    grow_full_iters(base, w, u_hard, salt, vigour, mig_w, ds_widths, reinject,
                    spread, diff, erode, kernel_w, ITERS, u_at)
}

#[allow(clippy::too_many_arguments)]
fn grow_full_iters<F>(base: &[Vec2], w: f64, u_hard: f64, salt: u32, vigour: f64,
                      mig_w: f64, ds_widths: f64, reinject: usize, spread: f64,
                      diff: f64, erode: f64, kernel_w: f64, iters: usize,
                      mut u_at: F) -> Planform
where
    F: FnMut(Vec2) -> f64,
{
    let ds = ds_widths * w;
    let n0 = base.len().max(2);
    let n_max = n0 * N_MAX_MULT;
    let mut pf = Planform {
        p: base.to_vec(),
        t: (0..n0).map(|i| i as f64 / (n0 - 1) as f64).collect(),
        oxbows: Vec::new(),
    };
    let start = pf.p.clone();
    let u_soft = u_hard * U_SOFT_FRAC;
    let step_max = STEP_CLAMP_W * w;
    let cutoff_from = (iters as f64 * CUTOFF_FROM_FRAC) as usize;

    // initial broadband perturbation
    {
        let steps = arc_steps(&pf.p);
        let mut s = 0.0;
        for i in 1..pf.p.len().saturating_sub(1) {
            s += steps[i - 1];
            let t = sub(pf.p[i + 1], pf.p[i - 1]).normalized();
            let nrm = Vec2::new(-t.y, t.x);
            let e = perturb_sp(s, w, salt, PERTURB_W, spread);
            let q = Vec2::new(pf.p[i].x + nrm.x * e, pf.p[i].y + nrm.y * e);
            if u_at(q) < u_hard {
                pf.p[i] = q;
            }
        }
    }

    for it in 0..iters {
        let n = pf.p.len();
        if n < 5 {
            break;
        }
        let steps = arc_steps(&pf.p);
        let mut cum = vec![0.0; n];
        for i in 1..n {
            cum[i] = cum[i - 1] + steps[i - 1];
        }
        let mut c = curvature(&pf.p);
        smooth_curv(&mut c, ds, w);
        let r0: Vec<f64> = c.iter().map(|&ci| w * ci).collect();
        let r1 = migration_rate(&r0, &steps, w, kernel_w);

        // re-injection: fresh symmetry-breaking so bends carry mixed AGE
        let reinject = it > 0 && reinject > 0 && it % reinject == 0;

        let mut np = pf.p.clone();
        for i in 1..n - 1 {
            let tv = sub(pf.p[i + 1], pf.p[i - 1]).normalized();
            let nrm = Vec2::new(-tv.y, tv.x);
            // Migration is OUTWARD: away from the centre of curvature, which
            // is what amplifies a bend. `nrm` is the LEFT normal and a left
            // turn (c > 0) puts the centre to the left, so outward is -nrm*c.
            // Getting this backwards makes the model DECAY, and it decays
            // quietly -- the first run sat at its seed amplitude and looked
            // like a tuning problem rather than a sign error.
            // ROOM-SCALED MIGRATION (review, 2026-09-01: "if the valley
            // pinches in the creek should be straighter, if the valley
            // widens we can have more meandering"). `u` is normalised, so
            // the same u threshold sits at very different PHYSICAL distances
            // in a narrow reach and a wide one. Estimating the local room
            // from the u gradient turns that into a migration multiplier:
            // a pinched valley pins the creek, a wide one lets it swing.
            let room = {
                let u0 = u_at(pf.p[i]);
                let e = w;
                let ux = (u_at(Vec2::new(pf.p[i].x + e, pf.p[i].y))
                          - u_at(Vec2::new(pf.p[i].x - e, pf.p[i].y))) / (2.0 * e);
                let uy = (u_at(Vec2::new(pf.p[i].x, pf.p[i].y + e))
                          - u_at(Vec2::new(pf.p[i].x, pf.p[i].y - e))) / (2.0 * e);
                let g = (ux * ux + uy * uy).sqrt();
                if g < 1e-6 { ROOM_REF_M } else { ((u_hard - u0).max(0.0) / g).min(400.0) }
            };
            let room_k = (room / ROOM_REF_M).clamp(ROOM_MIN_K, ROOM_MAX_K);
            let en = noise::perlin2(pf.p[i].x / (ERODE_L_W * w),
                                    pf.p[i].y / (ERODE_L_W * w),
                                    salt ^ 0x51ED_270B);
            // gate first (resistant patches), then vary within the mobile part
            let gate = math::smoothstep(ERODE_GATE.0, ERODE_GATE.1, en);
            let ero = (ERODE_FLOOR + (1.0 - ERODE_FLOOR) * gate)
                * (1.0 + 0.35 * ERODE_VAR * erode
                   * noise::perlin2(pf.p[i].x / (ERODE_L_W * w * 0.45),
                                    pf.p[i].y / (ERODE_L_W * w * 0.45),
                                    salt ^ 0x2C1B_3E77));
            let mag = -r1[i] * hickin_nanson(c[i], w) * vigour * room_k
                * mig_w * w * ero.max(0.05);
            let mut d = Vec2::new(nrm.x * mag, nrm.y * mag);
            if reinject {
                let e = perturb_sp(cum[i], w,
                                   salt ^ (it as u32).wrapping_mul(0x85EB_CA6B),
                                   PERTURB_W * REINJECT_FRAC, spread);
                d = Vec2::new(d.x + nrm.x * e, d.y + nrm.y * e);
            }
            // per-step clamp
            let dl = d.length();
            if dl > step_max {
                d = Vec2::new(d.x * step_max / dl, d.y * step_max / dl);
            }
            // GUARD 1 (soft): damp only the OUTWARD component. Damping the
            // whole displacement means a node that ever lands past the wall
            // can never come back, and one such node drags its neighbours out
            // through the diffusive term.
            let here = u_at(pf.p[i]);
            if here > u_soft {
                let k = 1.0 - math::smoothstep(u_soft, u_hard, here);
                let probe = Vec2::new(pf.p[i].x + d.x, pf.p[i].y + d.y);
                if u_at(probe) > here {
                    d = Vec2::new(d.x * k, d.y * k);
                }
            }
            let q = Vec2::new(pf.p[i].x + d.x, pf.p[i].y + d.y);
            // GUARD 2 (hard): never past the floor edge.
            if u_at(q) >= u_hard {
                continue;
            }
            // GUARD 3 (structural): bounded excursion from the initialisation.
            if q.distance(start[(i * (start.len() - 1)) / (n - 1)]) > MAX_EXCURSION_W * w {
                continue;
            }
            np[i] = q;
        }
        pf.p = np;

        // Diffusion: the missing half of the law (`carve.rs`: stream power
        // alone "etches a rectilinear comb"). Physical diffusivity, split
        // into stable sub-passes so it does not depend on node spacing.
        {
            let ds2 = (ds * ds).max(1e-9);
            let d_phys = DIFF_W2_PER_ITER * diff * w * w;
            let passes = ((d_phys / (DIFF_ALPHA_MAX * ds2)).ceil() as usize).max(1);
            let alpha = d_phys / (passes as f64 * ds2);
            for _ in 0..passes {
                let src = pf.p.clone();
                for i in 1..src.len() - 1 {
                    let lap = Vec2::new(src[i - 1].x + src[i + 1].x - 2.0 * src[i].x,
                                        src[i - 1].y + src[i + 1].y - 2.0 * src[i].y);
                    let q = Vec2::new(src[i].x + alpha * lap.x,
                                      src[i].y + alpha * lap.y);
                    if u_at(q) < u_hard {
                        pf.p[i] = q;
                    }
                }
            }
        }

        if it >= cutoff_from {
            for _ in 0..CUTOFFS_PER_ITER {
                let steps = arc_steps(&pf.p);
                let mut cum = vec![0.0; pf.p.len()];
                for i in 1..pf.p.len() {
                    cum[i] = cum[i - 1] + steps[i - 1];
                }
                let Some((i, j)) = find_neck(&pf.p, &cum, w) else { break };
                let loop_pts: Vec<Vec2> = pf.p[i + 1..j].to_vec();
                if loop_pts.len() >= 3 {
                    pf.oxbows.push(Oxbow { pts: loop_pts, bed_z: f64::NAN, iter: it });
                }
                pf.p.drain(i + 1..j);
                pf.t.drain(i + 1..j);
            }
        }
        resample_nodes(&mut pf, ds, n_max);
        debug_assert!(pf.t.windows(2).all(|q| q[1] >= q[0] - 1e-9),
                      "planform t must stay non-decreasing");
    }
    pf
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight(n: usize, ds: f64) -> Vec<Vec2> {
        (0..n).map(|i| Vec2::new(i as f64 * ds, 0.0)).collect()
    }

    /// The IIR recursion must equal the explicit truncated sum. This guards
    /// the one numerical shortcut in the module.
    #[test]
    fn iir_matches_truncated_sum() {
        let w = 6.0;
        let n = 200;
        let r0: Vec<f64> = (0..n).map(|i| (i as f64 * 0.11).sin()).collect();
        let ds = vec![3.0; n - 1];
        let fast = migration_rate(&r0, &ds, w, KERNEL_W);
        let l = KERNEL_W * w;
        for i in 0..n {
            let (mut acc, mut norm) = (0.0, 0.0);
            for j in 0..=i {
                let s: f64 = ds[j..i].iter().sum();
                let g = math::exp(-s / l);
                acc += r0[j] * g;
                norm += g;
            }
            let slow = OMEGA * r0[i] + GAMMA * (acc / norm.max(1e-9));
            assert!((fast[i] - slow).abs() < 1e-9,
                    "node {i}: iir {} vs sum {}", fast[i], slow);
        }
    }

    /// The planform must be set by the PHYSICS, not by the node spacing. If
    /// it tracks `ds`, the fastest-growing mode is the grid mode and the model
    /// is a resolution artefact rather than an instability.
    ///
    /// SINUOSITY is the metric, not a zero-crossing wavelength. The first
    /// version of this test counted sign changes of the lateral coordinate,
    /// which on a migrating planform counts DRIFT crossings -- the reach
    /// translates sideways as it grows -- and reported a spread of 135/172/219
    /// m on a path whose sinuosity was 1.02, i.e. essentially straight. Bends
    /// are delimited by curvature inflections, and sinuosity needs no
    /// delimiting at all.
    #[test]
    fn planform_does_not_track_node_spacing() {
        let w = 6.0;
        let mut sins = Vec::new();
        let mut lams = Vec::new();
        for dsw in [0.25_f64, 0.5, 1.0] {
            let ds = dsw * w;
            let n = (2400.0 / ds) as usize;
            let base = straight(n, ds);
            let pf = grow_at(&base, w, 1.0, 0xBEEF, 1.0, MIG_W_PER_ITER, dsw,
                             |_| 0.0);
            let len: f64 = arc_steps(&pf.p).iter().sum();
            let chord = pf.p[0].distance(*pf.p.last().unwrap());
            sins.push(len / chord);
            let mut c = curvature(&pf.p);
            smooth_curv(&mut c, ds, w);
            let infl = c.windows(2)
                .filter(|v| v[0] != 0.0 && v[1] != 0.0 && v[0].signum() != v[1].signum())
                .count();
            if infl >= 4 {
                lams.push(2.0 * len / infl as f64);
            }
        }
        // Sinuosity DIRECTLY, not the excess (s-1). The excess ratio was
        // tried first and is over-sensitive near s = 1: measured
        // [1.188, 1.229, 1.136] -- an 8% spread in the quantity itself -- it
        // reports 1.68 and reads as a failure. Sinuosity is the claim; the
        // excess is an amplifier.
        //
        // Note this criterion alone cannot distinguish "converged" from
        // "nothing happened" (a dead model scores [1.018, 1.018, 1.017] and
        // agrees beautifully). `grows_from_straight` is the companion that
        // rules that out, and the wavelength criterion below is what actually
        // catches the grid mode -- the broken version measured 1.62x on it.
        let (slo, shi) = (sins.iter().cloned().fold(f64::MAX, f64::min),
                          sins.iter().cloned().fold(0.0, f64::max));
        assert!(shi / slo < 1.15, "sinuosity tracks node spacing: {sins:?}");
        let (llo, lhi) = (lams.iter().cloned().fold(f64::MAX, f64::min),
                          lams.iter().cloned().fold(0.0, f64::max));
        assert!(lhi / llo < 1.45, "bend wavelength tracks node spacing: {lams:?}");
        // and the bends must be RESOLVED at the shipping spacing
        assert!(llo / (DS_WIDTHS * w) > 8.0,
                "bends under-resolved at the shipping ds: {llo} m");
    }

    /// A free path must never exit the floor, whatever the vigour.
    #[test]
    fn confinement_holds() {
        let w = 6.0;
        let base = straight(400, 3.0);
        // u = |y| / 60 -> the floor ends 60 m either side
        let pf = grow(&base, w, 0.34, 0x1234, 4.0, |q| (q.y.abs() / 60.0).min(1.0));
        for q in &pf.p {
            assert!(q.y.abs() / 60.0 <= 0.34 + 1e-6, "node left the floor at {q:?}");
        }
    }

    /// Growth must actually happen: a straight line must not stay straight.
    #[test]
    fn grows_from_straight() {
        let w = 6.0;
        let base = straight(400, 3.0);
        let pf = grow(&base, w, 1.0, 0x77, 1.0, |_| 0.0);
        let len: f64 = arc_steps(&pf.p).iter().sum();
        let chord = pf.p[0].distance(*pf.p.last().unwrap());
        let sin = len / chord;
        // real creeks run 1.15-1.30; the band is wide because vigour varies
        assert!((1.12..1.45).contains(&sin), "sinuosity {sin} outside the real band");
    }
}
