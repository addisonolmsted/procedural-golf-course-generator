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
    debug_assert_eq!(implied.data.len(), near.len());
    // The incision field is discontinuous across Dijkstra wavefront
    // boundaries (adjacent cells can carry different source channels with
    // different base elevations) and those Voronoi seams print through the
    // hillshade. Blur the INCISION — never the surface — with a 24 m box
    // footprint: well under the 64 m band boundary, so no structure is
    // touched, only the seam steps.
    let mut incision: Vec<f64> = near
        .iter()
        .zip(&implied.data)
        .map(|(n, _)| {
            if !n.dist_m.is_finite() {
                return 0.0;
            }
            let w = libm::pow(n.dist_m / (n.dist_m + D_HALF_M), THETA);
            (n.implied_channel - n.z_channel) * incision_scale * (1.0 - w)
        })
        .collect();
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    for _ in 0..2 {
        incision = box3(&incision, nx, ny);
    }
    let mut out = implied.clone();
    for (i, inc) in incision.iter().enumerate() {
        out.data[i] = implied.data[i] - inc;
    }
    out
}

/// One 3×3 box-blur pass (edge-clamped).
fn box3(a: &[f64], nx: usize, ny: usize) -> Vec<f64> {
    let mut out = vec![0.0; a.len()];
    for y in 0..ny {
        for x in 0..nx {
            let mut sum = 0.0;
            let mut n = 0.0;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                    if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                        continue;
                    }
                    sum += a[yy as usize * nx + xx as usize];
                    n += 1.0;
                }
            }
            out[y * nx + x] = sum / n;
        }
    }
    out
}
