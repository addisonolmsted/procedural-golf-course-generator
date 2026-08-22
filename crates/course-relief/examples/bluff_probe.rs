//! How steep are the generated bluffs? Slope stats on the 8 m macro grid,
//! plus the analytic (pre-grid) scarp grade implied by the drawn zones.
use course_draw::Archetype;
use course_seed::RunIdentity;

fn main() {
    println!("corpus real slopes (outlet_probe): pied 11.4/25.8  gp 5.4/12.6  rv 0.9/3.4  hc 18.7/33.0 (p50/p90 %)");
    for a in [Archetype::HillCountry, Archetype::GreatPlains, Archetype::RiverValley, Archetype::Piedmont] {
        let mut steep: Vec<f64> = Vec::new();
        let mut all: Vec<f64> = Vec::new();
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
                    all.push(s);
                    if s > 0.15 {
                        steep.push(s);
                    }
                }
            }
        }
        all.sort_by(|p, q| p.partial_cmp(q).unwrap());
        let qa = |p: f64| all[((all.len() - 1) as f64 * p) as usize];
        println!("{:<14} FULL surface: p50 {:.1}%  p90 {:.1}%", format!("{a}"), qa(0.5) * 100.0, qa(0.9) * 100.0);
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
