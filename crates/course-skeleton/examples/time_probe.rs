//! Wall-clock for one S2 generate, per biome.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    for biome in BiomeId::ALL.iter() {
        let mut best = f64::INFINITY;
        for k in 0..3u64 {
            let id = RunIdentity::from_seed(48 + k);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let t = std::time::Instant::now();
            let _sk = course_skeleton::generate(&spec, &c1, &id);
            best = best.min(t.elapsed().as_secs_f64() * 1000.0);
        }
        println!("{:14} {:8.0} ms  (budget 900)", biome.key(), best);
    }
}
