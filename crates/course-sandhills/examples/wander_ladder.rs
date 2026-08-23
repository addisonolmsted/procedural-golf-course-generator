//! Wander ladder — the operating point is chosen against measured numbers,
//! never by eye alone. Sweeps `wind_wander_rad`, the dial that sets the
//! measured orientation order and the crest defect density.
use course_sandhills::{draw, record::FormClass, rng, surface, wind, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let out = std::path::Path::new("/tmp/sand_ladder");
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).unwrap();
    for w in [0.0f64, 0.10, 0.20, 0.30, 0.45, 0.60, 0.80, 1.00, 1.30, 1.70] {
        for seed in 1..=6u64 {
            let id = RunIdentity::from_seed(seed);
            let d = draw::site(&id, Some(Mode::Aeolian), Some(FormClass::Train));
            let mut r = rng::stream(&id, rng::WIND);
            let f = wind::build(&mut r, d.wind_rad, d.wavelength_m, w, d.wind_wander_m);
            let sf = surface::build(&f, &d);
            gridio::write_grid_f32(
                &out.join(format!("w{:04.0}_{seed}.cgrid", w * 1000.0)), &sf.height).unwrap();
            if seed == 1 {
                println!("wander {w:.2}  curl_mag {:.3}", f.curl_mag);
            }
        }
    }
}
