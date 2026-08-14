//! Why is skeleton agreement low? Distribution of derived accumulation at
//! S2 channel cells, with and without the datum transforms.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology;

fn main() {
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "assets/dictionary_v2.bin",
    ))
    .unwrap();
    let id = RunIdentity::from_seed(11);
    let spec = SiteSpec::generate_builtin(
        id,
        &SpecOverridesV2 { forced_biome: Some(BiomeId::Piedmont) },
    );
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let amp = course_amplify::generate(&spec, &sk, &dict, &id);
    for (label, transforms) in [
        ("no transforms", &[][..]),
        ("floodplain+wt", &hydrology::DEFAULT_TRANSFORMS[..]),
    ] {
        let t0 = std::time::Instant::now();
        let h = hydrology::generate(&spec, &sk, &amp, &id, transforms);
        println!("S4 took {:.0} ms (budget 250)", t0.elapsed().as_secs_f64() * 1e3);
        let n8 = sk.flow_distance.spec.nx as usize;
        let mut accs: Vec<f64> = Vec::new();
        for i in 0..n8 * n8 {
            if sk.flow_distance.data[i] == 0.0 {
                accs.push(h.flow_accum.data[i]);
            }
        }
        accs.sort_by(|a, b| a.total_cmp(b));
        let q = |p: f64| accs[((accs.len() - 1) as f64 * p) as usize];
        println!(
            "{label:14}: agreement {:.3} | acc at S2 channel cells p10 {:9.0} p50 {:9.0} p90 {:9.0} (threshold 120000)",
            h.skeleton_agreement,
            q(0.1),
            q(0.5),
            q(0.9)
        );
        let max = h.flow_accum.data.iter().cloned().fold(0.0f64, f64::max);
        println!(
            "               max acc anywhere {max:.0}  ({} channel cells)",
            accs.len()
        );
        // render: red = S2 channels, blue = derived acc (log), overlaid
        let n8 = sk.flow_distance.spec.nx as usize;
        let mut px = vec![0u8; n8 * n8 * 3];
        for i in 0..n8 * n8 {
            let a = (h.flow_accum.data[i].max(64.0)).ln();
            let t = ((a - 4.0) / 10.0).clamp(0.0, 1.0);
            let v = (t * 255.0) as u8;
            px[i * 3] = 30;
            px[i * 3 + 1] = 30;
            px[i * 3 + 2] = v;
            if sk.flow_distance.data[i] == 0.0 {
                px[i * 3] = 255;
                px[i * 3 + 1] = 40;
            }
        }
        let mut flipped = vec![0u8; px.len()];
        for y in 0..n8 {
            flipped[y * n8 * 3..(y + 1) * n8 * 3]
                .copy_from_slice(&px[(n8 - 1 - y) * n8 * 3..(n8 - y) * n8 * 3]);
        }
        image::RgbImage::from_raw(n8 as u32, n8 as u32, flipped)
            .unwrap()
            .save(format!("/tmp/agree_{label}.png").replace(' ', "_"))
            .unwrap();
    }
}
