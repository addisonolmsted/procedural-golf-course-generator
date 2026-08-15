//! Tinted rv verification renders: S2 base + S3 amplified per seed,
//! shared elevation scale (the stage-lab Amplify tab's own pair render).
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use stage_lab::render_s34::{render_s3_pair, render_s4, S4View};

fn main() {
    let out = std::path::Path::new("/tmp/rv_verify_tinted");
    std::fs::create_dir_all(out).unwrap();
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for seed in [3u64, 9, 17, 25, 31, 48] {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(
            id,
            &SpecOverridesV2 { forced_biome: Some(BiomeId::RiverValley) },
        );
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        let (base, ampi) = render_s3_pair(&sk.height, &amp.height, 900);
        base.save(out.join(format!("rv_{seed}_base.png"))).unwrap();
        ampi.save(out.join(format!("rv_{seed}_amp.png"))).unwrap();
        let h = course_transforms::hydrology::generate(
            &spec,
            &sk,
            &amp,
            &id,
            &course_transforms::hydrology::DEFAULT_TRANSFORMS,
        );
        render_s4(&h, &sk, S4View::Water, 900, true)
            .save(out.join(format!("rv_{seed}_water.png")))
            .unwrap();
        eprintln!("seed {seed} done");
    }
}
