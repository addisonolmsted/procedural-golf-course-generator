//! Count geometric segment intersections between different channels.
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;

fn seg_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    // Proper crossing with endpoint clearances measured in METRES (the
    // orientation values are normalized by segment length): both segments
    // must straddle each other's line by > 1 m at every endpoint. An
    // area-based epsilon silently exempted every short Chaikin
    // sub-segment — which is how a previous version of this counter
    // reported zero while the reviewer was looking at real crossings.
    let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let len = |p: Vec2, q: Vec2| ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt().max(1e-9);
    let (lab, lcd) = (len(a, b), len(c, d));
    let (d1, d2) = (o(a, b, c) / lab, o(a, b, d) / lab);
    let (d3, d4) = (o(c, d, a) / lcd, o(c, d, b) / lcd);
    if d1.abs() < 1.0 || d2.abs() < 1.0 || d3.abs() < 1.0 || d4.abs() < 1.0 {
        return false;
    }
    (d1 * d2 < 0.0) && (d3 * d4 < 0.0)
}

fn main() {
    let mut total = 0;
    for seed in 700u64..800 {
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
                            if dj < 20.0 && dk < 20.0 {
                                continue;
                            }
                        }
                        if seg_cross(wi[0], wi[1], wj[0], wj[1]) {
                            n += 1;
                            if n <= 6 {
                                println!(
                                    "  {seed} cross: ch{i}(ord{},par{:?}) x ch{j}(ord{},par{:?}) near ({:.0},{:.0})",
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
    println!("TOTAL crossings across 100 seeds: {total}");
}
