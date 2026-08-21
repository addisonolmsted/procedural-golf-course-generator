//! Render N2: trunks + tier-2 tributaries over the relief field.
//!
//!   cargo run --release -p course-network --example n2_export -- out/n2_viewer 1 2 3 4 5 6

use course_draw::{generate, Archetype};
use course_lab::{diverging, Canvas};
use course_network::build;
use course_seed::RunIdentity;
use course_world::spline::Spine;
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

const PX: u32 = 460;

fn main() {
    let mut a = std::env::args().skip(1);
    let out = a.next().unwrap_or_else(|| "out/n2_viewer".into());
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
            c.field(&t.fields.relief_pred, -1.0, 1.0, diverging);
            c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::INK, 0.4);
            let zmax = net.trunks.iter().map(|k| *k.z.last().unwrap_or(&1.0)).fold(1e-6, f64::max);
            for tk in &net.trunks {
                let w = 2.4 + 1.1 * tk.external_km2.sqrt().min(9.0);
                for i in 0..tk.pts.len() - 1 {
                    // Elevation colour: near-white (base level) -> deep violet
                    // (upstream). Violet, NOT blue: the terrain field already
                    // uses blue for LOW, and the first render coloured the
                    // line's HIGH end blue — one hue meaning opposite things
                    // in one image, which read as a river splitting at low
                    // ground (user report).
                    let f = (tk.z[i] / zmax).clamp(0.0, 1.0);
                    let col = [
                        (242.0 - 132.0 * f) as u8,
                        (242.0 - 187.0 * f) as u8,
                        (238.0 - 68.0 * f) as u8,
                    ];
                    c.line(tk.pts[i], tk.pts[i + 1], col, w, 0.95);
                }
                // Flow arrows every ~500 m, pointing DOWNSTREAM (toward the
                // mouth — pts are mouth-first, so toward decreasing index).
                // Direction is what makes a Y legible as a merge, not a split.
                let step = 25usize; // 25 pts * 20 m
                let mut i = step;
                while i + 1 < tk.pts.len() {
                    let dirv = course_world::math::Vec2::new(
                        tk.pts[i - 1].x - tk.pts[i + 1].x,
                        tk.pts[i - 1].y - tk.pts[i + 1].y,
                    )
                    .normalized();
                    let perp = dirv.perp();
                    let tip = tk.pts[i];
                    let l = 46.0;
                    for sgn in [1.0, -1.0] {
                        let tail = course_world::math::Vec2::new(
                            tip.x - dirv.x * l + perp.x * l * 0.55 * sgn,
                            tip.y - dirv.y * l + perp.y * l * 0.55 * sgn,
                        );
                        c.line(tip, tail, [20, 24, 30], 1.6, 0.85);
                    }
                    i += step;
                }
                c.disc(tk.pts[0], 5.0, course_lab::MOUTH, 1.0);
                let head = *tk.pts.last().unwrap();
                c.disc(head, 4.0, [120, 235, 170], 1.0);
                if tk.joins.is_some() {
                    // ring the junction so the confluence is visible
                    c.disc(tk.pts[0], 6.0, [255, 255, 255], 0.9);
                    c.disc(tk.pts[0], 3.5, [40, 90, 200], 1.0);
                }
            }
            // tributaries: thinner, same elevation ramp
            for tb in &net.tribs {
                let w = 1.5;
                for i in 0..tb.pts.len() - 1 {
                    let f = (tb.z[i] / zmax).clamp(0.0, 1.0);
                    let col = [
                        (242.0 - 132.0 * f) as u8,
                        (242.0 - 187.0 * f) as u8,
                        (238.0 - 68.0 * f) as u8,
                    ];
                    c.line(tb.pts[i], tb.pts[i + 1], col, w, 0.9);
                }
            }
            c.save_jpeg(&format!("{out}/{stem}_net.jpg"), 88).unwrap();

            let mut sins = Vec::new();
            let mut rmin = f64::MAX;
            for tk in &net.trunks {
                let sp = Spine::new(tk.pts.clone());
                rmin = rmin.min(sp.min_curvature_radius());
                let total = sp.length();
                if total > 600.0 {
                    let n = (total / 10.0) as usize;
                    let rs: Vec<_> = (0..=n).map(|i| sp.point_at(i as f64 / n as f64)).collect();
                    let w = 60;
                    for i in 0..rs.len().saturating_sub(w) {
                        let chord = rs[i].distance(rs[i + w]);
                        if chord > 1.0 { sins.push(600.0 / chord); }
                    }
                }
            }
            sins.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let sin = if sins.is_empty() { 0.0 } else { sins[sins.len() / 2] };
            let mut len = net.trunk_len_m();
            for tb in &net.tribs {
                len += tb.pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
            }
            rows.push(format!(
                r#"{{"arch":"{}","seed":{seed},"trunks":{},"joins":{},"tribs":{},"died":{},"sin600":{sin:.3},"min_rad":{:.0},"len_km":{:.2}}}"#,
                arch.key(), net.trunks.len(),
                net.trunks.iter().filter(|k| k.joins.is_some()).count(),
                net.tribs.len(),
                net.died_offtile + net.died_exhausted + net.died_stub,
                if rmin == f64::MAX { 0.0 } else { rmin },
                len / 1000.0));
        }
    }
    std::fs::write(format!("{out}/stats.json"), format!("[\n{}\n]", rows.join(",\n"))).unwrap();
    println!("wrote {} tiles to {out}", rows.len());
}
