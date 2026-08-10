//! E7 fit instrument: generated core relief (p95-p5) per biome, JSON out.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
fn main() {
    print!("{{");
    for (bi, biome) in BiomeId::ALL.iter().enumerate() {
        let mut meds = Vec::new();
        for k in 0..8u64 {
            let id = RunIdentity::from_seed(30_000 + k);
            let spec = SiteSpec::generate_builtin(
                id,
                &SpecOverridesV2 { forced_biome: Some(*biome) },
            );
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            let g = &sk.height;
            let (c0, c1i) = ((750.0 / 2.0) as u32, (2250.0 / 2.0) as u32);
            let mut v = Vec::new();
            for y in c0..c1i {
                for x in c0..c1i {
                    v.push(*g.get(x, y));
                }
            }
            v.sort_by(|a, b| a.total_cmp(b));
            let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
            meds.push(q(0.95) - q(0.05));
        }
        meds.sort_by(|a, b| a.total_cmp(b));
        if bi > 0 { print!(","); }
        print!("\"{}\":{:.2}", biome.key(), meds[meds.len() / 2]);
    }
    println!("}}");
}
