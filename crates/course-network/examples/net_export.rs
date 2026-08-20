//! Render the grown networks for review, including the ones that collapse.
//!
//!   cargo run --release -p course-network --example net_export -- out/m2_viewer 1 2 3 4 5 6

use course_draw::{generate, Archetype};
use course_lab::{sequential, Canvas};
use course_network::{build, CHANNEL_AREA_M2};
use course_seed::RunIdentity;
use course_world::math::Vec2;
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

const PX: u32 = 460;

fn main() {
    let mut a = std::env::args().skip(1);
    let out = a.next().unwrap_or_else(|| "out/m2_viewer".into());
    let seeds: Vec<u64> = {
        let v: Vec<u64> = a.filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { (1..=6).collect() } else { v }
    };
    std::fs::create_dir_all(&out).unwrap();
    let mut rows = Vec::new();

    for arch in Archetype::ALL {
        for &seed in &seeds {
            let id = RunIdentity::from_seed(seed);
            let t = course_template::build(&id, &generate(&id, Some(arch)));
            let net = build(&id, &t);
            let stem = format!("{}_{seed}", arch.key());

            let mut c = Canvas::new(PX, EXTENT_M);
            // dim relief so the network reads on top of it
            c.field(&t.fields.relief_pred, -1.0, 1.0, |v| {
                let s = sequential(v * 0.4 + 0.16);
                [s[0] / 3 + 14, s[1] / 3 + 15, s[2] / 3 + 18]
            });
            c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::MUTED, 0.35);

            // sub-threshold growth: where heads went but never became channel
            for n in &net.nodes {
                if n.area_m2 >= CHANNEL_AREA_M2 { continue; }
                if let Some(p) = n.parent {
                    c.line(net.nodes[p as usize].p, n.p, [110, 118, 130], 0.9, 0.5);
                }
            }
            // channels, width and colour by Strahler order
            let omax = net.omega.max(1);
            for r in &net.reaches {
                let f = r.order as f64 / omax as f64;
                let w = 1.4 + 3.4 * f;
                let col = [
                    (86.0 + 60.0 * (1.0 - f)) as u8,
                    (180.0 - 30.0 * (1.0 - f)) as u8,
                    255u8,
                ];
                c.polyline(&r.pts, col, w, 0.97);
            }
            for tk in &t.trunks {
                c.disc(tk.mouth, 5.5, course_lab::MOUTH, 1.0);
            }
            c.save_jpeg(&format!("{out}/{stem}_net.jpg"), 88).unwrap();

            // territory view: distance to nearest channel, so spacing is visible
            let mut c2 = Canvas::new(PX, EXTENT_M);
            let pts: Vec<Vec2> = net.nodes.iter()
                .filter(|n| n.area_m2 >= CHANNEL_AREA_M2).map(|n| n.p).collect();
            let n = 180usize;
            let mut g = course_world::grid::Grid::filled(
                course_world::grid::GridSpec::new(Vec2::new(0.0, 0.0), EXTENT_M / n as f64,
                    n as u32 + 1, n as u32 + 1), 0.0);
            for gy in 0..=n { for gx in 0..=n {
                let p = Vec2::new(gx as f64 * EXTENT_M / n as f64, gy as f64 * EXTENT_M / n as f64);
                let d = pts.iter().fold(f64::MAX, |m, q| m.min(p.distance(*q)));
                g.set(gx as u32, gy as u32, d.min(600.0));
            }}
            c2.field(&g, 0.0, 600.0, |v| {
                // banded so the 100 m contour is legible
                let s = sequential(1.0 - v);
                if ((v * 6.0).floor() as i32 % 2) == 0 { s } else { [s[0]/1.3 as u8, s[1]/1, s[2]/1] }
            });
            for r in &net.reaches { c2.polyline(&r.pts, [30, 40, 55], 1.6, 0.9); }
            c2.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::INK, 0.35);
            c2.save_jpeg(&format!("{out}/{stem}_terr.jpg"), 88).unwrap();

            let chan: usize = net.channel_nodes().count();
            let dens = net.channel_len_m() / 1000.0 / 9.0;
            rows.push(format!(
                r#"{{"arch":"{}","seed":{seed},"nodes":{},"chan":{chan},"reaches":{},"omega":{},"density":{:.2},"trunks":{},"steps":{},"offtile":{},"claimed":{},"filled":{}}}"#,
                arch.key(), net.nodes.len(), net.reaches.len(), net.omega, dens,
                t.trunks.len(), net.stats.steps, net.stats.died_offtile,
                net.stats.died_claimed, if dens >= 1.5 { "true" } else { "false" }));
        }
    }
    std::fs::write(format!("{out}/stats.json"), format!("[\n{}\n]", rows.join(",\n"))).unwrap();
    println!("wrote {} tiles x 2 views to {out}", rows.len());
}
