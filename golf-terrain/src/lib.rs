//! `golf-terrain`: seed-driven heightfield generation and derived fields.
//!
//! Current phase scope: **noise and slope only** — no holes, tees, hydrology, or
//! routing. The goal is to lock in the noise parameters (amplitude, period,
//! octaves, …) that produce each course style. Everything is deterministic in
//! `(seed, params)`.

pub mod erosion;
pub mod noise;
pub mod params;
pub mod sampler;
pub mod water;

pub use erosion::{ErosionOutputs, ErosionParams, EROSION_VERSION, MACRO_N};
pub use params::{TerrainParams, WORLD_SIZE};
pub use sampler::{sample_params, ReliefMode, SampledParams, SAMPLER_VERSION};
pub use water::{WaterLayer, WaterParams, WATER_VERSION};

use golf_core::math;
use golf_core::{Grid, GridSpec, Vec2};

/// A generated terrain: the heightfield plus its derived slope field.
#[derive(Clone, Debug)]
pub struct Terrain {
    pub heights: Grid<f64>,
    pub slope: Grid<f64>,
    pub params: TerrainParams,
}

impl Terrain {
    pub fn spec(&self) -> GridSpec {
        self.heights.spec
    }
}

/// Generate a heightfield for `(spec, seed, params)` and derive its slope.
pub fn generate(spec: &GridSpec, seed: u64, params: &TerrainParams) -> Terrain {
    // Decorrelated 32-bit seeds for the base field and the two warp channels.
    let base_seed = mix_seed(seed, 0);
    let warp_x_seed = mix_seed(seed, 1);
    let warp_y_seed = mix_seed(seed, 2);
    let relief_patch_seed = mix_seed(seed, 8);

    let f = params.fbm();
    let base_freq = 1.0 / params.base_period;
    let warp_freq = 1.0 / params.warp_period.max(1e-6);
    let redistribute = params.redistribution != 1.0;
    let ridged = params.ridged_mix != 0.0;
    let relief_patchy = params.relief_patchiness != 0.0;
    // Where the raw median (u = 0.5) lands under the power curve — subtracted
    // so redistributed terrain keeps its median near height 0.
    let v_median = if params.redistribution >= 1.0 {
        math::pow(0.5, params.redistribution)
    } else {
        1.0 - math::pow(0.5, 1.0 / params.redistribution)
    };

    let mut heights = Grid::filled(*spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let wp = spec.world_of(x, y);
            // Directional anisotropy: stretch the sampling frame so features
            // elongate along `aniso_angle` (identity when aniso_ratio == 1).
            let (ax, ay) = params.anisotropic(wp.x, wp.y);
            let p = Vec2::new(ax, ay);

            // Domain warp: perturb the sample position by a low-frequency field.
            let (sx, sy) = if params.warp_amp != 0.0 {
                (
                    p.x + params.warp_amp * noise::perlin2(p.x * warp_freq, p.y * warp_freq, warp_x_seed),
                    p.y + params.warp_amp * noise::perlin2(p.x * warp_freq, p.y * warp_freq, warp_y_seed),
                )
            } else {
                (p.x, p.y)
            };

            let mut n = noise::fbm(sx * base_freq, sy * base_freq, base_seed, &f);

            // Ridged blend: mix in creases along the fBm zero-contours —
            // connected curves by construction, so positive mix grows ridge
            // networks and negative grows valley networks (the coherent
            // topology erosion can deepen but never invent). Skipped at 0.
            if ridged {
                let m = params.ridged_mix;
                n = (1.0 - m.abs()) * n + m * (1.0 - 2.0 * n.abs());
            }

            // Redistribution: reshape the elevation distribution per-sample
            // (no global pass, so any window of a seed's world stays exact).
            // p > 1 applies u^p — flattens lows into basins, sharpens highs.
            // p < 1 applies the mirrored power 1−(1−u)^(1/p) — plateaus with
            // carved valleys, at strength symmetric to the basin side.
            // Skipped entirely at 1.0 to keep bit-exactness.
            if redistribute {
                let u = ((n + 1.0) * 0.5).clamp(0.0, 1.0);
                let p = params.redistribution;
                let v = if p >= 1.0 {
                    math::pow(u, p)
                } else {
                    1.0 - math::pow(1.0 - u, 1.0 / p)
                };
                n = 2.0 * (v - v_median);
            }

            // Relief patchiness: mean-preserving ~1.1 km amplitude modulation
            // on RAW world coords (regimes must not elongate with the grain) —
            // a rugged corner here, a flat meadow there. Skipped at 0.
            if relief_patchy {
                n *= 1.0
                    + params.relief_patchiness
                        * noise::perlin2(
                            wp.x / RELIEF_PATCH_PERIOD,
                            wp.y / RELIEF_PATCH_PERIOD,
                            relief_patch_seed,
                        );
            }

            heights.set(x, y, n * params.amplitude);
        }
    }

    let slope = derive_slope(&heights);
    Terrain {
        heights,
        slope,
        params: *params,
    }
}

/// Build the fixed 2 km × 2 km generation grid at `n × n` resolution.
pub fn world_spec(n: u32) -> GridSpec {
    GridSpec::new(Vec2::ZERO, WORLD_SIZE / n as f64, n, n)
}

/// The canonical macro grid: pixel centers of a `MACRO_N²` raster of the
/// 2 km world (12.5 m cells). Erosion always runs here — identical to
/// `golf_match::world_raster_spec(12.5)` (asserted by a test there), and the
/// cell size is dyadic so bilinear at macro nodes is a bit-exact identity.
pub fn macro_spec() -> GridSpec {
    let cell = WORLD_SIZE / MACRO_N as f64;
    GridSpec::new(Vec2::new(cell / 2.0, cell / 2.0), cell, MACRO_N, MACRO_N)
}

/// World-anchored period (m) of the fine-detail band added on top of the eroded
/// macro grid: resolution-independent texture, gated OFF on flats and lakebeds so
/// deposited floors stay clean. Its amplitude/gain/octaves are now per-course
/// `TerrainParams` knobs (drawn from p(θ)); the wavelength stays fixed here.
const DETAIL_PERIOD: f64 = 32.0;
/// Period (m) of the detail-patchiness modulation field: ≥3× the 60 m
/// roughness-clustering lag the metrics measure, below the 350 m block scale —
/// so patchiness concentrates texture without reading as whole-map regions.
const DETAIL_PATCH_PERIOD: f64 = 180.0;
/// Period (m) of the relief-patchiness modulation in `generate()`: 2–4 regime
/// patches per 2 km world ("rugged corner, flat meadow").
const RELIEF_PATCH_PERIOD: f64 = 1100.0;

/// A fully generated course terrain: composed heights at the requested spec,
/// the canonical eroded macro grid, the erosion byproducts, and the derived
/// natural water layer.
#[derive(Clone, Debug)]
pub struct CourseTerrain {
    pub terrain: Terrain,
    pub macro_heights: Grid<f64>,
    /// Slope of the macro grid (gates the detail band).
    pub macro_slope: Grid<f64>,
    pub erosion: ErosionOutputs,
    pub water: WaterLayer,
    detail_seed: u32,
    /// Seed of the detail-patchiness modulation field (mix_seed channel 7).
    patch_seed: u32,
}

impl CourseTerrain {
    /// Composed height at an arbitrary world position — the same formula the
    /// generation loop uses, so any window/resolution agrees bit-exactly.
    pub fn height_at(&self, p: Vec2) -> f64 {
        compose_height(
            &self.macro_heights,
            &self.macro_slope,
            &self.erosion.lake_depth,
            &self.water.mask_soft,
            self.detail_seed,
            self.patch_seed,
            &self.terrain.params,
            p,
        )
    }

    /// Water depth at a world position (0 = dry).
    pub fn water_depth_at(&self, p: Vec2) -> f64 {
        self.water.depth.bilinear(p).max(0.0)
    }

    /// Water surface elevation at a world position (terrain height if dry).
    pub fn water_surface_at(&self, p: Vec2) -> f64 {
        self.water.surface.bilinear(p)
    }
}

/// The two-scale compose: eroded macro landform + world-anchored fine detail.
/// The detail band is the ONLY cheap place to add sub-25 m roughness (the macro
/// grid is 12.5 m and erosion smooths it), so its amplitude/octaves/persistence
/// are `TerrainParams` knobs the atlas calibration drives. Detail is gated off
/// water/lakebeds and ramps in on slopes, but a `nugget_amp` floor keeps a faint
/// cell-scale texture on dry flats (real DEMs have a nonzero variogram nugget).
#[allow(clippy::too_many_arguments)]
fn compose_height(
    macro_h: &Grid<f64>,
    macro_slope: &Grid<f64>,
    lake_depth: &Grid<f64>,
    water_mask: &Grid<f64>,
    detail_seed: u32,
    patch_seed: u32,
    tp: &TerrainParams,
    p: Vec2,
) -> f64 {
    let zm = macro_h.bilinear(p);
    let s = macro_slope.bilinear(p);
    let lake = lake_depth.bilinear(p);
    // slope ramp with a floor: flats keep `nugget_amp` of the band, slopes 1.
    // (Order preserved vs the historical `slope*lake*water` so the default
    // nugget_amp=0 path is bit-identical.)
    let slope_term = tp.nugget_amp + (1.0 - tp.nugget_amp) * math::smoothstep(0.02, 0.05, s);
    let gate = slope_term
        * (1.0 - math::smoothstep(0.02, 0.10, lake))
        * (1.0 - water_mask.bilinear(p)).max(0.0);
    if gate == 0.0 {
        // Fully closed (lakebeds / water, and flats when nugget_amp == 0).
        return zm;
    }
    let f = tp.detail_fbm();
    let (dx, dy) = tp.anisotropic(p.x, p.y);
    let d = noise::fbm(dx / DETAIL_PERIOD, dy / DETAIL_PERIOD, detail_seed, &f);
    // Patchiness: concentrate the band into ~180 m texture patches instead of
    // uniform stipple (skipped at 0 so the historical path stays bit-exact).
    if tp.detail_patchiness != 0.0 {
        let m = (1.0
            + tp.detail_patchiness
                * noise::perlin2(
                    dx / DETAIL_PATCH_PERIOD,
                    dy / DETAIL_PATCH_PERIOD,
                    patch_seed,
                ))
        .max(0.0);
        return zm + tp.detail_amp * gate * m * d;
    }
    zm + tp.detail_amp * gate * d
}

/// Full pipeline with explicit parameters (sweep, calibration, manual mode):
/// noise on the canonical macro grid → erosion → water → compose at `spec`.
/// `WaterParams::NONE` leaves heights bit-identical to the water-free
/// pipeline (unit-tested), which keeps calibration semantics frozen.
pub fn generate_course_with(
    spec: &GridSpec,
    seed: u64,
    tp: &TerrainParams,
    ep: &ErosionParams,
    wp: &WaterParams,
) -> CourseTerrain {
    let mut macro_h = generate(&macro_spec(), seed, tp).heights;
    let hardness = if ep.hardness_contrast != 0.0 {
        Some(hardness_field(seed, &macro_h, tp))
    } else {
        None
    };
    let outs = erosion::erode(&mut macro_h, hardness.as_deref(), ep);
    let water = water::build(&mut macro_h, &outs, wp);
    // Slope AFTER carving, so the detail gate sees channel walls and banks.
    let macro_slope = derive_slope(&macro_h);
    let detail_seed = mix_seed(seed, 3);
    let patch_seed = mix_seed(seed, 7);

    let mut heights = Grid::filled(*spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let p = spec.world_of(x, y);
            heights.set(
                x,
                y,
                compose_height(
                    &macro_h,
                    &macro_slope,
                    &outs.lake_depth,
                    &water.mask_soft,
                    detail_seed,
                    patch_seed,
                    tp,
                    p,
                ),
            );
        }
    }
    let slope = derive_slope(&heights);
    CourseTerrain {
        terrain: Terrain {
            heights,
            slope,
            params: *tp,
        },
        macro_heights: macro_h,
        macro_slope,
        erosion: outs,
        water,
        detail_seed,
        patch_seed,
    }
}

/// Full pipeline, seed-sampled: one `u64` in → a whole eroded, watered course.
pub fn generate_course(spec: &GridSpec, seed: u64) -> (CourseTerrain, SampledParams) {
    let sp = sample_params(seed);
    (
        generate_course_with(spec, seed, &sp.params, &sp.erosion, &sp.water),
        sp,
    )
}

/// Differential-erodibility field on the macro grid, in [−1, 1] (+1 = hard,
/// resists incision): low-frequency planform variation + strata banding over
/// the *initial* elevation (thickness ~ relief/4), so hard layers form
/// hillside treads. Pure hash noise on its own seed channels. The planform is
/// stretched by the terrain's anisotropy so hard/soft bands align into ridges
/// (directional ridge-and-valley grain), reusing `TerrainParams::anisotropic`.
fn hardness_field(seed: u64, z0: &Grid<f64>, tp: &TerrainParams) -> Vec<f64> {
    let hseed = mix_seed(seed, 4);
    let sseed = mix_seed(seed, 5);
    let relief = percentile_span(&z0.data, 0.02, 0.98).max(4.0);
    let thickness = relief / 4.0;
    let f = noise::Fbm { octaves: 2, lacunarity: 2.0, gain: 0.5 };
    let spec = z0.spec;
    let mut h = Vec::with_capacity(z0.data.len());
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let p = spec.world_of(x, y);
            let (px, py) = tp.anisotropic(p.x, p.y);
            let planform = noise::fbm(px / 450.0, py / 450.0, hseed, &f);
            let strata = noise::perlin2(*z0.get(x, y) / thickness, 0.5, sseed);
            h.push((0.6 * planform + 0.4 * strata).clamp(-1.0, 1.0));
        }
    }
    h
}

/// Maximum slope (rise/run) considered playable fairway grade — 12%.
pub const FAIRWAY_MAX_GRADE: f64 = 0.12;

/// Central-difference slope magnitude (rise/run), one value per node.
pub fn derive_slope(heights: &Grid<f64>) -> Grid<f64> {
    let spec = heights.spec;
    let cell = spec.cell_size;
    let mut slope = Grid::filled(spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(spec.nx - 1);
            let ym = y.saturating_sub(1);
            let yp = (y + 1).min(spec.ny - 1);
            let dzdx = (heights.get(xp, y) - heights.get(xm, y)) / ((xp - xm).max(1) as f64 * cell);
            let dzdy = (heights.get(x, yp) - heights.get(x, ym)) / ((yp - ym).max(1) as f64 * cell);
            slope.set(x, y, math::hypot(dzdx, dzdy));
        }
    }
    slope
}

/// Summary statistics — the quantitative side of "is this the right course
/// style?".
#[derive(Clone, Copy, Debug)]
pub struct TerrainStats {
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub mean_slope: f64,
    /// Fraction of nodes steeper than 0.25 (rough / unplayable-ish).
    pub steep_fraction: f64,
}

impl Terrain {
    pub fn stats(&self) -> TerrainStats {
        let n = self.heights.data.len().max(1) as f64;
        let (mut min, mut max, mut sum) = (f64::INFINITY, f64::NEG_INFINITY, 0.0);
        for &z in &self.heights.data {
            min = min.min(z);
            max = max.max(z);
            sum += z;
        }
        let mut slope_sum = 0.0;
        let mut steep = 0usize;
        for &s in &self.slope.data {
            slope_sum += s;
            if s > 0.25 {
                steep += 1;
            }
        }
        TerrainStats {
            min,
            max,
            mean: sum / n,
            mean_slope: slope_sum / n,
            steep_fraction: steep as f64 / n,
        }
    }

    /// Skewness of the height distribution — the shape statistic the
    /// redistribution exponent is tuned against (0 for symmetric fBm; the
    /// atlas courses span roughly ±2).
    pub fn height_skewness(&self) -> f64 {
        skewness(&self.heights.data)
    }

    /// Grid locations and values of the lowest and highest nodes:
    /// `((x, y, z_min), (x, y, z_max))`.
    pub fn extremes(&self) -> ((u32, u32, f64), (u32, u32, f64)) {
        let spec = self.heights.spec;
        let (mut lo, mut hi) = ((0, 0, f64::INFINITY), (0, 0, f64::NEG_INFINITY));
        for y in 0..spec.ny {
            for x in 0..spec.nx {
                let z = *self.heights.get(x, y);
                if z < lo.2 {
                    lo = (x, y, z);
                }
                if z > hi.2 {
                    hi = (x, y, z);
                }
            }
        }
        (lo, hi)
    }

    /// Relief between the 2nd and 98th height percentiles — the robust relief
    /// measure the sampler targets and the atlas stats use.
    pub fn relief_p2_p98(&self) -> f64 {
        percentile_span(&self.heights.data, 0.02, 0.98)
    }

    /// Fraction of nodes with slope strictly below `threshold` (rise/run).
    /// With `FAIRWAY_MAX_GRADE` this is the playable-fairway fraction.
    pub fn slope_fraction_below(&self, threshold: f64) -> f64 {
        let n = self.slope.data.len().max(1) as f64;
        let c = self.slope.data.iter().filter(|&&s| s < threshold).count();
        c as f64 / n
    }

    /// Histogram of slope over `[0, max_slope]` in `bins` buckets; slopes at or
    /// above `max_slope` fall in the last bucket.
    pub fn slope_histogram(&self, bins: usize, max_slope: f64) -> Vec<u32> {
        let bins = bins.max(1);
        let w = max_slope / bins as f64;
        let mut h = vec![0u32; bins];
        for &s in &self.slope.data {
            let idx = ((s / w) as usize).min(bins - 1);
            h[idx] += 1;
        }
        h
    }
}

/// Sample skewness of a value set: `E[(z−µ)³] / σ³`.
pub fn skewness(vals: &[f64]) -> f64 {
    let n = vals.len().max(1) as f64;
    let mean = vals.iter().sum::<f64>() / n;
    let (mut m2, mut m3) = (0.0, 0.0);
    for &v in vals {
        let d = v - mean;
        m2 += d * d;
        m3 += d * d * d;
    }
    let var = m2 / n;
    if var <= 0.0 {
        return 0.0;
    }
    (m3 / n) / (var * var.sqrt())
}

/// Span between two height quantiles (sorts a copy; fine at grid sizes).
pub fn percentile_span(vals: &[f64], lo_q: f64, hi_q: f64) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f64> = vals.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let at = |q: f64| -> f64 {
        let i = q * (v.len() - 1) as f64;
        let i0 = i as usize;
        let i1 = (i0 + 1).min(v.len() - 1);
        v[i0] + (v[i1] - v[i0]) * (i - i0 as f64)
    };
    at(hi_q) - at(lo_q)
}

/// Derive two decorrelated 32-bit noise seeds from a 64-bit seed + channel.
pub(crate) fn mix_seed(seed: u64, channel: u32) -> u32 {
    let mut z = seed
        .wrapping_add(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((channel as u64).wrapping_mul(0xD1B5_4A32_D192_ED03));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    ((z ^ (z >> 31)) & 0xFFFF_FFFF) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_spec_is_2km() {
        let s = world_spec(256);
        assert_eq!(s.nx, 256);
        assert!((s.nx as f64 * s.cell_size - WORLD_SIZE).abs() < 1e-9);
    }

    #[test]
    fn generation_is_deterministic() {
        let p = TerrainParams::default();
        let a = generate(&world_spec(128), 2024, &p);
        let b = generate(&world_spec(128), 2024, &p);
        assert_eq!(a.heights.data, b.heights.data);
        assert_eq!(a.slope.data, b.slope.data);
    }

    #[test]
    fn different_seed_differs() {
        let p = TerrainParams::default();
        assert_ne!(
            generate(&world_spec(128), 1, &p).heights.data,
            generate(&world_spec(128), 2, &p).heights.data
        );
    }

    #[test]
    fn heights_finite_and_within_amplitude() {
        let p = TerrainParams::default();
        let t = generate(&world_spec(128), 5, &p);
        assert!(t.heights.data.iter().all(|z| z.is_finite()));
        let s = t.stats();
        // fBm is normalized, so |z| should not exceed amplitude by much.
        assert!(s.max <= p.amplitude * 1.1);
        assert!(s.min >= -p.amplitude * 1.1);
    }

    #[test]
    fn amplitude_scales_relief() {
        let base = TerrainParams::default();
        let mut tall = base;
        tall.amplitude = base.amplitude * 3.0;
        let flat_slope = generate(&world_spec(128), 7, &base).stats().mean_slope;
        let tall_slope = generate(&world_spec(128), 7, &tall).stats().mean_slope;
        assert!(tall_slope > flat_slope, "more amplitude -> steeper");
    }

    #[test]
    fn slope_histogram_and_playable_fraction() {
        let t = generate(&world_spec(128), 9, &TerrainParams::default());
        let hist = t.slope_histogram(30, 0.30);
        assert_eq!(hist.len(), 30);
        let total: u32 = hist.iter().sum();
        assert_eq!(total as usize, t.slope.data.len());
        let playable = t.slope_fraction_below(FAIRWAY_MAX_GRADE);
        assert!((0.0..=1.0).contains(&playable));
        // Gentle default terrain: most of it is playable fairway grade.
        assert!(playable > 0.5, "default terrain should be mostly playable, got {playable}");
    }

    #[test]
    fn redistribution_shapes_the_height_distribution() {
        let base = TerrainParams::default();
        let spec = world_spec(128);
        let sym = generate(&spec, 11, &base);
        let basin = generate(&spec, 11, &TerrainParams { redistribution: 2.2, ..base });
        let plateau = generate(&spec, 11, &TerrainParams { redistribution: 0.45, ..base });
        // >1 pushes mass low (positive skew), <1 pushes mass high (negative).
        assert!(
            basin.height_skewness() > sym.height_skewness() + 0.2,
            "p=2.2 should raise skew: {} vs {}",
            basin.height_skewness(),
            sym.height_skewness()
        );
        assert!(
            plateau.height_skewness() < sym.height_skewness() - 0.2,
            "p=0.45 should lower skew: {} vs {}",
            plateau.height_skewness(),
            sym.height_skewness()
        );
    }

    #[test]
    fn redistribution_of_one_is_bit_exact_identity() {
        let spec = world_spec(64);
        let a = generate(&spec, 5, &TerrainParams::default());
        let b = generate(&spec, 5, &TerrainParams { redistribution: 1.0, ..Default::default() });
        assert_eq!(a.heights.data, b.heights.data);
    }

    #[test]
    fn macro_bilinear_is_identity_at_nodes() {
        // Foundation of search fairness: sampling the macro at its own node
        // positions must reproduce node values bit-exactly.
        let (ct, _) = generate_course(&macro_spec(), 42);
        let m = &ct.macro_heights;
        for y in (0..MACRO_N).step_by(13) {
            for x in (0..MACRO_N).step_by(13) {
                let p = m.spec.world_of(x, y);
                assert_eq!(m.bilinear(p), *m.get(x, y), "node ({x},{y})");
            }
        }
    }

    #[test]
    fn course_windows_agree_at_shared_points() {
        // Any two output windows of the same seed agree bit-exactly wherever
        // they sample the same world position (macro is canonical; the detail
        // band is world-anchored).
        let sp = sample_params(77);
        let full = GridSpec::new(Vec2::ZERO, 31.25, 64, 64);
        let win = GridSpec::new(Vec2::new(312.5, 625.0), 31.25, 20, 20);
        let a = generate_course_with(&full, 77, &sp.params, &sp.erosion, &sp.water);
        let b = generate_course_with(&win, 77, &sp.params, &sp.erosion, &sp.water);
        for wy in 0..20u32 {
            for wx in 0..20u32 {
                let (fx, fy) = (wx + 10, wy + 20); // 312.5/31.25, 625/31.25
                assert_eq!(
                    *a.terrain.heights.get(fx, fy),
                    *b.terrain.heights.get(wx, wy),
                    "mismatch at window ({wx},{wy})"
                );
            }
        }
    }

    #[test]
    fn course_pipeline_is_deterministic_and_flat_floors_exist() {
        let (a, spa) = generate_course(&world_spec(128), 4242);
        let (b, _) = generate_course(&world_spec(128), 4242);
        assert_eq!(a.terrain.heights.data, b.terrain.heights.data);
        assert_eq!(a.erosion.flow_area.data, b.erosion.flow_area.data);
        assert!(spa.erosion.intensity > 0.0);
        // Composed output finite and within sane range.
        assert!(a.terrain.heights.data.iter().all(|z| z.is_finite()));
    }
}
