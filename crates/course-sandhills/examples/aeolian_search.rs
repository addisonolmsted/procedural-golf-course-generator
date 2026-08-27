//! Search seeds for the aeolian macro window closest to a target signature.
//! Mirrors fluvial_search, on the A0-A6a macro path (8 m) minus texture.
use course_sandhills::{blowout, draw, rng, surface, water, wind, Mode};
use course_world::grid::Grid;
use std::io::Write;

const WIN: usize = 174;
const STEP: usize = 22;

fn pct(v: &mut Vec<f64>, q: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * q).round() as usize]
}

fn features(z: &[f64], nx: usize, oy: usize, ox: usize) -> [f64; 6] {
    let mut zs: Vec<f64> = Vec::with_capacity(WIN * WIN);
    let mut ss: Vec<f64> = Vec::with_capacity(WIN * WIN);
    let (mut lo, mut hi, mut sum) = (f64::MAX, f64::MIN, 0.0);
    let (mut n5, mut n12, mut n) = (0usize, 0usize, 0usize);
    for y in 0..WIN {
        for x in 0..WIN {
            let i = (oy + y) * nx + ox + x;
            let v = z[i];
            zs.push(v);
            lo = lo.min(v);
            hi = hi.max(v);
            sum += v;
            if y > 0 && y + 1 < WIN && x > 0 && x + 1 < WIN {
                let dzdx = (z[i + 1] - z[i - 1]) / 16.0;
                let dzdy = (z[i + nx] - z[i - nx]) / 16.0;
                let s = (dzdx * dzdx + dzdy * dzdy).sqrt() * 100.0;
                ss.push(s);
                if s < 5.0 { n5 += 1; }
                if s > 12.0 { n12 += 1; }
                n += 1;
            }
        }
    }
    let nf = n.max(1) as f64;
    [pct(&mut zs, 0.98) - pct(&mut zs, 0.02),
     pct(&mut ss, 0.50), pct(&mut ss, 0.90),
     n5 as f64 / nf, n12 as f64 / nf,
     (sum / (WIN * WIN) as f64 - lo) / (hi - lo).max(1e-9)]
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    let (lo, hi): (u64, u64) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    let tgt: Vec<f64> = a[3..].iter().flat_map(|s| s.split_whitespace())
        .filter_map(|v| v.parse().ok()).collect();
    assert_eq!(tgt.len(), 6);
    let sc = [12.0, 2.5, 5.0, 0.18, 0.09, 0.06];
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    for seed in lo..hi {
        let id = course_seed::RunIdentity::from_seed(seed);
        let d = draw::site(&id, Some(Mode::Aeolian), None);
        let mut wr = rng::stream(&id, rng::WIND);
        let w = wind::build(&mut wr, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                             d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let mut br = rng::stream(&id, rng::PATCHY);
        let mut sf = surface::build(&mut br, &w, &hw, &d);
        let spec8 = sf.height.spec;
        let tnorm8 = {
            let mut sorted: Vec<f64> = sf.height.data.clone();
            sorted.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let n = sorted.len();
            let mut g = Grid::filled(spec8, 0.0f64);
            for i in 0..n {
                g.data[i] = sorted.partition_point(|v| *v < sf.height.data[i]) as f64 / n as f64;
            }
            g
        };
        let mut pr = rng::stream(&id, rng::BLOWOUT);
        let _ = blowout::pans(&mut pr, &mut sf.height, &tnorm8.data, &d);
        let mut rr = rng::stream(&id, rng::WATER);
        if d.allogenic_river {
            let pick = water::RiverStyle::PASSED[rr.below(water::RiverStyle::PASSED.len())];
            if let Some(pl) = water::plan_river(&mut rr, &sf.height, &d, pick) {
                water::carve_corridor(&mut sf.height, &pl);
            }
        }
        let nx = sf.height.spec.nx as usize;
        let ny = sf.height.spec.ny as usize;
        let z = &sf.height.data;
        let mut best = (f64::MAX, [0.0f64; 6]);
        let mut oy = 0;
        while oy + WIN <= ny {
            let mut ox = 0;
            while ox + WIN <= nx {
                let ft = features(z, nx, oy, ox);
                let dd: f64 = (0..6).map(|k| { let e = (ft[k] - tgt[k]) / sc[k]; e * e })
                    .sum::<f64>().sqrt();
                if dd < best.0 { best = (dd, ft); }
                ox += STEP;
            }
            oy += STEP;
        }
        writeln!(f, "{seed}\t{:.4}\t{:.3}\t{:.3}\t{:.3}\t{:.4}\t{:.4}\t{:.4}",
                 best.0, best.1[0], best.1[1], best.1[2], best.1[3], best.1[4], best.1[5]).unwrap();
    }
}
