//! Incision-boost ladder: one dry seed per biome rendered at five
//! boost levels, plus a numeric channel-incision depth (channel cell
//! vs its 80-120 m surroundings) so widths/depths can be chosen.
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
    for (biome, tag, seed) in
        [(BiomeId::HillCountry, "hc", 2u64), (BiomeId::Piedmont, "pied", 3)]
    {
        for boost in [1.0f64, 1.35, 1.6, 2.0, 2.5] {
            let id = RunIdentity::from_seed(seed);
            let mut spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            spec.dials.insert("skeleton.incision_boost".into(), boost);
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
            // channel incision: channel cell height vs mean of an
            // 80-120 m ring around it, on the final surface
            let n8 = sk.flow_distance.spec.nx as usize;
            let spec8 = sk.flow_distance.spec;
            let mut depths: Vec<f64> = Vec::new();
            for lin in 0..n8 * n8 {
                if sk.flow_distance.data[lin] > 0.0 {
                    continue;
                }
                let (y, x) = (lin / n8, lin % n8);
                if x < 16 || y < 16 || x >= n8 - 16 || y >= n8 - 16 {
                    continue;
                }
                let p0 = spec8.world_of(x as u32, y as u32);
                let z0 = h.height.bilinear(p0);
                let mut ring = 0.0;
                let mut cnt = 0.0;
                for k in 0..12 {
                    let a = k as f64 / 12.0 * std::f64::consts::TAU;
                    let r = 180.0;
                    let q = course_world::math::Vec2::new(
                        p0.x + r * a.cos(),
                        p0.y + r * a.sin(),
                    );
                    if q.x > 0.0 && q.y > 0.0 && q.x < 2990.0 && q.y < 2990.0 {
                        ring += h.height.bilinear(q);
                        cnt += 1.0;
                    }
                }
                if cnt > 6.0 {
                    depths.push(ring / cnt - z0);
                }
            }
            depths.sort_by(|a, b| a.total_cmp(b));
            let q = |p: f64| depths[((depths.len() - 1) as f64 * p) as usize];
            println!(
                "{tag} seed {seed} boost {boost:.2}: channel incision vs 180 m ring — p50 {:.2} m  p90 {:.2} m  (n={})",
                q(0.5),
                q(0.9),
                depths.len()
            );
            render_s4(&h, &sk, S4View::Water, 1200, true)
                .save(out.join(format!("{tag}_b{:.2}.png", boost)))
                .unwrap();
        }
    }
}
