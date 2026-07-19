//! Render the noiselab default view to output/noiselab/*.png — the headless
//! twin of terrain-lab's Noise tab, for QA review: flat noise, the composed
//! river_confluence skeleton, and its residual (modulation visible as damped
//! floors / full uplands).
//!
//!   cargo run -p golf-landform --example dump_noise --release [res_m]

use golf_landform::noiselab::{generate_noise, skeleton_fields, NoiseLabConfig};

const EXTENT_M: f64 = 3000.0;

fn main() {
    let res: f64 = std::env::args()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(4.0);
    let cfg = NoiseLabConfig {
        seed: 7,
        base_amp: 10.0,
        base_wavelength: 300.0,
        octaves: 5,
        gain: 0.5,
        lacunarity: 2.0,
        warp_amp: 80.0,
        warp_wavelength: 500.0,
        warp2_amp: 20.0,
        ridged_mix: 0.2,
        redistribution: 1.3,
        tex_amp: 0.4,
        tex_wavelength: 25.0,
        tex_gain: 0.55,
        nugget_amp: 0.0,
        aniso_ratio: 1.5,
        floor_damp: 0.2,
        slope_gain: 0.3,
        grain_align: 0.0,
        dummy: 0.0,
    };
    let dir = std::path::Path::new("output/noiselab");
    std::fs::create_dir_all(dir).unwrap();

    let mut save = |name: &str, g: &golf_core::grid::Grid<f64>| {
        let img = golf_viz::render_height_grid(g, 900, None);
        let path = dir.join(format!("{name}.png"));
        img.save(&path).unwrap();
        let lo = g.data.iter().cloned().fold(f64::INFINITY, f64::min);
        let hi = g.data.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        println!("{name}: {}x{} @ {res} m, z [{lo:.1}, {hi:.1}] -> {}",
                 g.spec.nx, g.spec.ny, path.display());
    };

    save("flat", &generate_noise(&cfg, EXTENT_M, None, res));

    let skel = golf_landform::preset("river_confluence").unwrap();
    let sk = skeleton_fields(&skel, res);
    let composed = generate_noise(&cfg, EXTENT_M, Some(&sk), res);
    save("river_confluence_composed", &composed);

    let mut residual = composed.clone();
    for i in 0..residual.data.len() {
        residual.data[i] -= sk.z.data[i];
    }
    save("river_confluence_residual", &residual);
}
