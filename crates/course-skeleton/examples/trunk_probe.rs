//! Does a continuous main channel cross the tile? Traces the highest-area
//! channel path from its farthest head to its outlet and reports where the
//! two ends sit relative to the borders.
use course_contracts::biome::BiomeId;
use course_seed::{streams, RunIdentity};
use course_skeleton::fluvial::carve::{self, CarveParams};
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    for biome in [BiomeId::RiverValley, BiomeId::Piedmont] {
        for seed in [48u64, 15, 1006, 41003] {
            let id = RunIdentity::from_seed(seed);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let spec8 = c1.grid;
            let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
            let n = nx * ny;
            let cell = spec8.cell_size;
            let mut implied = c1.tilt.clone();
            for (i, v) in implied.data.iter_mut().enumerate() { *v += c1.relief.data[i]; }
            let relief_amp = spec.dials.get("primitives.relief_amp_m").copied().unwrap_or(8.0);
            let density = spec.dials.get("skeleton.density_target").copied().unwrap_or(1.0);
            let tr = spec.dials.get("skeleton.trunk_river").copied().unwrap_or(0.0);
            let erod: Vec<f64> = c1.hardness.data.iter().map(|h| (1.6 - h).clamp(0.3, 1.6)).collect();
            let keep = vec![false; n];
            let p = CarveParams {
                roughness_frac: 0.05,
                k: if density >= 0.5 { 0.9 } else { 0.0 },
                area_threshold_m2: carve::AREA_THRESHOLD_M2,
                incision_scale: 1.0,
                base_drop_m: 4.0 * relief_amp.min(2.0),
                inflow_area_m2: carve::INFLOW_PER_TRUNK_DIAL_M2 * tr,
                close_borders: true,
                derangement: ((0.5 - spec.dials.get("skeleton.integration").copied().unwrap_or(0.5)) * 1.6).clamp(0.0, 0.9),
                iters: 15,
                step_clamp_m: 0.45,
            tributary_reach: 1.0,
            creep: 0.0,
            route_wander: 0.0,
            cut_spread_m: 0.0,
        channel_spread: 0.30,
        bank_angle_deg: 0.0,
        inflow_start_frac: 0.0,
        area_exp: 0.5,
        slope_init_floor: carve::SLOPE_INIT_CLAMP.0,
        shadow_prune_m: 0.0,
        route_meander: 0.0,
        majors_cap: carve::MAJORS_CAP,
        majors_boost: carve::MAJORS_K_BOOST,
        majors_iters: carve::MAJORS_ITERS,
        uplift_m: 0.0,
            };
            let mut rng = id.stream(streams::SKELETON_MODULE);
            let c = carve::carve(&spec8, &implied, &erod, &keep, c1.meta.base_level.edge, &mut rng, &p, relief_amp);
            // the outlet cell with the largest area
            let mut outlet = (0.0f64, usize::MAX);
            for i in 0..n {
                if c.rec[i] < 0 && c.area[i] > outlet.0 { outlet = (c.area[i], i); }
            }
            // walk upstream always taking the largest-area donor => main stem
            let mut donors: Vec<Vec<u32>> = vec![Vec::new(); n];
            for i in 0..n { let r = c.rec[i]; if r >= 0 { donors[r as usize].push(i as u32); } }
            let mut cur = outlet.1;
            let mut len_m = 0.0;
            let mut steps = 0;
            while steps < n {
                // follow discharge, not reach membership: channel_of is
                // None at every reach boundary, which stopped the walk
                // after one cell and made the trunk look nonexistent
                let best = donors[cur].iter().copied()
                    .filter(|&d| c.area[d as usize] >= carve::AREA_THRESHOLD_M2)
                    .max_by(|&a, &b| c.area[a as usize].total_cmp(&c.area[b as usize]));
                match best { Some(d) => { len_m += cell; cur = d as usize; steps += 1; } None => break }
            }
            let d_border = |lin: usize| -> f64 {
                let (y, x) = ((lin / nx) as f64, (lin % nx) as f64);
                let dx = x.min((nx - 1) as f64 - x);
                let dy = y.min((ny - 1) as f64 - y);
                dx.min(dy) * cell
            };
            let il = c.inlet;
            let (ix, iy) = if il == usize::MAX { (0, 0) } else { (il % nx, il / nx) };
            let mut plen = 0;
            if il != usize::MAX {
                let mut cur = il;
                while c.rec[cur] >= 0 && plen < n { plen += 1; cur = c.rec[cur] as usize; }
            }
            println!("    inlet ({ix},{iy}) rec={} area={:.1}km2 inflow-path {plen} cells",
                c.rec[il.min(n-1)], c.area[il.min(n-1)] / 1e6);
            println!(
                "{:13} seed {:5}: main stem {:5.0} m | head {:4.0} m from a border, mouth {:4.0} m | outlet area {:.1} km2 (tile is 9)",
                biome.key(), seed, len_m, d_border(cur), d_border(outlet.1), outlet.0 / 1e6
            );
        }
    }
}
