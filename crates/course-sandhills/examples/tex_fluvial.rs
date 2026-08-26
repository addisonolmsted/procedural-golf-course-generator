//! X3+X4 — dump finished fluvial tiles: textured ground, bays, water.
use course_sandhills::{assemble::HandProfile, build_fluvial_textured, texture};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");
    let pack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_fluvial.bin")).expect("fluvial pack");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, _, _, tex, water, bays) = build_fluvial_textured(&id, &prof, &pack);
        gridio::write_grid_f32(&out.join(format!("tex_{seed}.cgrid")), &tex).unwrap();
        gridio::write_grid_f32(&out.join(format!("tex_{seed}.water.cgrid")),
                               &water.surface).unwrap();
        let bl: String = bays.iter()
            .map(|b| format!("{:.0} {:.0} {:.0} {:.0} {:.2}\n",
                             b.center.x, b.center.y, b.a_m, b.b_m, b.depth_m))
            .collect();
        std::fs::write(out.join(format!("tex_{seed}.bays.txt")), bl).unwrap();
        println!("seed {seed}: {} bays, lake_frac {:.4}", bays.len(), water.lake_frac);
    }
}
