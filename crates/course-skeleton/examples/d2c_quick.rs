//! Quick d2c median probe over N seeds for the named biomes.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    for biome in [BiomeId::HillCountry, BiomeId::RiverValley, BiomeId::Piedmont] {
        let mut meds = Vec::new();
        for k in 0..40u64 {
            let id = RunIdentity::from_seed(20_000 + k);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            let mut d: Vec<f64> = sk.flow_distance.data.clone();
            d.sort_by(|a, b| a.total_cmp(b));
            meds.push(d[d.len() / 2]);
        }
        meds.sort_by(|a, b| a.total_cmp(b));
        println!("{biome:?}: d2c med {:.0}", meds[meds.len() / 2]);
    }
}
