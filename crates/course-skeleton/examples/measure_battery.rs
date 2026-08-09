//! Measure d2c/density the BATTERY's way: accumulation-threshold channels
//! on the generated base surface (identical to structure.py's policy).
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    for (biome, seeds) in [
        (BiomeId::Piedmont, [41u64, 31, 7]),
        (BiomeId::GreatPlains, [41, 31, 7]),
        (BiomeId::Sandhills, [21, 31, 7]),
        (BiomeId::Heathland, [31, 41, 7]),
        (BiomeId::HillCountry, [41, 31, 7]),
        (BiomeId::RiverValley, [41, 31, 7]),
    ] {
        let mut p50s = Vec::new();
        let mut dens = Vec::new();
        for seed in seeds {
            let id = RunIdentity::from_seed(seed);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            // rebuild height8 from height2 at aligned nodes
            let spec8 = sk.flow_distance.spec;
            let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
            let mut h8 = course_world::grid::Grid::filled(spec8, 0.0f64);
            for y in 0..ny {
                for x in 0..nx {
                    h8.data[y * nx + x] = *sk.height.get((x * 4) as u32, (y * 4) as u32);
                }
            }
            let ff = course_world::flow::route(&h8);
            let cell = spec8.cell_size;
            let a_thresh = 6.0e4 / (cell * cell); // cells
            let lake: Vec<bool> = (0..nx * ny)
                .map(|i| ff.filled.data[i] > h8.data[i] + 0.01)
                .collect();
            let ch: Vec<bool> = (0..nx * ny)
                .map(|i| {
                    let a = ff.acc[i] as f64;
                    a >= a_thresh && (!lake[i] || a >= 4.0 * a_thresh)
                })
                .collect();
            let n_ch = ch.iter().filter(|&&c| c).count();
            dens.push(n_ch as f64 * cell / 1000.0 / 9.0);
            // d2c via chamfer-ish BFS (Dijkstra on the grid, metric)
            let seeds: Vec<(usize, course_skeleton::fluvial::flow_distance::Nearest)> = ch
                .iter()
                .enumerate()
                .filter(|&(_, &c)| c)
                .map(|(lin, _)| {
                    (lin, course_skeleton::fluvial::flow_distance::Nearest {
                        dist_m: 0.0, z_channel: 0.0, implied_channel: 0.0, order: 0,
                    })
                })
                .collect();
            if seeds.is_empty() {
                p50s.push(f64::NAN);
                continue;
            }
            let near = course_skeleton::fluvial::flow_distance::dijkstra(&spec8, &seeds);
            let mut d: Vec<f64> = near.iter().map(|n| n.dist_m).collect();
            d.sort_by(|a, b| a.total_cmp(b));
            p50s.push(d[d.len() / 2]);
        }
        println!(
            "{biome:?}: battery d2c_p50 {:?} m  density {:?}",
            p50s.iter().map(|v| v.round()).collect::<Vec<_>>(),
            dens.iter().map(|v| (v * 100.0).round() / 100.0).collect::<Vec<_>>()
        );
    }
}
