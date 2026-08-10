use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use std::collections::BTreeMap;
fn main() {
    let mut per: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    for k in 0..60u64 {
        let id = RunIdentity::from_seed(40_000 + k);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(BiomeId::RiverValley) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let g = &sk.height;
        let (a, b) = (375u32, 1125u32);
        let mut v = Vec::new();
        for y in a..b { for x in a..b { v.push(*g.get(x, y)); } }
        v.sort_by(|x, y| x.total_cmp(y));
        let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
        per.entry(format!("{:?}", spec.structure_class.window)).or_default().push(q(0.95) - q(0.05));
    }
    for (k, mut v) in per {
        v.sort_by(|a, b| a.total_cmp(b));
        println!("{k:16} n={:2}  core relief med {:6.1} m", v.len(), v[v.len() / 2]);
    }
}
