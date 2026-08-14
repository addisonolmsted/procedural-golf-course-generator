//! Which window size makes lp400 structure-tensor coherence separate
//! grainy biomes (rv, hc) from isotropic ones (heathland, plains)?
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_amplify::conditioning::Conditioning;

fn main() {
    for biome in [
        BiomeId::RiverValley,
        BiomeId::HillCountry,
        BiomeId::Piedmont,
        BiomeId::GreatPlains,
        BiomeId::Heathland,
        BiomeId::Sandhills,
    ] {
        let id = RunIdentity::from_seed(3);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let n8 = sk.flow_distance.spec.nx as usize;
        let mut base8 = course_world::grid::Grid::filled(sk.flow_distance.spec, 0.0f64);
        for y in 0..n8 {
            for x in 0..n8 {
                let p = base8.spec.world_of(x as u32, y as u32);
                base8.data[y * n8 + x] = sk.height.bilinear(p);
            }
        }
        let cond = Conditioning::compute(&base8, &sk.flow_distance.data);
        print!("{:13}", spec.biome.key());
        // EXACT selection-path geometry: footprint origins from
        // patch_grid stride, coherence window 4x shifted-inward
        let (w, big) = (32usize, 128usize);
        let origins: Vec<usize> = {
            let mut xs: Vec<usize> =
                (0..).map(|i| i * 16).take_while(|&x| x + 32 <= n8).collect();
            if *xs.last().unwrap() + 32 != n8 {
                xs.push(n8 - 32);
            }
            xs
        };
        let mut cs: Vec<f64> = Vec::new();
        for &y0 in &origins {
            for &x0 in &origins {
                let yb = (y0 + w / 2).saturating_sub(big / 2).min(n8 - big);
                let xb = (x0 + w / 2).saturating_sub(big / 2).min(n8 - big);
                cs.push(cond.fabric_coherence(yb, xb, big).max(cond.channel_strength(y0, x0, w)));
            }
        }
        cs.sort_by(|a, b| a.total_cmp(b));
        let q = |p: f64| cs[((cs.len() - 1) as f64 * p) as usize];
        println!(
            "  selpath w128: p25 {:.2} p50 {:.2} p75 {:.2} | frac>0.40 {:.2}",
            q(0.25),
            q(0.5),
            q(0.75),
            cs.iter().filter(|&&c| c > 0.40).count() as f64 / cs.len() as f64
        );
    }
}
