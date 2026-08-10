//! `tile-lab` — campaign tile QA viewer.
//!
//! Shows the real DTM tiles fetched by `tools/macro_campaign` with the
//! extractor's classifications drawn on top: which cells it called channel,
//! ridge, or basin; which basins it accepted vs rejected; where the
//! depression fill flattened the surface; which cross-sections produced the
//! valley width; which along-axis bands became the bench cascade. The point
//! is to answer "are we fitting the prior to good data?" by looking.
//!
//! Nothing here measures anything — every overlay is read back from
//! `out/extract/**` exactly as the fit consumed it. Bad tiles are culled
//! into `out/exclude.json`, which `fit_knobs.collect()` honors.
//!
//! Run: `cargo run -p tile-lab --release [-- <out_dir> [<skeleton_dir>]]`

mod data;

use std::collections::BTreeMap;
use std::path::PathBuf;

use course_world::Grid;
use data::{Excluded, Review, TileData, TileEntry};
use eframe::egui;

const IMG_PX: u32 = 1500;
const NODATA_RGB: [u8; 3] = [92, 92, 100];

/// The v2 corpus biomes — the E4 review queue covers these and nothing else
/// (v1-era archetype directories share the same store).
const V2_BIOMES: [&str; 6] = [
    "piedmont",
    "great_plains",
    "river_valley",
    "sandhills",
    "heathland",
    "hill_country",
];

/// One-click exclusion reasons for the things the OSM screen cannot see,
/// bound to number keys 1–6.
const QUICK_REASONS: [&str; 6] = [
    "agriculture (pivots/terracing)",
    "quarry/mine",
    "reservoir/dam",
    "lidar artifact (seams/stripes)",
    "graded/developed",
    "water dominant",
];

/// Overlay layers, in draw order. The `u8` is the classes.cgrid bit mask
/// (0 = vector layer drawn from regions.json instead).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Nodata,
    Developed,
    FillFlat,
    Channels,
    RidgesRaw,
    Agri,
}

/// Every layer states its calibration role — anything not feeding the
/// v2 calibration was removed from the UI (review request 2026-08).
const LAYERS: [(Layer, &str, u8, [u8; 4]); 6] = [
    (
        Layer::Agri,
        "agriculture (OSM) — EXCLUDED from texture harvest",
        64,
        [200, 170, 40, 110],
    ),
    (Layer::Nodata, "nodata", 1, [255, 0, 255, 160]),
    (
        Layer::Channels,
        "channels — SPACING/DENSITY calibration",
        2,
        [64, 132, 244, 130],
    ),
    (
        Layer::RidgesRaw,
        "ridges (geomorphon) — reference only, nothing fitted",
        4,
        [235, 140, 50, 80],
    ),
    (
        Layer::FillFlat,
        "fill flats — EXCLUDED from texture harvest",
        32,
        [200, 60, 200, 90],
    ),
    (
        Layer::Developed,
        "developed (OSM) — EXCLUDED from everything",
        128,
        [180, 30, 30, 130],
    ),
];

const DEFAULT_ON: [Layer; 3] = [Layer::Channels, Layer::FillFlat, Layer::Developed];

struct Lab {
    root: PathBuf,
    tiles: BTreeMap<String, Vec<TileEntry>>,
    sel: Option<(String, String)>,
    loaded: Option<TileData>,
    on: Vec<Layer>,
    contours: bool,
    contour_step: f64,
    excluded: Excluded,
    review: Review,
    exclude_reason: String,
    compare: Option<(PathBuf, Grid<f64>)>,
    shared_range: bool,
    tex: Option<egui::TextureHandle>,
    tex_cmp: Option<egui::TextureHandle>,
    dirty: bool,
    status: String,
}

impl Lab {
    fn new(root: PathBuf, skeleton: Option<PathBuf>) -> Self {
        let tiles = data::scan_tiles(&root);
        let excluded = data::load_exclude(&root);
        let review = data::load_review(&root);
        let sel = tiles
            .iter()
            .next()
            .map(|(a, ts)| (a.clone(), ts[0].id.clone()));
        // Generated-vs-real compare pane: accepts any CGRID1 heightfield (a
        // .cgrid file, or a directory containing base_height.cgrid) — no
        // dependency on any particular generator stage.
        let compare = skeleton.and_then(|dir| {
            let path = if dir.is_dir() { dir.join("base_height.cgrid") } else { dir.clone() };
            match course_world::gridio::read_grid_f32(&path) {
                Ok(g) => Some((dir, g)),
                Err(e) => {
                    eprintln!("tile-lab: compare grid {}: {e}", path.display());
                    None
                }
            }
        });
        let mut lab = Lab {
            root,
            tiles,
            sel,
            loaded: None,
            on: DEFAULT_ON.to_vec(),
            contours: true,
            contour_step: 5.0,
            excluded,
            review,
            exclude_reason: String::new(),
            compare,
            shared_range: true,
            tex: None,
            tex_cmp: None,
            dirty: true,
            status: String::new(),
        };
        // Open on the queue, not on the alphabetically-first (v1) archetype.
        if let Some(n) = lab.next_unreviewed(None) {
            lab.sel = Some(n);
        }
        lab
    }

    fn is_on(&self, l: Layer) -> bool {
        self.on.contains(&l)
    }

    fn toggle(&mut self, l: Layer, on: bool) {
        self.on.retain(|&x| x != l);
        if on {
            self.on.push(l);
        }
        self.dirty = true;
    }

    /// The E4 queue: every non-excluded tile in the six v2 biomes, in
    /// biome-then-id order. Recomputed on demand — exclusion shrinks it.
    fn queue(&self) -> Vec<(String, String)> {
        let mut q = Vec::new();
        for b in V2_BIOMES {
            if let Some(ts) = self.tiles.get(b) {
                for t in ts {
                    if !self.excluded.contains(b, &t.id) {
                        q.push((b.to_string(), t.id.clone()));
                    }
                }
            }
        }
        q
    }

    /// First not-yet-kept queue tile after `after` (wrapping), so the queue
    /// resumes wherever the reviewer is rather than restarting.
    fn next_unreviewed(&self, after: Option<&(String, String)>) -> Option<(String, String)> {
        let q = self.queue();
        let start = after
            .and_then(|a| q.iter().position(|x| x == a).map(|i| i + 1))
            .unwrap_or(0);
        q.iter()
            .cycle()
            .skip(start)
            .take(q.len())
            .find(|(b, id)| !self.review.contains(b, id))
            .cloned()
    }

    fn advance(&mut self) {
        if let Some(next) = self.next_unreviewed(self.sel.as_ref()) {
            self.sel = Some(next);
            self.exclude_reason.clear();
            self.dirty = true;
        }
    }

    fn decide_keep(&mut self) {
        let Some((a, id)) = self.sel.clone() else { return };
        if !V2_BIOMES.contains(&a.as_str()) || self.excluded.contains(&a, &id) {
            return;
        }
        self.review.set(&a, &id);
        if let Err(e) = data::save_review(&self.root, &self.review) {
            self.status = format!("review_v2.json: {e}");
        }
        self.advance();
    }

    fn decide_exclude(&mut self, reason: &str) {
        let Some((a, id)) = self.sel.clone() else { return };
        if !V2_BIOMES.contains(&a.as_str()) || self.excluded.contains(&a, &id) {
            return;
        }
        // Pick the successor while the current tile is still in the queue,
        // then drop it from both ledgers' points of view.
        let next = self.next_unreviewed(Some(&(a.clone(), id.clone())));
        self.review.remove(&a, &id);
        self.excluded.set(&a, &id, &format!("human: {reason}"));
        if let Err(e) = data::save_exclude(&self.root, &self.excluded) {
            self.status = format!("exclude.json: {e}");
        }
        if let Err(e) = data::save_review(&self.root, &self.review) {
            self.status = format!("review_v2.json: {e}");
        }
        if let Some(n) = next {
            self.sel = Some(n);
            self.exclude_reason.clear();
            self.dirty = true;
        }
    }

    fn unkeep(&mut self) {
        let Some((a, id)) = self.sel.clone() else { return };
        self.review.remove(&a, &id);
        if let Err(e) = data::save_review(&self.root, &self.review) {
            self.status = format!("review_v2.json: {e}");
        }
    }

    fn entry(&self) -> Option<&TileEntry> {
        let (a, id) = self.sel.as_ref()?;
        self.tiles.get(a)?.iter().find(|t| &t.id == id)
    }

    fn regen(&mut self, ctx: &egui::Context) {
        self.dirty = false;
        let Some(e) = self.entry().cloned() else {
            self.status = "no tiles found — run `python3 -m macro_campaign fetch`".into();
            return;
        };
        let data = match data::load_tile(&self.root, &e) {
            Ok(d) => d,
            Err(err) => {
                self.status = format!("{}: {err}", e.id);
                self.loaded = None;
                self.tex = None;
                return;
            }
        };

        let real_range = course_viz::finite_range(&data.height);
        let range = match (self.shared_range, &self.compare, real_range) {
            (true, Some((_, cmp)), Some((lo, hi))) => course_viz::finite_range(cmp)
                .map(|(a, b)| (lo.min(a), hi.max(b)))
                .or(Some((lo, hi))),
            _ => real_range,
        };

        let img = self.render(&data, range);
        let ci = egui::ColorImage::from_rgba_unmultiplied(
            [img.width() as usize, img.height() as usize],
            img.as_flat_samples().as_slice(),
        );
        self.tex = Some(ctx.load_texture("tile", ci, egui::TextureOptions::LINEAR));

        if let Some((_, cmp)) = &self.compare {
            let cimg = course_viz::render_height_grid(cmp, IMG_PX, range);
            let cci = egui::ColorImage::from_rgba_unmultiplied(
                [cimg.width() as usize, cimg.height() as usize],
                cimg.as_flat_samples().as_slice(),
            );
            self.tex_cmp = Some(ctx.load_texture("generated", cci, egui::TextureOptions::LINEAR));
        }

        let (lo, hi) = real_range.unwrap_or((0.0, 0.0));
        let nan = data.height.data.iter().filter(|v| !v.is_finite()).count();
        let prov = match &data.knobs {
            None => "  [no extract — run `extract`]".to_string(),
            Some(k) => format!(
                "  extract v{}  shape {}  valid {:.1}%",
                k.extract_version.unwrap_or(1),
                k.shape_version
                    .map_or("—".to_string(), |v| format!("v{v}")),
                k.valid_frac.unwrap_or(f64::NAN) * 100.0,
            ),
        };
        self.status = format!(
            "{}/{}  relief {:.1} m ({:.0}–{:.0})  {} nodata cells{}",
            e.archetype,
            e.id,
            hi - lo,
            lo,
            hi,
            nan,
            prov
        );
        self.loaded = Some(data);
    }

    fn render(&self, d: &TileData, range: Option<(f64, f64)>) -> image::RgbaImage {
        let mut img = course_viz::render_height_grid_nan(&d.height, IMG_PX, range, NODATA_RGB);
        if self.contours {
            course_viz::overlay_contours(&mut img, &d.height, self.contour_step);
        }
        // Raster layers, in LAYERS order so the palette stacks predictably.
        if let Some(classes) = &d.classes {
            for (layer, _, mask, color) in LAYERS {
                if mask != 0 && self.is_on(layer) {
                    course_viz::overlay_class_mask(&mut img, classes, mask, color);
                }
            }
        }
        // The raster masks ARE the data; the v1 vector decorations (channel
        // component BBOXES above all — the review's "perfect rectangles" —
        // basins, scarps, transects, centerlines) are gone with v1.
        img
    }

    /// The v2 extract summary: channel components + regional tilt.
    fn detections(&self) -> String {
        let Some(r) = self.loaded.as_ref().and_then(|d| d.regions.as_ref()) else {
            return String::new();
        };
        let mut s = format!("channel components ≥800 m: {}", r.channels.len());
        if let Some(t) = &r.tilt {
            s += &format!("
tilt grade {:.5}", t.grade);
            if let Some(d) = t.downhill_xy {
                s += &format!("  downhill ({:.2}, {:.2})", d[0], d[1]);
            }
        }
        s
    }

}

fn fmt_opt(v: Option<f64>) -> String {
    v.map_or("—".to_string(), |v| format!("{v:.3}"))
}

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Review hotkeys — dead while any text field has focus, so typing a
        // free-text reason can't fire decisions.
        if !ctx.wants_keyboard_input() {
            use egui::Key;
            if ctx.input(|i| i.key_pressed(Key::K)) {
                self.decide_keep();
            }
            if ctx.input(|i| i.key_pressed(Key::N)) {
                self.advance();
            }
            if ctx.input(|i| i.key_pressed(Key::U)) {
                self.unkeep();
            }
            const NUMS: [egui::Key; 6] =
                [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6];
            for (n, reason) in NUMS.iter().zip(QUICK_REASONS) {
                if ctx.input(|i| i.key_pressed(*n)) {
                    self.decide_exclude(reason);
                }
            }
        }

        egui::SidePanel::left("browser")
            .min_width(320.0)
            .show(ctx, |ui| {
              egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .id_salt("side-panel")
                .show(ui, |ui| {
                ui.heading("tile-lab — campaign QA");
                ui.label(egui::RichText::new(self.root.display().to_string()).small().weak());
                ui.separator();

                // ---- E4 review queue ----------------------------------
                ui.label(egui::RichText::new("E4 review queue (v2)").strong());
                let q = self.queue();
                let done = q.iter().filter(|(a, id)| self.review.contains(a, id)).count();
                if done == q.len() && !q.is_empty() {
                    ui.colored_label(
                        egui::Color32::from_rgb(120, 210, 120),
                        format!("queue complete — {done} kept"),
                    );
                } else {
                    ui.label(format!("{done} kept / {} in queue / {} to go", q.len(), q.len() - done));
                }
                ui.horizontal_wrapped(|ui| {
                    for b in V2_BIOMES {
                        let Some(ts) = self.tiles.get(b) else { continue };
                        let tot = ts.iter().filter(|t| !self.excluded.contains(b, &t.id)).count();
                        let k = ts.iter().filter(|t| self.review.contains(b, &t.id)).count();
                        ui.label(
                            egui::RichText::new(format!("{b} {k}/{tot}"))
                                .small()
                                .color(if k == tot && tot > 0 {
                                    egui::Color32::from_rgb(120, 210, 120)
                                } else {
                                    ui.visuals().text_color()
                                }),
                        );
                    }
                });
                ui.horizontal(|ui| {
                    if ui.button("KEEP (K)").clicked() {
                        self.decide_keep();
                    }
                    if ui.button("skip (N)").clicked() {
                        self.advance();
                    }
                    let kept_now = self
                        .sel
                        .as_ref()
                        .is_some_and(|(a, id)| self.review.contains(a, id));
                    if kept_now {
                        ui.colored_label(egui::Color32::from_rgb(120, 210, 120), "KEPT");
                        if ui.small_button("undo (U)").clicked() {
                            self.unkeep();
                        }
                    }
                });
                ui.label(egui::RichText::new("exclude as (1–6):").small());
                ui.horizontal_wrapped(|ui| {
                    for (n, reason) in QUICK_REASONS.iter().enumerate() {
                        if ui.small_button(format!("{} {reason}", n + 1)).clicked() {
                            self.decide_exclude(reason);
                        }
                    }
                });
                ui.separator();

                egui::ScrollArea::vertical()
                    .id_salt("tile-list")
                    .max_height(420.0)
                    .show(ui, |ui| {
                        let sel = self.sel.clone();
                        let mut pick: Option<(String, String)> = None;
                        for (arch, entries) in &self.tiles {
                            egui::CollapsingHeader::new(format!("{arch}  ({})", entries.len()))
                                .default_open(true)
                                .show(ui, |ui| {
                                    for e in entries {
                                        let is_sel = sel.as_ref()
                                            == Some(&(e.archetype.clone(), e.id.clone()));
                                        let excluded =
                                            self.excluded.contains(&e.archetype, &e.id);
                                        let kept = self.review.contains(&e.archetype, &e.id);
                                        let mut text = egui::RichText::new(if kept {
                                            format!("✓ {}", e.id)
                                        } else {
                                            e.id.clone()
                                        });
                                        if excluded {
                                            text = text.strikethrough().weak();
                                        }
                                        if ui.selectable_label(is_sel, text).clicked() && !is_sel {
                                            pick = Some((e.archetype.clone(), e.id.clone()));
                                        }
                                    }
                                });
                        }
                        if let Some(p) = pick {
                            self.sel = Some(p);
                            self.exclude_reason.clear();
                            self.dirty = true;
                        }
                    });

                ui.separator();
                ui.label("layers");
                for (layer, label, ..) in LAYERS {
                    let mut on = self.is_on(layer);
                    if ui.checkbox(&mut on, label).changed() {
                        self.toggle(layer, on);
                    }
                }
                if ui.checkbox(&mut self.contours, "contours").changed() {
                    self.dirty = true;
                }
                if self.contours
                    && ui
                        .add(
                            egui::Slider::new(&mut self.contour_step, 1.0..=25.0)
                                .text("step (m)"),
                        )
                        .changed()
                {
                    self.dirty = true;
                }
                if self.compare.is_some()
                    && ui
                        .checkbox(&mut self.shared_range, "shared elevation scale")
                        .changed()
                {
                    self.dirty = true;
                }

                ui.separator();
                if let Some((arch, id)) = self.sel.clone() {
                    if let Some(reason) = self.excluded.reason(&arch, &id).map(str::to_string) {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 110, 110),
                            format!("EXCLUDED: {reason}"),
                        );
                        if ui.button("re-include tile").clicked() {
                            self.excluded.remove(&arch, &id);
                            let _ = data::save_exclude(&self.root, &self.excluded);
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.label("exclude:");
                            ui.add(
                                egui::TextEdit::singleline(&mut self.exclude_reason)
                                    .hint_text("reason (town, quarry, reservoir…)")
                                    .desired_width(150.0),
                            );
                        });
                        let ready = !self.exclude_reason.trim().is_empty();
                        if ui
                            .add_enabled(ready, egui::Button::new("exclude from fit"))
                            .clicked()
                        {
                            self.excluded.set(&arch, &id, self.exclude_reason.trim());
                            if let Err(e) = data::save_exclude(&self.root, &self.excluded) {
                                self.status = format!("exclude.json: {e}");
                            }
                            self.exclude_reason.clear();
                        }
                    }
                }

                ui.separator();
                let det = self.detections();
                if !det.is_empty() {
                    ui.monospace(det);
                }

                ui.separator();
                if let Some(k) = self.loaded.as_ref().and_then(|d| d.knobs.as_ref()) {
                    // The v2 extract metrics for this tile (extract_v2
                    // writes them): spacing, density, band amplitudes.
                    let mut lines: Vec<String> = k
                        .knobs
                        .iter()
                        .map(|(n, v)| format!("{n:<24} {}", fmt_opt(*v)))
                        .collect();
                    lines.extend(
                        k.extras
                            .iter()
                            .map(|(n, v)| format!("{n:<24} {}", fmt_opt(*v))),
                    );
                    if !lines.is_empty() {
                        ui.monospace(lines.join("\n"));
                    }
                }
              });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            if self.dirty {
                self.regen(ctx);
            }
            ui.label(&self.status);
            let two = self.tex_cmp.is_some();
            let avail = ui.available_size();
            let side = if two {
                (avail.x / 2.0 - 8.0).min(avail.y - 24.0).max(64.0)
            } else {
                avail.x.min(avail.y - 24.0).max(64.0)
            };
            ui.horizontal(|ui| {
                if let Some(tex) = &self.tex {
                    ui.vertical(|ui| {
                        ui.image((tex.id(), egui::vec2(side, side)));
                        if two {
                            ui.label("real");
                        }
                    });
                }
                if let Some(tex) = &self.tex_cmp {
                    ui.vertical(|ui| {
                        ui.image((tex.id(), egui::vec2(side, side)));
                        ui.label("generated");
                    });
                }
            });
        });
    }
}


fn main() -> eframe::Result {
    let mut args = std::env::args().skip(1);
    let root = args.next().map(PathBuf::from).unwrap_or_else(default_root);
    let skeleton = args.next().map(PathBuf::from);
    let opts = eframe::NativeOptions {
        renderer: eframe::Renderer::Wgpu,
        viewport: egui::ViewportBuilder::default().with_inner_size([1400.0, 980.0]),
        ..Default::default()
    };
    eframe::run_native(
        "tile-lab — campaign tile QA",
        opts,
        Box::new(|_cc| Ok(Box::new(Lab::new(root, skeleton)))),
    )
}

fn default_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tools/macro_campaign/out")
}
