//! Headless viewer smoke: every archetype renders through the course-viz
//! pipeline (terrain + fields + overlays) without panicking.

use std::collections::BTreeMap;

use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_macro::generate_skeleton;

#[test]
fn all_archetypes_render() {
    for arch in ArchetypeId::ALL {
        for seed in [3u64, 9] {
            let ov = SpecOverrides {
                forced_archetype: Some(arch),
                params: BTreeMap::new(),
            };
            let spec =
                CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &ov).unwrap();
            let r = generate_skeleton(&spec, 32.0);
            let s = &r.skeleton;
            let mut img = course_viz::render_height_grid(&s.base_height, 256, None);
            course_viz::overlay_contours(&mut img, &s.base_height, 2.0);
            for sp in &s.structure.spines {
                course_viz::draw_polyline(&mut img, &sp.pts, [255, 0, 0, 255], 0);
            }
            course_viz::draw_core_box(&mut img);
            let _ = course_viz::render_scalar_field(&s.fields.floor_damp, 128, 0.0, 1.0);
            let _ = course_viz::render_direction_field(&s.fields.grain_dir_rad, 128);
        }
    }
}
