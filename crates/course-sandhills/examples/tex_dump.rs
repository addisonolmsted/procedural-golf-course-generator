//! Dump TEXTURED tiles at 2 m — the Phase 5 measurement path.
use course_sandhills::{draw, record::FormClass, rng, surface, texture, wind, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let pack = texture::PatchPack::load(std::path::Path::new("assets/sandhills_patches_aeolian.bin"))
        .expect("assets/sandhills_patches.bin (build_patchpack.py)");
    println!("pack: {}x{} patches", pack.patch, pack.patch);
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = RunIdentity::from_seed(seed);
        for (tag, form) in [("train", FormClass::Train), ("mound", FormClass::Mound)] {
            let d = draw::site(&id, Some(Mode::Aeolian), Some(form));
            let mut r = rng::stream(&id, rng::WIND);
            let f = wind::build(&mut r, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                                d.wind_wander_m, d.kappa, 0.0);
            let mut hr = rng::stream(&id, rng::HUMMOCK);
            let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                                 d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
            let sf = surface::build(&mut rng::stream(&id, rng::PATCHY), &f, &hw, &d);
            let mut tr = rng::stream(&id, rng::TEXTURE);
            let tex = texture::quilt(&mut tr, &pack, &sf.height, &d);
            gridio::write_grid_f32(&out.join(format!("{tag}_{seed}.cgrid")), &tex).unwrap();
            gridio::write_grid_f32(&out.join(format!("{tag}_{seed}_notex.cgrid")), &sf.height).unwrap();
            println!("  {tag} {seed}: {}x{} @2 m", tex.spec.nx, tex.spec.ny);
        }
    }
}
