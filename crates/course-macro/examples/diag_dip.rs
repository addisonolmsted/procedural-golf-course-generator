//! Which valley scours the trunk? Walk the trunk floor, find the deepest
//! composed dip, and attribute it by knocking valleys out one at a time.
use std::collections::BTreeMap;
use course_macro::planner::{lower, plan};
use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_world::world::EXTENT_M;

/// The contract test only evaluates floor stations INSIDE the box. Without
/// the same guard this tool reports dips at points nobody is asserting on —
/// it did exactly that, pointing at y = -44 m and sending one fix after a
/// non-defect.
fn in_box(p: course_world::math::Vec2) -> bool {
    p.x >= 0.0 && p.x <= EXTENT_M && p.y >= 0.0 && p.y <= EXTENT_M
}

fn main() {
    let ov = SpecOverrides { forced_archetype: Some(ArchetypeId::Piedmont), params: BTreeMap::new() };
    let spec = CourseSpec::generate_builtin(RunIdentity::from_seed(0), &ov).unwrap();
    let p = plan(&spec);
    let cfg = lower(&p);
    let r = course_macro::resolve(&cfg);
    let n = r.n_valleys();
    // running-min scan along valley 0, exactly as the contract test does
    let (mut run, mut worst, mut worst_u) = (f64::INFINITY, 0.0f64, 0.0);
    let mut u = 0.01;
    while u <= 0.99 {
        let (q, _f) = r.valley_floor(0, u);
        if in_box(q) {
            let z = r.height_at(q);
            if z - run > worst { worst = z - run; worst_u = u; }
            run = run.min(z);
        }
        u += 0.005;
    }
    println!("worst rise {worst:.2} m at u={worst_u:.3}  ({n} valleys)");
    // the dip is BEFORE worst_u: find the station holding the running min
    let (mut minz, mut min_u) = (f64::INFINITY, 0.0);
    let mut u = 0.01;
    while u <= worst_u {
        let (q, _f) = r.valley_floor(0, u);
        if in_box(q) {
            let z = r.height_at(q);
            if z < minz { minz = z; min_u = u; }
        }
        u += 0.005;
    }
    let (dip_p, trunk_floor) = r.valley_floor(0, min_u);
    println!("dip at u={min_u:.3} {dip_p:?}: composed {minz:.2}, trunk floor {trunk_floor:.2} \
              (scour {:.2} m)", trunk_floor - minz);
    // attribute: which valley's surface is lowest there?
    let mut rows: Vec<(f64, usize)> = (0..n)
        .map(|i| (r.valley_surface_at(i, dip_p), i))
        .collect();
    rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    for (s, i) in rows.iter().take(4) {
        println!("   valley {i:3}: surface {s:.2}");
    }
}
