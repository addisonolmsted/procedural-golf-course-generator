//! M1 visual verification: render the template a seed produces.
//!
//! There is no heightfield at step 3 -- this draws the FIELDS and the placed
//! geometry, which is the whole of what M1 claims to make.
//!
//!   cargo run --release -p course-template --example template_render -- [out_dir] [seeds..]

use course_draw::{generate, Archetype};
use course_lab::{banded, diverging, sequential, Canvas};
use course_seed::RunIdentity;
use course_template::{build, Template};
use course_world::math::Vec2;
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

const PX: u32 = 620;

fn decorate(c: &mut Canvas, t: &Template, trunks: bool, scarps: bool) {
    c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::MUTED, 0.5);
    if scarps {
        for s in &t.scarps {
            c.polyline(s, course_lab::SCARP, 1.6, 0.85);
        }
    }
    if trunks {
        for tk in &t.trunks {
            // A stub in the heading direction: step 2 declares WHERE and
            // WHICH WAY, not the path -- the path is step 4's job.
            let d = Vec2::new(
                course_world::math::cos(tk.azimuth_rad),
                course_world::math::sin(tk.azimuth_rad),
            );
            let len = 240.0 + 26.0 * tk.external_km2.sqrt();
            let end = Vec2::new(tk.mouth.x + d.x * len, tk.mouth.y + d.y * len);
            let w = 2.0 + 0.9 * tk.external_km2.sqrt().min(9.0);
            c.line(tk.mouth, end, course_lab::TRUNK, w, 0.95);
            c.disc(tk.mouth, 5.0, course_lab::MOUTH, 1.0);
        }
    }
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "out/m1_template".into());
    let seeds: Vec<u64> = {
        let v: Vec<u64> = args.filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { vec![1, 2, 3] } else { v }
    };
    std::fs::create_dir_all(&out).expect("create out dir");

    println!("{:<14} {:>4} {:>7} {:>6} {:>7} {:>6} {:>6} {:>7}",
             "archetype", "seed", "relief", "trunks", "inflow", "scarps", "aniso", "d2c");
    for a in Archetype::ALL {
        for &seed in &seeds {
            let id = RunIdentity::from_seed(seed);
            let t = build(&id, &generate(&id, Some(a)));

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.relief_pred, -1.0, 1.0, diverging);
            decorate(&mut c, &t, true, false);
            c.save(&format!("{out}/{}_{seed}_relief.png", a.key())).unwrap();

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.resistance_hint, 0.0, 1.0, banded);
            decorate(&mut c, &t, false, true);
            c.save(&format!("{out}/{}_{seed}_resistance.png", a.key())).unwrap();

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.relief_pred, -1.0, 1.0, |v| {
                let s = sequential(v * 0.45 + 0.12);
                [s[0] / 2 + 10, s[1] / 2 + 11, s[2] / 2 + 13]
            });
            c.quiver(&t.fields.grain_rad, 96.0, 74.0, course_lab::GRAIN, 0.8);
            decorate(&mut c, &t, true, true);
            c.save(&format!("{out}/{}_{seed}_grain.png", a.key())).unwrap();

            let inflow: f64 = t.trunks.iter().map(|k| k.external_km2).sum::<f64>() + 0.0;
            println!("{:<14} {:>4} {:>7.1} {:>6} {:>7.1} {:>6} {:>6.2} {:>7.0}",
                     a.key(), seed, generate(&id, Some(a)).d.relief_budget_m,
                     t.trunks.len(), inflow, t.scarps.len(),
                     t.pattern.anisotropy, t.pattern.d2c_target_m);
        }
    }
    println!("\nwrote {} PNGs to {out}", Archetype::ALL.len() * seeds.len() * 3);
}
