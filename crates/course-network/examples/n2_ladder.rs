//! The down-valley gravity ladder: the same tiles at five pull strengths,
//! so the operating point is chosen by eye against measured numbers —
//! never one without the other.
//!
//!   cargo run --release -p course-network --example n2_ladder -- out/n2_ladder

use course_draw::{generate, Archetype};
use course_lab::{sequential, Canvas};
use course_network::{grow_tribs, CHANNEL_AREA_M2};
use course_seed::RunIdentity;
use course_world::math::{self, Vec2};
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

const PX: u32 = 430;
const RUNGS: [(f64, f64, &str); 5] = [
    (0.0, 0.0, "L0 · no gravity"),
    (420.0, 750.0, "L1 · current"),
    (900.0, 1400.0, "L2"),
    (1500.0, 2200.0, "L3"),
    (2400.0, 3400.0, "L4 · whole-tile pull"),
];
const TILES: [(Archetype, u64); 4] = [
    (Archetype::Piedmont, 1),
    (Archetype::Piedmont, 2),
    (Archetype::HillCountry, 2),
    (Archetype::RiverValley, 1),
];

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "out/n2_ladder".into());
    std::fs::create_dir_all(&out).unwrap();
    let mut rows = Vec::new();

    for (ri, (shift, reach, label)) in RUNGS.iter().enumerate() {
        let mut angs: Vec<f64> = Vec::new();
        for (arch, seed) in TILES {
            let id = RunIdentity::from_seed(seed);
            let t = course_template::build(&id, &generate(&id, Some(arch)));
            let trunks = course_network::build(&id, &t).trunks;
            let (tribs, _, _, _) = grow_tribs(&id, &t, &trunks, *shift, *reach);

            // measure entry angles over a 130 m baseline — the EYE's scale,
            // not the ruler's 50 m
            {
                let mut chans = course_network::proto::ChannelSet::new();
                for tk in &trunks {
                    chans.add_polyline(&tk.pts, &tk.z);
                }
                for tb in &tribs {
                    let k = (130.0 / 22.0) as usize;
                    let a = tb.pts[k.min(tb.pts.len() - 1)];
                    let inc = Vec2::new(tb.pts[0].x - a.x, tb.pts[0].y - a.y);
                    if inc.length() < 1e-6 { continue; }
                    if (tb.attach_pt as usize) < chans.pts.len() {
                        let down = chans.down[tb.attach_pt as usize];
                        let c = inc.normalized().dot(down).clamp(-1.0, 1.0);
                        angs.push(math::acos(c).to_degrees());
                    }
                }
            }

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.relief_pred, -1.0, 1.0, |v| {
                let s = sequential(v * 0.4 + 0.16);
                [s[0] / 3 + 14, s[1] / 3 + 15, s[2] / 3 + 18]
            });
            c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::MUTED, 0.3);
            let zmax = trunks.iter().map(|k| *k.z.last().unwrap_or(&1.0)).fold(1e-6, f64::max);
            for tk in &trunks {
                let w = 2.6 + 1.0 * tk.external_km2.sqrt().min(9.0);
                for i in 0..tk.pts.len() - 1 {
                    let f = (tk.z[i] / zmax).clamp(0.0, 1.0);
                    let col = [(242.0 - 132.0 * f) as u8, (242.0 - 187.0 * f) as u8, (238.0 - 68.0 * f) as u8];
                    c.line(tk.pts[i], tk.pts[i + 1], col, w, 0.95);
                }
            }
            for tb in &tribs {
                for i in 0..tb.pts.len() - 1 {
                    let f = (tb.z[i] / zmax).clamp(0.0, 1.0);
                    let col = [(242.0 - 132.0 * f) as u8, (242.0 - 187.0 * f) as u8, (238.0 - 68.0 * f) as u8];
                    c.line(tb.pts[i], tb.pts[i + 1], col, 1.6, 0.92);
                }
            }
            c.save_jpeg(&format!("{out}/r{ri}_{}_{seed}.jpg", arch.key()), 86).unwrap();
        }
        angs.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let p50 = if angs.is_empty() { f64::NAN } else { angs[angs.len() / 2] };
        let g80 = if angs.is_empty() { f64::NAN } else {
            100.0 * angs.iter().filter(|v| **v > 80.0).count() as f64 / angs.len() as f64 };
        rows.push(format!(
            r#"{{"rung":{ri},"label":"{label}","shift":{shift},"reach":{reach},"junc130_p50":{p50:.1},"g80":{g80:.1}}}"#));
        println!("{label:22}  shift {shift:6.0}  reach {reach:6.0}  junc@130m p50 {p50:5.1}  >80 {g80:4.1}%");
    }
    std::fs::write(format!("{out}/rungs.json"), format!("[\n{}\n]", rows.join(",\n"))).unwrap();
    let _ = CHANNEL_AREA_M2;
    println!("wrote {out}");
}
