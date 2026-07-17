//! terrain-lab — the terrain-v2 interactive visualizer (Stage 2: landform
//! primitives). Hillshade + contours, live sliders for the loaded config's
//! primitives, gate-preset buttons, and the 4x reduced-resolution fast path.
//!
//!   cargo run -p terrain-lab --release

use eframe::egui;
use golf_core::grid::Grid;
use golf_landform::{generate, preset_names, presets, MacroConfig, Outlet, Path};

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "terrain-lab — landform primitives (terrain-v2 Stage 2)",
        options,
        Box::new(|_cc| Ok(Box::new(Lab::default()))),
    )
}

struct Lab {
    cfg: MacroConfig,
    preset_name: String,
    fast: bool,
    contours: bool,
    contour_step: f64,
    dirty: bool,
    tex: Option<egui::TextureHandle>,
    stats: String,
}

impl Default for Lab {
    fn default() -> Self {
        let (name, cfg) = presets().into_iter().next().unwrap();
        Lab {
            cfg,
            preset_name: name.to_string(),
            fast: true,
            contours: true,
            contour_step: 2.0,
            dirty: true,
            tex: None,
            stats: String::new(),
        }
    }
}

impl Lab {
    /// Full res 4 m; fast path = 4x coarser (the calibration fast path).
    fn res_m(&self) -> f64 {
        if self.fast {
            16.0
        } else {
            4.0
        }
    }

    fn regen(&mut self, ctx: &egui::Context) {
        let t0 = std::time::Instant::now();
        let g = generate(&self.cfg, self.res_m());
        let mut img = golf_viz::render_height_grid(&g, 900, None);
        if self.contours {
            overlay_contours(&mut img, &g, self.contour_step);
        }
        let (lo, hi) = g
            .data
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
        self.stats = format!(
            "{}x{} @ {} m | z [{lo:.1}, {hi:.1}] m | {} ms",
            g.spec.nx,
            g.spec.ny,
            self.res_m(),
            t0.elapsed().as_millis()
        );
        let size = [img.width() as usize, img.height() as usize];
        let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_flat_samples().as_slice());
        self.tex = Some(ctx.load_texture("terrain", ci, egui::TextureOptions::LINEAR));
    }

    fn slider(ui: &mut egui::Ui, dirty: &mut bool, label: &str, v: &mut f64,
              range: std::ops::RangeInclusive<f64>) {
        if ui
            .add(egui::Slider::new(v, range).text(label))
            .changed()
        {
            *dirty = true;
        }
    }
}

/// Darken pixels where the elevation crosses an iso level (simple, effective
/// contour overlay working at any resolution).
fn overlay_contours(img: &mut image::RgbaImage, g: &Grid<f64>, step: f64) {
    let (w, h) = (img.width(), img.height());
    let nx = g.spec.nx;
    let ny = g.spec.ny;
    let band = |z: f64| (z / step).floor() as i64;
    for py in 0..h {
        for px in 0..w {
            let gx = ((px as f64 / w as f64 * nx as f64) as u32).min(nx - 1);
            // image row 0 = north = LAST grid row
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

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.dirty {
            self.dirty = false;
            self.regen(ctx);
        }
        let mut dirty = false;

        egui::SidePanel::left("controls").min_width(330.0).show(ctx, |ui| {
            ui.heading("terrain-lab");
            ui.label("Stage 2: landform primitives (no noise, no erosion)");
            ui.separator();

            ui.horizontal_wrapped(|ui| {
                for name in preset_names() {
                    if ui
                        .selectable_label(self.preset_name == name, name)
                        .clicked()
                    {
                        self.preset_name = name.to_string();
                        self.cfg = golf_landform::preset(name).unwrap();
                        dirty = true;
                    }
                }
            });
            ui.separator();

            if ui.checkbox(&mut self.fast, "fast path (4x coarser grid)").changed() {
                dirty = true;
            }
            if ui.checkbox(&mut self.contours, "contours").changed() {
                dirty = true;
            }
            if self.contours {
                Self::slider(ui, &mut dirty, "contour step (m)", &mut self.contour_step, 0.5..=10.0);
            }
            ui.separator();

            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.label(egui::RichText::new("tilt").strong());
                Self::slider(ui, &mut dirty, "grade x", &mut self.cfg.tilt.grade_x, -0.03..=0.03);
                Self::slider(ui, &mut dirty, "grade y", &mut self.cfg.tilt.grade_y, -0.03..=0.03);
                Self::slider(ui, &mut dirty, "curve (m)", &mut self.cfg.tilt.curve_m, -20.0..=20.0);

                for (i, v) in self.cfg.valleys.iter_mut().enumerate() {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("valley {i}")).strong());
                    Self::slider(ui, &mut dirty, "floor z0 (m)", &mut v.floor_z0_m, 40.0..=140.0);
                    Self::slider(ui, &mut dirty, "fall gradient", &mut v.fall_gradient, 0.0005..=0.03);
                    Self::slider(ui, &mut dirty, "wall grad L", &mut v.wall_grad_left, 0.05..=1.2);
                    Self::slider(ui, &mut dirty, "wall grad R", &mut v.wall_grad_right, 0.05..=1.2);
                    Self::slider(ui, &mut dirty, "floor round (m)", &mut v.floor_round_m, 0.5..=30.0);
                    Self::slider(ui, &mut dirty, "shoulder k (m)", &mut v.shoulder_k_m, 0.1..=15.0);
                    for k in v.floor_halfwidth.knots.iter_mut() {
                        Self::slider(ui, &mut dirty, &format!("floor hw @s={:.1}", k.0), &mut k.1, 2.0..=250.0);
                    }
                    if let Path::Meander(m) = &mut v.path {
                        Self::slider(ui, &mut dirty, "meander width (m)", &mut m.width_m, 20.0..=300.0);
                        Self::slider(ui, &mut dirty, "meander intensity", &mut m.intensity, 0.0..=1.0);
                        Self::slider(ui, &mut dirty, "wavelength mult", &mut m.wavelength_mult, 8.0..=16.0);
                        Self::slider(ui, &mut dirty, "jitter", &mut m.jitter, 0.0..=0.5);
                        let mut seed = m.seed as f64;
                        Self::slider(ui, &mut dirty, "seed", &mut seed, 0.0..=99.0);
                        m.seed = seed as u64;
                    }
                }

                for (i, r) in self.cfg.ridges.iter_mut().enumerate() {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("ridge {i}")).strong());
                    Self::slider(ui, &mut dirty, "crest z0 (m)", &mut r.crest_z0_m, 100.0..=220.0);
                    Self::slider(ui, &mut dirty, "fall gradient", &mut r.fall_gradient, -0.02..=0.02);
                    Self::slider(ui, &mut dirty, "flank grad L", &mut r.flank_grad_left, 0.05..=1.0);
                    Self::slider(ui, &mut dirty, "flank grad R", &mut r.flank_grad_right, 0.05..=1.0);
                    Self::slider(ui, &mut dirty, "crest round (m)", &mut r.crest_round_m, 0.5..=40.0);
                }

                for (i, b) in self.cfg.bluffs.iter_mut().enumerate() {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("bluff {i}")).strong());
                    Self::slider(ui, &mut dirty, "height (m)", &mut b.height_m, 2.0..=60.0);
                    Self::slider(ui, &mut dirty, "face grad", &mut b.face_grad, 0.05..=1.5);
                    Self::slider(ui, &mut dirty, "end taper", &mut b.taper_frac, 0.0..=0.4);
                }

                for (i, b) in self.cfg.bowls.iter_mut().enumerate() {
                    ui.separator();
                    ui.label(egui::RichText::new(format!("bowl {i}")).strong());
                    Self::slider(ui, &mut dirty, "rim z (m)", &mut b.rim_z_m, 60.0..=160.0);
                    Self::slider(ui, &mut dirty, "depth (m)", &mut b.depth_m, 1.0..=50.0);
                    Self::slider(ui, &mut dirty, "inner grad", &mut b.inner_grad, 0.05..=1.0);
                    Self::slider(ui, &mut dirty, "rim round (m)", &mut b.rim_round_m, 0.5..=40.0);
                    if let golf_landform::BowlBoundary::Blob { radius_m, wobble, cycles, .. } =
                        &mut b.boundary
                    {
                        Self::slider(ui, &mut dirty, "radius (m)", radius_m, 60.0..=700.0);
                        Self::slider(ui, &mut dirty, "wobble", wobble, 0.0..=0.5);
                        Self::slider(ui, &mut dirty, "wobble cycles", cycles, 0.3..=4.0);
                    }
                    if let Outlet::Spillway { at_s, halfwidth_m, depth_m } = &mut b.outlet {
                        Self::slider(ui, &mut dirty, "spillway at s", at_s, 0.0..=1.0);
                        Self::slider(ui, &mut dirty, "spillway halfwidth", halfwidth_m, 10.0..=400.0);
                        Self::slider(ui, &mut dirty, "spillway depth", depth_m, 0.5..=40.0);
                    }
                }
            });

            ui.separator();
            if ui.button("copy config JSON").clicked() {
                ctx.copy_text(self.cfg.to_json());
            }
            ui.label(&self.stats);
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if let Some(tex) = &self.tex {
                let avail = ui.available_size();
                let s = (avail.x / tex.size_vec2().x)
                    .min(avail.y / tex.size_vec2().y)
                    .min(1.6);
                ui.centered_and_justified(|ui| {
                    ui.add(egui::Image::new(tex).fit_to_exact_size(tex.size_vec2() * s));
                });
            }
        });

        if dirty {
            self.dirty = true;
            ctx.request_repaint();
        }
    }
}
