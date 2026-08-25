//! Dump the assembler's intermediate fields to diagnose surface artifacts.
use course_sandhills::{assemble::HandProfile, build_fluvial_macro};
use course_world::gridio;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    std::fs::create_dir_all(out).unwrap();
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).unwrap();
    for s in &a[1..] {
        let seed: u64 = s.parse().unwrap();
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, _, asm) = build_fluvial_macro(&id, &prof);
        for (tag, g) in [("h", &asm.height), ("u", &asm.u), ("w", &asm.w),
                         ("bed", &asm.bed), ("dist", &asm.dist)] {
            gridio::write_grid_f32(&out.join(format!("p{seed}_{tag}.cgrid")), g).unwrap();
        }
        println!("seed {seed} fields dumped");
    }
}
