//! Where does the water leave the tile, and how coherent is the trunk?
use course_contracts::biome::BiomeId;
use course_seed::{streams, RunIdentity};
use course_skeleton::fluvial::carve::{self, CarveParams};
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    for biome in BiomeId::ALL.iter() {
        let mut rows = Vec::new();
        for seed in [48u64, 7, 15, 1006, 41000] {
            let id = RunIdentity::from_seed(seed);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let spec8 = c1.grid;
            let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
            let n = nx * ny;
            let mut implied = c1.tilt.clone();
            for (i, v) in implied.data.iter_mut().enumerate() { *v += c1.relief.data[i]; }
            let relief_amp = spec.dials.get("primitives.relief_amp_m").copied().unwrap_or(8.0);
            let density = spec.dials.get("skeleton.density_target").copied().unwrap_or(1.0);
            let erod: Vec<f64> = c1.hardness.data.iter().map(|h| (1.6 - h).clamp(0.3, 1.6)).collect();
            let keep = vec![false; n];
            let p = CarveParams {
                roughness_frac: 0.05, k: if density >= 0.5 { 0.9 } else { 0.0 },
                area_threshold_m2: carve::AREA_THRESHOLD_M2, incision_scale: 1.0,
                base_drop_m: std::env::var("BD").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(6.0) * relief_amp, inflow_area_m2: 2.5e6,
                close_borders: std::env::var("OPEN").is_err(),
                iters: 15, step_clamp_m: 0.45,
            };
            let mut rng = id.stream(streams::SKELETON_MODULE);
            let c = carve::carve(&spec8, &implied, &erod, &keep, c1.meta.base_level.edge, &mut rng, &p, relief_amp);
            // outflow per edge = accumulation of border cells whose rec is -1
            let base = format!("{:?}", c1.meta.base_level.edge);
            let bi = match base.as_str() { "S" => 0, "N" => 1, "W" => 2, _ => 3 };
            let mut per_edge = [0.0f64; 4]; // S,N,W,E
            let mut total = 0.0;
            for lin in 0..n {
                if c.rec[lin] >= 0 { continue; }
                let (y, x) = (lin / nx, lin % nx);
                let a = c.area[lin];
                let on_border = y == 0 || y == ny - 1 || x == 0 || x == nx - 1;
                if !on_border { continue; }
                total += a;
                // corner cells belong to the base edge when they touch it
                // (an earlier version classified them by y first and read
                // base-edge outflow as low when it was leaving at a corner)
                let touches_base = match base.as_str() {
                    "S" => y == 0,
                    "N" => y == ny - 1,
                    "W" => x == 0,
                    _ => x == nx - 1,
                };
                if touches_base { per_edge[bi] += a; }
                else if y == 0 { per_edge[0] += a; }
                else if y == ny - 1 { per_edge[1] += a; }
                else if x == 0 { per_edge[2] += a; }
                else { per_edge[3] += a; }
            }
            let frac_base = if total > 0.0 { per_edge[bi] / total } else { f64::NAN };
            // trunk coherence: largest single outlet's share of total outflow
            let mut mx = 0.0f64;
            for lin in 0..n { if c.rec[lin] < 0 { mx = mx.max(c.area[lin]); } }
            let biggest = if total > 0.0 { mx / total } else { f64::NAN };
            rows.push((seed, base, frac_base, biggest));
        }
        for (seed, base, fb, big) in rows {
            println!("{:14} seed {:5}: base edge {:9} outflow via base {:.0}%  biggest single outlet {:.0}% of outflow",
                biome.key(), seed, base, fb * 100.0, big * 100.0);
        }
    }
}
