//! Dump C1 implied surfaces (the S1 macro field alone) for band analysis.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
fn main() {
    let out = std::path::Path::new("out/macro_dump");
    std::fs::create_dir_all(out).unwrap();
    for biome in BiomeId::ALL {
        for k in 0..6u64 {
            let id = RunIdentity::from_seed(50_000 + k);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let mut implied = c1.tilt.clone();
            for (i, v) in implied.data.iter_mut().enumerate() {
                *v += c1.relief.data[i];
            }
            course_world::gridio::write_grid_f32(
                &out.join(format!("{}_{}.cgrid", biome.key(), 50_000 + k)),
                &implied,
            ).unwrap();
        }
    }
    println!("dumped 36 macro surfaces");
}
