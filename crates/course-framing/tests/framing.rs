//! Stage-01 hard requirements: determinism, reroll stability (the INVERTED
//! test — framing must be byte-identical across every attempt), internal
//! consistency, categorical reachability, canonical bytes, golden.

use std::collections::BTreeMap;

use course_framing::{
    generate, Framing, WindowClass, MAX_BRANCHES, MAX_RIDGES, MAX_STEPS, STEP_AXIS_TOL_RAD,
    TILT_TOL_RAD,
};
use course_seed::{RunIdentity, MAX_ATTEMPTS};
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_world::math::{acos, atan2, cos, sin, Vec2};
use course_world::world::EXTENT_M;

fn spec_for(seed: u64, forced: Option<ArchetypeId>) -> CourseSpec {
    let overrides = SpecOverrides {
        forced_archetype: forced,
        ..SpecOverrides::default()
    };
    CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &overrides).unwrap()
}

#[test]
fn determinism_double_run() {
    for seed in [0, 1, 42, u64::MAX] {
        let a = generate(&spec_for(seed, None)).canonical_json();
        let b = generate(&spec_for(seed, None)).canonical_json();
        assert_eq!(a, b, "seed {seed} drifted between runs");
    }
}

/// Hard requirement 2: framing survives the whole reroll chain
/// byte-identically (stable stream scope + attempt-free artifact header).
#[test]
fn stable_across_reroll_chain() {
    let overrides = SpecOverrides::default();
    let mut id = RunIdentity::from_seed(1234);
    let baseline =
        generate(&CourseSpec::generate_builtin(id, &overrides).unwrap()).canonical_json();
    let mut attempts = 1;
    while let Ok(next) = id.reroll() {
        id = next;
        attempts += 1;
        let f = generate(&CourseSpec::generate_builtin(id, &overrides).unwrap()).canonical_json();
        assert_eq!(f, baseline, "framing drifted at attempt {}", id.attempt);
    }
    assert_eq!(attempts, MAX_ATTEMPTS);
}

/// Hard requirement 1 (internal consistency) over a broad sweep, plus the
/// artifact's own validator.
#[test]
fn consistency_invariants() {
    let cases = std::iter::once(None).chain(ArchetypeId::ALL.into_iter().map(Some));
    for forced in cases {
        for seed in 0..256u64 {
            let spec = spec_for(seed, forced);
            let f = generate(&spec);
            f.validate().unwrap_or_else(|e| {
                panic!("seed {seed} forced {forced:?} invalid: {e}");
            });

            // Tilt within tolerance of the base edge.
            let tv = Vec2::new(cos(f.regional_tilt.dir_rad), sin(f.regional_tilt.dir_rad));
            let angle = acos(tv.dot(f.base_level.edge.outward_dir()).clamp(-1.0, 1.0));
            assert!(angle <= TILT_TOL_RAD + 1e-12);

            // Values inside their θ ranges.
            assert_eq!(f.base_level.elev_m, -spec.param("framing.base_drop_m"));
            assert_eq!(f.regional_tilt.grade, spec.param("framing.tilt_grade"));
            assert!((0.0..=1.0).contains(&f.grain.anisotropy));

            // Boundary curve genuinely crosses the box.
            if let Some(b) = &f.provinces.boundary {
                assert_eq!(f.provinces.count, 2);
                assert!(b.curve.len() >= 4);
                let inside = |p: &[f64; 2]| {
                    (0.0..=EXTENT_M).contains(&p[0]) && (0.0..=EXTENT_M).contains(&p[1])
                };
                assert!(!inside(&b.curve[0]), "curve starts inside the box");
                assert!(!inside(b.curve.last().unwrap()), "curve ends inside the box");
                assert!(inside(&b.curve[b.curve.len() / 2]), "curve midpoint outside");
            } else {
                assert_eq!(f.provinces.count, 1);
            }
        }
    }
}

/// Hard requirement 4: over a seed sweep, every categorical outcome with
/// nonzero weight appears, and zero-weight outcomes never do.
#[test]
fn categorical_reachability() {
    const SEEDS: u64 = 512;
    for arch in ArchetypeId::ALL {
        let mut window_seen = std::collections::BTreeMap::new();
        let mut kinds_seen = std::collections::BTreeSet::new();
        let mut counts_seen = std::collections::BTreeSet::new();
        let mut weights = None;
        let mut province_p = 0.0;
        for seed in 0..SEEDS {
            let spec = spec_for(seed, Some(arch));
            let f = generate(&spec);
            *window_seen.entry(format!("{:?}", f.window)).or_insert(0u32) += 1;
            counts_seen.insert(f.provinces.count);
            if let Some(b) = &f.provinces.boundary {
                kinds_seen.insert(format!("{:?}", b.kind));
            }
            if weights.is_none() {
                weights = Some(
                    course_framing::WindowClass::ALL
                        .map(|c| (format!("{c:?}"), spec.param(c.weight_knob()))),
                );
                province_p = spec.param("framing.province_p");
            }
        }
        for (class, w) in weights.unwrap() {
            let seen = window_seen.contains_key(&class);
            assert_eq!(
                seen,
                w > 0.0,
                "{arch:?}: window {class} seen={seen} but weight={w}"
            );
        }
        if province_p > 0.0 && province_p < 1.0 {
            assert!(counts_seen.contains(&1) && counts_seen.contains(&2), "{arch:?}");
        }
        // Every nonzero-weight kind appears among boundary-bearing seeds.
        let spec = spec_for(0, Some(arch));
        for kind in course_framing::BoundaryKind::ALL {
            if spec.param(kind.weight_knob()) > 0.0 && province_p >= 0.05 {
                assert!(
                    kinds_seen.contains(&format!("{kind:?}")),
                    "{arch:?}: kind {kind:?} never drawn over {SEEDS} seeds"
                );
            }
        }
    }
}

#[test]
fn canonical_roundtrip() {
    for seed in [1u64, 7, 99] {
        let f = generate(&spec_for(seed, None));
        let bytes = f.canonical_json();
        let back = Framing::from_json(&bytes).unwrap();
        assert_eq!(back, f);
        assert_eq!(back.canonical_json(), bytes);
    }
}

#[test]
fn loud_load_rejections() {
    // Find a seed with a boundary so curve tampering is exercised.
    let f = (0..999u64)
        .map(|s| generate(&spec_for(s, Some(ArchetypeId::MountainBench))))
        .find(|f| f.provinces.boundary.is_some())
        .expect("mountain_bench has province_p 0.75; a boundary seed must exist");
    let json = f.canonical_json();

    // Wrong pipeline version.
    let tampered = json.replacen(
        &format!(r#""pipeline_version":{}"#, f.pipeline_version),
        r#""pipeline_version":999"#,
        1,
    );
    assert!(Framing::from_json(&tampered).is_err());

    // Wrong framing version.
    let needle = format!(r#""framing_version":{}"#, f.framing_version);
    assert!(json.contains(&needle), "test needle went stale");
    let tampered = json.replacen(&needle, r#""framing_version":999"#, 1);
    assert!(Framing::from_json(&tampered).is_err());

    // Unknown field.
    let tampered = json.replacen(r#"{"seed":"#, r#"{"rogue":1,"seed":"#, 1);
    assert!(Framing::from_json(&tampered).is_err());

    // Tilt rotated out of tolerance (point it opposite the edge).
    let mut broken = f.clone();
    broken.regional_tilt.dir_rad =
        (broken.regional_tilt.dir_rad + std::f64::consts::PI) % (2.0 * std::f64::consts::PI);
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // count/boundary mismatch.
    let mut broken = f.clone();
    broken.provinces.count = 1;
    assert!(Framing::from_json(&broken.canonical_json()).is_err());
    let mut broken = f.clone();
    broken.provinces.count = 3;
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // Curve truncated to end inside the box.
    let mut broken = f.clone();
    let b = broken.provinces.boundary.as_mut().unwrap();
    let n = b.curve.len();
    b.curve.truncate(n / 2);
    assert!(Framing::from_json(&broken.canonical_json()).is_err());
}

#[test]
fn loud_load_rejections_skeleton() {
    // A seed with the full skeleton: trunk + branches.
    let f = (0..999u64)
        .map(|s| generate(&spec_for(s, Some(ArchetypeId::Piedmont))))
        .find(|f| f.skeleton.trunk.is_some() && !f.skeleton.branches.is_empty())
        .expect("piedmont must produce a trunk with branches");

    // Trunk reversing on itself (non-monotone).
    let mut broken = f.clone();
    {
        let t = broken.skeleton.trunk.as_mut().unwrap();
        let n = t.spine.len();
        t.spine.swap(n / 2, n / 2 + 1);
    }
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // Trunk stopping mid-box (never reaches the outlet).
    let mut broken = f.clone();
    {
        let t = broken.skeleton.trunk.as_mut().unwrap();
        let n = t.spine.len();
        t.spine.truncate(n / 2);
    }
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // Non-positive corridor halfwidth.
    let mut broken = f.clone();
    broken.skeleton.trunk.as_mut().unwrap().halfwidth_m = 0.0;
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // Branch mouth floating off the trunk.
    let mut broken = f.clone();
    {
        let b = broken.skeleton.branches[0].last_mut().unwrap();
        b[0] += 25.0;
        b[1] += 25.0;
    }
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // Branches with no trunk at all.
    let mut broken = f.clone();
    broken.skeleton.trunk = None;
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // Ridge wandering out of the box.
    let mut broken = f.clone();
    broken.skeleton.ridges = vec![vec![[100.0, 100.0], [EXTENT_M + 500.0, 100.0]]];
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // Over the family cap.
    let mut broken = f.clone();
    broken.skeleton.branches = (0..MAX_BRANCHES + 1)
        .map(|_| broken2_branch(&f))
        .collect();
    assert!(Framing::from_json(&broken.canonical_json()).is_err());

    // A step that runs down the fall line instead of across it.
    let mut broken = f.clone();
    let d = Vec2::new(cos(f.regional_tilt.dir_rad), sin(f.regional_tilt.dir_rad));
    let c = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
    let a = c - d * (EXTENT_M * 0.8);
    let b2 = c + d * (EXTENT_M * 0.8);
    broken.skeleton.steps = vec![vec![
        [a.x, a.y],
        [c.x - d.x * 100.0, c.y - d.y * 100.0],
        [c.x, c.y],
        [b2.x, b2.y],
    ]];
    assert!(Framing::from_json(&broken.canonical_json()).is_err());
}

fn broken2_branch(f: &Framing) -> Vec<[f64; 2]> {
    f.skeleton.branches[0].clone()
}

/// The v1→v2 compatibility pin. The stage-01 BASE fields must not move when
/// the artifact gains the skeleton: `framing.topo_*` knobs are named to sort
/// after the last SAMPLED base knob (`framing.tilt_grade`), so the positional
/// θ draws feeding window/base_level/tilt/grain/provinces are unshifted. A
/// future knob named before `tilt_grade` fails HERE, loudly, instead of
/// silently reshuffling every seed's landscape.
#[test]
fn base_fields_survive_v2() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("base_fields_v1_fixture.json")).unwrap();
    let live: serde_json::Value =
        serde_json::from_str(&generate(&spec_for(1, None)).canonical_json()).unwrap();
    for field in [
        "window",
        "base_level",
        "regional_tilt",
        "grain",
        "provinces",
    ] {
        assert_eq!(
            fixture[field], live[field],
            "stage-01 base field `{field}` drifted at v2 — check that every \
             new knob sorts after `framing.tilt_grade`"
        );
    }
}

/// Skeleton structure over a broad sweep: the artifact validator plus the
/// θ-dependent rules it deliberately does not carry.
#[test]
fn skeleton_invariants() {
    let cases = std::iter::once(None).chain(ArchetypeId::ALL.into_iter().map(Some));
    for forced in cases {
        for seed in 0..256u64 {
            let spec = spec_for(seed, forced);
            let f = generate(&spec);
            f.validate()
                .unwrap_or_else(|e| panic!("seed {seed} forced {forced:?}: {e}"));
            let s = &f.skeleton;

            if let Some(t) = &s.trunk {
                assert_eq!(t.halfwidth_m, spec.param("framing.topo_trunk_halfwidth_m"));
                // Achieved sinuosity tracks the target from below (the
                // bisection is capped, and the curvature guard only shrinks).
                let pts: Vec<Vec2> = t.spine.iter().map(|p| Vec2::new(p[0], p[1])).collect();
                let arc: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
                let chord = (pts[pts.len() - 1] - pts[0]).length();
                let sin_achieved = arc / chord;
                let target = spec.param("framing.topo_trunk_sinuosity");
                assert!(
                    (1.0 - 1e-9..=target + 0.15).contains(&sin_achieved),
                    "seed {seed}: sinuosity {sin_achieved} vs target {target}"
                );
            } else {
                assert!(s.branches.is_empty(), "branches without a trunk");
            }
            assert!(s.branches.len() <= MAX_BRANCHES);
            assert!(s.ridges.len() <= MAX_RIDGES);
            assert!(s.steps.len() <= MAX_STEPS);

            // Steps only exist for terrace-reading windows, and never cut
            // the reserved middle of the site.
            if !s.steps.is_empty() {
                assert!(
                    matches!(
                        f.window,
                        WindowClass::TerraceFlight | WindowClass::EscarpmentFace
                    ),
                    "seed {seed}: steps on window {:?}",
                    f.window
                );
                let center = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
                let downhill = Vec2::new(cos(f.regional_tilt.dir_rad), sin(f.regional_tilt.dir_rad));
                for st in &s.steps {
                    let mid = Vec2::new(st[st.len() / 2][0], st[st.len() / 2][1]);
                    let a = (mid - center).dot(downhill) / EXTENT_M + 0.5;
                    assert!(
                        !(0.42..=0.58).contains(&a),
                        "seed {seed}: step riser at {a} cuts the reserved band"
                    );
                    // Contour-parallel within the contract tolerance.
                    let chord = Vec2::new(st[st.len() - 1][0], st[st.len() - 1][1])
                        - Vec2::new(st[0][0], st[0][1]);
                    let ang = atan2(chord.y, chord.x);
                    let contour = f.regional_tilt.dir_rad + std::f64::consts::FRAC_PI_2;
                    let pi = std::f64::consts::PI;
                    let d = (ang - contour).rem_euclid(pi);
                    assert!(d.min(pi - d) <= STEP_AXIS_TOL_RAD + 1e-12);
                }
            }
        }
    }
}

/// The empty case is legal, and stable: florida draws no skeleton at all,
/// every seed, every attempt.
#[test]
fn empty_case_florida() {
    let overrides = SpecOverrides {
        forced_archetype: Some(ArchetypeId::FloridaLowland),
        ..SpecOverrides::default()
    };
    for seed in 0..64u64 {
        let mut id = RunIdentity::from_seed(seed);
        let baseline =
            generate(&CourseSpec::generate_builtin(id, &overrides).unwrap()).canonical_json();
        let f = generate(&CourseSpec::generate_builtin(id, &overrides).unwrap());
        assert!(f.skeleton.trunk.is_none(), "seed {seed} grew a trunk");
        assert!(f.skeleton.branches.is_empty());
        assert!(f.skeleton.ridges.is_empty());
        assert!(f.skeleton.steps.is_empty());
        f.validate().unwrap();
        while let Ok(next) = id.reroll() {
            id = next;
            let again =
                generate(&CourseSpec::generate_builtin(id, &overrides).unwrap()).canonical_json();
            assert_eq!(again, baseline, "empty skeleton drifted on reroll");
        }
    }
}

/// Per-archetype skeleton tallies over a seed sweep.
struct Tally {
    trunk_yes: u32,
    trunk_no: u32,
    min_ridges: usize,
    max_ridges: usize,
    branch_seeds: u32,
    step_counts: std::collections::BTreeSet<usize>,
}

fn tally(arch: ArchetypeId, seeds: u64) -> Tally {
    let mut t = Tally {
        trunk_yes: 0,
        trunk_no: 0,
        min_ridges: usize::MAX,
        max_ridges: 0,
        branch_seeds: 0,
        step_counts: std::collections::BTreeSet::new(),
    };
    for seed in 0..seeds {
        let f = generate(&spec_for(seed, Some(arch)));
        if f.skeleton.trunk.is_some() {
            t.trunk_yes += 1;
        } else {
            t.trunk_no += 1;
        }
        let r = f.skeleton.ridges.len();
        t.min_ridges = t.min_ridges.min(r);
        t.max_ridges = t.max_ridges.max(r);
        if !f.skeleton.branches.is_empty() {
            t.branch_seeds += 1;
        }
        t.step_counts.insert(f.skeleton.steps.len());
    }
    t
}

/// Every archetype's intended skeleton character is reachable.
#[test]
fn skeleton_reachability() {
    const SEEDS: u64 = 512;

    // Piedmont: the meandering-creek archetype — trunk usually, not always.
    let p = tally(ArchetypeId::Piedmont, SEEDS);
    assert!(
        p.trunk_yes > 0 && p.trunk_no > 0,
        "piedmont trunk {}/{}",
        p.trunk_yes,
        p.trunk_no
    );
    assert!(p.branch_seeds > 0, "piedmont never grew a tributary");
    assert_eq!(p.step_counts, std::iter::once(0).collect());

    // Sandhills: dune-train axes always, surface water never.
    let s = tally(ArchetypeId::Sandhills, SEEDS);
    assert_eq!(s.trunk_yes, 0, "sandhills has no surface water");
    assert!(s.min_ridges >= 4, "sandhills thinned to {} axes", s.min_ridges);

    // Florida: the empty case, every seed.
    let fl = tally(ArchetypeId::FloridaLowland, SEEDS);
    assert_eq!(fl.trunk_yes, 0);
    assert_eq!(fl.max_ridges, 0);

    // Moraine: occasional short trunk.
    let g = tally(ArchetypeId::GlacialMoraine, SEEDS);
    assert!(
        g.trunk_yes > 0 && g.trunk_no > 0,
        "moraine trunk {}/{}",
        g.trunk_yes,
        g.trunk_no
    );

    // Mountain: flights appear on terrace windows, absent on others.
    let m = tally(ArchetypeId::MountainBench, SEEDS);
    assert!(m.step_counts.contains(&0), "mountain always steps");
    assert!(
        m.step_counts.iter().any(|&n| n >= 2),
        "mountain never got a flight"
    );
    assert!(m.trunk_yes > 0);
}

/// Sub-seed isolation: overriding only ridge knobs must not move the trunk.
#[test]
fn family_isolation() {
    let base = spec_for(3, Some(ArchetypeId::Piedmont));
    let trunk_before = generate(&base).skeleton.trunk;
    assert!(trunk_before.is_some(), "test needs a seed with a trunk");

    let mut params = BTreeMap::new();
    params.insert("framing.topo_ridge_count".to_string(), 2.0);
    params.insert("framing.topo_ridge_spacing_m".to_string(), 900.0);
    let overrides = SpecOverrides {
        forced_archetype: Some(ArchetypeId::Piedmont),
        params,
    };
    let tweaked =
        CourseSpec::generate_builtin(RunIdentity::from_seed(3), &overrides).unwrap();
    let after = generate(&tweaked);
    assert_eq!(after.skeleton.trunk, trunk_before, "ridge knobs moved the trunk");
}

/// Golden artifact; re-bless deliberately via
/// `cargo run -p course-framing --example bless_golden`.
#[test]
fn golden_framing_seed_1() {
    let golden = include_str!("golden_framing_seed_1.json");
    let f = generate(&spec_for(1, None));
    assert_eq!(
        f.canonical_json(),
        golden,
        "golden framing drifted — re-bless deliberately"
    );
    Framing::from_json(golden).unwrap();
}
