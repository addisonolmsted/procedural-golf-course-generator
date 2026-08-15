//! River width distribution over rv seeds: per-tile min/mean/max
//! (breathing ±28%) and the cross-tile spread of tile means.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
fn main() {
    let mut means = Vec::new();
    for seed in 1u64..=24 {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(BiomeId::RiverValley) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let area = sk.channels.iter().filter(|c| c.parent.is_none())
            .map(|c| c.area_m2).fold(0.0f64, f64::max);
        let km2 = area / 1.0e6;
        let pers = 0.75 + id.course_scalar(0x81D7);
        let w = (8.5 * km2.sqrt() * pers).clamp(6.0, 80.0);
        means.push(w);
        println!("seed {seed:2}: trunk {km2:5.1} km2  width mean {w:5.1} m  range [{:.1}, {:.1}]",
                 w * 0.72, w * 1.28);
    }
    means.sort_by(|a, b| a.total_cmp(b));
    let q = |p: f64| means[((means.len() - 1) as f64 * p) as usize];
    println!("\ntile-mean widths across 24 seeds: p10 {:.0} p50 {:.0} p90 {:.0} (min {:.0}, max {:.0})",
             q(0.1), q(0.5), q(0.9), means[0], means[means.len()-1]);
}
