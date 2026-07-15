//! Natural water coverage derived from the eroded terrain — no global level.
//!
//! Three mechanisms, all driven by the erosion pass's canonical flow tree:
//! - **Streams** (brooks/creeks/rivers): cells whose drainage area exceeds a
//!   sampled channel-initiation threshold, traced into links with hydraulic
//!   widths and a monotone non-increasing water surface.
//! - **Ponds/lakes**: depression basins (from `lake_depth`) filled to their
//!   spill, greedily selected into the coverage budget.
//! - **Wetlands** via HAND (height above nearest drainage): the lowest-lying
//!   ground relative to the local stream surface gets wet — the mechanism
//!   that naturally gives lowland courses sprawling coverage and confines
//!   mountain water to valley floors.
//!
//! The build is RNG-free (all stochastic choices arrive in [`WaterParams`]);
//! organic wetland edges come from hash noise on a seed channel carried in
//! the params. Entities (links, bodies) are the source of truth; the grids
//! are rebuilt from them, so the later routing phase can edit entities and
//! re-derive without drift.

use crate::erosion::ErosionOutputs;
use crate::noise;
use golf_core::{Grid, Vec2};
use serde::{Deserialize, Serialize};

/// Bump when water semantics change — folded into search fingerprints.
pub const WATER_VERSION: u32 = 8;

/// Water cell classes in [`WaterLayer::class`].
pub const CLASS_DRY: u8 = 0;
pub const CLASS_STREAM: u8 = 1;
pub const CLASS_POND: u8 = 2;
pub const CLASS_LAKE: u8 = 3;
pub const CLASS_WETLAND: u8 = 4;

/// Playability guards (fixed constants; gated by `xtask water-report`).
const MIN_POND_CELLS: usize = 3;
const MIN_WETLAND_CELLS: usize = 5;
/// Per-body cap ≈ 22 ha — a ~530 m circle or ~660×330 m oval, the top of the
/// golf-hole size range. Bigger basins split into pocket lakes at this cap
/// (the freeze fill in `select_bodies`), not one mega-lake.
const MAX_BODY_WORLD_SHARE: f64 = 0.055;
const MAX_BODY_WORLD_SHARE_LAKE_COURSE: f64 = 0.14;
const MAX_STREAM_WORLD_SHARE: f64 = 0.03;
const DRY_CONNECTIVITY_MIN: f64 = 0.70;
/// Lake-field courses (coverage ≥ 12%): the ground between the lakes is the
/// course — a lower connected-dry floor is the design, not an accident.
const DRY_CONNECTIVITY_MIN_LAKE_FIELD: f64 = 0.60;
/// River courses legitimately split the world (holes cross on bridges and
/// carries — Phase B places the crossings); the larger side still must hold
/// most of a routing. A major river (50–90 m) removes ~5% of the world and
/// splits the rest near-evenly, hence the lower floor.
const DRY_CONNECTIVITY_MIN_RIVER: f64 = 0.45;
/// Streams narrower than this are carry hazards, not barriers.
const BLOCKING_STREAM_WIDTH: f64 = 8.0;
const LAKE_MIN_CELLS: usize = 128; // 2 ha at 12.5 m cells
const WETLAND_MAX_BASIN_DEPTH: f64 = 0.4;
const WETLAND_SLOPE_MAX: f64 = 0.04;
const WETLAND_NOISE_AMP: f64 = 0.3;
const WETLAND_NOISE_PERIOD: f64 = 300.0;
/// Hydraulic width `w = K_WIDTH·sqrt(runoff·A)`, clamped. (The 90 m ceiling
/// only comes into play for major external rivers.)
const K_WIDTH: f64 = 0.005;
const WIDTH_MIN: f64 = 2.5;
const WIDTH_MAX: f64 = 90.0;
/// Visible (perennial) channels need this multiple of the swale-initiation
/// drainage area — the swale tier below keeps tracing the fine network as
/// HAND reference + routing metadata, but only collected mains carry water.
const SWALE_AREA_DIVISOR: f64 = 10.0;
/// Body-outline regularization: how far (in macro cells) the smoothed
/// outline may move from the raw basin boundary, the target depth of cells
/// the rounding adds, and the deepest terrain cut the rounding may make
/// (cells needing more stay dry — the outline keeps the hill).
const REG_BAND_CELLS: f64 = 1.6;
const REG_DIG_DEPTH: f64 = 0.4;
const REG_DIG_MAX: f64 = 1.2;
/// Proportion guard. Deliberate lakes are golf-hole scale — ~400 ± 200 m
/// long at 1:1 up to ~1:8 length:width — so three rules cull the accidents:
/// elongated bodies (> ASPECT_MAX) that are also NARROW (< ASPECT_WIDTH_MIN
/// — string-of-cells snakes), anything beyond the hard 1:8 spec regardless
/// of width, and anything shorter than the size floor.
const ASPECT_MAX: f64 = 3.2;
const ASPECT_WIDTH_MIN: f64 = 22.0;
const ASPECT_HARD_MAX: f64 = 8.5;
const BODY_LEN_MIN: f64 = 180.0;
/// Stream heads that start in plain ground snap to the nearest map edge or
/// standing body within this many macro cells (~150 m) — a straight feeder
/// channel is prepended. Farther than that, the link is removed outright
/// (no source ponds).
const HEAD_SNAP_CELLS: i32 = 12;
/// Perennial links shorter than this are stubs — culled (targeted removal).
const MIN_LINK_LENGTH_M: f64 = 150.0;
/// Lone-stream handling: a small isolated stream (touching no other stream
/// and no pond/lake) must end up with pond proportions — length : width of
/// roughly 4–5 : 1 — or be removed. (Ratio-based per user direction: a
/// fattened thread is still a thread.)
const LONE_SMALL_WIDTH: f64 = 6.0;
/// Target length:width ratio band for pooled lone streams.
const LONE_RATIO_MIN: f64 = 4.0;
const LONE_RATIO_MAX: f64 = 5.0;
/// Pools needing more width than this to reach the ratio are removed instead.
const POOL_MAX_WIDTH: f64 = 80.0;
/// Lone links dropping more than this along their run are too steep to pool.
const LONE_MAX_DROP: f64 = 2.5;
/// Centerline geometry: resample spacing, smoothing passes, and the meander
/// law (amplitude grows with channel width, wavelength ≈ 14×width — the real
/// hydrological ratio; attenuated on steep ground so channels stay in their
/// valleys). This is what breaks D8's ruler-straight runs.
const RESAMPLE_M: f64 = 10.0;
const SMOOTH_PASSES: usize = 4;
const MEANDER_AMP_PER_W: f64 = 2.0;
const MEANDER_AMP_BASE: f64 = 12.0;
const MEANDER_WAVELENGTH_PER_W: f64 = 13.0;
const MEANDER_WAVELENGTH_MIN: f64 = 80.0;
/// Amplitude ceiling as a fraction of wavelength (real sinuous channels sit
/// around 0.1–0.3).
const MEANDER_AMP_LAMBDA_CAP: f64 = 0.28;
/// Extra meander on near-flat ground (the deposition floors where D8 runs
/// dead straight — the lowland straightness complaint).
const MEANDER_FLAT_BOOST: f64 = 1.5;
/// Channel bed inset depth = clamp(0.10·w, …) and bank pull-down grade.
const CHANNEL_DEPTH_FACTOR: f64 = 0.10;
const CHANNEL_DEPTH_MIN: f64 = 0.3;
const CHANNEL_DEPTH_MAX: f64 = 1.0;
const BANK_GRADE: f64 = 0.30;
const BANK_MAX_CUT: f64 = 1.0;

/// Sampled per-seed water axes (drawn in the sampler; `NONE` = dry pipeline,
/// bit-identical to no water pass at all).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaterParams {
    /// Target water fraction of the world area (0 = dry course).
    pub coverage: f64,
    /// Channel-initiation drainage area, m².
    pub stream_area_min: f64,
    /// Discharge/width factor in the hydraulic-geometry width.
    pub runoff: f64,
    /// Share of the non-stream budget that goes to ponds/lakes (the rest
    /// becomes HAND wetland).
    pub pond_share: f64,
    /// Cap on the wetland share of total coverage (relief-conditioned).
    pub wetland_share_cap: f64,
    /// Mountain lake-course tail: force the dominant basin to fill first.
    pub lake_course: bool,
    /// External upstream catchment (m²) injected at the world edge — a
    /// through-river fed by land beyond the map (0 = none). A 2 km world's
    /// internal catchments alone can never make more than a ~6 m creek; real
    /// river courses (Banff, Colonial) sit on water fed from far upstream.
    pub river_area: f64,
    /// Minimum visible stream width, m (0 = keep everything). Rolling and
    /// mountain courses keep only wide water: links narrower than ~60% of
    /// this are culled, survivors are floored to it.
    pub min_stream_width: f64,
    /// Minimum standing-water body area, m² (mountains keep only sizable
    /// lakes; lowland keeps its fine pond field).
    pub min_body_area: f64,
    /// Seed channel for the organic wetland-edge / meander noise.
    pub noise_seed: u32,
}

impl WaterParams {
    pub const NONE: WaterParams = WaterParams {
        coverage: 0.0,
        stream_area_min: 8.0e4,
        runoff: 1.0,
        pond_share: 0.7,
        wetland_share_cap: 0.5,
        lake_course: false,
        river_area: 0.0,
        min_stream_width: 0.0,
        min_body_area: 470.0,
        noise_seed: 0,
    };
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamClass {
    Brook,
    Creek,
    River,
}

/// A maximal channel chain between head/junction and junction/outlet.
#[derive(Clone, Debug)]
pub struct StreamLink {
    /// Macro cell indices along the D8 flow path, downstream order (the
    /// topology; geometry lives in `pts`).
    pub cells: Vec<u32>,
    /// Resampled (~10 m), smoothed, meandered centerline, downstream order.
    pub pts: Vec<Vec2>,
    /// Water-surface width per centerline point, meters.
    pub width: Vec<f64>,
    /// Water surface elevation per centerline point (non-increasing).
    pub surface: Vec<f64>,
    pub class: StreamClass,
    /// False = traced dry swale (routing metadata, no water, no carve).
    pub perennial: bool,
    /// A lone small stream widened into a thin lake: flat surface, 4–6× the
    /// channel width, classed (and rendered) as a pond.
    pub pooled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BodyClass {
    Pond,
    Lake,
    Wetland,
}

/// A standing-water body (or wetland patch).
#[derive(Clone, Debug)]
pub struct WaterBody {
    pub class: BodyClass,
    /// Wet cells at the current stage, ascending index.
    pub cells: Vec<u32>,
    /// Water surface (flat for ponds/lakes; per-cell terrain+0.1 for
    /// wetlands, recorded here as the mean).
    pub surface: f64,
    pub spill: f64,
    pub max_depth: f64,
    pub area_m2: f64,
}

/// Fine water raster resolution (2.5 m cells over the 2 km world — 5× the
/// macro grid). Streams are re-stamped from their vector centerlines, so
/// diagonal channels stay connected ribbons instead of corner-touching
/// macro-pixel chains.
pub const FINE_N: u32 = 800;

/// The derived water layer. Entities first; grids rebuilt from them.
#[derive(Clone, Debug)]
pub struct WaterLayer {
    /// CLASS_* per macro cell.
    pub class: Grid<u8>,
    /// Water surface where wet, terrain elsewhere.
    pub surface: Grid<f64>,
    /// Water depth (surface − carved terrain), ≥ 0.
    pub depth: Grid<f64>,
    /// 1 on water, one-cell falloff on banks, 0 elsewhere (compose gate +
    /// render blending).
    pub mask_soft: Grid<f64>,
    /// CLASS_* at `FINE_N²` (2.5 m): the display/zone-resolution class
    /// raster — streams from vector centerlines, standing water/wetland
    /// boundaries from the smoothed SDF isolines below. Built once at the
    /// end of `build` (semantic/gameplay rules stay on the macro grid).
    pub fine_class: Grid<u8>,
    /// Smoothed signed distance (m, + = wet) to the standing-water boundary
    /// (ponds + lakes) on the macro grid. `bilinear(p) ≥ 0` is the smooth
    /// implicit outline — renderers sample it at any zoom. A ±half-cell
    /// protection band keeps islands/isthmuses/thin arms intact.
    pub standing_sdf: Grid<f64>,
    /// Same for wetlands (standing water wins where both are positive).
    pub wetland_sdf: Grid<f64>,
    /// Thin connector capsules for diagonal-only wet pairs (the SDF saddle
    /// is ambiguous: a wet diagonal reads like a dry isthmus, and the
    /// protection band keeps both — these resolve WATER connectivity with
    /// an organic thin neck instead). `(a, b, class)` cell-center segments;
    /// class is CLASS_POND (standing family) or CLASS_WETLAND.
    pub water_bridges: Vec<(Vec2, Vec2, u8)>,
    pub streams: Vec<StreamLink>,
    pub bodies: Vec<WaterBody>,
    /// Realized coverage fraction (analytic stream area + wet cells).
    pub coverage: f64,
}

impl WaterLayer {
    /// Does this cell block a hole routing? Ponds, lakes, wetlands, and
    /// river-class channels block; brooks/creeks are crossable hazards.
    pub fn is_blocking(&self, i: usize) -> bool {
        match self.class.data[i] {
            CLASS_POND | CLASS_LAKE | CLASS_WETLAND => true,
            CLASS_STREAM => self.depth.data[i] >= 0.75,
            _ => false,
        }
    }

    /// Water class at a world point, at display resolution (falls back to
    /// the macro grid when the fine raster hasn't been built).
    pub fn class_fine_at(&self, p: Vec2) -> u8 {
        let s = if self.fine_class.spec.len() > 4 {
            &self.fine_class
        } else {
            &self.class
        };
        let gx = ((p.x / s.spec.cell_size) as i64).clamp(0, s.spec.nx as i64 - 1) as u32;
        let gy = ((p.y / s.spec.cell_size) as i64).clamp(0, s.spec.ny as i64 - 1) as u32;
        *s.get(gx, gy)
    }

    fn empty(spec: golf_core::GridSpec) -> Self {
        WaterLayer {
            class: Grid::filled(spec, CLASS_DRY),
            surface: Grid::filled(spec, 0.0),
            depth: Grid::filled(spec, 0.0),
            mask_soft: Grid::filled(spec, 0.0),
            fine_class: Grid::filled(golf_core::GridSpec::new(Vec2::ZERO, 1.0, 2, 2), CLASS_DRY),
            standing_sdf: Grid::filled(spec, -1.0e9),
            wetland_sdf: Grid::filled(spec, -1.0e9),
            water_bridges: Vec::new(),
            streams: Vec::new(),
            bodies: Vec::new(),
            coverage: 0.0,
        }
    }

    /// Is `p` in standing water (pond/lake) under the smooth outline?
    pub fn standing_at(&self, p: Vec2) -> bool {
        self.standing_sdf.bilinear(p) >= 0.0
    }

    /// Is `p` in wetland under the smooth outline (standing water wins)?
    pub fn wetland_at(&self, p: Vec2) -> bool {
        self.wetland_sdf.bilinear(p) >= 0.0 && !self.standing_at(p)
    }

    /// Build the smooth standing/wetland SDF outlines + the fine class
    /// raster. Blob boundaries come from smoothed signed-distance isolines
    /// (a binary-mask threshold hugs cell edges and steps on thin diagonal
    /// features); a ±half-cell protection band pins every decisively-wet or
    /// decisively-dry cell, so islands, isthmuses, and one-cell arms
    /// survive — smoothing only rounds the boundary within its own band.
    /// Streams stamp their smooth vector corridors directly.
    fn build_fine(&mut self) {
        let mspec = self.class.spec;
        let cell_f = crate::WORLD_SIZE / FINE_N as f64;
        let fspec = golf_core::GridSpec::new(
            Vec2::new(cell_f / 2.0, cell_f / 2.0),
            cell_f,
            FINE_N,
            FINE_N,
        );
        let mut fine = Grid::filled(fspec, CLASS_DRY);

        let standing_mask: Vec<bool> = self
            .class
            .data
            .iter()
            .map(|&c| c == CLASS_POND || c == CLASS_LAKE)
            .collect();
        let wet_mask: Vec<bool> = self.class.data.iter().map(|&c| c == CLASS_WETLAND).collect();
        self.standing_sdf = smooth_mask_sdf(&standing_mask, mspec);
        self.wetland_sdf = smooth_mask_sdf(&wet_mask, mspec);

        // Diagonal saddles: a diagonal wet pair whose shared corner cells
        // are both dry gets a thin connector capsule (the smooth outlines
        // keep the blobs apart; a real water chain should connect).
        self.water_bridges.clear();
        {
            let (mnx, mny) = (mspec.nx as usize, mspec.ny as usize);
            let any_wet = |i: usize| standing_mask[i] || wet_mask[i];
            for y in 0..mny - 1 {
                for x in 0..mnx - 1 {
                    let i00 = y * mnx + x;
                    let i10 = i00 + 1;
                    let i01 = i00 + mnx;
                    let i11 = i01 + 1;
                    let mut push = |a: usize, b: usize| {
                        let fam = if standing_mask[a] && standing_mask[b] {
                            CLASS_POND
                        } else {
                            CLASS_WETLAND
                        };
                        let pa = mspec.world_of((a % mnx) as u32, (a / mnx) as u32);
                        let pb = mspec.world_of((b % mnx) as u32, (b / mnx) as u32);
                        self.water_bridges.push((pa, pb, fam));
                    };
                    if any_wet(i00) && any_wet(i11) && !any_wet(i10) && !any_wet(i01) {
                        push(i00, i11);
                    } else if any_wet(i10) && any_wet(i01) && !any_wet(i00) && !any_wet(i11) {
                        push(i10, i01);
                    }
                }
            }
        }

        // Fine class raster from the smooth outlines (coarse consumers).
        let (mnx, mny) = (mspec.nx as usize, mspec.ny as usize);
        let any_wet = standing_mask.iter().chain(&wet_mask).any(|&b| b);
        if any_wet {
            let ratio = (FINE_N as usize).div_ceil(mnx);
            for my in 0..mny {
                for mx in 0..mnx {
                    // Visit only cells whose 3×3 macro neighborhood touches water.
                    let mut near = false;
                    'scan: for dy in -1i64..=1 {
                        for dx in -1i64..=1 {
                            let x = mx as i64 + dx;
                            let y = my as i64 + dy;
                            if x < 0 || y < 0 || x >= mnx as i64 || y >= mny as i64 {
                                continue;
                            }
                            let j = y as usize * mnx + x as usize;
                            if standing_mask[j] || wet_mask[j] {
                                near = true;
                                break 'scan;
                            }
                        }
                    }
                    if !near {
                        continue;
                    }
                    for sy in 0..ratio {
                        for sx in 0..ratio {
                            let fx = (mx * ratio + sx) as u32;
                            let fy = (my * ratio + sy) as u32;
                            if fx >= FINE_N || fy >= FINE_N {
                                continue;
                            }
                            let p = fspec.world_of(fx, fy);
                            if self.standing_sdf.bilinear(p) >= 0.0 {
                                let mc = self.class.data[my * mnx + mx];
                                let cls = if mc == CLASS_LAKE { CLASS_LAKE } else { CLASS_POND };
                                fine.set(fx, fy, cls);
                            } else if self.wetland_sdf.bilinear(p) >= 0.0 {
                                fine.set(fx, fy, CLASS_WETLAND);
                            }
                        }
                    }
                }
            }
        }

        // Bridge capsules into the fine raster (thin necks, radius 3 m).
        for &(a, b, fam) in &self.water_bridges {
            let hw = 3.0f64;
            let x0 = (((a.x.min(b.x) - hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
            let x1 = (((a.x.max(b.x) + hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
            let y0 = (((a.y.min(b.y) - hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
            let y1 = (((a.y.max(b.y) + hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
            let ab = b - a;
            let len2 = ab.dot(ab).max(1e-12);
            for gy in y0..=y1 {
                for gx in x0..=x1 {
                    let p = fspec.world_of(gx as u32, gy as u32);
                    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
                    if p.distance(a + ab * t) <= hw && *fine.get(gx as u32, gy as u32) == CLASS_DRY {
                        fine.set(gx as u32, gy as u32, fam);
                    }
                }
            }
        }

        // Streams: smooth vector corridors stamped at fine resolution
        // (pooled lone streams read as ponds). Standing water wins.
        for l in &self.streams {
            if !l.perennial {
                continue;
            }
            let cls = if l.pooled { CLASS_POND } else { CLASS_STREAM };
            for k in 1..l.pts.len() {
                let (a, b) = (l.pts[k - 1], l.pts[k]);
                let hw = (0.5 * 0.5 * (l.width[k - 1] + l.width[k])).max(cell_f * 0.6);
                let x0 = (((a.x.min(b.x) - hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
                let x1 = (((a.x.max(b.x) + hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
                let y0 = (((a.y.min(b.y) - hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
                let y1 = (((a.y.max(b.y) + hw) / cell_f) as i64).clamp(0, FINE_N as i64 - 1);
                let ab = b - a;
                let len2 = ab.dot(ab).max(1e-12);
                for gy in y0..=y1 {
                    for gx in x0..=x1 {
                        let p = fspec.world_of(gx as u32, gy as u32);
                        let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
                        let q = a + ab * t;
                        if p.distance(q) <= hw {
                            let cur = *fine.get(gx as u32, gy as u32);
                            if cur != CLASS_POND && cur != CLASS_LAKE {
                                fine.set(gx as u32, gy as u32, cls);
                            }
                        }
                    }
                }
            }
        }

        self.fine_class = fine;
    }
}

/// Signed distance (m, + = inside the mask) to the mask boundary, smoothed
/// with two 1-2-1 binomial passes so its zero isoline is an organically
/// rounded outline instead of a cell-edge staircase. A ±half-cell protection
/// band then re-pins every decisively-inside/outside cell: thin one-cell
/// arms, islands, and dry isthmuses can never be smoothed away — the
/// boundary only moves within its own transition band.
fn smooth_mask_sdf(mask: &[bool], spec: golf_core::GridSpec) -> Grid<f64> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    let orth = spec.cell_size;
    let diag = spec.cell_size * SQRT_2;
    let half = 0.5 * spec.cell_size;

    // Empty mask (dry courses, wetland-free seeds): everything is far-dry.
    if !mask.iter().any(|&b| b) {
        return Grid::filled(spec, -1.0e9);
    }

    // Two-pass 3-4 chamfer distance to the nearest seed cell.
    let chamfer = |seed: &dyn Fn(usize) -> bool| -> Vec<f64> {
        let mut d = vec![1.0e9f64; n];
        for (i, di) in d.iter_mut().enumerate() {
            if seed(i) {
                *di = 0.0;
            }
        }
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                let mut v = d[i];
                if x > 0 {
                    v = v.min(d[i - 1] + orth);
                }
                if y > 0 {
                    v = v.min(d[i - nx] + orth);
                    if x > 0 {
                        v = v.min(d[i - nx - 1] + diag);
                    }
                    if x + 1 < nx {
                        v = v.min(d[i - nx + 1] + diag);
                    }
                }
                d[i] = v;
            }
        }
        for y in (0..ny).rev() {
            for x in (0..nx).rev() {
                let i = y * nx + x;
                let mut v = d[i];
                if x + 1 < nx {
                    v = v.min(d[i + 1] + orth);
                }
                if y + 1 < ny {
                    v = v.min(d[i + nx] + orth);
                    if x + 1 < nx {
                        v = v.min(d[i + nx + 1] + diag);
                    }
                    if x > 0 {
                        v = v.min(d[i + nx - 1] + diag);
                    }
                }
                d[i] = v;
            }
        }
        d
    };

    let d_to_wet = chamfer(&|i| mask[i]);
    let d_to_dry = chamfer(&|i| !mask[i]);
    // The boundary sits between cell centers: half-cell offset each side.
    let mut sdf = Grid::filled(spec, -1.0e9f64);
    for i in 0..n {
        sdf.data[i] = if mask[i] {
            (d_to_dry[i] - half).max(0.05)
        } else {
            -((d_to_wet[i] - half).max(0.05))
        };
    }

    // Smooth: two separable 1-2-1 binomial passes.
    let mut cur = sdf.data.clone();
    for _ in 0..2 {
        let mut tmp = cur.clone();
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                let a = cur[y * nx + x.saturating_sub(1)];
                let b = cur[y * nx + (x + 1).min(nx - 1)];
                tmp[i] = 0.25 * a + 0.5 * cur[i] + 0.25 * b;
            }
        }
        let mut out = tmp.clone();
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                let a = tmp[y.saturating_sub(1) * nx + x];
                let b = tmp[(y + 1).min(ny - 1) * nx + x];
                out[i] = 0.25 * a + 0.5 * tmp[i] + 0.25 * b;
            }
        }
        cur = out;
    }

    // Protection band: a cell that was decisively wet (or dry) must keep a
    // matching sign; only boundary-band cells take the smoothed value.
    for (i, c) in cur.iter_mut().enumerate() {
        let orig = sdf.data[i];
        let flipped_wet = orig >= half && *c < 0.05;
        let flipped_dry = orig <= -half && *c > -0.05;
        if flipped_wet || flipped_dry {
            *c = orig;
        }
    }
    Grid::from_data(spec, cur)
}

/// Fixed neighbor order (matches erosion.rs).
const NBR: [(i32, i32); 8] = [
    (0, -1),
    (-1, 0),
    (1, 0),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];
const SQRT_2: f64 = std::f64::consts::SQRT_2;

#[inline]
fn is_boundary(x: usize, y: usize, nx: usize, ny: usize) -> bool {
    x == 0 || y == 0 || x == nx - 1 || y == ny - 1
}

/// Derive the water layer and carve stream channels into `heights`.
/// `heights` must be the eroded macro grid the `outs` routing describes.
pub fn build(heights: &mut Grid<f64>, outs: &ErosionOutputs, p: &WaterParams) -> WaterLayer {
    let spec = heights.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    let a0 = spec.cell_size * spec.cell_size;
    let world_area = n as f64 * a0;
    let rt = &outs.routing;

    let mut layer = WaterLayer::empty(spec);
    // Surface defaults to terrain.
    layer.surface.data.copy_from_slice(&heights.data);

    // ---- 0. drainage area, plus any external through-river ----------------
    // An external upstream catchment enters at the world edge and rides the
    // internal drainage to its outlet — the only way a 2 km world gets a
    // river wider than a creek.
    let mut area = outs.flow_area.data.clone();
    if p.river_area > 0.0 {
        inject_external_river(&mut area, rt, nx, ny, p.river_area);
    }

    // ---- 1. channel network, two tiers -------------------------------------
    // Swale tier (fine network): HAND wetland reference + routing metadata,
    // never water. Perennial tier (10× the drainage): the visible streams —
    // only collected mains carry water, so thin headwater brooks don't
    // clutter the course.
    let swale_min = (p.stream_area_min / SWALE_AREA_DIVISOR).max(4.0 * a0);
    let mut swales = trace_links(&area, &rt.rcv, nx, ny, swale_min, p.runoff);
    for l in &mut swales {
        l.perennial = false;
        l.pts = cell_centers(&l.cells, spec, nx);
    }

    if p.coverage <= 0.0 {
        // Dry course: swales only.
        layer.streams = swales;
        layer.build_fine();
        return layer;
    }

    let stream_cap = if p.river_area > 0.0 { 0.08 } else { MAX_STREAM_WORLD_SHARE };
    let mut area_min = p.stream_area_min.max(4.0 * a0);
    let mut links;
    let mut stream_frac;
    let mut notches = 0;
    loop {
        links = trace_links(&area, &rt.rcv, nx, ny, area_min, p.runoff);
        // Targeted stub removal: perennial links shorter than a hole feature.
        links.retain(|l| cell_path_len(&l.cells, nx, spec.cell_size) >= MIN_LINK_LENGTH_M);
        // Wide-water rule (rolling/mountain): links whose natural mean width
        // falls well short of the floor are culled outright — narrow brooks
        // don't exist on those courses; the kept mains get floored to river
        // width in `finalize_geometry`.
        if p.min_stream_width > 0.0 {
            links.retain(|l| {
                let mean_w = l
                    .cells
                    .iter()
                    .map(|&c| width_for(area[c as usize], p.runoff))
                    .sum::<f64>()
                    / l.cells.len().max(1) as f64;
                mean_w >= 0.6 * p.min_stream_width
            });
        }
        // Cell-path estimate for budgeting (final geometry adds ~8% length).
        stream_frac = cell_stream_area(&links, &area, p.runoff, spec.cell_size, nx, p.min_stream_width)
            / world_area;
        // Keep channels within the stream budget: sparse main stems on dry
        // draws, denser networks on wet ones. External rivers keep their
        // trunk regardless (their area dwarfs any threshold notch).
        let budget = (p.coverage * 0.9).clamp(0.005, stream_cap);
        if stream_frac <= budget || notches >= 4 {
            break;
        }
        area_min *= 2.0;
        notches += 1;
    }
    let stream_frac = stream_frac.min(stream_cap);

    // ---- 2. ponds/lakes from depression basins ---------------------------
    let body_budget =
        ((p.coverage - stream_frac).max(0.0) * p.pond_share * world_area / a0).round() as usize;
    let max_body_cells = ((if p.lake_course {
        MAX_BODY_WORLD_SHARE_LAKE_COURSE
    } else {
        MAX_BODY_WORLD_SHARE
    }) * n as f64) as usize;
    let min_body_cells = ((p.min_body_area / a0).ceil() as usize).max(MIN_POND_CELLS);
    let mut bodies = select_bodies(
        heights,
        &outs.lake_depth.data,
        nx,
        ny,
        a0,
        body_budget,
        max_body_cells,
        p.lake_course,
        min_body_cells,
    );

    // ---- 2a. organic outlines + sane proportions --------------------------
    // Cull skinny string-of-cells bodies, then round each survivor's raw
    // basin boundary (SDF smoothing; the terrain is dug — bounded — where
    // the rounded outline adds cells, so the water stays justified).
    regularize_bodies(&mut bodies, heights, nx, ny, a0, min_body_cells);

    // Cell → body surface lookup for stream-surface capping.
    let mut in_body = vec![u16::MAX; n];
    for (bi, b) in bodies.iter().enumerate() {
        for &c in &b.cells {
            in_body[c as usize] = bi as u16;
        }
    }

    // ---- 2b. anchor stream endpoints -------------------------------------
    // Every visible stream must begin AND end at the map edge, a pond/lake,
    // or a junction with another stream — channels may not appear out of or
    // vanish into plain ground. Violating heads snap to a nearby edge/body
    // via a prepended feeder channel or the link is removed; violating
    // tails extend down the flow tree until they anchor, or are removed.
    anchor_endpoints(&mut links, &in_body, rt, nx, ny);

    // ---- 3. stream water surfaces (monotone, junction-consistent) --------
    let channel_wet = mark_channel_cells(&links, n);
    let mut surf = vec![f64::INFINITY; n];
    for i in 0..n {
        if channel_wet[i] {
            surf[i] = if in_body[i] != u16::MAX {
                bodies[in_body[i] as usize].surface
            } else {
                heights.data[i]
            };
        }
    }
    // Running min along every downstream path (donors before receivers).
    for oi in (0..rt.order.len()).rev() {
        let i = rt.order[oi] as usize;
        let r = rt.rcv[i] as usize;
        if r != i && channel_wet[i] && channel_wet[r] && surf[i] < surf[r] {
            surf[r] = surf[i];
        }
    }

    // ---- 3b. link geometry: resample, smooth, meander ----------------------
    // Slope of the (pre-carve) terrain attenuates meander on steep ground.
    let slope = crate::derive_slope(heights);
    for l in &mut links {
        finalize_geometry(l, &area, &surf, &slope, p, spec, nx);
    }

    // ---- 3c. lone small streams: pool into thin lakes, or remove ----------
    // A small stream that meets no other stream and no pond/lake reads as an
    // accident: widen it 4–6× into a deliberate thin-lake feature, or drop
    // it when it's too short/steep to hold a pool.
    {
        let mut link_count = vec![0u8; n];
        for l in &links {
            for &c in &l.cells {
                link_count[c as usize] = link_count[c as usize].saturating_add(1);
            }
        }
        // Endpoint cells of every link: a link that some OTHER link's head or
        // tail anchors onto (8-adjacency) must not be removed, or that link's
        // endpoint would dangle in plain ground.
        let endpoints: Vec<(usize, u32, u32)> = links
            .iter()
            .enumerate()
            .map(|(k, l)| (k, l.cells[0], *l.cells.last().unwrap()))
            .collect();
        let mut keep = Vec::with_capacity(links.len());
        for (k, mut l) in links.drain(..).enumerate() {
            let mean_w = l.width.iter().sum::<f64>() / l.width.len().max(1) as f64;
            let touches_other = l.cells.iter().any(|&c| link_count[c as usize] >= 2);
            let touches_body = l.cells.iter().any(|&c| {
                let i = c as usize;
                if in_body[i] != u16::MAX {
                    return true;
                }
                let (x, y) = (i % nx, i / nx);
                NBR.iter().any(|&(dx, dy)| {
                    let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                    cx >= 0
                        && cy >= 0
                        && (cx as usize) < nx
                        && (cy as usize) < ny
                        && in_body[cy as usize * nx + cx as usize] != u16::MAX
                })
            });
            let len = cell_path_len(&l.cells, nx, spec.cell_size);
            let ratio_max = LONE_RATIO_MIN
                + (LONE_RATIO_MAX - LONE_RATIO_MIN) * hash01(l.cells[0] ^ p.noise_seed);
            if !touches_other
                && !touches_body
                && mean_w < LONE_SMALL_WIDTH
                && len / mean_w.max(1e-9) > ratio_max
            {
                let s0 = l.surface.first().copied().unwrap_or(0.0);
                let s1 = l.surface.last().copied().unwrap_or(0.0);
                // Width needed for pond proportions (length : width ≤ ratio).
                let target_w = len / ratio_max;
                if s0 - s1 > LONE_MAX_DROP || target_w > POOL_MAX_WIDTH {
                    // Removal orphan guard: keep the link as-is if another
                    // link's endpoint anchors onto it.
                    let anchors_someone = endpoints.iter().any(|&(ok, h, t)| {
                        ok != k
                            && l.cells.iter().any(|&c| {
                                let (x, y) = ((c % nx as u32) as i32, (c / nx as u32) as i32);
                                for e in [h, t] {
                                    let (ex, ey) =
                                        ((e % nx as u32) as i32, (e / nx as u32) as i32);
                                    if (ex - x).abs() <= 1 && (ey - y).abs() <= 1 {
                                        return true;
                                    }
                                }
                                false
                            })
                    });
                    if !anchors_someone {
                        continue; // removed: can't hold a pool of sane proportions
                    }
                    keep.push(l);
                    continue;
                }
                // Pool: uniform pond width with tapered ends, surface
                // flattened at a level most of the channel can hold.
                let npts = l.width.len();
                for (k, w) in l.width.iter_mut().enumerate() {
                    let edge = k.min(npts - 1 - k) as f64 / npts.max(1) as f64;
                    let taper = 0.35 + 0.65 * (6.0 * edge).min(1.0);
                    *w = target_w * taper;
                }
                let mut sorted = l.surface.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
                let pool = sorted[(sorted.len() - 1) * 35 / 100];
                for s in &mut l.surface {
                    *s = pool;
                }
                l.pooled = true;
            }
            keep.push(l);
        }
        links = keep;
    }

    // Final stream share from the meandered (and pooled) geometry.
    let stream_frac = (analytic_stream_area(&links) / world_area).min(stream_cap);

    // ---- 4. wetlands via HAND ---------------------------------------------
    // The HAND reference is the SWALE network (where water would drain, wet
    // or not), so thinning the visible streams doesn't shrink the water
    // table's reach. Reference height: stream surface where perennial,
    // terrain height along dry swales.
    let channel_ref = mark_all_channel_cells(&swales, n);
    let mut surf_ref = vec![0.0f64; n];
    for i in 0..n {
        if channel_ref[i] {
            surf_ref[i] = if channel_wet[i] && surf[i].is_finite() {
                surf[i]
            } else {
                heights.data[i]
            };
        }
    }
    let wet_budget_cells = {
        let raw = ((p.coverage - stream_frac).max(0.0) * (1.0 - p.pond_share) * world_area
            / a0)
            .round() as usize;
        let cap = (p.coverage * p.wetland_share_cap * world_area / a0) as usize;
        raw.min(cap)
    };
    let mut wet_cells = select_wetlands(
        heights,
        &slope,
        rt,
        &channel_wet,
        &channel_ref,
        &surf_ref,
        &in_body,
        wet_budget_cells,
        p.noise_seed,
        spec,
    );

    // ---- 5. dry-connectivity guard (monotone removal) ---------------------
    // "Blocking" water only: ponds/lakes/wetlands and river-class channels.
    // Brooks and creeks (< 8 m) are carry hazards a routing crosses freely.
    let mut is_water = vec![false; n];
    for b in &bodies {
        for &c in &b.cells {
            is_water[c as usize] = true;
        }
    }
    for &c in &wet_cells {
        is_water[c as usize] = true;
    }
    for i in 0..n {
        if channel_wet[i] && width_for(area[i], p.runoff) >= BLOCKING_STREAM_WIDTH {
            is_water[i] = true;
        }
    }
    // Pooled lone streams are pond-like: they block regardless of their
    // original hydraulic width.
    for l in &links {
        if l.pooled {
            for &c in &l.cells {
                is_water[c as usize] = true;
            }
        }
    }
    let dry_min = if p.river_area > 0.0 {
        DRY_CONNECTIVITY_MIN_RIVER
    } else if p.coverage >= 0.12 {
        // Lake-field courses legitimately fragment the ground — holes route
        // between the lakes; the routing phase verifies actual playability.
        DRY_CONNECTIVITY_MIN_LAKE_FIELD
    } else {
        DRY_CONNECTIVITY_MIN
    };
    for _ in 0..8 {
        if largest_dry_component(&is_water, nx, ny) >= dry_min * n as f64 {
            break;
        }
        if !wet_cells.is_empty() {
            // Drop the top decile of wetland cells (they were selected in
            // ascending HAND order, so the tail is the marginal ground).
            let keep = wet_cells.len() * 9 / 10;
            for &c in &wet_cells[keep..] {
                is_water[c as usize] = false;
            }
            wet_cells.truncate(keep);
        } else if let Some(smallest) = bodies
            .iter()
            .enumerate()
            .filter(|(_, b)| b.class != BodyClass::Wetland)
            .min_by_key(|(bi, b)| (b.cells.len(), *bi))
            .map(|(bi, _)| bi)
        {
            for &c in &bodies[smallest].cells {
                is_water[c as usize] = false;
            }
            bodies.remove(smallest);
            in_body.iter_mut().for_each(|v| *v = u16::MAX);
            for (bi, b) in bodies.iter().enumerate() {
                for &c in &b.cells {
                    in_body[c as usize] = bi as u16;
                }
            }
        } else {
            break;
        }
    }

    // ---- 6. carve channels into the terrain -------------------------------
    let corridor = corridor_field(&links, spec);
    carve_channels(heights, &corridor, &in_body, spec.cell_size);

    // ---- 7. rebuild grids from entities -----------------------------------
    layer.streams = links;
    layer.streams.extend(swales); // metadata tier, after the visible streams
    layer.bodies = bodies;
    // Wetland patches become bodies too (grouped), after culling small ones.
    let wetland_bodies = wetland_components(&wet_cells, heights, nx, ny, a0);
    layer.bodies.extend(wetland_bodies);
    rebuild_grids(&mut layer, heights, &corridor, spec);
    layer.coverage = stream_frac
        + layer
            .bodies
            .iter()
            .map(|b| b.cells.len())
            .sum::<usize>() as f64
            / n as f64;
    // Display-resolution class raster (2.5 m) — after all entity edits and
    // grid rebuilds, so it reflects the final layer.
    layer.build_fine();
    layer
}

/// All grid cells on the integer segment from `(x0,y0)` to `(x1,y1)`
/// inclusive (Bresenham; deterministic, no floats).
fn cell_line(x0: i32, y0: i32, x1: i32, y1: i32, nx: usize) -> Vec<u32> {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    let mut out = Vec::new();
    loop {
        out.push((y as usize * nx + x as usize) as u32);
        if x == x1 && y == y1 {
            return out;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// PCA proportions test over a body's cells: deliberate lakes run 1:1 up to
/// ~1:8 length:width. Rejects elongated-AND-narrow snakes (accidents) and
/// anything beyond the hard 1:8 spec.
fn proportions_ok(cells: &[u32], nx: usize, cell: f64) -> bool {
    let m = cells.len() as f64;
    let (mut mx, mut my) = (0.0f64, 0.0f64);
    for &c in cells {
        mx += (c as usize % nx) as f64;
        my += (c as usize / nx) as f64;
    }
    mx /= m;
    my /= m;
    let (mut cxx, mut cyy, mut cxy) = (0.0f64, 0.0f64, 0.0f64);
    for &c in cells {
        let dx = (c as usize % nx) as f64 - mx;
        let dy = (c as usize / nx) as f64 - my;
        cxx += dx * dx;
        cyy += dy * dy;
        cxy += dx * dy;
    }
    // Each cell is a unit square, not a point: + 1/12 per axis.
    cxx = cxx / m + 1.0 / 12.0;
    cyy = cyy / m + 1.0 / 12.0;
    cxy /= m;
    let half_tr = 0.5 * (cxx + cyy);
    let disc = (half_tr * half_tr - (cxx * cyy - cxy * cxy)).max(0.0).sqrt();
    let (l1, l2) = (half_tr + disc, (half_tr - disc).max(1e-9));
    let aspect = (l1 / l2).sqrt();
    // Uniform strip of width w has variance w²/12 along its minor axis.
    let width_m = (12.0 * l2).sqrt() * cell;
    let len_m = (12.0 * l1).sqrt() * cell;
    (aspect <= ASPECT_MAX || width_m >= ASPECT_WIDTH_MIN)
        && aspect <= ASPECT_HARD_MAX
        && len_m >= BODY_LEN_MIN
}

/// Round each standing body's raw basin boundary into an organic outline,
/// split multi-pocket bodies, and cull disproportionate ones.
///
/// - **Proportion cull** (`proportions_ok`): PCA aspect/width rules.
/// - **Outline smoothing**: per-body signed chamfer distance → binomial
///   smoothing → re-threshold, with the outline allowed to move at most
///   `REG_BAND_CELLS` from the raw boundary. Cells the rounding *adds* are
///   dug to `surface − REG_DIG_DEPTH` (cut bounded by `REG_DIG_MAX`; cells
///   needing more stay dry).
/// - **Component split**: a body whose (possibly stage-truncated) cells form
///   several disconnected pockets becomes several bodies — a mega-basin
///   capped at the per-body share yields a natural multi-lake field instead
///   of losing everything but its largest pocket. Components under
///   `min_cells` or failing proportions are dropped.
fn regularize_bodies(
    bodies: &mut Vec<WaterBody>,
    heights: &mut Grid<f64>,
    nx: usize,
    ny: usize,
    a0: f64,
    min_cells: usize,
) {
    let n = nx * ny;
    let cell = a0.sqrt(); // square macro cells
    // --- proportion cull -----------------------------------------------
    bodies.retain(|b| b.class == BodyClass::Wetland || proportions_ok(&b.cells, nx, cell));

    // --- outline smoothing (bounded dig) ---------------------------------
    // `taken` guards against neighboring bodies growing into each other.
    let mut taken = vec![false; n];
    for b in bodies.iter() {
        for &c in &b.cells {
            taken[c as usize] = true;
        }
    }
    let old = std::mem::take(bodies);
    for b in old {
        if b.class == BodyClass::Wetland {
            bodies.push(b);
            continue;
        }
        for &c in &b.cells {
            taken[c as usize] = false;
        }
        // Local window: bbox + REG pad.
        let pad = 4usize;
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        for &c in &b.cells {
            let (x, y) = (c as usize % nx, c as usize / nx);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
        let (x0, y0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
        let (x1, y1) = ((x1 + pad).min(nx - 1), (y1 + pad).min(ny - 1));
        let (wx, wy) = (x1 - x0 + 1, y1 - y0 + 1);
        let wn = wx * wy;
        let mut mask = vec![false; wn];
        for &c in &b.cells {
            let (x, y) = (c as usize % nx, c as usize / nx);
            mask[(y - y0) * wx + (x - x0)] = true;
        }
        // Signed 3-4 chamfer distance in cell units (positive = wet).
        let chamfer = |seed: &dyn Fn(usize) -> bool| -> Vec<f64> {
            let mut d = vec![1.0e9f64; wn];
            for (i, di) in d.iter_mut().enumerate() {
                if seed(i) {
                    *di = 0.0;
                }
            }
            for y in 0..wy {
                for x in 0..wx {
                    let i = y * wx + x;
                    let mut v = d[i];
                    if x > 0 {
                        v = v.min(d[i - 1] + 1.0);
                    }
                    if y > 0 {
                        v = v.min(d[i - wx] + 1.0);
                        if x > 0 {
                            v = v.min(d[i - wx - 1] + SQRT_2);
                        }
                        if x + 1 < wx {
                            v = v.min(d[i - wx + 1] + SQRT_2);
                        }
                    }
                    d[i] = v;
                }
            }
            for y in (0..wy).rev() {
                for x in (0..wx).rev() {
                    let i = y * wx + x;
                    let mut v = d[i];
                    if x + 1 < wx {
                        v = v.min(d[i + 1] + 1.0);
                    }
                    if y + 1 < wy {
                        v = v.min(d[i + wx] + 1.0);
                        if x + 1 < wx {
                            v = v.min(d[i + wx + 1] + SQRT_2);
                        }
                        if x > 0 {
                            v = v.min(d[i + wx - 1] + SQRT_2);
                        }
                    }
                    d[i] = v;
                }
            }
            d
        };
        let d_wet = chamfer(&|i| mask[i]);
        let d_dry = chamfer(&|i| !mask[i]);
        let sdf0: Vec<f64> = (0..wn)
            .map(|i| if mask[i] { d_dry[i] - 0.5 } else { -(d_wet[i] - 0.5) })
            .collect();
        // Three separable 1-2-1 binomial passes, then clamp to the band so
        // decisively wet/dry cells keep their sign.
        let mut cur = sdf0.clone();
        for _ in 0..3 {
            let mut tmp = cur.clone();
            for y in 0..wy {
                for x in 0..wx {
                    let a = cur[y * wx + x.saturating_sub(1)];
                    let bb = cur[y * wx + (x + 1).min(wx - 1)];
                    tmp[y * wx + x] = 0.25 * a + 0.5 * cur[y * wx + x] + 0.25 * bb;
                }
            }
            for y in 0..wy {
                for x in 0..wx {
                    let a = tmp[y.saturating_sub(1) * wx + x];
                    let bb = tmp[(y + 1).min(wy - 1) * wx + x];
                    cur[y * wx + x] = 0.25 * a + 0.5 * tmp[y * wx + x] + 0.25 * bb;
                }
            }
        }
        for i in 0..wn {
            cur[i] = cur[i].clamp(sdf0[i] - REG_BAND_CELLS, sdf0[i] + REG_BAND_CELLS);
        }
        // Re-threshold; added cells must be diggable and unclaimed.
        let mut newmask = vec![false; wn];
        for wyi in 0..wy {
            for wxi in 0..wx {
                let i = wyi * wx + wxi;
                if cur[i] < 0.0 {
                    continue;
                }
                let g = (y0 + wyi) * nx + (x0 + wxi);
                if taken[g] {
                    continue;
                }
                if !mask[i] {
                    let cut = heights.data[g] - (b.surface - REG_DIG_DEPTH);
                    if cut > REG_DIG_MAX {
                        continue;
                    }
                }
                newmask[i] = true;
            }
        }
        // Largest 8-connected component (scan order → deterministic).
        let mut comp = vec![u32::MAX; wn];
        let mut sizes: Vec<usize> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        for s in 0..wn {
            if !newmask[s] || comp[s] != u32::MAX {
                continue;
            }
            let id = sizes.len() as u32;
            comp[s] = id;
            stack.push(s);
            let mut count = 0usize;
            while let Some(i) = stack.pop() {
                count += 1;
                let (x, y) = (i % wx, i / wx);
                for (dx, dy) in NBR {
                    let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                    if cx < 0 || cy < 0 || cx >= wx as i32 || cy >= wy as i32 {
                        continue;
                    }
                    let m = cy as usize * wx + cx as usize;
                    if newmask[m] && comp[m] == u32::MAX {
                        comp[m] = id;
                        stack.push(m);
                    }
                }
            }
            sizes.push(count);
        }
        // Every pocket ≥ min_cells with sane proportions becomes its own
        // body (a stage-truncated mega-basin splits into a lake field).
        let mut comp_cells: Vec<Vec<u32>> = vec![Vec::new(); sizes.len()];
        for wyi in 0..wy {
            for wxi in 0..wx {
                let id = comp[wyi * wx + wxi];
                if id != u32::MAX {
                    comp_cells[id as usize].push(((y0 + wyi) * nx + (x0 + wxi)) as u32);
                }
            }
        }
        for cells in comp_cells {
            if cells.len() < min_cells || !proportions_ok(&cells, nx, cell) {
                continue; // dropped pocket (terrain untouched)
            }
            // Dig: every kept cell sits ≥ REG_DIG_DEPTH under the surface.
            let bed = b.surface - REG_DIG_DEPTH;
            let mut max_depth = 0.0f64;
            for &c in &cells {
                if heights.data[c as usize] > bed {
                    heights.data[c as usize] = bed;
                }
                max_depth = max_depth.max(b.surface - heights.data[c as usize]);
                taken[c as usize] = true;
            }
            let class = if cells.len() >= LAKE_MIN_CELLS {
                BodyClass::Lake
            } else {
                BodyClass::Pond
            };
            bodies.push(WaterBody {
                class,
                area_m2: cells.len() as f64 * a0,
                cells,
                surface: b.surface,
                spill: b.spill.max(b.surface),
                max_depth,
            });
        }
    }
}

/// Anchor every stream's endpoints: each link must start and end at the map
/// edge, at standing water, or at a junction with another link. Heads that
/// begin in plain ground snap to the nearest edge/body within
/// `HEAD_SNAP_CELLS` via a prepended straight feeder channel — otherwise the
/// whole link is **removed** (no source ponds). Tails extend down the flow
/// tree until they anchor, or the link is removed. Removals can orphan
/// endpoints that anchored on the removed channel, so both passes iterate
/// to a fixpoint.
fn anchor_endpoints(
    links: &mut Vec<StreamLink>,
    in_body: &[u16],
    rt: &crate::erosion::RoutingSnapshot,
    nx: usize,
    ny: usize,
) {
    let n = nx * ny;
    let near_edge = |c: usize| -> bool {
        let (x, y) = (c % nx, c / nx);
        x <= 1 || y <= 1 || x >= nx - 2 || y >= ny - 2
    };
    // Anchored at the cell itself or via 8-neighbors: edge / body / other link.
    let anchored = |c: usize, li: usize, occ: &[u16]| -> bool {
        if near_edge(c) || in_body[c] != u16::MAX {
            return true;
        }
        let (x, y) = (c % nx, c / nx);
        for (dx, dy) in NBR {
            let (cx, cy) = (x as i32 + dx, y as i32 + dy);
            if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                continue;
            }
            let m = cy as usize * nx + cx as usize;
            if in_body[m] != u16::MAX || (occ[m] != u16::MAX && occ[m] != li as u16) {
                return true;
            }
        }
        false
    };
    let build_occ = |links: &[StreamLink]| -> Vec<u16> {
        let mut occ = vec![u16::MAX; n];
        for (li, l) in links.iter().enumerate() {
            for &c in &l.cells {
                if occ[c as usize] == u16::MAX {
                    occ[c as usize] = li as u16;
                }
            }
        }
        occ
    };

    loop {
        let mut removed = false;

        // --- Heads: snap to a nearby edge/body or remove. -----------------
        let occ = build_occ(links);
        let mut kept: Vec<StreamLink> = Vec::with_capacity(links.len());
        for (li, mut l) in links.drain(..).enumerate() {
            if l.cells.is_empty() {
                removed = true;
                continue;
            }
            let head = l.cells[0] as usize;
            if !anchored(head, li, &occ) {
                // Nearest edge/body cell (squared distance, tie → lowest
                // cell index) within the snap window.
                let (hx, hy) = ((head % nx) as i32, (head / nx) as i32);
                let mut best: Option<(i32, usize)> = None;
                for dy in -HEAD_SNAP_CELLS..=HEAD_SNAP_CELLS {
                    for dx in -HEAD_SNAP_CELLS..=HEAD_SNAP_CELLS {
                        let (cx, cy) = (hx + dx, hy + dy);
                        if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                            continue;
                        }
                        let c = cy as usize * nx + cx as usize;
                        if !(near_edge(c) || in_body[c] != u16::MAX) {
                            continue;
                        }
                        let d2 = dx * dx + dy * dy;
                        if best.is_none_or(|(bd, bc)| (d2, c) < (bd, bc)) {
                            best = Some((d2, c));
                        }
                    }
                }
                let Some((_, target)) = best else {
                    removed = true;
                    continue; // no anchor in reach — remove the link
                };
                let (tx, ty) = ((target % nx) as i32, (target / nx) as i32);
                let mut line = cell_line(tx, ty, hx, hy, nx);
                line.pop(); // head itself is already the link's first cell
                if !line.is_empty() {
                    line.extend_from_slice(&l.cells);
                    l.cells = line;
                }
            }
            kept.push(l);
        }
        *links = kept;

        // --- Tails: extend along the flow tree or remove. -----------------
        let mut occ = build_occ(links);
        let mut kept: Vec<StreamLink> = Vec::with_capacity(links.len());
        for (li, mut l) in links.drain(..).enumerate() {
            let mut tail = *l.cells.last().unwrap() as usize;
            let mut appended: Vec<u32> = Vec::new();
            let mut guard = 0;
            while !anchored(tail, li, &occ) && guard < 400 {
                let r = rt.rcv[tail] as usize;
                if r == tail {
                    break; // boundary cell (drains off-world)
                }
                appended.push(r as u32);
                if occ[r] == u16::MAX {
                    occ[r] = li as u16;
                } else {
                    tail = r;
                    break; // reached another link's channel: a junction
                }
                tail = r;
                guard += 1;
            }
            if !anchored(tail, li, &occ) && !near_edge(tail) {
                removed = true;
                continue; // dangling tail with no anchor in reach — remove
            }
            if !appended.is_empty() {
                l.cells.extend(appended);
            }
            kept.push(l);
        }
        *links = kept;

        if !removed {
            break;
        }
    }
}

// ---------------------------------------------------------------------------
// Streams
// ---------------------------------------------------------------------------

fn width_for(area: f64, runoff: f64) -> f64 {
    (K_WIDTH * (runoff * area).sqrt()).clamp(WIDTH_MIN, WIDTH_MAX)
}

fn class_for(width: f64) -> StreamClass {
    if width < 4.0 {
        StreamClass::Brook
    } else if width < 8.0 {
        StreamClass::Creek
    } else {
        StreamClass::River
    }
}

/// Downstream path length per cell, then inject an external catchment at the
/// interior-edge cell with the longest run to its outlet — the through-river
/// entry. Deterministic (max path length, ties to the lowest index).
fn inject_external_river(
    area: &mut [f64],
    rt: &crate::erosion::RoutingSnapshot,
    nx: usize,
    ny: usize,
    river_area: f64,
) {
    let n = nx * ny;
    let mut plen = vec![0u32; n];
    for &io in &rt.order {
        let i = io as usize;
        let r = rt.rcv[i] as usize;
        if r != i {
            plen[i] = plen[r] + 1;
        }
    }
    let mut best: Option<(u32, usize)> = None;
    #[allow(clippy::needless_range_loop)]
    for i in 0..n {
        let (x, y) = (i % nx, i / nx);
        if is_boundary(x, y, nx, ny) {
            continue;
        }
        // Interior cell touching the boundary ring.
        if x > 1 && x < nx - 2 && y > 1 && y < ny - 2 {
            continue;
        }
        match best {
            Some((bp, bi)) if plen[i] < bp || (plen[i] == bp && i >= bi) => {}
            _ => best = Some((plen[i], i)),
        }
    }
    if let Some((_, entry)) = best {
        let mut i = entry;
        loop {
            area[i] += river_area;
            let r = rt.rcv[i] as usize;
            if r == i {
                break;
            }
            i = r;
        }
    }
}

/// Trace maximal channel chains on the D8 tree. Deterministic: heads and
/// junction continuations claimed in ascending cell-index order.
fn trace_links(
    area: &[f64],
    rcv: &[u32],
    nx: usize,
    ny: usize,
    area_min: f64,
    runoff: f64,
) -> Vec<StreamLink> {
    let n = nx * ny;
    let channel: Vec<bool> = (0..n).map(|i| area[i] >= area_min).collect();
    // Donor counts within the channel set.
    let mut ndon = vec![0u8; n];
    for i in 0..n {
        let r = rcv[i] as usize;
        if r != i && channel[i] && channel[r] {
            ndon[r] = ndon[r].saturating_add(1);
        }
    }
    let mut claimed = vec![false; n];
    let mut links = Vec::new();
    // Starts: heads (no channel donors), then junction continuations.
    for pass in 0..2 {
        for start in 0..n {
            if !channel[start] || claimed[start] {
                continue;
            }
            let is_head = ndon[start] == 0;
            if (pass == 0 && !is_head) || (pass == 1 && is_head) {
                continue;
            }
            let mut cells = Vec::new();
            let mut i = start;
            loop {
                if claimed[i] {
                    break;
                }
                claimed[i] = true;
                cells.push(i as u32);
                let r = rcv[i] as usize;
                if r == i || !channel[r] {
                    break;
                }
                // Stop at (before claiming) a junction that is not our start.
                if ndon[r] >= 2 {
                    cells.push(r as u32); // shared junction node for geometry
                    break;
                }
                i = r;
            }
            if cells.len() < 2 {
                continue;
            }
            let mean_w = cells
                .iter()
                .map(|&c| width_for(area[c as usize], runoff))
                .sum::<f64>()
                / cells.len() as f64;
            links.push(StreamLink {
                pts: Vec::new(),    // finalize_geometry fills these
                width: Vec::new(),  //
                surface: Vec::new(),
                class: class_for(mean_w),
                perennial: true,
                pooled: false,
                cells,
            });
        }
    }
    links
}

/// Raw cell-center polyline (dry swales keep this simple geometry).
fn cell_centers(cells: &[u32], spec: golf_core::GridSpec, nx: usize) -> Vec<Vec2> {
    cells
        .iter()
        .map(|&c| {
            let (x, y) = (c as usize % nx, c as usize / nx);
            spec.world_of(x as u32, y as u32)
        })
        .collect()
}

/// Budgeting estimate before geometry exists: cell-path length × width
/// (floored to `min_w` — the wide-water rule widens what it keeps).
fn cell_stream_area(
    links: &[StreamLink],
    area: &[f64],
    runoff: f64,
    cell: f64,
    nx: usize,
    min_w: f64,
) -> f64 {
    let diag = cell * SQRT_2;
    let mut a = 0.0;
    for l in links {
        for k in 1..l.cells.len() {
            let (c0, c1) = (l.cells[k - 1] as usize, l.cells[k] as usize);
            let dx = (c0 % nx) as i64 - (c1 % nx) as i64;
            let dy = (c0 / nx) as i64 - (c1 / nx) as i64;
            let len = if dx != 0 && dy != 0 { diag } else { cell };
            let w = (0.5 * (width_for(area[c0], runoff) + width_for(area[c1], runoff))).max(min_w);
            a += len * w;
        }
    }
    a
}

/// Build the final centerline: resample the D8 cell path at ~10 m spacing,
/// smooth, then meander with width-scaled amplitude and wavelength
/// (λ ≈ 14·w — the real hydrological ratio), attenuated on steep ground so
/// channels stay in their valleys. Widths and (monotone) surfaces are
/// interpolated onto the resampled points. This is what turns D8's
/// ruler-straight quantized runs into organic channels — and the carve
/// follows the same geometry, so terrain and water agree.
fn finalize_geometry(
    l: &mut StreamLink,
    area: &[f64],
    surf: &[f64],
    slope: &Grid<f64>,
    p: &WaterParams,
    spec: golf_core::GridSpec,
    nx: usize,
) {
    let raw = cell_centers(&l.cells, spec, nx);
    // Wide-water floor: rolling/mountain streams read as proper rivers.
    let raw_w: Vec<f64> = l
        .cells
        .iter()
        .map(|&c| width_for(area[c as usize], p.runoff).max(p.min_stream_width))
        .collect();
    let raw_s: Vec<f64> = l.cells.iter().map(|&c| surf[c as usize]).collect();

    // Cumulative arc length over the raw path.
    let mut cum = vec![0.0f64; raw.len()];
    for k in 1..raw.len() {
        cum[k] = cum[k - 1] + raw[k].distance(raw[k - 1]);
    }
    let total = *cum.last().unwrap();
    let n_pts = ((total / RESAMPLE_M).ceil() as usize).max(1) + 1;

    let mut pts = Vec::with_capacity(n_pts);
    let mut width = Vec::with_capacity(n_pts);
    let mut surface = Vec::with_capacity(n_pts);
    let mut j = 0usize;
    for k in 0..n_pts {
        let s = total * k as f64 / (n_pts - 1).max(1) as f64;
        while j + 1 < cum.len() - 1 && cum[j + 1] < s {
            j += 1;
        }
        let seg = (cum[j + 1] - cum[j]).max(1e-9);
        let t = ((s - cum[j]) / seg).clamp(0.0, 1.0);
        pts.push(raw[j].lerp(raw[j + 1], t));
        width.push(raw_w[j] + (raw_w[j + 1] - raw_w[j]) * t);
        surface.push(raw_s[j] + (raw_s[j + 1] - raw_s[j]) * t);
    }

    // Smooth (endpoints pinned so junctions stay connected).
    for _ in 0..SMOOTH_PASSES {
        if pts.len() < 3 {
            break;
        }
        let base = pts.clone();
        for k in 1..base.len() - 1 {
            pts[k] = Vec2::new(
                0.25 * base[k - 1].x + 0.5 * base[k].x + 0.25 * base[k + 1].x,
                0.25 * base[k - 1].y + 0.5 * base[k].y + 0.25 * base[k + 1].y,
            );
        }
    }

    // Meander: two noise octaves along arc length; amplitude grows with
    // width, gets an extra boost on near-flat ground (deposition floors are
    // where D8 runs dead straight), and is attenuated on steep ground so
    // channels stay in their valleys.
    if pts.len() >= 3 {
        let base = pts.clone();
        let phase = (l.cells[0] % 977) as f64 * 0.173;
        let mut arc = 0.0;
        for k in 1..base.len() - 1 {
            arc += base[k].distance(base[k - 1]);
            let w = width[k];
            let s = slope.bilinear(base[k]);
            let atten = 1.0 - golf_core::math::smoothstep(0.03, 0.09, s);
            if atten <= 0.0 {
                continue;
            }
            let flat_boost =
                1.0 + (MEANDER_FLAT_BOOST - 1.0) * (1.0 - golf_core::math::smoothstep(0.005, 0.02, s));
            let lam = (MEANDER_WAVELENGTH_PER_W * w).max(MEANDER_WAVELENGTH_MIN);
            let amp = ((MEANDER_AMP_PER_W * w + MEANDER_AMP_BASE) * atten * flat_boost)
                .min(MEANDER_AMP_LAMBDA_CAP * lam);
            let seed = p.noise_seed ^ 0x5eed_11ff;
            let m = noise::perlin2(arc / lam, phase, seed)
                + 0.45 * noise::perlin2(arc / (lam / 2.7) + 13.7, phase + 7.7, seed);
            let tangent = (base[k + 1] - base[k - 1]).normalized();
            pts[k] = base[k] + tangent.perp() * (amp * m);
        }
    }

    // Widths: light EMA so junction steps read as gradual growth.
    for k in 1..width.len() {
        width[k] = 0.6 * width[k] + 0.4 * width[k - 1];
    }
    // Surfaces: re-enforce monotone non-increasing downstream.
    for k in 1..surface.len() {
        if surface[k] > surface[k - 1] {
            surface[k] = surface[k - 1];
        }
    }

    l.pts = pts;
    l.width = width;
    l.surface = surface;
}

/// Σ segment length × mean width (the raster would 4× overstate a brook).
fn analytic_stream_area(links: &[StreamLink]) -> f64 {
    let mut a = 0.0;
    for l in links {
        for k in 1..l.pts.len() {
            let len = l.pts[k].distance(l.pts[k - 1]);
            a += len * 0.5 * (l.width[k] + l.width[k - 1]);
        }
    }
    a
}

fn mark_channel_cells(links: &[StreamLink], n: usize) -> Vec<bool> {
    let mut m = vec![false; n];
    for l in links {
        if l.perennial {
            for &c in &l.cells {
                m[c as usize] = true;
            }
        }
    }
    m
}

/// Every traced channel cell regardless of perennial status (the HAND
/// drainage reference).
fn mark_all_channel_cells(links: &[StreamLink], n: usize) -> Vec<bool> {
    let mut m = vec![false; n];
    for l in links {
        for &c in &l.cells {
            m[c as usize] = true;
        }
    }
    m
}

/// Deterministic u32 → [0, 1) hash (per-link pool-factor draw).
fn hash01(x: u32) -> f64 {
    let mut h = x ^ 0x9E37_79B9;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7FEB_352D);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846C_A68B);
    h ^= h >> 16;
    h as f64 / (u32::MAX as f64 + 1.0)
}

/// Exact D8 cell-path length in meters.
fn cell_path_len(cells: &[u32], nx: usize, cell: f64) -> f64 {
    let diag = cell * SQRT_2;
    let mut len = 0.0;
    for k in 1..cells.len() {
        let (c0, c1) = (cells[k - 1] as usize, cells[k] as usize);
        let dx = (c0 % nx) as i64 - (c1 % nx) as i64;
        let dy = (c0 / nx) as i64 - (c1 / nx) as i64;
        len += if dx != 0 && dy != 0 { diag } else { cell };
    }
    len
}

// ---------------------------------------------------------------------------
// Ponds / lakes
// ---------------------------------------------------------------------------

struct Basin {
    cells_sorted: Vec<u32>, // ascending z
    spill: f64,
    max_depth: f64,
}

/// Depression basins → greedy selection into the cell budget.
#[allow(clippy::too_many_arguments)]
fn select_bodies(
    heights: &Grid<f64>,
    lake_depth: &[f64],
    nx: usize,
    ny: usize,
    a0: f64,
    budget_cells: usize,
    max_body_cells: usize,
    lake_course: bool,
    min_cells: usize,
) -> Vec<WaterBody> {
    let n = nx * ny;
    let z = &heights.data;
    // Connected components of lake_depth > 1e-3 (excludes flood-ε ladders).
    let deep: Vec<bool> = (0..n).map(|i| lake_depth[i] > 1e-3).collect();
    let mut seen = vec![false; n];
    let mut basins: Vec<Basin> = Vec::new();
    let mut stack = Vec::new();
    for s in 0..n {
        if !deep[s] || seen[s] {
            continue;
        }
        stack.push(s);
        seen[s] = true;
        let mut cells = Vec::new();
        while let Some(i) = stack.pop() {
            cells.push(i as u32);
            let (x, y) = (i % nx, i / nx);
            for (dx, dy) in NBR {
                let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                    continue;
                }
                let c = cy as usize * nx + cx as usize;
                if deep[c] && !seen[c] {
                    seen[c] = true;
                    stack.push(c);
                }
            }
        }
        if cells.len() < min_cells {
            continue;
        }
        // Spill = bottom + max fill depth (zfill is flat across the basin).
        let spill = cells
            .iter()
            .map(|&c| z[c as usize] + lake_depth[c as usize])
            .fold(f64::NEG_INFINITY, f64::max);
        let max_depth = cells
            .iter()
            .map(|&c| lake_depth[c as usize])
            .fold(0.0f64, f64::max);
        let mut cs = cells;
        cs.sort_by(|&a, &b| {
            (z[a as usize], a)
                .partial_cmp(&(z[b as usize], b))
                .unwrap()
        });
        basins.push(Basin {
            cells_sorted: cs,
            spill,
            max_depth,
        });
    }

    // Greedy order: lake-course puts the dominant basin first; otherwise by
    // quality depth·area descending (tie: first cell index).
    let mut order: Vec<usize> = (0..basins.len()).collect();
    let quality = |b: &Basin| b.max_depth * b.cells_sorted.len() as f64;
    order.sort_by(|&i, &j| {
        let (qi, qj) = (quality(&basins[i]), quality(&basins[j]));
        qj.partial_cmp(&qi)
            .unwrap()
            .then(basins[i].cells_sorted[0].cmp(&basins[j].cells_sorted[0]))
    });
    let _ = lake_course; // ordering already favors the dominant basin

    // Ascending union-find fill with a per-pocket freeze. Flooding a basin
    // cell-by-cell in z order, each connected pocket that reaches the
    // per-body cap is FROZEN at its current stage (its water surface); cells
    // that would join a frozen pocket stay dry, so the fill continues into
    // the basin's other arms. A mega-basin therefore yields SEVERAL
    // hole-scale lakes separated by land at its internal saddles, instead of
    // one truncated mega-lake.
    fn find(parent: &mut [u32], mut i: u32) -> u32 {
        while parent[i as usize] != i {
            let g = parent[parent[i as usize] as usize];
            parent[i as usize] = g;
            i = g;
        }
        i
    }
    let mut bodies = Vec::new();
    let mut remaining = budget_cells;
    let mut slot = vec![u32::MAX; n]; // global cell → basin-local slot
    for &bi in &order {
        if remaining < min_cells {
            break;
        }
        let b = &basins[bi];
        let mut parent: Vec<u32> = Vec::new();
        let mut size: Vec<u32> = Vec::new();
        let mut frozen: Vec<Option<f64>> = Vec::new(); // Some(surface) once capped
        let mut skipped: Vec<bool> = Vec::new();
        let mut processed = 0usize;
        let mut wet = 0usize;
        let mut z_last = f64::NEG_INFINITY;
        for &c in &b.cells_sorted {
            if wet >= remaining {
                break; // budget stage reached
            }
            let (x, y) = (c as usize % nx, c as usize / nx);
            let mut near_frozen = false;
            let mut roots: Vec<u32> = Vec::new();
            for (dx, dy) in NBR {
                let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                    continue;
                }
                let m = cy as usize * nx + cx as usize;
                let s = slot[m];
                if s == u32::MAX || skipped[s as usize] {
                    continue;
                }
                let r = find(&mut parent, s);
                if frozen[r as usize].is_some() {
                    near_frozen = true;
                } else if !roots.contains(&r) {
                    roots.push(r);
                }
            }
            let li = parent.len() as u32;
            slot[c as usize] = li;
            parent.push(li);
            size.push(1);
            frozen.push(None);
            // A cell touching a frozen (full) lake stays dry — the land
            // divider that separates the pocket lakes.
            skipped.push(near_frozen);
            processed += 1;
            if near_frozen {
                continue;
            }
            let mut root = li;
            for r in roots {
                let (mut a, mut bb) = (find(&mut parent, root), find(&mut parent, r));
                if a == bb {
                    continue;
                }
                // Attach smaller to larger (tie → lower slot) — deterministic.
                if (size[a as usize], bb) < (size[bb as usize], a) {
                    std::mem::swap(&mut a, &mut bb);
                }
                parent[bb as usize] = a;
                size[a as usize] += size[bb as usize];
                root = a;
            }
            let zc = z[c as usize];
            if size[root as usize] as usize >= max_body_cells {
                frozen[root as usize] = Some(zc);
            }
            wet += 1;
            z_last = zc;
        }
        let stage_end = if processed == b.cells_sorted.len() {
            b.spill
        } else {
            z_last
        };
        // Group wet cells per pocket (processing order is ascending z, so
        // per-pocket cell lists come out deterministic).
        let mut pocket_cells: Vec<Vec<u32>> = vec![Vec::new(); parent.len()];
        for &c in b.cells_sorted.iter().take(processed) {
            let s = slot[c as usize];
            if skipped[s as usize] {
                continue;
            }
            let r = find(&mut parent, s);
            pocket_cells[r as usize].push(c);
        }
        for (r, mut cells) in pocket_cells.into_iter().enumerate() {
            if cells.len() < min_cells || remaining < cells.len() {
                continue;
            }
            let surface = frozen[r].unwrap_or(stage_end);
            let max_depth = cells
                .iter()
                .map(|&c| surface - z[c as usize])
                .fold(0.0f64, f64::max);
            if max_depth < WETLAND_MAX_BASIN_DEPTH {
                // Too shallow to be a deliberate pond/lake. Terrain water is
                // large open water only — marshy basins are skipped
                // (strategic small ponds come later, in the hole phase).
                continue;
            }
            let class = if cells.len() >= LAKE_MIN_CELLS {
                BodyClass::Lake
            } else {
                BodyClass::Pond
            };
            cells.sort_unstable();
            remaining -= cells.len();
            bodies.push(WaterBody {
                class,
                area_m2: cells.len() as f64 * a0,
                cells,
                surface,
                spill: b.spill.max(surface),
                max_depth,
            });
        }
        // Release the scratch map for the next basin.
        for &c in b.cells_sorted.iter().take(processed) {
            slot[c as usize] = u32::MAX;
        }
    }
    bodies
}

// ---------------------------------------------------------------------------
// Wetlands (HAND)
// ---------------------------------------------------------------------------

/// Select the wetland cell set: lowest height-above-nearest-drainage first.
#[allow(clippy::too_many_arguments)]
fn select_wetlands(
    heights: &Grid<f64>,
    slope: &Grid<f64>,
    rt: &crate::erosion::RoutingSnapshot,
    channel_wet: &[bool],
    channel_ref: &[bool],
    surf_ref: &[f64],
    in_body: &[u16],
    budget_cells: usize,
    noise_seed: u32,
    spec: golf_core::GridSpec,
) -> Vec<u32> {
    if budget_cells == 0 {
        return Vec::new();
    }
    let n = heights.data.len();
    let nx = spec.nx as usize;
    let z = &heights.data;
    // Forward pass (receivers first): reference drainage surface propagates up.
    let mut refz = vec![0.0f64; n];
    let mut has_ref = vec![false; n];
    let mut hand = vec![f64::INFINITY; n];
    for &io in &rt.order {
        let i = io as usize;
        let r = rt.rcv[i] as usize;
        if channel_ref[i] {
            refz[i] = surf_ref[i];
            has_ref[i] = true;
            hand[i] = 0.0;
        } else if r != i && has_ref[r] {
            refz[i] = refz[r];
            has_ref[i] = true;
            hand[i] = z[i] - refz[i];
        }
    }
    // Eligible cells with organic edge noise on the HAND value.
    let mut cand: Vec<(f64, u32)> = Vec::new();
    for i in 0..n {
        if !has_ref[i] || channel_wet[i] || in_body[i] != u16::MAX {
            continue;
        }
        if slope.data[i] >= WETLAND_SLOPE_MAX || hand[i] < 0.0 {
            continue;
        }
        let (x, y) = (i % nx, i / nx);
        let p = spec.world_of(x as u32, y as u32);
        let jitter = WETLAND_NOISE_AMP
            * noise::perlin2(p.x / WETLAND_NOISE_PERIOD, p.y / WETLAND_NOISE_PERIOD, noise_seed)
                .abs();
        cand.push((hand[i] + jitter, i as u32));
    }
    if cand.is_empty() {
        return Vec::new();
    }
    let k = budget_cells.min(cand.len());
    cand.sort_by(|a, b| a.partial_cmp(b).unwrap());
    cand.truncate(k);
    // Keep ascending-HAND order (the connectivity guard drops the tail).
    cand.into_iter().map(|(_, i)| i).collect()
}

/// Group selected wetland cells into components, cull the tiny ones.
fn wetland_components(
    wet_cells: &[u32],
    heights: &Grid<f64>,
    nx: usize,
    ny: usize,
    a0: f64,
) -> Vec<WaterBody> {
    let n = nx * ny;
    let mut in_wet = vec![false; n];
    for &c in wet_cells {
        in_wet[c as usize] = true;
    }
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for &s in wet_cells {
        let s = s as usize;
        if seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let mut cells = Vec::new();
        while let Some(i) = stack.pop() {
            cells.push(i as u32);
            let (x, y) = (i % nx, i / nx);
            for (dx, dy) in NBR {
                let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                    continue;
                }
                let c = cy as usize * nx + cx as usize;
                if in_wet[c] && !seen[c] {
                    seen[c] = true;
                    stack.push(c);
                }
            }
        }
        if cells.len() < MIN_WETLAND_CELLS {
            continue;
        }
        cells.sort_unstable();
        let mean_surf = cells
            .iter()
            .map(|&c| heights.data[c as usize] + 0.1)
            .sum::<f64>()
            / cells.len() as f64;
        out.push(WaterBody {
            class: BodyClass::Wetland,
            area_m2: cells.len() as f64 * a0,
            cells,
            surface: mean_surf,
            spill: mean_surf,
            max_depth: 0.1,
        });
    }
    out
}

// ---------------------------------------------------------------------------
// Guards
// ---------------------------------------------------------------------------

/// Size of the largest 8-connected dry component.
fn largest_dry_component(is_water: &[bool], nx: usize, ny: usize) -> f64 {
    let n = nx * ny;
    let mut seen = vec![false; n];
    let mut best = 0usize;
    let mut stack = Vec::new();
    for s in 0..n {
        if is_water[s] || seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let mut count = 0usize;
        while let Some(i) = stack.pop() {
            count += 1;
            let (x, y) = (i % nx, i / nx);
            for (dx, dy) in NBR {
                let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                    continue;
                }
                let c = cy as usize * nx + cx as usize;
                if !is_water[c] && !seen[c] {
                    seen[c] = true;
                    stack.push(c);
                }
            }
        }
        best = best.max(count);
        if best >= n / 2 {
            break; // cannot be beaten meaningfully; save time
        }
    }
    best as f64
}

// ---------------------------------------------------------------------------
// Corridor field + carving
// ---------------------------------------------------------------------------

/// Closest-segment field of the perennial stream network: per cell, distance
/// to the nearest centerline plus the interpolated surface/bed/width there.
/// Shared by the carve and the grid rebuild so terrain, classes, and depths
/// all agree on the same geometry.
pub struct Corridor {
    pub dmin: Vec<f64>,
    pub surf: Vec<f64>,
    pub bed: Vec<f64>,
    pub width: Vec<f64>,
    /// True where the closest segment belongs to a pooled (thin-lake) link.
    pub pooled: Vec<bool>,
}

pub fn corridor_field(links: &[StreamLink], spec: golf_core::GridSpec) -> Corridor {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let n = nx * ny;
    let mut c = Corridor {
        dmin: vec![f64::INFINITY; n],
        surf: vec![0.0; n],
        bed: vec![0.0; n],
        width: vec![0.0; n],
        pooled: vec![false; n],
    };
    for l in links {
        if !l.perennial {
            continue;
        }
        for k in 1..l.pts.len() {
            let (a, b) = (l.pts[k - 1], l.pts[k]);
            let (wa, wb) = (l.width[k - 1], l.width[k]);
            let (sa, sb) = (l.surface[k - 1], l.surface[k]);
            let reach = 0.5 * wa.max(wb) + cell + 1.0;
            let x0 = (((a.x.min(b.x) - reach) / cell).floor().max(0.0)) as usize;
            let x1 = (((a.x.max(b.x) + reach) / cell).ceil() as usize).min(nx - 1);
            let y0 = (((a.y.min(b.y) - reach) / cell).floor().max(0.0)) as usize;
            let y1 = (((a.y.max(b.y) + reach) / cell).ceil() as usize).min(ny - 1);
            let ab = b - a;
            let ab2 = ab.dot(ab).max(1e-9);
            for gy in y0..=y1 {
                for gx in x0..=x1 {
                    let i = gy * nx + gx;
                    let pp = spec.world_of(gx as u32, gy as u32);
                    let t = ((pp - a).dot(ab) / ab2).clamp(0.0, 1.0);
                    let proj = a.lerp(b, t);
                    let d = pp.distance(proj);
                    if d < c.dmin[i] {
                        c.dmin[i] = d;
                        let w = wa + (wb - wa) * t;
                        let s = sa + (sb - sa) * t;
                        let depth =
                            (CHANNEL_DEPTH_FACTOR * w).clamp(CHANNEL_DEPTH_MIN, CHANNEL_DEPTH_MAX);
                        c.surf[i] = s;
                        c.bed[i] = s - depth;
                        c.width[i] = w;
                        c.pooled[i] = l.pooled;
                    }
                }
            }
        }
    }
    c
}

/// Cut the channel bed and pull banks down along the corridor. Beds are
/// non-increasing downstream by construction (surface is a running min,
/// depth grows with width), so no new pits form.
fn carve_channels(heights: &mut Grid<f64>, cor: &Corridor, in_body: &[u16], cell: f64) {
    #[allow(clippy::needless_range_loop)]
    for i in 0..heights.data.len() {
        if !cor.dmin[i].is_finite() || in_body[i] != u16::MAX {
            continue; // untouched, or inside a pond (bed already below stage)
        }
        let (s, bed, w) = (cor.surf[i], cor.bed[i], cor.width[i]);
        let half = 0.5 * w;
        let z = heights.data[i];
        if cor.dmin[i] <= half {
            // Parabolic cross-section: full depth at center, surface at edge.
            let frac = cor.dmin[i] / half.max(1e-9);
            let target = bed + (s - bed) * frac * frac;
            if target < z {
                heights.data[i] = target.max(z - BANK_MAX_CUT - (s - bed));
            }
        } else if cor.dmin[i] <= half + cell {
            // Bank ring: pull down toward a gentle grade, cut capped.
            let target = s + (cor.dmin[i] - half) * BANK_GRADE;
            if target < z {
                heights.data[i] = target.max(z - BANK_MAX_CUT);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Grid rebuild (entities → grids); the routing phase reuses this after edits.
// ---------------------------------------------------------------------------

pub fn rebuild_grids(
    layer: &mut WaterLayer,
    heights: &Grid<f64>,
    corridor: &Corridor,
    spec: golf_core::GridSpec,
) {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    layer.class = Grid::filled(spec, CLASS_DRY);
    layer.surface = Grid::from_data(spec, heights.data.clone());
    layer.depth = Grid::filled(spec, 0.0);
    layer.mask_soft = Grid::filled(spec, 0.0);

    for b in &layer.bodies {
        let cls = match b.class {
            BodyClass::Pond => CLASS_POND,
            BodyClass::Lake => CLASS_LAKE,
            BodyClass::Wetland => CLASS_WETLAND,
        };
        for &c in &b.cells {
            let i = c as usize;
            layer.class.data[i] = cls;
            if b.class == BodyClass::Wetland {
                layer.surface.data[i] = heights.data[i] + 0.1;
                layer.depth.data[i] = 0.1;
            } else {
                layer.surface.data[i] = b.surface;
                layer.depth.data[i] = (b.surface - heights.data[i]).max(0.0);
            }
        }
    }
    // Stream corridor: every cell within the channel's half-width becomes
    // stream (a 25 m river is two cells wide in the data, not a centerline).
    // Sub-cell brooks still claim their nearest cell row.
    for i in 0..n {
        if !corridor.dmin[i].is_finite() || layer.class.data[i] != CLASS_DRY {
            continue;
        }
        let claim = (0.5 * corridor.width[i]).max(0.35 * spec.cell_size);
        if corridor.dmin[i] <= claim {
            // Pooled lone streams read (and play) as thin ponds.
            layer.class.data[i] = if corridor.pooled[i] { CLASS_POND } else { CLASS_STREAM };
            layer.surface.data[i] = corridor.surf[i].min(layer.surface.data[i]);
            layer.depth.data[i] = (layer.surface.data[i] - heights.data[i]).max(0.05);
        }
    }
    // Soft mask: 1 on water, 0.45 on dry 8-neighbors of water.
    for i in 0..n {
        if layer.class.data[i] != CLASS_DRY {
            layer.mask_soft.data[i] = 1.0;
        }
    }
    for y in 0..ny {
        for x in 0..nx {
            let i = y * nx + x;
            if layer.class.data[i] != CLASS_DRY {
                continue;
            }
            let mut near = false;
            for (dx, dy) in NBR {
                let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                    continue;
                }
                if layer.class.data[cy as usize * nx + cx as usize] != CLASS_DRY {
                    near = true;
                    break;
                }
            }
            if near {
                layer.mask_soft.data[i] = 0.45;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{erosion, generate, ErosionParams, TerrainParams};
    use golf_core::GridSpec;

    fn eroded(seed: u64, relief_amp: f64) -> (Grid<f64>, ErosionOutputs) {
        let params = TerrainParams {
            amplitude: relief_amp,
            base_period: 900.0,
            ..TerrainParams::default()
        };
        let spec = GridSpec::new(Vec2::new(6.25, 6.25), 12.5, 160, 160);
        let mut h = generate(&spec, seed, &params).heights;
        let outs = erosion::erode(&mut h, None, &ErosionParams::default());
        (h, outs)
    }

    fn wet_params() -> WaterParams {
        WaterParams {
            coverage: 0.06,
            stream_area_min: 6.0e4,
            runoff: 1.0,
            pond_share: 0.6,
            wetland_share_cap: 0.6,
            lake_course: false,
            river_area: 0.0,
            noise_seed: 9,
            ..WaterParams::NONE
        }
    }

    #[test]
    fn build_is_deterministic() {
        let (h0, outs) = eroded(21, 60.0);
        let mut a = h0.clone();
        let mut b = h0.clone();
        let la = build(&mut a, &outs, &wet_params());
        let lb = build(&mut b, &outs, &wet_params());
        assert_eq!(a.data, b.data);
        assert_eq!(la.class.data, lb.class.data);
        assert_eq!(la.depth.data, lb.depth.data);
        assert_eq!(la.coverage, lb.coverage);
    }

    #[test]
    fn none_is_a_no_op_on_heights() {
        let (h0, outs) = eroded(22, 60.0);
        let mut h = h0.clone();
        let layer = build(&mut h, &outs, &WaterParams::NONE);
        assert_eq!(h.data, h0.data, "NONE must not touch heights");
        assert_eq!(layer.coverage, 0.0);
        assert!(layer.bodies.is_empty());
        assert!(layer.streams.iter().all(|l| !l.perennial), "swales only");
        assert!(layer.mask_soft.data.iter().all(|&v| v == 0.0));
    }

    #[test]
    fn stream_surfaces_are_monotone_downstream() {
        let (mut h, outs) = eroded(23, 80.0);
        let layer = build(&mut h, &outs, &wet_params());
        let mut checked = 0;
        for l in &layer.streams {
            if !l.perennial {
                continue;
            }
            assert_eq!(l.pts.len(), l.surface.len());
            assert_eq!(l.pts.len(), l.width.len());
            for k in 1..l.surface.len() {
                assert!(
                    l.surface[k] <= l.surface[k - 1] + 1e-9,
                    "surface rises along link at pt {k}"
                );
                checked += 1;
            }
        }
        assert!(checked > 0, "no stream points checked");
    }

    #[test]
    fn stream_endpoints_are_anchored() {
        // The terrain-water contract: every visible stream begins and ends at
        // the map edge, standing water, or a junction — never plain ground
        // (and no dug source ponds justifying a mid-slope head).
        let mut checked = 0;
        for (seed, msw) in [(23u64, 0.0), (31, 10.0), (44, 0.0), (57, 10.0)] {
            let (mut h, outs) = eroded(seed, 60.0);
            let p = WaterParams { min_stream_width: msw, ..wet_params() };
            let layer = build(&mut h, &outs, &p);
            let (nx, ny) = (layer.class.spec.nx as usize, layer.class.spec.ny as usize);
            // Anchor targets: ponds/lakes (wetlands never anchored anything)
            // and other perennial channels.
            let mut body = vec![false; nx * ny];
            for b in layer.bodies.iter().filter(|b| b.class != BodyClass::Wetland) {
                for &c in &b.cells {
                    body[c as usize] = true;
                }
            }
            let mut occ = vec![u32::MAX; nx * ny];
            for (li, l) in layer.streams.iter().enumerate() {
                if !l.perennial {
                    continue;
                }
                for &c in &l.cells {
                    if occ[c as usize] == u32::MAX {
                        occ[c as usize] = li as u32;
                    }
                }
            }
            let anchored = |c: usize, li: usize| -> bool {
                let (x, y) = (c % nx, c / nx);
                if x <= 1 || y <= 1 || x >= nx - 2 || y >= ny - 2 || body[c] {
                    return true;
                }
                NBR.iter().any(|&(dx, dy)| {
                    let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                    cx >= 0
                        && cy >= 0
                        && (cx as usize) < nx
                        && (cy as usize) < ny
                        && {
                            let m = cy as usize * nx + cx as usize;
                            body[m] || (occ[m] != u32::MAX && occ[m] != li as u32)
                        }
                })
            };
            for (li, l) in layer.streams.iter().enumerate() {
                if !l.perennial || l.pooled {
                    continue;
                }
                let head = l.cells[0] as usize;
                let tail = *l.cells.last().unwrap() as usize;
                assert!(anchored(head, li), "seed {seed}: head of link {li} dangles");
                assert!(anchored(tail, li), "seed {seed}: tail of link {li} dangles");
                checked += 1;
            }
        }
        assert!(checked > 0, "no perennial links checked");
    }

    #[test]
    fn bodies_have_sane_proportions() {
        // Aspect-ratio cull: no narrow string-of-cells ponds survive.
        for seed in [23u64, 31, 44] {
            let (mut h, outs) = eroded(seed, 60.0);
            let p = WaterParams { min_body_area: 3500.0, ..wet_params() };
            let layer = build(&mut h, &outs, &p);
            let nx = layer.class.spec.nx as usize;
            for b in layer.bodies.iter().filter(|b| b.class != BodyClass::Wetland) {
                let m = b.cells.len() as f64;
                let (mut mx, mut my) = (0.0f64, 0.0f64);
                for &c in &b.cells {
                    mx += (c as usize % nx) as f64;
                    my += (c as usize / nx) as f64;
                }
                mx /= m;
                my /= m;
                let (mut cxx, mut cyy, mut cxy) = (0.0f64, 0.0f64, 0.0f64);
                for &c in &b.cells {
                    let dx = (c as usize % nx) as f64 - mx;
                    let dy = (c as usize / nx) as f64 - my;
                    cxx += dx * dx;
                    cyy += dy * dy;
                    cxy += dx * dy;
                }
                cxx = cxx / m + 1.0 / 12.0;
                cyy = cyy / m + 1.0 / 12.0;
                cxy /= m;
                let half_tr = 0.5 * (cxx + cyy);
                let disc =
                    (half_tr * half_tr - (cxx * cyy - cxy * cxy)).max(0.0).sqrt();
                let l2 = (half_tr - disc).max(1e-9);
                let aspect = ((half_tr + disc) / l2).sqrt();
                let width_m = (12.0 * l2).sqrt() * 12.5;
                assert!(
                    (aspect <= ASPECT_MAX + 0.8 || width_m >= ASPECT_WIDTH_MIN - 4.0)
                        && aspect <= ASPECT_HARD_MAX + 1.0,
                    "seed {seed}: skinny body survived (aspect {aspect:.1}, width {width_m:.0} m)"
                );
            }
        }
    }

    #[test]
    fn external_river_makes_a_wide_meandering_channel() {
        let (mut h, outs) = eroded(33, 50.0);
        let p = WaterParams { coverage: 0.05, river_area: 5.0e7, ..wet_params() };
        let layer = build(&mut h, &outs, &p);
        let max_w = layer
            .streams
            .iter()
            .filter(|l| l.perennial)
            .flat_map(|l| l.width.iter())
            .fold(0.0f64, |m, &w| m.max(w));
        assert!(max_w >= 15.0, "river width {max_w} m too small for 50 km² inflow");
        // The wide corridor must claim multiple cells across, not a centerline:
        // stream-classed cells should clearly exceed the union of path cells.
        let path_cells: usize = layer
            .streams
            .iter()
            .filter(|l| l.perennial)
            .map(|l| l.cells.len())
            .sum();
        let stream_cells = layer
            .class
            .data
            .iter()
            .filter(|&&c| c == CLASS_STREAM)
            .count();
        assert!(
            stream_cells as f64 >= path_cells as f64 * 1.1,
            "corridor not wider than centerline: {stream_cells} vs {path_cells}"
        );
    }

    #[test]
    fn bodies_respect_spill_and_depth() {
        let (mut h, outs) = eroded(24, 70.0);
        let layer = build(&mut h, &outs, &wet_params());
        for b in &layer.bodies {
            assert!(b.surface <= b.spill + 1e-6, "surface above spill");
            assert!(b.max_depth >= 0.0);
            for &c in &b.cells {
                assert!(layer.depth.data[c as usize] >= 0.0);
            }
        }
    }

    #[test]
    fn carve_only_lowers_and_is_bounded() {
        let (h0, outs) = eroded(25, 60.0);
        let mut h = h0.clone();
        build(&mut h, &outs, &wet_params());
        // Channel carve + bank pull-down, plus (for a body dropped after its
        // outline was regularized) the bounded outline dig on the same cell.
        let max_cut = CHANNEL_DEPTH_MAX + BANK_MAX_CUT + REG_DIG_MAX + 1e-9;
        for i in 0..h.data.len() {
            assert!(h.data[i] <= h0.data[i] + 1e-12, "carve raised a cell");
            assert!(
                h0.data[i] - h.data[i] <= max_cut,
                "cut too deep at {i}: {}",
                h0.data[i] - h.data[i]
            );
        }
    }

    #[test]
    fn carving_creates_no_meaningful_new_fill() {
        // Re-flood the carved surface: fill beyond what the pre-carve terrain
        // already had (unselected natural basins) must be negligible.
        let (mut h, outs) = eroded(26, 60.0);
        let layer = build(&mut h, &outs, &wet_params());
        let mut h2 = h.clone();
        let outs2 = erosion::erode(
            &mut h2,
            None,
            &ErosionParams { intensity: 0.0, deposition: 0.0, diffusion: 0.0, hardness_contrast: 0.0 },
        );
        let mut bad = 0usize;
        for i in 0..h.data.len() {
            let grew = outs2.lake_depth.data[i] - outs.lake_depth.data[i];
            if layer.class.data[i] == CLASS_DRY && grew > 0.05 {
                bad += 1;
            }
        }
        assert!(
            bad <= h.data.len() / 500,
            "carving created {bad} new ponded cells"
        );
    }

    #[test]
    fn dry_connectivity_holds() {
        for seed in [27u64, 28, 29] {
            let (mut h, outs) = eroded(seed, 40.0);
            let p = WaterParams { coverage: 0.15, ..wet_params() };
            let layer = build(&mut h, &outs, &p);
            let n = layer.class.data.len();
            let blocking: Vec<bool> = (0..n).map(|i| layer.is_blocking(i)).collect();
            let biggest = largest_dry_component(&blocking, 160, 160);
            assert!(
                biggest >= DRY_CONNECTIVITY_MIN * n as f64,
                "seed {seed}: routable component {biggest} < 70%"
            );
        }
    }

    #[test]
    fn coverage_lands_near_target() {
        let (mut h, outs) = eroded(30, 30.0);
        let p = WaterParams { coverage: 0.08, ..wet_params() };
        let layer = build(&mut h, &outs, &p);
        assert!(
            layer.coverage > 0.02 && layer.coverage <= 0.10,
            "coverage {} far from target 0.08",
            layer.coverage
        );
    }

    #[test]
    fn wetland_components_are_culled() {
        let (mut h, outs) = eroded(31, 20.0);
        let layer = build(&mut h, &outs, &wet_params());
        for b in &layer.bodies {
            if b.class == BodyClass::Wetland && b.max_depth <= 0.1 {
                assert!(b.cells.len() >= MIN_WETLAND_CELLS);
            }
        }
    }
}
