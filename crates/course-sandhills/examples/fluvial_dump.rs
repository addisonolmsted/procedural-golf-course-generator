//! Dump C1-C5 full fluvial tiles (textured, 2 m) for the textured round.
use course_sandhills::{build_fluvial_full, texture};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let pack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_fluvial.bin")).expect("fluvial pack");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, _, tex, net) = build_fluvial_full(&id, &pack);
        gridio::write_grid_f32(&out.join(format!("full_{seed}.cgrid")), &tex).unwrap();
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
        std::fs::write(out.join(format!("full_{seed}.json")), j).unwrap();
        println!("seed {seed} done");
    }
}
