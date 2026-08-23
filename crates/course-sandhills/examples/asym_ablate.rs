//! Ablation: is the crosshatch weave caused by the per-wave ASYMMETRY?
//!
//! Each wave carries the full stoss/lee profile, so eight waves at spread
//! orientations give eight crossing families of slip faces. A real dune field
//! has ONE wind and therefore ONE family. Setting the share to 0.5 makes every
//! wave symmetric; if the weave is slip faces, it should vanish.
use course_sandhills::{draw, record::FormClass, rng, surface, wind, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let out = std::path::Path::new("/tmp/sand_asym");
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).unwrap();
    for seed in [3u64, 19] {
        for (tag, share) in [("asym", None), ("sym", Some(0.5f64))] {
            let id = RunIdentity::from_seed(seed);
            let mut d = draw::site(&id, Some(Mode::Aeolian), Some(FormClass::Train));
            if let Some(s) = share { d.stoss_share = s; }
            let mut r = rng::stream(&id, rng::WIND);
            let f = wind::build(&mut r, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                                d.wind_wander_m, d.kappa, 0.0);
            let mut hr = rng::stream(&id, rng::HUMMOCK);
            let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                                 d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
            let sf = surface::build(&f, &hw, &d);
            gridio::write_grid_f32(&out.join(format!("{tag}_{seed}.cgrid")), &sf.height).unwrap();
        }
    }
    println!("ablation written");
}
