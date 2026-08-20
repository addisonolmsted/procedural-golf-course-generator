//! M1 review sheet: every archetype x several seeds in one image.
//!
//!   cargo run --release -p course-template --example template_sheet -- [out.png] [seeds..]
//!
//! Rows are `Archetype::ALL` order, top to bottom:
//!   piedmont / great_plains / river_valley / hill_country / heathland / sandhills
//! A colour tab at each row's left edge repeats that order so the rows can be
//! told apart without a font.

use course_draw::{generate, Archetype};
use course_lab::{diverging, Canvas, Sheet, TABS};
use course_seed::RunIdentity;
use course_world::math::Vec2;
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

const CELL: u32 = 340;

fn tile(a: Archetype, seed: u64) -> Canvas {
    let id = RunIdentity::from_seed(seed);
    let t = course_template::build(&id, &generate(&id, Some(a)));
    let mut c = Canvas::new(CELL, EXTENT_M);
    // Relief predisposition as the ground. This is the field that steers
    // trunk placement AND network growth, so it has to be legible: an
    // earlier version dimmed it to near-black to let the overlays read, which
    // hid the one thing the sheet exists to show.
    c.field(&t.fields.relief_pred, -1.0, 1.0, diverging);
    // provisional bed contacts
    for s in &t.scarps {
        c.polyline(s, course_lab::SCARP, 1.3, 0.9);
    }
    // structural grain
    c.quiver(&t.fields.grain_rad, 165.0, 118.0, course_lab::GRAIN, 0.75);
    c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::INK, 0.45);
    // trunks: stub length and width both scale with declared external inflow
    for tk in &t.trunks {
        let d = Vec2::new(
            course_world::math::cos(tk.azimuth_rad),
            course_world::math::sin(tk.azimuth_rad),
        );
        let len = 240.0 + 26.0 * tk.external_km2.sqrt();
        let end = Vec2::new(tk.mouth.x + d.x * len, tk.mouth.y + d.y * len);
        c.line(tk.mouth, end, course_lab::TRUNK, 2.0 + 0.9 * tk.external_km2.sqrt().min(9.0), 0.95);
        c.disc(tk.mouth, 4.0, course_lab::MOUTH, 1.0);
    }
    c
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "out/m1_template/SHEET.png".into());
    let seeds: Vec<u64> = {
        let v: Vec<u64> = args.filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { vec![1, 2, 3, 4] } else { v }
    };
    if let Some(p) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(p).ok();
    }

    let mut sheet = Sheet::new(CELL);
    for (r, a) in Archetype::ALL.into_iter().enumerate() {
        sheet.row(TABS[r], seeds.iter().map(|&s| tile(a, s)).collect());
    }
    let (w, h) = sheet.save(&out).expect("save sheet");
    println!("rows top->bottom: {}", Archetype::ALL.map(|a| a.key()).join(" / "));
    println!("cols: seeds {seeds:?}");
    println!("wrote {out} ({w}x{h})");
}
