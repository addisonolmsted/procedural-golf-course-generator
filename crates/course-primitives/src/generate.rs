//! The S1 entry point: `SiteSpec` -> `PrimitiveField` (contract C1).
//!
//! Determinism contract — the `primitives/v1` draw transcript, in order,
//! every count fixed:
//!   1 grain axis + 1 grain-strength jitter + 1 base edge + 1 base drop
//!   + 1 tilt jitter + 2 per relief mode (K_MODES phases/orientations)
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

/// Relief modes: fixed wavelengths >= 400 m (the C1 band edge).
const MODE_WAVELENGTHS_M: [f64; 6] = [1500.0, 1150.0, 850.0, 650.0, 500.0, 400.0];

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
    let mut phases = [0.0f64; MODE_WAVELENGTHS_M.len()];
    let mut mode_dirs = [0.0f64; MODE_WAVELENGTHS_M.len()];
    for i in 0..MODE_WAVELENGTHS_M.len() {
        phases[i] = rng.next_f64() * TAU;
        // Mode directions cluster around the grain axis by grain_strength.
        let spread = (1.0 - grain_strength) * std::f64::consts::PI;
        mode_dirs[i] = grain_axis_rad + (rng.next_f64() - 0.5) * spread;
    }
    // Wait — the loop above draws 2 per mode interleaved; keep exactly that
    // order forever (the transcript is the contract).
    let disc_angle = rng.next_f64() * TAU;
    let disc_offset = (rng.next_f64() - 0.5) * 1200.0;
    let disc_bow = (rng.next_f64() - 0.5) * 700.0;
    let _class_a = rng.next_f64();
    let _class_b = rng.next_f64();

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

    // The class shape CARRIES the macro form; modes season it. The first
    // legibility renders had these comparable and every class drowned in
    // blob noise — the P1 protocol's first catch.
    let mode_amp = relief_amp * 0.18 / (MODE_WAVELENGTHS_M.len() as f64).sqrt();
    for y in 0..grid.ny {
        for x in 0..grid.nx {
            let p = grid.world_of(x, y);
            let (along, cross) = classes::axial(p, toward_edge);

            // Tilt: monotone toward the edge by construction.
            tilt.set(x, y, -tilt_grade * along);

            // Relief: class shape + grain-clustered smooth modes.
            let (class_relief, class_accom) =
                classes::shape(spec.structure_class.window, along, cross, relief_amp * 0.85);
            let mut r = class_relief;
            for i in 0..MODE_WAVELENGTHS_M.len() {
                let u = p.x * libm::cos(mode_dirs[i]) + p.y * libm::sin(mode_dirs[i]);
                r += mode_amp * libm::sin(u / MODE_WAVELENGTHS_M[i] * TAU + phases[i]);
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
