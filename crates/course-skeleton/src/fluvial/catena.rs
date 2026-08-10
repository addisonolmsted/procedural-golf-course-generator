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

/// Profile-law curvature exponent (calibration target; provisional).
pub const THETA: f64 = 0.62;
/// Distance over which the hillslope recovers the full incision — of the
/// order of the corpus's d2c p90 (230–270 m). Incision spans the whole
/// channel→divide hillslope; the earlier w=(d/(d+D))^θ (θ<1) profile had
/// divergent slope at d→0 — vertical walls at every channel — and the
/// review read the result as slot trenches in an undrained plateau. The
/// current form has finite channel-wall slope ((1+θ)·inc/D_FULL) and zero
/// slope at the divide (rounded crests).
pub const D_FULL_M: f64 = 260.0;

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
            // Composite concave hillslope: a definite near-channel groove
            // (30% of the incision within the first 40 m — without it the
            // groove is centimetres and any cross-slope of the implied
            // field swamps it, leaving channels perched beside their own
            // valleys) plus the broad rise to the crest over D_FULL.
            let ug = (n.dist_m / 40.0).min(1.0);
            let groove = 1.0 - (1.0 - ug) * (1.0 - ug);
            let uh = (n.dist_m / D_FULL_M).min(1.0);
            let hillslope = 1.0 - libm::pow(1.0 - uh, 1.0 + THETA);
            let w = 0.3 * groove + 0.7 * hillslope;
            (n.implied_channel - n.z_channel) * incision_scale * (1.0 - w)
        })
        .collect();
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    // 4 passes ≈ 72 m footprint: rounds the knife-edge crease where two
    // valleys' incision fields meet at a divide (the review's "ridges much
    // too thin"). Removing sub-64 m creases is band-legal; adding sub-64 m
    // detail would not be.
    for _ in 0..4 {
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
