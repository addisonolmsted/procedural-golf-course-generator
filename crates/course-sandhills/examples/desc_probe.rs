//! Print the descriptors that decide a tile's orientation.
use course_sandhills::{draw, Mode};

fn main() {
    for s in std::env::args().skip(1) {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let d = draw::site(&id, Some(Mode::Aeolian), None);
        let crest = (d.wind_rad + std::f64::consts::FRAC_PI_2).to_degrees().rem_euclid(180.0);
        println!("seed {seed}: wind {:.1} deg  crest axis {:.1} deg  tilt {:.1} deg  river {}",
                 d.wind_rad.to_degrees().rem_euclid(360.0), crest,
                 d.floor_tilt_rad.to_degrees().rem_euclid(360.0), d.allogenic_river);
    }
}
