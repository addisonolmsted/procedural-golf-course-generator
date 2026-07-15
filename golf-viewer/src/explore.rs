//! Explore tab — deliverable #1: randomize a seed, view its terrain, read
//! total relief / high / low, and the slope-grade distribution.
//!
//! Parameters come from the seed via the atlas-fit sampler by default; a
//! Manual mode keeps the raw sliders for hand-tuning experiments.

use crate::to_color_image;
use eframe::egui;
use golf_terrain::{
    generate, generate_course, generate_course_with, world_spec, CourseTerrain, ErosionParams,
    SampledParams, Terrain, TerrainParams, FAIRWAY_MAX_GRADE,
};

/// Detail-view generation resolution and render size.
const DETAIL_N: u32 = 256;
const DETAIL_PX: u32 = 640;
/// Grid-thumbnail generation resolution and render size.
const THUMB_N: u32 = 96;
const THUMB_PX: u32 = 200;
/// Displayed size of each grid thumbnail (points).
const THUMB_DISP: f32 = 150.0;
/// Slope histogram: bins over [0, HIST_MAX_SLOPE] (0%..30% grade).
const HIST_BINS: usize = 30;
const HIST_MAX_SLOPE: f64 = 0.30;

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Detail,
    Grid,
}

#[derive(Clone, Copy, PartialEq)]
enum ViewKind {
    Height,
    Slope,
    /// Drainage/lakes debug view — needs the erosion pipeline.
    Flow,
}

#[derive(Clone, Copy, PartialEq)]
enum ParamSource {
    /// Seed drives everything through the sampler.
    Sampled,
    /// Sliders drive the params; the seed only moves the noise field.
    Manual,
}

struct HistData {
    counts: Vec<u32>,
    playable: f64,
}

/// Elevation along the routed course, in play order: every hole's line of
/// play then its walk to the next tee (hole 9's walk returns to the
/// clubhouse). Sampled every ~10 m on the composed surface.
struct ProfileData {
    /// (cumulative distance m, elevation m, hole 0-8, is_walk)
    pts: Vec<(f32, f32, u8, bool)>,
    total: f32,
    zmin: f32,
    zmax: f32,
    /// Cumulative distance at each hole's tee (label anchors).
    hole_starts: [f32; 9],
    pars: [u8; 9],
    climb: f32,
    descent: f32,
}

fn build_profile(
    ct: &CourseTerrain,
    r: &golf_routing::Routing,
    build: Option<&golf_holes::CourseBuild>,
) -> ProfileData {
    let mut pts: Vec<(f32, f32, u8, bool)> = Vec::with_capacity(512);
    let mut hole_starts = [0.0f32; 9];
    let mut dist = 0.0f64;
    let z_at = |p: golf_core::math::Vec2| -> f32 {
        (match build {
            Some(b) => b.surface_at(ct, p),
            None => ct.height_at(p),
        }) as f32
    };
    let push_poly = |poly: &[golf_core::math::Vec2],
                     hole: u8,
                     walk: bool,
                     dist: &mut f64,
                     pts: &mut Vec<(f32, f32, u8, bool)>| {
        for w in poly.windows(2) {
            let seg = w[0].distance(w[1]);
            let n = (seg / 10.0).ceil().max(1.0) as usize;
            for k in 0..n {
                let t = k as f64 / n as f64;
                let p = w[0].lerp(w[1], t);
                pts.push(((*dist + seg * t) as f32, z_at(p), hole, walk));
            }
            *dist += seg;
        }
    };
    for (i, h) in r.holes.iter().enumerate() {
        hole_starts[i] = dist as f32;
        push_poly(&h.pts, i as u8, false, &mut dist, &mut pts);
        push_poly(&h.walk_to_next, i as u8, true, &mut dist, &mut pts);
    }
    // Close the last sample at the clubhouse end.
    if let Some(last) = r.holes.last() {
        if let Some(&p) = last.walk_to_next.last() {
            pts.push((dist as f32, z_at(p), 8, true));
        }
    }
    let (mut zmin, mut zmax) = (f32::INFINITY, f32::NEG_INFINITY);
    let (mut climb, mut descent) = (0.0f32, 0.0f32);
    for w in pts.windows(2) {
        let dz = w[1].1 - w[0].1;
        if !w[1].3 && !w[0].3 {
            if dz > 0.0 {
                climb += dz;
            } else {
                descent -= dz;
            }
        }
    }
    for &(_, z, _, _) in &pts {
        zmin = zmin.min(z);
        zmax = zmax.max(z);
    }
    ProfileData {
        total: dist as f32,
        pts,
        zmin,
        zmax,
        hole_starts,
        pars: r.par_seq,
        climb,
        descent,
    }
}

struct DetailData {
    tex: egui::TextureHandle,
    stats: golf_terrain::TerrainStats,
    relief_p2p98: f64,
    /// (x, y, z) at DETAIL_N resolution.
    lo: (u32, u32, f64),
    hi: (u32, u32, f64),
    hist: HistData,
    sampled: Option<SampledParams>,
    /// Water summary line (full pipeline only).
    water_line: Option<String>,
    /// Routing summary lines (seed-sampled detail with the overlay on).
    routing_lines: Vec<String>,
    /// Course elevation profile (when the seed routed).
    profile: Option<ProfileData>,
}

struct GridCell {
    seed: u64,
    tex: egui::TextureHandle,
    caption: String,
}

/// One generated view: raw noise (Manual, erosion off) or the full pipeline.
enum Generated {
    Raw(Box<Terrain>),
    Full(Box<CourseTerrain>),
}

impl Generated {
    fn terrain(&self) -> &Terrain {
        match self {
            Generated::Raw(t) => t,
            Generated::Full(ct) => &ct.terrain,
        }
    }

    fn course(&self) -> Option<&CourseTerrain> {
        match self {
            Generated::Raw(_) => None,
            Generated::Full(ct) => Some(ct),
        }
    }
}

pub struct ExploreTab {
    source: ParamSource,
    params: TerrainParams,
    manual_erosion: bool,
    eparams: ErosionParams,
    water_overlay: bool,
    routing_overlay: bool,
    build_overlay: bool,
    seed: u64,
    grid_base: u64,
    mode: Mode,
    kind: ViewKind,
    detail: Option<DetailData>,
    grid: Vec<GridCell>,
    dirty: bool,
}

impl ExploreTab {
    /// Current detail-view seed (the Course 3D tab syncs to it).
    pub fn seed(&self) -> u64 {
        self.seed
    }

    pub fn new() -> Self {
        ExploreTab {
            source: ParamSource::Sampled,
            params: TerrainParams::default(),
            manual_erosion: false,
            eparams: ErosionParams::default(),
            water_overlay: true,
            routing_overlay: true,
            build_overlay: true,
            seed: 2024,
            grid_base: 1,
            mode: Mode::Detail,
            kind: ViewKind::Height,
            detail: None,
            grid: Vec::new(),
            dirty: true,
        }
    }

    /// Generate one seed under the current source (raw noise or full pipeline).
    fn gen_for(&self, seed: u64, n: u32) -> (Generated, Option<SampledParams>) {
        let spec = world_spec(n);
        match self.source {
            ParamSource::Sampled => {
                let (ct, sp) = generate_course(&spec, seed);
                (Generated::Full(Box::new(ct)), Some(sp))
            }
            ParamSource::Manual => {
                if self.manual_erosion {
                    let ct = generate_course_with(&spec, seed, &self.params, &self.eparams, &golf_terrain::WaterParams::NONE);
                    (Generated::Full(Box::new(ct)), None)
                } else {
                    (Generated::Raw(Box::new(generate(&spec, seed, &self.params))), None)
                }
            }
        }
    }

    fn render_of(&self, g: &Generated, px: u32) -> image::RgbaImage {
        match self.kind {
            ViewKind::Height => match g.course() {
                Some(ct) if self.water_overlay => {
                    golf_viz::render_height_water(&ct.terrain.heights, &ct.water, px, None)
                }
                _ => golf_viz::render_height_px(g.terrain(), px),
            },
            ViewKind::Slope => golf_viz::render_slope_px(g.terrain(), px),
            ViewKind::Flow => match g.course() {
                Some(ct) => golf_viz::render_flow(&ct.erosion.flow_area, &ct.erosion.lake_depth, px),
                // No erosion data in raw-noise mode: fall back to height.
                None => golf_viz::render_height_px(g.terrain(), px),
            },
        }
    }

    fn regen(&mut self, ctx: &egui::Context) {
        match self.mode {
            Mode::Detail => {
                let (g, sampled) = self.gen_for(self.seed, DETAIL_N);
                let mut img = self.render_of(&g, DETAIL_PX);
                // Routed nine overlay: seed-sampled height view only (the
                // game path; manual params are a terrain-tuning tool).
                let mut routing_lines = Vec::new();
                let mut profile = None;
                if self.routing_overlay
                    && self.kind == ViewKind::Height
                    && self.source == ParamSource::Sampled
                {
                    if let Some(ct) = g.course() {
                        match golf_routing::route(ct, self.seed) {
                            Ok(r) => {
                                let built = if self.build_overlay {
                                    let t0 = std::time::Instant::now();
                                    let b = golf_holes::build(ct, &r, self.seed);
                                    let ms = t0.elapsed().as_secs_f64() * 1e3;
                                    golf_viz::draw_build(&mut img, &b, 2000.0);
                                    Some((b, ms))
                                } else {
                                    None
                                };
                                golf_viz::draw_routing(&mut img, &r, 2000.0);
                                let pars: Vec<String> =
                                    r.par_seq.iter().map(|p| p.to_string()).collect();
                                routing_lines.push(format!("routing  par {}", pars.join("-")));
                                routing_lines.push(format!(
                                    "         {:.0} m + {:.0} m walks · stage {}",
                                    r.total_len, r.total_walk, r.relax_stage
                                ));
                                if let Some((b, ms)) = &built {
                                    let (mut alo, mut ahi, mut pmin) =
                                        (f64::INFINITY, 0.0f64, f64::INFINITY);
                                    for h in &b.holes {
                                        alo = alo.min(h.green.area);
                                        ahi = ahi.max(h.green.area);
                                        pmin = pmin.min(h.pinnable_share);
                                    }
                                    routing_lines.push(format!(
                                        "built    greens {:.0}–{:.0} m² · pinnable ≥{:.0}% · {:.0} ms",
                                        alo,
                                        ahi,
                                        100.0 * pmin,
                                        ms
                                    ));
                                }
                                profile = Some(build_profile(
                                    ct,
                                    &r,
                                    built.as_ref().map(|(b, _)| b),
                                ));
                            }
                            Err(e) => {
                                routing_lines.push(format!("routing  UNROUTABLE ({})", e.reason));
                            }
                        }
                    }
                }
                let t = g.terrain();
                let (lo, hi) = t.extremes();
                let water_line = g.course().map(|ct| {
                    let target = sampled.as_ref().map(|sp| sp.water.coverage).unwrap_or(0.0);
                    format!(
                        "water    {:>6.1} %  (⌖ {:.1}%) · {} bodies · {} streams",
                        100.0 * ct.water.coverage,
                        100.0 * target,
                        ct.water.bodies.len(),
                        ct.water.streams.iter().filter(|l| l.perennial).count(),
                    )
                });
                self.detail = Some(DetailData {
                    tex: ctx.load_texture("detail", to_color_image(&img), egui::TextureOptions::LINEAR),
                    stats: t.stats(),
                    relief_p2p98: t.relief_p2_p98(),
                    lo,
                    hi,
                    hist: HistData {
                        counts: t.slope_histogram(HIST_BINS, HIST_MAX_SLOPE),
                        playable: t.slope_fraction_below(FAIRWAY_MAX_GRADE),
                    },
                    sampled,
                    water_line,
                    routing_lines,
                    profile,
                });
            }
            Mode::Grid => {
                self.grid.clear();
                for i in 0..16u64 {
                    let seed = self.grid_base.wrapping_add(i);
                    let (g, sampled) = self.gen_for(seed, THUMB_N);
                    let img = self.render_of(&g, THUMB_PX);
                    let caption = match sampled {
                        Some(sp) => format!(
                            "seed {seed} · {} · {:.0} m",
                            sp.mode.label(),
                            sp.relief_target
                        ),
                        None => format!("seed {seed}"),
                    };
                    let tex = ctx.load_texture(
                        format!("grid{i}"),
                        to_color_image(&img),
                        egui::TextureOptions::LINEAR,
                    );
                    self.grid.push(GridCell { seed, tex, caption });
                }
            }
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if self.dirty {
            self.dirty = false;
            self.regen(ctx);
        }

        egui::SidePanel::left("explore_controls")
            .exact_width(310.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| self.controls(ui));
            });

        if self.mode == Mode::Detail {
            egui::TopBottomPanel::bottom("explore_histogram")
                .exact_height(148.0)
                .show(ctx, |ui| {
                    ui.add_space(2.0);
                    ui.strong("Slope grade distribution");
                    if let Some(d) = &self.detail {
                        draw_histogram(ui, &d.hist.counts, d.hist.playable);
                    }
                });
            if let Some(p) = self.detail.as_ref().and_then(|d| d.profile.as_ref()) {
                let title = format!(
                    "Course elevation profile — {:.0} m walked · climb +{:.0} m / −{:.0} m (lines of play)",
                    p.total, p.climb, p.descent
                );
                egui::TopBottomPanel::bottom("course_profile")
                    .exact_height(132.0)
                    .show(ctx, |ui| {
                        ui.add_space(2.0);
                        ui.strong(title);
                        if let Some(p) = self.detail.as_ref().and_then(|d| d.profile.as_ref()) {
                            draw_profile(ui, p);
                        }
                    });
            }
        }

        egui::CentralPanel::default().show(ctx, |ui| match self.mode {
            Mode::Detail => self.detail_view(ui),
            Mode::Grid => self.grid_view(ui),
        });
    }

    fn controls(&mut self, ui: &mut egui::Ui) {
        ui.heading("Explore");
        ui.label("2 km × 2 km · deterministic in seed");
        ui.separator();

        ui.horizontal(|ui| {
            ui.label("View:");
            if ui.selectable_label(self.mode == Mode::Detail, "Detail").clicked() {
                self.mode = Mode::Detail;
                self.dirty = true;
            }
            if ui.selectable_label(self.mode == Mode::Grid, "4×4 grid").clicked() {
                self.mode = Mode::Grid;
                self.dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Field:");
            if ui.selectable_label(self.kind == ViewKind::Height, "Height").clicked() {
                self.kind = ViewKind::Height;
                self.dirty = true;
            }
            if ui.selectable_label(self.kind == ViewKind::Slope, "Slope").clicked() {
                self.kind = ViewKind::Slope;
                self.dirty = true;
            }
            if ui
                .selectable_label(self.kind == ViewKind::Flow, "Flow")
                .on_hover_text("Drainage + lakes (needs the erosion pipeline)")
                .clicked()
            {
                self.kind = ViewKind::Flow;
                self.dirty = true;
            }
            if self.kind == ViewKind::Height
                && ui
                    .checkbox(&mut self.water_overlay, "Water")
                    .on_hover_text("Draw the natural water layer on the height view")
                    .changed()
            {
                self.dirty = true;
            }
            if self.kind == ViewKind::Height
                && self.source == ParamSource::Sampled
                && ui
                    .checkbox(&mut self.routing_overlay, "Routing")
                    .on_hover_text("Route and draw the 9-hole par-36 nine (detail view)")
                    .changed()
            {
                self.dirty = true;
            }
            if self.kind == ViewKind::Height
                && self.source == ParamSource::Sampled
                && self.routing_overlay
                && ui
                    .checkbox(&mut self.build_overlay, "Built")
                    .on_hover_text("Fairway/green/tee zones + pins from the hole build-out")
                    .changed()
            {
                self.dirty = true;
            }
        });
        ui.horizontal(|ui| {
            ui.label("Params:");
            if ui
                .selectable_label(self.source == ParamSource::Sampled, "Seed-sampled")
                .on_hover_text("Distributions fit to the 27 atlas courses")
                .clicked()
            {
                self.source = ParamSource::Sampled;
                self.dirty = true;
            }
            if ui.selectable_label(self.source == ParamSource::Manual, "Manual").clicked() {
                self.source = ParamSource::Manual;
                self.dirty = true;
            }
        });

        ui.separator();

        match self.mode {
            Mode::Detail => {
                ui.label("Seed");
                ui.horizontal(|ui| {
                    if ui.button("−").clicked() {
                        self.seed = self.seed.wrapping_sub(1);
                        self.dirty = true;
                    }
                    if ui.add(egui::DragValue::new(&mut self.seed).speed(1.0)).changed() {
                        self.dirty = true;
                    }
                    if ui.button("+").clicked() {
                        self.seed = self.seed.wrapping_add(1);
                        self.dirty = true;
                    }
                    if ui.button("🎲 randomize").clicked() {
                        self.seed = lcg(self.seed);
                        self.dirty = true;
                    }
                });
            }
            Mode::Grid => {
                ui.label("Grid base seed (16 consecutive)");
                ui.horizontal(|ui| {
                    if ui.button("−16").clicked() {
                        self.grid_base = self.grid_base.wrapping_sub(16);
                        self.dirty = true;
                    }
                    if ui.add(egui::DragValue::new(&mut self.grid_base).speed(1.0)).changed() {
                        self.dirty = true;
                    }
                    if ui.button("+16").clicked() {
                        self.grid_base = self.grid_base.wrapping_add(16);
                        self.dirty = true;
                    }
                    if ui.button("🎲").clicked() {
                        self.grid_base = lcg(self.grid_base);
                        self.dirty = true;
                    }
                });
            }
        }

        ui.separator();

        match self.source {
            ParamSource::Sampled => self.sampled_readout(ui),
            ParamSource::Manual => self.manual_sliders(ui),
        }

        ui.separator();
        if let (Mode::Detail, Some(d)) = (self.mode, &self.detail) {
            let s = &d.stats;
            ui.label("Terrain stats");
            ui.monospace(format!("relief   {:>7.1} m  (max−min)", s.max - s.min));
            ui.monospace(format!("p2–p98   {:>7.1} m", d.relief_p2p98));
            ui.monospace(format!(
                "high  ▲  {:>7.1} m  at ({:.0}, {:.0}) m",
                d.hi.2,
                d.hi.0 as f64 * 2000.0 / DETAIL_N as f64,
                d.hi.1 as f64 * 2000.0 / DETAIL_N as f64
            ));
            ui.monospace(format!(
                "low   ▼  {:>7.1} m  at ({:.0}, {:.0}) m",
                d.lo.2,
                d.lo.0 as f64 * 2000.0 / DETAIL_N as f64,
                d.lo.1 as f64 * 2000.0 / DETAIL_N as f64
            ));
            ui.monospace(format!("slope    {:>7.4} mean", s.mean_slope));
            ui.monospace(format!(
                "playable {:>6.1} %  (≤{:.0}% grade)",
                d.hist.playable * 100.0,
                FAIRWAY_MAX_GRADE * 100.0
            ));
            if let Some(w) = &d.water_line {
                ui.monospace(w.clone());
            }
            for line in &d.routing_lines {
                ui.monospace(line.clone());
            }
        }

        ui.separator();
        ui.small("Distributions fit to the Parkland Atlas · erosion comes next");
    }

    fn sampled_readout(&mut self, ui: &mut egui::Ui) {
        ui.label("Sampled parameters (read-only)");
        let sp = match self.mode {
            Mode::Detail => self.detail.as_ref().and_then(|d| d.sampled),
            Mode::Grid => None,
        };
        let Some(sp) = sp else {
            if self.mode == Mode::Grid {
                ui.small("Each grid cell draws its own parameter set — captions show mode + relief.");
            }
            return;
        };
        let p = &sp.params;
        let h = -(p.gain.ln() / 2f64.ln());
        ui.monospace(format!("mode      {}", sp.mode.label()));
        ui.monospace(format!("relief ⌖  {:.1} m", sp.relief_target));
        ui.monospace(format!("amplitude {:.1} m", p.amplitude));
        ui.monospace(format!("period    {:.0} m", p.base_period));
        ui.monospace(format!("octaves   {}", p.octaves));
        ui.monospace(format!("gain      {:.3}  (H {:.2})", p.gain, h));
        ui.monospace(format!("warp      {:.0} m @ {:.0} m", p.warp_amp, p.warp_period));
        ui.monospace(format!(
            "redist    p {:.2}  (skew ⌖ {:+.2})",
            p.redistribution, sp.skew_target
        ));
        ui.monospace(format!(
            "erosion   Θ {:.2} · g {:.2} · α {:.2} · hard {:.2}",
            sp.erosion.intensity,
            sp.erosion.deposition,
            sp.erosion.diffusion,
            sp.erosion.hardness_contrast
        ));
        if sp.water.coverage > 0.0 {
            ui.monospace(format!(
                "water     ⌖ {:.1}%{} · A_min {:.0}k m² · pond {:.0}%",
                100.0 * sp.water.coverage,
                if sp.water.lake_course { " · LAKE" } else { "" },
                sp.water.stream_area_min / 1000.0,
                100.0 * sp.water.pond_share,
            ));
        } else {
            ui.monospace("water     dry course (swales only)");
        }
        ui.add_space(4.0);
        if ui
            .button("Copy → manual sliders")
            .on_hover_text("Start hand-tuning from this draw")
            .clicked()
        {
            self.params = sp.params;
            self.eparams = sp.erosion;
            self.manual_erosion = true;
            self.source = ParamSource::Manual;
            self.dirty = true;
        }
    }

    fn manual_sliders(&mut self, ui: &mut egui::Ui) {
        ui.label("Noise parameters");
        let mut changed = false;
        changed |= ui
            .add(egui::Slider::new(&mut self.params.amplitude, 0.0..=350.0).text("amplitude (m)"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.base_period, 100.0..=2400.0).text("base period (m)"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.octaves, 1..=8).text("octaves"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.lacunarity, 1.5..=3.0).text("lacunarity"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.gain, 0.2..=0.8).text("gain / persistence"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.warp_amp, 0.0..=280.0).text("warp amp (m)"))
            .changed();
        changed |= ui
            .add(egui::Slider::new(&mut self.params.warp_period, 100.0..=2400.0).text("warp period (m)"))
            .changed();
        changed |= ui
            .add(
                egui::Slider::new(&mut self.params.redistribution, 0.1..=10.0)
                    .logarithmic(true)
                    .text("redistribution"),
            )
            .changed();
        if changed {
            self.dirty = true;
        }

        ui.add_space(6.0);
        if ui.checkbox(&mut self.manual_erosion, "Erosion pipeline").changed() {
            self.dirty = true;
        }
        if self.manual_erosion {
            let mut ech = false;
            ech |= ui
                .add(egui::Slider::new(&mut self.eparams.intensity, 0.05..=2.0).text("intensity Θ"))
                .changed();
            ech |= ui
                .add(egui::Slider::new(&mut self.eparams.deposition, 0.4..=2.5).text("deposition g"))
                .changed();
            ech |= ui
                .add(egui::Slider::new(&mut self.eparams.diffusion, 0.3..=3.0).text("diffusion α"))
                .changed();
            ech |= ui
                .add(
                    egui::Slider::new(&mut self.eparams.hardness_contrast, 0.0..=0.85)
                        .text("hardness contrast"),
                )
                .changed();
            if ech {
                self.dirty = true;
            }
        }

        if ui.button("Reset params").clicked() {
            self.params = TerrainParams::default();
            self.eparams = ErosionParams::default();
            self.dirty = true;
        }
    }

    fn detail_view(&mut self, ui: &mut egui::Ui) {
        let Some(d) = &self.detail else { return };
        let avail = ui.available_size();
        let s = d.tex.size();
        let tsize = egui::vec2(s[0] as f32, s[1] as f32);
        let scale = (avail.x / tsize.x).min(avail.y / tsize.y).min(1.5);
        ui.centered_and_justified(|ui| {
            let resp = ui.add(
                egui::Image::from_texture(egui::load::SizedTexture::new(d.tex.id(), tsize))
                    .fit_to_exact_size(tsize * scale),
            );
            // High / low markers over the image (grid y is north-up; image y is
            // down). `centered_and_justified` allocates the WHOLE panel to the
            // widget, so resp.rect is not the drawn image — reconstruct the
            // actual image rect (fitted size, centered) or markers land
            // outside the map.
            let rect = egui::Rect::from_center_size(resp.rect.center(), tsize * scale);
            let painter = ui.painter_at(rect);
            let mark = |painter: &egui::Painter, cell: (u32, u32, f64), sym: &str, col: egui::Color32| {
                let n = DETAIL_N as f32;
                let px = rect.left() + (cell.0 as f32 + 0.5) / n * rect.width();
                let py = rect.top() + (1.0 - (cell.1 as f32 + 0.5) / n) * rect.height();
                painter.text(
                    egui::pos2(px, py),
                    egui::Align2::CENTER_CENTER,
                    sym,
                    egui::FontId::proportional(16.0),
                    col,
                );
                painter.text(
                    egui::pos2(px, py + 12.0),
                    egui::Align2::CENTER_TOP,
                    format!("{:.0} m", cell.2),
                    egui::FontId::monospace(11.0),
                    col,
                );
            };
            mark(&painter, d.hi, "▲", egui::Color32::from_rgb(228, 87, 61));
            mark(&painter, d.lo, "▼", egui::Color32::from_rgb(127, 183, 222));
        });
    }

    fn grid_view(&mut self, ui: &mut egui::Ui) {
        let cells: Vec<(u64, egui::TextureId, String)> = self
            .grid
            .iter()
            .map(|c| (c.seed, c.tex.id(), c.caption.clone()))
            .collect();
        let mut promote: Option<u64> = None;

        ui.heading("4×4 seed grid — click to open in detail");
        egui::Grid::new("seedgrid").spacing([8.0, 8.0]).show(ui, |ui| {
            for row in 0..4 {
                for col in 0..4 {
                    let (seed, id, caption) = &cells[row * 4 + col];
                    ui.vertical(|ui| {
                        let sized =
                            egui::load::SizedTexture::new(*id, egui::vec2(THUMB_DISP, THUMB_DISP));
                        if ui
                            .add(egui::ImageButton::new(egui::Image::from_texture(sized)))
                            .clicked()
                        {
                            promote = Some(*seed);
                        }
                        ui.small(caption);
                    });
                }
                ui.end_row();
            }
        });

        if let Some(seed) = promote {
            self.seed = seed;
            self.mode = Mode::Detail;
            self.dirty = true;
        }
    }
}

/// Slope-grade histogram: green bars below the 12% fairway limit, hot above,
/// dotted line at the threshold, playable % readout.
fn draw_histogram(ui: &mut egui::Ui, counts: &[u32], playable: f64) {
    let bins = counts.len().max(1);
    let maxc = counts.iter().copied().max().unwrap_or(1).max(1) as f32;

    let desired = egui::vec2(ui.available_width(), ui.available_height().min(112.0));
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));

    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 8.0, rect.top() + 18.0),
        egui::pos2(rect.right() - 8.0, rect.bottom() - 16.0),
    );
    let bin_w = plot.width() / bins as f32;
    let green = egui::Color32::from_rgb(90, 190, 90);
    let hot = egui::Color32::from_rgb(210, 120, 60);

    for (i, &c) in counts.iter().enumerate() {
        let bar_h = (c as f32 / maxc) * plot.height();
        let x0 = plot.left() + i as f32 * bin_w;
        let bar = egui::Rect::from_min_max(
            egui::pos2(x0 + 0.5, plot.bottom() - bar_h),
            egui::pos2(x0 + bin_w - 0.5, plot.bottom()),
        );
        let grade = (i as f64 + 0.5) / bins as f64 * HIST_MAX_SLOPE;
        let color = if grade <= FAIRWAY_MAX_GRADE { green } else { hot };
        painter.rect_filled(bar, 0.0, color);
    }

    let tx = plot.left() + (FAIRWAY_MAX_GRADE / HIST_MAX_SLOPE) as f32 * plot.width();
    painter.extend(egui::Shape::dashed_line(
        &[egui::pos2(tx, plot.top()), egui::pos2(tx, plot.bottom())],
        egui::Stroke::new(1.5, egui::Color32::WHITE),
        4.0,
        3.0,
    ));

    let label = |x: f32, s: String| {
        painter.text(
            egui::pos2(x, plot.bottom() + 1.0),
            egui::Align2::CENTER_TOP,
            s,
            egui::FontId::proportional(11.0),
            egui::Color32::LIGHT_GRAY,
        );
    };
    label(plot.left(), "0%".into());
    label(tx, "12%".into());
    label(plot.right(), format!("{}%", (HIST_MAX_SLOPE * 100.0) as i32));

    painter.text(
        egui::pos2(plot.left() + 2.0, rect.top() + 1.0),
        egui::Align2::LEFT_TOP,
        format!("playable ≤12%: {:.1}%", playable * 100.0),
        egui::FontId::proportional(13.0),
        green,
    );
}

/// Elevation profile along the routed course: colored by hole (the map
/// overlay palette), walking stretches in gray, hole numbers at each tee,
/// elevation range on the left, distance ticks below.
fn draw_profile(ui: &mut egui::Ui, p: &ProfileData) {
    let desired = egui::vec2(ui.available_width(), ui.available_height().min(100.0));
    let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));

    let plot = egui::Rect::from_min_max(
        egui::pos2(rect.left() + 44.0, rect.top() + 12.0),
        egui::pos2(rect.right() - 8.0, rect.bottom() - 14.0),
    );
    let span = (p.zmax - p.zmin).max(1.0);
    let (zlo, zhi) = (p.zmin - 0.06 * span, p.zmax + 0.06 * span);
    let to_pos = |d: f32, z: f32| -> egui::Pos2 {
        egui::pos2(
            plot.left() + d / p.total.max(1.0) * plot.width(),
            plot.bottom() - (z - zlo) / (zhi - zlo) * plot.height(),
        )
    };
    let hole_col = |i: usize| {
        let c = golf_viz::HOLE_COLORS[i % 9];
        egui::Color32::from_rgb(c[0] as u8, c[1] as u8, c[2] as u8)
    };
    let walk_col = egui::Color32::from_gray(110);

    // Distance ticks every 500 m.
    let mut d = 0.0f32;
    while d <= p.total {
        let x = to_pos(d, zlo).x;
        painter.line_segment(
            [egui::pos2(x, plot.bottom()), egui::pos2(x, plot.bottom() + 3.0)],
            egui::Stroke::new(1.0, egui::Color32::from_gray(70)),
        );
        painter.text(
            egui::pos2(x, plot.bottom() + 3.0),
            egui::Align2::CENTER_TOP,
            if d == 0.0 { "0".into() } else { format!("{:.1} km", d / 1000.0) },
            egui::FontId::proportional(9.5),
            egui::Color32::from_gray(140),
        );
        d += 500.0;
    }
    // Elevation range labels.
    for (z, label_z) in [(p.zmax, p.zmax), (p.zmin, p.zmin)] {
        painter.text(
            egui::pos2(rect.left() + 40.0, to_pos(0.0, z).y),
            egui::Align2::RIGHT_CENTER,
            format!("{label_z:.0} m"),
            egui::FontId::monospace(9.5),
            egui::Color32::from_gray(150),
        );
    }

    // The profile itself.
    for w in p.pts.windows(2) {
        let (d0, z0, h0, w0) = w[0];
        let (d1, z1, _h1, w1) = w[1];
        let col = if w0 || w1 { walk_col } else { hole_col(h0 as usize) };
        let width = if w0 || w1 { 1.0 } else { 1.8 };
        painter.line_segment(
            [to_pos(d0, z0), to_pos(d1, z1)],
            egui::Stroke::new(width, col),
        );
    }

    // Hole markers: separator + number + par at each tee.
    for i in 0..9 {
        let x = to_pos(p.hole_starts[i], zlo).x;
        painter.line_segment(
            [egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())],
            egui::Stroke::new(0.5, egui::Color32::from_gray(58)),
        );
        painter.text(
            egui::pos2(x + 2.0, plot.top() - 1.0),
            egui::Align2::LEFT_TOP,
            format!("{} · p{}", i + 1, p.pars[i]),
            egui::FontId::proportional(9.5),
            hole_col(i),
        );
    }
}

/// Small LCG step for the "randomize seed" button.
fn lcg(x: u64) -> u64 {
    x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407)
}
