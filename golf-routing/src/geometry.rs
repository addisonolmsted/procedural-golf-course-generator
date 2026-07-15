//! Geometry engine: segment distance/intersection, corridor terrain rules,
//! and the exact-length dogleg solvers.
//!
//! With at most 27 play segments per course, constraint checks are brute-force
//! over a small Vec — no spatial index needed at this scale.

use golf_core::math::{self, Vec2};
use golf_terrain::water::{CLASS_LAKE, CLASS_POND, CLASS_STREAM, CLASS_WETLAND};
use golf_terrain::CourseTerrain;

use crate::fields::cell_of;
use crate::{BOX_MAX, BOX_MIN};

/// Minimum separation between any two lines of play, meters (hard).
pub const BUFFER_M: f64 = 45.0;
/// Maximum dogleg turn angle, radians (~55°).
pub const MAX_TURN: f64 = 55.0 * std::f64::consts::PI / 180.0;
/// A line of play may cross wetland only if the contiguous wet run is shorter
/// than this (a short forced carry).
pub const WETLAND_CARRY_MAX: f64 = 120.0;
/// A stream cell this deep is river-class: it blocks play (matches
/// `WaterLayer::is_blocking`).
const RIVER_DEPTH: f64 = 0.75;

#[inline]
pub fn rot(v: Vec2, ang: f64) -> Vec2 {
    let (s, c) = (math::sin(ang), math::cos(ang));
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

pub fn point_seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    if len2 <= 0.0 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// Minimum distance between two segments (0 if they intersect).
pub fn seg_seg_dist(p1: Vec2, p2: Vec2, q1: Vec2, q2: Vec2) -> f64 {
    if seg_seg_intersect(p1, p2, q1, q2) {
        return 0.0;
    }
    point_seg_dist(p1, q1, q2)
        .min(point_seg_dist(p2, q1, q2))
        .min(point_seg_dist(q1, p1, p2))
        .min(point_seg_dist(q2, p1, p2))
}

fn orient(a: Vec2, b: Vec2, c: Vec2) -> f64 {
    (b - a).perp().dot(c - a)
}

/// Segment intersection, counting touching/collinear overlap as intersecting.
pub fn seg_seg_intersect(p1: Vec2, p2: Vec2, q1: Vec2, q2: Vec2) -> bool {
    let d1 = orient(q1, q2, p1);
    let d2 = orient(q1, q2, p2);
    let d3 = orient(p1, p2, q1);
    let d4 = orient(p1, p2, q2);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    let on = |a: Vec2, b: Vec2, c: Vec2, d: f64| -> bool {
        d == 0.0
            && c.x >= a.x.min(b.x)
            && c.x <= a.x.max(b.x)
            && c.y >= a.y.min(b.y)
            && c.y <= a.y.max(b.y)
    };
    on(q1, q2, p1, d1) || on(q1, q2, p2, d2) || on(p1, p2, q1, d3) || on(p1, p2, q2, d4)
}

pub fn in_box(p: Vec2, margin: f64) -> bool {
    p.x >= BOX_MIN + margin && p.x <= BOX_MAX - margin && p.y >= BOX_MIN + margin && p.y <= BOX_MAX - margin
}

/// Consecutive segments of a polyline.
pub fn segments(pts: &[Vec2]) -> impl Iterator<Item = (Vec2, Vec2)> + '_ {
    pts.windows(2).map(|w| (w[0], w[1]))
}

/// Interior turn angles of a polyline (radians, 0 = straight).
pub fn turn_angles(pts: &[Vec2]) -> Vec<f64> {
    let mut out = Vec::new();
    for k in 1..pts.len().saturating_sub(1) {
        let a = (pts[k] - pts[k - 1]).normalized();
        let b = (pts[k + 1] - pts[k]).normalized();
        out.push(math::acos(a.dot(b).clamp(-1.0, 1.0)));
    }
    out
}

/// Water rule for a line of play: ponds/lakes/river-class stream cells reject;
/// wetland is allowed only in contiguous runs shorter than `WETLAND_CARRY_MAX`;
/// brooks/creeks pass freely (carry hazards).
pub fn corridor_water_ok(ct: &CourseTerrain, a: Vec2, b: Vec2) -> bool {
    let len = a.distance(b);
    let n = (len / 5.0).ceil().max(1.0) as usize;
    let spec = ct.water.class.spec;
    let mut wet_run = 0.0f64;
    let step = len / n as f64;
    for k in 0..=n {
        let p = a.lerp(b, k as f64 / n as f64);
        let i = cell_of(&spec, p);
        match ct.water.class.data[i] {
            CLASS_POND | CLASS_LAKE => return false,
            CLASS_STREAM => {
                if ct.water.depth.data[i] >= RIVER_DEPTH {
                    return false;
                }
                wet_run = 0.0;
            }
            CLASS_WETLAND => {
                wet_run += step;
                if wet_run >= WETLAND_CARRY_MAX {
                    return false;
                }
            }
            _ => wet_run = 0.0,
        }
    }
    true
}

/// Corridor grade rule (soft in scoring; this is the hard backstop): reject
/// if more than `max_steep_frac` of 25 m samples exceed fairway grade. The
/// fraction loosens with the relaxation ladder — mountain courses accept
/// steeper carry sections, matching the real mountain entries in the atlas.
pub fn corridor_grade_ok(ct: &CourseTerrain, a: Vec2, b: Vec2, max_steep_frac: f64) -> bool {
    let len = a.distance(b);
    let n = (len / 25.0).ceil().max(1.0) as usize;
    let mut steep = 0usize;
    for k in 0..=n {
        let p = a.lerp(b, k as f64 / n as f64);
        if ct.macro_slope.bilinear(p) > golf_terrain::FAIRWAY_MAX_GRADE {
            steep += 1;
        }
    }
    (steep as f64) / ((n + 1) as f64) <= max_steep_frac
}

/// Mean corridor slope (for scoring — flatter fairways preferred).
pub fn corridor_mean_slope(ct: &CourseTerrain, a: Vec2, b: Vec2) -> f64 {
    let len = a.distance(b);
    let n = (len / 25.0).ceil().max(1.0) as usize;
    let mut s = 0.0;
    for k in 0..=n {
        let p = a.lerp(b, k as f64 / n as f64);
        s += ct.macro_slope.bilinear(p);
    }
    s / (n + 1) as f64
}

/// Does the segment cross a crossable (brook/creek) stream cell between `lo`
/// and `hi` meters from `a`? Drives the tee "carry" bonus.
pub fn crosses_creek(ct: &CourseTerrain, a: Vec2, b: Vec2, lo: f64, hi: f64) -> bool {
    let len = a.distance(b);
    let n = (len / 5.0).ceil().max(1.0) as usize;
    let spec = ct.water.class.spec;
    for k in 0..=n {
        let t = k as f64 / n as f64;
        let d = t * len;
        if d < lo || d > hi {
            continue;
        }
        let i = cell_of(&spec, a.lerp(b, t));
        if ct.water.class.data[i] == CLASS_STREAM && ct.water.depth.data[i] < RIVER_DEPTH {
            return true;
        }
    }
    false
}

/// Solve the par-4 dogleg: one vertex, legs `a·L` and `(1−a)·L`, total length
/// exactly `target`. The turn angle is implied by the tee→green chord; returns
/// `None` when no vertex with turn ≤ `MAX_TURN` exists. `side` = ±1 picks the
/// dogleg direction.
pub fn solve_dogleg(tee: Vec2, green: Vec2, target: f64, afrac: f64, side: f64) -> Option<(Vec2, f64)> {
    let d = tee.distance(green);
    if d <= 1.0 || target < d {
        return None;
    }
    let (a, b) = (afrac * target, (1.0 - afrac) * target);
    // Interior angle at the vertex from the law of cosines; turn = π − γ.
    let cos_g = ((a * a + b * b - d * d) / (2.0 * a * b)).clamp(-1.0, 1.0);
    let turn = std::f64::consts::PI - math::acos(cos_g);
    if turn > MAX_TURN {
        return None;
    }
    // Circle–circle intersection (radius a about tee, b about green).
    let d1 = (d * d + a * a - b * b) / (2.0 * d);
    let h2 = a * a - d1 * d1;
    let h = if h2 > 0.0 { h2.sqrt() } else { 0.0 };
    let u = (green - tee) * (1.0 / d);
    let v = tee + u * d1 + u.perp() * (side * h);
    Some((v, turn))
}

/// Solve the par-5 double dogleg: leg fractions `f1 < f2`, an aim-off angle
/// `delta1` (radians, signed) at the tee, and `side2` = ±1 for the second
/// vertex. Total length is exactly `target`; returns the two vertices and the
/// larger of the two turn angles, or `None` if either turn exceeds `MAX_TURN`.
pub fn solve_par5(
    tee: Vec2,
    green: Vec2,
    target: f64,
    f1: f64,
    f2: f64,
    delta1: f64,
    side2: f64,
) -> Option<([Vec2; 2], f64)> {
    let d = tee.distance(green);
    if d <= 1.0 || target < d {
        return None;
    }
    let l1 = f1 * target;
    let l2 = (f2 - f1) * target;
    let l3 = (1.0 - f2) * target;
    let dir0 = (green - tee) * (1.0 / d);
    let v1 = tee + rot(dir0, delta1) * l1;
    let c = v1.distance(green);
    if c >= l2 + l3 || c <= (l2 - l3).abs() + 1e-9 {
        return None;
    }
    let cos_g = ((l2 * l2 + l3 * l3 - c * c) / (2.0 * l2 * l3)).clamp(-1.0, 1.0);
    let turn2 = std::f64::consts::PI - math::acos(cos_g);
    if turn2 > MAX_TURN {
        return None;
    }
    let d1 = (c * c + l2 * l2 - l3 * l3) / (2.0 * c);
    let h2 = l2 * l2 - d1 * d1;
    let h = if h2 > 0.0 { h2.sqrt() } else { 0.0 };
    let u = (green - v1) * (1.0 / c);
    let v2 = v1 + u * d1 + u.perp() * (side2 * h);
    // Turn at v1: between the tee leg and the second leg.
    let in1 = (v1 - tee).normalized();
    let out1 = (v2 - v1).normalized();
    let turn1 = math::acos(in1.dot(out1).clamp(-1.0, 1.0));
    if turn1 > MAX_TURN {
        return None;
    }
    Some(([v1, v2], turn1.max(turn2)))
}

/// Polyline length.
pub fn poly_len(pts: &[Vec2]) -> f64 {
    segments(pts).map(|(a, b)| a.distance(b)).sum()
}

/// Is the tee shot blind? Line of sight from tee eye height (1.7 m) to the
/// drive landing point, broken when intervening ground rises above it.
pub fn blind_tee(ct: &CourseTerrain, tee: Vec2, landing: Vec2) -> bool {
    let dist = tee.distance(landing);
    if dist < 40.0 {
        return false;
    }
    let z0 = ct.macro_heights.bilinear(tee) + 1.7;
    let z1 = ct.macro_heights.bilinear(landing) + 0.5;
    let n = (dist / 15.0).ceil() as usize;
    for k in 1..n {
        let t = k as f64 / n as f64;
        let sight = z0 + (z1 - z0) * t;
        if ct.macro_heights.bilinear(tee.lerp(landing, t)) > sight + 0.5 {
            return true;
        }
    }
    false
}

/// Sustained-steep cost past the opening stretch: meters of |grade| beyond
/// 7% integrated over 25 m windows after the first 150 m of arc (tee shots
/// may play over aggressive ground — ideally downhill — but the rest of the
/// hole should stay near fairway grade).
pub fn steep_grade_pen(ct: &CourseTerrain, pts: &[Vec2]) -> f64 {
    let mut pen = 0.0;
    let mut arc = 0.0;
    for w in pts.windows(2) {
        let seg = w[0].distance(w[1]);
        if seg < 1e-9 {
            continue;
        }
        let n = (seg / 25.0).ceil().max(1.0) as usize;
        for k in 0..n {
            let t0 = k as f64 / n as f64;
            let t1 = (k + 1) as f64 / n as f64;
            let a = w[0].lerp(w[1], t0);
            let b = w[0].lerp(w[1], t1);
            let d = a.distance(b);
            let mid = arc + 0.5 * d;
            arc += d;
            if mid < 150.0 || d < 1.0 {
                continue;
            }
            let g = (ct.macro_heights.bilinear(b) - ct.macro_heights.bilinear(a)).abs() / d;
            pen += (g - 0.07).max(0.0) * d;
        }
    }
    pen
}

/// Signed tee→green elevation change of a line of play (m, + = uphill),
/// on the macro landform.
pub fn hole_dz(ct: &CourseTerrain, pts: &[Vec2]) -> f64 {
    ct.macro_heights.bilinear(*pts.last().unwrap()) - ct.macro_heights.bilinear(pts[0])
}

/// Tee elevation minus the path elevation 137 m (150 yd) out — the drive
/// landing zone. Positive = elevated tee overlooking the fairway. Falls back
/// to the green for holes shorter than 137 m.
pub fn drive_drop(ct: &CourseTerrain, pts: &[Vec2]) -> f64 {
    let z_tee = ct.macro_heights.bilinear(pts[0]);
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let seg = w[0].distance(w[1]);
        if acc + seg >= 137.0 && seg > 0.0 {
            let p = w[0].lerp(w[1], (137.0 - acc) / seg);
            return z_tee - ct.macro_heights.bilinear(p);
        }
        acc += seg;
    }
    z_tee - ct.macro_heights.bilinear(*pts.last().unwrap())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seg_dist_basics() {
        let d = seg_seg_dist(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 0.0),
            Vec2::new(0.0, 5.0),
            Vec2::new(10.0, 5.0),
        );
        assert!((d - 5.0).abs() < 1e-12);
        assert!(seg_seg_intersect(
            Vec2::new(0.0, 0.0),
            Vec2::new(10.0, 10.0),
            Vec2::new(0.0, 10.0),
            Vec2::new(10.0, 0.0),
        ));
        assert_eq!(
            seg_seg_dist(
                Vec2::new(0.0, 0.0),
                Vec2::new(10.0, 10.0),
                Vec2::new(0.0, 10.0),
                Vec2::new(10.0, 0.0),
            ),
            0.0
        );
    }

    #[test]
    fn dogleg_hits_target_exactly() {
        let tee = Vec2::new(0.0, 0.0);
        let green = Vec2::new(380.0, 0.0);
        let target = 400.0;
        let (v, turn) = solve_dogleg(tee, green, target, 0.58, 1.0).unwrap();
        let len = tee.distance(v) + v.distance(green);
        assert!((len - target).abs() < 1e-9, "len {len}");
        assert!(turn > 0.0 && turn <= MAX_TURN);
        // Drive leg fraction honored.
        assert!((tee.distance(v) - 0.58 * target).abs() < 1e-9);
        // Mirrored side flips the vertex.
        let (vm, _) = solve_dogleg(tee, green, target, 0.58, -1.0).unwrap();
        assert!((v.y + vm.y).abs() < 1e-9);
    }

    #[test]
    fn dogleg_rejects_too_sharp() {
        let tee = Vec2::new(0.0, 0.0);
        let green = Vec2::new(300.0, 0.0); // needs a big detour
        assert!(solve_dogleg(tee, green, 400.0, 0.58, 1.0).is_none());
    }

    #[test]
    fn par5_hits_target_exactly() {
        let tee = Vec2::new(0.0, 0.0);
        let green = Vec2::new(500.0, 40.0);
        let target = 520.0;
        let got = solve_par5(tee, green, target, 0.37, 0.72, 0.12, -1.0);
        let ([v1, v2], maxturn) = got.expect("solvable");
        let len = tee.distance(v1) + v1.distance(v2) + v2.distance(green);
        assert!((len - target).abs() < 1e-9, "len {len}");
        assert!(maxturn <= MAX_TURN);
    }

    #[test]
    fn turn_angles_straight_is_zero() {
        let pts = [Vec2::new(0.0, 0.0), Vec2::new(1.0, 0.0), Vec2::new(2.0, 0.0)];
        let t = turn_angles(&pts);
        assert!(t[0].abs() < 1e-9);
    }
}
