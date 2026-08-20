//! M2 battery: every network metric the corpus declares a band for, measured
//! on the GRAPH — before any surface exists.
//!
//!   cargo run --release -p course-network --example net_battery [seeds]

use course_draw::{generate, Archetype};
use course_network::{build, CHANNEL_AREA_M2};
use course_seed::RunIdentity;
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

include!("common/metrics.rs");

fn main() {
    let seeds: Vec<u64> = {
        let v: Vec<u64> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { (1..=12).collect() } else { v }
    };
    println!("M2 network battery — measured on the GRAPH, no surface exists");
    println!("corpus: d2c 96-116 | density 2.21-2.60 | Rb 3.92-4.74 | Rl 1.64-2.25 | junc 37-45 | >80 8.5-13.4%\n");
    println!("{:<14} {:>7} {:>8} {:>6} {:>6} {:>4} {:>7} {:>7} {:>7} {:>6}",
             "archetype", "d2c", "density", "Rb", "Rl", "W", "junc", ">80%", "nodes", "ms");
    for a in Archetype::ALL {
        let (mut d, mut den, mut rb, mut rl, mut w, mut j, mut g80, mut nn) =
            (vec![], vec![], vec![], vec![], vec![], vec![], vec![], vec![]);
        let mut ms = 0.0;
        for &s in &seeds {
            let id = RunIdentity::from_seed(s);
            let t = course_template::build(&id, &generate(&id, Some(a)));
            let t0 = std::time::Instant::now();
            let b = build(&id, &t);
            let net = Net { nodes: b.nodes, reaches: b.reaches, omega: b.omega };
            ms += t0.elapsed().as_secs_f64() * 1000.0;
            if net.reaches.is_empty() { continue; }
            d.push(d2c_p50(&net));
            den.push(net.channel_len_m() / 1000.0 / 9.0);
            let (b, l, om) = horton(&net);
            if b.is_finite() { rb.push(b); }
            if l.is_finite() { rl.push(l); }
            w.push(om as f64);
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
            let mut x = v.clone(); x.sort_by(|a, b| a.partial_cmp(b).unwrap()); x[x.len() / 2] };
        println!("{:<14} {:>7.1} {:>8.2} {:>6.2} {:>6.2} {:>4.0} {:>7.1} {:>7.1} {:>7.0} {:>6.1}",
                 a.key(), m(&d), m(&den), m(&rb), m(&rl), m(&w), m(&j), m(&g80), m(&nn),
                 ms / seeds.len() as f64);
    }
}
