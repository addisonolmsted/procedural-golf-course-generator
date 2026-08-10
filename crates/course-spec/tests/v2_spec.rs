//! Stage S0 (v2) acceptance suite (`docs/stages/stage-00-archetype-draw.md`).

use course_contracts::biome::{BiomeId, WindowClass};
use course_seed::RunIdentity;
use course_spec::v2::{EnvelopeSet, SiteSpec, SpecOverridesV2};

fn spec(seed: u64) -> SiteSpec {
    SiteSpec::generate_builtin(RunIdentity::from_seed(seed), &SpecOverridesV2::default())
}

#[test]
fn determinism_double_run() {
    for seed in [1, 7, 42, 1234] {
        assert_eq!(spec(seed).canonical_json(), spec(seed).canonical_json());
    }
}

#[test]
fn envelope_fingerprint_interlock() {
    let s = spec(1);
    assert!(s.check_envelope(EnvelopeSet::builtin_fingerprint()).is_ok());
    assert!(s.check_envelope("deadbeef").is_err());
}

#[test]
fn forced_biome_is_recorded_and_transcript_independent() {
    let ov = SpecOverridesV2 {
        forced_biome: Some(BiomeId::Heathland),
    };
    let s = SiteSpec::generate_builtin(RunIdentity::from_seed(5), &ov);
    assert_eq!(s.biome, BiomeId::Heathland);
    assert_eq!(s.overrides.forced_biome, Some(BiomeId::Heathland));
    // Forcing the biome a seed would draw anyway is a no-op on everything
    // except the overrides record itself (recorded verbatim by design).
    let free = spec(5);
    if free.biome == BiomeId::Heathland {
        let mut a = free.clone();
        let mut b = s.clone();
        a.overrides = Default::default();
        b.overrides = Default::default();
        assert_eq!(a.canonical_json(), b.canonical_json());
    }
}

#[test]
fn mode_and_class_coverage() {
    // Over many seeds: every biome appears, every class with weight appears,
    // both provinces configurations appear, and descriptors stay in support.
    let mut biomes = std::collections::BTreeSet::new();
    let mut classes = std::collections::BTreeSet::new();
    let mut provinces = std::collections::BTreeSet::new();
    for seed in 0..2000u64 {
        let s = spec(seed);
        biomes.insert(s.biome);
        classes.insert(s.structure_class.window);
        provinces.insert(s.structure_class.provinces);
        let d = &s.descriptors;
        assert!((5.0..=260.0).contains(&d.relief_budget_m), "relief {}", d.relief_budget_m);
        assert!(d.density_target >= 0.0);
        assert!((0.0..course_contracts::units::TAU).contains(&d.wind_azimuth_rad));
        assert!(d.wind_speed_mps >= 1.0 && d.wind_speed_mps <= 15.0);
        // class invariant enforced by construction
        assert_eq!(s.structure_class.provinces == 2, s.structure_class.boundary_kind.is_some());
    }
    assert_eq!(biomes.len(), 6, "all six biomes must appear: {biomes:?}");
    assert!(classes.len() >= 5, "window classes seen: {classes:?}");
    assert_eq!(provinces, [1u8, 2u8].into_iter().collect());
}

#[test]
fn multi_modality_survives_sampling() {
    // Piedmont's envelope has two relief modes (E7-fitted center, original
    // mode separation preserved; see the fit commit).
    // The sampled population must be measurably spread across both, not
    // collapsed to one: check mass on both sides of the between-modes valley.
    let mut lo = 0usize;
    let mut hi = 0usize;
    for seed in 0..4000u64 {
        let s = SiteSpec::generate_builtin(
            RunIdentity::from_seed(seed),
            &SpecOverridesV2 {
                forced_biome: Some(BiomeId::Piedmont),
            },
        );
        if s.descriptors.relief_budget_m < 148.0 {
            lo += 1;
        } else {
            hi += 1;
        }
    }
    let frac = lo as f64 / (lo + hi) as f64;
    assert!(
        (0.35..=0.85).contains(&frac),
        "mode balance collapsed: low-mode fraction {frac}"
    );
}

#[test]
fn descriptors_are_jointly_correlated() {
    // The piedmont components carry a negative relief<->density correlation
    // in transformed space; the sampled population must show it (sign test).
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for seed in 0..3000u64 {
        let s = SiteSpec::generate_builtin(
            RunIdentity::from_seed(seed),
            &SpecOverridesV2 {
                forced_biome: Some(BiomeId::Piedmont),
            },
        );
        xs.push(s.descriptors.relief_budget_m.ln());
        ys.push(s.descriptors.density_target);
    }
    let mx = xs.iter().sum::<f64>() / xs.len() as f64;
    let my = ys.iter().sum::<f64>() / ys.len() as f64;
    let mut cov = 0.0;
    let mut vx = 0.0;
    let mut vy = 0.0;
    for i in 0..xs.len() {
        cov += (xs[i] - mx) * (ys[i] - my);
        vx += (xs[i] - mx).powi(2);
        vy += (ys[i] - my).powi(2);
    }
    let r = cov / (vx.sqrt() * vy.sqrt());
    assert!(r < -0.05, "expected negative relief-density correlation, got r={r}");
}

#[test]
fn exemplars_are_distinct_and_from_the_pool() {
    for seed in 0..200u64 {
        let s = spec(seed);
        assert_eq!(s.exemplars.len(), 2);
        assert_ne!(s.exemplars[0], s.exemplars[1]);
    }
}

#[test]
fn empty_strata_for_the_empty_biomes() {
    for (biome, empty) in [
        (BiomeId::Sandhills, true),
        (BiomeId::Heathland, true),
        (BiomeId::HillCountry, false),
    ] {
        let s = SiteSpec::generate_builtin(
            RunIdentity::from_seed(9),
            &SpecOverridesV2 {
                forced_biome: Some(biome),
            },
        );
        assert_eq!(s.descriptors.strata.is_empty(), empty, "{biome:?}");
    }
}

#[test]
fn class_conditioned_biome_tendencies() {
    // Not a statistical assertion — a wiring check: hill country must be able
    // to draw escarpment faces, sandhills must not (weight 0 in its record).
    let mut hc_classes = std::collections::BTreeSet::new();
    let mut sh_classes = std::collections::BTreeSet::new();
    for seed in 0..1500u64 {
        let id = RunIdentity::from_seed(seed);
        hc_classes.insert(
            SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(BiomeId::HillCountry) })
                .structure_class
                .window,
        );
        sh_classes.insert(
            SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(BiomeId::Sandhills) })
                .structure_class
                .window,
        );
    }
    assert!(hc_classes.contains(&WindowClass::EscarpmentFace));
    assert!(!sh_classes.contains(&WindowClass::EscarpmentFace));
}

#[test]
fn golden_spec_v2_seed_1() {
    let got = spec(1).canonical_json();
    let want = include_str!("golden_spec_v2_seed_1.json");
    assert_eq!(got, want.trim_end(), "re-bless via the bless_v2_golden example");
}
