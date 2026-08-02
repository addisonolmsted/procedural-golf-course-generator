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

use course_spec::{ArchetypeId, Priors};
use course_world::math::Vec2;
use course_world::Grid;
use data::{Excluded, Regions, TileData, TileEntry};
use eframe::egui;

const IMG_PX: u32 = 900;
const NODATA_RGB: [u8; 3] = [92, 92, 100];

/// Overlay layers, in draw order. The `u8` is the classes.cgrid bit mask
/// (0 = vector layer drawn from regions.json instead).
#[derive(Clone, Copy, PartialEq, Eq)]
enum Layer {
    Nodata,
    Developed,
    FillFlat,
    Channels,
    RidgesRaw,
    RidgesAccepted,
    Basins,
    BasinsRejected,
    Scarps,
    Transects,
    Centerlines,
}

const LAYERS: [(Layer, &str, u8, [u8; 4]); 11] = [
    (Layer::Nodata, "nodata", 1, [255, 0, 255, 160]),
    (Layer::Developed, "developed (OSM roads/buildings)", 128, [180, 30, 30, 130]),
    (Layer::FillFlat, "fill flats (lakes/pits)", 32, [200, 60, 200, 90]),
    (Layer::Channels, "channels", 2, [64, 132, 244, 130]),
    (Layer::RidgesRaw, "ridges (geomorphon)", 4, [235, 140, 50, 80]),
    (Layer::RidgesAccepted, "ridges (accepted)", 64, [235, 140, 50, 150]),
    (Layer::Basins, "basins (accepted)", 8, [90, 200, 220, 140]),
    (Layer::BasinsRejected, "basins (rejected)", 16, [120, 120, 120, 110]),
    (Layer::Scarps, "scarps (terrace edges)", 0, [220, 60, 60, 255]),
    (Layer::Transects, "valley transects", 0, [255, 240, 120, 255]),
    (Layer::Centerlines, "valley centerlines", 0, [40, 240, 200, 255]),
];

/// Layers on by default: the three that answer "did it find the landforms".
const DEFAULT_ON: [Layer; 4] =
    [Layer::Channels, Layer::Basins, Layer::Scarps, Layer::Developed];

struct Lab {
    root: PathBuf,
    tiles: BTreeMap<String, Vec<TileEntry>>,
    sel: Option<(String, String)>,
    loaded: Option<TileData>,
    on: Vec<Layer>,
    contours: bool,
    contour_step: f64,
    excluded: Excluded,
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
        let sel = tiles
            .iter()
            .next()
            .map(|(a, ts)| (a.clone(), ts[0].id.clone()));
        let compare = skeleton.and_then(|dir| match course_macro::artifact::load_skeleton(&dir) {
            Ok((sk, _)) => Some((dir, sk.base_height)),
            Err(e) => {
                eprintln!("tile-lab: skeleton {}: {e}", dir.display());
                None
            }
        });
        Lab {
            root,
            tiles,
            sel,
            loaded: None,
            on: DEFAULT_ON.to_vec(),
            contours: false,
            contour_step: 5.0,
            excluded,
            exclude_reason: String::new(),
            compare,
            shared_range: true,
            tex: None,
            tex_cmp: None,
            dirty: true,
            status: String::new(),
        }
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

    fn entry(&self) -> Option<&TileEntry> {
        let (a, id) = self.sel.as_ref()?;
        self.tiles.get(a)?.iter().find(|t| &t.id == id)
    }

    fn archetype_id(&self) -> Option<ArchetypeId> {
        let (a, _) = self.sel.as_ref()?;
        ArchetypeId::ALL.into_iter().find(|x| x.key() == a)
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
        let Some(r) = &d.regions else { return img };

        if self.is_on(Layer::Scarps) {
            for sc in &r.scarps {
                if sc.centerline_m.len() < 2 {
                    continue;
                }
                let pts: Vec<Vec2> = sc.centerline_m.iter().map(|p| pt(*p)).collect();
                // Accepted risers read solid; candidates that missed the
                // height/length gate are drawn thin and dim.
                let (c, thick) = if sc.accepted {
                    ([220, 60, 60, 255], 1)
                } else {
                    ([170, 120, 120, 255], 0)
                };
                course_viz::draw_polyline(&mut img, &pts, c, thick);
            }
        }
        if self.is_on(Layer::Basins) || self.is_on(Layer::BasinsRejected) {
            for b in &r.basins {
                let show = if b.accepted {
                    self.is_on(Layer::Basins)
                } else {
                    self.is_on(Layer::BasinsRejected)
                };
                if !show {
                    continue;
                }
                let c = if b.accepted {
                    [90, 200, 220, 255]
                } else {
                    [140, 140, 140, 255]
                };
                course_viz::draw_circle(&mut img, pt(b.center_m), b.radius_m, c, 0);
            }
        }
        if self.is_on(Layer::Channels) {
            // Bounding boxes of the components that counted as valleys —
            // `valley_count` is exactly len(these).
            let c = [64, 132, 244, 255];
            for ch in &r.channels {
                let [x0, y0, x1, y1] = ch.bbox_m;
                let box_pts = [
                    Vec2::new(x0, y0),
                    Vec2::new(x1, y0),
                    Vec2::new(x1, y1),
                    Vec2::new(x0, y1),
                    Vec2::new(x0, y0),
                ];
                course_viz::draw_polyline(&mut img, &box_pts, c, 0);
            }
        }
        if self.is_on(Layer::Transects) {
            let c = layer_color(Layer::Transects);
            for t in &r.transects {
                let ctr = pt(t.center_m);
                let dir = Vec2::new(t.perp_xy[0], t.perp_xy[1]);
                course_viz::draw_polyline(&mut img, &[ctr, ctr + dir * t.hw_m], c, 0);
            }
        }
        if self.is_on(Layer::RidgesAccepted) {
            let c = [255, 170, 70, 255];
            for rr in &r.ridges {
                if rr.kind == "accepted" && rr.centerline_m.len() > 1 {
                    let pts: Vec<Vec2> = rr.centerline_m.iter().map(|p| pt(*p)).collect();
                    course_viz::draw_polyline(&mut img, &pts, c, 1);
                }
            }
        }
        if self.is_on(Layer::Centerlines) {
            let c = layer_color(Layer::Centerlines);
            for cl in &r.valley_centerlines {
                if cl.pts_m.len() > 1 {
                    let pts: Vec<Vec2> = cl.pts_m.iter().map(|p| pt(*p)).collect();
                    course_viz::draw_polyline(&mut img, &pts, c, 1);
                }
            }
        }
        img
    }

    /// What the detectors found, as counts — the fast "is this tile even
    /// the right landscape?" read before looking at the overlays.
    fn detections(&self) -> String {
        let Some(r) = self.loaded.as_ref().and_then(|d| d.regions.as_ref()) else {
            return String::new();
        };
        let accepted = r.basins.iter().filter(|b| b.accepted).count();
        let mut s = format!(
            "channels {}  ridges {}  basins {} ({} rejected)  scarps {}/{}  transects {}",
            r.channels.len(),
            r.ridges.len(),
            accepted,
            r.basins.len() - accepted,
            r.scarps.iter().filter(|s| s.accepted).count(),
            r.scarps.len(),
            r.transects.len(),
        );
        if let Some(t) = &r.tilt {
            s += &format!("\ntilt grade {:.5}", t.grade);
            if let Some(d) = t.downhill_xy {
                s += &format!("  downhill ({:.2}, {:.2})", d[0], d[1]);
            }
        }
        let falls: Vec<f64> = r.channels.iter().filter_map(|c| c.fall_grad).collect();
        if !falls.is_empty() {
            let lo = falls.iter().cloned().fold(f64::INFINITY, f64::min);
            let hi = falls.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            s += &format!("\nchannel fall {lo:.5}–{hi:.5}");
        }
        let longest = r
            .channels
            .iter()
            .map(|c| c.diag_m)
            .fold(0.0f64, f64::max);
        if longest > 0.0 {
            s += &format!("\nlongest channel span {longest:.0} m");
        }
        if !r.valley_centerlines.is_empty() {
            let with_meander = r
                .valley_centerlines
                .iter()
                .filter(|c| c.meander.is_some())
                .count();
            s += &format!(
                "\ncenterlines {} ({with_meander} with meander fits)",
                r.valley_centerlines.len()
            );
            if let Some(m) = r
                .valley_centerlines
                .iter()
                .find_map(|c| c.meander.as_ref())
            {
                s += &format!(
                    "\n  first: intensity {}  λ×W {}  sinuosity {}",
                    fmt_opt(m.intensity),
                    fmt_opt(m.wavelength_mult),
                    fmt_opt(m.sinuosity)
                );
            }
        }
        if let Some(w) = r.valley_centerlines.iter().find_map(|c| c.top_width_m) {
            s += &format!("\n  top width {w:.0} m");
        }
        let ridge_span = r
            .ridges
            .iter()
            .filter_map(|rr| rr.diag_m.or(rr.len_m))
            .fold(0.0f64, f64::max);
        if ridge_span > 0.0 {
            s += &format!("\nlongest ridge span {ridge_span:.0} m");
        }
        if let Some(rr) = r.ridges.iter().find(|r| r.kind == "accepted") {
            s += &format!(
                "\naccepted ridge: crest hw {}  bbox {}",
                fmt_opt(rr.crest_hw_m),
                rr.bbox_m.map_or("—".to_string(), |b| format!(
                    "{:.0},{:.0}–{:.0},{:.0}",
                    b[0], b[1], b[2], b[3]
                ))
            );
        }
        if let Some(sc) = r
            .scarps
            .iter()
            .filter(|s| s.accepted)
            .max_by(|a, b| a.height_m.total_cmp(&b.height_m))
        {
            s += &format!(
                "\ntallest scarp {:.1} m  face {:.2}  len {:.0} m",
                sc.height_m, sc.face_grad, sc.length_m
            );
        }
        s
    }

    /// Per-knob comparison against the archetype's committed prior table —
    /// the same tables `fit` writes and generation samples, so an out-of-band
    /// tile is visible before it moves a quantile.
    fn knob_rows(&self) -> Vec<(String, Option<f64>, Option<[f64; 11]>)> {
        let Some(k) = self.loaded.as_ref().and_then(|d| d.knobs.as_ref()) else {
            return Vec::new();
        };
        let priors = Priors::builtin();
        let entry = self.archetype_id().map(|a| priors.entry(a));
        k.knobs
            .iter()
            .map(|(name, v)| {
                let table = entry
                    .and_then(|e| e.params.get(&format!("landform.{name}")))
                    .map(|q| q.q);
                (name.clone(), *v, table)
            })
            .collect()
    }
}

fn pt(p: [f64; 2]) -> Vec2 {
    Vec2::new(p[0], p[1])
}

fn fmt_opt(v: Option<f64>) -> String {
    v.map_or("—".to_string(), |v| format!("{v:.3}"))
}

fn layer_color(l: Layer) -> [u8; 4] {
    LAYERS.iter().find(|(x, ..)| *x == l).unwrap().3
}

/// Green inside [q10,q90], yellow in the tails, red outside the table or
/// missing entirely.
fn knob_color(v: Option<f64>, table: Option<[f64; 11]>) -> egui::Color32 {
    let (Some(v), Some(q)) = (v, table) else {
        return egui::Color32::from_rgb(220, 110, 110);
    };
    if v >= q[1] && v <= q[9] {
        egui::Color32::from_rgb(120, 210, 120)
    } else if v >= q[0] && v <= q[10] {
        egui::Color32::from_rgb(225, 205, 100)
    } else {
        egui::Color32::from_rgb(220, 110, 110)
    }
}

impl eframe::App for Lab {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::SidePanel::left("browser")
            .min_width(320.0)
            .show(ctx, |ui| {
                ui.heading("tile-lab — campaign QA");
                ui.label(egui::RichText::new(self.root.display().to_string()).small().weak());
                ui.separator();

                egui::ScrollArea::vertical()
                    .max_height(260.0)
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
                                        let mut text = egui::RichText::new(&e.id);
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
                    if !k.extras.is_empty() {
                        // Not knobs, but the gates that decide knobs — the
                        // dune estimator is anisotropy-gated, so seeing
                        // aniso_ratio next to dune_wavelength_m matters.
                        let extras: Vec<String> = k
                            .extras
                            .iter()
                            .map(|(n, v)| format!("{n} {}", fmt_opt(*v)))
                            .collect();
                        ui.monospace(extras.join("\n"));
                    }
                }

                ui.separator();
                ui.label("knobs vs archetype prior");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for (name, v, table) in self.knob_rows() {
                        ui.horizontal(|ui| {
                            ui.colored_label(knob_color(v, table), "●");
                            ui.monospace(format!("{name:<24}"));
                            ui.monospace(match v {
                                Some(v) => format!("{v:>10.4}"),
                                None => "      null".to_string(),
                            });
                        })
                        .response
                        .on_hover_text(match table {
                            Some(q) => format!(
                                "prior q0 {:.4}  q10 {:.4}  med {:.4}  q90 {:.4}  q100 {:.4}",
                                q[0], q[1], q[5], q[9], q[10]
                            ),
                            None => "not a prior knob".to_string(),
                        });
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
                        let resp = ui.image((tex.id(), egui::vec2(side, side)));
                        if two {
                            ui.label("real");
                        }
                        self.hover_inspect(ui, &resp, side);
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

impl Lab {
    /// Nearest point-feature under the cursor (basins, transects, accepted
    /// ridge crests) as a tooltip — the "what is this blob?" affordance.
    fn hover_inspect(&self, _ui: &mut egui::Ui, resp: &egui::Response, side: f32) {
        let Some(pos) = resp.hover_pos() else { return };
        let Some(r) = self.loaded.as_ref().and_then(|d| d.regions.as_ref()) else {
            return;
        };
        let rect = resp.rect;
        let fx = ((pos.x - rect.min.x) / side).clamp(0.0, 1.0) as f64;
        let fy = ((pos.y - rect.min.y) / side).clamp(0.0, 1.0) as f64;
        let world = Vec2::new(fx * 3000.0, (1.0 - fy) * 3000.0);
        if let Some(text) = nearest_feature(r, world) {
            resp.clone().on_hover_text(text);
        }
    }
}

fn nearest_feature(r: &Regions, world: Vec2) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    let mut consider = |d: f64, text: String| {
        if best.as_ref().is_none_or(|(bd, _)| d < *bd) {
            best = Some((d, text));
        }
    };
    for b in &r.basins {
        let c = pt(b.center_m);
        let d = (c.distance(world) - b.radius_m).max(0.0);
        consider(
            d,
            format!(
                "basin {} — r {:.0} m, depth {:.1} m, ecc {}",
                if b.accepted { "accepted" } else { "REJECTED (<1.5 m)" },
                b.radius_m,
                b.depth_m,
                b.ecc.map_or("—".into(), |e| format!("{e:.2}"))
            ),
        );
    }
    for t in &r.transects {
        let d = pt(t.center_m).distance(world);
        consider(
            d,
            format!(
                "transect — halfwidth {:.0} m, wall {}",
                t.hw_m,
                t.wall_grade.map_or("—".into(), |w| format!("{w:.3}"))
            ),
        );
    }
    for rr in &r.ridges {
        if rr.kind != "accepted" {
            continue;
        }
        for p in &rr.centerline_m {
            let d = pt(*p).distance(world);
            consider(
                d,
                format!(
                    "ridge — prominence {} m, len {} m",
                    rr.prominence_m.map_or("—".into(), |v| format!("{v:.1}")),
                    rr.len_m.map_or("—".into(), |v| format!("{v:.0}"))
                ),
            );
        }
    }
    best.filter(|(d, _)| *d < 120.0).map(|(_, t)| t)
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
