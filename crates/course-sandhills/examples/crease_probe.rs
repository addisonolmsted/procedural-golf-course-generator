//! Per-seed draw parameters for the visual-check tiles, so a visual defect can
//! be attributed to a dial rather than guessed at.
use course_sandhills::{draw, record::FormClass, rng, wind, Mode};
use course_seed::RunIdentity;

fn main() {
    println!("{:>6} {:>6} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7}",
             "seed", "form", "kappa", "spread", "wander", "wnd_m", "lambda", "relief", "share");
    for (tag, form) in [("train", FormClass::Train), ("mound", FormClass::Mound)] {
        for seed in [3u64, 7, 12, 19] {
            let id = RunIdentity::from_seed(seed);
            let d = draw::site(&id, Some(Mode::Aeolian), Some(form));
            let mut r = rng::stream(&id, rng::WIND);
            let f = wind::build(&mut r, d.wind_rad, d.wavelength_m,
                                d.wind_wander_rad, d.wind_wander_m, d.kappa);
            let mut hr = rng::stream(&id, rng::HUMMOCK);
            let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m,
                                 d.wind_wander_rad, d.wind_wander_m * 0.45, d.hummock_kappa);
            println!("{seed:>6} {tag:>6} {:>7.2} {:>7.3} {:>7.2} {:>7.0} {:>7.0} {:>7.1} {:>7.2}",
                     d.kappa, f.spread_rad, d.wind_wander_rad, d.wind_wander_m,
                     d.wavelength_m, d.dune_relief_m, d.stoss_share);
        }
    }
}
