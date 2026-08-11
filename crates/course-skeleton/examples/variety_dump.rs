//! Variety-audit dump: S2 output surfaces as raw f32 rasters (375×375 at
//! 8 m — the same decimation the real-corpus analysis uses), 20 seeds per
//! biome. Consumed by the macro variety audit script.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let out_dir = std::env::args().nth(1).expect("usage: variety_dump <out_dir>");
    std::fs::create_dir_all(&out_dir).unwrap();
    for biome in BiomeId::ALL.iter() {
        for k in 0..20u64 {
            let seed = 41_000 + k;
            let id = RunIdentity::from_seed(seed);
            let spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let s1_only = std::env::var("S1_ONLY").is_ok();
            let mut buf: Vec<u8> = Vec::with_capacity(375 * 375 * 4);
            if s1_only {
                // the C1 "Implied" view: tilt + relief, 8 m grid
                for y in 0..375u32 {
                    for x in 0..375u32 {
                        let v = (*c1.tilt.get(x, y) + *c1.relief.get(x, y)) as f32;
                        buf.extend_from_slice(&v.to_le_bytes());
                    }
                }
            } else {
                let sk = course_skeleton::generate(&spec, &c1, &id);
                let g = &sk.height; // 2 m grid, 1500 squared
                for y in 0..375u32 {
                    for x in 0..375u32 {
                        let v = *g.get(x * 4, y * 4) as f32;
                        buf.extend_from_slice(&v.to_le_bytes());
                    }
                }
            }
            let path = format!("{}/{}_{}.f32", out_dir, biome.key(), seed);
            std::fs::write(path, &buf).unwrap();
        }
        eprintln!("dumped {}", biome.key());
    }
}
