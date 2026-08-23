//! Visual check materials: form classes, ladders, and the hummock ablation.
//!
//! Discipline rule 4 — render before trusting metrics. Rule 3 — every claim
//! about a visual defect gets a toggle and a render.
use course_sandhills::{draw, record::FormClass, rng, surface, wind, Descriptors, Mode};
use course_seed::RunIdentity;
use course_world::gridio;

/// Build one tile. `kappa`/`wander` override the draw for a ladder;
/// `hummocks` off is the ablation.
fn tile(seed: u64, form: FormClass, kappa: Option<f64>, wander: Option<f64>, hummocks: bool)
    -> (Descriptors, surface::Surface, f64)
{
    let id = RunIdentity::from_seed(seed);
    let d = draw::site(&id, Some(Mode::Aeolian), Some(form));
    let kap = kappa.unwrap_or(d.kappa);
    let wan = wander.unwrap_or(d.wind_wander_rad);
    let mut r = rng::stream(&id, rng::WIND);
    let f = wind::build(&mut r, d.wind_rad, d.wavelength_m, wan, d.wind_wander_m, kap, 0.0);
    let mut hr = rng::stream(&id, rng::HUMMOCK);
    let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m,
                         wan, d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
    let mut dd = d;
    if !hummocks {
        dd.hummock_relief_m = 0.0;
    }
    let sf = surface::build(&mut rng::stream(&id, rng::PATCHY), &f, &hw, &dd);
    (d, sf, f.spread_rad)
}

fn main() {
    let out = std::path::Path::new("/tmp/sand_visual2");
    let _ = std::fs::remove_dir_all(out);
    std::fs::create_dir_all(out).unwrap();
    let put = |n: String, s: &surface::Surface, d: &Descriptors, sp: f64| {
        gridio::write_grid_f32(&out.join(format!("{n}.cgrid")), &s.height).unwrap();
        std::fs::write(out.join(format!("{n}.meta")),
                       format!("{} {} {} {} {} {}", d.wind_rad, d.wavelength_m, sp,
                               d.kappa, d.hummock_lambda_m, d.hummock_relief_m)).unwrap();
    };

    for (tag, form) in [("train", FormClass::Train), ("mound", FormClass::Mound)] {
        for seed in [3u64, 7, 12, 19] {
            let (d, sf, sp) = tile(seed, form, None, None, true);
            put(format!("{tag}_{seed}"), &sf, &d, sp);
        }
    }
    // THE ablation: same seed, hummock tier on and off.
    for seed in [3u64, 19] {
        let (d, on, sp) = tile(seed, FormClass::Train, None, None, true);
        let (_, off, _) = tile(seed, FormClass::Train, None, None, false);
        put(format!("abl_{seed}_on"), &on, &d, sp);
        put(format!("abl_{seed}_off"), &off, &d, sp);
    }
    // concentration ladder at the new train floor
    for kap in [0.30f64, 1.0, 3.0, 8.0, 14.0, 24.0] {
        let (d, sf, sp) = tile(7, FormClass::Train, Some(kap), None, true);
        put(format!("lad_{:05.0}", kap * 100.0), &sf, &d, sp);
    }
    println!("visual materials -> {}", out.display());
}
