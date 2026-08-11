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
    // Amplitude calibrated to the real mid-band (64-400 m) residual:
    // corpus sandhills tiles carry ~3.6 m std there; the previous
    // two-sinusoid version had the RIGHT variance but wrong spectrum -
    // all energy in two coherent spikes at one orientation reads as
    // corduroy, while real dune fields spread the same energy across
    // incoherent hummocks (the reviewer's standing "wavy" complaint).
    let amp = intensity * (0.123 * relief_budget_m).clamp(1.0, 5.6);
    // Expand the SAME 5-draw transcript into a many-wave mid-band field
    // via SplitMix64 - deterministic, no transcript change.
    let mut state: u64 = draws
        .iter()
        .enumerate()
        .fold(0u64, |acc, (i, d)| {
            acc ^ ((d.to_bits()).rotate_left(11 * i as u32))
        });
    let mut mix = move || {
        state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut zv = state;
        zv = (zv ^ (zv >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        zv = (zv ^ (zv >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        zv ^= zv >> 31;
        (zv >> 11) as f64 / (1u64 << 53) as f64
    };
    let tau = std::f64::consts::TAU;
    const K: usize = 14;
    let mut comps = Vec::with_capacity(K);
    for _ in 0..K {
        // wavelength log-uniform across the mid band; orientation
        // concentrated around the wind axis with a +/-28 degree spread so
        // trains still read transverse without laser coherence
        let lam = 120.0 * libm::exp(mix() * libm::log(400.0 / 120.0));
        let ang = wind_azimuth_rad + (mix() - 0.5) * 0.98;
        let phase = mix() * tau;
        let a = (0.5 + mix()) * libm::pow(lam / 400.0, 0.6);
        comps.push((lam, libm::cos(ang), libm::sin(ang), phase, a));
    }
    let var: f64 = comps.iter().map(|(_, _, _, _, a)| a * a * 0.5).sum();
    let norm = 1.0 / var.sqrt().max(1e-9);
    // slow cross-wind envelope: fields wax and wane, never fully zero
    let pe = draws[2] * tau;
    let (wx1, wy1) = (libm::cos(wind_azimuth_rad), libm::sin(wind_azimuth_rad));
    let cell = z.spec.cell_size;
    let nx = z.spec.nx as usize;
    for (i, zi) in z.data.iter_mut().enumerate() {
        let x = ((i % nx) as f64 + 0.5) * cell;
        let y = ((i / nx) as f64 + 0.5) * cell;
        let t1 = -x * wy1 + y * wx1;
        let env = 0.62 + 0.38 * libm::sin(tau * t1 / 900.0 + pe);
        let mut field = 0.0;
        for (lam, ca, sa, phase, a) in &comps {
            let u = x * ca + y * sa;
            field += a * libm::sin(tau * u / lam + phase);
        }
        *zi += amp * env * field * norm;
    }
}
