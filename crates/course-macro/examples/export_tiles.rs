//! Export generated base heightfields as campaign tiles so the SAME
//! extractor that measures real DTMs can measure generated terrain.
//!
//! `cargo run -p course-macro --example export_tiles --release -- [out_root] [n_seeds]`
//!
//! Writes `<out_root>/generated/<archetype>/seed<k>.cgrid` — CGRID1 f32 at
//! 2 m, origin (0,0), exactly the shape `macro_campaign` fetches, so
//! `python3 -m macro_campaign compare` reads them with no special casing.

use std::collections::BTreeMap;
use std::path::PathBuf;

use course_macro::generate_skeleton;
use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_world::gridio;

fn main() -> std::io::Result<()> {
    let mut args = std::env::args().skip(1);
    let root = args.next().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/macro_campaign/out")
    });
    let n_seeds: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(10);

    for a in ArchetypeId::ALL {
        let dir = root.join("generated").join(a.key());
        std::fs::create_dir_all(&dir)?;
        for seed in 1..=n_seeds {
            let ov = SpecOverrides {
                forced_archetype: Some(a),
                params: BTreeMap::new(),
            };
            let spec = CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &ov)
                .expect("spec");
            let res = generate_skeleton(&spec, 2.0);
            let path = dir.join(format!("seed{seed}.cgrid"));
            gridio::write_grid_f32(&path, &res.skeleton.base_height)?;
            let (lo, hi) = res
                .skeleton
                .base_height
                .data
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| {
                    (l.min(v), h.max(v))
                });
            println!(
                "  {} seed {seed}: relief {:.1} m  bowls {}  ridges {}  valleys {}",
                a.key(),
                hi - lo,
                res.config.bowls.len(),
                res.config.ridges.len(),
                res.config.valleys.len()
            );
        }
    }
    println!("\nwrote to {}/generated — now: python3 -m macro_campaign compare", root.display());
    Ok(())
}
