//! Hole viewer tab — one built hole at a time, zoomed to its bounding box
//! and rotated so the opening shot points up, sampled at 0.5 m per pixel
//! from the composed built surface (25× finer than the macro grid).

use crate::to_color_image;
use eframe::egui;
use golf_terrain::{generate_course, macro_spec, CourseTerrain};

/// Layout resolution (the height lattice under the contours stays 0.5 m).
const MPP: f64 = 0.1;

struct CourseData {
    seed: u64,
    ct: CourseTerrain,
    routing: Result<golf_routing::Routing, String>,
    build: Option<golf_holes::CourseBuild>,
    mode: &'static str,
}

pub struct HoleViewTab {
    seed: u64,
    auto_sync: bool,
    hole: usize,
    contours: bool,
    data: Option<CourseData>,
    tex: Option<egui::TextureHandle>,
    tex_hole: usize,
    tex_contours: bool,
    info: Vec<String>,
    dirty: bool,
}

impl HoleViewTab {
    pub fn new() -> Self {
        HoleViewTab {
            seed: 2024,
            auto_sync: true,
            hole: 0,
            contours: true,
            data: None,
            tex: None,
            tex_hole: usize::MAX,
            tex_contours: true,
            info: Vec::new(),
            dirty: true,
        }
    }

    fn ensure_data(&mut self, seed: u64) {
        if self.data.as_ref().map(|d| d.seed) == Some(seed) {
            return;
        }
        let (ct, sp) = generate_course(&macro_spec(), seed);
        let routing = golf_routing::route(&ct, seed).map_err(|e| e.reason.to_string());
        let build = routing.as_ref().ok().map(|r| golf_holes::build(&ct, r, seed));
        self.data = Some(CourseData {
            seed,
            ct,
            routing,
            build,
            mode: sp.mode.label(),
        });
        self.tex = None;
        self.tex_hole = usize::MAX;
    }

    fn ensure_texture(&mut self, ctx: &egui::Context) {
        let Some(d) = &self.data else { return };
        if self.tex.is_some() && self.tex_hole == self.hole && self.tex_contours == self.contours {
            return;
        }
        self.info.clear();
        match (&d.routing, &d.build) {
            (Ok(r), Some(b)) => {
                let img = golf_viz::render_hole_view(&d.ct, b, r, self.hole, MPP, self.contours);
                self.tex = Some(ctx.load_texture(
                    "hole_view",
                    to_color_image(&img),
                    egui::TextureOptions::LINEAR,
                ));
                self.tex_hole = self.hole;
                self.tex_contours = self.contours;
                let rh = &r.holes[self.hole];
                let hb = &b.holes[self.hole];
                self.info.push(format!(
                    "hole {}    par {} · {:.0} m (⌖ {:.0})",
                    self.hole + 1,
                    rh.par,
                    rh.len,
                    rh.target_len
                ));
                self.info.push(format!(
                    "green     {:.0} m² · pinnable {:.0}% · pin tier {}",
                    hb.green.area,
                    100.0 * hb.pinnable_share,
                    hb.pin_tier
                ));
                self.info.push(format!(
                    "tee pad   {:.1} × {:.1} m · lift {:+.2} m",
                    2.0 * hb.tee.a,
                    2.0 * hb.tee.b,
                    hb.tee.z - d.ct.macro_heights.bilinear(hb.tee.center)
                ));
                if hb.fw.is_none() {
                    self.info.push(format!(
                        "fairway   surround-only (par 3) · walk to next {:.0} m",
                        rh.walk_len
                    ));
                } else {
                    self.info.push(format!(
                        "fairway   starts {:.0} m out · walk to next {:.0} m",
                        hb.fw.start * hb.spine.length(),
                        rh.walk_len
                    ));
                }
                self.info.push(format!("seed {} · {}", d.seed, d.mode));
            }
            (Err(reason), _) => {
                self.tex = None;
                self.info.push(format!("seed {} UNROUTABLE ({reason})", d.seed));
            }
            _ => {}
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, explore_seed: u64) {
        if self.auto_sync && explore_seed != self.seed {
            self.seed = explore_seed;
            self.dirty = true;
        }
        if self.dirty {
            self.dirty = false;
            self.ensure_data(self.seed);
        }
        self.ensure_texture(ctx);

        egui::SidePanel::left("hole_view_controls")
            .exact_width(270.0)
            .show(ctx, |ui| {
                ui.heading("Hole viewer");
                ui.label("flat 2D · 0.1 m layout · shot points up");
                ui.separator();
                ui.checkbox(&mut self.auto_sync, "Sync seed from Explore");
                ui.horizontal(|ui| {
                    ui.label("Seed");
                    let resp = ui.add_enabled(
                        !self.auto_sync,
                        egui::DragValue::new(&mut self.seed).speed(1.0),
                    );
                    if resp.changed() {
                        self.dirty = true;
                    }
                });
                ui.checkbox(&mut self.contours, "Contour lines (1 m)");
                ui.separator();
                ui.label("Hole");
                let pars: Option<[u8; 9]> = self
                    .data
                    .as_ref()
                    .and_then(|d| d.routing.as_ref().ok().map(|r| r.par_seq));
                egui::Grid::new("hole_picker").spacing([6.0, 6.0]).show(ui, |ui| {
                    for i in 0..9usize {
                        let label = match pars {
                            Some(p) => format!("{}\npar {}", i + 1, p[i]),
                            None => format!("{}", i + 1),
                        };
                        if ui
                            .selectable_label(self.hole == i, egui::RichText::new(label).monospace())
                            .clicked()
                        {
                            self.hole = i;
                        }
                        if i % 3 == 2 {
                            ui.end_row();
                        }
                    }
                });
                ui.separator();
                for line in &self.info {
                    ui.monospace(line.clone());
                }
                ui.separator();
                ui.small("white: line of play, tee pad, green edge · flag: pin");
                ui.small("arrow: north · bar: 50 m");
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            let Some(tex) = &self.tex else {
                ui.centered_and_justified(|ui| {
                    ui.label(
                        self.info
                            .first()
                            .cloned()
                            .unwrap_or_else(|| "generating…".into()),
                    );
                });
                return;
            };
            let avail = ui.available_size();
            let s = tex.size();
            let tsize = egui::vec2(s[0] as f32, s[1] as f32);
            let scale = (avail.x / tsize.x).min(avail.y / tsize.y).min(2.0);
            ui.centered_and_justified(|ui| {
                ui.add(
                    egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), tsize))
                        .fit_to_exact_size(tsize * scale),
                );
            });
        });
    }
}
