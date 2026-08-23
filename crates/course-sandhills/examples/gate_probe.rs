use course_sandhills::{draw, record::FormClass, rng, surface, wind, Mode};
use course_seed::RunIdentity;
fn main() {
    for floor in [0.0f64, 0.5, 0.9] {
        let id = RunIdentity::from_seed(7);
        let mut d = draw::site(&id, Some(Mode::Aeolian), Some(FormClass::Train));
        d.hummock_floor = floor;
        let mut r = rng::stream(&id, rng::WIND);
        let f = wind::build(&mut r, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                             d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let sf = surface::build(&mut rng::stream(&id, rng::PATCHY), &f, &hw, &d);
        let g = &sf.hummock_gate;
        let mut v: Vec<f64> = g.data.clone();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
        // how much does the surface differ from the belts on the low ground?
        let mut lo: Vec<(f64, f64)> = Vec::new();
        for y in 0..g.spec.ny { for x in 0..g.spec.nx {
            lo.push((*sf.belts.get(x,y) - *sf.datum.get(x,y),
                     (*sf.height.get(x,y) - *sf.belts.get(x,y)).abs()));
        }}
        lo.sort_by(|a,b| a.0.partial_cmp(&b.0).unwrap());
        let n = lo.len()/3;
        let hum_lo: f64 = lo[..n].iter().map(|p| p.1).sum::<f64>() / n as f64;
        let hum_hi: f64 = lo[lo.len()-n..].iter().map(|p| p.1).sum::<f64>() / n as f64;
        println!("floor {floor:.1}  gate p10 {:.3} p50 {:.3} p90 {:.3} | |hummock| low third {hum_lo:.3} m, high third {hum_hi:.3} m",
                 q(0.10), q(0.50), q(0.90));
    }
}
