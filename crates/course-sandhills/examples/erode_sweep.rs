//! Sweep erosion coefficients on a fixed assembled surface.
use course_sandhills::{assemble::HandProfile, build_fluvial_macro_opts, erode};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");
    let seeds: Vec<u64> = a[1..].iter().map(|s| s.parse().unwrap()).collect();
    // (label, k, g, kd, steps)
    // (label, k, m, g, kd, sc, steps)
    let sets: Vec<(&str, f64, f64, f64, f64, f64, usize)> = vec![
        ("I1", 1.5e-3, 0.45, 0.05, 0.30, 0.0, 60),
        ("I2", 4.0e-3, 0.45, 0.05, 0.30, 0.0, 60),
        ("I3", 4.0e-3, 0.35, 0.05, 0.60, 0.0, 60),
        ("I4", 1.0e-2, 0.35, 0.03, 0.60, 0.0, 60),
        ("I5", 4.0e-3, 0.45, 0.05, 1.50, 0.0, 60),
        ("I6", 1.0e-2, 0.30, 0.03, 1.50, 0.0, 60),
        ("I7", 2.5e-2, 0.30, 0.03, 1.50, 0.0, 60),
    ];
    for seed in seeds {
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, _, pre, _) = build_fluvial_macro_opts(&id, &prof, None);
        gridio::write_grid_f32(&out.join(format!("pre_{seed}.cgrid")), &pre.height).unwrap();
        for (lab, k, mm, g, kd, sc, steps) in &sets {
            let p = erode::Erosion { k: *k, m: *mm, g: *g, kd: *kd, sc: *sc, dt: 1.0, steps: *steps };
            let mut h = pre.height.clone();
            let t = std::time::Instant::now();
            let st = erode::run(&mut h, &p);
            gridio::write_grid_f32(&out.join(format!("{lab}_{seed}.cgrid")), &h).unwrap();
            println!("seed {seed} set {lab}: {:.0} ms  mean|dz| {:.2}  range {:+.2}..{:+.2}",
                     t.elapsed().as_secs_f64()*1e3, st.mean_abs_dz, st.min_dz, st.max_dz);
        }
    }
}
