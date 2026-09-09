//! The Sandhills river gorge: a drainage network CUT into the dune field.
//!
//! Review 2026-08-26: our Nebraska river valleys read as painted on. Against
//! the real Dismal River the reason was plain — a real Sandhills river runs in
//! a sharply dissected canyon, its rim crenulated by short draws, while the
//! dune field a few hundred metres away carries no drainage at all. What we
//! had was a smooth per-station trough with a sinuous line in it.
//!
//! This replaces that trough on river seeds with the Carolina machinery, at
//! Sandhills numbers:
//!
//! * `channel::grow_sparse` — one trunk plus two SHORT tributary orders, and
//!   none of the three density-forcing passes. Sand is too permeable to shed
//!   surface flow, so channels belong only where the water table is cut.
//! * `carve::beds` — the same Flint-law long profiles, junction-integrated,
//!   run against the dune surface as their routing datum.
//! * a subtractive valley cut on DISTANCE TO NEAREST DRAINAGE, which is the
//!   shape the review asked for.
//! * a wigglier creek laid on the valley floor, offset from the trunk
//!   centre-line — the trunk is the straight element (`TrunkPlan::GORGE`) and
//!   the wiggle lives inside its canyon.
//!
//! The one Carolina stage that could NOT be reused is `assemble::assemble`.
//! It is a from-scratch generator: every cell of its output derives from a
//! channel bed, and it takes no base surface. A dune field already exists, so
//! the valley has to be cut into it. The cut follows the idiom proved by the
//! meander carve in `water::fluvial` — composed by `min` against a snapshot,
//! so it is order-independent and cannot double-cut a cell.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::Vec2;

use crate::draw::Descriptors;
use crate::{carve, channel};

/// What the gorge stage produced, for the water stage to finish.
pub struct Gorge {
    pub net: channel::Network,
    /// The creek line on the trunk floor, and its bed elevation.
    pub creek: Vec<Vec2>,
    pub creek_z: Vec<f64>,
    /// The planform's salt, passed on to the incision so the carve's noise
    /// belongs to this river without taking a draw of its own.
    pub salt: u32,
}

/// Valley half-width by tier, metres -- the distance at which the cut dies.
///
/// Measured, `tools/aeolian/dismal_profile.py`: the Dismal reach reaches 90% of
/// its rim height at a half-width of 534 m (744 and 1044 on the two incised
/// tiles), against 393 m for the pre-round-10 generator. The old 440 m trunk
/// was not a wide canyon, it was a narrow one -- what made it *read* narrow was
/// the wall exponent, not this number. Widened to put the apron on the measured
/// band; the trench stays crisp because of FLOOR_FRAC and WALL_P below.
fn half_width(tier: u8) -> f64 {
    match tier {
        1 => 1400.0,
        2 => 260.0,
        _ => 140.0,
    }
}

/// Fraction of the half-width that is flat valley floor before the wall
/// starts. Measured: the real sections sit within 5 m of the channel out to
/// ~100 m and do not climb at all for the first ~40 m, against 16 m for ours
/// -- we cut a V where the Dismal has a trough with a floor.
const FLOOR_FRAC: f64 = 0.10;

/// Only the trunk has a floor. A tributary catena is a V all the way down --
/// giving it the trunk's flat bottom turned every one into a flat-floored
/// tube with a rounded end, floating on the apron like a worm.
fn floor_frac(tier: u8) -> f64 {
    if tier == 1 { FLOOR_FRAC } else { 0.0 }
}

/// A tributary's cut must shallow to nothing at its head, or it arrives at
/// full depth and stops -- which is the other half of the worm. Exponent
/// below 1 keeps it deep for most of its length and closes it out quickly.
const HEAD_SHALLOW: f64 = 0.55;

/// Wall exponent by tier, on the normalised wall coordinate.
///
/// The trunk and its tributaries are shaped by different processes and the
/// reference shows it: the trunk carries a broad gentle apron, while the side
/// catenas are thin, sharp incisions cutting ACROSS that apron. Giving the
/// tributaries the trunk's wide gentle profile turned them into smooth lobes
/// -- the metrics were on band and the render read as scalloped ramp. Above 1
/// puts a sharp V on them, which is what "thin and sharp" actually means.
fn wall_p(tier: u8) -> f64 {
    if tier == 1 { WALL_P } else { 1.05 }
}

/// Wall exponent for the TRUNK, on the normalised wall coordinate.
///
/// This was 1.22, and being above 1 it made the wall SHALLOWEST at the floor
/// and steepest at the rim: a V with a hard edge. The measured Dismal does the
/// opposite -- the surface gains height fastest just outside the trench and
/// gentles as it goes out (8.7% from +5 to +15 m, 6.4% to +30, 3.4% to +45).
/// Below 1 reproduces that, which is what makes a crisp inner trench sit
/// inside an apron gentle enough to read as dune land rather than as valley.
const WALL_P: f64 = 0.72;

/// How quickly the dune grain reappears out of the trench, as a fraction of
/// the wall coordinate. Below this the floor is flat; above it the dunes are
/// at full amplitude, carried down by the valley rather than erased by it.
const DETAIL_KNEE: f64 = 0.30;

/// The trunk is cut in TWO layers, min-composed like everything else.
///
/// The reference has three scales, and the generator had two: a wide gentle
/// apron and a creek line, with a smooth ramp between them. What makes the
/// real Dismal read as "a very narrow valley" is the middle scale -- a narrow
/// slot incised into the apron floor with a distinct BREAK IN SLOPE at its
/// rim. A single smooth profile cannot produce a slope break at all, however
/// it is tuned, which is why widening and steepening both failed to make one.
/// The shares PARTITION the depth: layer 1 takes the top 70% and bottoms out
/// on the apron floor, layer 2 incises the remaining 30% into that floor. They
/// have to partition it -- with the apron cutting the full depth at the axis a
/// shallower trench can never win the min, which is exactly what happened on
/// the first attempt (every number identical to three decimal places).
///
/// (half-width m, wall exponent, floor fraction, share of the full depth)
/// The bool is the profile family. `false` is the power law `x^p`; `true` is
/// `1 - (1-x)^p`, which meets the rim TANGENTIALLY. The power law does not --
/// its slope at the rim is `p * cut`, a genuine slope discontinuity where the
/// cut meets untouched ground, and it drew a faint dotted line tracing the
/// whole valley edge. The tangent family also fits the measured Dismal better:
/// solving for p at four heights gives 1.63, 1.77, 1.61, 1.04 -- near-constant
/// where the power law needed 0.84, 0.72, 0.64, 0.94 to cover the same points.
/// The trench keeps the power law: a crease at ITS rim is the break in slope
/// that makes it read as a trench.
///
/// (half-width m, exponent, floor fraction, depth share, tangent-at-rim)
const TRUNK_LAYERS: [(f64, f64, f64, f64, bool); 2] = [
    (1400.0, 1.70, FLOOR_FRAC, 0.70, true),   // the apron
    (240.0, 1.15, 0.26, 0.30, false),         // the trench incised into it
];

/// How hard the trunk's half-width swings along its length.
///
/// Review 2026-08-27: the tributary network was doing this job and doing it
/// badly -- it read as a tidy dendritic diagram pasted onto a smooth trough,
/// and its rim sawtooth still measured a third of the real one. On the real
/// Dismal the ragged edge is not a drainage pattern at all: it is the dune
/// topography intersecting the valley, so the width wanders on the dunes' own
/// scale. Two octaves at 420 m and 155 m, which brackets the measured 320 m
/// notch pitch.
const WIDTH_SWING: f64 = 0.70;

/// Plan-view raggedness of the valley edge, as a fraction of the wall
/// coordinate. This warps WHERE the wall is, so the rim crenulates and the
/// slope carries texture, rather than adding bumps on top of a clean ramp.
const WALL_ROUGH: f64 = 0.52;

/// Vertical roughness ON the wall, as a fraction of the local cut. Shaped by
/// `4x(1-x)` so it vanishes at the floor and at the rim and peaks mid-slope,
/// which is where a real dissected valley side carries its gullying. Without
/// this the wall is a clean surface however ragged its outline is.
const WALL_TEX: f64 = 0.085;

/// A tributary's valley is as wide as the trunk's where it joins and narrows
/// to a point at its head. That taper is what cuts the discrete, flat-bottomed
/// embayments the real rim shows -- it swings between 150 and 500 m over a
/// ~330 m pitch -- and what leaves the heads pointed instead of rounded.
const HEAD_TAPER: f64 = 0.30;

/// No cell is cut more than this far below the ground it started at. A relief
/// valve in the same spirit as `carve.rs`'s `dcap`: without it a trunk that
/// happens to route under a tall dune train cuts a canyon far deeper than the
/// landscape warrants (measured, seed 41229: 113 m against a real 50-54).
const MAX_CUT_M: f64 = 62.0;

/// Mean wall grade a valley wall may not exceed (rise over the wall's
/// width). Measured on the seven Dismal tiles, 2 m, within 400 m of the
/// trunk: slope p99 50 %, p99.9 62 %, max 80 %. 0.60 puts the power law's
/// rim (1.15x the mean on the trench) at the real p99.9.
const WALL_SLOPE_MAX: f64 = 0.60;

/// The saturation above is a valley-scale relief valve, and it left the
/// river with nowhere to run: where the trunk crosses a dune train the cut
/// stops MAX_CUT_M short, the "floor" is the dune surface lowered bodily,
/// and the water level -- which must descend -- ends up 17 m below the
/// ground beside it (measured, seed 600032: floor above the graded bed
/// p50 5.7 m, p90 18.6 m, max 27.7 m ON THE TRUNK LINE). The creek crease
/// then dug a slot with near-vertical walls through the residual.
///
/// A river that was there before the dunes cuts THROUGH them: the valley
/// floor may rise over the train, but the inner gorge goes to grade. So the
/// trench layer cuts the residual the saturated apron leaves, and widens
/// with the extra depth so its wall slope does not blow up:
/// `hw *= sqrt(1 + res / trench_depth)`.
///
/// The same rule runs the other way. The cut never fills, so a deflation pan
/// or an interdune hollow lying on the trunk line stayed a hole in the floor
/// (seed 600006: a pan at -9.9 m on a bed at +3 m), and the creek's
/// descending level dragged the whole downstream reach to the hole's depth --
/// 4 km of creek 13-20 m below its floor. A river aggrades its floor: within
/// the trench, ground below the graded floor is raised to it, the raise
/// fading out across the lower wall so nothing steps. Ablation: same toggle.
fn through_cut() -> bool {
    std::env::var("GORGE_THROUGH").map(|v| v != "off").unwrap_or(true)
}

/// Cut the gorge into `height` (8 m macro grid). Returns None when the seed
/// draws no river.
pub fn build(rng: &mut DetRng, height: &mut Grid<f64>, d: &Descriptors)
    -> Option<Gorge> {
    let spec = height.spec;
    let cell = spec.cell_size;

    // --- routing datum: the dune surface itself ---------------------------
    // `channel::grow` wants a steering field, not an elevation — carve.rs
    // says so in as many words. The dune surface IS the right steerer here:
    // water in a dune field runs down the interdune corridors, which is
    // exactly what a downhill walk on this surface finds. Smoothed first so
    // the walks follow corridors rather than chasing individual hummocks.
    let mut datum = height.clone();
    blur8(&mut datum, 6);

    let net = channel::grow_sparse(rng, &datum, d);
    if net.chans.is_empty() {
        return None;
    }

    // Beds at Sandhills depth. The Carolina dials are calibrated for a 6-9 m
    // valley; a Sandhills canyon is 50 m, and the bed has to reach down that
    // far or the walls have nothing to stand on.
    let mut dg = *d;
    // 3.4x: measured against the real canyons, this lands valley depth at a
    // median of ~46 m (Dismal River 54, Sand Hills GC 50.5) with wall slopes
    // on their 10-17% band. 4.2 was tried first and cut 55-113 m gorges.
    // A wide river gets a SHALLOW valley, a narrow creek may keep its gorge.
    // Physically the same trade the reviewer asked for: the big allogenic
    // rivers here run on broad floors, and it is the small creeks that sit at
    // the bottom of the deep cuts.
    let wide = d.river_w_m > 7.0;
    let depth_k = if wide { 0.52 } else { 1.0 };
    dg.valley_depth_m = d.valley_depth_m * 3.4 * depth_k;
    let max_cut = MAX_CUT_M * depth_k;
    let through = through_cut();
    let slope_bound = std::env::var("GORGE_SLOPE").map(|v| v != "off").unwrap_or(true);
    // ablations for the wall-slope diagnosis
    let wall_tex = if std::env::var("GORGE_TEX").map(|v| v == "off").unwrap_or(false) { 0.0 } else { WALL_TEX };
    let wall_rough = if std::env::var("GORGE_ROUGH").map(|v| v == "off").unwrap_or(false) { 0.0 } else { WALL_ROUGH };
    let detail_on = !std::env::var("GORGE_DETAIL").map(|v| v == "off").unwrap_or(false);
    let fill_on = !std::env::var("GORGE_FILL").map(|v| v == "off").unwrap_or(false);
    let blur_on = !std::env::var("GORGE_BLUR").map(|v| v == "off").unwrap_or(false);
    if std::env::var("GORGE").map(|v| v == "off").unwrap_or(false) {
        return None;
    }
    let mut beds = carve::beds(rng, &net, &datum, &dg);
    let tiers: Vec<u8> = net.chans.iter().map(|c| c.tier).collect();
    // The bed runs no higher than the ground beside it would let water stand
    // (2026-09-09). The graded bed is measured off the routing datum, and
    // where the trunk crosses a hummock with open low ground beside it the
    // bed sat 12-15 m above that ground; filled to grade, the creek ran on a
    // ridge (700200, 700775). A stream is the local minimum. So at each
    // trunk station the bed is lowered to the lowest SPILL elevation within
    // BED_LOW_R of it -- the level water beside the station would drain to
    // -- and the profile is made monotone toward the mouth again, which is
    // what carries a low crossed near the head down the rest of the trunk
    // as a deeper cut. An enclosed pan keeps its rim above the bed and
    // changes nothing. Ablation: `GORGE_BEDLOW=off`.
    if std::env::var("GORGE_BEDLOW").map(|v| v != "off").unwrap_or(true) {
        const BED_LOW_R: f64 = 100.0;
        let spill = crate::water::fill_levels(&spec, &height.data, &vec![false; spec.len()], &vec![false; spec.len()]);
        let rc = (BED_LOW_R / cell).ceil() as i64;
        for (ci, (pts, bed)) in beds.iter_mut().enumerate() {
            if tiers.get(ci).copied().unwrap_or(1) != 1 || pts.len() < 2 {
                continue;
            }
            let mouth_last = bed[bed.len() - 1] <= bed[0];
            let mut lowered = 0usize;
            for k in 0..pts.len() {
                let (cx, cy) = ((pts[k].x / cell).round() as i64, (pts[k].y / cell).round() as i64);
                let mut s_min = f64::INFINITY;
                for gy in (cy - rc).max(0)..=(cy + rc).min(spec.ny as i64 - 1) {
                    for gx in (cx - rc).max(0)..=(cx + rc).min(spec.nx as i64 - 1) {
                        if spec.world_of(gx as u32, gy as u32).distance(pts[k]) <= BED_LOW_R {
                            s_min = s_min.min(spill[spec.index(gx as u32, gy as u32)]);
                        }
                    }
                }
                if s_min < bed[k] {
                    bed[k] = s_min;
                    lowered += 1;
                }
            }
            // monotone toward the mouth again
            if mouth_last {
                for k in 1..bed.len() { bed[k] = bed[k].min(bed[k - 1]); }
            } else {
                for k in (0..bed.len() - 1).rev() { bed[k] = bed[k].min(bed[k + 1]); }
            }
            if std::env::var("NET_DEBUG").is_ok() {
                eprintln!("gorge: bed lowered to the ground beside at {lowered}/{} stations", pts.len());
            }
        }
    }

    // --- the cut: one valley per channel, min-composed --------------------
    // Nearest-channel attribution was the bug. It hands every cell to
    // whichever channel is closest, so a 60 m tributary standing between a
    // cell and the trunk overrides the trunk's 900 m valley -- measured, the
    // half-width field's median came out at 131 m against a 900 m trunk,
    // which is why widening `half_width` moved the output by 2%.
    //
    // A valley is not a property of the nearest channel; it is the UNION of
    // every channel's own valley. Compose them with a min, the same
    // order-independent idiom `water::fluvial` uses for the meander carve.
    // The attribution seams that the 40-pass bed relaxation existed to cure
    // cannot arise at all this way -- a min of continuous fields is
    // continuous -- so that relaxation is gone with it.
    //
    // This is also what cuts the embayments: where a tributary joins, its own
    // valley (full width at the mouth, tapering to a point at the head) is
    // unioned with the trunk's, which is exactly the flat-bottomed bite the
    // real rim shows swinging between 150 and 500 m.
    // Noise seeds from the descriptors, not from `rng`: this keeps the DRAW
    // transcript for the stage exactly where it was, so the creek downstream
    // still draws the same style, lambda and swing it did before.
    let ns = (d.wind_rad.to_bits() ^ d.valley_depth_m.to_bits()) as u32;
    let (ns2, ns3) = (ns ^ 0x9e37_79b9, ns ^ 0x85eb_ca6b);

    // Plan-view wall roughness, precomputed once over the tile.
    let mut rough: Vec<f64> = vec![0.0; spec.len()];
    let mut fine: Vec<f64> = vec![0.0; spec.len()];
    let mut wide: Vec<f64> = vec![0.0; spec.len()];
    for i in 0..spec.len() {
        let px = (i % spec.nx as usize) as f64 * cell;
        let py = (i / spec.nx as usize) as f64 * cell;
        wide[i] = course_world::noise::perlin2(px / 300.0, py / 300.0, ns)
            + 0.6 * course_world::noise::perlin2(px / 118.0, py / 118.0, ns2);
        rough[i] = course_world::noise::perlin2(px / 165.0, py / 165.0, ns2)
            + 0.55 * course_world::noise::perlin2(px / 62.0, py / 62.0, ns3);
        fine[i] = course_world::noise::perlin2(px / 78.0, py / 78.0, ns3)
            + 0.6 * course_world::noise::perlin2(px / 31.0, py / 31.0, ns);
    }

    let h0: Vec<f64> = height.data.clone();
    // The floor is measured against a SMOOTHED ground, the rim against the
    // real one. Where the cut saturates at MAX_CUT_M, `h0 - cut` is just the
    // dune surface lowered bodily, so the "flat floor" inherited every
    // hummock -- measured, the floor held to within 5 m for only 34 m either
    // side against the real 74. Taking the floor off a smoothed ground makes
    // it genuinely flat; tying the wall to `h0` still lands the cut exactly
    // on untouched ground at the rim, so nothing steps at the valley edge.
    let mut h0g = height.clone();
    blur8(&mut h0g, 14);
    let h0s: Vec<f64> = h0g.data.clone();
    let mut target: Vec<f64> = h0.clone();
    // The graded floor the trunk's trench guarantees, and how firmly (1 on
    // the flat floor, 0 by the middle of the trench wall). See `through_cut`.
    let mut fill_t: Vec<f64> = vec![f64::NEG_INFINITY; spec.len()];
    let mut fill_w: Vec<f64> = vec![0.0; spec.len()];
    // The apron's own surface, per cell, for the trench to ride on (see the
    // trench branch below). Untouched ground where the apron does not reach.
    let mut t_apron: Vec<f64> = h0.clone();
    for (ci, (pts, bed)) in beds.iter().enumerate() {
        if pts.len() < 2 {
            continue;
        }
        let tier = tiers.get(ci).copied().unwrap_or(1);
        // Trunk only. The network is still grown and still returned -- the
        // water stage reads it -- but the side channels no longer cut.
        if tier != 1 {
            continue;
        }
        let layers: &[(f64, f64, f64, f64, bool)] = if tier == 1 {
            &TRUNK_LAYERS
        } else {
            &[(0.0, 0.0, 0.0, 1.0, false)]   // filled per tier below
        };
        let hw0 = half_width(tier);
        let wp = wall_p(tier);
        let ff = floor_frac(tier);
        // Which end is downstream? The bed falls toward the mouth.
        let mouth_last = bed.last().copied().unwrap_or(0.0) <= bed[0];
        let mut bed_seed: Vec<(u32, u32, f64)> = Vec::new();
        let mut hw_seed: Vec<(u32, u32, f64)> = Vec::new();
        let mut ds_seed: Vec<(u32, u32, f64)> = Vec::new();
        for k in 0..pts.len() - 1 {
            let (a, b) = (pts[k], pts[k + 1]);
            // Position along the channel, 0 at the head and 1 at the mouth.
            let f = k as f64 / (pts.len() - 1) as f64;
            let f = if mouth_last { f } else { 1.0 - f };
            let hw = hw0 * (HEAD_TAPER + (1.0 - HEAD_TAPER) * f);
            // Depth taper: the trunk holds its depth, a catena closes out.
            let ds = if tier == 1 { 1.0 } else { f.powf(HEAD_SHALLOW) };
            let seg = a.distance(b);
            let n = (seg / (cell * 0.5)).ceil().max(1.0) as usize;
            for j in 0..=n {
                let t = j as f64 / n as f64;
                let p = Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                let x = (p.x / cell).round().clamp(0.0, (spec.nx - 1) as f64) as u32;
                let y = (p.y / cell).round().clamp(0.0, (spec.ny - 1) as f64) as u32;
                let z = bed[k] + (bed[k + 1] - bed[k]) * t;
                bed_seed.push((x, y, z));
                hw_seed.push((x, y, hw));
                ds_seed.push((x, y, ds));
            }
        }
        if bed_seed.is_empty() {
            continue;
        }
        let (mut dist_c, mut bed_c) = edt(&spec, &bed_seed);
        // The bed field is a nearest-station carry, so between two reaches
        // of one meander it JUMPS by the bed's fall around the bend (metres
        // at a neck). The saturating cut attenuated that jump; the
        // through-cut, which cuts to the bed exactly, printed it as a cliff
        // along the medial line (700046: 5,400 cells above 150 %, 300-570 m
        // from the river, 174 with the through-cut off). Twelve passes give
        // a ~20 m Gaussian: a 10 m jump becomes a 20 % slope, and on the
        // axis, where adjacent stations differ by the grade times 8 m,
        // nothing moves.
        blur_vec(&spec, &mut bed_c, 12);
        // A distance field has a CREASE along its medial axis -- the locus
        // equidistant from two parts of the channel -- and the cut prints that
        // crease as a dead-straight line running out across the dune field.
        // Visible on every river tile and on no other, which is what located
        // it. Rounding the distance field removes the kink; it barely moves
        // the wall, which is governed by dist/hw over hundreds of metres.
        blur_vec(&spec, &mut dist_c, 2);
        let (_, mut hw_c) = edt(&spec, &hw_seed);
        let (_, mut ds_c) = edt(&spec, &ds_seed);
        blur_vec(&spec, &mut ds_c, 4);
        // Light smoothing only: this field now varies along ONE channel, so
        // there is no tier jump to erase -- just the taper's own steps.
        blur_vec(&spec, &mut hw_c, 12);
        let mut cum = 0.0f64;
        for li in 0..layers.len() {
            let (lhw, lwp, lff, ldep, ltan) = if tier == 1 {
                layers[li]
            } else {
                (hw0, wp, ff, 1.0, false)
            };
            let before = cum;
            cum += ldep;
        for i in 0..spec.len() {
            // For the trunk each layer has its own absolute half-width; for a
            // tributary the chamfered, tapered field is the half-width.
            // The width swing is a SMOOTH 2-D field, not a per-station value
            // on the channel. Seeding it along the channel and reading it back
            // through the nearest-feature transform made every station-to-
            // station jump in width into a straight radial seam running to the
            // tile edge -- the ruled lines that survived both the medial-axis
            // blur and the exact distance transform. A field that is
            // continuous in the plane cannot produce one.
            //
            // It must be applied BEFORE the reach test. Modulating after it
            // meant the cut stopped at the UNMODULATED radius wherever the
            // swing widened the valley, which put a hard edge in the surface
            // and drew a dashed line along the whole valley -- visible in the
            // macro grid, which is how it was told apart from the texture.
            // Saturate the cut SMOOTHLY. `bed.max(h0 - MAX_CUT)` was tried
            // and put a kink wherever the two branches cross, which on a
            // bumpy dune surface is a ragged line -- 809% slopes. tanh
            // saturates at the same limit with no corner anywhere.
            let ds = ds_c[i].clamp(0.0, 1.0);
            let want = (h0s[i] - bed_c[i]).max(0.0);
            let full = max_cut * (want / max_cut).tanh() * ds;
            // The innermost trunk layer carries the river to grade: it takes
            // the residual the saturation left (see `through_cut`).
            let res = if through && tier == 1 && li == layers.len() - 1 {
                (want * ds - full).max(0.0)
            } else {
                0.0
            };
            let widen = if res > 0.0 {
                (1.0 + res / (full * ldep).max(1.0)).sqrt()
            } else {
                1.0
            };
            let hw = (if tier == 1 {
                lhw * (hw_c[i] / hw0).clamp(0.0, 1.0)
            } else {
                hw_c[i]
            } * (1.0 + WIDTH_SWING * wide[i]).clamp(0.30, 1.95)
                * widen)
            .max(1.0);
            // The wall is never steeper than the real one. The width swing
            // (down to 0.30), the plan roughness (the wall coordinate scaled
            // by up to 1.8) and the through-cut's depth each narrowed the
            // wall on their own, and together they cut cliffs: on the
            // 200-seed mixed run 152/155 river tiles carried slopes above
            // 150 % on the trench wall, against a real Dismal wall of p99
            // 50 %, p99.9 62 %, max 80 % within 400 m of the trunk. So the
            // wall's width is at least what its depth needs at WALL_SLOPE_MAX
            // (mean grade; the power law's rim is p times that), measured
            // AFTER the roughness warp. Ablation: `GORGE_SLOPE=off`.
            let warp = 1.0 + wall_rough * rough[i];
            let trench_on_apron = through && tier == 1 && li == layers.len() - 1;
            let hw = if slope_bound {
                // the trench's wall climbs from the bed to the APRON SURFACE
                let depth_l = if trench_on_apron {
                    (t_apron[i] - bed_c[i]).max(0.0)
                } else {
                    full * ldep + res
                };
                let need = lwp * depth_l / (WALL_SLOPE_MAX * (1.0 - lff)) * warp.max(0.2);
                hw.max(need)
            } else {
                hw
            };
            let x = (dist_c[i] / hw * warp).clamp(0.0, 1.0);
            if x >= 1.0 {
                continue;
            }
            // Flat floor first, then the wall on the remaining coordinate.
            let xs = ((x - lff) / (1.0 - lff)).clamp(0.0, 1.0);
            let cut = full * ldep + res;
            // LOWER the dune surface; do not replace it. Interpolating toward
            // a floor -- `floor_z + (h0 - floor_z) * xs^p` -- scales the local
            // dune relief by xs^p, so the apron came out a smooth ramp with
            // the dunes ironed off it. Every metric improved and the render
            // got worse. On the real Dismal the dune grain runs right across
            // the apron, just carried lower. So: take the profile off the
            // SMOOTHED ground, then add the dune detail back over `g`, which
            // recovers within DETAIL_KNEE of the axis and leaves only the
            // trench floor genuinely flat.
            // Deeper layers start from the shallower one's floor, so the
            // trench is incised INTO the apron rather than measured from the
            // dune surface -- that is what puts the break in slope at its rim.
            let base = h0s[i] - full * before;
            let floor_z = base - cut;
            let detail = if detail_on { h0[i] - h0s[i] } else { 0.0 };
            let g = (xs / DETAIL_KNEE).clamp(0.0, 1.0);
            let shape = if ltan {
                1.0 - (1.0 - xs).powf(lwp)
            } else {
                xs.powf(lwp)
            };
            let tex = wall_tex * fine[i] * (4.0 * xs * (1.0 - xs)).max(0.0);
            // The floor converges on the river at the measured 5 % (see
            // `assemble::FLOOR_CONV_*`): the same rise the Carolina floor
            // gets, applied across the flat floor and fading as the wall
            // starts, referenced to the trunk axis so the creek's bends do
            // not print. Without it the trench sat in a flat floor and every
            // creek carve drew a rim.
            let dc = dist_c[i];
            let conv = if dc < crate::assemble::FLOOR_CONV_W {
                crate::assemble::FLOOR_CONV_S * dc
                    - crate::assemble::FLOOR_CONV_S * dc * dc / (2.0 * crate::assemble::FLOOR_CONV_W)
            } else {
                0.5 * crate::assemble::FLOOR_CONV_S * crate::assemble::FLOOR_CONV_W
            };
            let t = if trench_on_apron {
                // The trench rides on the apron SURFACE. Measured from the
                // apron's floor level, as before, its rim sat 0.3·full below
                // the apron wherever the apron had already started to climb
                // -- and the through-cut's widening pushed the rim out to
                // where that was 20 m: a cliff along the whole trench edge
                // (700046: 5,400 cells above 150 %, 300-570 m from the
                // river; 174 with the through-cut off). So: floor at the
                // bed (with the converging rise), the wall climbing to
                // whatever the apron is at this cell, the dune detail
                // arriving with it.
                let floor_t = bed_c[i] + conv * (1.0 - xs);
                let depth_h = (t_apron[i] - floor_t).max(0.0);
                floor_t + depth_h * (shape + tex)
            } else {
                floor_z + cut * (shape + tex) + detail * g + conv * (1.0 - xs)
            };
            if tier == 1 && li == 0 {
                t_apron[i] = t;
            }
            if t < target[i] {
                target[i] = t;
            }
            if through && fill_on && tier == 1 && li == layers.len() - 1 {
                fill_t[i] = bed_c[i] + conv * (1.0 - xs);
                fill_w[i] = 1.0 - course_world::ease::smoothstep(0.0, 0.6, xs);
            }
        }
        }
    }
    for i in 0..spec.len() {
        if target[i] < height.data[i] {
            height.data[i] = target[i];
        }
    }
    // The fill may not build a levee (2026-09-09). Filling a hollow on the
    // trunk line up to the bed made a platform wherever the bed stood above
    // the ground BESIDE the trench -- an interdune low, a pan -- and the
    // creek then ran on a ridge 12-15 m above both flanks (700200, 700775;
    // 2 of 155 river tiles). A stream is the local minimum. So the fill is
    // capped at each cell's spill elevation on the cut surface (a priority
    // flood from the tile edge): an enclosed hollow still fills to the bed,
    // one open to lower ground fills only to its rim, and the creek's level
    // then dips there and pools, which is what the low ground would do to
    // it. Ablation: `GORGE_LEVEE=allow`.
    let levee_cap = !std::env::var("GORGE_LEVEE").map(|v| v == "allow").unwrap_or(false);
    let spill = if levee_cap {
        let mut sp = crate::water::fill_levels(&spec, &height.data, &vec![false; spec.len()], &vec![false; spec.len()]);
        blur_vec(&spec, &mut sp, 4);      // a rim, not a scarp, where the cap changes
        sp
    } else {
        vec![f64::INFINITY; spec.len()]
    };
    for i in 0..spec.len() {
        // ground below the graded floor, inside the trench: raise it, but
        // never above what the ground around would hold
        let cap = fill_t[i].min(spill[i] + 0.2);
        let below = cap - height.data[i];
        if below > 0.0 && fill_w[i] > 0.0 {
            height.data[i] += fill_w[i] * below;
        }
    }
    // Angle of repose over the cut (2026-09-07). The plan-view roughness
    // warp scales the wall coordinate by a field with a 62 m octave, and
    // across a 400 m wall that field's gradient makes the wall locally six
    // times steeper than its mean (700046: a 61 m drop in 30 m; 2,085 cells
    // above 150 %, six with the warp off). The width swing does the same
    // on the apron. Rather than retune each term, the cut gets the pass
    // the dunes get (`surface::talus_at`), at the real wall's limit: 2 m
    // max 80 % within 400 m of the Dismal, so 0.60 on the 8 m macro.
    // Crenulation survives wherever its walls stand at repose. Ablation:
    // `GORGE_TALUS=off`.
    if !std::env::var("GORGE_TALUS").map(|v| v == "off").unwrap_or(false) {
        crate::surface::talus_at(height, 0.60);
    }
    // One light pass to take the corner off the rim, where the cut meets
    // untouched ground. The attribution seams are gone at source now.
    if blur_on {
        blur8(height, 1);
    }

    // --- the creek on the trunk floor -------------------------------------
    // The trunk is the straight element; the creek is where the wiggle went.
    // Same offset-from-centre-line idiom as the Carolina meander creek: an
    // offset line rather than a free path, so it cannot leave its own valley.
    let trunk_i = net.chans.iter().position(|c| c.tier == 1)?;
    let (tp_raw, _) = &beds[trunk_i];
    // Offset from a SMOOTHED centre-line. A lateral offset of amplitude A
    // taken about a bend of radius R folds into a cusp on the inside whenever
    // A > R, and the trunk has bends far tighter than the 37 m the amplitude
    // cap allows -- measured on the emitted polyline, seed 41421 turned 94
    // degrees where the meander alone accounts for 42. Smoothing the base
    // line raises R above the amplitude; the VALLEY cut still uses the raw
    // trunk, so only the creek's own path is affected.
    let tp: Vec<Vec2> = {
        let mut v = tp_raw.clone();
        for _ in 0..6 {
            let src = v.clone();
            for i in 1..src.len().saturating_sub(1) {
                v[i] = Vec2::new((src[i - 1].x + 2.0 * src[i].x + src[i + 1].x) * 0.25,
                                 (src[i - 1].y + 2.0 * src[i].y + src[i + 1].y) * 0.25);
            }
        }
        v
    };
    let tp = &tp;
    let style = crate::water::RiverStyle::PASSED[rng.below(crate::water::RiverStyle::PASSED.len())];
    let lam = rng.range_f64(style.lam.0, style.lam.1);
    let swing = rng.range_f64(style.swing.0, style.swing.1);
    let phase = rng.range_f64(0.0, std::f64::consts::TAU);
    let s_lam = rng.next_u32();
    // The planform is the BEND TRAIN, the same one the Carolina creek uses
    // (`planform::bend_train`): a sequence of individually drawn bends and
    // straight runs with no carrier, each bend confined to the trunk floor.
    // It replaces the two-sine offset that ran here -- review, twice: "too
    // sinusoidal" -- and takes no draws of its own, so the five above keep
    // their positions and their values.
    //
    // The base line is already smoothed six passes above, so the planform is
    // told not to smooth it again. Room is the trunk's flat floor half-width
    // (`FLOOR_FRAC * half_width(1)`), kept off the wall by a margin; at these
    // amplitudes the curvature floor is what actually binds, which is why the
    // old `limit_curvature(95 m)` guard and `MEANDER_RATIO` are gone with the
    // sine that needed them.
    let prm = crate::planform::Params {
        smooth_passes: 0,
        ..crate::planform::Params::from_draws(lam, swing, phase, d.river_w_m)
    };
    let room_max = 0.85 * FLOOR_FRAC * half_width(1);
    // Room is what the CUT FLOOR actually offers (2026-09-07), not the
    // nominal flat-floor half-width: the width swing shrinks the trench
    // floor to ~20 m in places, and a creek given 119 m there climbed the
    // wall -- on the 200-seed mixed run 25 river tiles cut 6-15 m at one
    // spot with the ground high on ONE side only. At each base point the
    // room is the distance, either side along the normal, at which the cut
    // surface first stands ROOM_RISE_M above the floor, less a margin.
    // Ablation: `GORGE_ROOM=fixed`.
    const ROOM_RISE_M: f64 = 2.0;
    const ROOM_MARGIN_M: f64 = 8.0;
    const ROOM_MIN_M: f64 = 6.0;
    let fixed_room = std::env::var("GORGE_ROOM").map(|v| v == "fixed").unwrap_or(false);
    let hgt: &Grid<f64> = height;
    let room_at = |q: Vec2| -> f64 {
        if fixed_room {
            return room_max;
        }
        let mut k = 0usize;
        let mut best = f64::MAX;
        for (j, p) in tp.iter().enumerate() {
            let dd = p.distance(q);
            if dd < best { best = dd; k = j; }
        }
        let (a, b) = (tp[k.saturating_sub(1)], tp[(k + 1).min(tp.len() - 1)]);
        let t = Vec2::new(b.x - a.x, b.y - a.y);
        let tl = (t.x * t.x + t.y * t.y).sqrt().max(1e-9);
        let nrm = Vec2::new(-t.y / tl, t.x / tl);
        let z0 = hgt.bilinear(tp[k]);
        let mut room = room_max;
        for sgn in [-1.0f64, 1.0] {
            let mut s = 4.0;
            while s < room_max {
                let p = Vec2::new(tp[k].x + nrm.x * s * sgn, tp[k].y + nrm.y * s * sgn);
                if hgt.bilinear(p) - z0 > ROOM_RISE_M { break; }
                s += 4.0;
            }
            room = room.min(s);
        }
        (room - ROOM_MARGIN_M).max(ROOM_MIN_M)
    };
    let pf = crate::planform::bend_train(tp, crate::planform::Flow::MouthFirst,
                                         &prm, s_lam, &room_at);
    let creek = pf.p;

    // Take the creek bed from the CUT SURFACE, not from carve::beds.
    // The valley cut saturates at MAX_CUT_M, so where the trunk routes under
    // a tall dune its graded bed ends up far below the floor that was
    // actually cut. Cutting the creek slot to that bed punched a 24 m step in
    // one cell at the water's edge (measured: every cell over 200% slope on
    // seed 41251 sat within 12 m of water). Sampling the floor makes the
    // creek sit on it by construction.
    let mut creek_z: Vec<f64> = creek.iter().map(|p| height.bilinear(*p)).collect();
    if std::env::var("NET_DEBUG").is_ok() {
        // DIAG: how far the cut floor sits above the graded bed, on the
        // trunk line itself and under the creek.
        let (_, tb) = &beds[trunk_i];
        let mut on_trunk: Vec<f64> = Vec::new();
        let mut want_v: Vec<f64> = Vec::new();
        for (k, p) in tp_raw.iter().enumerate() {
            on_trunk.push(height.bilinear(*p) - tb[k]);
            want_v.push(h0g.bilinear(*p) - tb[k]);
        }
        let mut under_creek: Vec<f64> = Vec::new();
        let mut off_axis: Vec<f64> = Vec::new();
        for (i, p) in creek.iter().enumerate() {
            let (mut kb, mut db) = (0usize, f64::MAX);
            for (k, q) in tp_raw.iter().enumerate() {
                let dd = p.distance(*q);
                if dd < db { db = dd; kb = k; }
            }
            under_creek.push(creek_z[i] - tb[kb]);
            off_axis.push(db);
        }
        let pct = |v: &mut Vec<f64>| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let q = |f: f64| v[((v.len() - 1) as f64 * f) as usize];
            format!("p50 {:.1} p90 {:.1} max {:.1}", q(0.5), q(0.9), q(1.0))
        };
        eprintln!("gorge diag: want (h0s-bed) on trunk {}", pct(&mut want_v));
        eprintln!("gorge diag: floor-bed on trunk {}", pct(&mut on_trunk));
        eprintln!("gorge diag: floor-bed under creek {}", pct(&mut under_creek));
        eprintln!("gorge diag: creek off-axis {}", pct(&mut off_axis));
        let ups = tb.windows(2).filter(|w| w[1] > w[0] + 1e-9).count();
        let downs = tb.windows(2).filter(|w| w[1] < w[0] - 1e-9).count();
        eprintln!("gorge diag: trunk bed {} nodes, first {:.1} last {:.1}, min {:.1} max {:.1}, rises {} falls {}",
                  tb.len(), tb[0], tb[tb.len() - 1],
                  tb.iter().cloned().fold(f64::MAX, f64::min), tb.iter().cloned().fold(f64::MIN, f64::max), ups, downs);
        let mut cz = creek_z.clone();
        eprintln!("gorge diag: creek_z raw {}", pct(&mut cz));
    }
    // a creek runs downhill; hold it monotone from whichever end is lower
    if creek_z.len() > 1 {
        if creek_z[0] > creek_z[creek_z.len() - 1] {
            for i in 1..creek_z.len() {
                creek_z[i] = creek_z[i].min(creek_z[i - 1]);
            }
        } else {
            for i in (0..creek_z.len() - 1).rev() {
                creek_z[i] = creek_z[i].min(creek_z[i + 1]);
            }
        }
    }
    Some(Gorge { net, creek, creek_z, salt: s_lam })
}

/// In-place 5-point blur over a bare Vec on `spec`, `n` passes.
fn blur_vec(spec: &course_world::grid::GridSpec, v: &mut [f64], n: usize) {
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut tmp = v.to_vec();
    for _ in 0..n {
        tmp.copy_from_slice(v);
        for y in 0..ny {
            for x in 0..nx {
                let at = |a: i64, b: i64| {
                    tmp[spec.index(a.clamp(0, nx - 1) as u32, b.clamp(0, ny - 1) as u32)]
                };
                let i = spec.index(x as u32, y as u32);
                v[i] = (tmp[i] * 4.0
                    + at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1))
                    / 8.0;
            }
        }
    }
}

/// 5-point blur with edge replication, `n` passes. Local copy so the gorge
/// does not have to make `assemble`'s private helpers public.
fn blur8(g: &mut Grid<f64>, n: usize) {
    let spec = g.spec;
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut tmp = g.data.clone();
    for _ in 0..n {
        tmp.copy_from_slice(&g.data);
        for y in 0..ny {
            for x in 0..nx {
                let at = |a: i64, b: i64| {
                    tmp[spec.index(a.clamp(0, nx - 1) as u32, b.clamp(0, ny - 1) as u32)]
                };
                g.data[spec.index(x as u32, y as u32)] = (tmp
                    [spec.index(x as u32, y as u32)]
                    * 4.0
                    + at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1))
                    / 8.0;
            }
        }
    }
}

/// Borgefors 5x5 chamfer with nearest-seed value carry. Same weights as
/// `assemble::chamfer`; duplicated rather than exported because that one is a
/// private detail of a module the gorge deliberately does not use.
fn chamfer(spec: &course_world::grid::GridSpec, seed: &[(u32, u32, f64)])
    -> (Vec<f64>, Vec<f64>) {
    let big = 1e18f64;
    let mut d = vec![big; spec.len()];
    let mut v = vec![0.0f64; spec.len()];
    for &(x, y, val) in seed {
        let i = spec.index(x, y);
        d[i] = 0.0;
        v[i] = val;
    }
    let c = spec.cell_size;
    let (a, b, e) = (c, c * 1.4, c * 2.2);
    let fwd: [(i64, i64, f64); 8] = [
        (-1, -2, e), (1, -2, e), (-2, -1, e), (-1, -1, b),
        (0, -1, a), (1, -1, b), (2, -1, e), (-1, 0, a),
    ];
    let bwd: [(i64, i64, f64); 8] = [
        (1, 2, e), (-1, 2, e), (2, 1, e), (1, 1, b),
        (0, 1, a), (-1, 1, b), (-2, 1, e), (1, 0, a),
    ];
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut sweep = |order: &[(i64, i64, f64); 8], rev: bool| {
        let ys: Vec<i64> = if rev { (0..ny).rev().collect() } else { (0..ny).collect() };
        let xs: Vec<i64> = if rev { (0..nx).rev().collect() } else { (0..nx).collect() };
        for &y in &ys {
            for &x in &xs {
                let i = spec.index(x as u32, y as u32);
                for &(ox, oy, w) in order.iter() {
                    let (px, py) = (x + ox, y + oy);
                    if px < 0 || py < 0 || px >= nx || py >= ny {
                        continue;
                    }
                    let j = spec.index(px as u32, py as u32);
                    if d[j] + w < d[i] {
                        d[i] = d[j] + w;
                        v[i] = v[j];
                    }
                }
            }
        }
    };
    sweep(&fwd, false);
    sweep(&bwd, true);
    (d, v)
}

/// Exact Euclidean distance transform with a feature transform
/// (Felzenszwalb & Huttenlocher, *Distance Transforms of Sampled Functions*),
/// returning distance in metres and the value carried by the nearest seed.
///
/// This replaces a 5x5 Borgefors chamfer. The chamfer builds its field by
/// propagating along a fixed set of step vectors, so far from the seed the
/// value at a cell depends on WHICH chain of steps reached it, and the
/// boundaries between chains are dead-straight rays. Measured on the field
/// itself: |grad| ran 1.005-1.020 where an exact transform is 1.000, and the
/// gradient image was a fan of straight lines across the whole tile. The
/// error is small but its DERIVATIVE jumps across each ray, and the valley
/// cut prints that as ruled lines running out over the dune field. It was
/// invisible while the valley was 440 m wide and appeared the moment the
/// apron widened to 1400 m. An exact transform has no preferred direction.
fn edt(spec: &course_world::grid::GridSpec, seed: &[(u32, u32, f64)])
    -> (Vec<f64>, Vec<f64>) {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    const INF: f64 = 1e12;

    let mut f = vec![INF; nx * ny];
    let mut val = vec![0.0f64; nx * ny];
    for &(x, y, v) in seed {
        let i = spec.index(x, y);
        if f[i] != 0.0 {
            f[i] = 0.0;
            val[i] = v;
        }
    }

    // 1-D lower envelope of parabolas; returns squared distance and argmin.
    fn dt1(g: &[f64], d: &mut [f64], arg: &mut [usize]) {
        let n = g.len();
        let mut v = vec![0usize; n];
        let mut z = vec![0.0f64; n + 1];
        let mut k = 0usize;
        v[0] = 0;
        z[0] = f64::NEG_INFINITY;
        z[1] = f64::INFINITY;
        for q in 1..n {
            loop {
                let p = v[k];
                let s = ((g[q] + (q * q) as f64) - (g[p] + (p * p) as f64))
                    / (2.0 * q as f64 - 2.0 * p as f64);
                if s <= z[k] {
                    if k == 0 {
                        v[0] = q;
                        z[0] = f64::NEG_INFINITY;
                        z[1] = f64::INFINITY;
                        break;
                    }
                    k -= 1;
                } else {
                    k += 1;
                    v[k] = q;
                    z[k] = s;
                    z[k + 1] = f64::INFINITY;
                    break;
                }
            }
        }
        let mut k = 0usize;
        for q in 0..n {
            while z[k + 1] < q as f64 {
                k += 1;
            }
            let p = v[k];
            let dq = q as f64 - p as f64;
            d[q] = dq * dq + g[p];
            arg[q] = p;
        }
    }

    // --- rows: nearest seed within each row -------------------------------
    let mut d1 = vec![INF; nx * ny];
    let mut ax = vec![0usize; nx * ny];
    let mut row = vec![0.0f64; nx];
    let mut dr = vec![0.0f64; nx];
    let mut ar = vec![0usize; nx];
    for y in 0..ny {
        row.copy_from_slice(&f[y * nx..(y + 1) * nx]);
        dt1(&row, &mut dr, &mut ar);
        d1[y * nx..(y + 1) * nx].copy_from_slice(&dr);
        ax[y * nx..(y + 1) * nx].copy_from_slice(&ar);
    }

    // --- columns: combine into the exact 2-D transform --------------------
    let mut dist = vec![0.0f64; nx * ny];
    let mut out = vec![0.0f64; nx * ny];
    let mut col = vec![0.0f64; ny];
    let mut dc = vec![0.0f64; ny];
    let mut ac = vec![0usize; ny];
    for x in 0..nx {
        for y in 0..ny {
            col[y] = d1[y * nx + x];
        }
        dt1(&col, &mut dc, &mut ac);
        for y in 0..ny {
            let fy = ac[y];
            let fx = ax[fy * nx + x];
            dist[y * nx + x] = dc[y].max(0.0).sqrt() * spec.cell_size;
            out[y * nx + x] = val[fy * nx + fx];
        }
    }
    (dist, out)
}

