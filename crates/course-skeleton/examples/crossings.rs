//! Count geometric segment intersections between different channels.
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;

fn seg_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    // Twice-area orientation with an EPSILON: a child's mouth anchor sits
    // exactly ON its parent's line, and fp sign noise on that degenerate
    // contact is not a crossing (5 m² ≈ a 2 m segment 2.5 m off-line).
    let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (o1, o2, o3, o4) = (o(a, b, c), o(a, b, d), o(c, d, a), o(c, d, b));
    const EPS: f64 = 5.0;
    if o1.abs() < EPS || o2.abs() < EPS || o3.abs() < EPS || o4.abs() < EPS {
        return false;
    }
    (o1 * o2 < 0.0) && (o3 * o4 < 0.0)
}

fn main() {
    let mut total = 0;
    for seed in 760u64..790 {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2::default());
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let mut n = 0;
        for i in 0..sk.channels.len() {
            for j in (i + 1)..sk.channels.len() {
                let (ci, cj) = (&sk.channels[i], &sk.channels[j]);
                // A junction CONTACT is not a crossing: skip segment pairs
                // within 90 m of a parent-child junction point.
                let junction: Option<Vec2> = if cj.parent == Some(i as u32) {
                    Some(cj.pts[0])
                } else if ci.parent == Some(j as u32) {
                    Some(ci.pts[0])
                } else {
                    None
                };
                for wi in ci.pts.windows(2) {
                    for wj in cj.pts.windows(2) {
                        // skip segments sharing a junction neighbourhood
                        let d0 = ((wi[0].x - wj[0].x).powi(2) + (wi[0].y - wj[0].y).powi(2)).sqrt();
                        if d0 < 30.0 {
                            continue;
                        }
                        if let Some(jp) = junction {
                            let dj = ((wi[0].x - jp.x).powi(2) + (wi[0].y - jp.y).powi(2)).sqrt();
                            let dk = ((wj[0].x - jp.x).powi(2) + (wj[0].y - jp.y).powi(2)).sqrt();
                            if dj < 90.0 && dk < 90.0 {
                                continue;
                            }
                        }
                        if seg_cross(wi[0], wi[1], wj[0], wj[1]) {
                            n += 1;
                            if seed == 788 && n <= 6 {
                                println!(
                                    "  788 cross: ch{i}(ord{},par{:?}) x ch{j}(ord{},par{:?}) near ({:.0},{:.0})",
                                    ci.order, ci.parent, cj.order, cj.parent, wi[0].x, wi[0].y
                                );
                            }
                        }
                    }
                }
            }
        }
        if n > 0 {
            println!("seed {seed} ({}): {} crossings", spec.biome.key(), n);
        }
        total += n;
    }
    println!("TOTAL crossings across 30 seeds: {total}");
}
