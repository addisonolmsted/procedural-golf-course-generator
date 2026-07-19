//! terrain-lab — the terrain-v2 interactive visualizer.
//!
//! Tabs:
//!   Landform — Stage-2 primitives (hillshade + contours, live sliders,
//!              gate-preset buttons)
//!   Noise    — the Stage-1/4 noiselab generator: the full assessed parameter
//!              set, optional Stage-2 preset as the modulation skeleton
//!              (floor_damp / slope_gain / grain_align), composed or residual
//!              view. Working box: 3 km x 3 km.
//!
//!   cargo run -p terrain-lab --release

use eframe::egui;
use golf_core::grid::Grid;
use golf_landform::noiselab::{generate_noise, skeleton_fields, NoiseLabConfig, SkeletonFields};
use golf_landform::{generate, preset_names, presets, MacroConfig, Outlet, Path};

/// The terrain-v2 working box (presets are authored on the same square).
const EXTENT_M: f64 = 3000.0;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1280.0, 900.0]),
        ..Default::default()
    };
    eframe::run_native(
        "terrain-lab — terrain-v2 (landform | noise)",
        options,
        Box::new(|_cc| Ok(Box::new(Lab::default()))),
    )
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Landform,
    Noise,
}

/// Skeleton predictor fields cached per (preset, resolution).
struct SkelCache {
    preset: String,
    res_m: f64,
    fields: SkeletonFields,
}

struct NoiseLab {
    cfg: NoiseLabConfig,
    /// None = flat sandbox; Some(preset) = modulation skeleton from Stage 2.
    skeleton: Option<String>,
    /// Compose skeleton z + modulated noise (off = residual noise only).
    compose: bool,
    cache: Option<SkelCache>,
}

impl Default for NoiseLab {
    fn default() -> Self {
        NoiseLab {
            // mid-box defaults spanning the Stage-1 KEEP set
            cfg: NoiseLabConfig {
                seed: 7,
                base_amp: 10.0,
                base_wavelength: 300.0,
                octaves: 5,
                gain: 0.5,
                lacunarity: 2.0,
                warp_amp: 80.0,
                warp_wavelength: 500.0,
                warp2_amp: 20.0,
                ridged_mix: 0.2,
                redistribution: 1.3,
                tex_amp: 0.4,
                tex_wavelength: 25.0,
                tex_gain: 0.55,
                nugget_amp: 0.0,
                aniso_ratio: 1.5,
                floor_damp: 0.2,
                slope_gain: 0.3,
                grain_align: 0.0,
                dummy: 0.0,
            },
            skeleton: Some("river_confluence".to_string()),
            compose: true,
            cache: None,
        }
    }
}

struct Lab {
    tab: Tab,
    cfg: MacroConfig,
    preset_name: String,
    noise: NoiseLab,
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
            tab: Tab::Landform,
            cfg,
            preset_name: name.to_string(),
            noise: NoiseLab::default(),
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
    /// Landform: full 4 m, fast 16 m (the calibration fast path).
    /// Noise: full 4 m, fast 8 m — fine texture is the point of the tab.
    fn res_m(&self) -> f64 {
        match (self.tab, self.fast) {
            (Tab::Landform, true) => 16.0,
            (Tab::Noise, true) => 8.0,
            (_, false) => 4.0,
        }
    }

    fn regen(&mut self, ctx: &egui::Context) {
        let t0 = std::time::Instant::now();
        let g = match self.tab {
            Tab::Landform => generate(&self.cfg, self.res_m()),
            Tab::Noise => self.gen_noise(),
        };
        let mut img = golf_viz::render_height_grid(&g, 900, None);
        if self.contours {
            overlay_contours(&mut img, &g, self.contour_step);
        }
        let (lo, hi) = g
            .data
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| (l.min(v), h.max(v)));
        let extra = match self.tab {
            Tab::Landform => String::new(),
            Tab::Noise => {
                let rms = self.noise_rms(&g);
                format!(" | noise rms {rms:.2} m")
            }
        };
        self.stats = format!(
            "{}x{} @ {} m | z [{lo:.1}, {hi:.1}] m{extra} | {} ms",
            g.spec.nx,
            g.spec.ny,
            self.res_m(),
            t0.elapsed().as_millis()
        );
        let size = [img.width() as usize, img.height() as usize];
        let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_flat_samples().as_slice());
        self.tex = Some(ctx.load_texture("terrain", ci, egui::TextureOptions::LINEAR));
    }

    /// Noise field on the working box; skeleton fields cached per (preset, res).
    fn gen_noise(&mut self) -> Grid<f64> {
        let res = self.res_m();
        if let Some(name) = &self.noise.skeleton {
            let stale = self
                .noise
                .cache
                .as_ref()
                .map_or(true, |c| c.preset != *name || c.res_m != res);
            if stale {
                let cfg = golf_landform::preset(name).expect("preset exists");
                self.noise.cache = Some(SkelCache {
                    preset: name.clone(),
                    res_m: res,
                    fields: skeleton_fields(&cfg, res),
                });
            }
        }
        let sk = match (&self.noise.skeleton, &self.noise.cache) {
            (Some(_), Some(c)) => Some(&c.fields),
            _ => None,
        };
        let mut g = generate_noise(&self.noise.cfg, EXTENT_M, sk, res);
        // residual view: strip the skeleton z back out (same spec: presets are
        // authored on EXTENT_M at the same res)
        if let (false, Some(f)) = (self.noise.compose, sk) {
            debug_assert_eq!(f.z.data.len(), g.data.len());
            for i in 0..g.data.len() {
                g.data[i] -= f.z.data[i];
            }
        }
        g
    }

    /// RMS of the texture component (composed view subtracts the skeleton).
    fn noise_rms(&self, g: &Grid<f64>) -> f64 {
        let skel = match (&self.noise.skeleton, &self.noise.cache, self.noise.compose) {
            (Some(_), Some(c), true) => Some(&c.fields.z),
            _ => None,
        };
        let n = g.data.len() as f64;
        let mut sum = 0.0;
        let mut sum2 = 0.0;
        for i in 0..g.data.len() {
            let v = g.data[i] - skel.map_or(0.0, |z| z.data[i]);
            sum += v;
            sum2 += v * v;
        }
        ((sum2 / n) - (sum / n) * (sum / n)).max(0.0).sqrt()
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

    fn log_slider(ui: &mut egui::Ui, dirty: &mut bool, label: &str, v: &mut f64,
                  range: std::ops::RangeInclusive<f64>) {
        if ui
            .add(egui::Slider::new(v, range).logarithmic(true).text(label))
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

impl Lab {
    fn landform_panel(&mut self, ui: &mut egui::Ui, dirty: &mut bool, ctx: &egui::Context) {
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
                    *dirty = true;
                }
            }
        });
        if ui.button("copy config JSON").clicked() {
            ctx.copy_text(self.cfg.to_json());
        }
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.label(egui::RichText::new("tilt").strong());
            Self::slider(ui, dirty, "grade x", &mut self.cfg.tilt.grade_x, -0.03..=0.03);
            Self::slider(ui, dirty, "grade y", &mut self.cfg.tilt.grade_y, -0.03..=0.03);
            Self::slider(ui, dirty, "curve (m)", &mut self.cfg.tilt.curve_m, -20.0..=20.0);

            for (i, v) in self.cfg.valleys.iter_mut().enumerate() {
                ui.separator();
                ui.label(egui::RichText::new(format!("valley {i}")).strong());
                Self::slider(ui, dirty, "floor z0 (m)", &mut v.floor_z0_m, 40.0..=140.0);
                Self::slider(ui, dirty, "fall gradient", &mut v.fall_gradient, 0.0005..=0.03);
                Self::slider(ui, dirty, "wall grad L", &mut v.wall_grad_left, 0.05..=1.2);
                Self::slider(ui, dirty, "wall grad R", &mut v.wall_grad_right, 0.05..=1.2);
                Self::slider(ui, dirty, "floor round (m)", &mut v.floor_round_m, 0.5..=30.0);
                Self::slider(ui, dirty, "shoulder k (m)", &mut v.shoulder_k_m, 0.1..=15.0);
                for k in v.floor_halfwidth.knots.iter_mut() {
                    Self::slider(ui, dirty, &format!("floor hw @s={:.1}", k.0), &mut k.1, 2.0..=250.0);
                }
                if let Path::Meander(m) = &mut v.path {
                    Self::slider(ui, dirty, "meander width (m)", &mut m.width_m, 20.0..=300.0);
                    Self::slider(ui, dirty, "meander intensity", &mut m.intensity, 0.0..=1.0);
                    Self::slider(ui, dirty, "wavelength mult", &mut m.wavelength_mult, 8.0..=16.0);
                    Self::slider(ui, dirty, "jitter", &mut m.jitter, 0.0..=0.5);
                    let mut seed = m.seed as f64;
                    Self::slider(ui, dirty, "seed", &mut seed, 0.0..=99.0);
                    m.seed = seed as u64;
                }
            }

            for (i, r) in self.cfg.ridges.iter_mut().enumerate() {
                ui.separator();
                ui.label(egui::RichText::new(format!("ridge {i}")).strong());
                Self::slider(ui, dirty, "crest z0 (m)", &mut r.crest_z0_m, 100.0..=220.0);
                Self::slider(ui, dirty, "fall gradient", &mut r.fall_gradient, -0.02..=0.02);
                Self::slider(ui, dirty, "flank grad L", &mut r.flank_grad_left, 0.05..=1.0);
                Self::slider(ui, dirty, "flank grad R", &mut r.flank_grad_right, 0.05..=1.0);
                Self::slider(ui, dirty, "crest round (m)", &mut r.crest_round_m, 0.5..=40.0);
            }

            for (i, b) in self.cfg.bluffs.iter_mut().enumerate() {
                ui.separator();
                ui.label(egui::RichText::new(format!("bluff {i}")).strong());
                Self::slider(ui, dirty, "height (m)", &mut b.height_m, 2.0..=60.0);
                Self::slider(ui, dirty, "face grad", &mut b.face_grad, 0.05..=1.5);
                Self::slider(ui, dirty, "end taper", &mut b.taper_frac, 0.0..=0.4);
            }

            for (i, b) in self.cfg.bowls.iter_mut().enumerate() {
                ui.separator();
                ui.label(egui::RichText::new(format!("bowl {i}")).strong());
                Self::slider(ui, dirty, "rim z (m)", &mut b.rim_z_m, 60.0..=160.0);
                Self::slider(ui, dirty, "depth (m)", &mut b.depth_m, 1.0..=50.0);
                Self::slider(ui, dirty, "inner grad", &mut b.inner_grad, 0.05..=1.0);
                Self::slider(ui, dirty, "rim round (m)", &mut b.rim_round_m, 0.5..=40.0);
                if let golf_landform::BowlBoundary::Blob { radius_m, wobble, cycles, .. } =
                    &mut b.boundary
                {
                    Self::slider(ui, dirty, "radius (m)", radius_m, 60.0..=700.0);
                    Self::slider(ui, dirty, "wobble", wobble, 0.0..=0.5);
                    Self::slider(ui, dirty, "wobble cycles", cycles, 0.3..=4.0);
                }
                if let Outlet::Spillway { at_s, halfwidth_m, depth_m } = &mut b.outlet {
                    Self::slider(ui, dirty, "spillway at s", at_s, 0.0..=1.0);
                    Self::slider(ui, dirty, "spillway halfwidth", halfwidth_m, 10.0..=400.0);
                    Self::slider(ui, dirty, "spillway depth", depth_m, 0.5..=40.0);
                }
            }
        });
    }

    fn noise_panel(&mut self, ui: &mut egui::Ui, dirty: &mut bool, ctx: &egui::Context) {
        ui.label("Stage 1/4: noiselab — the assessed parameter set on the 3 km box");
        ui.separator();

        // skeleton selection + view mode
        let current = self.noise.skeleton.clone().unwrap_or_else(|| "flat (none)".into());
        egui::ComboBox::from_label("skeleton")
            .selected_text(&current)
            .show_ui(ui, |ui| {
                if ui
                    .selectable_label(self.noise.skeleton.is_none(), "flat (none)")
                    .clicked()
                {
                    self.noise.skeleton = None;
                    *dirty = true;
                }
                for name in preset_names() {
                    if ui
                        .selectable_label(self.noise.skeleton.as_deref() == Some(name), name)
                        .clicked()
                    {
                        self.noise.skeleton = Some(name.to_string());
                        *dirty = true;
                    }
                }
            });
        if self.noise.skeleton.is_some() {
            if ui
                .checkbox(&mut self.noise.compose, "compose with skeleton z")
                .changed()
            {
                *dirty = true;
            }
        } else {
            ui.weak("modulation params are inert without a skeleton");
        }

        ui.horizontal(|ui| {
            let mut seed = self.noise.cfg.seed as i64;
            ui.label("seed");
            if ui.add(egui::DragValue::new(&mut seed).range(0..=i64::MAX)).changed() {
                self.noise.cfg.seed = seed.max(0) as u64;
                *dirty = true;
            }
            if ui.button("next").clicked() {
                self.noise.cfg.seed = self.noise.cfg.seed.wrapping_add(1);
                *dirty = true;
            }
        });
        if ui.button("copy config JSON").clicked() {
            if let Ok(js) = serde_json::to_string_pretty(&self.noise.cfg) {
                ctx.copy_text(js);
            }
        }
        ui.separator();

        egui::ScrollArea::vertical().show(ui, |ui| {
            let c = &mut self.noise.cfg;
            ui.weak("● = Stage-1 recommended (KEEP) parameter");

            ui.label(egui::RichText::new("base band").strong());
            Self::log_slider(ui, dirty, "base amp (m) ●", &mut c.base_amp, 2.0..=60.0);
            Self::log_slider(ui, dirty, "base wavelength (m) ●", &mut c.base_wavelength, 100.0..=800.0);
            let mut oct = c.octaves as f64;
            Self::slider(ui, dirty, "octaves", &mut oct, 3.0..=8.0);
            c.octaves = oct.round() as u32;
            Self::slider(ui, dirty, "gain", &mut c.gain, 0.35..=0.75);
            Self::slider(ui, dirty, "lacunarity", &mut c.lacunarity, 1.8..=2.4);

            ui.separator();
            ui.label(egui::RichText::new("domain warp").strong());
            Self::slider(ui, dirty, "warp amp (m)", &mut c.warp_amp, 0.0..=300.0);
            Self::log_slider(ui, dirty, "warp wavelength (m) ●", &mut c.warp_wavelength, 150.0..=1500.0);
            Self::slider(ui, dirty, "warp2 amp (m)", &mut c.warp2_amp, 0.0..=120.0);

            ui.separator();
            ui.label(egui::RichText::new("shape").strong());
            Self::slider(ui, dirty, "ridged mix (+ridge/−billow) ●", &mut c.ridged_mix, -1.0..=1.0);
            Self::log_slider(ui, dirty, "redistribution ●", &mut c.redistribution, 0.3..=3.0);

            ui.separator();
            ui.label(egui::RichText::new("fine texture").strong());
            Self::log_slider(ui, dirty, "tex amp (m)", &mut c.tex_amp, 0.05..=3.0);
            Self::log_slider(ui, dirty, "tex wavelength (m)", &mut c.tex_wavelength, 8.0..=60.0);
            Self::slider(ui, dirty, "tex gain", &mut c.tex_gain, 0.4..=0.72);
            Self::slider(ui, dirty, "nugget amp (m)", &mut c.nugget_amp, 0.0..=0.5);

            ui.separator();
            ui.label(egui::RichText::new("anisotropy").strong());
            Self::slider(ui, dirty, "aniso ratio ●", &mut c.aniso_ratio, 1.0..=4.0);

            ui.separator();
            ui.label(egui::RichText::new("modulation (needs skeleton)").strong());
            Self::slider(ui, dirty, "floor damp ●", &mut c.floor_damp, 0.05..=1.0);
            Self::slider(ui, dirty, "slope gain", &mut c.slope_gain, -0.5..=1.0);
            Self::slider(ui, dirty, "grain align", &mut c.grain_align, 0.0..=1.0);
        });
    }
}

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.dirty {
            self.dirty = false;
            self.regen(ctx);
        }
        let mut dirty = false;

        egui::TopBottomPanel::top("tabs").show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (tab, label) in [
                    (Tab::Landform, "Landform (Stage 2)"),
                    (Tab::Noise, "Noise (Stage 1/4)"),
                ] {
                    if ui.selectable_label(self.tab == tab, label).clicked() && self.tab != tab {
                        self.tab = tab;
                        dirty = true;
                    }
                }
            });
        });

        egui::SidePanel::left("controls").min_width(330.0).show(ctx, |ui| {
            ui.heading("terrain-lab");
            ui.label(&self.stats);
            if ui.checkbox(&mut self.fast, "fast path (coarser grid)").changed() {
                dirty = true;
            }
            if ui.checkbox(&mut self.contours, "contours").changed() {
                dirty = true;
            }
            if self.contours {
                Self::slider(ui, &mut dirty, "contour step (m)", &mut self.contour_step, 0.5..=10.0);
            }
            ui.separator();
            match self.tab {
                Tab::Landform => self.landform_panel(ui, &mut dirty, ctx),
                Tab::Noise => self.noise_panel(ui, &mut dirty, ctx),
            }
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
