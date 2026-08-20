//! Phase N1 gate: sinuosity band, monotone profile, curvature floor —
//! measured over many seeds, printed as a table.
//!
//! Sinuosity convention matches real_planform.py: 600 m windows over a 10 m
//! resample, p50 reported. Corpus band 1.06–1.10.

use course_draw::{generate, Archetype};
use course_network::build;
use course_seed::RunIdentity;
use course_world::math::Vec2;
use course_world::spline::Spine;

fn window_sinuosity(pts: &[Vec2], win_m: f64) -> Vec<f64> {
    // 10 m resample
    let sp = Spine::new(pts.to_vec());
    let total = sp.length();
    if total < win_m {
        return Vec::new();
    }
    let n = (total / 10.0).floor() as usize;
    let rs: Vec<Vec2> = (0..=n).map(|i| sp.point_at(i as f64 / n as f64)).collect();
    let w = (win_m / 10.0) as usize;
    let mut out = Vec::new();
    for i in 0..rs.len().saturating_sub(w) {
        let arc = 10.0 * w as f64;
        let chord = rs[i].distance(rs[i + w]);
        if chord > 1.0 {
            out.push(arc / chord);
        }
    }
    out
}

fn main() {
    let seeds: Vec<u64> = {
        let v: Vec<u64> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { (1..=20).collect() } else { v }
    };
    println!("N1 gate — corpus 600 m sinuosity band: 1.06–1.10\n");
    println!("{:<14} {:>7} {:>7} {:>7} {:>8} {:>8} {:>9} {:>8} {:>7}",
             "archetype", "sin600", "p10", "p90", "sin1500", "min_rad", "len_km", "through%", "ms");
    for a in Archetype::ALL {
        let (mut sin, mut sin15, mut rad, mut len, mut thr, mut tot): (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, usize, usize) = (vec![], vec![], vec![], vec![], 0, 0);
        let mut ms = 0.0;
        for &s in &seeds {
            let id = RunIdentity::from_seed(s);
            let t = course_template::build(&id, &generate(&id, Some(a)));
            let t0 = std::time::Instant::now();
            let net = build(&id, &t);
            ms += t0.elapsed().as_secs_f64() * 1000.0;
            for tk in &net.trunks {
                sin.extend(window_sinuosity(&tk.pts, 600.0));
                sin15.extend(window_sinuosity(&tk.pts, 1500.0));
                rad.push(Spine::new(tk.pts.clone()).min_curvature_radius());
                len.push(Spine::new(tk.pts.clone()).length() / 1000.0);
                if tk.through { thr += 1; }
                tot += 1;
            }
        }
        if sin.is_empty() {
            println!("{:<14} {:>7}", a.key(), "--");
            continue;
        }
        sin.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let q = |p: f64| sin[((sin.len() - 1) as f64 * p) as usize];
        let mrad = rad.iter().fold(f64::MAX, |m, v| m.min(*v));
        let mlen: f64 = len.iter().sum::<f64>() / len.len() as f64;
        sin15.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let s15 = if sin15.is_empty() { f64::NAN } else { sin15[sin15.len() / 2] };
        println!("{:<14} {:>7.3} {:>7.3} {:>7.3} {:>8.3} {:>8.0} {:>9.2} {:>7.0}% {:>7.1}",
                 a.key(), q(0.5), q(0.1), q(0.9), s15, mrad, mlen,
                 100.0 * thr as f64 / tot.max(1) as f64, ms / seeds.len() as f64);
    }
}
