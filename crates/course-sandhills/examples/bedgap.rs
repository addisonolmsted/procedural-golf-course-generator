//! Measure `ground - creek_z` on the CURRENT aeolian creek, to set CUT_CAP.
use course_sandhills::{draw, gorge, rng, surface, wind, Mode};
use course_seed::RunIdentity;
use course_world::grid::Grid;

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(40);
    let seed0: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(600_000);
    let mut gaps: Vec<f64> = Vec::new();
    let mut bins: Vec<Vec<f64>> = (0..10).map(|_| Vec::new()).collect();
    let mut tiles = 0u32;
    let mut s = seed0;
    while tiles < n as u32 && s < seed0 + 4000 {
        let id = RunIdentity::from_seed(s);
        s += 1;
        let d = draw::site(&id, Some(Mode::Aeolian), None);
        if !d.allogenic_river { continue; }
        let mut wr = rng::stream(&id, rng::WIND);
        let w = wind::build(&mut wr, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                             d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let mut br = rng::stream(&id, rng::PATCHY);
        let sf = surface::build(&mut br, &w, &hw, &d);
        let mut height: Grid<f64> = sf.height.clone();
        let mut rr = rng::stream(&id, rng::WATER);
        if let Some(g) = gorge::build(&mut rr, &mut height, &d) {
            let n = g.creek.len();
            for (i, p) in g.creek.iter().enumerate() {
                let gr = height.bilinear(*p);
                let gap = gr - g.creek_z[i.min(g.creek_z.len() - 1)];
                gaps.push(gap);
                let f = i as f64 / (n.max(2) - 1) as f64;
                let b = ((f * 10.0) as usize).min(9);
                bins[b].push(gap);
            }
            tiles += 1;
        }
    }
    gaps.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| gaps[((gaps.len() - 1) as f64 * p) as usize];
    println!("{tiles} river tiles, {} creek nodes", gaps.len());
    println!("ground - creek_z:  p50 {:.2}  p90 {:.2}  p95 {:.2}  p99 {:.2}  max {:.2} m",
             q(0.50), q(0.90), q(0.95), q(0.99), gaps[gaps.len() - 1]);
    println!("gap by position along creek (head -> mouth), median / p95:");
    for (k, b) in bins.iter_mut().enumerate() {
        b.sort_by(|a, c| a.partial_cmp(c).unwrap());
        if b.is_empty() { continue; }
        println!("  {:>3}%  med {:6.2}  p95 {:6.2} m", k * 10,
                 b[b.len() / 2], b[(b.len() as f64 * 0.95) as usize]);
    }
}
