//! Aeolian module — STRUCTURAL role only: dune-train ridges in the base
//! surface at the 64–400 m band, oriented by the wind azimuth in the metadata
//! thread. S1 owns the ≥400 m dune-train envelope; S3's dictionary owns
//! everything below ~64 m (flank form, blowouts, rumple). Three owners, three
//! disjoint bands.
//!
//! Two transverse ridge trains (crests perpendicular to the prevailing
//! wind) at 210 m and 340 m wavelengths — with the two ingredients that
//! keep a parametric dune field from reading as corduroy (the review:
//! "almost every ridge line is parallel"): the crests are SINUOUS
//! (along-crest phase modulation, ~760 m wave) and the second train is
//! ROTATED ~7° off the first, so crests beat against each other and
//! merge/split/terminate the way barchanoid ridges do. A slow cross-train
//! envelope waxes and wanes the trains. Deterministic phases from the
//! module stream.

use course_world::grid::Grid;

/// Uniform draws consumed (two train phases, envelope phase, two
/// crest-wave phases).
pub const DRAWS: usize = 5;

pub fn apply(
    z: &mut Grid<f64>,
    draws: &[f64],
    intensity: f64,
    wind_azimuth_rad: f64,
    relief_budget_m: f64,
) {
    if intensity <= 0.0 {
        return;
    }
    let amp = intensity * (0.14 * relief_budget_m).clamp(1.2, 6.5);
    let (p1, p2, pe, pw1, pw2) = (
        draws[0] * std::f64::consts::TAU,
        draws[1] * std::f64::consts::TAU,
        draws[2] * std::f64::consts::TAU,
        draws[3] * std::f64::consts::TAU,
        draws[4] * std::f64::consts::TAU,
    );
    let tau = std::f64::consts::TAU;
    // Train 1 along the wind; train 2 rotated 7° so the crests beat.
    let az2 = wind_azimuth_rad + 0.122;
    let (wx1, wy1) = (libm::cos(wind_azimuth_rad), libm::sin(wind_azimuth_rad));
    let (wx2, wy2) = (libm::cos(az2), libm::sin(az2));
    let cell = z.spec.cell_size;
    let nx = z.spec.nx as usize;
    for (i, zi) in z.data.iter_mut().enumerate() {
        let x = ((i % nx) as f64 + 0.5) * cell;
        let y = ((i / nx) as f64 + 0.5) * cell;
        let (s1, t1) = (x * wx1 + y * wy1, -x * wy1 + y * wx1);
        let (s2, t2) = (x * wx2 + y * wy2, -x * wy2 + y * wx2);
        // sinuous crests: the along-wind phase waves with the cross-wind
        // coordinate (crest wavelength ~760 m, swing ~±55 m)
        let s1w = s1 + 55.0 * libm::sin(tau * t1 / 760.0 + pw1);
        let s2w = s2 + 55.0 * libm::sin(tau * t2 / 640.0 + pw2);
        // slow envelope: trains wax/wane across ~900 m, never fully zero
        let env = 0.62 + 0.38 * libm::sin(tau * t1 / 900.0 + pe);
        let ridges =
            0.66 * libm::sin(tau * s1w / 210.0 + p1) + 0.34 * libm::sin(tau * s2w / 340.0 + p2);
        *zi += amp * env * ridges;
    }
}
