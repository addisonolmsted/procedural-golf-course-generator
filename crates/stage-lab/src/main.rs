//! stage-lab — the per-stage pipeline viewer. One tab per built v2 stage:
//! S0 spec table, C1 (S1 predisposition fields) + gallery, S2 (skeleton)
//! + gallery. Regenerates in-process (S2 is the slowest at ~150 ms).
//!
//!   cargo run -p stage-lab --release
//!
//! Review discipline: C1_REVIEW.md and S2_REVIEW.md say what to judge at
//! each stage and what belongs to a later one.

use course_contracts::biome::{
    BiomeId, BoundaryKind as KindV2, StructureClass, WindowClass as ClassV2,
};
use course_contracts::contracts::primitive_field::PrimitiveField;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use eframe::egui;
use stage_lab::render_s2::{render_s2, S2View, S2_VIEWS};
use stage_lab::render_v2::{render_c1, C1View, C1_VIEWS};

const IMG_PX: u32 = 900;
const THUMB_PX: u32 = 210;
const GALLERY_N: usize = 16;

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
}

const TABS: [(Tab, &str); 5] = [
    (Tab::SpecV2, "0 Spec"),
    (Tab::C1, "1 C1 (S1)"),
    (Tab::C1Gallery, "C1 gallery"),
    (Tab::S2, "2 S2 (skeleton)"),
    (Tab::S2Gallery, "S2 gallery"),
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

struct C1Thumb {
    seed: u64,
    biome: BiomeId,
    class: ClassV2,
    forced_class: bool,
    tex: egui::TextureHandle,
}

struct S2Thumb {
    seed: u64,
    biome: BiomeId,
    tex: egui::TextureHandle,
}

struct Lab {
    tab: Tab,
    seed: u64,
    gallery_base: u64,
    dirty: bool,
    stats: String,
    forced_biome: Option<BiomeId>,
    c1_view: C1View,
    c1_overlays: bool,
    v2_spec: Option<SiteSpec>,
    c1_tex: Option<egui::TextureHandle>,
    c1_thumbs: Vec<C1Thumb>,
    c1_gallery_forced: bool,
    spec_rows: Vec<String>,
    s2_view: S2View,
    s2_overlays: bool,
    s2_tex: Option<egui::TextureHandle>,
    s2_thumbs: Vec<S2Thumb>,
}

impl Default for Lab {
    fn default() -> Self {
        Lab {
            tab: Tab::SpecV2,
            seed: 1,
            gallery_base: 0,
            dirty: true,
            stats: String::new(),
            forced_biome: None,
            c1_view: C1View::Implied,
            c1_overlays: true,
            v2_spec: None,
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

fn load_tex(ctx: &egui::Context, name: &str, img: &image::RgbaImage) -> egui::TextureHandle {
    let size = [img.width() as usize, img.height() as usize];
    let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_flat_samples().as_slice());
    ctx.load_texture(name, ci, egui::TextureOptions::LINEAR)
}

impl Lab {
    fn regen(&mut self, ctx: &egui::Context) {
        match self.tab {
            Tab::SpecV2 => self.regen_spec_v2(),
            Tab::C1 => self.regen_c1(ctx),
            Tab::C1Gallery => self.regen_c1_gallery(ctx),
            Tab::S2 => self.regen_s2(ctx),
            Tab::S2Gallery => self.regen_s2_gallery(ctx),
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
            "S0 · seed {} · envelope {}…",
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
}

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("controls").min_width(320.0).show(ctx, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
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

                    ui.separator();
                    for (t, label) in TABS {
                        if ui.selectable_label(self.tab == t, label).clicked() && self.tab != t {
                            self.tab = t;
                            self.dirty = true;
                        }
                    }

                    ui.separator();
                    let biome_label = self
                        .forced_biome
                        .map(|b| b.key().to_string())
                        .unwrap_or_else(|| "natural".into());
                    egui::ComboBox::from_label("biome")
                        .selected_text(biome_label)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(&mut self.forced_biome, None, "natural")
                                .changed()
                            {
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
                    if matches!(self.tab, Tab::C1Gallery | Tab::S2Gallery) {
                        ui.horizontal(|ui| {
                            ui.label("base seed");
                            if ui
                                .add(egui::DragValue::new(&mut self.gallery_base).speed(1))
                                .changed()
                            {
                                self.dirty = true;
                            }
                            if ui.button("next page").clicked() {
                                self.gallery_base =
                                    self.gallery_base.wrapping_add(GALLERY_N as u64);
                                self.dirty = true;
                            }
                        });
                    }
                    if self.tab == Tab::C1Gallery
                        && ui
                            .checkbox(&mut self.c1_gallery_forced, "grouped by class (forced)")
                            .changed()
                    {
                        self.dirty = true;
                    }

                    if matches!(self.tab, Tab::SpecV2 | Tab::C1 | Tab::S2) {
                        ui.separator();
                        if let Some(spec) = &self.v2_spec {
                            let json = serde_json::to_string_pretty(spec).unwrap_or_default();
                            ui.monospace(json);
                        }
                    }
                });
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
                        "S0 draw table — 14 seeds from the current one",
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
                        "C1 is PREDISPOSITION — no drainage, no texture (S2/S3's jobs). \
                         Judge class legibility and field coherence only; see C1_REVIEW.md.",
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
                                        .image((
                                            t.tex.id(),
                                            egui::vec2(THUMB_PX as f32, THUMB_PX as f32),
                                        ))
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
                        "S2 is STRUCTURE — candle-wax smoothness between channels is \
                         EXPECTED (texture is S3's job). Judge the network: space-filling, \
                         hierarchical, obeying base level and discontinuities; see \
                         S2_REVIEW.md.",
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
                                        .image((
                                            t.tex.id(),
                                            egui::vec2(THUMB_PX as f32, THUMB_PX as f32),
                                        ))
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
            }
        });
    }
}
