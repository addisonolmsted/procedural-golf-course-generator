//! T1 renders: the macro surface, hillshaded — the first terrain of attempt 4.
//!
//!   cargo run --release -p course-relief --example t1_export -- out/t1_viewer 1 2 3 4 5 6

use course_draw::Archetype;
use course_lab::Canvas;
use course_relief::build_macro;
use course_seed::RunIdentity;
use course_world::world::EXTENT_M;

const PX: u32 = 460;

fn main() {
    let mut a = std::env::args().skip(1);
    let out = a.next().unwrap_or_else(|| "out/t1_viewer".into());
    let seeds: Vec<u64> = {
        let v: Vec<u64> = a.filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { (1..=6).collect() } else { v }
    };
    std::fs::create_dir_all(&out).unwrap();
    let mut rows = Vec::new();

    for arch in Archetype::ALL {
        if arch == Archetype::Sandhills {
            continue; // aeolian generator is T4; no fluvial macro yet
        }
        for &seed in &seeds {
            let id = RunIdentity::from_seed(seed);
            let t0 = std::time::Instant::now();
            let (_t, trunks, ms, d) = build_macro(&id, Some(arch));
            let ms_t = t0.elapsed().as_secs_f64() * 1000.0;

            let (mut lo, mut hi) = (f64::MAX, f64::MIN);
            for v in &ms.height.data {
                lo = lo.min(*v);
                hi = hi.max(*v);
            }
            let mut c = Canvas::new(PX, EXTENT_M);
            course_lab::render_terrain(&mut c, &ms.height, lo, hi);
            // trunk beds as a thin line so the valley alignment is checkable
            for tk in &trunks {
                c.polyline(&tk.pts, [40, 90, 200], 1.2, 0.65);
            }
            c.save_jpeg(&format!("{out}/{}_{seed}_terrain.jpg", arch.key()), 88).unwrap();

            // golf slope readout (in-loop; the Python proxy stays the gate)
            let (mut cap, mut steep, mut n) = (0u32, 0u32, 0u32);
            let g = &ms.height;
            let nn = g.spec.nx as usize;
            for y in 1..nn - 1 {
                for x in 1..nn - 1 {
                    let dzdx = (g.get(x as u32 + 1, y as u32) - g.get(x as u32 - 1, y as u32)) / (2.0 * 8.0);
                    let dzdy = (g.get(x as u32, y as u32 + 1) - g.get(x as u32, y as u32 - 1)) / (2.0 * 8.0);
                    let sl = (dzdx * dzdx + dzdy * dzdy).sqrt();
                    if sl <= 0.08 { cap += 1; }
                    if sl <= 0.15 { steep += 1; }
                    n += 1;
                }
            }
            rows.push(format!(
                r#"{{"arch":"{}","seed":{seed},"relief":{:.1},"cap":{:.3},"steep":{:.3},"terr_steps":{},"ms":{ms_t:.0}}}"#,
                arch.key(), hi - lo, cap as f64 / n as f64, steep as f64 / n as f64, d.terrace_steps));
            println!("{:<14} {seed}  relief {:6.1} m  under8% {:5.3}  under15% {:5.3}  {:4.0} ms",
                     arch.key(), hi - lo, cap as f64 / n as f64, steep as f64 / n as f64, ms_t);
        }
    }
    std::fs::write(format!("{out}/stats.json"), format!("[\n{}\n]", rows.join(",\n"))).unwrap();
}
