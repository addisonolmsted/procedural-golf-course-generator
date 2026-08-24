//! Dump C1-C3 carved tiles for the carved review round: carved height +
//! datum + channels JSON (net_dump format).
use course_sandhills::{build_fluvial_carved, channel};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, datum, height, net) = build_fluvial_carved(&id);
        gridio::write_grid_f32(&out.join(format!("carve_{seed}.cgrid")), &height).unwrap();
        gridio::write_grid_f32(&out.join(format!("carve_{seed}.datum.cgrid")), &datum).unwrap();
        let mut j = format!("{{\"seed\":{seed},\"chans\":[");
        for (i, c) in net.chans.iter().enumerate() {
            if i > 0 { j.push(','); }
            j.push_str(&format!("{{\"tier\":{},\"pts\":[", c.tier));
            for (k, p) in c.pts.iter().enumerate() {
                if k > 0 { j.push(','); }
                j.push_str(&format!("[{:.0},{:.0}]", p.x, p.y));
            }
            j.push_str("]}");
        }
        j.push_str("]}");
        std::fs::write(out.join(format!("carve_{seed}.json")), j).unwrap();
        let mut cut_max = 0.0f64;
        let mut cut_sum = 0.0f64;
        for i in 0..height.data.len() {
            let c = datum.data[i] - height.data[i];
            cut_max = cut_max.max(c);
            cut_sum += c;
        }
        println!("seed {seed}: max cut {cut_max:.1} m, mean {:.2} m",
                 cut_sum / height.data.len() as f64);
    }
}
