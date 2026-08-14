//! S4 acceptance subset: determinism, sandhills identity pass, trunk
//! incision through the floodplain, agreement reported, order matters,
//! basins survive, goldens.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology::{self, TransformId};

#[allow(clippy::type_complexity)]
fn chain(
    seed: u64,
    biome: BiomeId,
) -> (
    SiteSpec,
    course_skeleton::kernel::Skeleton,
    course_amplify::Amplified,
    RunIdentity,
) {
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .expect("dictionary");
    let id = RunIdentity::from_seed(seed);
    let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let amp = course_amplify::generate(&spec, &sk, &dict, &id);
    (spec, sk, amp, id)
}

fn run(seed: u64, biome: BiomeId) -> hydrology::Hydrology {
    let (spec, sk, amp, id) = chain(seed, biome);
    hydrology::generate(&spec, &sk, &amp, &id, &hydrology::DEFAULT_TRANSFORMS)
}

#[test]
fn determinism_double_run() {
    let a = run(7, BiomeId::Heathland);
    let b = run(7, BiomeId::Heathland);
    assert_eq!(a.height.data, b.height.data);
    assert_eq!(a.water.len(), b.water.len());
    assert_eq!(a.skeleton_agreement, b.skeleton_agreement);
}

#[test]
fn sandhills_identity_pass_no_special_case() {
    let h = run(21, BiomeId::Sandhills);
    assert_eq!(h.applied, hydrology::DEFAULT_TRANSFORMS.to_vec());
    assert!(h.flow_accum.data.iter().all(|v| v.is_finite()));
    let wt = h
        .water
        .iter()
        .filter(|w| matches!(w.origin, hydrology::WaterPlaneOrigin::WaterTable))
        .count();
    assert_eq!(wt, 0, "sandhills grew {wt} water-table bodies");
}

#[test]
fn agreement_reported_and_in_expected_band() {
    // Current honest level ~0.45 on integrated biomes: the major network
    // agrees (the accumulation image traces the skeleton) but UNALIGNED
    // mid-band texture buries low-order swales — the same root cause as
    // the reviewer's river-valley grain observation, queued as the
    // patch-orientation work in G-TERRAIN calibration. This test is a
    // REGRESSION floor, not the ≥0.9 gate target; raise it when
    // orientation-aligned pasting lands.
    let h = run(11, BiomeId::Piedmont);
    assert!(
        h.skeleton_agreement > 0.35,
        "agreement {} regressed below the recorded baseline",
        h.skeleton_agreement
    );
    assert!(h.skeleton_agreement <= 1.0);
}

#[test]
fn transform_order_changes_output() {
    let (spec, sk, amp, id) = chain(13, BiomeId::RiverValley);
    let a = hydrology::generate(
        &spec,
        &sk,
        &amp,
        &id,
        &[TransformId::Floodplain, TransformId::WaterTable],
    );
    let b = hydrology::generate(
        &spec,
        &sk,
        &amp,
        &id,
        &[TransformId::WaterTable, TransformId::Floodplain],
    );
    assert_eq!(a.applied, vec![TransformId::Floodplain, TransformId::WaterTable]);
    assert_eq!(b.applied, vec![TransformId::WaterTable, TransformId::Floodplain]);
    // Order shows up in the WATER (pond levels are computed from the
    // height state at the moment the water-table transform runs, and the
    // floodplain modifies that state); heights are only touched by the
    // floodplain, which runs identically in both orders.
    let levels = |h: &hydrology::Hydrology| {
        h.water.iter().map(|w| w.surface_m.to_bits()).collect::<Vec<_>>()
    };
    assert_ne!(levels(&a), levels(&b), "order must matter (pond levels)");
}

#[test]
fn trunk_stays_incised_through_floodplain() {
    let (spec, sk, amp, id) = chain(15, BiomeId::RiverValley);
    let h = hydrology::generate(&spec, &sk, &amp, &id, &hydrology::DEFAULT_TRANSFORMS);
    let n2 = h.height.spec.nx as usize;
    let big = sk
        .channels
        .iter()
        .max_by(|a, b| a.area_m2.total_cmp(&b.area_m2))
        .unwrap();
    let mut below = 0;
    let mut total = 0;
    for p in big.pts.iter().step_by(4) {
        let x = ((p.x / 2.0) as usize).min(n2 - 1);
        let y = ((p.y / 2.0) as usize).min(n2 - 1);
        let bed = h.height.data[y * n2 + x];
        let xs = (x + 30).min(n2 - 1);
        let side = h.height.data[y * n2 + xs];
        total += 1;
        if side >= bed {
            below += 1;
        }
    }
    assert!(
        below as f64 / total.max(1) as f64 > 0.8,
        "trunk not incised: only {below}/{total} centreline points below their floodplain"
    );
}

#[test]
fn basins_survive_and_closed_are_flagged() {
    let h = run(31, BiomeId::Heathland);
    assert!(!h.basins.is_empty(), "heathland must inventory basins");
    assert!(
        h.basins.iter().any(|b| b.closed),
        "heathland kettles must be flagged closed"
    );
    for b in &h.basins {
        assert!(b.area_m2 > 0.0);
        assert!(b.polygon.len() >= 3);
    }
}

#[test]
fn golden_hydrology_seed_1() {
    fn fnv(data: &[f64]) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for v in data {
            for b in v.to_bits().to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
        }
        h
    }
    let mut got = String::new();
    for biome in [BiomeId::Piedmont, BiomeId::Heathland] {
        let h = run(1, biome);
        got += &format!(
            "{biome:?} {:016x} water={} basins={} agree={:.3}\n",
            fnv(&h.height.data),
            h.water.len(),
            h.basins.len(),
            h.skeleton_agreement
        );
    }
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write("tests/golden_hydrology_seed_1.txt", &got).unwrap();
        return;
    }
    let want = include_str!("golden_hydrology_seed_1.txt");
    assert_eq!(got, want, "S4 golden diverged — re-bless if intended");
}
