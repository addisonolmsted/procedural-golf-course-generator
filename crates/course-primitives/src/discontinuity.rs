//! Province boundary construction: a bowed line clipped to the box, entering
//! and exiting (a boundary terminating mid-site is not a province boundary).

use course_world::math::Vec2;
use course_world::world::EXTENT_M;

/// Signed side of the (unbowed) boundary line for a world point: the fields
/// step across this. Positive = the side the normal points to.
pub fn side_of(p: Vec2, center: Vec2, dir: Vec2) -> f64 {
    let n = Vec2::new(-dir.y, dir.x);
    (p.x - center.x) * n.x + (p.y - center.y) * n.y
}

/// Polyline of the boundary: a line through `center` along `dir`, bowed by
/// `bow` metres at mid-span, clipped to the box with endpoints ON the box
/// boundary.
pub fn curve(center: Vec2, dir: Vec2, bow: f64) -> Vec<Vec2> {
    // Find the parameter range [t0, t1] where center + t*dir is inside the box.
    let mut t0 = -2.0 * EXTENT_M;
    let mut t1 = 2.0 * EXTENT_M;
    for (c, d) in [(center.x, dir.x), (center.y, dir.y)] {
        if d.abs() > 1e-12 {
            let a = (0.0 - c) / d;
            let b = (EXTENT_M - c) / d;
            let (lo, hi) = if a < b { (a, b) } else { (b, a) };
            t0 = t0.max(lo);
            t1 = t1.min(hi);
        }
    }
    let n = Vec2::new(-dir.y, dir.x);
    let k = 9usize;
    (0..k)
        .map(|i| {
            let s = i as f64 / (k - 1) as f64;
            let t = t0 + s * (t1 - t0);
            // Bow: zero at the endpoints so entry/exit stay on the boundary.
            let b = bow * libm::sin(s * std::f64::consts::PI);
            Vec2::new(
                (center.x + t * dir.x + b * n.x).clamp(0.0, EXTENT_M),
                (center.y + t * dir.y + b * n.y).clamp(0.0, EXTENT_M),
            )
        })
        .collect()
}
