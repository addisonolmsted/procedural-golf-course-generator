//! Diagnostic for the proposed flow-routed gully tier: dump the S2/S4
//! surfaces plus an 8 m drained-area grid derived from the skeleton's
//! receiver directions, so the sub-threshold flow paths (the candidate
//! gully scaffold) can be drawn against the terrain and compared with
//! the real tiles' extracted networks.
//!
//!   cargo run --release -p course-transforms --example gully_scaffold -- <out> <biome> <seed>
//!   -> <out>/<biome>_<seed>_s4.f32      final 2 m surface
//!      <out>/<biome>_<seed>_s2.f32      skeleton 2 m surface
//!      <out>/<biome>_<seed>_accum8.f32  drained area m^2 at 8 m
//!      <out>/<biome>_<seed>_chan8.u8    tier-1 channel mask (flow_distance == 0)

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology;

fn main() {
    let out = std::env::args().nth(1).expect("out dir");
    std::fs::create_dir_all(&out).unwrap();
    let biome_arg = std::env::args().nth(2).expect("biome");
    let seed: u64 = std::env::args().nth(3).expect("seed").parse().unwrap();
    let biome = BiomeId::ALL
        .iter()
        .copied()
        .find(|b| {
            format!("{b:?}").to_lowercase().replace(' ', "_")
                == biome_arg.to_lowercase()
                || SiteSpec::generate_builtin(
                    RunIdentity::from_seed(1),
                    &SpecOverridesV2 { forced_biome: Some(*b) },
                )
                .biome
                .key()
                    == biome_arg
        })
        .expect("biome key");
    let id = RunIdentity::from_seed(seed);
    let mut spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
    // ladder overrides (visual calibration): TRIB_REACH / TRIB_DEPTH env
    if let Ok(v) = std::env::var("TRIB_REACH") {
        if let Ok(x) = v.parse::<f64>() {
            spec.dials.insert("skeleton.tributary_reach".into(), x);
        }
    }
    if let Ok(v) = std::env::var("TRIB_DEPTH") {
        if let Ok(x) = v.parse::<f64>() {
            spec.dials.insert("skeleton.tributary_depth_m".into(), x);
        }
    }
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let amp = course_amplify::generate(&spec, &sk, &dict, &id);
    let h = hydrology::generate(&spec, &sk, &amp, &id, &hydrology::DEFAULT_TRANSFORMS);

    let n8 = sk.flow_distance.spec.nx as usize;
    let cell8 = sk.flow_distance.spec.cell_size;
    // 8 m heights (skeleton surface sampled at the 8 m nodes)
    let mut z8 = vec![0.0f64; n8 * n8];
    for y in 0..n8 {
        for x in 0..n8 {
            let p = sk.flow_distance.spec.world_of(x as u32, y as u32);
            z8[y * n8 + x] = sk.height.bilinear(p);
        }
    }
    // receivers from flow_dir_rad (direction TOWARD the receiver)
    let mut rec: Vec<i64> = vec![-1; n8 * n8];
    for y in 0..n8 {
        for x in 0..n8 {
            let a = sk.flow_dir_rad.data[y * n8 + x];
            if !a.is_finite() {
                continue;
            }
            let dx = libm::cos(a).round() as i64;
            let dy = libm::sin(a).round() as i64;
            let (xx, yy) = (x as i64 + dx, y as i64 + dy);
            if xx >= 0 && yy >= 0 && xx < n8 as i64 && yy < n8 as i64 {
                rec[y * n8 + x] = yy * n8 as i64 + xx;
            }
        }
    }
    // accumulate in height-descending order
    let mut order: Vec<usize> = (0..n8 * n8).collect();
    order.sort_by(|&a, &b| z8[b].partial_cmp(&z8[a]).unwrap());
    let cell_area = cell8 * cell8;
    let mut acc = vec![cell_area; n8 * n8];
    for &i in &order {
        if rec[i] >= 0 {
            let r = rec[i] as usize;
            acc[r] += acc[i];
        }
    }

    let dump32 = |name: &str, data: &[f64]| {
        let mut buf = Vec::with_capacity(data.len() * 4);
        for v in data {
            buf.extend_from_slice(&(*v as f32).to_le_bytes());
        }
        std::fs::write(format!("{out}/{}_{seed}_{name}.f32", spec.biome.key()), &buf).unwrap();
    };
    dump32("s4", &h.height.data);
    dump32("s2", &sk.height.data);
    dump32("accum8", &acc);
    let cm: Vec<u8> = sk
        .flow_distance
        .data
        .iter()
        .map(|&d| if d <= 0.0 { 1u8 } else { 0u8 })
        .collect();
    std::fs::write(format!("{out}/{}_{seed}_chan8.u8", spec.biome.key()), &cm).unwrap();
    eprintln!("{} {} done", spec.biome.key(), seed);
}
