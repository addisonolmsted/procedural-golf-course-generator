//! Dump the carved surface + drained-area raster as raw f32 (375x375, 8 m)
//! so the SAME Python analysis can run on generated and real tiles.
use course_contracts::biome::BiomeId;
use course_seed::{streams, RunIdentity};
use course_skeleton::fluvial::carve::{self, CarveParams};
use course_spec::v2::{SiteSpec, SpecOverridesV2};

fn main() {
    let dir = std::env::args().nth(1).expect("out dir");
    std::fs::create_dir_all(&dir).unwrap();
    for biome in BiomeId::ALL.iter() {
        for seed in [48u64, 7, 15, 765, 788, 1006, 1008, 1012, 41000, 41003] {
            let id = RunIdentity::from_seed(seed);
            let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let spec8 = c1.grid;
            let n = (spec8.nx * spec8.ny) as usize;
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
                base_drop_m: 4.0,
                inflow_area_m2: carve::INFLOW_PER_TRUNK_DIAL_M2 * tr,
                close_borders: true,
                derangement: ((0.5 - spec.dials.get("skeleton.integration").copied().unwrap_or(0.5)) * 1.6).clamp(0.0, 0.9),
                iters: 15,
                step_clamp_m: 0.45,
            tributary_reach: 1.0,
            creep: 0.0,
            route_wander: 0.0,
            cut_spread_m: 0.0,
        area_exp: 0.5,
        uplift_m: 0.0,
            };
            let mut rng = id.stream(streams::SKELETON_MODULE);
            let c = carve::carve(&spec8, &implied, &erod, &keep, c1.meta.base_level.edge, &mut rng, &p, relief_amp);
            let mut zb = Vec::with_capacity(n * 4);
            let mut ab = Vec::with_capacity(n * 4);
            for i in 0..n {
                zb.extend_from_slice(&(c.z.data[i] as f32).to_le_bytes());
                ab.extend_from_slice(&(c.area[i] as f32).to_le_bytes());
            }
            std::fs::write(format!("{dir}/{}_{seed}.z.f32", biome.key()), &zb).unwrap();
            std::fs::write(format!("{dir}/{}_{seed}.a.f32", biome.key()), &ab).unwrap();
        }
    }
    eprintln!("dumped to {dir}");
}
