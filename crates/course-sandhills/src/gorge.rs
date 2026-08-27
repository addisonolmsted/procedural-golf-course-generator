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
use course_world::math::{self, Vec2};

use crate::draw::Descriptors;
use crate::{carve, channel};

/// What the gorge stage produced, for the water stage to finish.
pub struct Gorge {
    pub net: channel::Network,
    /// The creek line on the trunk floor, and its bed elevation.
    pub creek: Vec<Vec2>,
    pub creek_z: Vec<f64>,
}

/// Valley half-width by tier, metres. The trunk canyon is wide; the side
/// draws are narrow. Measured against the real thing: Dismal River sits 54 m
/// below its rim with 16.7% walls, Sand Hills GC 50.5 m and 10.4%.
fn half_width(tier: u8) -> f64 {
    match tier {
        1 => 440.0,
        2 => 190.0,
        _ => 90.0,
    }
}

/// No cell is cut more than this far below the ground it started at. A relief
/// valve in the same spirit as `carve.rs`'s `dcap`: without it a trunk that
/// happens to route under a tall dune train cuts a canyon far deeper than the
/// landscape warrants (measured, seed 41229: 113 m against a real 50-54).
const MAX_CUT_M: f64 = 62.0;

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
    dg.valley_depth_m = d.valley_depth_m * 3.4;
    let beds = carve::beds(rng, &net, &datum, &dg);
    let tiers: Vec<u8> = net.chans.iter().map(|c| c.tier).collect();

    // --- rasterise: bed elevation and valley half-width, per channel ------
    let mut bed_seed: Vec<(u32, u32, f64)> = Vec::new();
    let mut hw_seed: Vec<(u32, u32, f64)> = Vec::new();
    for (ci, (pts, bed)) in beds.iter().enumerate() {
        let tier = tiers.get(ci).copied().unwrap_or(1);
        let hw = half_width(tier);
        for k in 0..pts.len().saturating_sub(1) {
            let (a, b) = (pts[k], pts[k + 1]);
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
            }
        }
    }
    if bed_seed.is_empty() {
        return None;
    }
    let (dist, bed_raw) = chamfer(&spec, &bed_seed);
    let (_, mut hw_near) = chamfer(&spec, &hw_seed);
    // The half-width field is piecewise constant with three values, so where
    // the governing channel changes tier it jumps 440 -> 90 m and the cut
    // jumps with it. Smooth it hard: a valley's width is a property of the
    // valley, not of which cell you stand in. (Same reasoning as assemble.rs
    // blurring `w` by 14 before use.)
    blur_vec(&spec, &mut hw_near, 24);

    // --- RELAX the bed field ----------------------------------------------
    // Nearest-seed attribution is piecewise constant, so where the governing
    // channel switches the bed jumps by the whole difference between two
    // channels' beds — and the cut prints that jump as a knife seam radiating
    // from the valley (first render: hard black lines across the dune field).
    // This is the defect assemble.rs cures with a clamped diffusion, and it
    // needs the same cure here, not a couple of blur passes: relax toward the
    // neighbourhood mean with the channel cells re-anchored every pass, so
    // the field is harmonic between channels and exact on them.
    // The update is DAMPED (self-weighted), not a plain neighbour average.
    // A pure 4-neighbour mean with no self term is checkerboard-unstable
    // Jacobi: tried first, it oscillated instead of relaxing and drove max
    // slope to 809%. The 5-point form with weight 4 on the centre is the same
    // kernel `blur8` uses and is unconditionally stable.
    let mut bedf = bed_raw;
    let mut tmp = bedf.clone();
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    for _ in 0..40 {
        tmp.copy_from_slice(&bedf);
        for y in 0..ny {
            for x in 0..nx {
                let at = |a: i64, b: i64| {
                    tmp[spec.index(a.clamp(0, nx - 1) as u32, b.clamp(0, ny - 1) as u32)]
                };
                let i = spec.index(x as u32, y as u32);
                bedf[i] = (tmp[i] * 4.0
                    + at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1))
                    / 8.0;
            }
        }
        for &(sx, sy, z) in &bed_seed {
            bedf[spec.index(sx, sy)] = z;
        }
    }
    let bed_near = bedf;

    // --- the cut ----------------------------------------------------------
    // z = bed + (ground - bed) * shape(dist / W). shape(0) = 0 puts the floor
    // on the bed; shape(1) = 1 leaves the rim untouched, so the cut dies at
    // the valley edge by construction and never has to be told where that is.
    // The exponent makes the wall concave-up — a canyon, not a bowl.
    let h0: Vec<f64> = height.data.clone();
    let wall_p = 1.22f64;
    for i in 0..spec.len() {
        let hw = hw_near[i].max(1.0);
        if dist[i] >= hw {
            continue;
        }
        let x = (dist[i] / hw).clamp(0.0, 1.0);
        // Saturate the cut SMOOTHLY. `bed.max(h0 - MAX_CUT)` was tried and
        // put a kink wherever the two branches cross, which on a bumpy dune
        // surface is a ragged line -- 809% slopes. tanh saturates at the same
        // limit with no corner anywhere.
        let want = (h0[i] - bed_near[i]).max(0.0);
        let cut = MAX_CUT_M * (want / MAX_CUT_M).tanh();
        let floor_z = h0[i] - cut;
        let target = floor_z + cut * x.powf(wall_p);
        if target < height.data[i] {
            height.data[i] = target;
        }
    }
    // One light pass to take the corner off the rim, where the cut meets
    // untouched ground. The attribution seams are gone at source now.
    blur8(height, 1);

    // --- the creek on the trunk floor -------------------------------------
    // The trunk is the straight element; the creek is where the wiggle went.
    // Same offset-from-centre-line idiom as the Carolina meander creek: an
    // offset line rather than a free path, so it cannot leave its own valley.
    let trunk_i = net.chans.iter().position(|c| c.tier == 1)?;
    let (tp, _) = &beds[trunk_i];
    let style = crate::water::RiverStyle::PASSED[rng.below(crate::water::RiverStyle::PASSED.len())];
    let lam = rng.range_f64(style.lam.0, style.lam.1);
    let swing = rng.range_f64(style.swing.0, style.swing.1);
    let phase = rng.range_f64(0.0, std::f64::consts::TAU);
    let s_lam = rng.next_u32();
    let mut creek: Vec<Vec2> = Vec::with_capacity(tp.len());
    let mut creek_z: Vec<f64> = Vec::with_capacity(tp.len());
    // Sub-sample each trunk segment. The offset is a lateral displacement,
    // so the OFFSET path is longer than the centre-line it came from: at this
    // amplitude and wavelength the offset moves ~4.7 m per 6 m of centre-line,
    // which spaces consecutive creek points ~7.6 m apart. Emitting one creek
    // point per bed point therefore leaves gaps wider than the creek, and the
    // wet ribbon breaks into hundreds of disconnected puddles (measured: seed
    // 9 gave 271 bodies of ~30 m2). Step along the segment instead.
    let mut arc = 0.0;
    for k in 0..tp.len().saturating_sub(1) {
        let (a, b) = (tp[k], tp[k + 1]);
        let seg = a.distance(b);
        if seg <= 1e-6 {
            continue;
        }
        // Tangent over a WINDOW, not the raw segment. A lateral offset is
        // multiplied by any rotation of `perp`: at a bend, a 10 degree turn
        // swings a 50 m offset point by 8.7 m and opens another gap. Taking
        // the tangent across +/-4 bed points makes it rotate smoothly, which
        // is the same fix the Carolina creek needed.
        let lo = k.saturating_sub(4);
        let hi = (k + 4).min(tp.len() - 1);
        let tv = Vec2::new(tp[hi].x - tp[lo].x, tp[hi].y - tp[lo].y);
        let tl = tv.length().max(1e-9);
        let perp = Vec2::new(-tv.y / tl, tv.x / tl);
        let n = (seg / 2.0).ceil().max(1.0) as usize;
        for j in 0..n {
            let t = j as f64 / n as f64;
            let s_here = arc + seg * t;
            let lam_e =
                lam * (1.0 + 0.35 * course_world::noise::perlin1(s_here / 640.0, s_lam));
            let off = swing * 34.0
                * (math::sin(std::f64::consts::TAU * s_here / lam_e + phase)
                    + 0.35 * math::sin(std::f64::consts::TAU * s_here / (lam_e * 2.7)
                                       + phase * 1.7));
            let p = Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            creek.push(Vec2::new(p.x + perp.x * off, p.y + perp.y * off));
        }
        arc += seg;
    }
    // Resample along the CREEK's own arc. Sub-sampling the centre-line is
    // not enough: the offset derivative depends on the drawn style, and the
    // steepest one (lam 260 m, swing 1.70) moves the offset 3.7 m per metre
    // of centre-line, so even a 2 m centre-line step can space creek points
    // more than a creek-width apart. Walking the creek itself at a fixed
    // interval makes continuity independent of the style drawn.
    let (creek, _) = resample(&creek, &vec![0.0; creek.len()], 1.5);

    // Take the creek bed from the CUT SURFACE, not from carve::beds.
    // The valley cut saturates at MAX_CUT_M, so where the trunk routes under
    // a tall dune its graded bed ends up far below the floor that was
    // actually cut. Cutting the creek slot to that bed punched a 24 m step in
    // one cell at the water's edge (measured: every cell over 200% slope on
    // seed 41251 sat within 12 m of water). Sampling the floor makes the
    // creek sit on it by construction.
    let mut creek_z: Vec<f64> = creek.iter().map(|p| height.bilinear(*p)).collect();
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
    Some(Gorge { net, creek, creek_z })
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

/// Walk a polyline at a fixed interval, carrying its per-point value.
fn resample(pts: &[Vec2], val: &[f64], step: f64) -> (Vec<Vec2>, Vec<f64>) {
    if pts.len() < 2 {
        return (pts.to_vec(), val.to_vec());
    }
    let (mut op, mut ov) = (vec![pts[0]], vec![val[0]]);
    let mut carry = 0.0f64;
    for k in 0..pts.len() - 1 {
        let (a, b) = (pts[k], pts[k + 1]);
        let seg = a.distance(b);
        if seg <= 1e-9 {
            continue;
        }
        let mut t = step - carry;
        while t <= seg {
            let f = t / seg;
            op.push(Vec2::new(a.x + (b.x - a.x) * f, a.y + (b.y - a.y) * f));
            ov.push(val[k] + (val[k + 1] - val[k]) * f);
            t += step;
        }
        carry = seg - (t - step);
    }
    (op, ov)
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
