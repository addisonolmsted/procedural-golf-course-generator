//! First-light instrument for S3: run the full S0->S3 chain on one seed
//! per biome, print band stats + timing, dump hillshade PNGs.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::grid::Grid;

fn hillshade(z: &Grid<f64>, ex: f64) -> Vec<u8> {
    let (nx, ny) = (z.spec.nx as usize, z.spec.ny as usize);
    let cell = z.spec.cell_size;
    let mut out = vec![0u8; nx * ny];
    let (az, alt) = (315f64.to_radians(), 45f64.to_radians());
    for y in 0..ny {
        for x in 0..nx {
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(nx - 1);
            let ym = y.saturating_sub(1);
            let yp = (y + 1).min(ny - 1);
            let gx = (z.data[y * nx + xp] - z.data[y * nx + xm]) * ex
                / (((xp - xm).max(1)) as f64 * cell);
            let gy = (z.data[yp * nx + x] - z.data[ym * nx + x]) * ex
                / (((yp - ym).max(1)) as f64 * cell);
            let slope = (gx * gx + gy * gy).sqrt().atan();
            let aspect = libm::atan2(-gx, gy);
            let hs = alt.sin() * slope.cos() + alt.cos() * slope.sin() * (az - aspect).cos();
            out[y * nx + x] = (((hs + 0.15) / 1.15).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    out
}

fn save(path: &str, nx: usize, ny: usize, px: &[u8]) {
    let mut flipped = vec![0u8; px.len()];
    for y in 0..ny {
        flipped[y * nx..(y + 1) * nx].copy_from_slice(&px[(ny - 1 - y) * nx..(ny - y) * nx]);
    }
    image::GrayImage::from_raw(nx as u32, ny as u32, flipped).unwrap().save(path).unwrap();
}

fn main() {
    let out_dir = std::env::args().nth(1).unwrap_or_else(|| "/tmp/s3".into());
    let seed: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(48);
    std::fs::create_dir_all(&out_dir).unwrap();
    let dict = course_amplify::dictionary::Dictionary::load(std::path::Path::new(
        "assets/dictionary_v2.bin",
    ))
    .expect("dictionary");
    for biome in BiomeId::ALL.iter() {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let t = std::time::Instant::now();
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        eprintln!(
            "{:14} seed {seed}: S3 {ms:6.0} ms  mid_std {:.2}  fine_std {:.2}",
            biome.key(),
            amp.mid_std_m,
            amp.fine_std_m
        );
        let n2 = amp.height.spec.nx as usize;
        if std::env::var("DUMP_F32").is_ok() {
            let mut buf = Vec::with_capacity(n2 * n2 * 4);
            for v in &amp.height.data {
                buf.extend_from_slice(&(*v as f32).to_le_bytes());
            }
            std::fs::write(format!("{out_dir}/{}_{seed}.f32", biome.key()), &buf).unwrap();
        }
        save(
            &format!("{out_dir}/{}_{seed}_amplified.png", biome.key()),
            n2,
            n2,
            &hillshade(&amp.height, 1.5),
        );
        save(
            &format!("{out_dir}/{}_{seed}_base.png", biome.key()),
            n2,
            n2,
            &hillshade(&sk.height, 1.5),
        );
    }
}
