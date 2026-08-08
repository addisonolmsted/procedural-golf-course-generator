//! Contract invariant + artifact round-trip suite (workplan item B1).
//!
//! The round-trip definition: CGRID1 stores f32, so the FIRST write quantizes;
//! `read(write(x))` then `write` again must produce byte-identical files —
//! write→read→write is idempotent after the initial quantization.

use course_contracts::contracts::corridor_graph::CorridorGraph;
use course_contracts::contracts::primitive_field::PrimitiveField;
use course_contracts::contracts::routing_substrate::{
    check_play_window, Rect, RoutingSubstrate, PLAY_M,
};
use course_contracts::fixtures;
use course_contracts::ContractError;
use course_world::math::Vec2;
use std::fs;
use std::path::PathBuf;

fn tmp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("course-contracts-test-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn dir_bytes(dir: &PathBuf) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = fs::read_dir(dir)
        .unwrap()
        .map(|e| {
            let p = e.unwrap().path();
            (
                p.file_name().unwrap().to_string_lossy().into_owned(),
                fs::read(&p).unwrap(),
            )
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ------------------------------------------------------------------ fixtures

#[test]
fn fixtures_construct_through_real_validators() {
    // Each builder goes through the validating constructor, so this test is
    // the proof that a minimal valid instance of every contract exists.
    let _ = fixtures::c1(1);
    let _ = fixtures::c2(1);
    let _ = fixtures::c3(1);
    let _ = fixtures::c0_manifest(1);
}

// ----------------------------------------------------------------------- C1

#[test]
fn c1_rejects_nan_and_range_violations() {
    let good = fixtures::c1(1);
    // NaN in a field.
    let mut tilt = good.tilt.clone();
    tilt.data[0] = f64::NAN;
    assert!(matches!(
        PrimitiveField::new(
            1,
            [7u8; 32],
            good.grid,
            tilt,
            good.relief.clone(),
            good.hardness.clone(),
            good.accommodation.clone(),
            good.meta.clone(),
        ),
        Err(ContractError::Invariant { .. })
    ));
    // hardness out of [0, 1].
    let mut hardness = good.hardness.clone();
    hardness.data[3] = 1.5;
    assert!(PrimitiveField::new(
        1,
        [7u8; 32],
        good.grid,
        good.tilt.clone(),
        good.relief.clone(),
        hardness,
        good.accommodation.clone(),
        good.meta.clone(),
    )
    .is_err());
    // relief not mean-zero over the core.
    let mut relief = good.relief.clone();
    for v in relief.data.iter_mut() {
        *v += 1.0;
    }
    assert!(PrimitiveField::new(
        1,
        [7u8; 32],
        good.grid,
        good.tilt.clone(),
        relief,
        good.hardness.clone(),
        good.accommodation.clone(),
        good.meta,
    )
    .is_err());
}

#[test]
fn c1_artifact_roundtrip_is_byte_identical() {
    let c1 = fixtures::c1(1);
    let d1 = tmp("c1-a");
    c1.write_dir(&d1).unwrap();
    let loaded = PrimitiveField::read_dir(&d1).unwrap();
    let d2 = tmp("c1-b");
    loaded.write_dir(&d2).unwrap();
    assert_eq!(dir_bytes(&d1), dir_bytes(&d2));
}

#[test]
fn c1_load_is_loud_on_tampering() {
    let c1 = fixtures::c1(1);
    let d = tmp("c1-tamper");
    c1.write_dir(&d).unwrap();
    // Flip a byte in a grid payload: the hash check must catch it.
    let p = d.join("relief.cgrid");
    let mut bytes = fs::read(&p).unwrap();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(&p, bytes).unwrap();
    assert!(matches!(
        PrimitiveField::read_dir(&d),
        Err(ContractError::HashMismatch { .. })
    ));
}

// ----------------------------------------------------------------------- C2

#[test]
fn c2_play_window_arithmetic() {
    // The centred window and the four extreme corners are all legal…
    let mid = 1500.0;
    for (cx, cy) in [
        (mid, mid),
        (mid - 450.0, mid - 450.0),
        (mid + 450.0, mid - 450.0),
        (mid - 450.0, mid + 450.0),
        (mid + 450.0, mid + 450.0),
    ] {
        let w = Rect {
            min: Vec2::new(cx - PLAY_M / 2.0, cy - PLAY_M / 2.0),
            max: Vec2::new(cx + PLAY_M / 2.0, cy + PLAY_M / 2.0),
        };
        check_play_window(&w).unwrap();
        // …and every legal window keeps >= 750 m of terrain beyond any edge.
        let margin = [
            w.min.x,
            w.min.y,
            3000.0 - w.max.x,
            3000.0 - w.max.y,
        ]
        .into_iter()
        .fold(f64::INFINITY, f64::min);
        assert!(margin >= 750.0 - 1e-9, "margin {margin}");
    }
    // One metre past the range is rejected.
    let w = Rect {
        min: Vec2::new(mid + 451.0 - PLAY_M / 2.0, mid - PLAY_M / 2.0),
        max: Vec2::new(mid + 451.0 + PLAY_M / 2.0, mid + PLAY_M / 2.0),
    };
    assert!(check_play_window(&w).is_err());
    // Wrong size is rejected.
    let w = Rect {
        min: Vec2::new(mid - 200.0, mid - 200.0),
        max: Vec2::new(mid + 200.0, mid + 200.0),
    };
    assert!(check_play_window(&w).is_err());
}

#[test]
fn c2_rejects_infinite_cost_and_bad_flow() {
    let good = fixtures::c2(1);
    let rebuild = |cost_earthwork: Option<f64>, flow_dir: Option<f64>| {
        let mut cost = good.cost.clone();
        if let Some(v) = cost_earthwork {
            cost.earthwork.data[10] = v;
        }
        let mut meta = good.meta.clone();
        if let Some(v) = flow_dir {
            meta.flow_dir_rad.data[10] = v;
        }
        RoutingSubstrate::new(
            1,
            [7u8; 32],
            good.grid_full,
            good.grid_cost,
            good.height.clone(),
            good.play_window,
            good.masks.clone(),
            cost,
            meta,
            good.preset.clone(),
        )
    };
    assert!(rebuild(Some(f64::INFINITY), None).is_err()); // infinity = smuggled veto
    assert!(rebuild(Some(-1.0), None).is_err());
    assert!(rebuild(None, Some(7.0)).is_err()); // >= 2π
    assert!(rebuild(None, None).is_ok());
}

#[test]
fn c2_empty_case_is_legal() {
    // The fixture IS the empty case: water, clearing, borrow identically zero.
    let c2 = fixtures::c2(1);
    assert!(c2.masks.water.data.iter().all(|v| *v == 0.0));
    assert!(c2.cost.clearing.data.iter().all(|v| *v == 0.0));
    assert!(c2.cost.borrow.data.iter().all(|v| *v == 0.0));
}

#[test]
fn c2_metadata_bit_identity() {
    let c2 = fixtures::c2(1);
    let m = fixtures::meta();
    assert!(c2.metadata_matches(&m));
    let mut drifted = m.clone();
    drifted.grain_axis_rad += 1e-15; // even one ulp of drift is a violation
    assert!(!c2.metadata_matches(&drifted));
}

#[test]
fn c2_artifact_roundtrip_is_byte_identical_and_self_sufficient() {
    let c2 = fixtures::c2(1);
    let d1 = tmp("c2-a");
    c2.write_dir(&d1).unwrap();
    // Self-sufficiency: the directory alone is a complete input.
    let loaded = RoutingSubstrate::read_dir(&d1).unwrap();
    let d2 = tmp("c2-b");
    loaded.write_dir(&d2).unwrap();
    assert_eq!(dir_bytes(&d1), dir_bytes(&d2));
}

// ----------------------------------------------------------------------- C3

#[test]
fn c3_rejects_broken_plans() {
    let good = fixtures::c3(1);
    // Wrong par.
    let mut holes = good.holes.clone();
    holes[2].par = 4; // PAR_SEQUENCE[2] is 3
    assert!(CorridorGraph::new(1, [7u8; 32], holes, good.nodes.clone(), good.budgets, good.plasticity).is_err());
    // Halfwidth length mismatch.
    let mut holes = good.holes.clone();
    holes[0].halfwidth_m.pop();
    assert!(CorridorGraph::new(1, [7u8; 32], holes, good.nodes.clone(), good.budgets, good.plasticity).is_err());
    // Spine escapes the core.
    let mut holes = good.holes.clone();
    holes[0].spine[1] = Vec2::new(100.0, 100.0);
    assert!(CorridorGraph::new(1, [7u8; 32], holes, good.nodes.clone(), good.budgets, good.plasticity).is_err());
    // Borrow does not cover fill.
    let mut budgets = good.budgets;
    budgets.total_fill_m3 = 1e9;
    assert!(CorridorGraph::new(1, [7u8; 32], good.holes.clone(), good.nodes.clone(), budgets, good.plasticity).is_err());
    // Loop does not return.
    let mut budgets = good.budgets;
    budgets.clubhouse_radius_m = 1.0;
    assert!(CorridorGraph::new(1, [7u8; 32], good.holes.clone(), good.nodes.clone(), budgets, good.plasticity).is_err());
}

#[test]
fn c3_roundtrips_via_json() {
    let c3 = fixtures::c3(1);
    let d = tmp("c3");
    let p = d.join("corridors.json");
    c3.write_file(&p).unwrap();
    let loaded = CorridorGraph::read_file(&p).unwrap();
    assert_eq!(c3, loaded);
    let p2 = d.join("corridors2.json");
    loaded.write_file(&p2).unwrap();
    assert_eq!(fs::read(&p).unwrap(), fs::read(&p2).unwrap());
}

// ----------------------------------------------------------------------- C0

#[test]
fn c0_manifest_validates_wind_and_window() {
    let mut man = fixtures::c0_manifest(1);
    man.prevailing_wind.azimuth_rad = -0.5;
    assert!(man.validate().is_err());
    let mut man = fixtures::c0_manifest(1);
    man.prevailing_wind.speed_mps = f64::NAN;
    assert!(man.validate().is_err());
    let mut man = fixtures::c0_manifest(1);
    man.play_window.max.x += 100.0;
    assert!(man.validate().is_err());
}
