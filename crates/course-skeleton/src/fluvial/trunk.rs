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
    // A path may never revisit a cell it already occupies — a looping or
    // self-crossing channel is non-physical. The set holds EVERY 8 m cell
    // the path's segments pass through (4 m sampling), not just step
    // endpoints: with 70–120 m steps, an endpoint-only set let segments
    // cross between the recorded cells (the review kept catching it).
    let cell8 = 8.0;
    let key = |q: Vec2| ((q.y / cell8) as i64, (q.x / cell8) as i64);
    let mut visited: std::collections::BTreeSet<(i64, i64)> = std::collections::BTreeSet::new();
    visited.insert(key(start));
    let steps = ((max_len_m / step_m).round() as usize).min(jitters.len());
    let grain = steer.meta.grain_axis_rad;
    let grain_w = 0.25 * steer.meta.grain_strength;

    for &u in jitters.iter().take(steps) {
        // Thalweg steering. An earlier draft pulled growth UP the implied
        // gradient ("upstream is uphill") — which walked the network onto
        // the C1 ridges, and 1–16 m of carving cannot sink a ridge-running
        // channel below the surface's real lows (the review measured
        // channels averaging 9.5 m ABOVE the far field: a full inversion).
        // Real networks occupy the lows of the initial surface, so growth
        // now picks, among candidate headings, the one that lands on the
        // LOWEST implied ground, with a momentum penalty for turning, an
        // accommodation bonus, and a grain-alignment bonus. The channel
        // long-profile stays monotone by construction regardless — this is
        // purely planform.
        let gvec = Vec2::new(libm::cos(grain), libm::sin(grain));
        // Trunks must PROGRESS INLAND: pure thalweg-seeking rounds a high
        // and follows the lows straight back toward base level — the
        // review's "horseshoe trunks" (~15% of seeds). Headings with a
        // base-ward component pay a stiff metres-equivalent penalty;
        // tributaries stay free to run any direction.
        let inward = edge_inward(steer.meta.base_level.edge);
        let mut best_d = dir;
        let mut best_score = f64::INFINITY;
        for k in -5i32..=5 {
            let ang = k as f64 * 0.26; // ±75° in 15° steps
            let cand = rotate(dir, ang);
            let q = Vec2::new(p.x + cand.x * step_m, p.y + cand.y * step_m);
            let turn_pen = (1.0 - libm::cos(ang)) * 14.0; // metres-equivalent
            let acc_bonus = steer.accommodation.bilinear(q).min(6.0).max(-6.0) * 0.4;
            let galign = (cand.x * gvec.x + cand.y * gvec.y).abs();
            let grain_bonus = grain_w * 10.0 * galign;
            let baseward_pen = if is_trunk {
                let toward_base = -(cand.x * inward.x + cand.y * inward.y);
                toward_base.max(0.0) * 12.0
            } else {
                0.0
            };
            let score =
                steer.implied.bilinear(q) + turn_pen + baseward_pen - acc_bonus - grain_bonus;
            if score < best_score {
                best_score = score;
                best_d = cand;
            }
        }
        let mut d = rotate(best_d, (u - 0.5) * 0.35);

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
        // Self-avoidance over the WHOLE candidate segment: any sampled
        // cell (beyond the first 20 m, which adjoins the previous segment)
        // that the path already occupies rejects the step.
        let seg_len = step_m;
        let n_sub = (seg_len / 4.0).ceil() as usize;
        let mut collide = false;
        for t in 1..=n_sub {
            let f = t as f64 / n_sub as f64;
            if f * seg_len < 20.0 {
                continue;
            }
            let m = Vec2::new(p.x + d.x * seg_len * f, p.y + d.y * seg_len * f);
            if visited.contains(&key(m)) {
                collide = true;
                break;
            }
        }
        if collide {
            stop = Stop::EdgeMargin;
            break;
        }
        for t in 0..=n_sub {
            let f = t as f64 / n_sub as f64;
            let m = Vec2::new(p.x + d.x * seg_len * f, p.y + d.y * seg_len * f);
            visited.insert(key(m));
        }
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

