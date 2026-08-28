//! The closing pass: N seeds through the real entry point, timed.
//!
//! Each seed is drawn the way the game draws it -- no forced mode, no forced
//! form -- so the mix of Nebraska/Carolina and train/mound/regular is whatever
//! the coins give. The timer wraps ONLY the generator call, not the grid
//! writes, because the writes are this harness's cost and not the product's.
//!
//!     cargo run --release -p course-sandhills --example final_pass -- <out> <n> [seed0]
use course_sandhills::{
    assemble::HandProfile, build_full, build_fluvial_textured, draw, texture, Mode,
};
use course_seed::RunIdentity;
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let n: u64 = a.get(1).and_then(|s| s.parse().ok()).unwrap_or(100);
    let seed0: u64 = a.get(2).and_then(|s| s.parse().ok()).unwrap_or(500_000);

    let aeo = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_aeolian.bin")).expect("aeolian pack");
    let flu = texture::PatchPack::load(std::path::Path::new(
        "assets/sandhills_patches_fluvial.bin")).expect("fluvial pack");
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");

    println!("seed\tmode\tform\tregular\triver\tw_m\tlake_frac\tms");
    for k in 0..n {
        let seed = seed0 + k;
        let id = RunIdentity::from_seed(seed);
        let d = draw::site(&id, None, None);
        let t0 = std::time::Instant::now();
        let (height, water, lake_frac, river) = if d.mode == Mode::Aeolian {
            let t = build_full(&id, &aeo, None);
            (t.height, t.water, t.lake_frac, t.river.is_some())
        } else {
            let (_, _, _, tex, w, _) = build_fluvial_textured(&id, &prof, &flu);
            let lf = w.lake_frac;
            let has = w.river.is_some();
            (tex, w.surface, lf, has)
        };
        let ms = t0.elapsed().as_secs_f64() * 1000.0;
        let tag = if d.mode == Mode::Aeolian { "aeolian" } else { "fluvial" };
        gridio::write_grid_f32(&out.join(format!("{seed}.cgrid")), &height).unwrap();
        gridio::write_grid_f32(&out.join(format!("{seed}.water.cgrid")), &water).unwrap();
        println!("{seed}\t{tag}\t{:?}\t{}\t{}\t{:.1}\t{:.4}\t{ms:.1}",
                 d.form, d.regular, river, d.river_w_m, lake_frac);
    }
}
