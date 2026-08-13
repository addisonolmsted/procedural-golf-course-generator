//! G-SKELETON D5: the invariant battery at scale. Runs N seeds per biome,
//! measures the full structural family per seed, aggregates per biome, and
//! asserts the gate bands. Output: a markdown table + verdict lines.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_skeleton::fluvial::carve as trunk;
use course_world::math::Vec2;

const SEEDS_PER_BIOME: u64 = 150;

fn q(v: &mut Vec<f64>, p: f64) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    if v.is_empty() { return f64::NAN; }
    v[((v.len() - 1) as f64 * p) as usize]
}

fn main() {
    let mut biome_d2c: Vec<(String, f64)> = Vec::new();
    let mut all_ok = true;
    println!("| biome | d2c p50 (med) [p10–p90 of seeds] | density | rb | rl | Ω≥3 % | junc p50° | >80° % | crossings | conn med | ms med |");
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for biome in BiomeId::ALL {
        let mut d2cs = Vec::new();
        let mut dens = Vec::new();
        let mut rbs = Vec::new();
        let mut rls = Vec::new();
        let mut omega3 = 0u32;
        let mut n_networks = 0u32;
        let mut angles: Vec<f64> = Vec::new();
        let mut crossings = 0u64;
        let mut conns = Vec::new();
        let mut times = Vec::new();
        for k in 0..SEEDS_PER_BIOME {
            let seed = 20_000 + k;
            let id = RunIdentity::from_seed(seed);
            let spec = SiteSpec::generate_builtin(
                id,
                &SpecOverridesV2 { forced_biome: Some(biome) },
            );
            let c1 = course_primitives::generate(&spec, &id);
            let t0 = std::time::Instant::now();
            let sk = course_skeleton::generate(&spec, &c1, &id);
            times.push(t0.elapsed().as_secs_f64() * 1e3);
            let d = &sk.diagnostics;
            conns.push(d.connectivity);
            if sk.channels.is_empty() {
                continue;
            }
            n_networks += 1;
            dens.push(d.achieved_density_km_km2);
            if let (Some(rb), Some(rl)) = (d.bifurcation_ratio, d.length_ratio) {
                rbs.push(rb);
                rls.push(rl);
            }
            if sk.channels.iter().map(|c| c.order).max().unwrap_or(0) >= 3 {
                omega3 += 1;
            }
            let mut fd: Vec<f64> = sk.flow_distance.data.clone();
            d2cs.push(q(&mut fd, 0.5));
            // junction angles
            for c in &sk.channels {
                let Some(p) = c.parent else { continue };
                if c.pts.len() < 3 { continue; }
                // Baseline measured in METRES, not points. A 2-cell tangent
                // on an 8 m D8 grid is quantised to 45° steps, which
                // inflates the apparent T-junction share on any derived
                // network (real corpus: 7–25% at 2 cells, 4–20% at 6 —
                // tools/macro_campaign/junction_real.py). Counting POINTS
                // is a trap here: Chaikin quadruples point density, so a
                // 6-point baseline collapsed to ~12 m and the share went
                // UP to 34%.
                let mut kk = 1usize;
                let mut arc = 0.0;
                while kk < c.pts.len() - 1 && arc < 48.0 {
                    let d = (c.pts[kk].x - c.pts[kk - 1].x, c.pts[kk].y - c.pts[kk - 1].y);
                    arc += (d.0 * d.0 + d.1 * d.1).sqrt();
                    kk += 1;
                }
                let cf = (c.pts[0].x - c.pts[kk].x, c.pts[0].y - c.pts[kk].y);
                let cl = (cf.0 * cf.0 + cf.1 * cf.1).sqrt().max(1e-9);
                let parent = &sk.channels[p as usize];
                let (_, up) = trunk::point_at_arc(&parent.pts, c.junction_arc_m);
                let dot = (cf.0 / cl) * (-up.x) + (cf.1 / cl) * (-up.y);
                angles.push(dot.clamp(-1.0, 1.0).acos().to_degrees());
            }
            // crossings (honest test, sampled every 3rd seed for time)
            if k % 3 == 0 {
                let o = |p: Vec2, qq: Vec2, r: Vec2| {
                    (qq.x - p.x) * (r.y - p.y) - (qq.y - p.y) * (r.x - p.x)
                };
                for i in 0..sk.channels.len() {
                    for j in (i + 1)..sk.channels.len() {
                        let (ci, cj) = (&sk.channels[i], &sk.channels[j]);
                        let junction: Option<Vec2> = if cj.parent == Some(i as u32) {
                            Some(cj.pts[0])
                        } else if ci.parent == Some(j as u32) {
                            Some(ci.pts[0])
                        } else {
                            None
                        };
                        for wi in ci.pts.windows(2) {
                            for wj in cj.pts.windows(2) {
                                if let Some(jp) = junction {
                                    let di = ((wi[0].x - jp.x).powi(2) + (wi[0].y - jp.y).powi(2)).sqrt();
                                    let dj = ((wj[0].x - jp.x).powi(2) + (wj[0].y - jp.y).powi(2)).sqrt();
                                    if di < 20.0 && dj < 20.0 { continue; }
                                }
                                let ln = |p: Vec2, qq: Vec2| ((qq.x - p.x).powi(2) + (qq.y - p.y).powi(2)).sqrt().max(1e-9);
                                let (lab, lcd) = (ln(wi[0], wi[1]), ln(wj[0], wj[1]));
                                let (d1, d2) = (o(wi[0], wi[1], wj[0]) / lab, o(wi[0], wi[1], wj[1]) / lab);
                                let (d3, d4) = (o(wj[0], wj[1], wi[0]) / lcd, o(wj[0], wj[1], wi[1]) / lcd);
                                if d1.abs() > 1.0 && d2.abs() > 1.0 && d3.abs() > 1.0 && d4.abs() > 1.0
                                    && d1 * d2 < 0.0 && d3 * d4 < 0.0 { crossings += 1; }
                            }
                        }
                    }
                }
            }
        }
        // Integrated = drainage actually reaches base level for most
        // seeds. Heathland grows a channel on most seeds but dangles it
        // (connectivity ~0) BY DESIGN — network presence is the wrong
        // criterion and asserted the shared band against a deranged biome.
        let integrated = q(&mut conns.clone(), 0.5) > 0.5;
        let d2c_med = q(&mut d2cs.clone(), 0.5);
        if integrated {
            biome_d2c.push((format!("{biome:?}"), d2c_med));
        }
        let gt80 = 100.0 * angles.iter().filter(|a| **a > 80.0).count() as f64
            / angles.len().max(1) as f64;
        println!(
            "| {:?} | {:.0} [{:.0}–{:.0}] | {:.2} | {:.1} | {:.1} | {:.0}% | {:.0}° | {:.0}% | {} | {:.2} | {:.0} |",
            biome,
            d2c_med,
            q(&mut d2cs.clone(), 0.1),
            q(&mut d2cs.clone(), 0.9),
            q(&mut dens.clone(), 0.5),
            q(&mut rbs.clone(), 0.5),
            q(&mut rls.clone(), 0.5),
            100.0 * omega3 as f64 / n_networks.max(1) as f64,
            q(&mut angles.clone(), 0.5),
            gt80,
            crossings,
            q(&mut conns.clone(), 0.5),
            q(&mut times.clone(), 0.5),
        );
        // gate assertions
        if integrated {
            if !(85.0..=150.0).contains(&d2c_med) {
                println!("GATE FAIL: {biome:?} d2c median {d2c_med:.0} out of band");
                all_ok = false;
            }
            // Band RE-BASED on the corpus: measured with this same
            // extraction, reach definition and baseline, real tiles give a
            // >80° share of 4–20% (piedmont 14, hill_country 15,
            // heathland 20). The old 8% ceiling came from the authored
            // engine, which enforced a 30–62° mouth angle by construction.
            if gt80 > 25.0 {
                println!("GATE FAIL: {biome:?} T-junction share {gt80:.0}%");
                all_ok = false;
            }
        }
        if crossings > 0 {
            println!("GATE FAIL: {biome:?} {crossings} crossings");
            all_ok = false;
        }
    }
    // shared invariant: biome medians must not separate
    let lo = biome_d2c.iter().map(|(_, v)| *v).fold(f64::INFINITY, f64::min);
    let hi = biome_d2c.iter().map(|(_, v)| *v).fold(f64::NEG_INFINITY, f64::max);
    println!("\nintegrated-biome d2c medians spread: {lo:.0}–{hi:.0} m (must be < 30 m apart)");
    if hi - lo >= 30.0 {
        println!("GATE FAIL: biome separation on shared invariant");
        all_ok = false;
    }
    println!("\nD5 VERDICT: {}", if all_ok { "PASS" } else { "FAIL" });
}
