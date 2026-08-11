//! The S1 entry point: `SiteSpec` -> `PrimitiveField` (contract C1).
//!
//! Determinism contract — the `primitives/v1` draw transcript, in order,
//! every count fixed:
//!   1 grain axis + 1 grain-strength jitter + 1 base edge + 1 base drop
//!   + 1 tilt jitter + 4 per relief wave (N_WAVES × wavelength/direction/
//!   phase/amplitude uniforms)
//!   + 3 discontinuity params (always drawn, used only with two provinces)
//!   + 2 class params.
//!
//! Every surface primitive here lives at >= 400 m wavelength, so the C1
//! band-limit holds by construction; the constructor's mean-zero invariant is
//! closed numerically at the end.

use course_contracts::contracts::primitive_field::PrimitiveField;
use course_contracts::metadata::{BaseLevel, Discontinuity, Edge, StructureMeta, Stratum};
use course_contracts::units::{normalize_axis, TAU};
use course_seed::{streams, RunIdentity};
use course_spec::v2::SiteSpec;
use course_world::math::Vec2;
use course_world::world::{world_spec, EXTENT_M, RES_PREVIEW_M};
use course_world::Grid;

use crate::{classes, discontinuity};

/// The relief background is a MANY-WAVE band-limited field, not discrete
/// modes: six plane waves measured four orders of magnitude too
/// anisotropic against the corpus (each wave is a single spectral spike
/// with one orientation; real macro terrain spreads power continuously —
/// directional max/min ≈ 22–28). See docs/calibration/e6-report.md,
/// "S1-band spectral realism".
const N_WAVES: usize = 48;
/// Band the waves tile (S1 owns ≥ 400 m; measured organization runs to
/// ~1600 m).
const WAVE_BAND_M: (f64, f64) = (400.0, 1600.0);
/// Spectral tilt: amplitude ∝ (λ/λmax)^(BETA/2) with λ uniform in log λ —
/// FIT dial (targets: per-biome dominant wavelength + long:short power).
const WAVE_BETA: f64 = 1.8;
/// Orientation mixture: this fraction of waves is isotropic (uniform
/// axis); the rest concentrate on the grain axis — FIT dial (target:
/// band anisotropy ratio ≈ real 22–28, class shape included).
const WAVE_ISO_FRAC: f64 = 0.75;
/// Spread of the grain-concentrated waves, radians.
const WAVE_GRAIN_SPREAD: f64 = 0.45;

pub fn generate(spec: &SiteSpec, identity: &RunIdentity) -> PrimitiveField {
    let mut rng = identity.stream(streams::PRIMITIVES);

    // ---- the fixed transcript -------------------------------------------
    let grain_axis_rad = normalize_axis(rng.next_f64() * TAU);
    let grain_base = spec.dials.get("primitives.grain_strength").copied().unwrap_or(0.4);
    let grain_strength = (grain_base + 0.2 * (rng.next_f64() - 0.5)).clamp(0.0, 1.0);
    let edge = match (rng.next_f64() * 4.0) as u32 {
        0 => Edge::N,
        1 => Edge::E,
        2 => Edge::S,
        _ => Edge::W,
    };
    let relief_amp = spec.dials.get("primitives.relief_amp_m").copied().unwrap_or(8.0);
    let base_drop = (0.3 + 0.4 * rng.next_f64()) * relief_amp;
    // Drawn for the transcript; reserved for a jittered tilt direction once
    // the monotonicity check learns tolerance (stage-01 open question).
    let _tilt_jitter = (rng.next_f64() - 0.5) * 0.5;
    // Per-wave draws, interleaved (λ, θ, φ, amp) × N_WAVES — the
    // transcript is the contract.
    let (lam_lo, lam_hi) = WAVE_BAND_M;
    let log_ratio = libm::log(lam_hi / lam_lo);
    let mut waves = Vec::with_capacity(N_WAVES);
    for _ in 0..N_WAVES {
        let u_lam = rng.next_f64();
        let u_dir = rng.next_f64();
        let u_phi = rng.next_f64();
        let u_amp = rng.next_f64();
        let lam = lam_lo * libm::exp(u_lam * log_ratio); // uniform in log λ
        // Orientation mixture: mostly isotropic, some grain-concentrated
        // (grain_strength narrows the concentrated share further).
        let dir = if u_dir < WAVE_ISO_FRAC {
            (u_dir / WAVE_ISO_FRAC) * std::f64::consts::PI
        } else {
            let v = (u_dir - WAVE_ISO_FRAC) / (1.0 - WAVE_ISO_FRAC); // [0,1)
            grain_axis_rad
                + (v - 0.5) * 2.0 * WAVE_GRAIN_SPREAD * (1.0 - 0.5 * grain_strength)
        };
        let phase = u_phi * TAU;
        // amplitude: spectral tilt × jitter; normalized below
        let amp = libm::pow(lam / lam_hi, WAVE_BETA / 2.0) * (0.6 + 0.8 * u_amp);
        waves.push((lam, dir, phase, amp));
    }
    // Normalize so the wave sum has unit std, then scale by the mode share.
    let var: f64 = waves.iter().map(|(_, _, _, a)| a * a * 0.5).sum();
    let wave_norm = 1.0 / var.sqrt().max(1e-9);
    let disc_angle = rng.next_f64() * TAU;
    let disc_offset = (rng.next_f64() - 0.5) * 1200.0;
    let disc_bow = (rng.next_f64() - 0.5) * 700.0;
    // The class draws now carry feature-line curvature (they were
    // reserved in the transcript from the start — no transcript change).
    let class_a = rng.next_f64();
    let class_b = rng.next_f64();
    let line_curve = classes::LineCurve::from_draws(class_a, class_b);

    // ---- geometry helpers ----------------------------------------------
    let toward_edge = match edge {
        Edge::N | Edge::CornerNe | Edge::CornerNw => Vec2::new(0.0, 1.0),
        Edge::S | Edge::CornerSe | Edge::CornerSw => Vec2::new(0.0, -1.0),
        Edge::E => Vec2::new(1.0, 0.0),
        Edge::W => Vec2::new(-1.0, 0.0),
    };
    let tilt_grade = classes::tilt_share(spec.structure_class.window) * relief_amp / EXTENT_M;

    let two_provinces = spec.structure_class.provinces == 2;
    let disc_dir = Vec2::new(libm::cos(disc_angle), libm::sin(disc_angle));
    let disc_center = {
        let n = Vec2::new(-disc_dir.y, disc_dir.x);
        Vec2::new(
            EXTENT_M / 2.0 + disc_offset * n.x,
            EXTENT_M / 2.0 + disc_offset * n.y,
        )
    };
    let province_relief = spec
        .dials
        .get("primitives.province_relief_m")
        .copied()
        .unwrap_or(0.0);

    // ---- fields ---------------------------------------------------------
    let grid = world_spec(RES_PREVIEW_M);
    let mut tilt = Grid::filled(grid, 0.0f64);
    let mut relief = Grid::filled(grid, 0.0f64);
    let hardness_base = if spec.descriptors.strata.is_empty() { 0.35 } else { 0.55 };
    let mut hardness = Grid::filled(grid, hardness_base);
    let mut accommodation = Grid::filled(grid, 0.5f64);

    // The class shape CARRIES the macro form; the wave field seasons it.
    // (The 0.85/0.18 split survived the many-wave rework numerically; the
    // C1 legibility gate must be re-run whenever this synthesis changes.)
    let mode_amp = relief_amp * 0.18 * wave_norm;
    for y in 0..grid.ny {
        for x in 0..grid.nx {
            let p = grid.world_of(x, y);
            let (along, cross) = classes::axial(p, toward_edge);

            // Tilt: monotone toward the edge by construction.
            tilt.set(x, y, -tilt_grade * along);

            // Relief: class shape + grain-clustered smooth modes.
            let (class_relief, class_accom, wave_mult) =
                classes::shape(spec.structure_class.window, along, cross, relief_amp * 0.85, line_curve);
            let mut r = class_relief;
            for (lam, dir, phase, amp) in &waves {
                let u = p.x * libm::cos(*dir) + p.y * libm::sin(*dir);
                r += wave_mult * mode_amp * amp * libm::sin(u / lam * TAU + phase);
            }
            let mut a = class_accom;
            let mut h = hardness_base;

            if two_provinces {
                let s = discontinuity::side_of(p, disc_center, disc_dir);
                // Steps in the character fields; a 400 m-smoothed ramp in
                // relief (a hard step would break the band limit).
                let ramp = {
                    let t = (s / 400.0).clamp(-1.0, 1.0);
                    0.5 * (1.0 + t * (1.5 - 0.5 * t * t)) // smooth odd ramp
                };
                h = (h + 0.2 * (ramp - 0.5) * 2.0).clamp(0.0, 1.0);
                a = (a - 0.15 * (ramp - 0.5) * 2.0).clamp(0.0, 1.0);
                r += province_relief * (ramp - 0.5);
            }

            relief.set(x, y, r);
            hardness.set(x, y, h);
            accommodation.set(x, y, a.clamp(0.0, 1.0));
        }
    }

    // Close the mean-zero invariant numerically over the core.
    let mut sum = 0.0;
    let mut n = 0usize;
    for y in 0..grid.ny {
        for x in 0..grid.nx {
            if course_world::world::in_core(grid.world_of(x, y)) {
                sum += *relief.get(x, y);
                n += 1;
            }
        }
    }
    let mean = sum / n.max(1) as f64;
    for v in relief.data.iter_mut() {
        *v -= mean;
    }

    // ---- metadata -------------------------------------------------------
    let discontinuities = if two_provinces {
        vec![Discontinuity {
            kind: spec
                .structure_class
                .boundary_kind
                .expect("two provinces carry a kind (class invariant)"),
            curve: discontinuity::curve(disc_center, disc_dir, disc_bow),
        }]
    } else {
        vec![]
    };
    let meta = StructureMeta {
        grain_axis_rad,
        grain_strength,
        base_level: BaseLevel {
            edge,
            elev_m: -base_drop,
        },
        // ONE wind system: bit-copy of the spec's prevailing wind.
        wind_azimuth_rad: spec.descriptors.wind_azimuth_rad,
        strata: spec
            .descriptors
            .strata
            .iter()
            .map(|s| Stratum {
                strike_axis_rad: grain_axis_rad, // strata align to the fabric
                ..*s
            })
            .collect(),
        discontinuities,
    };

    let spec_digest = *blake3::hash(spec.canonical_json().as_bytes()).as_bytes();
    PrimitiveField::new(
        identity.seed,
        spec_digest,
        grid,
        tilt,
        relief,
        hardness,
        accommodation,
        meta,
    )
    .expect("S1 construction satisfies every C1 invariant by design")
}
