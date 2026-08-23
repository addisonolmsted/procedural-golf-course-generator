//! Dump full A0–A6 tiles: height + water + blowout list.
use course_sandhills::{build_full, record::FormClass, texture};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let pack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_aeolian.bin")).expect("patch pack");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = RunIdentity::from_seed(seed);
        for (tag, form) in [("train", FormClass::Train), ("mound", FormClass::Mound)] {
            let t = build_full(&id, &pack, Some(form));
            gridio::write_grid_f32(&out.join(format!("{tag}_{seed}.cgrid")), &t.height).unwrap();
            gridio::write_grid_f32(&out.join(format!("{tag}_{seed}.water.cgrid")), &t.water).unwrap();
            let bl: String = t.blowouts.iter()
                .map(|b| format!("{:.1} {:.1} {:.1} {:.1}\n",
                                 b.center.x, b.center.y, b.radius_m, b.depth_m))
                .collect();
            std::fs::write(out.join(format!("{tag}_{seed}.blowouts.txt")), bl).unwrap();
            println!("  {tag} {seed}: {} blowouts, lake_frac {:.3}, river {}",
                     t.blowouts.len(), t.lake_frac, t.river.is_some());
        }
    }
}
