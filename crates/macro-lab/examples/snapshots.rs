//! Render one PNG per archetype (terrain + structure overlays) for quick
//! visual review without launching the interactive lab:
//!   cargo run -p macro-lab --example snapshots --release -- <out_dir> [seed]

use std::collections::BTreeMap;

use course_macro::generate_skeleton;
use course_macro::skeleton::SpineKind;
use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "snapshots".to_string());
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(7);
    std::fs::create_dir_all(&out).unwrap();
    for arch in ArchetypeId::ALL {
        let ov = SpecOverrides {
            forced_archetype: Some(arch),
            params: BTreeMap::new(),
        };
        let spec = CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &ov).unwrap();
        let r = generate_skeleton(&spec, 8.0);
        let s = &r.skeleton;
        let mut img = course_viz::render_height_grid(&s.base_height, 900, None);
        course_viz::overlay_contours(&mut img, &s.base_height, 2.0);
        for sp in &s.structure.spines {
            let (c, t) = match sp.kind {
                SpineKind::Drain => ([64, 132, 244, 255], 1),
                SpineKind::RidgeLine => ([235, 140, 50, 255], 0),
                SpineKind::BenchEdge => ([220, 60, 60, 255], 0),
            };
            course_viz::draw_polyline(&mut img, &sp.pts, c, t);
        }
        course_viz::draw_core_box(&mut img);
        let path = format!("{out}/{}_seed{}.png", arch.key(), seed);
        img.save(&path).unwrap();
        println!(
            "{path}  core {:.1}/{:.1} m  valleys {}  ridges {}  scarps {}  basins {}",
            r.plan.budget.relief_final_m,
            r.plan.budget.cap_m,
            r.plan.drainage.valleys.len(),
            r.plan.uplands.ridges.len(),
            r.plan.uplands.scarps.len(),
            r.plan.basins.bowls.len()
        );
    }
}
