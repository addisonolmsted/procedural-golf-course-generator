//! Natural-draw tiles for the cross-engineer blind: no forced form class.
use course_sandhills::{build_full, texture};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let pack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_aeolian.bin")).expect("pack");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let t = build_full(&RunIdentity::from_seed(seed), &pack, None);
        gridio::write_grid_f32(&out.join(format!("g_{seed}.cgrid")), &t.height).unwrap();
        gridio::write_grid_f32(&out.join(format!("g_{seed}.water.cgrid")), &t.water).unwrap();
        println!("  {seed}: {:?} river {}", t.d.form, t.river.is_some());
    }
}
