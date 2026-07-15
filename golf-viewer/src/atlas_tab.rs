//! Atlas tab — browse the real Parkland Atlas survey.
//!
//! Shows each surveyed course's hillshaded terrain with toggleable overlays:
//! **terrain / tree canopy / water / hole routes**. The in-app companion to the
//! standalone `parkland_atlas.html`; reads the same packed `atlas.bin`, so it
//! scales to the full 200-course set and sits beside Explore/Match/Hole for
//! A/B comparison against generated terrain.

use crate::to_color_image;
use eframe::egui;
use golf_atlas::{Atlas, Course, StyleGroup};
use golf_core::math::Vec2;
use std::sync::Arc;

/// Long-edge pixels of the course render.
const VIEW_PX: u32 = 760;

struct AtlasView {
    /// (course, terrain, tree, water, holes) this texture was built for.
    key: (usize, bool, bool, bool, bool),
    tex: egui::TextureHandle,
}

pub struct AtlasTab {
    atlas: Result<Arc<Atlas>, String>,
    sel: usize,
    terrain: bool,
    trees: bool,
    water: bool,
    holes: bool,
    view: Option<AtlasView>,
}

impl AtlasTab {
    pub fn new() -> Self {
        let path = golf_atlas::binfmt::default_bin_path();
        let atlas = golf_atlas::binfmt::load(&path).map(Arc::new).map_err(|e| {
            format!(
                "Cannot load the course atlas from {}:\n{e}\n\n\
                 Pack it once with:\n  cargo run -p xtask -- atlas-pack <parkland_atlas.html>",
                path.display()
            )
        });
        AtlasTab {
            atlas,
            sel: 0,
            terrain: true,
            trees: true,
            water: true,
            holes: true,
            view: None,
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        let atlas = match &self.atlas {
            Ok(a) => Arc::clone(a),
            Err(msg) => {
                let msg = msg.clone();
                egui::CentralPanel::default().show(ctx, |ui| {
                    ui.add_space(40.0);
                    ui.vertical_centered(|ui| ui.label(msg));
                });
                return;
            }
        };

        egui::SidePanel::left("atlas_courses")
            .exact_width(250.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.course_rail(ui, &atlas));
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.layer_bar(ui);
            ui.separator();
            self.ensure_view(ctx, &atlas);
            self.course_view(ui, &atlas);
        });
    }

    fn course_rail(&mut self, ui: &mut egui::Ui, atlas: &Atlas) {
        ui.heading("Courses");
        ui.small(format!("Parkland Atlas survey · {} courses", atlas.courses.len()));
        for group in [StyleGroup::Lowland, StyleGroup::Rolling, StyleGroup::Mountain] {
            let n = atlas.courses.iter().filter(|c| c.group == group).count();
            if n == 0 {
                continue;
            }
            ui.add_space(6.0);
            ui.label(
                egui::RichText::new(format!("{} ({n})", group.label().to_uppercase()))
                    .small()
                    .weak(),
            );
            ui.separator();
            for (i, c) in atlas.courses.iter().enumerate() {
                if c.group != group {
                    continue;
                }
                let line = format!(
                    "{}\nrelief {:.0} m · canopy {:.0}% · {} holes",
                    c.label,
                    c.relief(),
                    c.treepct,
                    c.holes.len()
                );
                if ui.selectable_label(self.sel == i, line).clicked() {
                    self.sel = i;
                }
            }
        }
    }

    fn layer_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.strong("Layers:");
            ui.checkbox(&mut self.terrain, "Terrain");
            ui.checkbox(&mut self.trees, "Tree canopy");
            ui.checkbox(&mut self.water, "Water");
            ui.checkbox(&mut self.holes, "Holes");
        });
    }

    fn ensure_view(&mut self, ctx: &egui::Context, atlas: &Atlas) {
        let key = (self.sel, self.terrain, self.trees, self.water, self.holes);
        if self.view.as_ref().map(|v| v.key) == Some(key) {
            return;
        }
        let c = &atlas.courses[self.sel];
        let mpp = c.wm.max(c.hm) / VIEW_PX as f64;

        // Base: hillshaded terrain, or a flat dark ground when terrain is off.
        let grid = c.window_grid(c.wm, c.hm, mpp);
        let mut img = golf_viz::render_height_grid(&grid, VIEW_PX, None);
        if !self.terrain {
            for px in img.pixels_mut() {
                *px = image::Rgba([16, 22, 18, 255]);
            }
        }
        if self.trees {
            golf_viz::tint_tree_mask(&mut img, &c.tree_window(c.wm, c.hm, mpp));
        }
        if self.water {
            golf_viz::tint_water_mask(&mut img, &c.water_window(c.wm, c.hm, mpp));
        }
        if self.holes {
            golf_viz::draw_atlas_holes(&mut img, &hole_polylines(c), c.wm, c.hm);
        }
        let tex = ctx.load_texture(
            "atlas_view",
            to_color_image(&img),
            egui::TextureOptions::LINEAR,
        );
        self.view = Some(AtlasView { key, tex });
    }

    fn course_view(&mut self, ui: &mut egui::Ui, atlas: &Atlas) {
        let c = &atlas.courses[self.sel];
        ui.horizontal(|ui| {
            ui.heading(&c.label);
            if !c.arch.is_empty() {
                ui.label(egui::RichText::new(&c.arch).italics().weak());
            }
        });
        ui.label(format!(
            "Survey {:.0} × {:.0} m · relief {:.1} m ({:.0}–{:.0} m) · canopy {:.0}% · water {:.1}% · {} holes",
            c.wm, c.hm, c.relief(), c.emin, c.emax, c.treepct, c.waterpct, c.holes.len()
        ));
        ui.add_space(6.0);
        if let Some(v) = &self.view {
            let s = v.tex.size();
            let tsize = egui::vec2(s[0] as f32, s[1] as f32);
            let avail = ui.available_size();
            let scale = (avail.x / tsize.x).min(avail.y / tsize.y).clamp(0.05, 2.0);
            ui.add(
                egui::Image::from_texture(egui::load::SizedTexture::new(v.tex.id(), tsize))
                    .fit_to_exact_size(tsize * scale),
            );
        }
    }
}

/// Project a course's holes (lat/lon) into window-local world meters
/// (origin SW, x east, y north) for `golf_viz::draw_atlas_holes`.
fn hole_polylines(c: &Course) -> Vec<Vec<Vec2>> {
    let [la0, lo0, la1, lo1] = c.bbox;
    let dlat = (la1 - la0).max(1e-9);
    let dlon = (lo1 - lo0).max(1e-9);
    c.holes
        .iter()
        .map(|h| {
            h.pts_ll
                .iter()
                .map(|&(lat, lon)| {
                    Vec2::new(
                        (lon - lo0) / dlon * c.wm,
                        (lat - la0) / dlat * c.hm, // metres from SOUTH edge
                    )
                })
                .collect()
        })
        .collect()
}
