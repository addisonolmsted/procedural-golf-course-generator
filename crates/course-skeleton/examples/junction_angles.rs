//! Measure convergence angles: for every channel with a parent, the angle
//! between the child's flow direction at its mouth and the parent's
//! downstream tangent at the junction. 0° = perfectly tangential merge,
//! 90° = T-junction. Real tributary junctions cluster ~30–70°.
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_skeleton::fluvial::carve as trunk;

fn main() {
    let mut all: Vec<f64> = Vec::new();
    for seed in [36u64, 1001, 1004, 1007, 1010, 1013, 1016, 1019] {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2::default());
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let mut angles = Vec::new();
        for c in &sk.channels {
            let Some(p) = c.parent else { continue };
            if c.pts.len() < 3 {
                continue;
            }
            // child flow at mouth: toward pts[0], measured over ~2 segments
            let k = 2.min(c.pts.len() - 1);
            let cf = (
                c.pts[0].x - c.pts[k].x,
                c.pts[0].y - c.pts[k].y,
            );
            let cl = (cf.0 * cf.0 + cf.1 * cf.1).sqrt().max(1e-9);
            let parent = &sk.channels[p as usize];
            let (_, up_tan) = trunk::point_at_arc(&parent.pts, c.junction_arc_m);
            let down = (-up_tan.x, -up_tan.y);
            let dot = (cf.0 / cl) * down.0 + (cf.1 / cl) * down.1;
            angles.push(dot.clamp(-1.0, 1.0).acos().to_degrees());
        }
        angles.sort_by(|a, b| a.total_cmp(b));
        if !angles.is_empty() {
            let q = |p: f64| angles[((angles.len() - 1) as f64 * p) as usize];
            println!(
                "seed {seed:5} ({}): n={:3}  p25 {:3.0}°  p50 {:3.0}°  p75 {:3.0}°  >80°: {}",
                spec.biome.key(),
                angles.len(),
                q(0.25),
                q(0.5),
                q(0.75),
                angles.iter().filter(|a| **a > 80.0).count()
            );
        }
        all.extend(angles);
    }
    all.sort_by(|a, b| a.total_cmp(b));
    let q = |p: f64| all[((all.len() - 1) as f64 * p) as usize];
    println!(
        "\nALL n={}: p10 {:.0}° p25 {:.0}° p50 {:.0}° p75 {:.0}° p90 {:.0}°  share >80° {:.0}%",
        all.len(), q(0.1), q(0.25), q(0.5), q(0.75), q(0.9),
        100.0 * all.iter().filter(|a| **a > 80.0).count() as f64 / all.len() as f64
    );
}
