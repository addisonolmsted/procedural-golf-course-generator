//! Bowl: a closed-boundary depression with a revolved C1 profile — rim
//! rounding, inner wall gradient, flat floor — and a MANDATORY outlet choice:
//! a spillway notch through the rim (dry bowl that drains) or an intentional
//! lake (no outlet; later stages fill it). The swept surface covers inside
//! (floor), the rim, and a rising outside cone, so the smooth-min composition
//! embeds the bowl into any surrounding terrain with C1 seams.

use course_world::math::{cos, sin, Vec2};
use serde::{Deserialize, Serialize};

use course_world::ease::{ramp, smin, smoothstep};
use course_world::noise::perlin_ring;
use course_world::spline::Spine;

fn d_rim_round() -> f64 { 8.0 }
fn d_floor_k() -> f64 { 3.0 }
fn d_outer() -> f64 { 0.35 }
fn d_blend() -> f64 { 2.5 }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BowlBoundary {
    /// Explicit closed boundary (first point need not repeat — it is closed
    /// automatically).
    Points(Vec<Vec2>),
    /// Organic blob: radius(theta) = radius_m * (1 + wobble * noise(theta)),
    /// sampled at 96 stations. `cycles` sets the wobble's angular frequency.
    Blob {
        center: Vec2,
        radius_m: f64,
        wobble: f64,
        cycles: f64,
        seed: u32,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Outlet {
    /// No outlet — the bowl is a lake basin by intent.
    Lake,
    /// A notch through the rim at normalized boundary arc `at_s`, easing over
    /// `halfwidth_m` of boundary arc, `depth_m` below the rim.
    Spillway {
        at_s: f64,
        halfwidth_m: f64,
        depth_m: f64,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bowl {
    pub boundary: BowlBoundary,
    /// Rim elevation, meters.
    pub rim_z_m: f64,
    /// Depth of the floor below the rim, meters.
    pub depth_m: f64,
    /// Inner wall gradient (rise/run) from floor to rim.
    pub inner_grad: f64,
    /// C1 rounding at the rim, meters (rim sharpness: small = sharp).
    #[serde(default = "d_rim_round")]
    pub rim_round_m: f64,
    /// Smooth-min band where wall meets floor, meters of elevation.
    #[serde(default = "d_floor_k")]
    pub floor_k_m: f64,
    /// Outside falloff gradient (how fast the surface rises away from the rim).
    #[serde(default = "d_outer")]
    pub outer_grad: f64,
    /// Blend band for composing into the terrain.
    #[serde(default = "d_blend")]
    pub blend_k_m: f64,
    pub outlet: Outlet,
}

pub(crate) struct ResolvedBowl {
    pub spine: Spine,
    pub rim_z: f64,
    pub depth: f64,
    pub inner: f64,
    pub rim_round: f64,
    pub floor_k: f64,
    pub outer: f64,
    pub blend_k: f64,
    pub outlet: Outlet,
    pub bbox: (Vec2, Vec2),
    /// Boundary centroid and its maximum boundary radius — together with
    /// `rim_lo`/`grad_lo` they give a cheap conservative lower bound on
    /// [`Self::surface`] without running the 96-segment spine projection.
    center: Vec2,
    r_max: f64,
    /// Lowest rim elevation anywhere on the boundary (a spillway notch is
    /// the only thing that lowers it).
    rim_lo: f64,
    /// Smallest outward gradient anywhere on the boundary.
    grad_lo: f64,
}

impl ResolvedBowl {
    pub fn new(b: &Bowl) -> Self {
        let pts = match &b.boundary {
            BowlBoundary::Points(pts) => {
                let mut v = pts.clone();
                if v.first() != v.last() {
                    v.push(v[0]);
                }
                v
            }
            BowlBoundary::Blob { center, radius_m, wobble, cycles, seed } => {
                let n = 96usize;
                let mut v = Vec::with_capacity(n + 1);
                for i in 0..n {
                    let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                    let (cs, sn) = (cos(a), sin(a));
                    let r = radius_m
                        * (1.0 + wobble * perlin_ring(cs, sn, *cycles, *seed));
                    v.push(Vec2::new(center.x + r * cs, center.y + r * sn));
                }
                v.push(v[0]);
                v
            }
        };
        let spine = Spine::new(pts);
        // The pad stays relief-scaled, NOT depth-scaled: outside the rim the
        // bowl surface rises at `outer_grad`, so against terrain that climbs
        // faster it stays below and keeps carving far away. `apply` does the
        // real culling with an exact terrain-aware test.
        let pad = 300.0 / b.outer_grad.max(0.05) + b.rim_round_m + 50.0;
        let bbox = spine.bbox(pad);
        let center = {
            let n = spine.pts.len().max(1) as f64;
            let s = spine.pts.iter().fold(Vec2::ZERO, |a, p| a + *p);
            Vec2::new(s.x / n, s.y / n)
        };
        let r_max = spine
            .pts
            .iter()
            .map(|p| p.distance(center))
            .fold(0.0, f64::max);
        let notch = match &b.outlet {
            Outlet::Lake => 0.0,
            Outlet::Spillway { depth_m, .. } => depth_m.min(b.depth_m.max(0.5)),
        };
        let outer = b.outer_grad.max(0.02);
        let grad_lo = match &b.outlet {
            Outlet::Lake => outer,
            Outlet::Spillway { .. } => outer.min(Self::EXIT_GRAD),
        };
        ResolvedBowl {
            spine,
            rim_z: b.rim_z_m,
            depth: b.depth_m.max(0.5),
            inner: b.inner_grad.max(0.02),
            rim_round: b.rim_round_m.max(0.5),
            floor_k: b.floor_k_m.max(0.1),
            outer: b.outer_grad.max(0.02),
            blend_k: b.blend_k_m.max(0.1),
            outlet: b.outlet.clone(),
            bbox,
            center,
            r_max,
            rim_lo: b.rim_z_m - notch,
            grad_lo,
        }
    }

    /// Spillway influence at boundary arc `u`: 1 at the notch center easing
    /// to 0 over the halfwidth (closed-loop arc distance). 0 for lakes.
    fn notch_w(&self, u: f64) -> f64 {
        match &self.outlet {
            Outlet::Lake => 0.0,
            Outlet::Spillway { at_s, halfwidth_m, .. } => {
                let total = self.spine.length();
                let da = (u - at_s).abs() * total;
                let da = da.min(total - da);
                1.0 - smoothstep(0.0, halfwidth_m.max(1.0), da)
            }
        }
    }

    fn notch_depth(&self) -> f64 {
        match &self.outlet {
            Outlet::Lake => 0.0,
            Outlet::Spillway { depth_m, .. } => depth_m.min(self.depth),
        }
    }

    /// Gentle drainage-exit gradient inside the notch window (the spillway is
    /// a channel out, not a wall breach that immediately climbs again).
    const EXIT_GRAD: f64 = 0.05;

    pub fn surface(&self, p: Vec2) -> f64 {
        let hit = self.spine.project(p);
        let w = self.notch_w(hit.u);
        let rim = self.rim_z - self.notch_depth() * w;
        if self.spine.contains(p) {
            // inside: descend from the (possibly notched) rim, easing into the
            // floor. The notch lowers the WALL only — depth shrinks by the same
            // amount so the floor stays at rim_z - depth everywhere. (Interior
            // cells far from the boundary sit on the nearest-segment Voronoi
            // seams; a notched FLOOR would step across them.)
            let depth_eff = (self.depth - (self.rim_z - rim)).max(0.25);
            rim - smin(ramp(hit.d, self.inner, self.rim_round), depth_eff, self.floor_k)
        } else {
            // outside: rise away (exact no-op once above the local terrain).
            // In the notch window the rise is the gentle EXIT_GRAD, so the
            // spillway carves a proper drainage trench through the rim
            // surroundings instead of a stub.
            let grad = self.outer * (1.0 - w) + Self::EXIT_GRAD.min(self.outer) * w;
            rim + ramp(hit.d, grad, self.rim_round)
        }
    }

    /// Conservative lower bound on [`Self::surface`] at `p`, computed from
    /// one distance instead of the 96-segment spine projection. Outside the
    /// boundary the true distance to it is at least `|p-center| - r_max`,
    /// `ramp` is monotone in both distance and gradient, and the rim is
    /// never lower than `rim_lo` — so this can only under-estimate.
    #[inline]
    fn surface_lower_bound(&self, p: Vec2) -> f64 {
        let outside_by = p.distance(self.center) - self.r_max;
        if outside_by <= 0.0 {
            return f64::NEG_INFINITY;
        }
        self.rim_lo + ramp(outside_by, self.grad_lo, self.rim_round)
    }

    #[inline]
    pub fn apply(&self, p: Vec2, z: f64, w: f64) -> f64 {
        if w <= 0.0 {
            return z;
        }
        if p.x < self.bbox.0.x || p.x > self.bbox.1.x
            || p.y < self.bbox.0.y || p.y > self.bbox.1.y
        {
            return z;
        }
        // `smin(z, s, k)` saturates to z once s >= z + k, so a lower bound on
        // the surface that already clears the terrain by a blend band proves
        // this bowl cannot change the result — an exact test, not a
        // tolerance. Without it a 30-bowl sandhills tile spent 11 s of a 12 s
        // 2 m raster projecting onto 96-segment rings it could not affect
        // (full skeleton 58 s -> 4 s).
        //
        // It is not BYTE-identical to running the smooth-min: that branch
        // evaluates `s + (z - s)`, which is algebraically z but drifts by an
        // ULP (measured max 4.3e-14 m). Returning z verbatim is the more
        // accurate of the two; the goldens were re-blessed with MACRO_VERSION 3.
        if self.surface_lower_bound(p) >= z + self.blend_k {
            return z;
        }
        crate::prims::valley::blend(z, smin(z, self.surface(p), self.blend_k), w)
    }
}

#[cfg(test)]
mod bound_tests {
    use super::*;

    /// The early-out's lower bound must never exceed the true surface.
    #[test]
    fn surface_lower_bound_never_exceeds_surface() {
        for (outlet, wobble) in [
            (Outlet::Lake, 0.0),
            (Outlet::Lake, 0.3),
            (Outlet::Spillway { at_s: 0.3, halfwidth_m: 40.0, depth_m: 2.0 }, 0.2),
        ] {
            let b = Bowl {
                boundary: BowlBoundary::Blob {
                    center: Vec2::new(1500.0, 1500.0),
                    radius_m: 60.0,
                    wobble,
                    cycles: 2.0,
                    seed: 7,
                },
                rim_z_m: 100.0,
                depth_m: 4.0,
                inner_grad: 0.22,
                rim_round_m: 8.0,
                floor_k_m: 3.0,
                outer_grad: 0.30,
                blend_k_m: 2.5,
                outlet,
            };
            let r = ResolvedBowl::new(&b);
            let mut worst = f64::NEG_INFINITY;
            let mut worst_p = Vec2::ZERO;
            for i in 0..400 {
                for j in 0..400 {
                    let p = Vec2::new(1200.0 + i as f64 * 1.5, 1200.0 + j as f64 * 1.5);
                    let lb = r.surface_lower_bound(p);
                    if !lb.is_finite() {
                        continue;
                    }
                    let err = lb - r.surface(p);
                    if err > worst {
                        worst = err;
                        worst_p = p;
                    }
                }
            }
            assert!(
                worst <= 1e-9,
                "lower bound exceeds surface by {worst:.4} m at {worst_p:?}"
            );
        }
    }
}
