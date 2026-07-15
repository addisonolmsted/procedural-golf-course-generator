//! Match tab — deliverable #2: the seed-searching viewer.
//!
//! Runs the two-stage search (every seed scored against every atlas course at
//! absolute scale, best window placement inside the 2 km world) on a
//! background thread with live progress, caches results to
//! `output/seed_search.json`, and shows course vs best-seed heightmaps side
//! by side under a shared elevation color scale.

use crate::to_color_image;
use eframe::egui;
use golf_atlas::{Atlas, StyleGroup};
use golf_match::{Progress, SearchConfig, SearchResults, SeedScore};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

/// Display resolution of the comparison images (finer than the scoring pass).
const DISPLAY_MPP: f64 = 7.8125;
/// Long-edge pixels of each comparison render.
const MATCH_PX: u32 = 460;

type ResultSlot = Arc<Mutex<Option<Option<SearchResults>>>>;

enum SearchUi {
    Idle,
    Running { progress: Arc<Progress>, slot: ResultSlot },
}

struct MatchView {
    /// (course, rank, results fingerprint, water overlay) this view was built for.
    key: (usize, usize, u64, bool),
    course_tex: egui::TextureHandle,
    course_cap: Vec<String>,
    gen_tex: Option<egui::TextureHandle>,
    gen_cap: Vec<String>,
}

pub struct MatchTab {
    atlas: Result<Arc<Atlas>, String>,
    seed_count: u64,
    state: SearchUi,
    results: Option<SearchResults>,
    note: String,
    sel_course: usize,
    sel_rank: usize,
    show_water: bool,
    view: Option<MatchView>,
}

impl MatchTab {
    pub fn new() -> Self {
        let atlas_path = golf_atlas::binfmt::default_bin_path();
        let atlas = golf_atlas::binfmt::load(&atlas_path).map(Arc::new).map_err(|e| {
            format!(
                "Cannot load the course atlas from {}:\n{e}\n\n\
                 Pack it once with:\n  cargo run -p xtask -- atlas-pack <path to parkland_atlas.html>",
                atlas_path.display()
            )
        });
        let mut tab = MatchTab {
            atlas,
            seed_count: SearchConfig::default().seed_count,
            state: SearchUi::Idle,
            results: None,
            note: "no results yet — run the search".into(),
            sel_course: 0,
            sel_rank: 0,
            show_water: true,
            view: None,
        };
        if let Ok(atlas) = &tab.atlas {
            if let Ok(r) = golf_match::load_results(&golf_match::default_results_path()) {
                if r.fingerprint == golf_match::fingerprint(atlas, &r.config) {
                    tab.seed_count = r.config.seed_count;
                    tab.note =
                        format!("cached: {} seeds in {:.0} s", r.config.seed_count, r.elapsed_s);
                    tab.results = Some(r);
                } else {
                    tab.note =
                        "cached results are stale (atlas or generator changed) — re-run".into();
                }
            }
        }
        tab
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        self.poll(ctx);

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

        egui::SidePanel::left("match_courses")
            .exact_width(250.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.course_rail(ui, &atlas));
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.search_bar(ui, &atlas);
            ui.separator();
            self.ensure_view(ctx, &atlas);
            self.compare_view(ui, &atlas);
        });
    }

    /// Collect a finished/cancelled background search.
    fn poll(&mut self, ctx: &egui::Context) {
        if let SearchUi::Running { slot, .. } = &self.state {
            let done = slot.lock().unwrap().take();
            match done {
                Some(Some(results)) => {
                    let path = golf_match::default_results_path();
                    match golf_match::save_results(&results, &path) {
                        Ok(()) => {
                            self.note = format!(
                                "{} seeds in {:.0} s — saved to {}",
                                results.config.seed_count,
                                results.elapsed_s,
                                path.display()
                            );
                        }
                        Err(e) => self.note = format!("done, but cache save failed: {e}"),
                    }
                    self.results = Some(results);
                    self.sel_rank = 0;
                    self.state = SearchUi::Idle;
                }
                Some(None) => {
                    self.note = "search cancelled".into();
                    self.state = SearchUi::Idle;
                }
                None => {
                    // Still running — keep the progress bar moving.
                    ctx.request_repaint_after(std::time::Duration::from_millis(120));
                }
            }
        }
    }

    fn course_rail(&mut self, ui: &mut egui::Ui, atlas: &Atlas) {
        ui.heading("Courses");
        ui.small("Parkland Atlas survey · 27 references");
        for group in [StyleGroup::Lowland, StyleGroup::Rolling, StyleGroup::Mountain] {
            ui.add_space(6.0);
            ui.label(egui::RichText::new(group.label().to_uppercase()).small().weak());
            ui.separator();
            for (i, c) in atlas.courses.iter().enumerate() {
                if c.group != group {
                    continue;
                }
                let best = self.results.as_ref().and_then(|r| {
                    r.matches
                        .iter()
                        .find(|m| m.course_key == c.key)
                        .and_then(|m| m.best.first())
                });
                let line = match best {
                    Some(b) => format!(
                        "{}\nrelief {:.0} m · best rmse {:.2} m",
                        c.label,
                        c.relief(),
                        b.rmse
                    ),
                    None => format!("{}\nrelief {:.0} m", c.label, c.relief()),
                };
                if ui.selectable_label(self.sel_course == i, line).clicked() {
                    self.sel_course = i;
                    self.sel_rank = 0;
                }
            }
        }
    }

    fn search_bar(&mut self, ui: &mut egui::Ui, atlas: &Arc<Atlas>) {
        ui.horizontal(|ui| {
            let mut start = false;
            match &self.state {
                SearchUi::Idle => {
                    ui.label("Seeds:");
                    ui.add(
                        egui::DragValue::new(&mut self.seed_count)
                            .speed(1000)
                            .range(100..=5_000_000),
                    );
                    start = ui.button("▶ Run search").clicked();
                    ui.separator();
                    ui.checkbox(&mut self.show_water, "Water")
                        .on_hover_text("Overlay real course water (survey mask) and the seed's generated water");
                    ui.separator();
                    ui.weak(&self.note);
                }
                SearchUi::Running { progress, .. } => {
                    let stage = progress.stage.load(Ordering::Relaxed);
                    let frac = progress.fraction();
                    let label = match stage {
                        1 => format!("stage 1/2 · coarse scan of every seed · {:.0}%", frac * 100.0),
                        2 => format!("stage 2/2 · refining shortlist · {:.0}%", frac * 100.0),
                        _ => "starting…".into(),
                    };
                    ui.add(
                        egui::ProgressBar::new(frac)
                            .desired_width(420.0)
                            .text(label),
                    );
                    if ui.button("Cancel").clicked() {
                        progress.cancel.store(true, Ordering::Relaxed);
                    }
                }
            }
            if start {
                self.start_search(atlas);
            }
        });
    }

    fn start_search(&mut self, atlas: &Arc<Atlas>) {
        let config = SearchConfig {
            seed_count: self.seed_count,
            ..Default::default()
        };
        let progress = Arc::new(Progress::default());
        let slot: ResultSlot = Arc::new(Mutex::new(None));
        let (a, p, s) = (Arc::clone(atlas), Arc::clone(&progress), Arc::clone(&slot));
        std::thread::spawn(move || {
            let r = golf_match::run_search(&a, &config, &p);
            *s.lock().unwrap() = Some(r);
        });
        self.state = SearchUi::Running { progress, slot };
    }

    /// The best match entry for the selected course, if any.
    fn selected_score(&self, atlas: &Atlas) -> Option<SeedScore> {
        let key = &atlas.courses[self.sel_course].key;
        self.results.as_ref().and_then(|r| {
            r.matches
                .iter()
                .find(|m| &m.course_key == key)
                .and_then(|m| m.best.get(self.sel_rank).copied())
        })
    }

    /// (Re)build the side-by-side textures when the selection changes.
    fn ensure_view(&mut self, ctx: &egui::Context, atlas: &Atlas) {
        let fp = self.results.as_ref().map(|r| r.fingerprint).unwrap_or(0);
        let key = (self.sel_course, self.sel_rank, fp, self.show_water);
        if self.view.as_ref().is_some_and(|v| v.key == key) {
            return;
        }

        let course = &atlas.courses[self.sel_course];
        let s2mpp = self
            .results
            .as_ref()
            .map(|r| r.config.stage2_mpp)
            .unwrap_or_else(|| SearchConfig::default().stage2_mpp);
        let (win_w, win_h) = course.window_dims(s2mpp);
        let cgrid = course.window_grid(win_w, win_h, DISPLAY_MPP);
        let cmean = cgrid.data.iter().sum::<f64>() / cgrid.data.len().max(1) as f64;
        let (cmin, cmax) = min_max(&cgrid.data);
        let cp298 = golf_terrain::percentile_span(&cgrid.data, 0.02, 0.98);

        let course_cap = vec![
            format!("{} — {}", course.label, course.group.label()),
            format!(
                "window {:.0} × {:.0} m of the {:.0} × {:.0} m survey",
                win_w, win_h, course.wm, course.hm
            ),
            format!(
                "relief {:.1} m · p2–p98 {:.1} m · water {:.1}%",
                cmax - cmin,
                cp298,
                course.waterpct
            ),
        ];

        let view = match self.selected_score(atlas) {
            Some(sc) => {
                let (mut g, mu, gen_water) = golf_match::regenerate_window(
                    sc.seed, win_w, win_h, sc.off_x, sc.off_y, DISPLAY_MPP, cmean,
                );
                // Shift the generated window onto the course's absolute datum
                // (matching removes only this DC offset).
                for z in &mut g.data {
                    *z -= mu;
                }
                let (gmin, gmax) = min_max(&g.data);
                let range = Some((cmin.min(gmin), cmax.max(gmax)));
                let gp298 = golf_terrain::percentile_span(&g.data, 0.02, 0.98);
                let sp = golf_terrain::sample_params(sc.seed);
                let seed_water = 100.0
                    * gen_water.data.iter().sum::<f64>()
                    / gen_water.data.len().max(1) as f64;
                let mut course_img = golf_viz::render_height_grid(&cgrid, MATCH_PX, range);
                let mut gen_img = golf_viz::render_height_grid(&g, MATCH_PX, range);
                if self.show_water {
                    // Real survey water on the course; our build on the seed.
                    let cmask = course.water_window(win_w, win_h, DISPLAY_MPP);
                    golf_viz::tint_water_mask(&mut course_img, &cmask);
                    golf_viz::tint_water_mask(&mut gen_img, &gen_water);
                }
                MatchView {
                    key,
                    course_tex: ctx.load_texture(
                        "match_course",
                        to_color_image(&course_img),
                        egui::TextureOptions::LINEAR,
                    ),
                    course_cap,
                    gen_tex: Some(ctx.load_texture(
                        "match_gen",
                        to_color_image(&gen_img),
                        egui::TextureOptions::LINEAR,
                    )),
                    gen_cap: vec![
                        format!("seed {} (rank {})", sc.seed, self.sel_rank + 1),
                        format!(
                            "RMSE {:.2} m · window at ({:.0}, {:.0}) m · datum shift {:+.1} m",
                            sc.rmse, sc.off_x, sc.off_y, -mu
                        ),
                        format!(
                            "{} draw · relief ⌖ {:.0} m · p2–p98 {:.1} m · window water {:.1}% (course {:.1}%)",
                            sp.mode.label(),
                            sp.relief_target,
                            gp298,
                            seed_water,
                            course.waterpct
                        ),
                    ],
                }
            }
            None => {
                let mut course_img = golf_viz::render_height_grid(&cgrid, MATCH_PX, None);
                if self.show_water {
                    let cmask = course.water_window(win_w, win_h, DISPLAY_MPP);
                    golf_viz::tint_water_mask(&mut course_img, &cmask);
                }
                MatchView {
                    key,
                    course_tex: ctx.load_texture(
                        "match_course",
                        to_color_image(&course_img),
                        egui::TextureOptions::LINEAR,
                    ),
                    course_cap,
                    gen_tex: None,
                    gen_cap: vec![self.note.clone()],
                }
            }
        };
        self.view = Some(view);
    }

    fn compare_view(&mut self, ui: &mut egui::Ui, atlas: &Atlas) {
        let Some(v) = &self.view else { return };

        let col_w = (ui.available_width() - 24.0) / 2.0;
        let img_scale = |tex: &egui::TextureHandle| {
            let s = tex.size();
            let tsize = egui::vec2(s[0] as f32, s[1] as f32);
            let scale = (col_w / tsize.x).min(1.2);
            tsize * scale
        };

        ui.columns(2, |cols| {
            cols[0].strong("Target course (survey data)");
            let sz = img_scale(&v.course_tex);
            cols[0].add(
                egui::Image::from_texture(egui::load::SizedTexture::new(v.course_tex.id(), sz))
                    .fit_to_exact_size(sz),
            );
            for line in &v.course_cap {
                cols[0].small(line.clone());
            }

            cols[1].strong("Generated terrain (matched seed)");
            match &v.gen_tex {
                Some(tex) => {
                    let sz = img_scale(tex);
                    cols[1].add(
                        egui::Image::from_texture(egui::load::SizedTexture::new(tex.id(), sz))
                            .fit_to_exact_size(sz),
                    );
                }
                None => {
                    cols[1].add_space(30.0);
                    cols[1].weak("—");
                }
            }
            for line in &v.gen_cap {
                cols[1].small(line.clone());
            }
        });

        // Top-N seed list for the selected course.
        let course_key = &atlas.courses[self.sel_course].key;
        let ranks: Vec<(usize, SeedScore)> = self
            .results
            .as_ref()
            .and_then(|r| r.matches.iter().find(|m| &m.course_key == course_key))
            .map(|m| m.best.iter().copied().enumerate().collect())
            .unwrap_or_default();
        if !ranks.is_empty() {
            ui.add_space(8.0);
            ui.strong("Top matching seeds");
            let mut new_rank = None;
            for (i, sc) in &ranks {
                let text = format!(
                    "#{:<2}  seed {:<10}  rmse {:>6.2} m   window at ({:>4.0}, {:>4.0}) m",
                    i + 1,
                    sc.seed,
                    sc.rmse,
                    sc.off_x,
                    sc.off_y
                );
                if ui
                    .selectable_label(*i == self.sel_rank, egui::RichText::new(text).monospace())
                    .clicked()
                {
                    new_rank = Some(*i);
                }
            }
            if let Some(r) = new_rank {
                self.sel_rank = r;
            }
        }
    }
}

fn min_max(vals: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in vals {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    (lo, hi)
}
