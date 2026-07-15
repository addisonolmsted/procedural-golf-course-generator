//! The shared "spine spline + width profile + falloff" primitive.
//!
//! Reused for fairway corridors, creek centerlines, and ridge/valley landform
//! primitives. A `SpineCurve` is a polyline centerline parameterized by
//! normalized arc length `s in [0, 1]`, plus a `width` profile (half-width as a
//! function of `s`) and a `falloff` profile (edge feather shape). It exposes the
//! along-spine / cross-spine `(u, v)` frame that downstream stages author in.

use crate::math::Vec2;
use serde::{Deserialize, Serialize};

/// A 1D profile defined by knots `(s, value)` with `s` ascending in [0, 1].
/// Sampled by clamped linear interpolation. Used for width, falloff, and
/// longitudinal elevation profiles alike.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Knots sorted by `s` ascending. Must be non-empty.
    pub knots: Vec<(f64, f64)>,
}

impl Profile {
    pub fn constant(v: f64) -> Self {
        Profile {
            knots: vec![(0.0, v)],
        }
    }

    pub fn new(knots: Vec<(f64, f64)>) -> Self {
        assert!(!knots.is_empty(), "profile needs at least one knot");
        debug_assert!(
            knots.windows(2).all(|w| w[0].0 <= w[1].0),
            "profile knots must be ascending in s"
        );
        Profile { knots }
    }

    /// Sample at `s`, clamping outside the knot range.
    pub fn sample(&self, s: f64) -> f64 {
        let k = &self.knots;
        if s <= k[0].0 {
            return k[0].1;
        }
        let last = k[k.len() - 1];
        if s >= last.0 {
            return last.1;
        }
        // Find the bracketing segment (linear scan; profiles are tiny).
        for w in k.windows(2) {
            let (s0, v0) = w[0];
            let (s1, v1) = w[1];
            if s <= s1 {
                let t = if s1 == s0 { 0.0 } else { (s - s0) / (s1 - s0) };
                return v0 + (v1 - v0) * t;
            }
        }
        last.1
    }
}

/// A centerline with width + falloff profiles. The polyline is sampled by
/// normalized arc length so `s` is geometry-independent.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SpineCurve {
    /// Centerline vertices in world XY. At least 2 points.
    pub polyline: Vec<Vec2>,
    /// Half-width as a function of `s`.
    pub width: Profile,
    /// Edge feather shape as a function of `s` (interpretation is caller-side;
    /// carried alongside the curve so the primitive stays self-describing).
    pub falloff: Profile,
    /// Cumulative arc length per vertex (cached at construction — the query
    /// methods are hot paths in patch rasterization and zone tests).
    cum: Vec<f64>,
}

impl SpineCurve {
    pub fn new(polyline: Vec<Vec2>, width: Profile, falloff: Profile) -> Self {
        assert!(polyline.len() >= 2, "spine needs at least 2 points");
        let mut cum = Vec::with_capacity(polyline.len());
        let mut total = 0.0;
        cum.push(0.0);
        for w in polyline.windows(2) {
            total += w[0].distance(w[1]);
            cum.push(total);
        }
        SpineCurve {
            polyline,
            width,
            falloff,
            cum,
        }
    }

    /// Cumulative arc length at each vertex; last entry is total length.
    fn cumulative(&self) -> &[f64] {
        &self.cum
    }

    pub fn length(&self) -> f64 {
        *self.cum.last().unwrap()
    }

    /// Locate normalized `s` within the polyline: returns (segment index i,
    /// local t in [0,1]) such that the point lies on segment [i, i+1].
    fn locate(&self, s: f64, cum: &[f64]) -> (usize, f64) {
        let total = *cum.last().unwrap();
        if total == 0.0 {
            return (0, 0.0);
        }
        let target = s.clamp(0.0, 1.0) * total;
        // Linear scan; corridors have few vertices. Deterministic.
        for i in 0..cum.len() - 1 {
            if target <= cum[i + 1] {
                let seg = cum[i + 1] - cum[i];
                let t = if seg == 0.0 {
                    0.0
                } else {
                    (target - cum[i]) / seg
                };
                return (i, t);
            }
        }
        (self.polyline.len() - 2, 1.0)
    }

    /// Point on the centerline at normalized arc length `s`.
    pub fn point_at(&self, s: f64) -> Vec2 {
        let (i, t) = self.locate(s, self.cumulative());
        self.polyline[i].lerp(self.polyline[i + 1], t)
    }

    /// Unit tangent at `s` (direction of increasing `s`).
    pub fn tangent_at(&self, s: f64) -> Vec2 {
        let (i, _) = self.locate(s, self.cumulative());
        (self.polyline[i + 1] - self.polyline[i]).normalized()
    }

    /// `(tangent, normal)` frame at `s`. Normal is the left-hand perpendicular
    /// of the tangent — the cross-spine `v` axis.
    pub fn frame_at(&self, s: f64) -> (Vec2, Vec2) {
        let t = self.tangent_at(s);
        (t, t.perp())
    }

    pub fn half_width_at(&self, s: f64) -> f64 {
        self.width.sample(s)
    }

    /// World position from corridor-local coordinates: `u` is normalized arc
    /// length, `v` is signed cross-spine offset in world units.
    pub fn uv_to_world(&self, u: f64, v: f64) -> Vec2 {
        let (_, normal) = self.frame_at(u);
        self.point_at(u) + normal * v
    }

    /// Inverse of [`uv_to_world`]: project a world point onto the spine,
    /// returning `(u, v)` where `u` is normalized arc length of the nearest
    /// point and `v` is the signed cross-spine offset. This is the world->local
    /// map used when compositing a displacement layer onto the base field.
    pub fn project(&self, p: Vec2) -> (f64, f64) {
        let cum = self.cumulative();
        let total = *cum.last().unwrap();
        let mut best_d2 = f64::INFINITY;
        let (mut best_u, mut best_v) = (0.0, 0.0);

        for (i, seg) in self.polyline.windows(2).enumerate() {
            let a = seg[0];
            let b = seg[1];
            let ab = b - a;
            let len2 = ab.dot(ab);
            let t = if len2 == 0.0 {
                0.0
            } else {
                ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
            };
            let proj = a + ab * t;
            let d = p - proj;
            let d2 = d.dot(d);
            if d2 < best_d2 {
                best_d2 = d2;
                let arclen = cum[i] + t * ab.length();
                best_u = if total == 0.0 { 0.0 } else { arclen / total };
                let normal = ab.normalized().perp();
                best_v = d.dot(normal);
            }
        }
        (best_u, best_v)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_interpolates_and_clamps() {
        let p = Profile::new(vec![(0.0, 0.0), (0.5, 10.0), (1.0, 0.0)]);
        assert_eq!(p.sample(-1.0), 0.0);
        assert_eq!(p.sample(0.25), 5.0);
        assert_eq!(p.sample(0.5), 10.0);
        assert_eq!(p.sample(0.75), 5.0);
        assert_eq!(p.sample(2.0), 0.0);
    }

    fn straight() -> SpineCurve {
        SpineCurve::new(
            vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)],
            Profile::constant(2.0),
            Profile::constant(1.0),
        )
    }

    #[test]
    fn length_and_point() {
        let s = straight();
        assert_eq!(s.length(), 10.0);
        assert_eq!(s.point_at(0.0), Vec2::new(0.0, 0.0));
        assert_eq!(s.point_at(0.5), Vec2::new(5.0, 0.0));
        assert_eq!(s.point_at(1.0), Vec2::new(10.0, 0.0));
    }

    #[test]
    fn frame_is_along_and_cross() {
        let s = straight();
        let (t, n) = s.frame_at(0.5);
        assert_eq!(t, Vec2::new(1.0, 0.0));
        assert_eq!(n, Vec2::new(0.0, 1.0));
    }

    #[test]
    fn uv_maps_to_offset() {
        let s = straight();
        // Half a unit cross-spine at the midpoint.
        assert_eq!(s.uv_to_world(0.5, 3.0), Vec2::new(5.0, 3.0));
    }

    #[test]
    fn project_inverts_uv_to_world() {
        let s = straight();
        let (u, v) = s.project(Vec2::new(5.0, 3.0));
        assert!((u - 0.5).abs() < 1e-9);
        assert!((v - 3.0).abs() < 1e-9);
    }

    #[test]
    fn project_point_on_centerline_has_zero_v() {
        let s = straight();
        let (_, v) = s.project(Vec2::new(7.0, 0.0));
        assert!(v.abs() < 1e-9);
    }

    #[test]
    fn arc_length_parameterization_is_uniform() {
        // An L-shaped polyline: total length 20, midpoint at the corner.
        let s = SpineCurve::new(
            vec![
                Vec2::new(0.0, 0.0),
                Vec2::new(10.0, 0.0),
                Vec2::new(10.0, 10.0),
            ],
            Profile::constant(1.0),
            Profile::constant(1.0),
        );
        assert_eq!(s.length(), 20.0);
        assert_eq!(s.point_at(0.5), Vec2::new(10.0, 0.0));
    }
}
