//! Spine geometry for swept primitives: Catmull-Rom densify + uniform
//! arc-length resample (ported from the golf-holes fairway spine, which this
//! generalizes), plus a projection that returns the TRUE Euclidean distance —
//! `golf_core::spline::SpineCurve::project` returns the perpendicular offset
//! to the nearest segment's infinite line, which under-reports at corners and
//! beyond endpoints; an SDF needs the real distance.

use golf_core::math::Vec2;
use serde::{Deserialize, Serialize};

/// Catmull-Rom through `pts` (ghost-reflected endpoints, so the curve passes
/// through every control point), densified 64x per segment and resampled at
/// ~`step_m` uniform arc length.
pub fn catmull_rom(pts: &[Vec2], step_m: f64) -> Vec<Vec2> {
    let n = pts.len();
    assert!(n >= 2, "catmull_rom needs at least 2 points");
    let reflect = |a: Vec2, b: Vec2| Vec2::new(2.0 * a.x - b.x, 2.0 * a.y - b.y);
    let cr = |p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f64| -> Vec2 {
        let t2 = t * t;
        let t3 = t2 * t;
        Vec2::new(
            0.5 * (2.0 * p1.x
                + (-p0.x + p2.x) * t
                + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3),
            0.5 * (2.0 * p1.y
                + (-p0.y + p2.y) * t
                + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3),
        )
    };
    let mut dense: Vec<Vec2> = Vec::with_capacity(64 * (n - 1) + 1);
    for i in 0..n - 1 {
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p0 = if i == 0 { reflect(pts[0], pts[1]) } else { pts[i - 1] };
        let p3 = if i + 2 < n { pts[i + 2] } else { reflect(pts[n - 1], pts[n - 2]) };
        for k in 0..64 {
            dense.push(cr(p0, p1, p2, p3, k as f64 / 64.0));
        }
    }
    dense.push(pts[n - 1]);

    let mut cum = vec![0.0f64];
    for w in dense.windows(2) {
        cum.push(cum.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *cum.last().unwrap();
    let m = ((total / step_m.max(0.5)).ceil() as usize).max(4);
    let mut out: Vec<Vec2> = Vec::with_capacity(m + 1);
    let mut j = 0usize;
    for k in 0..=m {
        let target = total * k as f64 / m as f64;
        while j + 1 < cum.len() - 1 && cum[j + 1] < target {
            j += 1;
        }
        let span = (cum[j + 1] - cum[j]).max(1e-12);
        let f = ((target - cum[j]) / span).clamp(0.0, 1.0);
        out.push(dense[j].lerp(dense[j + 1], f));
    }
    out
}

/// Result of projecting a world point onto a [`Spine`].
#[derive(Clone, Copy, Debug)]
pub struct SpineHit {
    /// Normalized arc length of the closest point, in [0, 1].
    pub u: f64,
    /// TRUE Euclidean distance to the polyline (>= 0).
    pub d: f64,
    /// Which side of the spine: +1 = left of the tangent, -1 = right,
    /// (sign of the cross product at the nearest segment).
    pub side: f64,
}

/// An arc-length-parameterized polyline centerline. Open (valley/bluff spines)
/// or closed (bowl boundaries — first point repeated last).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spine {
    pub pts: Vec<Vec2>,
    cum: Vec<f64>,
}

impl Spine {
    pub fn new(pts: Vec<Vec2>) -> Self {
        assert!(pts.len() >= 2, "spine needs at least 2 points");
        let mut cum = Vec::with_capacity(pts.len());
        let mut total = 0.0;
        cum.push(0.0);
        for w in pts.windows(2) {
            total += w[0].distance(w[1]);
            cum.push(total);
        }
        Spine { pts, cum }
    }

    pub fn length(&self) -> f64 {
        *self.cum.last().unwrap()
    }

    pub fn is_closed(&self) -> bool {
        self.pts[0].distance(*self.pts.last().unwrap()) < 1e-9
    }

    /// Point at normalized arc length `s` in [0, 1].
    pub fn point_at(&self, s: f64) -> Vec2 {
        let (i, t) = self.locate(s);
        self.pts[i].lerp(self.pts[i + 1], t)
    }

    fn locate(&self, s: f64) -> (usize, f64) {
        let total = self.length();
        if total == 0.0 {
            return (0, 0.0);
        }
        let target = s.clamp(0.0, 1.0) * total;
        // binary search over the cumulative table (spines can be ~1000 pts)
        let mut lo = 0usize;
        let mut hi = self.cum.len() - 1;
        while hi - lo > 1 {
            let mid = (lo + hi) / 2;
            if self.cum[mid] <= target {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        let seg = self.cum[lo + 1] - self.cum[lo];
        let t = if seg == 0.0 { 0.0 } else { (target - self.cum[lo]) / seg };
        (lo, t)
    }

    /// Project a world point: nearest point on the polyline, TRUE distance,
    /// and side sign. Deterministic (first-best wins on exact ties).
    pub fn project(&self, p: Vec2) -> SpineHit {
        let total = self.length();
        let mut best_d2 = f64::INFINITY;
        let (mut best_u, mut best_side) = (0.0, 1.0);
        for (i, seg) in self.pts.windows(2).enumerate() {
            let a = seg[0];
            let b = seg[1];
            let ab = b - a;
            let len2 = ab.dot(ab);
            let t = if len2 == 0.0 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
            let proj = a + ab * t;
            let d = p - proj;
            let d2 = d.dot(d);
            if d2 < best_d2 {
                best_d2 = d2;
                let arclen = self.cum[i] + t * (len2.sqrt());
                best_u = if total == 0.0 { 0.0 } else { arclen / total };
                let cross = ab.x * d.y - ab.y * d.x; // >0 = left of tangent
                best_side = if cross >= 0.0 { 1.0 } else { -1.0 };
            }
        }
        SpineHit { u: best_u, d: best_d2.sqrt(), side: best_side }
    }

    /// Even-odd point-in-polygon test (closed spines only).
    pub fn contains(&self, p: Vec2) -> bool {
        let mut inside = false;
        for w in self.pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            if (a.y > p.y) != (b.y > p.y) {
                let xi = a.x + (p.y - a.y) / (b.y - a.y) * (b.x - a.x);
                if xi > p.x {
                    inside = !inside;
                }
            }
        }
        inside
    }

    /// Axis-aligned bbox padded by `pad` — the per-primitive evaluation
    /// prefilter (blends are EXACT no-ops beyond their band, so skipping
    /// far cells is pure speedup, not an approximation).
    pub fn bbox(&self, pad: f64) -> (Vec2, Vec2) {
        let mut lo = Vec2::new(f64::INFINITY, f64::INFINITY);
        let mut hi = Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY);
        for p in &self.pts {
            lo.x = lo.x.min(p.x);
            lo.y = lo.y.min(p.y);
            hi.x = hi.x.max(p.x);
            hi.y = hi.y.max(p.y);
        }
        (Vec2::new(lo.x - pad, lo.y - pad), Vec2::new(hi.x + pad, hi.y + pad))
    }

    /// Minimum discrete curvature radius along the polyline (meters), from the
    /// circumradius of consecutive point triples. Used by the meander clamp.
    pub fn min_curvature_radius(&self) -> f64 {
        let mut rmin = f64::INFINITY;
        for w in self.pts.windows(3) {
            let (a, b, c) = (w[0], w[1], w[2]);
            let ab = b.distance(a);
            let bc = c.distance(b);
            let ca = a.distance(c);
            let cross = (b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x);
            let area2 = cross.abs();
            if area2 < 1e-12 {
                continue; // collinear = infinite radius
            }
            rmin = rmin.min(ab * bc * ca / (2.0 * area2));
        }
        rmin
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn true_distance_beyond_endpoint() {
        let s = Spine::new(vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]);
        // beyond the end: SpineCurve::project would report v=3 (perp only);
        // the true distance is 5.
        let hit = s.project(Vec2::new(14.0, 3.0));
        assert!((hit.d - 5.0).abs() < 1e-9);
        assert!((hit.u - 1.0).abs() < 1e-9);
        assert_eq!(hit.side, 1.0);
    }

    #[test]
    fn side_signs() {
        let s = Spine::new(vec![Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)]);
        assert_eq!(s.project(Vec2::new(5.0, 2.0)).side, 1.0);
        assert_eq!(s.project(Vec2::new(5.0, -2.0)).side, -1.0);
    }

    #[test]
    fn closed_contains() {
        let sq = Spine::new(vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(0.0, 0.0),
        ]);
        assert!(sq.is_closed());
        assert!(sq.contains(Vec2::new(5.0, 5.0)));
        assert!(!sq.contains(Vec2::new(15.0, 5.0)));
    }

    #[test]
    fn catmull_passes_through_controls() {
        let pts = vec![
            Vec2::new(0.0, 0.0),
            Vec2::new(100.0, 50.0),
            Vec2::new(200.0, -30.0),
            Vec2::new(300.0, 0.0),
        ];
        let dense = catmull_rom(&pts, 4.0);
        for cp in &pts {
            let min_d = dense.iter().map(|p| p.distance(*cp)).fold(f64::INFINITY, f64::min);
            assert!(min_d < 2.5, "control point {cp:?} missed by {min_d}");
        }
        // uniform arc steps
        let steps: Vec<f64> = dense.windows(2).map(|w| w[0].distance(w[1])).collect();
        let mean = steps.iter().sum::<f64>() / steps.len() as f64;
        assert!(steps.iter().all(|s| (s - mean).abs() < 0.5 * mean));
    }

    #[test]
    fn circle_curvature_radius() {
        // a regular polygon approximating a circle of radius 50
        let n = 64;
        let pts: Vec<Vec2> = (0..=n)
            .map(|i| {
                let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
                Vec2::new(50.0 * golf_core::math::cos(a), 50.0 * golf_core::math::sin(a))
            })
            .collect();
        let s = Spine::new(pts);
        let r = s.min_curvature_radius();
        assert!((r - 50.0).abs() < 1.0, "got {r}");
    }
}
