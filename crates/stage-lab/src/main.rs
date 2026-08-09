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

use course_contracts::biome::{BiomeId, BoundaryKind as KindV2, StructureClass, WindowClass as ClassV2};
use course_contracts::contracts::primitive_field::PrimitiveField;
use course_seed::{RunIdentity, MAX_ATTEMPTS, PIPELINE_VERSION};
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_spec::ArchetypeId;
use course_framing::{BoundaryKind, WindowClass};
use data::{build_case, Case};
use eframe::egui;
use render::{render_framing_schematic, render_implied_terrain, ImpliedRelief};
use stage_lab::render_s2::{render_s2, S2View, S2_VIEWS};
use stage_lab::render_v2::{render_c1, C1View, C1_VIEWS};

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
    SpecV2,
    C1,
    C1Gallery,
    S2,
    S2Gallery,
    Framing,
    Gallery,
}

const TABS: [(Tab, &str); 7] = [
    (Tab::SpecV2, "0 Spec (v2)"),
    (Tab::C1, "1 C1 (S1)"),
    (Tab::C1Gallery, "C1 gallery"),
    (Tab::S2, "2 S2 (skeleton)"),
    (Tab::S2Gallery, "S2 gallery"),
    (Tab::Framing, "framing (v1)"),
    (Tab::Gallery, "v1 gallery"),
];

fn class_label(w: ClassV2) -> &'static str {
    match w {
        ClassV2::ValleyFloor => "valley_floor",
        ClassV2::Interfluve => "interfluve",
        ClassV2::EscarpmentFace => "escarpment_face",
        ClassV2::BasinMargin => "basin_margin",
        ClassV2::PiedmontSlope => "piedmont_slope",
        ClassV2::TerraceFlight => "terrace_flight",
    }
}

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

struct C1Thumb {
    seed: u64,
    biome: BiomeId,
    class: ClassV2,
    forced_class: bool,
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
    // ---- v2 ----
    forced_biome: Option<BiomeId>,
    c1_view: C1View,
    c1_overlays: bool,
    v2_spec: Option<SiteSpec>,
    c1: Option<PrimitiveField>,
    c1_tex: Option<egui::TextureHandle>,
    c1_thumbs: Vec<C1Thumb>,
    c1_gallery_forced: bool,
    spec_rows: Vec<String>,
    s2_view: S2View,
    s2_overlays: bool,
    s2_tex: Option<egui::TextureHandle>,
    s2_thumbs: Vec<S2Thumb>,
}

struct S2Thumb {
    seed: u64,
    biome: BiomeId,
    tex: egui::TextureHandle,
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
            forced_biome: None,
            c1_view: C1View::Implied,
            c1_overlays: true,
            v2_spec: None,
            c1: None,
            c1_tex: None,
            c1_thumbs: Vec::new(),
            c1_gallery_forced: true,
            spec_rows: Vec::new(),
            s2_view: S2View::Base,
            s2_overlays: true,
            s2_tex: None,
            s2_thumbs: Vec::new(),
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
        match self.tab {
            Tab::SpecV2 => self.regen_spec_v2(),
            Tab::C1 => self.regen_c1(ctx),
            Tab::C1Gallery => self.regen_c1_gallery(ctx),
            Tab::S2 => self.regen_s2(ctx),
            Tab::S2Gallery => self.regen_s2_gallery(ctx),
            _ => {}
        }
        self.dirty = false;
    }

    fn v2_case(&self, seed: u64, forced_class: Option<ClassV2>) -> (SiteSpec, PrimitiveField) {
        let id = RunIdentity::from_seed(seed);
        let ov = SpecOverridesV2 {
            forced_biome: self.forced_biome,
        };
        let mut spec = SiteSpec::generate_builtin(id, &ov);
        if let Some(class) = forced_class {
            // Lab-only: realize a chosen class with this seed's other draws
            // (the same construction the S1 distinctness tests use).
            spec.structure_class = StructureClass::new(
                class,
                spec.structure_class.provinces,
                spec.structure_class.boundary_kind,
            )
            .unwrap();
        }
        let c1 = course_primitives::generate(&spec, &id);
        (spec, c1)
    }

    fn regen_spec_v2(&mut self) {
        let (spec, _c1) = self.v2_case(self.seed, None);
        self.spec_rows.clear();
        for s in self.seed..self.seed + 14 {
            let (row, _) = self.v2_case(s, None);
            self.spec_rows.push(format!(
                "{:>6}  {:14}  {:16}  {}p{}  relief {:5.1} m  dens {:5.3}  plast {:.2}  wind {:3.0}° @ {:4.1} m/s",
                s,
                row.biome.key(),
                class_label(row.structure_class.window),
                row.structure_class.provinces,
                row.structure_class
                    .boundary_kind
                    .map(|k| match k {
                        KindV2::Scarp => " scarp",
                        KindV2::ValleyWall => " wall",
                        KindV2::MaterialContact => " contact",
                    })
                    .unwrap_or(""),
                row.descriptors.relief_budget_m,
                row.descriptors.density_target,
                row.descriptors.plasticity.value(),
                row.descriptors.wind_azimuth_rad.to_degrees(),
                row.descriptors.wind_speed_mps,
            ));
        }
        self.stats = format!(
            "S0 v2 · seed {} · envelope {}…",
            self.seed,
            &spec.envelope_fingerprint[..12]
        );
        self.v2_spec = Some(spec);
    }

    fn regen_c1(&mut self, ctx: &egui::Context) {
        let t0 = std::time::Instant::now();
        let (spec, c1) = self.v2_case(self.seed, None);
        let img = render_c1(&c1, self.c1_view, IMG_PX, self.c1_overlays);
        self.c1_tex = Some(load_tex(ctx, "c1", &img));
        self.stats = format!(
            "S1 · seed {} · {} · {} · {}p · edge {:?} @ {:.1} m · grain {:.0}°×{:.2} · wind {:.0}° · {} ms",
            self.seed,
            spec.biome.key(),
            class_label(spec.structure_class.window),
            spec.structure_class.provinces,
            c1.meta.base_level.edge,
            c1.meta.base_level.elev_m,
            c1.meta.grain_axis_rad.to_degrees(),
            c1.meta.grain_strength,
            c1.meta.wind_azimuth_rad.to_degrees(),
            t0.elapsed().as_millis()
        );
        self.v2_spec = Some(spec);
        self.c1 = Some(c1);
    }

    fn regen_c1_gallery(&mut self, ctx: &egui::Context) {
        self.c1_thumbs.clear();
        if self.c1_gallery_forced {
            // Grouped by class: 3 examples per class, class forced so every
            // row exists — the material behind the legibility session.
            for (ci, &class) in ClassV2::ALL.iter().enumerate() {
                for k in 0..3u64 {
                    let seed = self.gallery_base + ci as u64 * 3 + k;
                    let (spec, c1) = self.v2_case(seed, Some(class));
                    let img = render_c1(&c1, C1View::Implied, THUMB_PX, false);
                    self.c1_thumbs.push(C1Thumb {
                        seed,
                        biome: spec.biome,
                        class,
                        forced_class: true,
                        tex: load_tex(ctx, &format!("c1t-{ci}-{k}"), &img),
                    });
                }
            }
        } else {
            // Natural draws, first come first shown.
            for k in 0..(GALLERY_N as u64) {
                let seed = self.gallery_base + k;
                let (spec, c1) = self.v2_case(seed, None);
                let img = render_c1(&c1, C1View::Implied, THUMB_PX, false);
                self.c1_thumbs.push(C1Thumb {
                    seed,
                    biome: spec.biome,
                    class: spec.structure_class.window,
                    forced_class: false,
                    tex: load_tex(ctx, &format!("c1n-{k}"), &img),
                });
            }
        }
    }

    fn regen_s2(&mut self, ctx: &egui::Context) {
        let t0 = std::time::Instant::now();
        let (spec, c1) = self.v2_case(self.seed, None);
        let id = RunIdentity::from_seed(self.seed);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let img = render_s2(&sk, self.s2_view, IMG_PX, self.s2_overlays);
        self.s2_tex = Some(load_tex(ctx, "s2", &img));
        let d = &sk.diagnostics;
        self.stats = format!(
            "S2 · seed {} · {} · {} channels (Ω {}) · dens {:.2}/{:.2} km/km² · rb {} · rl {} · conn {:.2} · {} embryos · {} ms",
            self.seed,
            spec.biome.key(),
            d.channel_count,
            sk.channels.iter().map(|c| c.order).max().unwrap_or(0),
            d.achieved_density_km_km2,
            d.target_density_km_km2,
            d.bifurcation_ratio.map_or("—".into(), |v| format!("{v:.1}")),
            d.length_ratio.map_or("—".into(), |v| format!("{v:.1}")),
            d.connectivity,
            sk.embryos.len(),
            t0.elapsed().as_millis()
        );
        self.v2_spec = Some(spec);
    }

    fn regen_s2_gallery(&mut self, ctx: &egui::Context) {
        self.s2_thumbs.clear();
        // Grouped by biome: 3 seeds per biome, biome forced so every row
        // exists — the S2 P1-review material.
        for (bi, &biome) in BiomeId::ALL.iter().enumerate() {
            for k in 0..3u64 {
                let seed = self.gallery_base + bi as u64 * 3 + k;
                let id = RunIdentity::from_seed(seed);
                let spec = SiteSpec::generate_builtin(
                    id,
                    &SpecOverridesV2 { forced_biome: Some(biome) },
                );
                let c1 = course_primitives::generate(&spec, &id);
                let sk = course_skeleton::generate(&spec, &c1, &id);
                let img = render_s2(&sk, S2View::Base, THUMB_PX, true);
                self.s2_thumbs.push(S2Thumb {
                    seed,
                    biome,
                    tex: load_tex(ctx, &format!("s2t-{bi}-{k}"), &img),
                });
            }
        }
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

            if matches!(self.tab, Tab::SpecV2 | Tab::C1 | Tab::C1Gallery | Tab::S2 | Tab::S2Gallery) {
                ui.separator();
                let biome_label = self
                    .forced_biome
                    .map(|b| b.key().to_string())
                    .unwrap_or_else(|| "natural".into());
                egui::ComboBox::from_label("biome (v2)")
                    .selected_text(biome_label)
                    .show_ui(ui, |ui| {
                        if ui.selectable_value(&mut self.forced_biome, None, "natural").changed() {
                            self.dirty = true;
                        }
                        for b in BiomeId::ALL {
                            if ui
                                .selectable_value(&mut self.forced_biome, Some(b), b.key())
                                .changed()
                            {
                                self.dirty = true;
                            }
                        }
                    });
            }
            if self.tab == Tab::C1 {
                for (v, label) in C1_VIEWS {
                    if ui.selectable_label(self.c1_view == v, label).clicked() {
                        self.c1_view = v;
                        self.dirty = true;
                    }
                }
                if ui.checkbox(&mut self.c1_overlays, "overlays").changed() {
                    self.dirty = true;
                }
            }
            if self.tab == Tab::S2 {
                for (v, label) in S2_VIEWS {
                    if ui.selectable_label(self.s2_view == v, label).clicked() {
                        self.s2_view = v;
                        self.dirty = true;
                    }
                }
                if ui.checkbox(&mut self.s2_overlays, "overlays").changed() {
                    self.dirty = true;
                }
            }
            if self.tab == Tab::S2Gallery {
                ui.horizontal(|ui| {
                    ui.label("base seed");
                    if ui
                        .add(egui::DragValue::new(&mut self.gallery_base).speed(1))
                        .changed()
                    {
                        self.dirty = true;
                    }
                    if ui.button("next page").clicked() {
                        self.gallery_base = self.gallery_base.wrapping_add(18);
                        self.dirty = true;
                    }
                });
            }
            if self.tab == Tab::C1Gallery {
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
                if ui
                    .checkbox(&mut self.c1_gallery_forced, "grouped by class (forced)")
                    .changed()
                {
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
            if matches!(self.tab, Tab::SpecV2 | Tab::C1 | Tab::S2) {
                if let Some(spec) = &self.v2_spec {
                    let json = serde_json::to_string_pretty(spec).unwrap_or_default();
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        ui.monospace(json);
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
                Tab::SpecV2 => {
                    ui.colored_label(
                        egui::Color32::from_rgb(160, 200, 160),
                        "S0 v2 draw table — 14 seeds from the current one",
                    );
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        for row in &self.spec_rows {
                            ui.monospace(row);
                        }
                    });
                }
                Tab::C1 => {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 190, 90),
                        "C1 is PREDISPOSITION — no drainage, no texture (S2/S3's jobs).                          Judge class legibility and field coherence only; see C1_REVIEW.md.",
                    );
                    if let Some(tex) = &self.c1_tex {
                        let avail = ui.available_size();
                        let side = avail.x.min(avail.y - 40.0).max(64.0);
                        ui.image((tex.id(), egui::vec2(side, side)));
                    }
                }
                Tab::C1Gallery => {
                    let mut open_seed = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("c1gallery").spacing([8.0, 8.0]).show(ui, |ui| {
                            for (i, t) in self.c1_thumbs.iter().enumerate() {
                                ui.vertical(|ui| {
                                    let resp = ui
                                        .image((t.tex.id(), egui::vec2(THUMB_PX as f32, THUMB_PX as f32)))
                                        .interact(egui::Sense::click());
                                    if resp.clicked() {
                                        open_seed = Some(t.seed);
                                    }
                                    ui.label(format!(
                                        "{} · {} · {}{}",
                                        t.seed,
                                        t.biome.key(),
                                        class_label(t.class),
                                        if t.forced_class { " (forced)" } else { "" }
                                    ));
                                });
                                if (i + 1) % 3 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                    if let Some(seed) = open_seed {
                        self.seed = seed;
                        self.tab = Tab::C1;
                        self.dirty = true;
                    }
                }
                Tab::S2 => {
                    ui.colored_label(
                        egui::Color32::from_rgb(220, 190, 90),
                        "S2 is STRUCTURE — candle-wax smoothness between channels is                          EXPECTED (texture is S3's job). Judge the network: space-filling,                          hierarchical, obeying base level and discontinuities; divides                          derived, embryos recorded.",
                    );
                    if let Some(tex) = &self.s2_tex {
                        let avail = ui.available_size();
                        let side = avail.x.min(avail.y - 40.0).max(64.0);
                        ui.image((tex.id(), egui::vec2(side, side)));
                    }
                }
                Tab::S2Gallery => {
                    let mut open = None;
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        egui::Grid::new("s2gallery").spacing([8.0, 8.0]).show(ui, |ui| {
                            for (i, t) in self.s2_thumbs.iter().enumerate() {
                                ui.vertical(|ui| {
                                    let resp = ui
                                        .image((t.tex.id(), egui::vec2(THUMB_PX as f32, THUMB_PX as f32)))
                                        .interact(egui::Sense::click());
                                    if resp.clicked() {
                                        open = Some((t.seed, t.biome));
                                    }
                                    ui.label(format!("{} · {}", t.seed, t.biome.key()));
                                });
                                if (i + 1) % 3 == 0 {
                                    ui.end_row();
                                }
                            }
                        });
                    });
                    if let Some((seed, biome)) = open {
                        self.seed = seed;
                        self.forced_biome = Some(biome);
                        self.tab = Tab::S2;
                        self.dirty = true;
                    }
                }
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
