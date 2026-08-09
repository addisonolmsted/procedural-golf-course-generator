//! Aeolian module — STRUCTURAL role only: dune-train ridges in the base
//! surface at the 64–400 m band, oriented by the wind azimuth in the metadata
//! thread. S1 owns the ≥400 m dune-train envelope; S3's dictionary owns
//! everything below ~64 m (flank form, blowouts, rumple). Three owners, three
//! disjoint bands.
//!
//! Two superposed transverse ridge trains (crests perpendicular to the
//! prevailing wind) at 210 m and 340 m wavelengths, with a slow cross-train
//! amplitude envelope so trains wax and wane rather than corduroy the whole
//! window. Deterministic phases come from the module stream.

use course_world::grid::Grid;

/// Uniform draws consumed (two phases + one envelope phase).
pub const DRAWS: usize = 3;

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
    let (p1, p2, pe) = (
        draws[0] * std::f64::consts::TAU,
        draws[1] * std::f64::consts::TAU,
        draws[2] * std::f64::consts::TAU,
    );
    // wave vector along the wind (crests are perpendicular to it)
    let (wx, wy) = (libm::cos(wind_azimuth_rad), libm::sin(wind_azimuth_rad));
    let cell = z.spec.cell_size;
    let nx = z.spec.nx as usize;
    let tau = std::f64::consts::TAU;
    for (i, zi) in z.data.iter_mut().enumerate() {
        let x = ((i % nx) as f64 + 0.5) * cell;
        let y = ((i / nx) as f64 + 0.5) * cell;
        let s = x * wx + y * wy; // along-wind coordinate
        let t = -x * wy + y * wx; // cross-wind coordinate
        // slow envelope: trains wax/wane across ~900 m, never fully zero
        let env = 0.62 + 0.38 * libm::sin(tau * t / 900.0 + pe);
        let ridges =
            0.66 * libm::sin(tau * s / 210.0 + p1) + 0.34 * libm::sin(tau * s / 340.0 + p2);
        *zi += amp * env * ridges;
    }
}
