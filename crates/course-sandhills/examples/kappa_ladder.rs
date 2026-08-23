//! Concentration ladder — the operating point is chosen against measured
//! numbers, never by eye alone. Sweeps the von Mises `kappa` that separates
//! the two dune form classes.
use course_sandhills::{draw, record::FormClass, rng, surface, wind, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let out = std::path::Path::new("/tmp/sand_ladder");
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).unwrap();
    for kap in [0.10f64, 0.20, 0.40, 0.70, 1.20, 2.50, 5.00, 10.0, 20.0, 60.0] {
        let mut spread = 0.0;
        for seed in 1..=6u64 {
            let id = RunIdentity::from_seed(seed);
            let d = draw::site(&id, Some(Mode::Aeolian), Some(FormClass::Train));
            let mut r = rng::stream(&id, rng::WIND);
            let f = wind::build(&mut r, d.wind_rad, d.wavelength_m,
                                d.wind_wander_rad, d.wind_wander_m, kap, 0.0);
            let mut hr = rng::stream(&id, rng::HUMMOCK);
            let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m,
                                 d.wind_wander_rad, d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
            let sf = surface::build(&f, &hw, &d);
            gridio::write_grid_f32(
                &out.join(format!("k{:05.0}_{seed}.cgrid", kap * 100.0)), &sf.height).unwrap();
            if seed == 1 { spread = f.spread_rad; }
        }
        println!("kappa {kap:6.2}  spread {spread:.3} rad");
    }
}
