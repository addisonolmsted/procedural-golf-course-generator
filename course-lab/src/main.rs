//! course-lab — the archetype-pipeline visualizer.
//!
//! One tab per pipeline stage (3–10 plus the spec), a seed scrub, and an
//! archetype selector. Every tab currently renders the [`course_contracts::fixtures`]
//! stand-ins; as real stage crates land they replace the fixture calls here
//! one at a time, contract-unchanged.
//!
//!   cargo run -p course-lab --release

use course_contracts::fixtures::{run_fixture_pipeline, FixtureBundle};
use course_contracts::stages::CoverClass;
use course_contracts::{ArchetypeId, CORE_MAX_M, CORE_MIN_M, EXTENT_M};
use eframe::egui;
use golf_core::{Grid, Vec2};

const IMG_PX: u32 = 900;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1320.0, 940.0]),
        ..Default::default()
    };
    eframe::run_native(
        "course-lab — archetype pipeline (fixtures)",
        options,
        Box::new(|_cc| Ok(Box::new(Lab::default()))),
    )
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Tab {
    Spec,
    Macro,
    Noise,
    Hydro,
    Cover,
    Gate,
    Route,
    Earthworks,
    Drainage,
}

const TABS: [(Tab, &str); 9] = [
    (Tab::Spec, "1–2 Spec"),
    (Tab::Macro, "3 Macro"),
    (Tab::Noise, "4 Noise"),
    (Tab::Hydro, "5 Hydro"),
    (Tab::Cover, "6 Cover"),
    (Tab::Gate, "7 Gate"),
    (Tab::Route, "8 Route"),
    (Tab::Earthworks, "9 Earthworks"),
    (Tab::Drainage, "10 Drainage"),
];

struct Lab {
    tab: Tab,
    seed: u64,
    archetype: ArchetypeId,
    fast: bool,
    dirty: bool,
    bundle: Option<FixtureBundle>,
    tex: Option<egui::TextureHandle>,
    stats: String,
    contours: bool,
    contour_step: f64,
}

impl Default for Lab {
    fn default() -> Self {
        Lab {
            tab: Tab::Macro,
            seed: 7,
            archetype: ArchetypeId::Piedmont,
            fast: true,
            dirty: true,
            bundle: None,
            tex: None,
            stats: String::new(),
            contours: true,
            contour_step: 2.0,
        }
    }
}

impl Lab {
    fn res_m(&self) -> f64 {
        if self.fast {
            16.0
        } else {
            4.0
        }
    }

    fn regen(&mut self, ctx: &egui::Context) {
        let t0 = std::time::Instant::now();
        let bundle = run_fixture_pipeline(self.seed, self.archetype, self.res_m());
        let img = self.render(&bundle);
        let g = &bundle.composed.height;
        let (lo, hi) = g
            .data
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
        self.stats = format!(
            "{} | {}x{} @ {} m | z [{lo:.1}, {hi:.1}] m | gate {} | {} ms",
            bundle.spec.archetype.label(),
            g.spec.nx,
            g.spec.ny,
            self.res_m(),
            if bundle.gate.pass { "PASS" } else { "fail" },
            t0.elapsed().as_millis()
        );
        let size = [img.width() as usize, img.height() as usize];
        let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_flat_samples().as_slice());
        self.tex = Some(ctx.load_texture("course", ci, egui::TextureOptions::LINEAR));
        self.bundle = Some(bundle);
        self.dirty = false;
    }

    fn render(&self, b: &FixtureBundle) -> image::RgbaImage {
        let base: &Grid<f64> = match self.tab {
            Tab::Spec | Tab::Macro => &b.skeleton.base_height,
            _ => &b.composed.height,
        };
        let mut img = golf_viz::render_height_grid(base, IMG_PX, None);
        if self.contours {
            overlay_contours(&mut img, base, self.contour_step);
        }
        match self.tab {
            Tab::Spec => {}
            Tab::Macro => {
                for spine in &b.skeleton.structure.spines {
                    draw_polyline(&mut img, &spine.pts, [200, 60, 40, 255]);
                }
            }
            Tab::Noise => {}
            Tab::Hydro => {
                tint_flow(&mut img, &b.hydro.flow.flow_area_m2, b.spec.params.hydro.channel_threshold_ha);
                for s in &b.hydro.graph.streams {
                    draw_polyline(&mut img, &s.pts, [40, 90, 220, 255]);
                }
                for l in &b.hydro.graph.lakes {
                    draw_polyline_closed(&mut img, &l.outline, [30, 70, 200, 255]);
                }
                for w in &b.hydro.graph.wetlands {
                    draw_polyline_closed(&mut img, &w.outline, [60, 140, 130, 255]);
                }
            }
            Tab::Cover => {
                img = render_cover(&b.cover.class);
            }
            Tab::Gate => {
                // dim non-core so the gated window reads at a glance
                dim_outside_core(&mut img);
            }
            Tab::Route | Tab::Earthworks => {
                for h in &b.routing.holes {
                    draw_polyline_closed(&mut img, &h.corridor, [230, 230, 230, 255]);
                    draw_disc(&mut img, h.tee, 5, [30, 30, 30, 255]);
                    draw_disc(&mut img, h.green, 7, [20, 120, 20, 255]);
                }
                draw_disc(&mut img, b.routing.clubhouse, 9, [160, 40, 160, 255]);
                if self.tab == Tab::Earthworks {
                    for f in &b.earthworks.features {
                        let px = (f.radius_m / EXTENT_M * IMG_PX as f64).max(2.0) as i32;
                        draw_disc(&mut img, f.center, px, [220, 160, 40, 200]);
                    }
                }
            }
            Tab::Drainage => {
                for h in &b.routing.holes {
                    draw_polyline_closed(&mut img, &h.corridor, [230, 230, 230, 255]);
                }
            }
        }
        if self.tab != Tab::Cover {
            draw_core_box(&mut img);
        }
        img
    }

    fn side_text(&self) -> String {
        let Some(b) = &self.bundle else { return String::new() };
        match self.tab {
            Tab::Spec => serde_json::to_string_pretty(&b.spec).unwrap_or_default(),
            Tab::Gate => {
                let mut s = format!("gate: {}\n\nmetrics:\n", if b.gate.pass { "PASS" } else { "FAIL" });
                for (k, v) in &b.gate.metrics {
                    s.push_str(&format!("  {k} = {v:.3}\n"));
                }
                if !b.gate.reasons.is_empty() {
                    s.push_str("\nreasons:\n");
                    for r in &b.gate.reasons {
                        s.push_str(&format!("  - {r}\n"));
                    }
                }
                s
            }
            Tab::Hydro => format!(
                "streams: {}\nlakes: {}\nwetlands: {}\nmode: {:?}",
                b.hydro.graph.streams.len(),
                b.hydro.graph.lakes.len(),
                b.hydro.graph.wetlands.len(),
                b.spec.hydrology_mode,
            ),
            Tab::Route => {
                let mut s = String::from("hole  par\n");
                for (i, h) in b.routing.holes.iter().enumerate() {
                    s.push_str(&format!("  {}    {}\n", i + 1, h.par));
                }
                s
            }
            Tab::Earthworks => format!(
                "patches: {}\nfeatures: {}",
                b.earthworks.patches.len(),
                b.earthworks.features.len()
            ),
            Tab::Drainage => {
                let mut s = format!("drainage: {}\n\n", if b.drainage.pass { "PASS" } else { "FAIL" });
                for (k, v) in &b.drainage.metrics {
                    s.push_str(&format!("  {k} = {v:.1}\n"));
                }
                for note in &b.drainage.notes {
                    s.push_str(&format!("  ! {note}\n"));
                }
                s
            }
            _ => String::new(),
        }
    }
}

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("controls").min_width(280.0).show(ctx, |ui| {
            ui.heading("course-lab");
            ui.label("all stages fixture-backed (M0)");
            ui.separator();

            let mut seed = self.seed;
            ui.horizontal(|ui| {
                ui.label("seed");
                if ui.add(egui::DragValue::new(&mut seed).speed(1)).changed() {
                    self.seed = seed;
                    self.dirty = true;
                }
                if ui.button("+1").clicked() {
                    self.seed += 1;
                    self.dirty = true;
                }
            });

            egui::ComboBox::from_label("archetype")
                .selected_text(self.archetype.label())
                .show_ui(ui, |ui| {
                    for a in ArchetypeId::ALL {
                        if ui.selectable_value(&mut self.archetype, a, a.label()).changed() {
                            self.dirty = true;
                        }
                    }
                });

            if ui.checkbox(&mut self.fast, "fast (16 m preview)").changed() {
                self.dirty = true;
            }
            if ui.checkbox(&mut self.contours, "contours").changed() {
                self.dirty = true;
            }
            if self.contours
                && ui
                    .add(egui::Slider::new(&mut self.contour_step, 0.5..=10.0).text("contour step (m)"))
                    .changed()
            {
                self.dirty = true;
            }

            ui.separator();
            for (t, label) in TABS {
                if ui.selectable_label(self.tab == t, label).clicked() && self.tab != t {
                    self.tab = t;
                    self.dirty = true;
                }
            }

            ui.separator();
            let side = self.side_text();
            if !side.is_empty() {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.monospace(side);
                });
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.dirty {
                self.regen(ctx);
            }
            ui.label(&self.stats);
            if let Some(tex) = &self.tex {
                let avail = ui.available_size();
                let side = avail.x.min(avail.y);
                ui.image((tex.id(), egui::vec2(side, side)));
            }
        });
    }
}

// ---------------------------------------------------------------------------
// Image helpers (world convention: y up, image row 0 = north)
// ---------------------------------------------------------------------------

fn to_px(p: Vec2, w: u32, h: u32) -> (i64, i64) {
    (
        (p.x / EXTENT_M * w as f64) as i64,
        ((1.0 - p.y / EXTENT_M) * h as f64) as i64,
    )
}

fn put(img: &mut image::RgbaImage, x: i64, y: i64, c: [u8; 4]) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, image::Rgba(c));
    }
}

fn draw_line(img: &mut image::RgbaImage, a: (i64, i64), b: (i64, i64), c: [u8; 4]) {
    let n = (b.0 - a.0).abs().max((b.1 - a.1).abs()).max(1);
    for i in 0..=n {
        let t = i as f64 / n as f64;
        put(
            img,
            a.0 + ((b.0 - a.0) as f64 * t).round() as i64,
            a.1 + ((b.1 - a.1) as f64 * t).round() as i64,
            c,
        );
    }
}

fn draw_polyline(img: &mut image::RgbaImage, pts: &[Vec2], c: [u8; 4]) {
    let (w, h) = (img.width(), img.height());
    for seg in pts.windows(2) {
        draw_line(img, to_px(seg[0], w, h), to_px(seg[1], w, h), c);
    }
}

fn draw_polyline_closed(img: &mut image::RgbaImage, pts: &[Vec2], c: [u8; 4]) {
    if pts.len() < 2 {
        return;
    }
    draw_polyline(img, pts, c);
    let (w, h) = (img.width(), img.height());
    draw_line(img, to_px(pts[pts.len() - 1], w, h), to_px(pts[0], w, h), c);
}

fn draw_disc(img: &mut image::RgbaImage, center: Vec2, r_px: i32, c: [u8; 4]) {
    let (w, h) = (img.width(), img.height());
    let (cx, cy) = to_px(center, w, h);
    for dy in -(r_px as i64)..=r_px as i64 {
        for dx in -(r_px as i64)..=r_px as i64 {
            if dx * dx + dy * dy <= (r_px as i64) * (r_px as i64) {
                put(img, cx + dx, cy + dy, c);
            }
        }
    }
}

fn draw_core_box(img: &mut image::RgbaImage) {
    let (w, h) = (img.width(), img.height());
    let c = [255, 255, 255, 255];
    let corners = [
        Vec2 { x: CORE_MIN_M, y: CORE_MIN_M },
        Vec2 { x: CORE_MAX_M, y: CORE_MIN_M },
        Vec2 { x: CORE_MAX_M, y: CORE_MAX_M },
        Vec2 { x: CORE_MIN_M, y: CORE_MAX_M },
    ];
    for i in 0..4 {
        draw_line(img, to_px(corners[i], w, h), to_px(corners[(i + 1) % 4], w, h), c);
    }
}

fn dim_outside_core(img: &mut image::RgbaImage) {
    let (w, h) = (img.width(), img.height());
    for py in 0..h {
        for px in 0..w {
            let wx = px as f64 / w as f64 * EXTENT_M;
            let wy = (1.0 - py as f64 / h as f64) * EXTENT_M;
            if !course_contracts::in_core(Vec2 { x: wx, y: wy }) {
                let p = img.get_pixel_mut(px, py);
                for k in 0..3 {
                    p.0[k] = (p.0[k] as f32 * 0.45) as u8;
                }
            }
        }
    }
}

/// Blue tint where effective drained area approaches the channel threshold.
fn tint_flow(img: &mut image::RgbaImage, flow_area_m2: &Grid<f64>, channel_threshold_ha: f64) {
    let (w, h) = (img.width(), img.height());
    let s = flow_area_m2.spec;
    let thresh = channel_threshold_ha * 10_000.0;
    for py in 0..h {
        for px in 0..w {
            let gx = ((px as f64 / w as f64 * s.nx as f64) as u32).min(s.nx - 1);
            let row = ((py as f64 / h as f64 * s.ny as f64) as u32).min(s.ny - 1);
            let gy = s.ny - 1 - row;
            let a = *flow_area_m2.get(gx, gy);
            if a > thresh * 0.1 {
                let t = ((a / thresh).log10() + 1.0).clamp(0.0, 2.0) / 2.0;
                let p = img.get_pixel_mut(px, py);
                p.0[0] = (p.0[0] as f64 * (1.0 - 0.6 * t)) as u8;
                p.0[1] = (p.0[1] as f64 * (1.0 - 0.4 * t)) as u8;
                p.0[2] = (p.0[2] as f64 * (1.0 - t) + 235.0 * t) as u8;
            }
        }
    }
}

fn render_cover(class: &Grid<u8>) -> image::RgbaImage {
    let s = class.spec;
    let mut img = image::RgbaImage::new(IMG_PX, IMG_PX);
    for py in 0..IMG_PX {
        for px in 0..IMG_PX {
            let gx = ((px as f64 / IMG_PX as f64 * s.nx as f64) as u32).min(s.nx - 1);
            let row = ((py as f64 / IMG_PX as f64 * s.ny as f64) as u32).min(s.ny - 1);
            let gy = s.ny - 1 - row;
            let c = match CoverClass::from_u8(*class.get(gx, gy)) {
                Some(CoverClass::Turf) => [120, 190, 90, 255],
                Some(CoverClass::Rough) => [90, 140, 70, 255],
                Some(CoverClass::Sand) => [225, 205, 150, 255],
                Some(CoverClass::Wetland) => [110, 160, 140, 255],
                Some(CoverClass::Water) => [60, 110, 200, 255],
                Some(CoverClass::Rock) => [140, 135, 130, 255],
                Some(CoverClass::Forest) => [45, 95, 55, 255],
                None => [255, 0, 255, 255],
            };
            img.put_pixel(px, py, image::Rgba(c));
        }
    }
    img
}

/// Same convention as terrain-lab: darken pixels on iso-band changes.
fn overlay_contours(img: &mut image::RgbaImage, g: &Grid<f64>, step: f64) {
    let (w, h) = (img.width(), img.height());
    let nx = g.spec.nx;
    let ny = g.spec.ny;
    let band = |z: f64| (z / step).floor() as i64;
    for py in 0..h {
        for px in 0..w {
            let gx = ((px as f64 / w as f64 * nx as f64) as u32).min(nx - 1);
            let row = ((py as f64 / h as f64 * ny as f64) as u32).min(ny - 1);
            let gy = ny - 1 - row;
            let z = *g.get(gx, gy);
            let right = *g.get((gx + 1).min(nx - 1), gy);
            let down = *g.get(gx, gy.saturating_sub(1));
            if band(z) != band(right) || band(z) != band(down) {
                let p = img.get_pixel_mut(px, py);
                p.0[0] = (p.0[0] as f32 * 0.55) as u8;
                p.0[1] = (p.0[1] as f32 * 0.55) as u8;
                p.0[2] = (p.0[2] as f32 * 0.55) as u8;
            }
        }
    }
}
