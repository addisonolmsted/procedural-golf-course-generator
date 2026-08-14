//! S3 acceptance: determinism, taper, biome-blindness.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn run(seed: u64, biome: BiomeId) -> course_amplify::Amplified {
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .expect("dictionary");
    let id = RunIdentity::from_seed(seed);
    let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    course_amplify::generate(&spec, &sk, &dict, &id)
}

#[test]
fn determinism_double_run() {
    let a = run(7, BiomeId::Piedmont);
    let b = run(7, BiomeId::Piedmont);
    assert_eq!(a.height.data, b.height.data, "same seed must be byte-identical");
}

#[test]
fn taper_holds_channel_profile() {
    // Channel cells must move less than tolerance between base and
    // amplified: the taper floor bounds it structurally, this pins it.
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "../../assets/dictionary_v2.bin",
    ))
    .unwrap();
    for biome in [BiomeId::Piedmont, BiomeId::RiverValley] {
        let id = RunIdentity::from_seed(11);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        let n8 = sk.flow_distance.spec.nx as usize;
        let mut worst = 0.0f64;
        for y8 in 0..n8 {
            for x8 in 0..n8 {
                if sk.flow_distance.data[y8 * n8 + x8] > 0.0 {
                    continue; // only channel cells
                }
                let p = sk.flow_distance.spec.world_of(x8 as u32, y8 as u32);
                let dz = (amp.height.bilinear(p) - sk.height.bilinear(p)).abs();
                worst = worst.max(dz);
            }
        }
        assert!(
            worst < 1.2,
            "{biome:?}: channel cell moved {worst:.2} m under amplification"
        );
    }
}

#[test]
fn no_biome_branch_in_crate() {
    // The biome may only select the dictionary shelf. Any other biome
    // reference in the crate is a branch waiting to happen.
    let src = concat!(
        include_str!("../src/lib.rs"),
        include_str!("../src/synth.rs"),
        include_str!("../src/blend.rs"),
        include_str!("../src/polish.rs"),
        include_str!("../src/conditioning.rs"),
    );
    for name in ["piedmont", "sandhills", "heathland", "river_valley", "great_plains", "hill_country"] {
        assert!(
            !src.to_lowercase().contains(name),
            "biome name `{name}` appears in course-amplify source"
        );
    }
}

#[test]
fn deranged_and_integrated_get_different_texture_stats() {
    // Conditioning demonstrably works: two biomes with the same code path
    // must land different band statistics (their dictionaries differ).
    let a = run(21, BiomeId::HillCountry);
    let b = run(21, BiomeId::RiverValley);
    assert!(
        (a.fine_std_m - b.fine_std_m).abs() > 0.1,
        "hc {} vs rv {} fine_std should differ",
        a.fine_std_m,
        b.fine_std_m
    );
}
