//! Where does the aeolian megaform put its LOWEST ground, and why?
//! Reports distance-from-edge of the argmin of each intermediate field.
use course_sandhills::{draw, rng, surface, wind, Mode};
use course_seed::RunIdentity;

fn argmin_edge(g: &course_world::grid::Grid<f64>) -> f64 {
    let spec = g.spec;
    let mut best = (f64::INFINITY, 0u32, 0u32);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let v = *g.get(x, y);
            if v < best.0 { best = (v, x, y); }
        }
    }
    let (_, x, y) = best;
    let dx = x.min(spec.nx - 1 - x) as f64 * spec.cell_size;
    let dy = y.min(spec.ny - 1 - y) as f64 * spec.cell_size;
    dx.min(dy)
}

fn main() {
    let n: u64 = std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(40);
    let seed0: u64 = std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(600_000);
    let (mut sum_t, mut sum_h, mut cnt) = (0.0f64, 0.0f64, 0u32);
    let (mut edge_t, mut edge_h) = (0u32, 0u32);
    for k in 0..n {
        let id = RunIdentity::from_seed(seed0 + k);
        let d = draw::site(&id, Some(Mode::Aeolian), None);
        let mut wr = rng::stream(&id, rng::WIND);
        let w = wind::build(&mut wr, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                             d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let mut br = rng::stream(&id, rng::PATCHY);
        let sf = surface::build(&mut br, &w, &hw, &d);
        // megaform above datum == dune_relief * t  (monotone in t)
        let spec = sf.belts.spec;
        let mut tg = course_world::grid::Grid::filled(spec, 0.0f64);
        let mut hg = course_world::grid::Grid::filled(spec, 0.0f64);
        for y in 0..spec.ny {
            for x in 0..spec.nx {
                tg.set(x, y, sf.belts.get(x, y) - sf.datum.get(x, y));
                hg.set(x, y, sf.height.get(x, y) - sf.datum.get(x, y));
            }
        }
        let (et, eh) = (argmin_edge(&tg), argmin_edge(&hg));
        sum_t += et; sum_h += eh; cnt += 1;
        if et < 250.0 { edge_t += 1; }
        if eh < 250.0 { edge_h += 1; }
    }
    let c = cnt as f64;
    println!("n={cnt}");
    println!("  megaform (belts-datum) argmin: mean {:.0} m from edge, \
              {:.0}% within 250 m", sum_t / c, edge_t as f64 / c * 100.0);
    println!("  full height-above-datum argmin: mean {:.0} m from edge, \
              {:.0}% within 250 m", sum_h / c, edge_h as f64 / c * 100.0);
    println!("  (uniform expectation: mean ~560 m, 30% within 250 m)");
}
