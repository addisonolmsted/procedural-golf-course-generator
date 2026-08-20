//! Calibrate branch angle and branch rate against the corpus bands.
//! corpus: Rb 3.92-4.74 | Rl 1.64-2.25 | junc 37-45 | >80% 8.5-13.4 | density 2.21-2.60
use course_draw::{generate, Archetype};
use course_network::{account, grow, GrowParams, CHANNEL_AREA_M2};
use course_seed::RunIdentity;
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

include!("common/metrics.rs");

const CF: f64 = 2.6;
fn main() {
    let seeds: Vec<u64> = (1..=8).collect();
    println!("{:>6} {:>6} | {:>8} {:>6} {:>6} {:>7} {:>7} {:>6}",
             "angle", "rate", "density", "Rb", "Rl", "junc", ">80%", "nodes");
    for angle_deg in [40.0, 52.0] {
        for rate in [4.0, 7.0, 11.0, 16.0] {
            let (mut den, mut rb, mut rl, mut j, mut g80, mut nn) =
                (vec![], vec![], vec![], vec![], vec![], vec![]);
            for &s in &seeds {
                let id = RunIdentity::from_seed(s);
                let t = course_template::build(&id, &generate(&id, Some(Archetype::Piedmont)));
                let seedpts: Vec<(Vec2, f64)> =
                    t.trunks.iter().map(|k| (k.mouth, k.azimuth_rad)).collect();
                if seedpts.is_empty() { continue; }
                let mut rng = course_draw::rng::stream(&id, course_network::NETWORK_GROW);
                let p = GrowParams {
                    claim_m: t.pattern.d2c_target_m * CF,
                    branch_angle_rad: (angle_deg as f64).to_radians(),
                    branch_rate: rate,
                    ..Default::default()
                };
                let (mut nodes, _) =
                    grow::grow(&mut rng, &seedpts, &t.fields.relief_pred, &t.fields.grain_rad, &p);
                account::accumulate(&mut nodes);
                account::strahler(&mut nodes, CHANNEL_AREA_M2);
                let reaches = account::reaches(&nodes, CHANNEL_AREA_M2);
                if reaches.is_empty() { continue; }
                let omega = nodes.iter().map(|n| n.order).max().unwrap_or(0);
                let net = Net { nodes, reaches, omega };
                den.push(net.channel_len_m() / 1000.0 / 9.0);
                let (b, l, _) = horton(&net);
                if b.is_finite() { rb.push(b); }
                if l.is_finite() { rl.push(l); }
                let ang = junctions(&net);
                if !ang.is_empty() {
                    let mut v = ang.clone();
                    v.sort_by(|x, y| x.partial_cmp(y).unwrap());
                    j.push(v[v.len() / 2]);
                    g80.push(100.0 * ang.iter().filter(|x| **x > 80.0).count() as f64 / ang.len() as f64);
                }
                nn.push(net.nodes.len() as f64);
            }
            let m = |v: &Vec<f64>| if v.is_empty() { f64::NAN } else {
                let mut x = v.clone(); x.sort_by(|a, b| a.partial_cmp(b).unwrap()); x[x.len()/2] };
            println!("{angle_deg:>6.0} {rate:>6.1} | {:>8.2} {:>6.2} {:>6.2} {:>7.1} {:>7.1} {:>6.0}",
                     m(&den), m(&rb), m(&rl), m(&j), m(&g80), m(&nn));
        }
    }
}
