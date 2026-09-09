//! Dump the 8 m dune surface by tier for seeds: the belts alone, the full
//! macro (belts + hummocks, after the repose pass), and the datum.
//!
//!   surface_dump <out_dir> <seed>...
use course_sandhills::{draw, rng, surface, wind, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = RunIdentity::from_seed(seed);
        let d = draw::site(&id, Some(Mode::Aeolian), None);
        let mut wr = rng::stream(&id, rng::WIND);
        let w = wind::build(&mut wr, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                             d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let mut br = rng::stream(&id, rng::PATCHY);
        let sf = surface::build(&mut br, &w, &hw, &d);
        gridio::write_grid_f32(&out.join(format!("s_{seed}.height.cgrid")), &sf.height).unwrap();
        gridio::write_grid_f32(&out.join(format!("s_{seed}.belts.cgrid")), &sf.belts).unwrap();
        gridio::write_grid_f32(&out.join(format!("s_{seed}.datum.cgrid")), &sf.datum).unwrap();
        println!("seed {seed}: form {:?} relief {:.1} hummock {:.1}/{:.0} body_p {:.2} stoss_share {:.2}",
                 d.form, d.dune_relief_m, d.hummock_relief_m, d.hummock_lambda_m, d.body_p, d.stoss_share);
    }
}
