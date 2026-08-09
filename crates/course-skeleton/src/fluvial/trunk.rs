//! Channel path growth — the trunk and every tributary grow through this
//! one routine. A path starts somewhere and walks UPSTREAM in fixed-length
//! steps, steered by the C1 predisposition fields and deflected or truncated
//! by discontinuities per their kind (stage-02 open question 2, adopted):
//!
//! - **Scarp**: tributaries TRUNCATE at it (drainage on a scarp face starts
//!   over); the trunk crosses undeflected (a water gap with a knick).
//! - **Valley wall**: the step direction loses its wall-normal component
//!   within the influence band — channels run along the wall, not through.
//! - **Material contact**: the direction refracts toward the softer side
//!   within the band; no truncation.
//!
//! Every drawn jitter comes from a pre-drawn slice — the caller draws a
//! fixed count regardless of how much of it growth consumes, so the stream
//! transcript never depends on geometry.

use course_contracts::biome::BoundaryKind;
use course_contracts::metadata::{Edge, StructureMeta};
use course_world::grid::Grid;
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

/// One growth step, metres. 8 m grid → each step spans ~15 cells, long
/// enough that direction changes read as valley bends, not noise.
pub const STEP_M: f64 = 120.0;
/// Growth stops this close to any box edge (other than sprouting near the
/// base edge itself).
pub const EDGE_MARGIN_M: f64 = 60.0;
/// Half-width of a discontinuity's influence band.
pub const DISC_BAND_M: f64 = 90.0;

/// Steering context: the C1 fields growth reads (all 8 m).
pub struct Steer<'a> {
    /// tilt + relief summed — the implied macro surface.
    pub implied: &'a Grid<f64>,
    pub accommodation: &'a Grid<f64>,
    pub hardness: &'a Grid<f64>,
    pub meta: &'a StructureMeta,
}

impl Steer<'_> {
    pub(crate) fn grad(&self, g: &Grid<f64>, p: Vec2) -> Vec2 {
        let h = 8.0;
        let gx = (g.bilinear(Vec2::new(p.x + h, p.y)) - g.bilinear(Vec2::new(p.x - h, p.y)))
            / (2.0 * h);
        let gy = (g.bilinear(Vec2::new(p.x, p.y + h)) - g.bilinear(Vec2::new(p.x, p.y - h)))
            / (2.0 * h);
        Vec2::new(gx, gy)
    }
}

/// Point on the box boundary for an edge at parameter `t` ∈ [0,1]; corners
/// pin to the corner and spread along both faces.
pub fn edge_point(edge: Edge, t: f64) -> Vec2 {
    let e = EXTENT_M;
    let s = t * e;
    match edge {
        Edge::S => Vec2::new(s, 0.0),
        Edge::N => Vec2::new(s, e),
        Edge::W => Vec2::new(0.0, s),
        Edge::E => Vec2::new(e, s),
        // Corner outlets: walk the two adjoining half-edges.
        Edge::CornerSw => {
            if t < 0.5 {
                Vec2::new(t * e, 0.0)
            } else {
                Vec2::new(0.0, (t - 0.5) * e)
            }
        }
        Edge::CornerSe => {
            if t < 0.5 {
                Vec2::new(e - t * e, 0.0)
            } else {
                Vec2::new(e, (t - 0.5) * e)
            }
        }
        Edge::CornerNw => {
            if t < 0.5 {
                Vec2::new(t * e, e)
            } else {
                Vec2::new(0.0, e - (t - 0.5) * e)
            }
        }
        Edge::CornerNe => {
            if t < 0.5 {
                Vec2::new(e - t * e, e)
            } else {
                Vec2::new(e, e - (t - 0.5) * e)
            }
        }
    }
}

/// Inward unit normal for an edge (corner edges point diagonally inward).
pub fn edge_inward(edge: Edge) -> Vec2 {
    let d = std::f64::consts::FRAC_1_SQRT_2;
    match edge {
        Edge::S => Vec2::new(0.0, 1.0),
        Edge::N => Vec2::new(0.0, -1.0),
        Edge::W => Vec2::new(1.0, 0.0),
        Edge::E => Vec2::new(-1.0, 0.0),
        Edge::CornerSw => Vec2::new(d, d),
        Edge::CornerSe => Vec2::new(-d, d),
        Edge::CornerNw => Vec2::new(d, -d),
        Edge::CornerNe => Vec2::new(-d, -d),
    }
}

pub(crate) fn norm(v: Vec2) -> Vec2 {
    let l = (v.x * v.x + v.y * v.y).sqrt();
    if l > 1e-12 {
        Vec2::new(v.x / l, v.y / l)
    } else {
        Vec2::new(0.0, 0.0)
    }
}

pub fn rotate(v: Vec2, a: f64) -> Vec2 {
    let (s, c) = (libm::sin(a), libm::cos(a));
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Signed distance-ish side of `p` relative to the polyline: sign of the
/// cross product against the nearest segment, with the distance to it.
pub(crate) fn nearest_side(curve: &[Vec2], p: Vec2) -> (f64, f64) {
    let mut best_d2 = f64::INFINITY;
    let mut side = 0.0;
    for w in curve.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ab = Vec2::new(b.x - a.x, b.y - a.y);
        let ap = Vec2::new(p.x - a.x, p.y - a.y);
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let t = if len2 > 1e-12 {
            ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = Vec2::new(a.x + ab.x * t, a.y + ab.y * t);
        let dq = Vec2::new(p.x - q.x, p.y - q.y);
        let d2 = dq.x * dq.x + dq.y * dq.y;
        if d2 < best_d2 {
            best_d2 = d2;
            side = ab.x * ap.y - ab.y * ap.x;
        }
    }
    (side, best_d2.sqrt())
}

/// Tangent direction of the polyline near `p` (nearest segment's direction).
pub(crate) fn nearest_tangent(curve: &[Vec2], p: Vec2) -> Vec2 {
    let mut best_d2 = f64::INFINITY;
    let mut tan = Vec2::new(1.0, 0.0);
    for w in curve.windows(2) {
        let mid = Vec2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5);
        let d = Vec2::new(p.x - mid.x, p.y - mid.y);
        let d2 = d.x * d.x + d.y * d.y;
        if d2 < best_d2 {
            best_d2 = d2;
            tan = norm(Vec2::new(w[1].x - w[0].x, w[1].y - w[0].y));
        }
    }
    tan
}

/// Why growth stopped — the caller records truncations for diagnostics.
#[derive(PartialEq, Eq, Debug, Clone, Copy)]
pub enum Stop {
    Budget,
    EdgeMargin,
    Scarp,
}

/// Grow a path upstream from `start`.
///
/// `jitters` supplies one pre-drawn uniform per potential step (length =
/// max step count); growth consumes a prefix. `is_trunk` selects the scarp
/// behavior (cross vs truncate).
#[allow(clippy::too_many_arguments)]
pub fn grow(
    steer: &Steer,
    start: Vec2,
    dir0: Vec2,
    max_len_m: f64,
    step_m: f64,
    jitters: &[f64],
    is_trunk: bool,
) -> (Vec<Vec2>, Stop) {
    let mut pts = vec![start];
    let mut dir = norm(dir0);
    let mut p = start;
    let mut stop = Stop::Budget;
    // A path may never revisit a cell it already occupies — a looping
    // channel is non-physical and the review caught one.
    let cell8 = 8.0;
    let key = |q: Vec2| ((q.y / cell8) as i64, (q.x / cell8) as i64);
    let mut visited: std::collections::BTreeSet<(i64, i64)> = std::collections::BTreeSet::new();
    visited.insert(key(start));
    let steps = ((max_len_m / step_m).round() as usize).min(jitters.len());
    let grain = steer.meta.grain_axis_rad;
    let grain_w = 0.25 * steer.meta.grain_strength;

    for &u in jitters.iter().take(steps) {
        // Steering blend: keep heading, climb the implied surface (upstream
        // is uphill), seek accommodation, align to grain, jitter.
        let up = norm(steer.grad(steer.implied, p));
        let acc = norm(steer.grad(steer.accommodation, p));
        let gvec = Vec2::new(libm::cos(grain), libm::sin(grain));
        // grain is an axis, not a direction: take the sign that agrees.
        let gsign = if gvec.x * dir.x + gvec.y * dir.y >= 0.0 { 1.0 } else { -1.0 };
        let mut d = Vec2::new(
            1.35 * dir.x + 0.85 * up.x + 0.45 * acc.x + grain_w * gsign * gvec.x,
            1.35 * dir.y + 0.85 * up.y + 0.45 * acc.y + grain_w * gsign * gvec.y,
        );
        d = rotate(norm(d), (u - 0.5) * 0.9);

        // Discontinuity behavior inside the influence band.
        let mut truncated = false;
        for disc in &steer.meta.discontinuities {
            let (_, dist) = nearest_side(&disc.curve, p);
            if dist > DISC_BAND_M {
                continue;
            }
            match disc.kind {
                BoundaryKind::Scarp => {
                    if !is_trunk {
                        truncated = true;
                    }
                }
                BoundaryKind::ValleyWall => {
                    let t = nearest_tangent(&disc.curve, p);
                    let along = d.x * t.x + d.y * t.y;
                    let sign = if along >= 0.0 { 1.0 } else { -1.0 };
                    d = norm(Vec2::new(sign * t.x, sign * t.y));
                }
                BoundaryKind::MaterialContact => {
                    // Refract toward softer ground: nudge along -grad(hardness).
                    let soft = norm(steer.grad(steer.hardness, p));
                    d = norm(Vec2::new(d.x - 0.5 * soft.x, d.y - 0.5 * soft.y));
                }
            }
        }
        if truncated {
            stop = Stop::Scarp;
            break;
        }

        let q = Vec2::new(p.x + d.x * step_m, p.y + d.y * step_m);
        if q.x < EDGE_MARGIN_M
            || q.y < EDGE_MARGIN_M
            || q.x > EXTENT_M - EDGE_MARGIN_M
            || q.y > EXTENT_M - EDGE_MARGIN_M
        {
            stop = Stop::EdgeMargin;
            break;
        }
        // 3×3 neighbourhood self-avoidance: an exact-cell test lets wide
        // loops close between cells (the review caught a looping trunk).
        let (ky, kx) = key(q);
        let near_own = (-1..=1).any(|dy| {
            (-1..=1).any(|dx| visited.contains(&(ky + dy, kx + dx)))
        });
        if near_own {
            stop = Stop::EdgeMargin;
            break;
        }
        visited.insert((ky, kx));
        pts.push(q);
        dir = d;
        p = q;
    }
    (pts, stop)
}

/// Arc length of a polyline, metres.
pub fn arc_len(pts: &[Vec2]) -> f64 {
    pts.windows(2)
        .map(|w| {
            let d = Vec2::new(w[1].x - w[0].x, w[1].y - w[0].y);
            (d.x * d.x + d.y * d.y).sqrt()
        })
        .sum()
}

/// Point at arc position `s` along the polyline (clamped), with the local
/// downstream-to-upstream tangent.
pub fn point_at_arc(pts: &[Vec2], s: f64) -> (Vec2, Vec2) {
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let d = Vec2::new(w[1].x - w[0].x, w[1].y - w[0].y);
        let l = (d.x * d.x + d.y * d.y).sqrt();
        if acc + l >= s && l > 1e-12 {
            let t = (s - acc) / l;
            return (
                Vec2::new(w[0].x + d.x * t, w[0].y + d.y * t),
                norm(d),
            );
        }
        acc += l;
    }
    let n = pts.len();
    let tan = if n >= 2 {
        norm(Vec2::new(
            pts[n - 1].x - pts[n - 2].x,
            pts[n - 1].y - pts[n - 2].y,
        ))
    } else {
        Vec2::new(1.0, 0.0)
    };
    (pts[n - 1], tan)
}

