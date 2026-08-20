//! Mouth placement probe: distance from each trunk mouth to the nearest
//! corner, over many seeds. The instrument behind MOUTH_CORNER_CLEARANCE_M.

fn main() {
    use course_draw::{generate, Archetype};
    use course_seed::RunIdentity;
    use course_world::world::EXTENT_M;
    let mut d: Vec<f64> = Vec::new();
    for seed in 0..400u64 {
        for a in Archetype::ALL {
            let id = RunIdentity::from_seed(seed);
            let t = course_template::build(&id, &generate(&id, Some(a)));
            for tk in &t.trunks {
                let along = match t.base_edge {
                    course_template::Edge::South | course_template::Edge::North => tk.mouth.x,
                    _ => tk.mouth.y,
                };
                d.push(along.min(EXTENT_M - along));
            }
        }
    }
    d.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| d[((d.len() - 1) as f64 * p) as usize];
    println!("mouth distance to nearest corner, n={}: min {:.0}  p05 {:.0}  p50 {:.0}  max {:.0}",
             d.len(), d[0], q(0.05), q(0.50), d[d.len() - 1]);
    println!("under 450 m: {:.1}%", 100.0 * d.iter().filter(|v| **v < 450.0).count() as f64 / d.len() as f64);
}
