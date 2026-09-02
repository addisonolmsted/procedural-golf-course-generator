//! Dump aeolian tiles with the form drawn from the seed (no forcing).
use course_sandhills::{build_full, texture};
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
        let t = build_full(&id, &pack, None);
        gridio::write_grid_f32(&out.join(format!("a_{seed}.cgrid")), &t.height).unwrap();
        gridio::write_grid_f32(&out.join(format!("a_{seed}.water.cgrid")), &t.water).unwrap();
        if let Some(r) = &t.river {
            let txt: String = r.iter().map(|p| format!("{:.2} {:.2}\n", p.x, p.y)).collect();
            std::fs::write(out.join(format!("a_{seed}.creek.txt")), txt).unwrap();
        }
        println!("seed {seed}: {} blowouts, lake_frac {:.3}, river {}",
                 t.blowouts.len(), t.lake_frac, t.river.is_some());
    }
}
