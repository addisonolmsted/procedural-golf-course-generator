//! Dump generated Sandhills tiles as CGRID1, for the Python ruler.
//!
//!   cargo run --release -p course-sandhills --example sand_dump -- out/dump 1 2 3
//!
//! CGRID1 because that is the interchange format `tools/metrics` and
//! `macro_campaign/cgrid.py` read -- the same bytes a 3DEP tile arrives in, so
//! real and generated go through one measurement path.

use course_sandhills::{draw, record::FormClass, rng, surface, wind, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 2 {
        eprintln!("usage: sand_dump <out_dir> <seed>...");
        std::process::exit(2);
    }
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();

    println!("{:>6} {:>6} {:>8} {:>8} {:>7} {:>7} {:>7} {:>7}",
             "seed", "form", "lambda", "relief", "kappa", "spread", "share", "lee");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = RunIdentity::from_seed(seed);
        for (tag, form) in [("train", FormClass::Train), ("mound", FormClass::Mound)] {
            let d = draw::site(&id, Some(Mode::Aeolian), Some(form));
            let mut r = rng::stream(&id, rng::WIND);
            let w = wind::build(&mut r, d.wind_rad, d.wavelength_m,
                                d.wind_wander_rad, d.wind_wander_m, d.kappa);
            let mut hr = rng::stream(&id, rng::HUMMOCK);
            let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m,
                                 d.wind_wander_rad, d.wind_wander_m * 0.45, d.hummock_kappa);
            let sf = surface::build(&w, &hw, &d);
            let (stoss, lee) = surface::derived_angles(&d);
            gridio::write_grid_f32(&out.join(format!("{tag}_{seed}.cgrid")), &sf.height).unwrap();
            let _ = stoss;
            println!("{seed:>6} {tag:>6} {:>8.0} {:>8.1} {:>7.2} {:>7.2} {:>7.2} {:>7.2}",
                     d.wavelength_m, surface::relief(&sf.height), d.kappa,
                     w.spread_rad, d.stoss_share, lee);
        }
    }
    println!("\nwrote {} tiles to {}", (a.len() - 1) * 2, out.display());
}
