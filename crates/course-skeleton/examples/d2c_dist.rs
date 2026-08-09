use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
fn main() {
    println!("GENERATED d2c distribution (authored network):");
    for biome in [BiomeId::Piedmont, BiomeId::HillCountry] {
        for seed in [301u64, 302] {
            let id = RunIdentity::from_seed(seed);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            let mut d: Vec<f64> = sk.flow_distance.data.clone();
            d.sort_by(|a, b| a.total_cmp(b));
            let q = |p: f64| d[((d.len() - 1) as f64 * p) as usize];
            println!(
                "{biome:?} seed {seed}: p25 {:3.0} p50 {:3.0} p75 {:3.0} p90 {:3.0} max {:3.0}",
                q(0.25), q(0.5), q(0.75), q(0.9), q(1.0)
            );
        }
    }
}
