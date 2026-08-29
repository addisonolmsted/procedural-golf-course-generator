//! Which seeds carry running water? Cheap: descriptors + one RNG draw, no build.
//!
//! Aeolian tiles get a channel when `allogenic_river` is drawn. Carolina tiles
//! get the meander creek when the FIRST draw off the WATER stream falls under
//! `p_valley_creek` -- which is exactly what `water::fluvial` does, so this
//! reproduces the coin without generating a tile.
use course_sandhills::{draw, rng, Mode};
use course_seed::RunIdentity;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let want: &str = &a[0];                        // "aeolian" | "fluvial"
    let n: usize = a[1].parse().unwrap();
    let seed0: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(600_000);

    let mut found = 0usize;
    let mut s = seed0;
    while found < n && s < seed0 + 400_000 {
        let id = RunIdentity::from_seed(s);
        let d = draw::site(&id, None, None);
        let hit = match (want, d.mode) {
            ("aeolian", Mode::Aeolian) => d.allogenic_river,
            ("fluvial", Mode::Fluvial) => {
                let mut w = rng::stream(&id, rng::WATER);
                w.next_f64() < d.p_valley_creek
            }
            _ => false,
        };
        if hit {
            println!("{s}");
            found += 1;
        }
        s += 1;
    }
    eprintln!("{want}: {found} seeds in {} scanned", s - seed0);
}
