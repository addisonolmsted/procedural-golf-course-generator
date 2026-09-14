//! Shared geometry: segments, polyline crossings, the clearance profile,
//! the trapezoid score, resampling. Ported from `routing.py` /
//! `siting.py`; points are `(y, x)` world metres.

use crate::Yx;

/// `routing.CLEAR_END_M`: the measured line-of-play clearance at the ends
/// of a hole (p10 profile), metres.
pub const CLEAR_END_M: f64 = 30.0;
/// `routing.CLEAR_MID_M`: the measured clearance mid-hole, metres.
pub const CLEAR_MID_M: f64 = 50.0;
/// `routing.CH_KEEPOUT_M`: the clubhouse keep-out disc (the pad gate is a
/// 25-60 m buildable disc; 45 m covers it), metres.
pub const CH_KEEPOUT_M: f64 = 45.0;
/// `routing.clearance_violation` samples every 15 m (`_resample_t`).
pub const CLEAR_STEP_M: f64 = 15.0;

/// `siting.trapezoid(v, lo, hi, ramp, tail)`: 1 across `[lo, hi]`, ramping
/// linearly from 0 over `r = ramp * (hi - lo)` below `lo`, and above `hi`
/// ramping over the same `r` DOWN TO `tail` (not to zero -- "calmer than
/// the band is dull, not illegal"):
/// `min(clip((v - (lo - r)) / r, 0, 1), 1 - (1 - tail) * clip((v - hi) / r, 0, 1))`.
/// Prototype defaults `ramp = 0.25`, `tail = 0.4`.
pub fn trapezoid(v: f64, lo: f64, hi: f64, ramp: f64, tail: f64) -> f64 {
    let span = hi - lo;
    let r = ramp * span;
    let up = ((v - (lo - r)) / r).clamp(0.0, 1.0);
    let down = 1.0 - (1.0 - tail) * ((v - hi) / r).clamp(0.0, 1.0);
    up.min(down)
}

/// `routing.seg_intersect`'s orientation sign with its 1e-9 dead band.
#[inline]
fn orient(a: Yx, b: Yx, c: Yx) -> i8 {
    let v = (b.1 - a.1) * (c.0 - a.0) - (b.0 - a.0) * (c.1 - a.1);
    if v.abs() < 1e-9 {
        0
    } else if v > 0.0 {
        1
    } else {
        -1
    }
}

/// Strict proper intersection of segments `p-q` and `r-s`
/// (`routing.seg_intersect`): collinear overlaps, shared endpoints and
/// T-junctions (any zero orientation, `|cross| < 1e-9`) are NOT crossings.
pub fn seg_intersect(p: Yx, q: Yx, r: Yx, s: Yx) -> bool {
    let (o1, o2) = (orient(p, q, r), orient(p, q, s));
    let (o3, o4) = (orient(r, s, p), orient(r, s, q));
    o1 != o2 && o3 != o4 && o1 != 0 && o2 != 0 && o3 != 0 && o4 != 0
}

/// `routing._seg_point`: the intersection point of the (assumed crossing)
/// lines `p-q`, `r-s`; the midpoint of `p-q` if parallel (`|den| < 1e-12`).
pub fn seg_point(p: Yx, q: Yx, r: Yx, s: Yx) -> Yx {
    let (d1y, d1x) = (q.0 - p.0, q.1 - p.1);
    let (d2y, d2x) = (s.0 - r.0, s.1 - r.1);
    let den = d1x * d2y - d1y * d2x;
    if den.abs() < 1e-12 {
        return ((p.0 + q.0) / 2.0, (p.1 + q.1) / 2.0);
    }
    let t = ((r.1 - p.1) * d2y - (r.0 - p.0) * d2x) / den;
    (p.0 + t * d1y, p.1 + t * d1x)
}

/// Every crossing point between two polylines, each trimmed `trim_m` at
/// both ends first (`routing.polyline_crossings`, default `trim_m = 8`);
/// all segment pairs, in `(i, j)` order.
pub fn polyline_crossings(a: &[Yx], b: &[Yx], trim_m: f64) -> Vec<Yx> {
    let a = trim_polyline(a, trim_m);
    let b = trim_polyline(b, trim_m);
    let mut out = Vec::new();
    for i in 0..a.len().saturating_sub(1) {
        for j in 0..b.len().saturating_sub(1) {
            if seg_intersect(a[i], a[i + 1], b[j], b[j + 1]) {
                out.push(seg_point(a[i], a[i + 1], b[j], b[j + 1]));
            }
        }
    }
    out
}

/// `routing._trim`: shrink a polyline's FIRST and LAST segment by `trim_m`
/// at the ends (the endpoint moves inward along its own segment; it does
/// not walk the arc), and only when that segment is longer than
/// `2 * trim_m`, so legitimately shared endpoints (walk meets spine at the
/// green) don't count as crossings. Fewer than two points: a copy.
pub fn trim_polyline(a: &[Yx], trim_m: f64) -> Vec<Yx> {
    let mut p = a.to_vec();
    let n = p.len();
    if n < 2 {
        return p;
    }
    for (i, j) in [(0usize, 1usize), (n - 1, n - 2)] {
        let v = (p[j].0 - p[i].0, p[j].1 - p[i].1);
        let len = v.0.hypot(v.1);
        if len > 2.0 * trim_m {
            p[i] = (p[i].0 + v.0 / len * trim_m, p[i].1 + v.1 / len * trim_m);
        }
    }
    p
}

/// Arc length of a polyline (sum of `np.hypot` segment lengths).
pub fn polyline_length(a: &[Yx]) -> f64 {
    a.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum()
}

/// `np.interp(s, cum, coord)` along the polyline: the point at arc length
/// `s`, with NumPy's conventions (`s >= L` -> the last point; a zero-length
/// segment is never the bracketing one because `cum[j] <= s < cum[j+1]`
/// is required).
fn interp_at(a: &[Yx], cum: &[f64], s: f64) -> Yx {
    let n = a.len();
    if s <= 0.0 {
        return a[0];
    }
    if s >= cum[n - 1] {
        return a[n - 1];
    }
    // largest j with cum[j] <= s (binary search on a non-decreasing cum)
    let j = cum.partition_point(|&c| c <= s) - 1;
    let (c0, c1) = (cum[j], cum[j + 1]);
    let slope_y = (a[j + 1].0 - a[j].0) / (c1 - c0);
    let slope_x = (a[j + 1].1 - a[j].1) / (c1 - c0);
    (slope_y * (s - c0) + a[j].0, slope_x * (s - c0) + a[j].1)
}

/// `np.linspace(0, L, n)`: `i * (L / (n - 1))`, the last entry exactly `L`.
fn linspace_arc(l: f64, n: usize) -> Vec<f64> {
    let step = if n > 1 { l / (n - 1) as f64 } else { 0.0 };
    (0..n).map(|i| if i + 1 == n { l } else { i as f64 * step }).collect()
}

/// Resample a polyline at `step_m` along its arc with at least `min_segs`
/// segments: `max(int(L / step_m), min_segs) + 1` points on
/// `np.linspace(0, L)`, each `np.interp`'d from the cumulative arc length.
/// `routing._resample_t` uses `min_segs = 3` (15 m), `routing.spine_profile`
/// `min_segs = 4` (8 m). Endpoints included. Empty input -> empty.
pub fn resample_min(a: &[Yx], step_m: f64, min_segs: usize) -> Vec<Yx> {
    if a.is_empty() {
        return Vec::new();
    }
    let mut cum = Vec::with_capacity(a.len());
    cum.push(0.0);
    for w in a.windows(2) {
        let l = (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1);
        cum.push(cum[cum.len() - 1] + l);
    }
    let big_l = cum[cum.len() - 1];
    let n = ((big_l / step_m) as usize).max(min_segs) + 1;
    linspace_arc(big_l, n).into_iter().map(|s| interp_at(a, &cum, s)).collect()
}

/// Resample a polyline at `step_m` along its arc, endpoints included
/// (`routing._resample_t`'s point set: `max(int(L / step_m), 3) + 1`
/// points; no length gate -- see [`resample_t`] for the gated form).
pub fn resample(a: &[Yx], step_m: f64) -> Vec<Yx> {
    resample_min(a, step_m, 3)
}

/// `routing._resample_t(poly, step_m)`: `(points, t = s / L)`, or `None`
/// when the arc is shorter than `2 * step_m`.
pub fn resample_t(a: &[Yx], step_m: f64) -> Option<(Vec<Yx>, Vec<f64>)> {
    if a.len() < 2 {
        return None;
    }
    let big_l = polyline_length(a);
    if big_l < 2.0 * step_m {
        return None;
    }
    let pts = resample_min(a, step_m, 3);
    let t = linspace_arc(big_l, pts.len()).into_iter().map(|s| s / big_l).collect();
    Some((pts, t))
}

/// `routing._clear_radius(t)`: the measured clearance profile, 30 m at the
/// ends rising over the first/last 30 % of the hole to 50 m mid-hole:
/// `CLEAR_END + (CLEAR_MID - CLEAR_END) * clip(min(t, 1 - t) / 0.3, 0, 1)`.
pub fn clear_radius(t: f64) -> f64 {
    let ramp = (t.min(1.0 - t) / 0.3).clamp(0.0, 1.0);
    CLEAR_END_M + (CLEAR_MID_M - CLEAR_END_M) * ramp
}

/// `routing.clearance_violation(A, B, consecutive)`: the WORST fractional
/// intrusion of two spines into each other's measured clearance radius
/// (30 m at the ends, 50 m mid-hole), both resampled every 15 m
/// (`_resample_t`; 0 if either is shorter than 30 m), over all sample
/// pairs, the radius at a pair being the larger of the two holes' radii at
/// their own `t`. `consecutive` exempts the junction exactly as the
/// prototype does: keep only A's samples with `t <= 0.82` and B's with
/// `t >= 0.18` (0 if either set empties). Returns `max(clip((r - d) / r, 0, 1))`.
pub fn clearance_violation(a: &[Yx], b: &[Yx], consecutive: bool) -> f64 {
    let (Some((pa, ta)), Some((pb, tb))) = (resample_t(a, CLEAR_STEP_M), resample_t(b, CLEAR_STEP_M)) else {
        return 0.0;
    };
    let (mut pa, mut ta, mut pb, mut tb) = (pa, ta, pb, tb);
    if consecutive {
        let keep_a: Vec<usize> = (0..ta.len()).filter(|&i| ta[i] <= 0.82).collect();
        let keep_b: Vec<usize> = (0..tb.len()).filter(|&i| tb[i] >= 0.18).collect();
        pa = keep_a.iter().map(|&i| pa[i]).collect();
        ta = keep_a.iter().map(|&i| ta[i]).collect();
        pb = keep_b.iter().map(|&i| pb[i]).collect();
        tb = keep_b.iter().map(|&i| tb[i]).collect();
        if pa.is_empty() || pb.is_empty() {
            return 0.0;
        }
    }
    let ra: Vec<f64> = ta.iter().map(|&t| clear_radius(t)).collect();
    let rb: Vec<f64> = tb.iter().map(|&t| clear_radius(t)).collect();
    let mut worst = 0.0f64;
    for (i, &p) in pa.iter().enumerate() {
        for (j, &q) in pb.iter().enumerate() {
            let (dy, dx) = (p.0 - q.0, p.1 - q.1);
            let d = (dy * dy + dx * dx).sqrt();
            let r = ra[i].max(rb[j]);
            let v = ((r - d) / r).clamp(0.0, 1.0);
            if v > worst {
                worst = v;
            }
        }
    }
    worst
}

/// `routing.ch_intrusion(spine, ch_yx, trim_m)` (default `trim_m = 20`):
/// fractional intrusion of the clubhouse point into the trimmed spine's
/// keep-out corridor, `clip((CH_KEEPOUT_M - d) / CH_KEEPOUT_M, 0, 1)` with
/// `d` the least point-to-segment distance; 0 = the line stays
/// `>= CH_KEEPOUT_M` away, 1 = dead over. A spine with no segment: 0.
pub fn ch_intrusion(spine: &[Yx], ch_yx: Yx, trim_m: f64) -> f64 {
    let sp = trim_polyline(spine, trim_m);
    let mut best = f64::INFINITY;
    for w in sp.windows(2) {
        let (a2, b2) = (w[0], w[1]);
        let ab = (b2.0 - a2.0, b2.1 - a2.1);
        let ac = (ch_yx.0 - a2.0, ch_yx.1 - a2.1);
        let t = ((ac.0 * ab.0 + ac.1 * ab.1) / (ab.0 * ab.0 + ab.1 * ab.1).max(1e-9)).clamp(0.0, 1.0);
        let d = (a2.0 + t * ab.0 - ch_yx.0).hypot(a2.1 + t * ab.1 - ch_yx.1);
        if d < best {
            best = d;
        }
    }
    ((CH_KEEPOUT_M - best) / CH_KEEPOUT_M).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trapezoid_shape() {
        // lo 2, hi 4, ramp 0.25 -> r = 0.5, tail 0.4
        assert_eq!(trapezoid(3.0, 2.0, 4.0, 0.25, 0.4), 1.0);
        assert_eq!(trapezoid(2.0, 2.0, 4.0, 0.25, 0.4), 1.0);
        assert!((trapezoid(1.75, 2.0, 4.0, 0.25, 0.4) - 0.5).abs() < 1e-12);
        assert_eq!(trapezoid(1.0, 2.0, 4.0, 0.25, 0.4), 0.0);
        assert!((trapezoid(4.25, 2.0, 4.0, 0.25, 0.4) - 0.7).abs() < 1e-12);
        assert!((trapezoid(9.0, 2.0, 4.0, 0.25, 0.4) - 0.4).abs() < 1e-12); // plateaus at tail
    }

    #[test]
    fn seg_intersect_is_strict() {
        let x = |a: (f64, f64), b, c, d| seg_intersect(a, b, c, d);
        assert!(x((0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (2.0, 0.0))); // proper X
        assert!(!x((0.0, 0.0), (1.0, 1.0), (1.0, 1.0), (2.0, 0.0))); // shared endpoint
        assert!(!x((0.0, 0.0), (2.0, 2.0), (1.0, 1.0), (3.0, 3.0))); // collinear overlap
        assert!(!x((0.0, 0.0), (2.0, 0.0), (1.0, 0.0), (1.0, 1.0))); // T-junction
        assert!(!x((0.0, 0.0), (2.0, 0.0), (0.0, 1.0), (2.0, 1.0))); // parallel
        // stopping short of the diagonal (s on r's side): no crossing
        assert!(!x((0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (1.0, 1.0 + 1e-6)));
        // crossing it by 1e-6: a crossing; by 2e-10 (|cross| = 4e-10, inside
        // the 1e-9 dead band): the endpoint reads as ON the line, no crossing
        assert!(x((0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (1.0, 1.0 - 1e-6)));
        assert!(!x((0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (1.0, 1.0 - 2e-10)));
    }

    #[test]
    fn seg_point_hand() {
        let p = seg_point((0.0, 0.0), (2.0, 2.0), (0.0, 2.0), (2.0, 0.0));
        assert!((p.0 - 1.0).abs() < 1e-12 && (p.1 - 1.0).abs() < 1e-12);
        // parallel -> midpoint of p-q
        let m = seg_point((0.0, 0.0), (2.0, 0.0), (1.0, 1.0), (3.0, 1.0));
        assert_eq!(m, (1.0, 0.0));
    }

    #[test]
    fn trim_only_moves_long_end_segments() {
        let a = [(0.0, 0.0), (0.0, 100.0), (50.0, 100.0)];
        let t = trim_polyline(&a, 8.0);
        assert_eq!(t[0], (0.0, 8.0)); // first segment 100 m > 16: moved 8 m in
        assert_eq!(t[1], (0.0, 100.0));
        assert!((t[2].0 - 42.0).abs() < 1e-12 && t[2].1 == 100.0);
        // a 10 m end segment (< 2 * 8) is left alone
        let b = [(0.0, 0.0), (0.0, 10.0), (0.0, 110.0)];
        let t = trim_polyline(&b, 8.0);
        assert_eq!(t[0], (0.0, 0.0));
        assert!((t[2].1 - 102.0).abs() < 1e-12);
        assert_eq!(trim_polyline(&[(1.0, 2.0)], 8.0), vec![(1.0, 2.0)]);
        assert_eq!(polyline_length(&a), 150.0);
        assert_eq!(polyline_length(&[]), 0.0);
    }

    #[test]
    fn crossings_after_trim() {
        // two 100 m lines crossing at (50, 50)
        let a = [(0.0, 0.0), (100.0, 100.0)];
        let b = [(0.0, 100.0), (100.0, 0.0)];
        let c = polyline_crossings(&a, &b, 8.0);
        assert_eq!(c.len(), 1);
        assert!((c[0].0 - 50.0).abs() < 1e-9 && (c[0].1 - 50.0).abs() < 1e-9);
        // sharing an endpoint: not a crossing; and once trimmed, the
        // near-endpoint touch is gone too
        let b2 = [(100.0, 100.0), (200.0, 0.0)];
        assert!(polyline_crossings(&a, &b2, 8.0).is_empty());
        // a crossing 3 m from A's end disappears with trim 8 (trimmed away)
        let b3 = [(97.0, 0.0), (97.0, 200.0)];
        assert_eq!(polyline_crossings(&a, &b3, 0.0).len(), 1);
        assert!(polyline_crossings(&a, &b3, 8.0).is_empty());
    }

    #[test]
    fn resample_hand() {
        let a = [(0.0, 0.0), (0.0, 30.0), (40.0, 30.0)]; // L = 70
        // int(70 / 15) = 4 -> 5 points at 0, 17.5, 35, 52.5, 70 along the arc
        let (p, t) = resample_t(&a, 15.0).unwrap();
        assert_eq!(p.len(), 5);
        assert_eq!(p[0], (0.0, 0.0));
        assert!((p[1].1 - 17.5).abs() < 1e-12 && p[1].0 == 0.0);
        assert!((p[2].0 - 5.0).abs() < 1e-12 && p[2].1 == 30.0);
        assert_eq!(p[4], (40.0, 30.0));
        assert!((t[2] - 0.5).abs() < 1e-12 && t[4] == 1.0);
        // shorter than 2 * step -> None; resample() has no gate (min 3 segs)
        assert!(resample_t(&[(0.0, 0.0), (0.0, 20.0)], 15.0).is_none());
        assert_eq!(resample(&[(0.0, 0.0), (0.0, 20.0)], 15.0).len(), 4);
        assert_eq!(resample_min(&[(0.0, 0.0), (0.0, 20.0)], 8.0, 4).len(), 5);
        // a zero-length leading segment does not break interpolation
        let z = [(0.0, 0.0), (0.0, 0.0), (0.0, 30.0)];
        let p = resample(&z, 15.0);
        assert_eq!(p.len(), 4);
        assert!((p[1].1 - 10.0).abs() < 1e-12);
        assert!(resample(&[], 15.0).is_empty());
        assert_eq!(resample(&[(3.0, 4.0)], 15.0), vec![(3.0, 4.0); 4]);
    }

    #[test]
    fn clear_radius_profile() {
        assert_eq!(clear_radius(0.0), 30.0);
        assert_eq!(clear_radius(1.0), 30.0);
        assert!((clear_radius(0.15) - 40.0).abs() < 1e-12);
        assert_eq!(clear_radius(0.3), 50.0);
        assert_eq!(clear_radius(0.5), 50.0);
        assert_eq!(clear_radius(0.7), 50.0);
        assert!((clear_radius(0.85) - 40.0).abs() < 1e-12);
    }

    #[test]
    fn clearance_violation_hand() {
        // two parallel 300 m spines 100 m apart: no violation (max radius 50)
        let a = [(0.0, 0.0), (0.0, 300.0)];
        let b = [(100.0, 0.0), (100.0, 300.0)];
        assert_eq!(clearance_violation(&a, &b, false), 0.0);
        // 25 m apart: mid-hole radius 50 -> (50 - 25) / 50 = 0.5
        let b = [(25.0, 0.0), (25.0, 300.0)];
        assert!((clearance_violation(&a, &b, false) - 0.5).abs() < 1e-12);
        // identical spines: dead over, 1.0
        assert_eq!(clearance_violation(&a, &a, false), 1.0);
        // too short (< 30 m) on either side: 0
        assert_eq!(clearance_violation(&[(0.0, 0.0), (0.0, 20.0)], &a, false), 0.0);
        // the consecutive exemption: B starts where A ends (a tee beside
        // the previous green) -- without it the junction reads 1.0
        let b = [(0.0, 300.0), (0.0, 600.0)];
        assert_eq!(clearance_violation(&a, &b, false), 1.0);
        // with it, A keeps t <= 0.82 (x <= 246) and B keeps t >= 0.18
        // (x >= 354): nearest pair 108 m apart, no violation
        assert_eq!(clearance_violation(&a, &b, true), 0.0);
        // asymmetric: the gates are on A's END and B's START only
        assert_eq!(clearance_violation(&b, &a, true), 1.0);
    }

    #[test]
    fn ch_intrusion_hand() {
        let s = [(0.0, 0.0), (0.0, 200.0)];
        assert_eq!(ch_intrusion(&s, (0.0, 100.0), 20.0), 1.0); // dead over
        assert_eq!(ch_intrusion(&s, (45.0, 100.0), 20.0), 0.0); // on the keep-out edge
        assert!((ch_intrusion(&s, (22.5, 100.0), 20.0) - 0.5).abs() < 1e-12);
        // the trim: the clubhouse 10 m beyond the tee is 30 m from the
        // trimmed start -> (45 - 30) / 45
        assert!((ch_intrusion(&s, (0.0, -10.0), 20.0) - 15.0 / 45.0).abs() < 1e-12);
        assert_eq!(ch_intrusion(&s, (0.0, -10.0), 0.0), 1.0 - 10.0 / 45.0);
        assert_eq!(ch_intrusion(&[(0.0, 0.0)], (0.0, 0.0), 20.0), 0.0);
    }

    // ---- prototype reference battery (needs kernels_ref.json) --------------

    fn ref_cases() -> Vec<serde_json::Value> {
        let path = std::env::var("COURSE_ROUTING_KERNEL_REF").expect("COURSE_ROUTING_KERNEL_REF");
        let txt = std::fs::read_to_string(path).expect("read reference json");
        serde_json::from_str(&txt).expect("parse")
    }

    fn f(v: &serde_json::Value) -> f64 {
        match v {
            serde_json::Value::String(s) if s == "inf" => f64::INFINITY,
            _ => v.as_f64().unwrap(),
        }
    }

    fn pt(v: &serde_json::Value) -> Yx {
        (f(&v[0]), f(&v[1]))
    }

    fn poly(v: &serde_json::Value) -> Vec<Yx> {
        v.as_array().unwrap().iter().map(pt).collect()
    }

    #[test]
    #[ignore]
    fn prototype_reference_battery() {
        let mut n = 0;
        for c in ref_cases() {
            let kind = c["kind"].as_str().unwrap();
            let cases = match c.get("cases") {
                Some(x) => x.as_array().unwrap(),
                None => continue,
            };
            for (k, x) in cases.iter().enumerate() {
                let name = format!("{kind}#{k}");
                match kind {
                    "seg_intersect" => {
                        let got = seg_intersect(pt(&x["p"]), pt(&x["q"]), pt(&x["r"]), pt(&x["s"]));
                        assert_eq!(got, x["expect"].as_bool().unwrap(), "{name}");
                    }
                    "polyline_crossings" => {
                        let (a, b, trim) = (poly(&x["a"]), poly(&x["b"]), f(&x["trim"]));
                        let ta = trim_polyline(&a, trim);
                        let want_ta = poly(&x["trim_a"]);
                        for (g, w) in ta.iter().zip(&want_ta) {
                            assert!((g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9, "{name}: trim");
                        }
                        let got = polyline_crossings(&a, &b, trim);
                        let want = poly(&x["expect"]);
                        assert_eq!(got.len(), want.len(), "{name}: count");
                        for (g, w) in got.iter().zip(&want) {
                            assert!((g.0 - w.0).abs() < 1e-9 && (g.1 - w.1).abs() < 1e-9, "{name}: {g:?} vs {w:?}");
                        }
                    }
                    "resample_t" => {
                        let (a, step) = (poly(&x["a"]), f(&x["step"]));
                        assert!((polyline_length(&a) - f(&x["length"])).abs() < 1e-9, "{name}: length");
                        let got = resample_t(&a, step);
                        if x["expect"].is_null() {
                            assert!(got.is_none(), "{name}");
                        } else {
                            let (p, t) = got.expect(&name);
                            let (wp, wt) = (poly(&x["expect"]), x["t"].as_array().unwrap());
                            assert_eq!(p.len(), wp.len(), "{name}: count");
                            for i in 0..p.len() {
                                assert!((p[i].0 - wp[i].0).abs() < 1e-9 && (p[i].1 - wp[i].1).abs() < 1e-9, "{name}: pt {i}");
                                assert!((t[i] - f(&wt[i])).abs() < 1e-12, "{name}: t {i}");
                            }
                        }
                    }
                    "clearance_violation" => {
                        let got = clearance_violation(&poly(&x["a"]), &poly(&x["b"]), x["consecutive"].as_bool().unwrap());
                        assert!((got - f(&x["expect"])).abs() < 1e-9, "{name}: {got} vs {}", f(&x["expect"]));
                    }
                    "ch_intrusion" => {
                        let got = ch_intrusion(&poly(&x["spine"]), pt(&x["ch"]), f(&x["trim"]));
                        assert!((got - f(&x["expect"])).abs() < 1e-9, "{name}: {got} vs {}", f(&x["expect"]));
                    }
                    "trapezoid" => {
                        let got = trapezoid(f(&x["v"]), f(&x["lo"]), f(&x["hi"]), f(&x["ramp"]), f(&x["tail"]));
                        assert!((got - f(&x["expect"])).abs() < 1e-12, "{name}");
                    }
                    _ => continue,
                }
                n += 1;
            }
        }
        assert!(n >= 200, "only {n} geom cases ran");
        eprintln!("geom battery: {n} cases matched the prototype");
    }
}
