//! The catena base surface: elevation as a function of flow distance and
//! the nearest channel's base elevation, hung off the C1 implied surface.
//!
//! Deliberately simple (stage-02: "the catena's job is a correct mean
//! surface, not a characterful one"). The construction:
//!
//!   z(x) = implied(x) − (implied_ch(x) − z_ch(x)) · (1 − w(d))
//!   w(d) = (d / (d + D_HALF))^θ
//!
//! where `implied_ch`/`z_ch` ride the Dijkstra wavefront from the nearest
//! channel cell. At a channel cell w=0 and z = z_ch exactly (the base
//! profile, monotone along the network by construction); at the divide
//! w→1 and z returns to C1's implied surface. θ is the profile-law
//! exponent — a calibrated fit target. Candle-wax smoothness between
//! channel and divide is EXPECTED here; texture is S3's job.

use course_world::grid::{Grid, GridSpec};

use super::flow_distance::Nearest;

/// Profile-law exponent (calibration target; provisional).
pub const THETA: f64 = 0.62;
/// Distance at which the catena has recovered half its incision — of the
/// order of the shared invariant `dist_to_channel_p50` (104–120 m).
pub const D_HALF_M: f64 = 110.0;

/// Channel base profile: elevation gain above base level as a function of
/// network arc from the outlet. Concave (slope decays downstream) — the
/// slope–area relation's integral form, kept parametric.
pub fn channel_profile(outlet_arc_m: f64, max_arc_m: f64, incision_m: f64) -> f64 {
    if max_arc_m <= 0.0 {
        return 0.0;
    }
    let s = (outlet_arc_m / max_arc_m).clamp(0.0, 1.0);
    incision_m * libm::pow(s, 1.35)
}

/// Assemble the 8 m base surface from the implied surface + the transform.
/// `incision_scale` ∈ (0,1] shrinks valley depth (negative integration).
pub fn assemble(
    spec: &GridSpec,
    implied: &Grid<f64>,
    near: &[Nearest],
    incision_scale: f64,
) -> Grid<f64> {
    let mut out = implied.clone();
    debug_assert_eq!(out.data.len(), near.len());
    for (i, n) in near.iter().enumerate() {
        if !n.dist_m.is_finite() {
            continue;
        }
        let w = libm::pow(n.dist_m / (n.dist_m + D_HALF_M), THETA);
        let incision = (n.implied_channel - n.z_channel) * incision_scale;
        out.data[i] = implied.data[i] - incision * (1.0 - w);
    }
    let _ = spec;
    out
}
