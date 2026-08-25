//! Dump X1+X2 assembled fluvial tiles (8 m) for the assembled round.
use course_sandhills::{assemble::HandProfile, build_fluvial_macro};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, net, asm) = build_fluvial_macro(&id, &prof);
        gridio::write_grid_f32(&out.join(format!("asm_{seed}.cgrid")), &asm.height)
            .unwrap();
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
        std::fs::write(out.join(format!("asm_{seed}.json")), j).unwrap();
        let mut v = asm.height.data.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let relief = v[(v.len() as f64 * 0.99) as usize]
            - v[(v.len() as f64 * 0.01) as usize];
        println!("seed {seed}: {} chans, relief {relief:.1} m", net.chans.len());
    }
}
