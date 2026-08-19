//! Quick d2c median probe over N seeds for every biome.
//!
//! This is the number D5 gates on (`gate_battery` takes the median of
//! `sk.flow_distance` per seed), so it is the one that has to be watched
//! whenever anything changes how much network the extraction mints. The
//! same env overrides as `planform` / `s2_network`, so a ladder can be run
//! against both instruments without a rebuild.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let n: u64 = std::env::var("SEEDS").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    println!(
        "{:14} {:>9} {:>9} {:>9} {:>9} {:>7} {:>7} {:>7}",
        "biome", "d2c p50", "p10", "p90", "chan km", "rb", "rl", "omax"
    );
    for biome in BiomeId::ALL.iter().copied() {
        let mut meds = Vec::new();
        let mut kms = Vec::new();
        // Horton ratios: the corpus reads rb 5.20 / rl 2.14, and the
        // shadow-leaf prune moved both AWAY (4.97 -> 4.76, 2.10 -> 1.97) by
        // taking branch tips with the duplicates. They are the thing this
        // round has to raise, so they ladder here beside d2c.
        let mut rbs = Vec::new();
        let mut rls = Vec::new();
        let mut omax = Vec::new();
        for k in 0..n {
            let id = RunIdentity::from_seed(20_000 + k);
            let mut spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            for (ev, dial) in [
                ("WANDER", "skeleton.route_wander"),
                ("CREEP", "skeleton.creep"),
                ("MEXP", "skeleton.area_exp"),
                ("AREA_TH", "skeleton.area_threshold_m2"),
                ("SLOPE_FLOOR", "skeleton.slope_init_floor"),
        ("PRUNE", "skeleton.shadow_prune_m"),
        ("MEANDER", "skeleton.route_meander"),
                ("ROUGH", "skeleton.roughness_frac"),
                ("SPREAD", "skeleton.cut_spread_m"),
                ("CH_SPREAD", "skeleton.channel_spread"),
                ("BOOST", "skeleton.incision_boost"),
                ("WAVE_ISO", "primitives.wave_iso_frac"),
            ] {
                if let Ok(v) = std::env::var(ev) {
                    if let Ok(x) = v.parse::<f64>() {
                        spec.dials.insert(dial.into(), x);
                    }
                }
            }
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            // RE-DERIVED network, corpus policy on the built surface: this
            // is the one the energy-distance battery vectorizes, and it
            // disagrees with the extraction's own (rb 5.0 vs 3.4 on the same
            // tile against a corpus 5.45).
            let spec8 = sk.flow_distance.spec;
            let (nx8, ny8) = (spec8.nx as usize, spec8.ny as usize);
            let mut h8 = course_world::grid::Grid::filled(spec8, 0.0f64);
            for y in 0..ny8 {
                for x in 0..nx8 {
                    h8.data[y * nx8 + x] = *sk.height.get((x * 4) as u32, (y * 4) as u32);
                }
            }
            let ff = course_world::flow::route(&h8);
            let a_thresh = 6.0e4 / (spec8.cell_size * spec8.cell_size);
            let ch: Vec<bool> = (0..nx8 * ny8)
                .map(|i| {
                    let lake = ff.filled.data[i] > h8.data[i] + 0.01;
                    let a = ff.acc[i] as f64;
                    a >= a_thresh && (!lake || a >= 4.0 * a_thresh)
                })
                .collect();
            let (rb, rl, mo) =
                course_skeleton::fluvial::carve::horton_from_flow(&spec8, &ff.rec, &ch);
            if let Some(v) = rb {
                rbs.push(v);
            }
            if let Some(v) = rl {
                rls.push(v);
            }
            omax.push(mo as f64);
            let mut d: Vec<f64> = sk.flow_distance.data.clone();
            let cell = sk.flow_distance.spec.cell_size;
            let km2 = (sk.flow_distance.spec.nx as f64 * cell / 1000.0).powi(2);
            kms.push(d.iter().filter(|v| **v <= 0.0).count() as f64 * cell / 1000.0 / km2);
            d.sort_by(|a, b| a.total_cmp(b));
            meds.push(d[d.len() / 2]);
        }
        for v in [&mut meds, &mut kms, &mut rbs, &mut rls, &mut omax] {
            v.sort_by(|a, b| a.total_cmp(b));
        }
        let q = |v: &Vec<f64>, p: f64| {
            if v.is_empty() {
                f64::NAN
            } else {
                v[((v.len() - 1) as f64 * p) as usize]
            }
        };
        println!(
            "{:14} {:>9.0} {:>9.0} {:>9.0} {:>9.2} {:>7.2} {:>7.2} {:>7.1}",
            format!("{biome:?}"),
            q(&meds, 0.5),
            q(&meds, 0.1),
            q(&meds, 0.9),
            q(&kms, 0.5),
            q(&rbs, 0.5),
            q(&rls, 0.5),
            q(&omax, 0.5)
        );
    }
}
