//! Export M1 tiles + stats for the viewer artifact.
//!
//!   cargo run --release -p course-template --example m1_export -- out/m1_viewer 1 2 3 4

use course_draw::{generate, Archetype};
use course_lab::{banded, diverging, sequential, Canvas};
use course_seed::RunIdentity;
use course_template::{build, Template};
use course_world::math::Vec2;
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

const PX: u32 = 460;

fn trunks_on(c: &mut Canvas, t: &Template) {
    for tk in &t.trunks {
        let d = Vec2::new(
            course_world::math::cos(tk.azimuth_rad),
            course_world::math::sin(tk.azimuth_rad),
        );
        let len = 240.0 + 26.0 * tk.external_km2.sqrt();
        let end = Vec2::new(tk.mouth.x + d.x * len, tk.mouth.y + d.y * len);
        c.line(tk.mouth, end, course_lab::TRUNK, 2.2 + 0.9 * tk.external_km2.sqrt().min(9.0), 0.95);
        c.disc(tk.mouth, 5.0, course_lab::MOUTH, 1.0);
    }
}

fn main() {
    let mut a = std::env::args().skip(1);
    let out = a.next().unwrap_or_else(|| "out/m1_viewer".into());
    let seeds: Vec<u64> = {
        let v: Vec<u64> = a.filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { vec![1, 2, 3, 4] } else { v }
    };
    std::fs::create_dir_all(&out).unwrap();
    let mut rows = Vec::new();

    for arch in Archetype::ALL {
        for &seed in &seeds {
            let id = RunIdentity::from_seed(seed);
            let dr = generate(&id, Some(arch));
            let t = build(&id, &dr);
            let stem = format!("{}_{seed}", arch.key());

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.relief_pred, -1.0, 1.0, diverging);
            c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::INK, 0.4);
            trunks_on(&mut c, &t);
            c.save_jpeg(&format!("{out}/{stem}_relief.jpg"), 88).unwrap();

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.resistance_hint, 0.0, 1.0, banded);
            for s in &t.scarps { c.polyline(s, course_lab::SCARP, 1.5, 0.9); }
            c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::MUTED, 0.5);
            c.save_jpeg(&format!("{out}/{stem}_strata.jpg"), 88).unwrap();

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.relief_pred, -1.0, 1.0, |v| {
                let s = sequential(v * 0.45 + 0.14);
                [s[0] / 2 + 14, s[1] / 2 + 15, s[2] / 2 + 18]
            });
            c.quiver(&t.fields.grain_rad, 108.0, 82.0, course_lab::GRAIN, 0.85);
            c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::MUTED, 0.45);
            c.save_jpeg(&format!("{out}/{stem}_grain.jpg"), 88).unwrap();

            let mut c = Canvas::new(PX, EXTENT_M);
            c.field(&t.fields.relief_pred, -1.0, 1.0, diverging);
            for s in &t.scarps { c.polyline(s, course_lab::SCARP, 1.3, 0.85); }
            c.quiver(&t.fields.grain_rad, 175.0, 120.0, course_lab::GRAIN, 0.6);
            c.core_box(CORE_MIN_M, CORE_MAX_M, course_lab::INK, 0.45);
            trunks_on(&mut c, &t);
            c.save_jpeg(&format!("{out}/{stem}_all.jpg"), 88).unwrap();

            let inflow: f64 = t.trunks.iter().map(|k| k.external_km2).sum::<f64>() + 0.0;
            let beds = t.fields.strata.layers.len();
            let thick = t.fields.strata.total_thickness_m();
            rows.push(format!(
                r#"{{"arch":"{}","seed":{seed},"structure":"{:?}","relief":{:.1},"trunks":{},"inflow":{:.1},"scarps":{},"aniso":{:.3},"d2c":{:.0},"integration":{:.3},"riser":{:.2},"widen":{:.2},"terraces":{},"asym":{:.2},"beds":{beds},"stack_m":{:.1},"dip_pct":{:.2},"edge":"{:?}"}}"#,
                arch.key(), t.structure, dr.d.relief_budget_m, t.trunks.len(), inflow,
                t.scarps.len(), dr.d.anisotropy, dr.d.d2c_target_m, dr.d.integration,
                dr.d.riser_m, dr.d.floor_widen, dr.d.terrace_steps, dr.d.terrace_asymmetry,
                thick, t.fields.strata.dip_grade * 100.0, t.base_edge
            ));
        }
    }
    std::fs::write(format!("{out}/stats.json"), format!("[\n{}\n]", rows.join(",\n"))).unwrap();
    println!("wrote {} tiles x 4 views + stats.json to {out}", rows.len());
}
