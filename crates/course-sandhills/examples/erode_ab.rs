//! A/B the macro surface with and without the water-erosion pass.
use course_sandhills::{assemble::HandProfile, build_fluvial_macro_opts, erode};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");
    let p = erode::Erosion::default();
    println!("params {p:?}");
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, _, pre, _) = build_fluvial_macro_opts(&id, &prof, None);
        gridio::write_grid_f32(&out.join(format!("pre_{seed}.cgrid")), &pre.height).unwrap();
        let t = std::time::Instant::now();
        let (_, _, post, _) = build_fluvial_macro_opts(&id, &prof, Some(p));
        let whole = t.elapsed();
        let mut h = pre.height.clone();
        let t2 = std::time::Instant::now();
        let st = erode::run(&mut h, &p);
        let just = t2.elapsed();
        gridio::write_grid_f32(&out.join(format!("post_{seed}.cgrid")), &post.height).unwrap();
        println!("seed {seed}: erosion {:.0} ms (whole macro {:.0} ms) | mean|dz| {:.2} m \
                  p99 {:.2} m  range {:.2}..{:.2} m  eroded {:.0} m3 deposited {:.0} m3",
                 just.as_secs_f64() * 1e3, whole.as_secs_f64() * 1e3,
                 st.mean_abs_dz, st.p99_abs_dz, st.min_dz, st.max_dz,
                 st.eroded_m3, st.deposited_m3);
    }
}
