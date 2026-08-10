//! Stratigraphy / bench module — STRUCTURAL role only: tread and riser
//! GEOMETRY from the strata stack, shared by hill-country, great-plains, and
//! river-valley. Riser-face and tread texture moved to S3's dictionary.
//!
//! Soft elevation quantization: the surface is pulled toward tread levels
//! whose spacing comes from the strata stack, weighted by C1's hardness
//! field (benches express where the caprock is, not uniformly).

use course_contracts::metadata::Stratum;
use course_world::grid::Grid;

use crate::fluvial::flow_distance::Nearest;

/// Apply in place on the 8 m base surface. Benches express on valley
/// SIDES and uplands — never on valley floors (alluvium is not bedrock):
/// the review caught tread edges crossing trunk corridors as evenly
/// spaced perpendicular ridges.
pub fn apply(
    z: &mut Grid<f64>,
    hardness: &Grid<f64>,
    near: &[Nearest],
    strata: &[Stratum],
    intensity: f64,
) {
    if intensity <= 0.0 || strata.is_empty() {
        return;
    }
    // Tread spacing: mean stratum thickness, clamped to the band S2 owns.
    let mean_thick =
        strata.iter().map(|s| s.thickness_m).sum::<f64>() / strata.len() as f64;
    let spacing = mean_thick.clamp(4.0, 24.0);

    for (i, zi) in z.data.iter_mut().enumerate() {
        let frac = *zi / spacing - libm::floor(*zi / spacing); // [0,1) within a band
        // Pull toward the tread (frac→0.5 compresses risers): a smooth
        // sawtooth displacement, strongest mid-riser.
        let pull = -libm::sin(frac * std::f64::consts::TAU) * spacing / std::f64::consts::TAU;
        let dist_fade = (near[i].dist_m / 120.0).clamp(0.0, 1.0);
        let w = (intensity * hardness.data[i]).clamp(0.0, 1.0) * 0.8 * dist_fade;
        *zi += pull * w;
    }
}
