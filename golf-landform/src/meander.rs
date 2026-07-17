//! Meander generator: trend line between two endpoints + lateral displacement
//! (sine + low-frequency noise), bounded by a hard curvature clamp.
//!
//! Physical anchoring: wavelength ~ `wavelength_mult` x channel/valley width
//! (10-14x, river-morphology rule of thumb). Amplitude is driven by
//! `intensity` in [0, 1] toward the LARGEST amplitude the curvature clamp
//! allows (min radius >= `MIN_RADIUS_WIDTHS` x total width) — for a sine,
//! R_min = wavelength^2 / (4 pi^2 A), so A_max = wavelength^2 / (4 pi^2 R_min).
//! (The plan doc's "A ~ 2-3 x wavelength at intensity 1" cannot satisfy its own
//! curvature clamp — the clamp is the authority here; at wavelength = 12 x W
//! that yields A_max ~ 1.5 x W, i.e. sinuous but never self-crossing.)
//!
//! Self-approach rejection: points far apart along the arc must stay at least
//! a valley width apart in the plane; violators regenerate with a new phase
//! (bounded, deterministic), backing intensity off as a last resort.

use golf_core::det::DetRng;
use golf_core::math::{sin, Vec2};
use serde::{Deserialize, Serialize};

use crate::ease::end_taper;
use crate::noise::perlin1;
use crate::spline::{catmull_rom, Spine};

/// Hard clamp: minimum bend radius as a multiple of the total width.
pub const MIN_RADIUS_WIDTHS: f64 = 2.5;
/// Self-approach: minimum planar separation (in widths) for points more than
/// half a wavelength apart along the arc.
const MIN_SEPARATION_WIDTHS: f64 = 1.2;
const MAX_ATTEMPTS: usize = 8;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MeanderSpec {
    /// Trend-line endpoints (usually on the window boundary).
    pub entry: Vec2,
    pub exit: Vec2,
    /// Total feature width (valley top width for valleys, channel width for
    /// rivers) — sets the wavelength and the curvature clamp.
    pub width_m: f64,
    /// 0 = straight, 1 = as sinuous as the curvature clamp allows.
    pub intensity: f64,
    /// Wavelength as a multiple of `width_m` (river rule of thumb: 10-14).
    pub wavelength_mult: f64,
    /// Fraction of the sine amplitude added as low-frequency noise (0-0.5).
    pub jitter: f64,
    pub seed: u64,
}

impl MeanderSpec {
    pub fn wavelength(&self) -> f64 {
        (self.wavelength_mult.max(4.0)) * self.width_m.max(1.0)
    }

    /// Largest sine amplitude satisfying the curvature clamp.
    pub fn amplitude_max(&self) -> f64 {
        let lam = self.wavelength();
        let rmin = MIN_RADIUS_WIDTHS * self.width_m;
        lam * lam / (4.0 * std::f64::consts::PI * std::f64::consts::PI * rmin)
    }
}

/// Generate the meander centerline as a smooth arc-length polyline.
/// Deterministic in the spec (incl. its seed).
pub fn generate(spec: &MeanderSpec) -> Spine {
    let trend = spec.exit - spec.entry;
    let len = trend.length().max(1.0);
    let dir = trend.normalized();
    let perp = dir.perp();
    let lam = spec.wavelength();
    let a_clamp = spec.amplitude_max();
    let mut rng = DetRng::new(spec.seed, b"landform-meander-v1");

    let mut intensity = spec.intensity.clamp(0.0, 1.0);
    for attempt in 0..MAX_ATTEMPTS {
        let phase = rng.range_f64(0.0, 2.0 * std::f64::consts::PI);
        let nseed = rng.next_u32();
        let amp = intensity * a_clamp;
        // control points every ~lam/8 along the trend
        let n_ctrl = ((len / (lam / 8.0)).ceil() as usize).clamp(4, 400);
        let mut ctrl: Vec<Vec2> = Vec::with_capacity(n_ctrl + 1);
        for k in 0..=n_ctrl {
            let t = k as f64 / n_ctrl as f64;
            let arc = t * len;
            let lat = amp
                * (sin(2.0 * std::f64::consts::PI * arc / lam + phase)
                    + spec.jitter.clamp(0.0, 0.5)
                        * perlin1(arc / (0.71 * lam), nseed))
                * end_taper(t, 0.10);
            ctrl.push(spec.entry + dir * arc + perp * lat);
        }
        let spine = Spine::new(catmull_rom(&ctrl, (lam / 24.0).clamp(4.0, 30.0)));

        if passes(&spine, spec, lam) {
            return spine;
        }
        // new phase for the next two tries, then start backing intensity off
        if attempt >= 1 {
            intensity *= 0.8;
        }
    }
    // final fallback: straight line (always valid)
    Spine::new(catmull_rom(&[spec.entry, spec.exit], 20.0))
}

/// Curvature clamp + self-approach acceptance.
fn passes(spine: &Spine, spec: &MeanderSpec, lam: f64) -> bool {
    // 5% slack: the discrete circumradius of a sampled smooth curve
    // under-reports slightly at the apex
    if spine.min_curvature_radius() < 0.95 * MIN_RADIUS_WIDTHS * spec.width_m {
        return false;
    }
    // self-approach: compare all sample pairs more than lam/2 apart along arc
    let total = spine.length();
    let min_sep = MIN_SEPARATION_WIDTHS * spec.width_m;
    let pts = &spine.pts;
    let step = total / (pts.len() - 1) as f64;
    let gap = ((0.5 * lam) / step.max(1e-9)).ceil() as usize;
    for i in 0..pts.len() {
        for j in (i + gap.max(1))..pts.len() {
            if pts[i].distance(pts[j]) < min_sep {
                return false;
            }
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(intensity: f64, seed: u64) -> MeanderSpec {
        MeanderSpec {
            entry: Vec2::new(0.0, 1250.0),
            exit: Vec2::new(2500.0, 1250.0),
            width_m: 60.0,
            intensity,
            wavelength_mult: 12.0,
            jitter: 0.3,
            seed,
        }
    }

    #[test]
    fn deterministic() {
        let a = generate(&spec(0.9, 7));
        let b = generate(&spec(0.9, 7));
        assert_eq!(a.pts, b.pts);
    }

    #[test]
    fn respects_curvature_clamp_at_max_intensity() {
        for seed in 0..20 {
            let s = spec(1.0, seed);
            let spine = generate(&s);
            assert!(
                spine.min_curvature_radius() >= 0.95 * MIN_RADIUS_WIDTHS * s.width_m,
                "seed {seed}: radius {}",
                spine.min_curvature_radius()
            );
        }
    }

    #[test]
    fn straight_at_zero_intensity() {
        let spine = generate(&spec(0.0, 3));
        for p in &spine.pts {
            assert!((p.y - 1250.0).abs() < 1.0);
        }
    }

    #[test]
    fn endpoints_anchored() {
        let s = spec(1.0, 11);
        let spine = generate(&s);
        assert!(spine.pts[0].distance(s.entry) < 2.0);
        assert!(spine.pts.last().unwrap().distance(s.exit) < 2.0);
    }
}
