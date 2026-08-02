//! macro-lab — the STEP-03-ONLY viewer. Renders every macro-landform
//! construction stage (frame → drainage → uplands → basins → budget →
//! terrain → fields) for a scrubbable seed. Deliberately knows nothing about
//! steps 04+ (each step builds its own viewer on `course-viz`).
//!
//!   cargo run -p macro-lab --release

use std::collections::BTreeMap;

use course_macro::plan::Edge;
use course_macro::skeleton::SpineKind;
use course_macro::{generate_skeleton, MacroResult};
use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_world::math::Vec2;
use course_world::world::EXTENT_M;
use eframe::egui;

const IMG_PX: u32 = 900;

const DRAIN: [u8; 4] = [64, 132, 244, 255];
const RIDGE: [u8; 4] = [235, 140, 50, 255];
const SCARP: [u8; 4] = [220, 60, 60, 255];
const BASIN: [u8; 4] = [90, 200, 220, 255];
const CHORD: [u8; 4] = [250, 250, 250, 220];

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1360.0, 960.0]),
        ..Default::default()
    };
    eframe::run_native(
        "macro-lab — step 03 macro landform",
        options,
        Box::new(|_cc| Ok(Box::new(Lab::default()))),
    )
}

#[derive(PartialEq, Clone, Copy, Debug)]
enum Tab {
    Frame,
    Drainage,
    Uplands,
    Basins,
    Budget,
    Terrain,
    Fields,
}

const TABS: [(Tab, &str); 7] = [
    (Tab::Frame, "1 Frame"),
    (Tab::Drainage, "2 Drainage"),
    (Tab::Uplands, "3 Uplands"),
    (Tab::Basins, "4 Basins"),
    (Tab::Budget, "5 Budget"),
    (Tab::Terrain, "6 Terrain"),
    (Tab::Fields, "7 Fields"),
];

#[derive(PartialEq, Clone, Copy, Debug)]
enum FieldView {
    FloorDamp,
    SlopeGain,
    GrainDir,
    ValleyDist,
    CoreProtect,
}

const FIELD_VIEWS: [(FieldView, &str); 5] = [
    (FieldView::FloorDamp, "floor_damp"),
    (FieldView::SlopeGain, "slope_gain"),
    (FieldView::GrainDir, "grain_dir_rad"),
    (FieldView::ValleyDist, "valley_dist_m"),
    (FieldView::CoreProtect, "core_protect"),
];

struct Lab {
    tab: Tab,
    field_view: FieldView,
    seed: u64,
    archetype: ArchetypeId,
    fast: bool,
    contours: bool,
    contour_step: f64,
    overrides: BTreeMap<String, f64>,
    dirty: bool,
    result: Option<MacroResult>,
    spec: Option<CourseSpec>,
    tex: Option<egui::TextureHandle>,
    stats: String,
}

impl Default for Lab {
    fn default() -> Self {
        Lab {
            tab: Tab::Terrain,
            field_view: FieldView::FloorDamp,
            seed: 7,
            archetype: ArchetypeId::Piedmont,
            fast: true,
            contours: true,
            contour_step: 2.0,
            overrides: BTreeMap::new(),
            dirty: true,
            result: None,
            spec: None,
            tex: None,
            stats: String::new(),
        }
    }
}

impl Lab {
    fn res_m(&self) -> f64 {
        if self.fast { 16.0 } else { 4.0 }
    }

    fn regen(&mut self, ctx: &egui::Context) {
        let t0 = std::time::Instant::now();
        let ov = SpecOverrides {
            forced_archetype: Some(self.archetype),
            params: self.overrides.clone(),
        };
        let spec = CourseSpec::generate_builtin(RunIdentity::from_seed(self.seed), &ov)
            .expect("spec generation");
        let result = generate_skeleton(&spec, self.res_m());
        let img = self.render(&result);
        let g = &result.skeleton.base_height;
        let (lo, hi) = g
            .data
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), &v| {
                (l.min(v), h.max(v))
            });
        let b = &result.plan.budget;
        self.stats = format!(
            "{} seed {} | {}×{} @ {} m | z [{lo:.1}, {hi:.1}] | core {:.1}/{:.1} m (raw {:.1}) | {} ms",
            self.archetype.label(),
            self.seed,
            g.spec.nx,
            g.spec.ny,
            self.res_m(),
            b.relief_final_m,
            b.cap_m,
            b.relief_raw_m,
            t0.elapsed().as_millis()
        );
        let size = [img.width() as usize, img.height() as usize];
        let ci = egui::ColorImage::from_rgba_unmultiplied(size, img.as_flat_samples().as_slice());
        self.tex = Some(ctx.load_texture("macro", ci, egui::TextureOptions::LINEAR));
        self.result = Some(result);
        self.spec = Some(spec);
        self.dirty = false;
    }

    fn render(&self, r: &MacroResult) -> image::RgbaImage {
        let s = &r.skeleton;
        let mut img = match self.tab {
            Tab::Fields => match self.field_view {
                FieldView::FloorDamp => {
                    course_viz::render_scalar_field(&s.fields.floor_damp, IMG_PX, 0.0, 1.0)
                }
                FieldView::SlopeGain => {
                    course_viz::render_scalar_field(&s.fields.slope_gain, IMG_PX, 0.0, 1.0)
                }
                FieldView::GrainDir => {
                    course_viz::render_direction_field(&s.fields.grain_dir_rad, IMG_PX)
                }
                FieldView::ValleyDist => {
                    course_viz::render_scalar_field(&s.fields.valley_dist_m, IMG_PX, 0.0, 800.0)
                }
                FieldView::CoreProtect => {
                    course_viz::render_scalar_field(&s.fields.core_protect, IMG_PX, 0.0, 1.0)
                }
            },
            _ => course_viz::render_height_grid(&s.base_height, IMG_PX, None),
        };

        if self.contours && matches!(self.tab, Tab::Terrain | Tab::Drainage | Tab::Uplands) {
            course_viz::overlay_contours(&mut img, &s.base_height, self.contour_step);
        }

        let plan = &r.plan;
        match self.tab {
            Tab::Frame => {
                // Downhill arrow from the center + grain strokes on a lattice.
                let c = Vec2::new(1500.0, 1500.0);
                let d = plan.frame.downhill;
                course_viz::draw_polyline(&mut img, &[c, c + d * 600.0], CHORD, 1);
                course_viz::draw_disc(&mut img, c + d * 600.0, 30.0, CHORD);
                let g = &s.fields.grain_dir_rad;
                let mut y = 150.0;
                while y < EXTENT_M {
                    let mut x = 150.0;
                    while x < EXTENT_M {
                        let p = Vec2::new(x, y);
                        let a = g.bilinear(p);
                        let v = Vec2::new(course_world::math::cos(a), course_world::math::sin(a));
                        course_viz::draw_polyline(
                            &mut img,
                            &[p - v * 55.0, p + v * 55.0],
                            [230, 230, 230, 160],
                            0,
                        );
                        x += 220.0;
                    }
                    y += 220.0;
                }
                if let Some(e) = plan.frame.exit_edge {
                    let mid = match e {
                        Edge::West => Vec2::new(40.0, 1500.0),
                        Edge::East => Vec2::new(EXTENT_M - 40.0, 1500.0),
                        Edge::South => Vec2::new(1500.0, 40.0),
                        Edge::North => Vec2::new(1500.0, EXTENT_M - 40.0),
                    };
                    course_viz::draw_disc(&mut img, mid, 40.0, DRAIN);
                }
            }
            Tab::Drainage => {
                // Meander chords + resolved drain spines + junction dots.
                for v in &plan.drainage.valleys {
                    if let course_macro::Path::Meander(m) = &v.path {
                        course_viz::draw_polyline(&mut img, &[m.entry, m.exit], CHORD, 0);
                    }
                }
                for sp in &s.structure.spines {
                    if sp.kind == SpineKind::Drain {
                        course_viz::draw_polyline(&mut img, &sp.pts, DRAIN, 1);
                        if let Some(last) = sp.pts.last() {
                            course_viz::draw_disc(&mut img, *last, 25.0, DRAIN);
                        }
                    }
                }
            }
            Tab::Uplands => {
                for sp in &s.structure.spines {
                    match sp.kind {
                        SpineKind::RidgeLine => {
                            course_viz::draw_polyline(&mut img, &sp.pts, RIDGE, 1)
                        }
                        SpineKind::BenchEdge => {
                            course_viz::draw_polyline(&mut img, &sp.pts, SCARP, 1)
                        }
                        SpineKind::Drain => {
                            course_viz::draw_polyline(&mut img, &sp.pts, DRAIN, 0)
                        }
                    }
                }
                // Highlight the reserved core tread band.
                if plan.uplands.core_tread_w_m > 0.0 {
                    let c = Vec2::new(1500.0, 1500.0);
                    let along = plan.frame.downhill.perp();
                    let hw = (plan.uplands.core_tread_w_m * 0.5).max(250.0);
                    for side in [-1.0, 1.0] {
                        let o = plan.frame.downhill * (hw * side);
                        course_viz::draw_polyline(
                            &mut img,
                            &[c + o - along * 1100.0, c + o + along * 1100.0],
                            [255, 255, 160, 180],
                            0,
                        );
                    }
                }
            }
            Tab::Basins => {
                for bp in &plan.basins.bowls {
                    let (center, radius) = bowl_center_radius(&bp.bowl);
                    let depth_t =
                        ((bp.bowl.depth_m * bp.depth_scale) / 12.0).clamp(0.15, 1.0);
                    let col = [
                        (BASIN[0] as f64 * depth_t) as u8,
                        (BASIN[1] as f64 * depth_t) as u8,
                        (BASIN[2] as f64 * depth_t) as u8,
                        255,
                    ];
                    course_viz::draw_circle(&mut img, center, radius, col, 1);
                    if let course_macro::Outlet::Spillway { at_s, .. } = bp.bowl.outlet {
                        let a = at_s * std::f64::consts::TAU;
                        let p = center
                            + Vec2::new(course_world::math::cos(a), course_world::math::sin(a))
                                * radius;
                        course_viz::draw_disc(&mut img, p, 22.0, [255, 255, 255, 255]);
                    }
                }
                for sp in &s.structure.spines {
                    if sp.kind == SpineKind::Drain {
                        course_viz::draw_polyline(&mut img, &sp.pts, DRAIN, 0);
                    }
                }
            }
            Tab::Budget => {
                course_viz::dim_outside_core(&mut img);
            }
            Tab::Terrain => {
                for sp in &s.structure.spines {
                    let (c, t) = match sp.kind {
                        SpineKind::Drain => (DRAIN, 1),
                        SpineKind::RidgeLine => (RIDGE, 0),
                        SpineKind::BenchEdge => (SCARP, 0),
                    };
                    course_viz::draw_polyline(&mut img, &sp.pts, c, t);
                }
            }
            Tab::Fields => {}
        }
        course_viz::draw_core_box(&mut img);
        img
    }

    fn side_text(&self) -> String {
        let Some(r) = &self.result else {
            return String::new();
        };
        match self.tab {
            Tab::Frame => serde_json::to_string_pretty(&r.plan.frame).unwrap_or_default(),
            Tab::Budget => serde_json::to_string_pretty(&r.plan.budget).unwrap_or_default(),
            Tab::Drainage => format!(
                "valleys: {} (tangent_routed: {})\n\n{}",
                r.plan.drainage.valleys.len(),
                r.plan.drainage.tangent_routed,
                r.plan
                    .drainage
                    .valleys
                    .iter()
                    .enumerate()
                    .map(|(i, v)| format!(
                        "[{i}] fall {:.4}  hw {:.0}-{:.0} m  walls {:.2}/{:.2}{}",
                        v.fall_gradient,
                        v.floor_halfwidth.knots.first().map(|k| k.1).unwrap_or(0.0),
                        v.floor_halfwidth.knots.last().map(|k| k.1).unwrap_or(0.0),
                        v.wall_grad_left,
                        v.wall_grad_right,
                        if v.join_trunk.is_some() { "  (trib)" } else { "  (trunk)" }
                    ))
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            Tab::Uplands => format!(
                "ridges: {}\nscarps: {}\ncore tread: {:.0} m",
                r.plan.uplands.ridges.len(),
                r.plan.uplands.scarps.len(),
                r.plan.uplands.core_tread_w_m
            ),
            Tab::Basins => format!(
                "basins: {}\n{}",
                r.plan.basins.bowls.len(),
                r.plan
                    .basins
                    .bowls
                    .iter()
                    .enumerate()
                    .map(|(i, b)| {
                        let (_, rad) = bowl_center_radius(&b.bowl);
                        format!(
                            "[{i}] r {:.0} m  depth {:.1} m × {:.2}  {}",
                            rad,
                            b.bowl.depth_m,
                            b.depth_scale,
                            match b.bowl.outlet {
                                course_macro::Outlet::Lake => "lake",
                                course_macro::Outlet::Spillway { .. } => "spillway",
                            }
                        )
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            ),
            _ => String::new(),
        }
    }
}

fn bowl_center_radius(b: &course_macro::Bowl) -> (Vec2, f64) {
    match &b.boundary {
        course_macro::BowlBoundary::Blob {
            center, radius_m, ..
        } => (*center, *radius_m),
        course_macro::BowlBoundary::Points(pts) => {
            let n = pts.len().max(1) as f64;
            let c = pts
                .iter()
                .fold(Vec2::ZERO, |acc, p| acc + *p)
                * (1.0 / n);
            let r = pts.iter().map(|p| p.distance(c)).sum::<f64>() / n;
            (c, r)
        }
    }
}

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("controls").min_width(300.0).show(ctx, |ui| {
            ui.heading("step 03 — macro landform");
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

            egui::ComboBox::from_label("archetype")
                .selected_text(self.archetype.label())
                .show_ui(ui, |ui| {
                    for a in ArchetypeId::ALL {
                        if ui
                            .selectable_value(&mut self.archetype, a, a.label())
                            .changed()
                        {
                            self.overrides.clear();
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
                    .add(
                        egui::Slider::new(&mut self.contour_step, 0.5..=10.0)
                            .text("contour step (m)"),
                    )
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
            if self.tab == Tab::Fields {
                ui.separator();
                for (f, label) in FIELD_VIEWS {
                    if ui.selectable_label(self.field_view == f, label).clicked()
                        && self.field_view != f
                    {
                        self.field_view = f;
                        self.dirty = true;
                    }
                }
            }

            ui.separator();
            ui.collapsing("landform knob overrides", |ui| {
                if let Some(spec) = &self.spec {
                    let knobs: Vec<(String, f64)> = spec
                        .params
                        .iter()
                        .filter(|(k, _)| k.starts_with("landform."))
                        .map(|(k, v)| (k.clone(), *v))
                        .collect();
                    for (k, sampled) in knobs {
                        let mut val = *self.overrides.get(&k).unwrap_or(&sampled);
                        let overridden = self.overrides.contains_key(&k);
                        ui.horizontal(|ui| {
                            let name = k.trim_start_matches("landform.");
                            let label = if overridden {
                                format!("* {name}")
                            } else {
                                name.to_string()
                            };
                            ui.label(label);
                            let speed = (sampled.abs() * 0.02).max(0.001);
                            if ui
                                .add(egui::DragValue::new(&mut val).speed(speed))
                                .changed()
                            {
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
                let side = avail.x.min(avail.y - 24.0).max(64.0);
                ui.image((tex.id(), egui::vec2(side, side)));
            }
        });
    }
}
