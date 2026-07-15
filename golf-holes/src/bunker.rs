//! Strategic bunkering.
//!
//! Shapes reuse the two long-form primitives already in the system: **petal
//! blobs** (the green evaluator — harmonics give the funky organic edges)
//! and **spline shapes** (a `SpineCurve` with knotted half-widths and
//! elliptical end caps) for longer trench bunkers. Placement is strategic
//! and cheap — a few dozen candidate draws per hole, each validated with a
//! handful of outline samples:
//!
//! - **greenside**: 0–3 bunkers guarding the green's flanks/front/back,
//!   sometimes clustered into an overlapping **complex**, sometimes a tiny
//!   round **pot**;
//! - **landing**: pinching the drive/second-shot landing zones just off the
//!   fairway edge (petal, spline, or pot);
//! - **dogleg liners**: long spline bunkers along the OUTSIDE of a turn,
//!   catching weakly-shaped shots;
//! - **decorative**: occasional off-line sand well away from play.

use golf_core::det::DetRng;
use golf_core::math::{self, Vec2};
use golf_core::spline::{Profile, SpineCurve};
use golf_core::Grid;
use golf_routing::Hole;

use crate::fairway::FairwayWidth;
use crate::{GreenShape, TeePad, FRINGE_M};

/// A bunker shape: petal blob or capped spline trench.
#[derive(Clone, Debug)]
pub enum BunkerShape {
    Petal(GreenShape),
    Spline {
        spine: SpineCurve,
        /// (s, half-width m) knots, smoothstep-eased.
        half_w: Vec<(f64, f64)>,
        /// End-cap radius (quarter-ellipse taper), m.
        cap: f64,
    },
}

impl BunkerShape {
    pub fn contains(&self, p: Vec2) -> bool {
        match self {
            BunkerShape::Petal(g) => g.rnorm(p) <= 1.0,
            BunkerShape::Spline { spine, half_w, cap } => {
                let (u, v) = spine.project(p);
                if !(0.0..=1.0).contains(&u) {
                    return false;
                }
                let len = spine.length();
                let w = crate::fairway::smooth_knots(half_w, u);
                let capf = |d_m: f64| -> f64 {
                    if d_m >= *cap {
                        1.0
                    } else {
                        let t = (d_m / cap.max(1e-9)).clamp(0.0, 1.0);
                        (t * (2.0 - t)).max(0.0).sqrt()
                    }
                };
                v.abs() <= w * capf(u * len) * capf((1.0 - u) * len)
            }
        }
    }

    /// Boundary samples (validation + extent + rendering ranges).
    pub fn outline(&self, n: usize) -> Vec<Vec2> {
        match self {
            BunkerShape::Petal(g) => (0..n)
                .map(|k| g.boundary(k as f64 / n as f64 * std::f64::consts::TAU))
                .collect(),
            BunkerShape::Spline { spine, half_w, cap } => {
                let len = spine.length();
                let m = (n / 2).max(3);
                let capf = |d_m: f64| -> f64 {
                    if d_m >= *cap {
                        1.0
                    } else {
                        let t = (d_m / cap.max(1e-9)).clamp(0.0, 1.0);
                        (t * (2.0 - t)).max(0.0).sqrt()
                    }
                };
                let mut left = Vec::with_capacity(m + 1);
                let mut right = Vec::with_capacity(m + 1);
                for k in 0..=m {
                    let s = k as f64 / m as f64;
                    let w = crate::fairway::smooth_knots(half_w, s)
                        * capf(s * len)
                        * capf((1.0 - s) * len);
                    let p = spine.point_at(s);
                    let (_, nrm) = spine.frame_at(s);
                    left.push(p + nrm * w);
                    right.push(p + nrm * -w);
                }
                right.reverse();
                left.extend(right);
                left
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Bunker {
    pub shape: BunkerShape,
    /// Bounding box (query prefilter).
    pub bbox: (Vec2, Vec2),
}

impl Bunker {
    fn new(shape: BunkerShape) -> Self {
        let outline = shape.outline(24);
        let (mut lo, mut hi) = (
            Vec2::new(f64::INFINITY, f64::INFINITY),
            Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
        );
        for &p in &outline {
            lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        Bunker {
            shape,
            bbox: (lo - Vec2::new(0.5, 0.5), hi + Vec2::new(0.5, 0.5)),
        }
    }

    pub fn contains(&self, p: Vec2) -> bool {
        let (lo, hi) = self.bbox;
        p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y && self.shape.contains(p)
    }
}

/// A drawn petal-blob bunker at `center`, long axis along `rot`.
fn petal(center: Vec2, rot: f64, a: f64, b: f64, funk: f64, rng: &mut DetRng) -> BunkerShape {
    let mut coefs = [(0.0f64, 0.0f64); 4];
    coefs[1] = (rng.range_f64(0.4, 1.0) * funk, rng.range_f64(0.0, std::f64::consts::TAU));
    coefs[2] = (rng.range_f64(0.2, 0.7) * funk, rng.range_f64(0.0, std::f64::consts::TAU));
    let mut g = GreenShape {
        center,
        rot,
        a,
        b,
        n: 2.0,
        coefs,
        outline: Vec::new(),
        area: 0.0,
        guard_dir: None,
    };
    g.outline = (0..24)
        .map(|k| g.boundary(k as f64 / 24.0 * std::f64::consts::TAU))
        .collect();
    BunkerShape::Petal(g)
}

struct Placer<'a> {
    green: &'a GreenShape,
    tee: &'a TeePad,
    dblock: &'a Grid<f64>,
    placed: Vec<(Vec2, f64)>, // (center, clearance radius)
    out: Vec<Bunker>,
}

impl Placer<'_> {
    /// Validate a shape against water, the green + fringe, the tee pad,
    /// the box, and already-placed bunkers (complex members skip that).
    fn try_add(&mut self, shape: BunkerShape, in_complex: bool) -> bool {
        let outline = shape.outline(20);
        let fringe_out = 1.0 + (FRINGE_M + 0.8) / self.green.a.min(self.green.b).max(1.0);
        let mut center = Vec2::ZERO;
        for &p in &outline {
            center = center + p * (1.0 / outline.len() as f64);
        }
        let mut radius = 0.0f64;
        for &p in &outline {
            radius = radius.max(p.distance(center));
            if !golf_routing::geometry::in_box(p, 5.0)
                || self.dblock.bilinear(p) < 2.0
                || self.green.rnorm(p) < fringe_out
                || self.tee.rnorm(p) < 1.3
            {
                return false;
            }
        }
        if !in_complex {
            for &(c, r) in &self.placed {
                if center.distance(c) < r + radius + 1.0 {
                    return false;
                }
            }
        }
        self.placed.push((center, radius));
        self.out.push(Bunker::new(shape));
        true
    }
}

/// Place a hole's bunkers. Cheap: a few dozen draws, ~20 outline samples per
/// candidate.
#[allow(clippy::too_many_arguments)]
pub fn place(
    hole: &Hole,
    spine: &SpineCurve,
    fw: &FairwayWidth,
    green: &GreenShape,
    tee: &TeePad,
    dir_in: Vec2,
    dblock: &Grid<f64>,
    rng: &mut DetRng,
) -> Vec<Bunker> {
    let mut pl = Placer {
        green,
        tee,
        dblock,
        placed: Vec::new(),
        out: Vec::new(),
    };
    let len = spine.length().max(1.0);

    // --- Greenside: 0–3 slots around the green. ---
    let n_green = {
        let u = rng.next_f64();
        if u < 0.10 {
            0
        } else if u < 0.45 {
            1
        } else if u < 0.85 {
            2
        } else {
            3
        }
    };
    // Candidate slot directions (world): flanks, front, back.
    let ang_in = math::atan2(dir_in.y, dir_in.x);
    let mut slots = [
        ang_in + std::f64::consts::FRAC_PI_2,
        ang_in - std::f64::consts::FRAC_PI_2,
        ang_in + std::f64::consts::PI, // front (toward the tee)
        ang_in,                        // back
    ];
    // Deterministic shuffle of the slot order.
    for i in (1..slots.len()).rev() {
        let j = rng.below(i + 1);
        slots.swap(i, j);
    }
    for &slot in slots.iter().take(n_green) {
        let world_ang = slot + rng.range_f64(-0.3, 0.3);
        let dir = Vec2::new(math::cos(world_ang), math::sin(world_ang));
        // Green-local boundary point in that direction.
        let (_, th) = green.local(green.center + dir * 10.0);
        let edge = green.boundary(th);
        let outward = (edge - green.center).normalized();
        let pot = rng.next_f64() < 0.12;
        let (a, b) = if pot {
            let r = rng.range_f64(1.8, 3.2);
            (r, r)
        } else {
            let a = rng.range_f64(4.0, 8.0);
            (a, a * rng.range_f64(0.5, 0.75))
        };
        let center = edge + outward * (FRINGE_M + 1.2 + b + rng.range_f64(0.0, 1.5));
        let rot = math::atan2(outward.y, outward.x) + std::f64::consts::FRAC_PI_2;
        let funk = if pot { 0.06 } else { rng.range_f64(0.10, 0.22) };
        let shape = petal(center, rot, a, b, funk, rng);
        let added = pl.try_add(shape, false);
        // Complex: cluster 1–2 satellites along the green edge tangent.
        if added && !pot && rng.next_f64() < 0.30 {
            let tangent = outward.perp();
            let n_sat = 1 + rng.below(2);
            let mut prev_c = center;
            let mut prev_a = a;
            for _ in 0..n_sat {
                let sa = prev_a * rng.range_f64(0.55, 0.85);
                let sb = sa * rng.range_f64(0.5, 0.75);
                let sign = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
                let c = prev_c
                    + tangent * (sign * (prev_a + sa * 0.8 + rng.range_f64(0.2, 1.2)))
                    + outward * rng.range_f64(-1.0, 2.0);
                let shape = petal(c, rot + rng.range_f64(-0.4, 0.4), sa, sb, rng.range_f64(0.12, 0.25), rng);
                pl.try_add(shape, true);
                prev_c = c;
                prev_a = sa;
            }
        }
    }

    // --- Landing-zone bunkers (par 4/5, per recorded landing). ---
    if !fw.is_none() {
        for &(s_land, _) in &crate::fairway::landing_zones(hole, spine) {
            if rng.next_f64() >= 0.70 {
                continue;
            }
            let n_here = if rng.next_f64() < 0.25 { 2 } else { 1 };
            for k in 0..n_here {
                let side = if k == 0 {
                    if rng.next_f64() < 0.5 { 1.0 } else { -1.0 }
                } else {
                    // Second bunker takes the other side.
                    -pl.placed.last().map(|_| 1.0).unwrap_or(1.0)
                };
                let u = (s_land + rng.range_f64(-35.0, 35.0) / len).clamp(fw.start, fw.end);
                let hw = fw.half_width(len, u, side > 0.0).max(6.0);
                let gap = rng.range_f64(-1.0, 4.0);
                let p0 = spine.point_at(u);
                let (tan, nrm) = spine.frame_at(u);
                let kind = rng.next_f64();
                if kind < 0.35 {
                    // Spline trench along the fairway edge.
                    let blen = rng.range_f64(15.0, 35.0);
                    let w = rng.range_f64(2.0, 3.5);
                    let mut pts = Vec::with_capacity(4);
                    for j in 0..4 {
                        let sj = (u + (j as f64 / 3.0 - 0.5) * blen / len).clamp(fw.start, fw.end);
                        let hwj = fw.half_width(len, sj, side > 0.0).max(6.0);
                        let (_, nj) = spine.frame_at(sj);
                        let wig = rng.range_f64(-1.2, 1.2);
                        pts.push(spine.point_at(sj) + nj * (side * (hwj + gap + w + wig)));
                    }
                    let bsp = SpineCurve::new(pts, Profile::constant(w), Profile::constant(1.0));
                    let shape = BunkerShape::Spline {
                        spine: bsp,
                        half_w: vec![(0.0, w * 0.8), (0.5, w), (1.0, w * 0.85)],
                        cap: w,
                    };
                    pl.try_add(shape, false);
                } else {
                    let pot = kind > 0.85;
                    let (a, b) = if pot {
                        let r = rng.range_f64(1.8, 3.0);
                        (r, r)
                    } else {
                        let a = rng.range_f64(5.0, 9.0);
                        (a, a * rng.range_f64(0.45, 0.7))
                    };
                    let c = p0 + nrm * (side * (hw + gap + b));
                    let rot = math::atan2(tan.y, tan.x);
                    let shape = petal(c, rot, a, b, if pot { 0.06 } else { rng.range_f64(0.10, 0.22) }, rng);
                    pl.try_add(shape, false);
                }
            }
        }

        // --- Dogleg liners: long spline sand on the OUTSIDE of each turn. ---
        let mut acc = 0.0;
        for k in 1..hole.pts.len() - 1 {
            acc += hole.pts[k - 1].distance(hole.pts[k]);
            let a = (hole.pts[k] - hole.pts[k - 1]).normalized();
            let b = (hole.pts[k + 1] - hole.pts[k]).normalized();
            let cross = a.perp().dot(b);
            if cross.abs() < 0.12 || rng.next_f64() >= 0.60 {
                continue;
            }
            // Left turn (+cross): outside is the RIGHT side.
            let side = if cross > 0.0 { -1.0 } else { 1.0 };
            let s_turn = (acc / len).clamp(fw.start, fw.end);
            let blen = rng.range_f64(18.0, 40.0);
            let gap = rng.range_f64(1.0, 3.0);
            let w = rng.range_f64(2.2, 3.6);
            let mut pts = Vec::with_capacity(5);
            for j in 0..5 {
                let sj = (s_turn + (j as f64 / 4.0 - 0.5) * blen / len).clamp(fw.start, fw.end);
                let hwj = fw.half_width(len, sj, side > 0.0).max(6.0);
                let (_, nj) = spine.frame_at(sj);
                pts.push(spine.point_at(sj) + nj * (side * (hwj + gap + w + rng.range_f64(-0.8, 0.8))));
            }
            let bsp = SpineCurve::new(pts, Profile::constant(w), Profile::constant(1.0));
            let shape = BunkerShape::Spline {
                spine: bsp,
                half_w: vec![(0.0, w * 0.75), (0.5, w), (1.0, w * 0.8)],
                cap: w,
            };
            pl.try_add(shape, false);
        }

        // --- Decorative: occasional off-line sand. ---
        if rng.next_f64() < 0.25 {
            let u = rng.range_f64(fw.start + 0.05, fw.end - 0.05);
            let side = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
            let hw = fw.half_width(len, u, side > 0.0).max(6.0);
            let off = rng.range_f64(8.0, 20.0);
            let (tan, nrm) = spine.frame_at(u);
            let a = rng.range_f64(3.0, 5.0);
            let b = a * rng.range_f64(0.55, 0.8);
            let c = spine.point_at(u) + nrm * (side * (hw + off + b));
            let shape = petal(c, math::atan2(tan.y, tan.x) + rng.range_f64(-0.5, 0.5), a, b, rng.range_f64(0.1, 0.2), rng);
            pl.try_add(shape, false);
        }
    } else {
        // Par 3: sometimes a bunker short of the green on the line (a carry
        // tease), in addition to the greenside set.
        if rng.next_f64() < 0.30 {
            let d = green.a.max(green.b) + rng.range_f64(10.0, 26.0);
            let perp = dir_in.perp();
            let c = green.center - dir_in * d + perp * rng.range_f64(-9.0, 9.0);
            let a = rng.range_f64(3.5, 6.5);
            let b = a * rng.range_f64(0.5, 0.75);
            let rot = math::atan2(dir_in.perp().y, dir_in.perp().x);
            let shape = petal(c, rot, a, b, rng.range_f64(0.1, 0.2), rng);
            pl.try_add(shape, false);
        }
    }

    pl.out.truncate(6);
    pl.out
}
