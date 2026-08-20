//! Our trunk-mouth spacing, for comparison against real outlet spacing.
fn main() {
    use course_draw::{generate, Archetype};
    use course_seed::RunIdentity;
    let mut per: Vec<(String, Vec<f64>, f64, f64)> = Vec::new();
    for a in Archetype::ALL {
        let mut gaps = Vec::new();
        let (mut got, mut want) = (0u32, 0u32);
        for seed in 0..400u64 {
            let id = RunIdentity::from_seed(seed);
            let t = course_template::build(&id, &generate(&id, Some(a)));
            want += generate(&id, Some(a)).d.trunk_count;
            got += t.trunks.len() as u32;
            let mut along: Vec<f64> = t.trunks.iter().map(|k| match t.base_edge {
                course_template::Edge::South | course_template::Edge::North => k.mouth.x,
                _ => k.mouth.y,
            }).collect();
            along.sort_by(|x, y| x.partial_cmp(y).unwrap());
            for w in along.windows(2) { gaps.push(w[1] - w[0]); }
        }
        gaps.sort_by(|x, y| x.partial_cmp(y).unwrap());
        per.push((a.key().into(), gaps, got as f64, want as f64));
    }
    println!("{:<14} {:>5} {:>8} {:>7} {:>7} {:>6} {:>10}", "biome", "n", "gap p10", "p50", "p90", "min", "trunks got");
    println!("REAL major outlets:            67-182  510-784 1546-1958  16-64");
    for (k, g, got, want) in per {
        let fill = if want > 0.0 { 100.0 * got / want } else { 100.0 };
        if g.is_empty() { println!("{k:<14} {:>5}      --      --      --     -- {fill:>9.0}%", 0); continue; }
        let q = |p: f64| g[((g.len() - 1) as f64 * p) as usize];
        println!("{k:<14} {:>5} {:>8.0} {:>7.0} {:>7.0} {:>6.0} {fill:>9.0}%", g.len(), q(0.10), q(0.50), q(0.90), g[0]);
    }
}
