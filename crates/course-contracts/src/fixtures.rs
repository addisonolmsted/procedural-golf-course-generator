//! Fixture builders — minimal *valid* contract instances for testing stages
//! in isolation before their upstream neighbours exist (`ARCHITECTURE.md`
//! separation principle 3).
//!
//! Every builder returns a fully-validated contract via the real constructor,
//! so a fixture can never drift from the invariants. Grids are production
//! rungs over the real extent (C2's play-window arithmetic is absolute), but
//! filled with cheap analytic content.

use crate::biome::{BiomeId, StructureClass, WindowClass};
use crate::contracts::corridor_graph::{
    BorrowSite, Budgets, CorridorGraph, Hole, Node, NodeId, NodeKind,
};
use crate::contracts::delivery::{CourseManifest, PrevailingWind, Provenance, WorldDesc};
use crate::contracts::primitive_field::PrimitiveField;
use crate::contracts::routing_substrate::{
    CostFields, ExclusionMasks, Rect, RoutingSubstrate, ScorerPreset, SubstrateMeta, PLAY_M,
};
use crate::metadata::{BaseLevel, Edge, StructureMeta, WaterPlaneOrigin};
use crate::plasticity::Plasticity;
use crate::PAR_SEQUENCE;
use course_world::math::Vec2;
use course_world::world::{world_spec, EXTENT_M, RES_FULL_M, RES_PREVIEW_M};
use course_world::Grid;

/// A valid `StructureMeta` with mid-range values and one stratum.
pub fn meta() -> StructureMeta {
    StructureMeta {
        grain_axis_rad: 0.7,
        grain_strength: 0.5,
        base_level: BaseLevel {
            edge: Edge::S,
            elev_m: -10.0,
        },
        wind_azimuth_rad: 4.2,
        strata: vec![],
    }
}

/// A valid C1 on the 8 m rung: gentle southward tilt, mean-zero sinusoidal
/// relief, uniform mid hardness/accommodation.
pub fn c1(seed: u64) -> PrimitiveField {
    let spec = world_spec(RES_PREVIEW_M);
    let mut tilt = Grid::filled(spec, 0.0f64);
    let mut relief = Grid::filled(spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let p = spec.world_of(x, y);
            tilt.set(x, y, 0.01 * (p.y - EXTENT_M)); // falls toward the S edge
            // Mean-zero over any window: sin in x around the extent.
            relief.set(
                x,
                y,
                6.0 * libm::sin(p.x / EXTENT_M * std::f64::consts::TAU * 3.0),
            );
        }
    }
    // Numerically re-centre over the core so the invariant holds exactly.
    let mut sum = 0.0;
    let mut n = 0usize;
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            if course_world::world::in_core(spec.world_of(x, y)) {
                sum += *relief.get(x, y);
                n += 1;
            }
        }
    }
    let mean = sum / n as f64;
    for v in relief.data.iter_mut() {
        *v -= mean;
    }
    let hardness = Grid::filled(spec, 0.5f64);
    let accommodation = Grid::filled(spec, 0.5f64);
    PrimitiveField::new(seed, [7u8; 32], spec, tilt, relief, hardness, accommodation, meta())
        .expect("fixture C1 must satisfy its own invariants")
}

/// The centred play window (the fixture default; siting varies it).
pub fn centered_window() -> Rect {
    let mid = EXTENT_M / 2.0;
    Rect {
        min: Vec2::new(mid - PLAY_M / 2.0, mid - PLAY_M / 2.0),
        max: Vec2::new(mid + PLAY_M / 2.0, mid + PLAY_M / 2.0),
    }
}

/// A valid mid-range scorer preset.
pub fn preset() -> ScorerPreset {
    ScorerPreset {
        w_earthwork: 1.0,
        w_clearing: 0.5,
        w_drainage: 0.5,
        w_fit: 1.0,
        water_exclude_threshold: 0.6,
        target_grade_fairway: 0.06,
        target_grade_green: 0.02,
        hole_length_m: [350.0, 360.0, 160.0, 480.0, 370.0, 170.0, 355.0, 490.0, 365.0],
        feasibility_strictness: 0.5,
    }
}

/// A valid C2: gentle sloped height at 2 m, all masks/costs benign at 8 m,
/// centred play window. The "empty case is legal" fixture — water, clearing,
/// and borrow are identically zero (the Sandhills shape).
pub fn c2(seed: u64) -> RoutingSubstrate {
    let grid_full = world_spec(RES_FULL_M);
    let grid_cost = world_spec(RES_FULL_M * 4.0);
    let mut height = Grid::filled(grid_full, 0.0f64);
    for y in 0..grid_full.ny {
        for x in 0..grid_full.nx {
            let p = grid_full.world_of(x, y);
            height.set(x, y, 0.01 * (EXTENT_M - p.y));
        }
    }
    let m = meta();
    let masks = ExclusionMasks {
        water: Grid::filled(grid_cost, 0.0),
        water_surface_m: Grid::filled(grid_cost, 0.0),
        protected: Grid::filled(grid_cost, 0u8),
        out_of_bounds: Grid::filled(grid_cost, 0u8),
        unbuildable: Grid::filled(grid_cost, 0.1),
    };
    let cost = CostFields {
        earthwork: Grid::filled(grid_cost, 5.0),
        clearing: Grid::filled(grid_cost, 0.0),
        borrow: Grid::filled(grid_cost, 0.0),
        drainage_risk: Grid::filled(grid_cost, 0.05),
    };
    let smeta = SubstrateMeta {
        flow_dir_rad: Grid::filled(grid_cost, 3.0 * std::f64::consts::FRAC_PI_2), // south
        flow_accum: Grid::filled(grid_cost, 64.0),
        grain_axis_rad: m.grain_axis_rad,
        wind_azimuth_rad: m.wind_azimuth_rad,
        water_plane_origin: WaterPlaneOrigin::None,
        plasticity: Plasticity::new(0.4).unwrap(),
    };
    RoutingSubstrate::new(
        seed,
        [7u8; 32],
        grid_full,
        grid_cost,
        height,
        centered_window(),
        masks,
        cost,
        smeta,
        preset(),
    )
    .expect("fixture C2 must satisfy its own invariants")
}

/// A valid C3: nine straight holes laid out in a fan inside the centred
/// window, forming a returning loop.
pub fn c3(seed: u64) -> CorridorGraph {
    let w = centered_window();
    let c = w.center();
    let mut nodes = Vec::new();
    let mut holes = Vec::new();
    for i in 0..9u32 {
        let ang = i as f64 / 9.0 * std::f64::consts::TAU;
        let tee_at = Vec2::new(c.x + 40.0 * libm::cos(ang), c.y + 40.0 * libm::sin(ang));
        // Hole 8 returns to the clubhouse: its green comes back beside tee 0.
        let (g_ang, g_r) = if i == 8 { (0.0, 120.0) } else { (ang, 240.0) };
        let green_at = Vec2::new(c.x + g_r * libm::cos(g_ang), c.y + g_r * libm::sin(g_ang));
        let tee = NodeId(i * 2);
        let green = NodeId(i * 2 + 1);
        nodes.push(Node {
            id: tee,
            kind: NodeKind::Tee,
            at: tee_at,
            elev_m: 0.0,
        });
        nodes.push(Node {
            id: green,
            kind: NodeKind::Green,
            at: green_at,
            elev_m: 0.0,
        });
        holes.push(Hole {
            index: i,
            par: PAR_SEQUENCE[i as usize],
            tee,
            green,
            spine: vec![tee_at, green_at],
            halfwidth_m: vec![30.0, 30.0],
            earthwork_budget_m3: 1000.0,
            borrow_sites: vec![BorrowSite {
                at: green_at,
                radius_m: 15.0,
                volume_m3: 600.0,
                becomes_water: false,
            }],
            target_grade: 0.06,
            neighbors: vec![(i + 1) % 9],
        });
    }
    let budgets = Budgets {
        total_cut_m3: 5000.0,
        total_fill_m3: 5000.0,
        total_clearing_m3: 0.0,
        per_hole_ms: 510.0,
        clubhouse_radius_m: 120.0,
    };
    CorridorGraph::new(seed, [7u8; 32], holes, nodes, budgets, Plasticity::new(0.4).unwrap())
        .expect("fixture C3 must satisfy its own invariants")
}

/// A valid C0 manifest with no layers (layers are added by the exporter).
pub fn c0_manifest(seed: u64) -> CourseManifest {
    let m = meta();
    let man = CourseManifest {
        c0_version: crate::contracts::delivery::C0_VERSION,
        seed,
        pipeline_version: 0,
        biome: BiomeId::Piedmont,
        structure_class: StructureClass::new(WindowClass::Interfluve, 1, None).unwrap(),
        play_window: centered_window(),
        world: WorldDesc::default(),
        datum_note: "elevations are metres in a local datum; 0 = core reference".into(),
        prevailing_wind: PrevailingWind {
            azimuth_rad: m.wind_azimuth_rad,
            speed_mps: 5.0,
        },
        layers: vec![],
        provenance: Provenance {
            exemplars: vec![],
            envelope_fingerprint: "0".repeat(64),
            dictionary_fingerprint: "0".repeat(64),
        },
    };
    man.validate().expect("fixture C0 must validate");
    man
}
