//! `golf-match`: find the seeds whose generated terrain most closely matches
//! each surveyed atlas course.
//!
//! **Score** = RMSE in meters between the course's comparison window (its
//! survey, center-cropped to ≤2 km per axis) and a same-size window of the
//! seed's 2 km world, after removing only the best-fit vertical (DC) offset.
//! Heights stay at absolute scale — no amplitude fitting — so a low-relief
//! seed genuinely cannot impersonate a mountain course. The window slides
//! over the world to find "the closest area inside the 2 km × 2 km".
//!
//! **Two stages**, both parallel over seeds (rayon):
//! 1. every seed at 31.25 m/px against every course, sparse offsets → a
//!    per-course shortlist;
//! 2. shortlisted seeds re-scored at 12.5 m/px with a dense offset sweep.
//!
//! Each seed's parameters come from [`golf_terrain::sample_params`] — the
//! search therefore explores both the noise field *and* the parameter draw.

use golf_atlas::Atlas;
use golf_core::{Grid, GridSpec, Vec2};
use golf_terrain::{generate_course, EROSION_VERSION, SAMPLER_VERSION, WATER_VERSION, WORLD_SIZE};
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

/// Bump when scoring/stage semantics change — invalidates cached results.
pub const SEARCH_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct SearchConfig {
    pub seed_start: u64,
    pub seed_count: u64,
    /// Stage-1 raster resolution (m/px) — every seed, sparse offsets.
    pub stage1_mpp: f64,
    /// Stage-2 raster resolution (m/px) — shortlist, dense offsets.
    pub stage2_mpp: f64,
    /// Per-course stage-1 survivors.
    pub shortlist: usize,
    /// Final matches kept per course.
    pub keep: usize,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            seed_start: 1,
            seed_count: 100_000,
            stage1_mpp: 31.25,
            stage2_mpp: 12.5,
            shortlist: 250,
            keep: 10,
        }
    }
}

/// Live progress, shared with the UI thread. Stage: 0 idle, 1/2 = stages, 3 done.
#[derive(Default)]
pub struct Progress {
    pub stage: AtomicU8,
    pub done: AtomicU64,
    pub total: AtomicU64,
    pub cancel: AtomicBool,
}

impl Progress {
    fn begin(&self, stage: u8, total: u64) {
        self.stage.store(stage, Ordering::Relaxed);
        self.total.store(total, Ordering::Relaxed);
        self.done.store(0, Ordering::Relaxed);
    }

    pub fn fraction(&self) -> f32 {
        let t = self.total.load(Ordering::Relaxed);
        if t == 0 {
            return 0.0;
        }
        self.done.load(Ordering::Relaxed) as f32 / t as f32
    }

    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::Relaxed)
    }
}

/// One matched seed: score plus where the course window sits in the world
/// (meters, SW corner).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SeedScore {
    pub seed: u64,
    pub rmse: f64,
    pub off_x: f64,
    pub off_y: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CourseMatches {
    pub course_key: String,
    /// Best first, `keep` entries.
    pub best: Vec<SeedScore>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchResults {
    /// Hash of atlas + config + generator versions; guards stale caches.
    pub fingerprint: u64,
    pub config: SearchConfig,
    pub elapsed_s: f64,
    pub matches: Vec<CourseMatches>,
}

/// Fingerprint binding results to the atlas content, the search config, and
/// the sampler/search algorithm versions.
pub fn fingerprint(atlas: &Atlas, config: &SearchConfig) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    let mut eat = |v: u64| {
        for b in v.to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    };
    eat(atlas.fingerprint);
    eat(config.seed_start);
    eat(config.seed_count);
    eat(config.stage1_mpp.to_bits());
    eat(config.stage2_mpp.to_bits());
    eat(config.shortlist as u64);
    eat(config.keep as u64);
    eat(SAMPLER_VERSION as u64);
    eat(SEARCH_VERSION as u64);
    eat(EROSION_VERSION as u64);
    eat(WATER_VERSION as u64);
    h
}

// ---------------------------------------------------------------------------
// Rasters
// ---------------------------------------------------------------------------

/// Spec sampling an `n × n` raster of the 2 km world at pixel centers
/// (`n = 2000 / mpp`, which must divide evenly).
pub fn world_raster_spec(mpp: f64) -> GridSpec {
    let n = (WORLD_SIZE / mpp).round() as u32;
    GridSpec::new(Vec2::new(mpp / 2.0, mpp / 2.0), mpp, n, n)
}

/// Spec sampling a `win_w × win_h` window of the world at pixel centers, with
/// its SW corner at `(off_x, off_y)` — for regenerating a matched window at
/// display resolution.
pub fn window_raster_spec(win_w: f64, win_h: f64, off_x: f64, off_y: f64, mpp: f64) -> GridSpec {
    let nx = ((win_w / mpp).round() as u32).max(2);
    let ny = ((win_h / mpp).round() as u32).max(2);
    GridSpec::new(Vec2::new(off_x + mpp / 2.0, off_y + mpp / 2.0), mpp, nx, ny)
}

/// A course template prepared for fast scoring.
struct PrepTemplate {
    nx: usize,
    ny: usize,
    data: Vec<f64>,
    sum: f64,
    sumsq: f64,
}

fn prep_templates(atlas: &Atlas, mpp: f64) -> Vec<PrepTemplate> {
    atlas
        .courses
        .iter()
        .map(|c| {
            let t = c.template(mpp);
            let data = t.grid.data;
            let sum: f64 = data.iter().sum();
            let sumsq: f64 = data.iter().map(|v| v * v).sum();
            PrepTemplate {
                nx: t.grid.spec.nx as usize,
                ny: t.grid.spec.ny as usize,
                data,
                sum,
                sumsq,
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Mean-removed MSE of the template placed at `(dx, dy)` in the world raster.
fn mse_at(world: &[f64], wn: usize, t: &PrepTemplate, dx: usize, dy: usize) -> f64 {
    let mut sg = 0.0f64;
    let mut sgg = 0.0f64;
    let mut sgt = 0.0f64;
    for row in 0..t.ny {
        let w = &world[(dy + row) * wn + dx..(dy + row) * wn + dx + t.nx];
        let tr = &t.data[row * t.nx..(row + 1) * t.nx];
        for i in 0..t.nx {
            let g = w[i];
            sg += g;
            sgg += g * g;
            sgt += g * tr[i];
        }
    }
    let n = (t.nx * t.ny) as f64;
    let mu = (sg - t.sum) / n; // best-fit DC offset
    ((sgg - 2.0 * sgt + t.sumsq) / n - mu * mu).max(0.0)
}

/// Offsets to try along one axis: `0..=slack` stepped, always including both
/// ends.
fn axis_offsets(slack: usize, step: usize) -> Vec<usize> {
    let step = step.max(1);
    let mut v: Vec<usize> = (0..=slack).step_by(step).collect();
    if *v.last().unwrap() != slack {
        v.push(slack);
    }
    v
}

/// Best (lowest-MSE) placement of `t` in the world raster.
fn best_placement(world: &[f64], wn: usize, t: &PrepTemplate, step: usize) -> (f64, usize, usize) {
    let sx = wn - t.nx;
    let sy = wn - t.ny;
    let mut best = (f64::INFINITY, 0usize, 0usize);
    for &dy in &axis_offsets(sy, step) {
        for &dx in &axis_offsets(sx, step) {
            let m = mse_at(world, wn, t, dx, dy);
            if m < best.0 {
                best = (m, dx, dy);
            }
        }
    }
    best
}

// ---------------------------------------------------------------------------
// Search driver
// ---------------------------------------------------------------------------

const CHUNK: u64 = 512;

/// Run the full two-stage search. Returns `None` if cancelled.
pub fn run_search(atlas: &Atlas, config: &SearchConfig, progress: &Progress) -> Option<SearchResults> {
    let t0 = std::time::Instant::now();
    let n_courses = atlas.courses.len();
    let shortlist = config.shortlist.max(1);

    // ---- Stage 1: every seed, coarse raster, sparse offsets --------------
    let templates1 = prep_templates(atlas, config.stage1_mpp);
    let spec1 = world_raster_spec(config.stage1_mpp);
    let wn1 = spec1.nx as usize;
    // Sparse: about 4 positions per axis.
    let step1 = ((wn1 as f64) / 16.0).ceil() as usize;

    progress.begin(1, config.seed_count);

    let chunk_ranges: Vec<(u64, u64)> = {
        let mut v = Vec::new();
        let mut s = config.seed_start;
        let end = config.seed_start + config.seed_count;
        while s < end {
            let e = (s + CHUNK).min(end);
            v.push((s, e));
            s = e;
        }
        v
    };

    // Per chunk: per-course (mse, seed) candidates, truncated to `shortlist`.
    let chunk_results: Vec<Vec<Vec<(f64, u64)>>> = chunk_ranges
        .par_iter()
        .map(|&(s0, s1)| {
            let mut local: Vec<Vec<(f64, u64)>> = vec![Vec::new(); n_courses];
            if progress.cancelled() {
                return local;
            }
            for seed in s0..s1 {
                let (world, _) = generate_course(&spec1, seed);
                let world = world.terrain;
                for (ci, t) in templates1.iter().enumerate() {
                    let (mse, _, _) = best_placement(&world.heights.data, wn1, t, step1);
                    local[ci].push((mse, seed));
                }
                progress.done.fetch_add(1, Ordering::Relaxed);
                if progress.cancelled() {
                    break;
                }
            }
            for l in &mut local {
                l.sort_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap());
                l.truncate(shortlist);
            }
            local
        })
        .collect();

    if progress.cancelled() {
        return None;
    }

    let mut stage1: Vec<Vec<(f64, u64)>> = vec![Vec::new(); n_courses];
    for chunk in chunk_results {
        for (ci, l) in chunk.into_iter().enumerate() {
            stage1[ci].extend(l);
        }
    }
    for l in &mut stage1 {
        l.sort_by(|a, b| (a.0, a.1).partial_cmp(&(b.0, b.1)).unwrap());
        l.truncate(shortlist);
    }

    // ---- Stage 2: shortlisted seeds, fine raster, dense offsets ----------
    let templates2 = prep_templates(atlas, config.stage2_mpp);
    let spec2 = world_raster_spec(config.stage2_mpp);
    let wn2 = spec2.nx as usize;
    // Dense: ~25 m steps.
    let step2 = ((25.0 / config.stage2_mpp).round() as usize).max(1);

    // seed → indices of courses that shortlisted it.
    let mut seed_courses: HashMap<u64, Vec<usize>> = HashMap::new();
    for (ci, l) in stage1.iter().enumerate() {
        for &(_, seed) in l {
            seed_courses.entry(seed).or_default().push(ci);
        }
    }
    let mut candidates: Vec<u64> = seed_courses.keys().copied().collect();
    candidates.sort_unstable();

    progress.begin(2, candidates.len() as u64);

    let rescored: Vec<Vec<(usize, SeedScore)>> = candidates
        .par_iter()
        .map(|&seed| {
            let mut out = Vec::new();
            if progress.cancelled() {
                return out;
            }
            let (world, _) = generate_course(&spec2, seed);
            let world = world.terrain;
            for &ci in &seed_courses[&seed] {
                let t = &templates2[ci];
                let (mse, dx, dy) = best_placement(&world.heights.data, wn2, t, step2);
                out.push((
                    ci,
                    SeedScore {
                        seed,
                        rmse: mse.sqrt(),
                        off_x: dx as f64 * config.stage2_mpp,
                        off_y: dy as f64 * config.stage2_mpp,
                    },
                ));
            }
            progress.done.fetch_add(1, Ordering::Relaxed);
            out
        })
        .collect();

    if progress.cancelled() {
        return None;
    }

    let mut per_course: Vec<Vec<SeedScore>> = vec![Vec::new(); n_courses];
    for group in rescored {
        for (ci, s) in group {
            per_course[ci].push(s);
        }
    }

    let matches = atlas
        .courses
        .iter()
        .zip(per_course)
        .map(|(c, mut best)| {
            best.sort_by(|a, b| (a.rmse, a.seed).partial_cmp(&(b.rmse, b.seed)).unwrap());
            best.truncate(config.keep);
            CourseMatches {
                course_key: c.key.clone(),
                best,
            }
        })
        .collect();

    progress.stage.store(3, Ordering::Relaxed);
    Some(SearchResults {
        fingerprint: fingerprint(atlas, config),
        config: *config,
        elapsed_s: t0.elapsed().as_secs_f64(),
        matches,
    })
}

// ---------------------------------------------------------------------------
// Result cache
// ---------------------------------------------------------------------------

/// Where search results are cached in this repo (`output/seed_search.json`).
pub fn default_results_path() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .map(|p| p.join("output/seed_search.json"))
        .unwrap_or_else(|| std::path::PathBuf::from("output/seed_search.json"))
}

pub fn save_results(results: &SearchResults, path: &Path) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let json = serde_json::to_string_pretty(results)?;
    std::fs::write(path, json)
}

pub fn load_results(path: &Path) -> std::io::Result<SearchResults> {
    let json = std::fs::read_to_string(path)?;
    serde_json::from_str(&json).map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))
}

/// Regenerate the matched window of a seed's world at display resolution.
/// Returns (heights, DC offset µ) where µ = mean(gen) − mean(course window):
/// subtract µ from generated heights to overlay the course's absolute scale.
pub fn regenerate_window(
    seed: u64,
    win_w: f64,
    win_h: f64,
    off_x: f64,
    off_y: f64,
    mpp: f64,
    course_window_mean: f64,
) -> (Grid<f64>, f64, Grid<f64>) {
    let spec = window_raster_spec(win_w, win_h, off_x, off_y, mpp);
    let (ct, _) = generate_course(&spec, seed);
    let mut heights = ct.terrain.heights;
    // The seed's water over the same window, as a 0/1-ish mask in local
    // coordinates (for the Match tab's water overlay).
    let mut water = Grid::filled(spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let p = spec.world_of(x, y);
            if ct.water.depth.bilinear(p) > 0.02 {
                water.set(x, y, 1.0);
            }
        }
    }
    // Rebase to local window coordinates so renderers (which sample the
    // grid's own [0, extent] space) see the whole window, not a clamped edge.
    heights.spec.origin = Vec2::ZERO;
    water.spec.origin = Vec2::ZERO;
    let mean = heights.data.iter().sum::<f64>() / heights.data.len().max(1) as f64;
    (heights, mean - course_window_mean, water)
}

#[cfg(test)]
mod tests {
    use super::*;
    use golf_atlas::{Course, StyleGroup, GRID_N};

    /// A course whose window is an exact copy of what some seed generates —
    /// the search must find that seed with ~zero RMSE.
    fn planted_atlas(seed: u64) -> Atlas {
        // Full-pipeline field, point-sampled at the source-grid geometry of a
        // fake 1500×1200 m survey (pixel centers, row 0 = north), offset
        // (100, 200) m. `height_at` is the same compose the search scores.
        let (ct, _) = generate_course(&golf_terrain::macro_spec(), seed);
        let (wm, hm) = (1500.0, 1200.0);
        let (off_x, off_y) = (100.0, 200.0);
        let mut heights = vec![0.0f32; GRID_N * GRID_N];
        let cw = wm / GRID_N as f64;
        let ch = hm / GRID_N as f64;
        for row in 0..GRID_N {
            for col in 0..GRID_N {
                let x = off_x + (col as f64 + 0.5) * cw;
                // row 0 = north = max y in world coords.
                let y = off_y + hm - (row as f64 + 0.5) * ch;
                heights[row * GRID_N + col] = (ct.height_at(Vec2::new(x, y)) + 300.0) as f32;
            }
        }
        let course = Course {
            key: "planted".into(),
            label: "Planted".into(),
            arch: String::new(),
            group: StyleGroup::Rolling,
            wm,
            hm,
            emin: 0.0,
            emax: 1000.0,
            heights,
            water: vec![0; GRID_N * GRID_N],
            trees: vec![0; GRID_N * GRID_N],
            waterpct: 0.0,
            treepct: 0.0,
            bbox: [0.0; 4],
            holes: Vec::new(),
        };
        let fingerprint = golf_atlas::binfmt::fingerprint(std::slice::from_ref(&course));
        Atlas {
            courses: vec![course],
            fingerprint,
        }
    }

    #[test]
    fn search_recovers_a_planted_seed() {
        let planted = 77u64;
        let atlas = planted_atlas(planted);
        let config = SearchConfig {
            seed_start: 70,
            seed_count: 16,
            shortlist: 8,
            keep: 3,
            ..Default::default()
        };
        let progress = Progress::default();
        let results = run_search(&atlas, &config, &progress).unwrap();
        let best = &results.matches[0].best[0];
        assert_eq!(best.seed, planted, "planted seed must win: {:?}", results.matches[0].best);
        // RMSE limited only by the survey's decimeter quantization + resampling.
        assert!(best.rmse < 1.0, "rmse {} too high", best.rmse);
        // The +300 m datum shift must be absorbed by the DC-offset removal.
    }

    #[test]
    fn canonical_macro_spec_is_the_stage2_raster() {
        // Stage 2 scores the erosion macro grid's own nodes: the two specs
        // must be identical or matching fairness breaks.
        assert_eq!(world_raster_spec(12.5), golf_terrain::macro_spec());
    }

    #[test]
    fn cancel_returns_none() {
        let atlas = planted_atlas(3);
        let progress = Progress::default();
        progress.cancel.store(true, Ordering::Relaxed);
        let r = run_search(&atlas, &SearchConfig { seed_count: 8, ..Default::default() }, &progress);
        assert!(r.is_none());
    }

    #[test]
    fn results_roundtrip_json() {
        let r = SearchResults {
            fingerprint: 9,
            config: SearchConfig::default(),
            elapsed_s: 1.5,
            matches: vec![CourseMatches {
                course_key: "x".into(),
                best: vec![SeedScore { seed: 1, rmse: 2.0, off_x: 0.0, off_y: 12.5 }],
            }],
        };
        let dir = std::env::temp_dir().join("golf-match-test");
        let p = dir.join("r.json");
        save_results(&r, &p).unwrap();
        let back = load_results(&p).unwrap();
        assert_eq!(back.fingerprint, 9);
        assert_eq!(back.matches[0].best[0].seed, 1);
        std::fs::remove_file(&p).ok();
    }
}
