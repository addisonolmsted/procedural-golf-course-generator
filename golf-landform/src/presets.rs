//! The Stage-2 gate presets — the target configurations the assessment
//! reviews by eye (docs/terrain-v2-plan.md Stage 2 definition of done).
//!
//! Authored on the 3 km x 3 km working box (plan-view geometry re-spanned
//! from the original 2.5 km drafts; cross-section physics unchanged).

use golf_core::math::Vec2;
use golf_core::spline::Profile;

use crate::{
    Bluff, Bowl, BowlBoundary, MacroConfig, MeanderSpec, Outlet, Path, Ridge, Tilt, Valley,
};

fn base(extent: f64) -> MacroConfig {
    MacroConfig {
        schema_version: crate::SCHEMA_VERSION,
        extent_m: extent,
        base_elev_m: 100.0,
        tilt: Tilt::default(),
        ridges: vec![],
        bluffs: vec![],
        bowls: vec![],
        valleys: vec![],
    }
}

/// Deep asymmetric barranca (Riviera-like): a winding canyon whose left wall
/// is markedly steeper, cutting a gently tilted upland.
fn barranca() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: -0.010, grade_y: -0.006, curve_m: 4.0 };
    c.valleys.push(Valley {
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(-144.0, 504.0),
            exit: Vec2::new(3144.0, 2616.0),
            width_m: 90.0,
            intensity: 0.55,
            wavelength_mult: 12.0,
            jitter: 0.35,
            seed: 11,
        }),
        floor_z0_m: 104.0,
        fall_gradient: 0.009,
        floor_halfwidth: Profile::new(vec![(0.0, 12.0), (0.6, 22.0), (1.0, 34.0)]),
        wall_grad_left: 0.62,
        wall_grad_right: 0.28,
        floor_round_m: 5.0,
        shoulder_k_m: 6.0,
        join_trunk: None,
    });
    c
}

/// Broad low floodplain (Valhalla-like): a wide flat-floored valley with a
/// gentle fall, plus a side tributary joining accordantly.
fn floodplain() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: -0.002, grade_y: 0.003, curve_m: 2.0 };
    c.valleys.push(Valley {
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(-120.0, 3072.0),
            exit: Vec2::new(3144.0, 216.0),
            width_m: 260.0,
            intensity: 0.35,
            wavelength_mult: 11.0,
            jitter: 0.3,
            seed: 21,
        }),
        floor_z0_m: 97.5,
        fall_gradient: 0.0016,
        floor_halfwidth: Profile::new(vec![(0.0, 110.0), (0.5, 165.0), (1.0, 190.0)]),
        wall_grad_left: 0.11,
        wall_grad_right: 0.16,
        floor_round_m: 14.0,
        shoulder_k_m: 3.5,
        join_trunk: None,
    });
    c.valleys.push(Valley {
        // side tributary entering from the north-east upland
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(3072.0, 3072.0),
            exit: Vec2::new(1500.0, 1680.0),
            width_m: 70.0,
            intensity: 0.45,
            wavelength_mult: 12.0,
            jitter: 0.3,
            seed: 22,
        }),
        floor_z0_m: 0.0, // accordant: overwritten by join_trunk snapping
        fall_gradient: 0.006,
        floor_halfwidth: Profile::new(vec![(0.0, 10.0), (1.0, 26.0)]),
        wall_grad_left: 0.20,
        wall_grad_right: 0.24,
        floor_round_m: 7.0,
        shoulder_k_m: 4.0,
        join_trunk: Some(0),
    });
    c
}

/// Straight steep canyon: no meander, hard incision.
fn canyon_straight() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: 0.0, grade_y: -0.004, curve_m: 0.0 };
    c.valleys.push(Valley {
        path: Path::Points(vec![
            Vec2::new(288.0, 3144.0),
            Vec2::new(1380.0, 1800.0),
            Vec2::new(2808.0, -144.0),
        ]),
        floor_z0_m: 88.0,
        fall_gradient: 0.007,
        floor_halfwidth: Profile::new(vec![(0.0, 9.0), (1.0, 16.0)]),
        wall_grad_left: 0.75,
        wall_grad_right: 0.70,
        floor_round_m: 3.5,
        shoulder_k_m: 5.0,
        join_trunk: None,
    });
    c
}

/// Ridge spine: a tapered crest line organizing the upland.
fn ridge_spine() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: -0.003, grade_y: -0.005, curve_m: 0.0 };
    c.ridges.push(Ridge {
        path: Path::Points(vec![
            Vec2::new(300.0, 480.0),
            Vec2::new(1200.0, 1260.0),
            Vec2::new(1980.0, 1800.0),
            Vec2::new(2760.0, 2580.0),
        ]),
        crest_z0_m: 148.0,
        fall_gradient: 0.008,
        crest_halfwidth: Profile::new(vec![(0.0, 12.0), (0.5, 26.0), (1.0, 14.0)]),
        flank_grad_left: 0.26,
        flank_grad_right: 0.19,
        crest_round_m: 16.0,
        base_k_m: 6.0,
        emphasis: Profile::new(vec![(0.0, 0.0), (0.22, 1.0), (0.78, 1.0), (1.0, 0.0)]),
    });
    c
}

/// Bluff with a river at the toe: an eased scarp, the (last-carved) river
/// hugging its low side.
fn bluff_river() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: 0.002, grade_y: -0.003, curve_m: 0.0 };
    c.bluffs.push(Bluff {
        path: Path::Points(vec![
            Vec2::new(-180.0, 2220.0),
            Vec2::new(1080.0, 1800.0),
            Vec2::new(1980.0, 1740.0),
            Vec2::new(3180.0, 1320.0),
        ]),
        height_m: 22.0,
        height: Profile::new(vec![(0.0, 1.0), (0.55, 0.85), (1.0, 1.0)]),
        face_grad: 0.55,
        taper_frac: 0.16,
        raise_left: true,
    });
    c.valleys.push(Valley {
        // the river runs ~70 m south of (below) the bluff toe
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(-144.0, 2016.0),
            exit: Vec2::new(3144.0, 1248.0),
            width_m: 60.0,
            intensity: 0.30,
            wavelength_mult: 12.0,
            jitter: 0.3,
            seed: 51,
        }),
        floor_z0_m: 95.5,
        fall_gradient: 0.0015,
        floor_halfwidth: Profile::new(vec![(0.0, 14.0), (1.0, 22.0)]),
        wall_grad_left: 0.28,
        wall_grad_right: 0.20,
        floor_round_m: 5.0,
        shoulder_k_m: 3.0,
        join_trunk: None,
    });
    c
}

/// Dry bowl with a spillway notch draining it to the south-west.
fn bowl_spillway() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: -0.004, grade_y: -0.003, curve_m: 0.0 };
    c.bowls.push(Bowl {
        boundary: BowlBoundary::Blob {
            center: Vec2::new(1560.0, 1620.0),
            radius_m: 456.0,
            wobble: 0.16,
            cycles: 2.3,
            seed: 61,
        },
        rim_z_m: 101.5,
        depth_m: 13.0,
        inner_grad: 0.22,
        rim_round_m: 12.0,
        floor_k_m: 3.0,
        outer_grad: 0.30,
        blend_k_m: 2.5,
        // s along the blob boundary: 0 = +x axis going CCW; ~0.55 -> SW side
        outlet: Outlet::Spillway { at_s: 0.58, halfwidth_m: 192.0, depth_m: 11.5 },
    });
    c
}

/// Bowl lake: the same landform, no outlet — an intentional lake basin.
fn bowl_lake() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: 0.003, grade_y: -0.002, curve_m: 3.0 };
    c.bowls.push(Bowl {
        boundary: BowlBoundary::Blob {
            center: Vec2::new(1440.0, 1500.0),
            radius_m: 360.0,
            wobble: 0.18,
            cycles: 1.1,
            seed: 71,
        },
        rim_z_m: 102.0,
        depth_m: 16.0,
        inner_grad: 0.28,
        rim_round_m: 9.0,
        floor_k_m: 3.0,
        outer_grad: 0.30,
        blend_k_m: 2.5,
        outlet: Outlet::Lake,
    });
    c
}

/// Three tributaries converging on a main river, the trunk widening after
/// every confluence. The tilt AUTHORS the hydrology: terrain falls west->east
/// along the trunk (grade_x < 0) and the north flank sits higher (grade_y > 0),
/// so two tributaries descend from the northern upland and one comes in from
/// the (gently higher, upstream) south-west. Every floor is accordant at its
/// junction and stays below local terrain end to end.
fn river_confluence() -> MacroConfig {
    let mut c = base(3000.0);
    c.tilt = Tilt { grade_x: -0.006, grade_y: 0.004, curve_m: 0.0 };
    // trunk: enters west at ~109 m terrain, exits east at ~90 m
    c.valleys.push(Valley {
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(-120.0, 1920.0),
            exit: Vec2::new(3120.0, 1080.0),
            width_m: 120.0,
            intensity: 0.35,
            wavelength_mult: 12.0,
            jitter: 0.25,
            seed: 81,
        }),
        floor_z0_m: 104.5,
        fall_gradient: 0.0055,
        // half-width steps up after each confluence (~u 0.30 / 0.52 / 0.76)
        floor_halfwidth: Profile::new(vec![
            (0.0, 8.0), (0.26, 9.0), (0.34, 15.0), (0.48, 16.0),
            (0.56, 22.0), (0.72, 23.0), (0.80, 30.0), (1.0, 34.0),
        ]),
        wall_grad_left: 0.16,
        wall_grad_right: 0.13,
        floor_round_m: 9.0,
        shoulder_k_m: 3.5,
        join_trunk: None,
    });
    // tributary A: northern upland, joins early
    c.valleys.push(Valley {
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(600.0, 3120.0),
            exit: Vec2::new(852.0, 1668.0),
            width_m: 60.0,
            intensity: 0.45,
            wavelength_mult: 12.0,
            jitter: 0.3,
            seed: 82,
        }),
        floor_z0_m: 0.0, // accordant via join_trunk
        fall_gradient: 0.006,
        floor_halfwidth: Profile::new(vec![(0.0, 6.0), (1.0, 10.0)]),
        wall_grad_left: 0.20,
        wall_grad_right: 0.24,
        floor_round_m: 6.0,
        shoulder_k_m: 3.0,
        join_trunk: Some(0),
    });
    // tributary B: from the (upstream-high) south-west, joins mid — gentle
    // gradient so its head stays below the flatter southern terrain
    c.valleys.push(Valley {
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(300.0, -180.0),
            exit: Vec2::new(1565.0, 1483.0),
            width_m: 50.0,
            intensity: 0.45,
            wavelength_mult: 12.0,
            jitter: 0.3,
            seed: 83,
        }),
        floor_z0_m: 0.0,
        fall_gradient: 0.0015,
        floor_halfwidth: Profile::new(vec![(0.0, 5.0), (1.0, 8.0)]),
        wall_grad_left: 0.22,
        wall_grad_right: 0.18,
        floor_round_m: 6.0,
        shoulder_k_m: 3.0,
        join_trunk: Some(0),
    });
    // tributary C: northern upland, joins late
    c.valleys.push(Valley {
        path: Path::Meander(MeanderSpec {
            entry: Vec2::new(2100.0, 3144.0),
            exit: Vec2::new(2342.0, 1282.0),
            width_m: 65.0,
            intensity: 0.5,
            wavelength_mult: 12.0,
            jitter: 0.3,
            seed: 84,
        }),
        floor_z0_m: 0.0,
        fall_gradient: 0.005,
        floor_halfwidth: Profile::new(vec![(0.0, 6.0), (1.0, 11.0)]),
        wall_grad_left: 0.24,
        wall_grad_right: 0.20,
        floor_round_m: 6.0,
        shoulder_k_m: 3.0,
        join_trunk: Some(0),
    });
    c
}

pub fn presets() -> Vec<(&'static str, MacroConfig)> {
    vec![
        ("barranca", barranca()),
        ("floodplain", floodplain()),
        ("canyon_straight", canyon_straight()),
        ("ridge_spine", ridge_spine()),
        ("bluff_river", bluff_river()),
        ("bowl_spillway", bowl_spillway()),
        ("bowl_lake", bowl_lake()),
        ("river_confluence", river_confluence()),
    ]
}

pub fn preset_names() -> Vec<&'static str> {
    presets().into_iter().map(|(n, _)| n).collect()
}

pub fn preset(name: &str) -> Option<MacroConfig> {
    presets().into_iter().find(|(n, _)| *n == name).map(|(_, c)| c)
}
