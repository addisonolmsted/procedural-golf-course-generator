//! Headless smoke render of the S3/S4 tab views — writes exactly what the
//! tabs display, so the tab wiring is verifiable without a GPU session.
//!
//!   cargo run -p stage-lab --release --example s34_contact
//!   → crates/stage-lab/out/s34_preview/<biome>_<seed>_{base,amp,delta,water,agree}.png

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use stage_lab::render_s34::{
    render_s3_delta, render_s3_pair, render_s4, terrain_stats, S4View, BIOME_REF,
};

fn main() {
    let out = std::path::Path::new("crates/stage-lab/out/s34_preview");
    std::fs::create_dir_all(out).unwrap();
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for (biome, seed) in [
        (BiomeId::Piedmont, 48u64),
        (BiomeId::RiverValley, 48),
        (BiomeId::Heathland, 48),
        (BiomeId::HillCountry, 7),
    ] {
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
        let key = format!("{}_{seed}", spec.biome.key());
        let (base, ampi) = render_s3_pair(&sk.height, &amp.height, 900);
        base.save(out.join(format!("{key}_base.png"))).unwrap();
        ampi.save(out.join(format!("{key}_amp.png"))).unwrap();
        render_s3_delta(&sk.height, &amp.height, 900)
            .save(out.join(format!("{key}_delta.png")))
            .unwrap();
        render_s4(&h, &sk, S4View::Water, 900, true)
            .save(out.join(format!("{key}_water.png")))
            .unwrap();
        render_s4(&h, &sk, S4View::Agreement, 900, true)
            .save(out.join(format!("{key}_agree.png")))
            .unwrap();
        let st = terrain_stats(&h.height);
        let reference = BIOME_REF
            .iter()
            .find(|(k, _, _)| *k == spec.biome.key())
            .map(|&(_, s, r)| format!("real: slope {s:.1}% relief {r:.0} m"))
            .unwrap_or_default();
        println!(
            "{key}: agree {:.2}, {} water, {} basins | slope med {:.1}% p90 {:.1}% relief {:.0} m | {reference}",
            h.skeleton_agreement,
            h.water.len(),
            h.basins.len(),
            st.median_pct,
            st.p90_pct,
            st.relief_m
        );
    }
}
