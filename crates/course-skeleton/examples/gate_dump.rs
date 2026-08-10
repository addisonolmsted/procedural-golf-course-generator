//! Dump base surfaces for D3 (conditioning overlap) + D6 (proxy): 6 biomes
//! × 5 seeds, 2 m height as CGRID1.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
fn main() {
    let out = std::path::Path::new("out/gate_dump");
    std::fs::create_dir_all(out).unwrap();
    for biome in BiomeId::ALL {
        for k in 0..5u64 {
            let seed = 20_000 + k;
            let id = RunIdentity::from_seed(seed);
            let spec = SiteSpec::generate_builtin(
                id,
                &SpecOverridesV2 { forced_biome: Some(biome) },
            );
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            course_world::gridio::write_grid_f32(
                &out.join(format!("{}_{seed}.cgrid", biome.key())),
                &sk.height,
            )
            .unwrap();
        }
    }
    println!("dumped 30 surfaces to out/gate_dump");
}
