//! How steep are the generated bluffs? Slope stats on the 8 m macro grid,
//! plus the analytic (pre-grid) scarp grade implied by the drawn zones.
use course_draw::Archetype;
use course_seed::RunIdentity;

fn main() {
    for a in [Archetype::HillCountry, Archetype::GreatPlains, Archetype::RiverValley] {
        let mut steep: Vec<f64> = Vec::new();
        for seed in [11u64, 23, 37, 58, 71, 90] {
            let id = RunIdentity::from_seed(seed);
            let (_, _, ms, _) = course_relief::build_macro(&id, Some(a));
            let g = &ms.height;
            let n = g.spec.nx as usize;
            for y in 1..n - 1 {
                for x in 1..n - 1 {
                    let dx = (g.get(x as u32 + 1, y as u32) - g.get(x as u32 - 1, y as u32)) / 16.0;
                    let dy = (g.get(x as u32, y as u32 + 1) - g.get(x as u32, y as u32 - 1)) / 16.0;
                    let s = (dx * dx + dy * dy).sqrt();
                    if s > 0.15 {
                        steep.push(s);
                    }
                }
            }
        }
        steep.sort_by(|p, q| p.partial_cmp(q).unwrap());
        let q = |p: f64| if steep.is_empty() { 0.0 } else { steep[((steep.len() - 1) as f64 * p) as usize] };
        let deg = |s: f64| s.atan().to_degrees();
        println!(
            "{:<14} steep cells {:>6}  p50 {:>4.0}% ({:>2.0}°)  p90 {:>4.0}% ({:>2.0}°)  p99 {:>4.0}% ({:>2.0}°)  max {:>4.0}% ({:>2.0}°)",
            format!("{a}"), steep.len(),
            q(0.5) * 100.0, deg(q(0.5)), q(0.9) * 100.0, deg(q(0.9)),
            q(0.99) * 100.0, deg(q(0.99)), q(1.0) * 100.0, deg(q(1.0)),
        );
    }
}
