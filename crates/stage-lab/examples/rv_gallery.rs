//! Batch water-view renders for a 20-seed gallery (biome selectable:
//! `rv_gallery <outdir> [heathland|rv|...]`).
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use stage_lab::render_s34::{render_s4, S4View};

fn main() {
    let out = std::path::PathBuf::from(
        std::env::args().nth(1).unwrap_or_else(|| "/tmp/rv_gallery".into()),
    );
    let biome = match std::env::args().nth(2).as_deref() {
        None | Some("rv") | Some("river_valley") => BiomeId::RiverValley,
        Some("heathland") => BiomeId::Heathland,
        Some("piedmont") => BiomeId::Piedmont,
        Some("hill_country") | Some("hc") => BiomeId::HillCountry,
        Some("great_plains") | Some("plains") => BiomeId::GreatPlains,
        Some("sandhills") => BiomeId::Sandhills,
        Some(other) => panic!("unknown biome {other}"),
    };
    std::fs::create_dir_all(&out).unwrap();
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for seed in 1u64..=20 {
        let id = RunIdentity::from_seed(seed);
        let spec =
            SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        let h = course_transforms::hydrology::generate(
            &spec,
            &sk,
            &amp,
            &id,
            &course_transforms::hydrology::DEFAULT_TRANSFORMS,
        );
        render_s4(&h, &sk, S4View::Water, 1200, true)
            .save(out.join(format!("rv_{seed:02}.png")))
            .unwrap();
        eprintln!("seed {seed} done");
    }
}
