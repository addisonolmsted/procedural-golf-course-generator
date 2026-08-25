//! X3 — dump finished (textured, 2 m) fluvial tiles.
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
        let (_, _, _, tex) = build_fluvial_textured(&id, &prof, &pack);
        gridio::write_grid_f32(&out.join(format!("tex_{seed}.cgrid")), &tex).unwrap();
        println!("seed {seed} textured");
    }
}
