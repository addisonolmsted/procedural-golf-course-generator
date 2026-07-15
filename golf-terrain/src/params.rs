//! Terrain parameters — the noise knobs. Usually produced by the seed
//! sampler ([`crate::sampler`]); the viewer's Manual mode exposes them as raw
//! sliders for hand-tuning experiments.

use crate::noise::Fbm;
use serde::{Deserialize, Serialize};

/// World size of the generation area, in meters. Fixed at 2 km × 2 km.
pub const WORLD_SIZE: f64 = 2000.0;

/// The full noise/terrain parameter set. World units are meters.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct TerrainParams {
    /// Vertical scale: redistributed fBm output (~[-1,1]) is multiplied by
    /// this. The sampler solves it from a target relief.
    pub amplitude: f64,
    /// World size of the largest noise feature (bigger = smoother, broader).
    pub base_period: f64,
    /// Number of fBm octaves (detail layers).
    pub octaves: u32,
    /// Frequency multiplier per octave (~2.0).
    pub lacunarity: f64,
    /// Amplitude falloff per octave (persistence, ~0.5). Higher = rougher.
    pub gain: f64,
    /// Domain-warp strength in world units (0 = off). Adds organic meander.
    pub warp_amp: f64,
    /// World size of the domain-warp feature.
    pub warp_period: f64,
    /// Redistribution exponent on normalized height `u = (n+1)/2`. `p > 1`
    /// applies `u^p`: flattens the lowlands into broad basins and sharpens
    /// the peaks. `p < 1` applies the mirrored power `1−(1−u)^(1/p)`:
    /// plateaus with carved valleys, at symmetric strength. `1` is the
    /// identity (bit-exact passthrough).
    pub redistribution: f64,

    // --- fine-detail band (query-time texture; see `compose_height`) --------
    /// Peak vertical amplitude of the fine-detail band, meters. The atlas
    /// calibration wants this well above the historical 0.25 to reach the
    /// real high-frequency roughness / curvature the macro grid can't hold.
    #[serde(default = "d_detail_amp")]
    pub detail_amp: f64,
    /// Persistence of the detail fBm (higher = more energy in the finer octaves).
    #[serde(default = "d_detail_gain")]
    pub detail_gain: f64,
    /// Detail octave count: with period 32 m / lacunarity 2, octave k lands at
    /// 32/2^k m, so 4 reaches ~4 m — the sub-25 m band the metrics measure.
    #[serde(default = "d_detail_octaves")]
    pub detail_octaves: u32,
    /// Un-gated micro-nugget floor (0..1): fraction of the detail band applied
    /// even on flats/lakebeds, so flat ground carries cell-scale texture
    /// (`variogram_nugget`). 0 = the historical fully-gated behavior.
    #[serde(default = "d_nugget_amp")]
    pub nugget_amp: f64,

    // --- directional anisotropy (breaks the isotropic grain) ---------------
    /// Anisotropic stretch ratio applied to the noise/warp/detail sample
    /// coordinates (1 = isotropic). Elongates features along `aniso_angle`,
    /// producing the directional ridge-and-valley grain real courses have.
    #[serde(default = "d_aniso_ratio")]
    pub aniso_ratio: f64,
    /// Direction of the anisotropic stretch, radians. A per-seed nuisance
    /// (the `anisotropy_ratio` metric is rotation-invariant).
    #[serde(default = "d_aniso_angle")]
    pub aniso_angle: f64,

    // --- landform organization (2026-07 "dialed in" pass) -------------------
    /// Spatial modulation depth of the detail band (0 = uniform, the historical
    /// behavior). A ~180 m-period field concentrates fine texture into patches
    /// (banks/ravines) instead of uniform stipple — real graded courses carry
    /// their roughness in the top decile of cells (`rough_top10_30`).
    #[serde(default = "d_zero")]
    pub detail_patchiness: f64,
    /// Ridged-blend of the base fBm: `n = (1−|m|)·n + m·(1−2|n|)`. Positive =
    /// ridge creases, negative = valley creases; the creases live on the fBm
    /// zero-contours, so they form CONNECTED networks (the coherent-valley
    /// topology erosion can reinforce but never invent). 0 = off (bit-exact).
    #[serde(default = "d_zero")]
    pub ridged_mix: f64,
    /// Mean-preserving large-scale (~1.1 km) amplitude modulation: within-map
    /// regime variety (a rugged corner, a flat meadow) — targets the
    /// heterogeneity/kurtosis tails. 0 = off (bit-exact).
    #[serde(default = "d_zero")]
    pub relief_patchiness: f64,
}

fn d_detail_amp() -> f64 { 0.25 }
fn d_detail_gain() -> f64 { 0.55 }
fn d_detail_octaves() -> u32 { 2 }
fn d_nugget_amp() -> f64 { 0.0 }
fn d_aniso_ratio() -> f64 { 1.0 }
fn d_aniso_angle() -> f64 { 0.0 }
fn d_zero() -> f64 { 0.0 }

impl Default for TerrainParams {
    fn default() -> Self {
        TerrainParams {
            amplitude: 20.0,
            base_period: 800.0,
            octaves: 5,
            lacunarity: 2.0,
            gain: 0.5,
            warp_amp: 70.0,
            warp_period: 600.0,
            redistribution: 1.0,
            // detail/aniso defaults reproduce the historical look, so the
            // pure-noise terrain golden (which never composes) is unaffected.
            detail_amp: d_detail_amp(),
            detail_gain: d_detail_gain(),
            detail_octaves: d_detail_octaves(),
            nugget_amp: d_nugget_amp(),
            aniso_ratio: d_aniso_ratio(),
            aniso_angle: d_aniso_angle(),
            detail_patchiness: 0.0,
            ridged_mix: 0.0,
            relief_patchiness: 0.0,
        }
    }
}

impl TerrainParams {
    pub fn fbm(&self) -> Fbm {
        Fbm {
            octaves: self.octaves,
            lacunarity: self.lacunarity,
            gain: self.gain,
        }
    }

    /// The fine-detail band fBm (period fixed at `DETAIL_PERIOD`).
    pub fn detail_fbm(&self) -> Fbm {
        Fbm {
            octaves: self.detail_octaves.max(1),
            lacunarity: 2.0,
            gain: self.detail_gain,
        }
    }

    /// Anisotropic stretch of a world point about the origin: rotate by
    /// `aniso_angle`, scale x by `sqrt(ratio)` and y by `1/sqrt(ratio)`
    /// (area-preserving), rotate back. Identity when `aniso_ratio == 1`.
    /// A pure function of world `(x,y)` — keeps compose bit-exact across windows.
    pub fn anisotropic(&self, x: f64, y: f64) -> (f64, f64) {
        if self.aniso_ratio == 1.0 {
            return (x, y);
        }
        let (s, c) = (golf_core::math::sin(self.aniso_angle), golf_core::math::cos(self.aniso_angle));
        let sx = self.aniso_ratio.sqrt();
        let sy = 1.0 / sx;
        // into the stretch frame, scale, back out
        let u = c * x + s * y;
        let v = -s * x + c * y;
        let (u, v) = (u * sx, v * sy);
        (c * u - s * v, s * u + c * v)
    }
}
