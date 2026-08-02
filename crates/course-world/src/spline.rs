//! Spine geometry for swept primitives: Catmull-Rom densify + uniform
//! arc-length resample, plus a projection that returns the TRUE Euclidean
//! distance to the polyline (a perpendicular-only offset under-reports at
//! corners and beyond endpoints; an SDF needs the real distance).
//! Ported from `terrain-v2:golf-landform/src/spline.rs`.

use crate::math::Vec2;
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

    /// Unit tangent (direction of increasing arc) at normalized arc `s`,
    /// from a small central difference. Deterministic.
    pub fn tangent_at(&self, s: f64) -> crate::math::Vec2 {
        let e = 1e-3;
        let a = self.point_at((s - e).max(0.0));
        let b = self.point_at((s + e).min(1.0));
        (b - a).normalized()
    }

    /// Project a world point: nearest point on the polyline, TRUE distance,
    /// and side sign. Deterministic (first-best wins on exact ties).
    pub fn project(&self, p: Vec2) -> SpineHit {
        self.project_segments(p, 0..self.pts.len() - 1)
    }

    /// The projection kernel, over an arbitrary ASCENDING segment iterator.
    /// Keeping one kernel is what makes [`Self::project_with`] bit-exact:
    /// the indexed path feeds the same comparisons in the same order.
    #[inline]
    fn project_segments(&self, p: Vec2, segs: impl Iterator<Item = usize>) -> SpineHit {
        let total = self.length();
        let mut best_d2 = f64::INFINITY;
        let (mut best_u, mut best_side) = (0.0, 1.0);
        for i in segs {
            let a = self.pts[i];
            let b = self.pts[i + 1];
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

    /// Uniform-grid segment index for [`Self::project_with`].
    ///
    /// `cell_m` trades memory for candidate count; ~8x the point spacing is
    /// a good default (see [`SegIndex::for_spine`]).
    pub fn build_index(&self, cell_m: f64) -> SegIndex {
        SegIndex::build(self, cell_m)
    }

    /// [`Self::project`] accelerated by a segment index — **bit-identical**.
    ///
    /// A spine is a linear scan over every segment, which at ~300 segments
    /// per spine and millions of cells is the raster's dominant cost. This
    /// gathers only the segments that can possibly win, then replays the
    /// exact same kernel over them in ASCENDING index order — so the
    /// "first-best wins on exact ties" rule, and therefore every output
    /// bit, is preserved. Correctness is pinned by
    /// `index_projection_is_bit_identical`.
    pub fn project_with(&self, ix: &SegIndex, p: Vec2) -> SpineHit {
        let Some(cands) = ix.candidates(self, p) else {
            return self.project(p);
        };
        self.project_segments(p, cands.into_iter())
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
mod index_tests {
    use super::*;

    fn spines() -> Vec<Spine> {
        // A meandering open spine, a closed ring, a near-degenerate stub,
        // and a self-approaching hairpin (the case where a naive ring
        // search can stop too early).
        let wiggle: Vec<Vec2> = (0..=300)
            .map(|i| {
                let t = i as f64 * 10.0;
                Vec2::new(200.0 + t, 1500.0 + 380.0 * (t / 430.0).sin())
            })
            .collect();
        let ring: Vec<Vec2> = (0..=96)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 96.0;
                Vec2::new(1500.0 + 240.0 * a.cos(), 1500.0 + 190.0 * a.sin())
            })
            .collect();
        let hairpin = vec![
            Vec2::new(500.0, 500.0), Vec2::new(2400.0, 520.0),
            Vec2::new(2420.0, 560.0), Vec2::new(520.0, 580.0),
        ];
        vec![
            Spine::new(wiggle),
            Spine::new(ring),
            Spine::new(vec![Vec2::new(10.0, 10.0), Vec2::new(10.0, 11.0)]),
            Spine::new(hairpin),
        ]
    }

    /// The whole optimization rests on this: the indexed projection must
    /// agree with the linear scan to the BIT, including `u` and `side` on
    /// exact ties, or the goldens move for no semantic reason.
    #[test]
    fn index_projection_is_bit_identical() {
        for (si, s) in spines().iter().enumerate() {
            let ix = SegIndex::for_spine(s);
            for gy in 0..60 {
                for gx in 0..60 {
                    let p = Vec2::new(-200.0 + gx as f64 * 58.0, -200.0 + gy as f64 * 58.0);
                    let a = s.project(p);
                    let b = s.project_with(&ix, p);
                    assert_eq!(
                        (a.d.to_bits(), a.u.to_bits(), a.side.to_bits()),
                        (b.d.to_bits(), b.u.to_bits(), b.side.to_bits()),
                        "spine {si} disagrees at {p:?}: {a:?} vs {b:?}"
                    );
                }
            }
        }
    }

    /// Points exactly on vertices and segment midpoints are the tie-prone
    /// cases the ascending-order replay exists to protect.
    #[test]
    fn index_exact_on_geometry_points() {
        for s in spines() {
            let ix = SegIndex::for_spine(&s);
            for w in s.pts.windows(2) {
                for p in [w[0], w[0].lerp(w[1], 0.5), w[1]] {
                    let a = s.project(p);
                    let b = s.project_with(&ix, p);
                    assert_eq!(a.d.to_bits(), b.d.to_bits());
                    assert_eq!(a.u.to_bits(), b.u.to_bits());
                    assert_eq!(a.side.to_bits(), b.side.to_bits());
                }
            }
        }
    }

    #[test]
    fn index_actually_culls() {
        let s = &spines()[0];
        let ix = SegIndex::for_spine(s);
        let near = ix.candidates(s, Vec2::new(400.0, 1500.0)).expect("culls near the spine");
        assert!(near.len() * 4 < s.pts.len(), "expected a small candidate set, got {}", near.len());
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
                Vec2::new(50.0 * crate::math::cos(a), 50.0 * crate::math::sin(a))
            })
            .collect();
        let s = Spine::new(pts);
        let r = s.min_curvature_radius();
        assert!((r - 50.0).abs() < 1.0, "got {r}");
    }
}

/// Uniform-grid index over a [`Spine`]'s segments, in CSR form.
///
/// Answers "which segments could contain the nearest point to `p`?" without
/// touching the rest. Built once per resolved primitive and reused for
/// every cell of the raster.
#[derive(Clone, Debug)]
pub struct SegIndex {
    origin: Vec2,
    cell: f64,
    nx: i64,
    ny: i64,
    starts: Vec<u32>,
    items: Vec<u32>,
}

impl SegIndex {
    /// A cell size that keeps the candidate count small without exploding
    /// the bucket count: ~8 point-spacings.
    pub fn for_spine(s: &Spine) -> SegIndex {
        let n = (s.pts.len().max(2) - 1) as f64;
        let spacing = (s.length() / n).max(1.0);
        SegIndex::build(s, (spacing * 8.0).clamp(16.0, 256.0))
    }

    pub fn build(s: &Spine, cell_m: f64) -> SegIndex {
        let cell = cell_m.max(1.0);
        let (mut lo, mut hi) = (s.pts[0], s.pts[0]);
        for p in &s.pts {
            lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
            hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
        }
        let nx = (((hi.x - lo.x) / cell).floor() as i64 + 1).max(1);
        let ny = (((hi.y - lo.y) / cell).floor() as i64 + 1).max(1);
        let nbuckets = (nx * ny) as usize;

        // Two passes: count per bucket, then fill (CSR, no per-bucket Vec).
        let n_seg = s.pts.len() - 1;
        let mut counts = vec![0u32; nbuckets + 1];
        let bucket_span = |a: Vec2, b: Vec2| {
            let x0 = (((a.x.min(b.x) - lo.x) / cell).floor() as i64).clamp(0, nx - 1);
            let x1 = (((a.x.max(b.x) - lo.x) / cell).floor() as i64).clamp(0, nx - 1);
            let y0 = (((a.y.min(b.y) - lo.y) / cell).floor() as i64).clamp(0, ny - 1);
            let y1 = (((a.y.max(b.y) - lo.y) / cell).floor() as i64).clamp(0, ny - 1);
            (x0, x1, y0, y1)
        };
        for i in 0..n_seg {
            let (x0, x1, y0, y1) = bucket_span(s.pts[i], s.pts[i + 1]);
            for gy in y0..=y1 {
                for gx in x0..=x1 {
                    counts[(gy * nx + gx) as usize + 1] += 1;
                }
            }
        }
        for i in 1..counts.len() {
            counts[i] += counts[i - 1];
        }
        let starts = counts.clone();
        let mut cursor = counts;
        let mut items = vec![0u32; starts[nbuckets] as usize];
        for i in 0..n_seg {
            let (x0, x1, y0, y1) = bucket_span(s.pts[i], s.pts[i + 1]);
            for gy in y0..=y1 {
                for gx in x0..=x1 {
                    let b = (gy * nx + gx) as usize;
                    items[cursor[b] as usize] = i as u32;
                    cursor[b] += 1;
                }
            }
        }
        SegIndex { origin: lo, cell, nx, ny, starts, items }
    }

    /// Segments that can hold the nearest point to `p`, ASCENDING and
    /// deduplicated, or `None` when a linear scan would be cheaper.
    ///
    /// Rings expand until the nearest possible point of the next ring is
    /// farther than the best distance already found, which is what makes
    /// the candidate set provably sufficient.
    ///
    /// Two guards keep this from being SLOWER than the scan it replaces —
    /// it measured 8x slower on bluffs before they were added, because a
    /// bluff has no bbox so most cells query from far outside the grid:
    /// distant queries bail to the linear scan immediately, and per-ring
    /// work touches only the segments that ring added (the first version
    /// re-sorted and re-measured the whole candidate set every ring, which
    /// is quadratic in the ring count).
    fn candidates(&self, s: &Spine, p: Vec2) -> Option<Vec<usize>> {
        // Distance from p to the grid's bbox. When p is far outside, every
        // segment is roughly equidistant, the ring walk degenerates into
        // sweeping the whole grid, and the index cannot cull anything.
        let hi_x = self.origin.x + self.nx as f64 * self.cell;
        let hi_y = self.origin.y + self.ny as f64 * self.cell;
        let dx = (self.origin.x - p.x).max(p.x - hi_x).max(0.0);
        let dy = (self.origin.y - p.y).max(p.y - hi_y).max(0.0);
        let span = (hi_x - self.origin.x).max(hi_y - self.origin.y);
        if dx.max(dy) > 0.5 * span {
            return None;
        }

        let gx = ((p.x - self.origin.x) / self.cell).floor() as i64;
        let gy = ((p.y - self.origin.y) / self.cell).floor() as i64;
        let mut found: Vec<usize> = Vec::new();
        let mut best_d2 = f64::INFINITY;
        let max_ring = gx
            .abs()
            .max((gx - (self.nx - 1)).abs())
            .max(gy.abs())
            .max((gy - (self.ny - 1)).abs());
        let seg_d2 = |i: usize| {
            let (a, b) = (s.pts[i], s.pts[i + 1]);
            let ab = b - a;
            let len2 = ab.dot(ab);
            let t = if len2 == 0.0 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
            let d = p - (a + ab * t);
            d.dot(d)
        };
        let mut r = 0i64;
        loop {
            // A point in ring r is at least (r-1)*cell away, so once that
            // exceeds the best distance found no further ring can win.
            if r > 0 && best_d2.is_finite() {
                let ring_min = (r - 1) as f64 * self.cell;
                if ring_min * ring_min > best_d2 {
                    break;
                }
            }
            if r > max_ring {
                break;
            }
            let ring_first = found.len();
            for y in (gy - r)..=(gy + r) {
                if y < 0 || y >= self.ny {
                    continue;
                }
                for x in (gx - r)..=(gx + r) {
                    // ring only: skip the interior already visited
                    if r > 0 && (x - gx).abs() != r && (y - gy).abs() != r {
                        continue;
                    }
                    if x < 0 || x >= self.nx {
                        continue;
                    }
                    let b = (y * self.nx + x) as usize;
                    for &seg in &self.items[self.starts[b] as usize..self.starts[b + 1] as usize] {
                        found.push(seg as usize);
                    }
                }
            }
            // Only the segments THIS ring contributed need measuring.
            for &i in &found[ring_first..] {
                best_d2 = best_d2.min(seg_d2(i));
            }
            r += 1;
        }
        found.sort_unstable();
        found.dedup();
        if found.is_empty() || found.len() * 2 >= s.pts.len() {
            return None;
        }
        Some(found)
    }
}
