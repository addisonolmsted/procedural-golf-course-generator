//! Per-point conditioning-field functions. Everything here is an ANALYTIC
//! function of world coordinates + the resolved geometry — never derived
//! from a raster — so coarse and fine grids of the same config agree by
//! construction (steps/03 hard requirement 4).

use course_world::ease::smoothstep;
use course_world::math::{atan2, cos, sin, Vec2};
use course_world::world::{CORE_MAX_M, CORE_MIN_M};

use course_world::spline::SegIndex;

use crate::config::Resolved;
use crate::planner::NO_DRAIN_DIST_M;

/// Shoulder width of the core-protect fade (m outside the core window).
pub const CORE_SHOULDER_M: f64 = 150.0;
/// floor_damp fades to 0 this far beyond the local floor half-width.
pub const FLOOR_DAMP_FADE_M: f64 = 150.0;
/// Grain follows the local drain tangent out to about this distance.
pub const GRAIN_BLEND_FADE_M: f64 = 300.0;
/// slope_gain ramp: 0 at this slope …
pub const SLOPE_GAIN_LO: f64 = 0.05;
/// … 1 at this slope (rise/run).
pub const SLOPE_GAIN_HI: f64 = 0.50;

/// 1 inside the routable core, easing to 0 by [`CORE_SHOULDER_M`] outside.
/// Pure function of world coords.
pub fn core_protect_at(p: Vec2) -> f64 {
    let axis = |v: f64| {
        smoothstep(CORE_MIN_M - CORE_SHOULDER_M, CORE_MIN_M, v)
            * (1.0 - smoothstep(CORE_MAX_M, CORE_MAX_M + CORE_SHOULDER_M, v))
    };
    axis(p.x) * axis(p.y)
}

/// Per-drain segment indices, built once and reused for every cell.
///
/// `nearest_drain` is called for every cell of the raster and projects
/// against EVERY drain spine with no bbox — the one field cost that scales
/// linearly with the number of valleys, so it is what decides whether a
/// 30-reach network is affordable. Building an index per spine turns each
/// projection from a ~300-segment scan into a handful of candidates.
pub struct DrainIndex {
    per_valley: Vec<SegIndex>,
}

impl DrainIndex {
    pub fn build(r: &Resolved) -> Self {
        DrainIndex {
            per_valley: (0..r.n_valleys())
                .map(|i| SegIndex::for_spine(r.valley_spine(i)))
                .collect(),
        }
    }
}

/// Nearest-drain query: `(distance, floor_halfwidth_at_hit, tangent)`.
/// Distance is [`NO_DRAIN_DIST_M`] when the config has no drains (the legal
/// empty case — steps/03 contract).
///
/// `index` is optional purely so callers that run once (tests, tooling) need
/// not build one; results are bit-identical either way
/// (`Spine::project_with`).
pub fn nearest_drain_indexed(
    r: &Resolved,
    p: Vec2,
    index: Option<&DrainIndex>,
) -> (f64, f64, Option<Vec2>) {
    let mut d = NO_DRAIN_DIST_M;
    let mut hw = 0.0;
    let mut tangent = None;
    for i in 0..r.n_valleys() {
        let sp = r.valley_spine(i);
        let hit = match index {
            Some(ix) => sp.project_with(&ix.per_valley[i], p),
            None => sp.project(p),
        };
        if hit.d < d {
            d = hit.d;
            hw = r.valley_halfwidth(i, hit.u);
            tangent = Some(sp.tangent_at(hit.u));
        }
    }
    (d, hw, tangent)
}

pub fn nearest_drain(r: &Resolved, p: Vec2) -> (f64, f64, Option<Vec2>) {
    nearest_drain_indexed(r, p, None)
}

/// floor_damp: 1 on the drainage floor (within the local half-width), easing
/// to 0 by [`FLOOR_DAMP_FADE_M`] beyond it.
pub fn floor_damp_at(dist: f64, hw: f64) -> f64 {
    1.0 - smoothstep(hw, hw + FLOOR_DAMP_FADE_M, dist)
}

/// slope_gain from the ANALYTIC slope (central differences of `height_at`
/// with a fixed 2 m offset, independent of raster resolution).
/// NOTE on batching: this costs four `height_at` calls, and an obvious
/// optimization is a `height_at_5` that culls once against the 4 m stencil
/// box. It is NOT done, deliberately — the primitive early-outs test
/// against the RUNNING composed height `z`, which differs per stencil point
/// and evolves differently through the primitive chain, so a shared cull
/// cannot be made bit-identical to five independent evaluations without a
/// bound over all five running values. The M1 early-outs already made each
/// call cheap (30 valleys raster in ~10.5 s), so the complexity is not
/// worth the remaining margin.
pub fn slope_gain_at(r: &Resolved, p: Vec2) -> f64 {
    let e = 2.0;
    let gx = (r.height_at(p + Vec2::new(e, 0.0)) - r.height_at(p - Vec2::new(e, 0.0))) / (2.0 * e);
    let gy = (r.height_at(p + Vec2::new(0.0, e)) - r.height_at(p - Vec2::new(0.0, e))) / (2.0 * e);
    let slope = (gx * gx + gy * gy).sqrt();
    smoothstep(SLOPE_GAIN_LO, SLOPE_GAIN_HI, slope)
}

/// Structural grain (radians mod π): the global grain azimuth, bending to
/// follow the local drain tangent near drainage corridors. Blended in the
/// angle-doubled vector space (grain is undirected).
pub fn grain_at(global_az: f64, dist: f64, hw: f64, tangent: Option<Vec2>) -> f64 {
    let mut cx = cos(2.0 * global_az);
    let mut cy = sin(2.0 * global_az);
    if let Some(t) = tangent {
        let w = 1.0 - smoothstep(hw + 50.0, hw + GRAIN_BLEND_FADE_M, dist);
        let ta = atan2(t.y, t.x);
        cx = w * cos(2.0 * ta) + (1.0 - w) * cx;
        cy = w * sin(2.0 * ta) + (1.0 - w) * cy;
    }
    let mut a = 0.5 * atan2(cy, cx);
    if a < 0.0 {
        a += std::f64::consts::PI;
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn core_protect_window() {
        assert_eq!(core_protect_at(Vec2::new(1500.0, 1500.0)), 1.0);
        assert_eq!(core_protect_at(Vec2::new(400.0, 1500.0)), 0.0);
        let shoulder = core_protect_at(Vec2::new(CORE_MIN_M - 75.0, 1500.0));
        assert!(shoulder > 0.0 && shoulder < 1.0);
    }

    #[test]
    fn grain_mod_pi() {
        let a = grain_at(3.0, NO_DRAIN_DIST_M, 0.0, None);
        assert!((0.0..std::f64::consts::PI).contains(&a));
        // Undirected: az and az+π give the same grain.
        let b = grain_at(3.0 - std::f64::consts::PI, NO_DRAIN_DIST_M, 0.0, None);
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }
}
