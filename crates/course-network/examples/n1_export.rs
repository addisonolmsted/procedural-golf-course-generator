//! Render N1 trunks for review: planform over the relief field, elevation
//! coloured along the line so monotone drainage is visually checkable.
//!
//!   cargo run --release -p course-network --example n1_export -- out/n1_viewer 1 2 3 4 5 6

use course_draw::{generate, Archetype};
use course_lab::{diverging, Canvas};
use course_network::build;
use course_seed::RunIdentity;
use course_world::spline::Spine;
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

const PX: u32 = 460;

fn main() {
    let mut a = std::env::args().skip(1);
    let out = a.next().unwrap_or_else(|| "out/n1_viewer".into());
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
            for tk in &net.trunks {
                let zmax = tk.z.last().copied().unwrap_or(1.0).max(1e-6);
                let w = 2.4 + 1.1 * tk.external_km2.sqrt().min(9.0);
                for i in 0..tk.pts.len() - 1 {
                    // elevation colour: white (base level) -> deep blue (head)
                    let f = (tk.z[i] / zmax).clamp(0.0, 1.0);
                    let col = [
                        (235.0 - 165.0 * f) as u8,
                        (240.0 - 90.0 * f) as u8,
                        255u8,
                    ];
                    c.line(tk.pts[i], tk.pts[i + 1], col, w, 0.95);
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
            c.save_jpeg(&format!("{out}/{stem}_trunk.jpg"), 88).unwrap();

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
            rows.push(format!(
                r#"{{"arch":"{}","seed":{seed},"trunks":{},"joins":{},"sin600":{sin:.3},"min_rad":{:.0},"len_km":{:.2},"head_z":{:.1}}}"#,
                arch.key(), net.trunks.len(),
                net.trunks.iter().filter(|k| k.joins.is_some()).count(),
                if rmin == f64::MAX { 0.0 } else { rmin },
                net.trunk_len_m() / 1000.0,
                net.trunks.iter().map(|k| *k.z.last().unwrap_or(&0.0)).fold(0.0, f64::max)));
        }
    }
    std::fs::write(format!("{out}/stats.json"), format!("[\n{}\n]", rows.join(",\n"))).unwrap();
    println!("wrote {} tiles to {out}", rows.len());
}
