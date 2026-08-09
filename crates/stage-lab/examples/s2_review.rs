//! Headless S2 review sheets (D4 material): per biome, 3 seeds of the base
//! view with full overlays, plus the four transform-field views for the
//! first seed. Review guidance: candle-wax between channels is EXPECTED —
//! judge the network per the S2 banner and stage-02 acceptance list.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use stage_lab::render_s2::{render_s2, S2View};

fn main() {
    let out = std::path::Path::new("crates/stage-lab/out/s2_review");
    std::fs::create_dir_all(out).unwrap();
    let mut n = 0;
    for biome in BiomeId::ALL {
        for (k, seed) in [301u64, 302, 303].iter().enumerate() {
            let id = RunIdentity::from_seed(*seed);
            let spec = SiteSpec::generate_builtin(
                id,
                &SpecOverridesV2 { forced_biome: Some(biome) },
            );
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            let img = render_s2(&sk, S2View::Base, 900, true);
            img.save(out.join(format!("{}_{}.png", biome.key(), seed))).unwrap();
            n += 1;
            if k == 0 {
                for (view, tag) in [
                    (S2View::FlowDistance, "fdist"),
                    (S2View::FlowNorm, "fnorm"),
                    (S2View::Hillslope, "hpos"),
                    (S2View::Accum, "accum"),
                ] {
                    let img = render_s2(&sk, view, 600, false);
                    img.save(out.join(format!("{}_{}_{}.png", biome.key(), seed, tag)))
                        .unwrap();
                    n += 1;
                }
            }
        }
    }
    println!("wrote {n} images to crates/stage-lab/out/s2_review");
}
