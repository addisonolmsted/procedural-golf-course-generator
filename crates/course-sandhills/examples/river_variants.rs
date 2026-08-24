//! River planform variants for review pass/fail — same code, different numbers.
use course_sandhills::{draw, record::FormClass, rng, surface, texture, water, wind, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let out = std::path::Path::new("/tmp/river_var");
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).unwrap();
    let pack = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_aeolian.bin")).expect("pack");
    let variants: [(&str, water::RiverStyle); 5] = [
        ("A_lazy",    water::RiverStyle { lam: (420.0, 560.0), swing: (0.70, 0.95), wander: 0.55 }),
        ("B_classic", water::RiverStyle { lam: (300.0, 420.0), swing: (0.95, 1.20), wander: 0.65 }),
        ("C_loops",   water::RiverStyle::default()),
        ("D_mixed",   water::RiverStyle { lam: (260.0, 480.0), swing: (0.75, 1.45), wander: 0.90 }),
        ("E_wander",  water::RiverStyle { lam: (500.0, 700.0), swing: (0.40, 0.60), wander: 1.10 }),
    ];
    for seed in [5u64, 24] {
        let id = RunIdentity::from_seed(seed);
        let d = draw::site(&id, Some(Mode::Aeolian), Some(FormClass::Train));
        let mut wr = rng::stream(&id, rng::WIND);
        let w = wind::build(&mut wr, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                             d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let mut br = rng::stream(&id, rng::PATCHY);
        let sf = surface::build(&mut br, &w, &hw, &d);
        let mut tr = rng::stream(&id, rng::TEXTURE);
        let base = texture::quilt(&mut tr, &pack, &sf.height, &d);
        for (name, st) in &variants {
            let mut h = course_world::grid::Grid { spec: base.spec, data: base.data.clone() };
            let mut wat = water::find(&h, &sf.datum, &d, None);
            let mut rr = rng::stream(&id, rng::WATER);
            water::river_styled(&mut rr, &mut h, &mut wat, &d, *st, true);
            gridio::write_grid_f32(&out.join(format!("{name}_{seed}.cgrid")), &h).unwrap();
            gridio::write_grid_f32(&out.join(format!("{name}_{seed}.water.cgrid")), &wat.surface).unwrap();
            println!("  {name} seed {seed}");
        }
    }
}
