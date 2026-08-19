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
    println!("{:14} {:>9} {:>9} {:>9} {:>9}", "biome", "d2c p50", "p10", "p90", "chan km");
    for biome in BiomeId::ALL.iter().copied() {
        let mut meds = Vec::new();
        let mut kms = Vec::new();
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
            let mut d: Vec<f64> = sk.flow_distance.data.clone();
            let cell = sk.flow_distance.spec.cell_size;
            let km2 = (sk.flow_distance.spec.nx as f64 * cell / 1000.0).powi(2);
            kms.push(d.iter().filter(|v| **v <= 0.0).count() as f64 * cell / 1000.0 / km2);
            d.sort_by(|a, b| a.total_cmp(b));
            meds.push(d[d.len() / 2]);
        }
        meds.sort_by(|a, b| a.total_cmp(b));
        kms.sort_by(|a, b| a.total_cmp(b));
        let q = |v: &Vec<f64>, p: f64| v[((v.len() - 1) as f64 * p) as usize];
        println!(
            "{:14} {:>9.0} {:>9.0} {:>9.0} {:>9.2}",
            format!("{biome:?}"),
            q(&meds, 0.5),
            q(&meds, 0.1),
            q(&meds, 0.9),
            q(&kms, 0.5)
        );
    }
}
