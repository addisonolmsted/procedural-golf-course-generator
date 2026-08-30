//! Dump specific seeds (mode-respecting): seed_dump <outdir> <seed>...
use course_sandhills::{assemble::HandProfile, build_full, build_fluvial_textured,
                       draw, texture, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let aeo = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_aeolian.bin")).expect("aeolian pack");
    let flu = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_fluvial.bin")).expect("fluvial pack");
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = RunIdentity::from_seed(seed);
        let d = draw::site(&id, None, None);
        let (h, w) = if d.mode == Mode::Aeolian {
            let t = build_full(&id, &aeo, None);
            (t.height, t.water)
        } else {
            let (_, _, _, tex, wat, _) = build_fluvial_textured(&id, &prof, &flu);
            (tex, wat.surface)
        };
        gridio::write_grid_f32(&out.join(format!("{seed}.cgrid")), &h).unwrap();
        gridio::write_grid_f32(&out.join(format!("{seed}.water.cgrid")), &w).unwrap();
        println!("{seed} done");
    }
}
