//! stage-lab — the per-stage pipeline viewer shell. One tab per built stage
//! (stage 01 framing today; mask/strokes/forcing slot in as they land), plus
//! a seed-sweep gallery. Regenerates in-process: framing is microseconds.
//!
//!   cargo run -p stage-lab --release
//!
//! The attempt scrubber exists to DEMONSTRATE stage-01 reroll stability:
//! framing is stable-scoped, so scrubbing attempt must not change the image.

mod data;
mod render;

use std::collections::BTreeMap;

use course_seed::{RunIdentity, MAX_ATTEMPTS, PIPELINE_VERSION};
use course_spec::ArchetypeId;
use course_framing::{BoundaryKind, WindowClass};
use data::{build_case, Case};
use eframe::egui;
use render::{render_framing_schematic, render_implied_terrain, ImpliedRelief};

const IMG_PX: u32 = 900;
/// Preview grid resolution in the interactive app (188² — a few ms).
const PREVIEW_RES_M: f64 = 16.0;
const THUMB_PX: u32 = 210;
const GALLERY_N: usize = 16;
/// Filter-mode scan bound: how many natural seeds to walk looking for
/// archetype matches before giving up a page.
const FILTER_SCAN_MAX: u64 = 4000;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 980.0]),
        ..Default::default()
    };
    eframe::run_native(
        "stage-lab — pipeline stages",
        options,
        Box::new(|_cc| Ok(Box::new(Lab::default()))),
    )
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Tab {
    Framing,
    Gallery,
}

const TABS: [(Tab, &str); 2] = [(Tab::Framing, "1 Framing"), (Tab::Gallery, "Gallery")];

fn window_label(w: WindowClass) -> &'static str {
    match w {
        WindowClass::ValleyFloor => "valley_floor",
        WindowClass::Interfluve => "interfluve",
        WindowClass::EscarpmentFace => "escarpment_face",
        WindowClass::BasinMargin => "basin_margin",
        WindowClass::PiedmontSlope => "piedmont_slope",
        WindowClass::TerraceFlight => "terrace_flight",
    }
}

fn kind_label(k: BoundaryKind) -> &'static str {
    match k {
        BoundaryKind::Scarp => "scarp",
        BoundaryKind::ValleyWall => "valley_wall",
        BoundaryKind::MaterialContact => "material_contact",
    }
}

struct Thumb {
    seed: u64,
    archetype: ArchetypeId,
    window: WindowClass,
    tex: egui::TextureHandle,
}

struct Lab {
    tab: Tab,
    seed: u64,
    attempt: u32,
    forced: Option<ArchetypeId>,
    overrides: BTreeMap<String, f64>,
    gallery_base: u64,
    gallery_filter: Option<ArchetypeId>,
    gallery_force: bool,
    show_implied: bool,
    show_contours: bool,
    dirty: bool,
    case: Option<Case>,
    err: Option<String>,
    tex: Option<egui::TextureHandle>,
    thumbs: Vec<Thumb>,
    stats: String,
}

impl Default for Lab {
    fn default() -> Self {
        Lab {
            tab: Tab::Framing,
            seed: 1,
            attempt: 0,
            forced: None,
            overrides: BTreeMap::new(),
            gallery_base: 0,
            gallery_filter: None,
            gallery_force: false,
            show_implied: false,
            show_contours: true,
            dirty: true,
            case: None,
            err: None,
            tex: None,
            thumbs: Vec::new(),
            stats: String::new(),
        }
    }
}

/// Preview amplitudes come from θ, never from the artifact (which is pure
/// geometry) — so the knob-override panel drives the preview live.
fn implied_relief(case: &Case) -> ImpliedRelief {
    ImpliedRelief {
        ridge_relief_m: case.spec.param("framing.topo_ridge_relief_m"),
        trunk_carve_m: case.spec.param("framing.topo_trunk_carve_m"),
        step_riser_m: case.spec.param("framing.topo_step_riser_m"),
        province_relief_m: case.spec.param("framing.province_relief_m"),
    }
}

fn load_tex(ctx: &egui::Context, name: &str, img: &image::RgbaImage) -> egui::TextureHandle {
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_flat_samples().as_slice());
    ctx.load_texture(name, ci, egui::TextureOptions::LINEAR)
}

impl Lab {
    fn identity(&self) -> RunIdentity {
        RunIdentity {
            seed: self.seed,
            pipeline_version: PIPELINE_VERSION,
            attempt: self.attempt,
        }
    }

    fn regen(&mut self, ctx: &egui::Context) {
        let t0 = std::time::Instant::now();
        match build_case(self.identity(), self.forced, &self.overrides) {
            Ok(case) => {
                let img = if self.show_implied {
                    render_implied_terrain(
                        &case.framing,
                        &implied_relief(&case),
                        IMG_PX,
                        PREVIEW_RES_M,
                        self.show_contours,
                    )
                } else {
                    render_framing_schematic(&case.framing, IMG_PX)
                };
                self.tex = Some(load_tex(ctx, "framing", &img));
                let f = &case.framing;
                self.stats = format!(
                    "{} seed {} attempt {} | {} | outlet {:?} @ {:.1} m | grade {:.4} | grain {:.0}° × {:.2} | provinces {}{} | {} ms",
                    case.spec.archetype.label(),
                    self.seed,
                    self.attempt,
                    window_label(f.window),
                    f.base_level.edge,
                    f.base_level.elev_m,
                    f.regional_tilt.grade,
                    f.grain.dir_rad.to_degrees(),
                    f.grain.anisotropy,
                    f.provinces.count,
                    f.provinces
                        .boundary
                        .as_ref()
                        .map(|b| format!(" ({})", kind_label(b.kind)))
                        .unwrap_or_default(),
                    t0.elapsed().as_millis()
                );
                self.case = Some(case);
                self.err = None;
            }
            Err(e) => {
                self.err = Some(e);
            }
        }
        if self.tab == Tab::Gallery {
            self.regen_gallery(ctx);
        }
        self.dirty = false;
    }

    fn regen_gallery(&mut self, ctx: &egui::Context) {
        self.thumbs.clear();
        let mut seed = self.gallery_base;
        let mut scanned = 0u64;
        while self.thumbs.len() < GALLERY_N && scanned < FILTER_SCAN_MAX {
            scanned += 1;
            let id = RunIdentity {
                seed,
                pipeline_version: PIPELINE_VERSION,
                attempt: self.attempt,
            };
            let forced = if self.gallery_force {
                self.gallery_filter
            } else {
                None
            };
            seed = seed.wrapping_add(1);
            let Ok(case) = build_case(id, forced, &self.overrides) else {
                continue;
            };
            if !self.gallery_force {
                if let Some(want) = self.gallery_filter {
                    if case.spec.archetype != want {
                        continue;
                    }
                }
            }
            let img = render_framing_schematic(&case.framing, THUMB_PX);
            self.thumbs.push(Thumb {
                seed: id.seed,
                archetype: case.spec.archetype,
                window: case.framing.window,
                tex: load_tex(ctx, &format!("thumb-{}", id.seed), &img),
            });
        }
    }

    fn side_text(&self) -> String {
        let Some(case) = &self.case else {
            return String::new();
        };
        let json = serde_json::to_string_pretty(&case.framing).unwrap_or_default();
        format!(
            "archetype: {}\nhydrology: {:?}\nprior: {}\n\n{json}",
            case.spec.archetype.label(),
            case.spec.hydrology_mode,
            case.spec.prior_version,
        )
    }
}

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("controls").min_width(320.0).show(ctx, |ui| {
            ui.heading("stage-lab");
            ui.horizontal(|ui| {
                ui.label("seed");
                if ui.add(egui::DragValue::new(&mut self.seed).speed(1)).changed() {
                    self.dirty = true;
                }
                if ui.button("−1").clicked() {
                    self.seed = self.seed.wrapping_sub(1);
                    self.dirty = true;
                }
                if ui.button("+1").clicked() {
                    self.seed = self.seed.wrapping_add(1);
                    self.dirty = true;
                }
            });
            ui.horizontal(|ui| {
                ui.label("attempt");
                if ui
                    .add(egui::DragValue::new(&mut self.attempt).range(0..=MAX_ATTEMPTS - 1))
                    .changed()
                {
                    self.dirty = true;
                }
                ui.label("(framing is stable-scoped: must not change)");
            });

            let forced_label = self
                .forced
                .map(|a| a.label().to_string())
                .unwrap_or_else(|| "natural".into());
            egui::ComboBox::from_label("archetype")
                .selected_text(forced_label)
                .show_ui(ui, |ui| {
                    if ui.selectable_value(&mut self.forced, None, "natural").changed() {
                        self.overrides.clear();
                        self.dirty = true;
                    }
                    for a in ArchetypeId::ALL {
                        if ui.selectable_value(&mut self.forced, Some(a), a.label()).changed() {
                            self.overrides.clear();
                            self.dirty = true;
                        }
                    }
                });

            ui.separator();
            for (t, label) in TABS {
                if ui.selectable_label(self.tab == t, label).clicked() && self.tab != t {
                    self.tab = t;
                    self.dirty = true;
                }
            }

            if self.tab == Tab::Framing {
                ui.separator();
                if ui
                    .checkbox(&mut self.show_implied, "implied terrain (illustrative)")
                    .changed()
                {
                    self.dirty = true;
                }
                if self.show_implied
                    && ui.checkbox(&mut self.show_contours, "contours").changed()
                {
                    self.dirty = true;
                }
            }

            if self.tab == Tab::Gallery {
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("base seed");
                    if ui
                        .add(egui::DragValue::new(&mut self.gallery_base).speed(1))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    if ui.button("next page").clicked() {
                        self.gallery_base = self.gallery_base.wrapping_add(GALLERY_N as u64);
                        self.dirty = true;
                    }
                });
                let filter_label = self
                    .gallery_filter
                    .map(|a| a.label().to_string())
                    .unwrap_or_else(|| "all".into());
                egui::ComboBox::from_label("gallery archetype")
                    .selected_text(filter_label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut self.gallery_filter, None, "all").changed() {
                            self.dirty = true;
                        }
                        for a in ArchetypeId::ALL {
                            if ui
                                .selectable_value(&mut self.gallery_filter, Some(a), a.label())
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                    });
                if self.gallery_filter.is_some()
                    && ui
                        .checkbox(&mut self.gallery_force, "force (vs filter natural draws)")
                        .changed()
                {
                    self.dirty = true;
                }
            }

            ui.separator();
            ui.collapsing("framing knob overrides", |ui| {
                if let Some(case) = &self.case {
                    let knobs: Vec<(String, f64)> = case
                        .spec
                        .params
                        .iter()
                        .filter(|(k, _)| k.starts_with("framing."))
                        .map(|(k, v)| (k.clone(), *v))
                        .collect();
                    for (k, sampled) in knobs {
                        let mut val = *self.overrides.get(&k).unwrap_or(&sampled);
                        let overridden = self.overrides.contains_key(&k);
                        ui.horizontal(|ui| {
                            let name = k.trim_start_matches("framing.");
                            let label = if overridden {
                                format!("* {name}")
                            } else {
                                name.to_string()
                            };
                            ui.label(label);
                            let speed = (sampled.abs() * 0.02).max(0.001);
                            if ui.add(egui::DragValue::new(&mut val).speed(speed)).changed() {
                                self.overrides.insert(k.clone(), val);
                                self.dirty = true;
                            }
                        });
                    }
                    if !self.overrides.is_empty() && ui.button("clear overrides").clicked() {
                        self.overrides.clear();
                        self.dirty = true;
                    }
                }
            });

            ui.separator();
            if let Some(err) = &self.err {
                ui.colored_label(egui::Color32::LIGHT_RED, err);
            }
            if self.tab == Tab::Framing {
                let side = self.side_text();
                if !side.is_empty() {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.monospace(side);
                    });
                }
            }
        });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.dirty {
                self.regen(ctx);
            }
            ui.label(&self.stats);
            match self.tab {
                Tab::Framing => {
                    if self.show_implied {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 190, 90),
                            "illustrative implied-terrain preview — not a pipeline artifact \
                             (stage 04 compiles the real forcing fields; stage 05 finishes)",
                        );
                    }
                    if let Some(tex) = &self.tex {
                        let avail = ui.available_size();
                        let side = avail.x.min(avail.y - 24.0).max(64.0);
                        ui.image((tex.id(), egui::vec2(side, side)));
                    }
                }
                Tab::Gallery => {
                    let mut open_seed = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("gallery").spacing([8.0, 8.0]).show(ui, |ui| {
                            for (i, t) in self.thumbs.iter().enumerate() {
                                ui.vertical(|ui| {
                                    let resp = ui
                                        .image((t.tex.id(), egui::vec2(THUMB_PX as f32, THUMB_PX as f32)))
                                        .interact(egui::Sense::click());
                                    if resp.clicked() {
                                        open_seed = Some(t.seed);
                                    }
                                    ui.label(format!(
                                        "{} · {} · {}",
                                        t.seed,
                                        t.archetype.label(),
                                        window_label(t.window)
                                    ));
                                });
                                if (i + 1) % 4 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                        if self.thumbs.is_empty() {
                            ui.label("no matches in scan range — raise base seed or use force");
                        }
                    });
                    if let Some(seed) = open_seed {
                        self.seed = seed;
                        self.forced = if self.gallery_force { self.gallery_filter } else { None };
                        self.tab = Tab::Framing;
                        self.dirty = true;
                    }
                }
            }
        });
    }
}
