//! Store round-trip: every artifact type saves into a course dir, reloads
//! equal (modulo f32 sidecar quantization), and the manifest records stable
//! content hashes.

use course_contracts::fixtures::run_fixture_pipeline;
use course_contracts::{Artifact, ArchetypeId, Manifest};
use course_contracts::spec::CourseSpec;
use course_contracts::stages::*;

fn assert_close(a: &golf_core::Grid<f64>, b: &golf_core::Grid<f64>) {
    assert_eq!(a.spec, b.spec);
    for (x, y) in a.data.iter().zip(&b.data) {
        assert!((x - y).abs() < 1e-3, "f32 round-trip drift too large: {x} vs {y}");
    }
}

#[test]
fn every_artifact_round_trips_and_manifests() {
    let b = run_fixture_pipeline(42, ArchetypeId::FloridaLowland, 30.0);
    let dir = std::env::temp_dir().join(format!("course_store_test_{}", b.spec.slug()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    b.spec.save(&dir).unwrap();
    b.skeleton.save(&dir).unwrap();
    b.composed.save(&dir).unwrap();
    b.hydro.save(&dir).unwrap();
    b.cover.save(&dir).unwrap();
    b.gate.save(&dir).unwrap();
    b.routing.save(&dir).unwrap();
    b.earthworks.save(&dir).unwrap();
    b.drainage.save(&dir).unwrap();

    let mut m = Manifest::default();
    m.record::<CourseSpec>(&dir).unwrap();
    m.record::<MacroSkeleton>(&dir).unwrap();
    m.record::<ComposedHeight>(&dir).unwrap();
    m.record::<HydroBundle>(&dir).unwrap();
    m.record::<CoverMap>(&dir).unwrap();
    m.record::<GateReport>(&dir).unwrap();
    m.record::<RoutingPlan>(&dir).unwrap();
    m.record::<EarthworksDelta>(&dir).unwrap();
    m.record::<DrainageReport>(&dir).unwrap();
    m.write(&dir).unwrap();

    // Exact round-trips for JSON-only artifacts.
    assert_eq!(CourseSpec::load(&dir).unwrap(), b.spec);
    assert_eq!(GateReport::load(&dir).unwrap(), b.gate);
    assert_eq!(RoutingPlan::load(&dir).unwrap(), b.routing);

    // Grid artifacts: f32-quantized but structurally identical.
    let skel = MacroSkeleton::load(&dir).unwrap();
    assert_eq!(skel.structure, b.skeleton.structure);
    assert_close(&skel.base_height, &b.skeleton.base_height);
    assert_close(&skel.fields.floor_damp, &b.skeleton.fields.floor_damp);

    let hydro = HydroBundle::load(&dir).unwrap();
    assert_eq!(hydro.flow.rcv, b.hydro.flow.rcv);
    assert_eq!(hydro.flow.order, b.hydro.flow.order);
    assert_eq!(hydro.graph, b.hydro.graph);
    assert_close(&hydro.flow.flow_area_m2, &b.hydro.flow.flow_area_m2);

    let cover = CoverMap::load(&dir).unwrap();
    assert_eq!(cover.class.data, b.cover.class.data);

    let ew = EarthworksDelta::load(&dir).unwrap();
    assert_eq!(ew.features, b.earthworks.features);
    assert_eq!(ew.patches.len(), b.earthworks.patches.len());

    let dr = DrainageReport::load(&dir).unwrap();
    assert_eq!(dr.pass, b.drainage.pass);

    // Manifest: reload equals, hashes stable across a re-save of same content.
    let m2 = Manifest::load(&dir).unwrap();
    assert_eq!(m2, m);
    b.hydro.save(&dir).unwrap();
    assert_eq!(HydroBundle::content_hash(&dir).unwrap(), m.entries["hydro_bundle"].blake3);

    let _ = std::fs::remove_dir_all(&dir);
}
