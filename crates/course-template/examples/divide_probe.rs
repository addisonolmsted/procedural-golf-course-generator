//! Is there a DIVIDE between adjacent trunk mouths?
//!
//! Review concern: two mouths in the same low grow into essentially one trunk
//! running side by side, and may cross. The physical condition for two
//! separate systems is not a distance -- it is a HIGH between them. This
//! measures the prominence of that high.
//!
//! For adjacent mouths at along-positions a < b, sample the relief
//! predisposition on a line 180 m inboard (the same probe placement scores
//! on), take the max between them, and subtract the higher of the two mouth
//! values. Prominence <= 0 means no divide: they share a low.

use course_draw::{generate, Archetype};
use course_seed::RunIdentity;
use course_template::{build, Edge};
use course_world::math::{self, Vec2};

fn main() {
    let probe_in = 180.0;
    let mut prom: Vec<f64> = Vec::new();
    let mut same_low = 0usize;
    for a in Archetype::ALL {
        for seed in 0..400u64 {
            let id = RunIdentity::from_seed(seed);
            let t = build(&id, &generate(&id, Some(a)));
            if t.trunks.len() < 2 { continue; }
            let (ic, is) = (math::cos(t.base_edge.inward()), math::sin(t.base_edge.inward()));
            let at = |along: f64| {
                let m = match t.base_edge {
                    Edge::South => Vec2::new(along, 0.0),
                    Edge::North => Vec2::new(along, course_world::world::EXTENT_M),
                    Edge::West => Vec2::new(0.0, along),
                    Edge::East => Vec2::new(course_world::world::EXTENT_M, along),
                };
                t.fields.relief_pred.bilinear(Vec2::new(m.x + ic * probe_in, m.y + is * probe_in))
            };
            let mut along: Vec<f64> = t.trunks.iter().map(|k| match t.base_edge {
                Edge::South | Edge::North => k.mouth.x,
                _ => k.mouth.y,
            }).collect();
            along.sort_by(|x, y| x.partial_cmp(y).unwrap());
            for w in along.windows(2) {
                let (lo, hi) = (w[0], w[1]);
                let n = ((hi - lo) / 16.0).ceil().max(2.0) as usize;
                let mut peak = f64::MIN;
                for i in 1..n {
                    peak = peak.max(at(lo + (hi - lo) * i as f64 / n as f64));
                }
                let p = peak - at(lo).max(at(hi));
                prom.push(p);
                if p <= 0.0 { same_low += 1; }
            }
        }
    }
    prom.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let q = |p: f64| prom[((prom.len() - 1) as f64 * p) as usize];
    println!("divide prominence between adjacent mouths (relief_pred units, field spans [-1,1])");
    println!("  n={}  p10 {:.3}  p50 {:.3}  p90 {:.3}  max {:.3}", prom.len(), q(0.10), q(0.50), q(0.90), prom[prom.len()-1]);
    println!("  NO DIVIDE (prominence <= 0): {}/{} = {:.1}%", same_low, prom.len(),
             100.0 * same_low as f64 / prom.len() as f64);
    let weak = prom.iter().filter(|v| **v < 0.10).count();
    println!("  weak divide (< 0.10):        {}/{} = {:.1}%", weak, prom.len(),
             100.0 * weak as f64 / prom.len() as f64);
}
