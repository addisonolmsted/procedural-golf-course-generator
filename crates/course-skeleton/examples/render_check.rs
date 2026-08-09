//! Quick visual sanity render of S2 output (not part of the test suite).
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;

fn main() {
    let out = std::path::Path::new("out/s2_check");
    std::fs::create_dir_all(out).unwrap();
    for (biome, seed) in [
        (BiomeId::Piedmont, 41u64),
        (BiomeId::RiverValley, 41),
        (BiomeId::Sandhills, 21),
        (BiomeId::Heathland, 31),
        (BiomeId::HillCountry, 41),
        (BiomeId::GreatPlains, 41),
    ] {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let mut img = course_viz::render_height_grid(&sk.height, 750, None);
        let (w, h) = (img.width(), img.height());
        for c in &sk.channels {
            let color = match c.order {
                1 => [110, 170, 250, 255],
                2 => [60, 130, 245, 255],
                _ => [20, 80, 220, 255],
            };
            for wnd in c.pts.windows(2) {
                let a = course_viz::to_px(Vec2::new(wnd[0].x, wnd[0].y), w, h);
                let b = course_viz::to_px(Vec2::new(wnd[1].x, wnd[1].y), w, h);
                course_viz::draw_line(&mut img, a, b, color, (c.order as i64).min(2));
            }
        }
        for line in &sk.divides {
            for wnd in line.windows(2) {
                let a = course_viz::to_px(wnd[0], w, h);
                let b = course_viz::to_px(wnd[1], w, h);
                course_viz::draw_line(&mut img, a, b, [235, 140, 50, 200], 0);
            }
        }
        let name = format!("{:?}_{seed}.png", biome).to_lowercase();
        img.save(out.join(&name)).unwrap();
        let d = &sk.diagnostics;
        let mut fd: Vec<f64> = sk.flow_distance.data.clone();
        fd.sort_by(|a, b| a.total_cmp(b));
        let p50 = fd[fd.len() / 2];
        println!(
            "{name}: n={} dens {:.2}/{:.2} conn {:.2} d2c_p50 {:.0} m",
            d.channel_count, d.achieved_density_km_km2, d.target_density_km_km2, d.connectivity, p50
        );
    }
}
