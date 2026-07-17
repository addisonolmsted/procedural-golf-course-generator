//! Render every gate preset to output/landform_presets/*.png (hillshade via
//! golf-viz). The headless twin of terrain-lab, for QA review.
//!
//!   cargo run -p golf-landform --example dump_presets --release [res_m]

use golf_landform::{generate, presets};

fn main() {
    let res: f64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(5.0);
    let dir = std::path::Path::new("output/landform_presets");
    std::fs::create_dir_all(dir).unwrap();
    for (name, cfg) in presets() {
        let g = generate(&cfg, res);
        let img = golf_viz::render_height_grid(&g, 900, None);
        let path = dir.join(format!("{name}.png"));
        img.save(&path).unwrap();
        let lo = g.data.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = g.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!("{name}: {}x{} @ {res} m, z [{lo:.1}, {hi:.1}] -> {}",
                 g.spec.nx, g.spec.ny, path.display());
    }
}
