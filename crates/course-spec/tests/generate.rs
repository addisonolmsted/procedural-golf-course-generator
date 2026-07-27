//! Step-02 integration suite: the determinism double-run (steps/02 hard
//! requirement 4), the golden spec artifact (which embeds the prior
//! fingerprint, so any prior edit forces an explicit re-bless HERE as well
//! as in the fingerprint golden), sampling-range and route invariants over
//! many seeds, override recording, and weighted-selection behavior.

use std::collections::BTreeMap;

use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, Priors, SpecError, SpecOverrides};

fn spec(seed: u64) -> CourseSpec {
    CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &SpecOverrides::default()).unwrap()
}

#[test]
fn determinism_double_run() {
    for seed in [0, 1, 42, u64::MAX] {
        assert_eq!(spec(seed).canonical_json(), spec(seed).canonical_json());
    }
}

#[test]
fn determinism_across_reroll_chain() {
    let build = || {
        let mut id = RunIdentity::from_seed(99);
        let mut out = vec![
            CourseSpec::generate_builtin(id, &SpecOverrides::default())
                .unwrap()
                .canonical_json(),
        ];
        while let Ok(next) = id.reroll() {
            id = next;
            out.push(
                CourseSpec::generate_builtin(id, &SpecOverrides::default())
                    .unwrap()
                    .canonical_json(),
            );
        }
        out
    };
    let a = build();
    let b = build();
    assert_eq!(a, b);
    // Attempts genuinely resample: not all identical to attempt 0.
    assert!(a.iter().skip(1).any(|j| *j != a[0]));
}

/// The pinned artifact for seed 1. Blessing rule: this changes ONLY on an
/// intentional contract event (prior edit, sampling change, schema change),
/// together with the prior fingerprint golden and, where required, a
/// `pipeline_version` bump.
#[test]
fn golden_spec_seed_1() {
    let got = spec(1).canonical_json();
    let want = include_str!("golden_spec_seed_1.json");
    assert_eq!(got, want, "golden spec drifted — re-bless deliberately");
}

#[test]
fn sampled_values_in_table_range_many_seeds() {
    let priors = Priors::builtin();
    let mut seen = BTreeMap::new();
    for seed in 0..512u64 {
        let s = spec(seed);
        *seen.entry(s.archetype.key()).or_insert(0u32) += 1;
        let entry = priors.entry(s.archetype);
        assert_eq!(s.params.len(), entry.params.len());
        for (key, table) in &entry.params {
            let v = s.param(key);
            assert!(
                (table.min()..=table.max()).contains(&v),
                "seed {seed} {key} = {v} outside [{}, {}]",
                table.min(),
                table.max()
            );
        }
        assert_eq!(s.hole_count, 9);
        assert_eq!(s.par_sequence.iter().sum::<u32>(), 36);
        assert_eq!(s.hydrology_mode, s.archetype.hydrology_mode());
    }
    // Equal placeholder weights: every archetype appears across 512 seeds.
    assert_eq!(seen.len(), ArchetypeId::ALL.len(), "{seen:?}");
}

#[test]
fn forced_archetype_recorded_and_params_unshifted() {
    let id = RunIdentity::from_seed(5);
    let natural = CourseSpec::generate_builtin(id, &SpecOverrides::default()).unwrap();
    for a in ArchetypeId::ALL {
        let forced = CourseSpec::generate_builtin(
            id,
            &SpecOverrides {
                forced_archetype: Some(a),
                params: BTreeMap::new(),
            },
        )
        .unwrap();
        assert_eq!(forced.archetype, a);
        assert_eq!(forced.hydrology_mode, a.hydrology_mode());
        assert_eq!(forced.overrides.forced_archetype, Some(a));
        if a == natural.archetype {
            // Forcing the natural pick is a byte-level no-op on params.
            assert_eq!(forced.params, natural.params);
        }
    }
}

#[test]
fn override_applied_and_recorded_verbatim() {
    let id = RunIdentity::from_seed(11);
    let base = CourseSpec::generate_builtin(id, &SpecOverrides::default()).unwrap();
    let key = "hydro.rainfall";
    let ov = SpecOverrides {
        forced_archetype: Some(base.archetype),
        params: BTreeMap::from([(key.to_string(), 123.456)]),
    };
    let got = CourseSpec::generate_builtin(id, &ov).unwrap();
    assert_eq!(got.param(key), 123.456);
    assert_eq!(got.overrides, ov);
    // Only the overridden knob differs — the draw transcript is
    // override-independent.
    for (k, v) in &base.params {
        if k != key {
            assert_eq!(got.param(k), *v, "{k}");
        }
    }
}

#[test]
fn override_errors_are_loud() {
    let id = RunIdentity::from_seed(3);
    let unknown = SpecOverrides {
        forced_archetype: None,
        params: BTreeMap::from([("nope.nope".to_string(), 1.0)]),
    };
    assert!(matches!(
        CourseSpec::generate_builtin(id, &unknown),
        Err(SpecError::UnknownOverrideKey { .. })
    ));
    let nan = SpecOverrides {
        forced_archetype: None,
        params: BTreeMap::from([("hydro.rainfall".to_string(), f64::NAN)]),
    };
    assert!(matches!(
        CourseSpec::generate_builtin(id, &nan),
        Err(SpecError::NonFiniteOverride { .. })
    ));
}

/// In-test priors (NOT the committed file) exercising the weight machinery.
fn weighted_priors(weights: [f64; 5]) -> Priors {
    let entries: Vec<String> = ArchetypeId::ALL
        .iter()
        .zip(weights)
        .map(|(a, w)| {
            format!(
                r#""{}": {{"weight": {w:?}, "params": {{"hydro.rainfall": {{"q": [0,1,2,3,4,5,6,7,8,9,10]}}}}}}"#,
                a.key()
            )
        })
        .collect();
    Priors::from_json(&format!(
        r#"{{"prior_version": "test-1", "archetypes": {{{}}}}}"#,
        entries.join(",")
    ))
    .unwrap()
}

fn draw_counts(priors: &Priors, seeds: u64) -> BTreeMap<&'static str, u32> {
    let mut counts = BTreeMap::new();
    for seed in 0..seeds {
        let s = CourseSpec::generate(
            RunIdentity::from_seed(seed),
            priors,
            "test-fingerprint",
            &SpecOverrides::default(),
        )
        .unwrap();
        *counts.entry(s.archetype.key()).or_insert(0) += 1;
    }
    counts
}

#[test]
fn zero_weight_never_selected_but_still_forceable() {
    let priors = weighted_priors([0.0, 1.0, 1.0, 1.0, 1.0]);
    let counts = draw_counts(&priors, 2000);
    assert_eq!(counts.get("sandhills"), None, "{counts:?}");
    let forced = CourseSpec::generate(
        RunIdentity::from_seed(1),
        &priors,
        "test-fingerprint",
        &SpecOverrides {
            forced_archetype: Some(ArchetypeId::Sandhills),
            params: BTreeMap::new(),
        },
    )
    .unwrap();
    assert_eq!(forced.archetype, ArchetypeId::Sandhills);
}

#[test]
fn heavy_weight_dominates() {
    let priors = weighted_priors([100.0, 1.0, 1.0, 1.0, 1.0]);
    let counts = draw_counts(&priors, 2000);
    assert!(counts["sandhills"] > 1800, "{counts:?}");
}

#[test]
fn equal_weights_spread_evenly() {
    let priors = weighted_priors([1.0; 5]);
    let counts = draw_counts(&priors, 5000);
    // Deterministic seeds ⇒ these bounds cannot flake. Expectation is 20%
    // each; loose 10–35% band.
    for a in ArchetypeId::ALL {
        let n = counts[a.key()];
        assert!((500..=1750).contains(&n), "{a}: {n} of 5000 ({counts:?})");
    }
}
