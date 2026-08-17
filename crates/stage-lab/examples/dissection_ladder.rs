//! Dissection-direction ladder: trade S1 wave relief down against S2
//! incision up, so ridges read as interfluves left BETWEEN channels
//! (user direction: real pied/hc hills look carved BY the network).
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use stage_lab::render_s34::{render_s4, S4View};

fn main() {
    let out = std::path::PathBuf::from(std::env::args().nth(1).expect("outdir"));
    std::fs::create_dir_all(&out).unwrap();
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    // (label, relief_mult on S1 waves, incision_boost)
    let variants = [("A_current", 1.0f64, -1.0f64), ("B_dissected", 0.75, 2.2), ("C_strong", 0.55, 3.0)];
    for (biome, tag, seed) in
        [(BiomeId::HillCountry, "hc", 2u64), (BiomeId::Piedmont, "pied", 3)]
    {
        for (label, relief_mult, boost) in variants {
            let id = RunIdentity::from_seed(seed);
            let mut spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let r0 = *spec.dials.get("primitives.relief_amp_m").unwrap();
            spec.dials.insert("primitives.relief_amp_m".into(), r0 * relief_mult);
            if boost > 0.0 {
                spec.dials.insert("skeleton.incision_boost".into(), boost);
            }
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
                .save(out.join(format!("{tag}_{label}.png")))
                .unwrap();
            eprintln!("{tag} {label} done (relief x{relief_mult}, boost {boost})");
        }
    }
}
