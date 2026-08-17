//! Slot-diagnosis stage dumps: S2 (2 m presentation), S3 (amplified),
//! S4 (final) heights for given biome/seeds — attribution by first
//! appearance. Usage: stage_dump <outdir> <biome> <seed> [<seed>...]
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology;

fn write_f32(path: &str, data: &[f64]) {
    let mut buf = Vec::with_capacity(data.len() * 4);
    for v in data {
        buf.extend_from_slice(&(*v as f32).to_le_bytes());
    }
    std::fs::write(path, &buf).unwrap();
}

fn main() {
    let out = std::env::args().nth(1).expect("outdir");
    std::fs::create_dir_all(&out).unwrap();
    let biome = match std::env::args().nth(2).expect("biome").as_str() {
        "piedmont" => BiomeId::Piedmont,
        "hc" | "hill_country" => BiomeId::HillCountry,
        other => panic!("unknown biome {other}"),
    };
    let seeds: Vec<u64> = std::env::args().skip(3).filter_map(|s| s.parse().ok()).collect();
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for &seed in &seeds {
        let id = RunIdentity::from_seed(seed);
        let spec =
            SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        let h = hydrology::generate(&spec, &sk, &amp, &id, &hydrology::DEFAULT_TRANSFORMS);
        let key = spec.biome.key();
        write_f32(&format!("{out}/{key}_{seed}_s2.f32"), &sk.height.data);
        write_f32(&format!("{out}/{key}_{seed}_s3.f32"), &amp.height.data);
        write_f32(&format!("{out}/{key}_{seed}_s4.f32"), &h.height.data);
        // channel mask at 8 m (flow_distance == 0), as u8
        let cm: Vec<u8> = sk
            .flow_distance
            .data
            .iter()
            .map(|&d| if d <= 0.0 { 1u8 } else { 0u8 })
            .collect();
        std::fs::write(format!("{out}/{key}_{seed}_chan8.u8"), &cm).unwrap();
        eprintln!("{key} {seed} done");
    }
}
