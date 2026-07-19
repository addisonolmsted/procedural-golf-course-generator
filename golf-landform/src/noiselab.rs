//! Noise LAB — the Stage-1 identifiability sandbox and Stage-4 prototype.
//!
//! NOT part of the MacroConfig schema: this module exists so the Stage-1 study
//! can measure which noise parameters are influential/identifiable BEFORE
//! Stage 4 commits to a parameter set. The composition ports the proven
//! v1 golf-terrain structure (fBm + domain warp + signed ridged/billow mix +
//! redistribution) onto golf-landform's hash-lattice perlin, adds a separate
//! fine-texture band, anisotropy, and the skeleton-driven modulation contract
//! (floor_damp / slope_gain / grain_align against a Stage-2 primitive
//! skeleton's low-pass fields).
//!
//! Deterministic: sub-seeds via splitmix-style channel mixing of the item
//! seed; pure f64 + libm; same config → identical grid on any platform.

use golf_core::grid::{Grid, GridSpec};
use golf_core::math::{cos, sin, Vec2};
use serde::{Deserialize, Serialize};

use crate::noise::perlin2;
use crate::{resolve, MacroConfig};

fn d_zero() -> f64 { 0.0 }
fn d_one() -> f64 { 1.0 }
fn d_lac() -> f64 { 2.0 }

/// The broad Stage-1 candidate parameter set (19 assessed dims + seed).
/// `dummy` is deliberately inert — the sensitivity noise floor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct NoiseLabConfig {
    pub seed: u64,
    // base band
    pub base_amp: f64,
    pub base_wavelength: f64,
    pub octaves: u32,
    pub gain: f64,
    #[serde(default = "d_lac")]
    pub lacunarity: f64,
    // domain warp (two-pass)
    #[serde(default = "d_zero")]
    pub warp_amp: f64,
    #[serde(default = "d_one")]
    pub warp_wavelength: f64,
    #[serde(default = "d_zero")]
    pub warp2_amp: f64,
    // shape
    #[serde(default = "d_zero")]
    pub ridged_mix: f64,          // signed: + ridge creases, - billow
    #[serde(default = "d_one")]
    pub redistribution: f64,
    // fine texture band
    #[serde(default = "d_zero")]
    pub tex_amp: f64,
    #[serde(default = "d_one")]
    pub tex_wavelength: f64,
    #[serde(default = "d_zero")]
    pub tex_gain: f64,
    #[serde(default = "d_zero")]
    pub nugget_amp: f64,
    // anisotropy
    #[serde(default = "d_one")]
    pub aniso_ratio: f64,
    // modulation contract (skeleton-driven; inert without a skeleton)
    #[serde(default = "d_one")]
    pub floor_damp: f64,          // multiplier at strongly-negative TPI
    #[serde(default = "d_zero")]
    pub slope_gain: f64,          // amplitude exponent vs macro slope
    #[serde(default = "d_zero")]
    pub grain_align: f64,         // 0..1: warp direction pulled along contour
    // inert
    #[serde(default = "d_zero")]
    pub dummy: f64,
}

#[inline]
fn mixseed(seed: u64, ch: u32) -> u32 {
    let mut z = seed ^ (0x9E37_79B9_7F4A_7C15u64.wrapping_mul(ch as u64 + 1));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z ^ (z >> 31)) as u32
}

fn fbm(x: f64, y: f64, seed: u32, octaves: u32, lac: f64, gain: f64) -> f64 {
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..octaves.max(1) {
        let s = seed.wrapping_add(o.wrapping_mul(0x9E37_79B9));
        sum += amp * perlin2(x * freq, y * freq, s);
        norm += amp;
        amp *= gain;
        freq *= lac;
    }
    if norm > 0.0 { sum / norm } else { 0.0 }
}

/// Precomputed skeleton fields (low-pass ~ the S3 macro predictors).
pub struct SkeletonFields {
    pub z: Grid<f64>,
    pub slope: Grid<f64>,
    pub tpi: Grid<f64>,
    pub aspect: Grid<f64>,
    pub tpi_sigma: f64,
}

/// Build skeleton + smoothed predictor fields at `res_m` (box-blur chain
/// approximates the Stage-1 Gaussian; exactness is irrelevant here — the
/// sandbox only needs a plausible, deterministic macro conditioning field).
pub fn skeleton_fields(cfg: &MacroConfig, res_m: f64) -> SkeletonFields {
    let r = resolve(cfg);
    let n = (cfg.extent_m / res_m).round().max(2.0) as u32;
    let spec = GridSpec::new(Vec2::new(res_m / 2.0, res_m / 2.0), res_m, n, n);
    let mut z = Grid::filled(spec, 0.0f64);
    for y in 0..n {
        for x in 0..n {
            z.set(x, y, r.height_at(spec.world_of(x, y)));
        }
    }
    // three box blurs ~ Gaussian; radius for L=200 m half-amplitude
    let lp = box_blur3(&z, (200.0 * 0.1874 / res_m).round().max(1.0) as i32);
    let mut slope = Grid::filled(spec, 0.0f64);
    let mut aspect = Grid::filled(spec, 0.0f64);
    let cell = res_m;
    for y in 0..n {
        for x in 0..n {
            let xm = lp.get(x.saturating_sub(1), y);
            let xp = lp.get((x + 1).min(n - 1), y);
            let ym = lp.get(x, y.saturating_sub(1));
            let yp = lp.get(x, (y + 1).min(n - 1));
            let gx = (xp - xm) / (2.0 * cell);
            let gy = (yp - ym) / (2.0 * cell);
            slope.set(x, y, (gx * gx + gy * gy).sqrt());
            aspect.set(x, y, golf_core::math::atan2(gy, gx));
        }
    }
    let tpi_lp = box_blur3(&lp, (300.0 / res_m / 2.0).round().max(1.0) as i32);
    let mut tpi = Grid::filled(spec, 0.0f64);
    let mut sum = 0.0;
    let mut sum2 = 0.0;
    for i in 0..lp.data.len() {
        let v = lp.data[i] - tpi_lp.data[i];
        tpi.data[i] = v;
        sum += v;
        sum2 += v * v;
    }
    let nn = lp.data.len() as f64;
    let var = (sum2 / nn - (sum / nn) * (sum / nn)).max(0.0);
    SkeletonFields { z, slope, tpi, aspect, tpi_sigma: var.sqrt() }
}

fn box_blur3(g: &Grid<f64>, r: i32) -> Grid<f64> {
    let mut out = g.clone();
    for _ in 0..3 {
        out = box_blur1(&out, r);
    }
    out
}

fn box_blur1(g: &Grid<f64>, r: i32) -> Grid<f64> {
    let (nx, ny) = (g.spec.nx as i32, g.spec.ny as i32);
    let mut tmp = g.clone();
    // horizontal
    for y in 0..ny {
        let mut acc = 0.0;
        let mut cnt = 0.0;
        for x in -r..=r {
            if x >= 0 && x < nx {
                acc += g.get(x as u32, y as u32);
                cnt += 1.0;
            }
        }
        for x in 0..nx {
            tmp.set(x as u32, y as u32, acc / cnt);
            let add = x + r + 1;
            let rem = x - r;
            if add < nx {
                acc += g.get(add as u32, y as u32);
                cnt += 1.0;
            }
            if rem >= 0 {
                acc -= g.get(rem as u32, y as u32);
                cnt -= 1.0;
            }
        }
    }
    // vertical
    let mut out = tmp.clone();
    for x in 0..nx {
        let mut acc = 0.0;
        let mut cnt = 0.0;
        for y in -r..=r {
            if y >= 0 && y < ny {
                acc += tmp.get(x as u32, y as u32);
                cnt += 1.0;
            }
        }
        for y in 0..ny {
            out.set(x as u32, y as u32, acc / cnt);
            let add = y + r + 1;
            let rem = y - r;
            if add < ny {
                acc += tmp.get(x as u32, add as u32);
                cnt += 1.0;
            }
            if rem >= 0 {
                acc -= tmp.get(x as u32, rem as u32);
                cnt -= 1.0;
            }
        }
    }
    out
}

/// Generate the lab noise field (optionally composed over a skeleton) at
/// `res_m` on an `extent_m` square.
pub fn generate_noise(cfg: &NoiseLabConfig, extent_m: f64,
                      skeleton: Option<&SkeletonFields>, res_m: f64) -> Grid<f64> {
    let n = (extent_m / res_m).round().max(2.0) as u32;
    let spec = GridSpec::new(Vec2::new(res_m / 2.0, res_m / 2.0), res_m, n, n);
    let s_base = mixseed(cfg.seed, 0);
    let s_wx = mixseed(cfg.seed, 1);
    let s_wy = mixseed(cfg.seed, 2);
    let s_w2x = mixseed(cfg.seed, 3);
    let s_w2y = mixseed(cfg.seed, 4);
    let s_tex = mixseed(cfg.seed, 5);
    let s_nug = mixseed(cfg.seed, 6);
    // nuisance anisotropy angle from the seed
    let ang = (mixseed(cfg.seed, 7) as f64 / u32::MAX as f64) * std::f64::consts::PI;
    let (sa, ca) = (sin(ang), cos(ang));
    let sqrt_r = cfg.aniso_ratio.max(1e-6).sqrt();

    let bw = cfg.base_wavelength.max(1.0);
    let ww = cfg.warp_wavelength.max(1.0);
    let tw = cfg.tex_wavelength.max(0.5);
    let redistribute = cfg.redistribution != 1.0;
    let v_median = if cfg.redistribution >= 1.0 {
        golf_core::math::pow(0.5, cfg.redistribution)
    } else {
        1.0 - golf_core::math::pow(0.5, 1.0 / cfg.redistribution)
    };

    let mut g = Grid::filled(spec, 0.0f64);
    for yy in 0..n {
        for xx in 0..n {
            let p = spec.world_of(xx, yy);
            // anisotropic frame (area-preserving stretch about the seed angle)
            let u = ca * p.x + sa * p.y;
            let v = -sa * p.x + ca * p.y;
            let (u, v) = (u * sqrt_r, v / sqrt_r);
            let (ax, ay) = (ca * u - sa * v, sa * u + ca * v);

            // grain alignment: pull the warp displacement toward the skeleton
            // contour direction (perpendicular to aspect)
            let mut wx = 0.0;
            let mut wy = 0.0;
            if cfg.warp_amp != 0.0 {
                wx = cfg.warp_amp * perlin2(ax / ww, ay / ww, s_wx);
                wy = cfg.warp_amp * perlin2(ax / ww, ay / ww, s_wy);
            }
            if cfg.warp2_amp != 0.0 {
                let w2 = ww / 4.0;
                wx += cfg.warp2_amp * perlin2(ax / w2, ay / w2, s_w2x);
                wy += cfg.warp2_amp * perlin2(ax / w2, ay / w2, s_w2y);
            }
            if let (Some(sk), true) = (skeleton, cfg.grain_align > 0.0) {
                let a = sk.aspect.bilinear(p);
                let (cs, sn) = (cos(a), sin(a));
                // contour direction = perpendicular to the gradient
                let (tx, ty) = (-sn, cs);
                let along = wx * tx + wy * ty;
                let k = cfg.grain_align.clamp(0.0, 1.0);
                wx = wx * (1.0 - k) + along * tx * k;
                wy = wy * (1.0 - k) + along * ty * k;
            }

            let mut nb = fbm((ax + wx) / bw, (ay + wy) / bw, s_base,
                             cfg.octaves, cfg.lacunarity, cfg.gain);
            if cfg.ridged_mix != 0.0 {
                let m = cfg.ridged_mix.clamp(-1.0, 1.0);
                nb = (1.0 - m.abs()) * nb + m * (1.0 - 2.0 * nb.abs());
            }
            if redistribute {
                let uu = ((nb + 1.0) * 0.5).clamp(0.0, 1.0);
                let pp = cfg.redistribution;
                let vv = if pp >= 1.0 {
                    golf_core::math::pow(uu, pp)
                } else {
                    1.0 - golf_core::math::pow(1.0 - uu, 1.0 / pp)
                };
                nb = 2.0 * (vv - v_median);
            }

            let mut amp_mod = 1.0;
            let mut z0 = 0.0;
            if let Some(sk) = skeleton {
                z0 = sk.z.bilinear(p);
                let slope = sk.slope.bilinear(p);
                // slope_gain: amplitude scales with macro slope
                amp_mod *= golf_core::math::pow(1.0 + slope / 0.05, cfg.slope_gain);
                // floor_damp: smooth ramp over TPI in [-1.5, -0.5] sigma
                if sk.tpi_sigma > 0.0 {
                    let t = sk.tpi.bilinear(p) / sk.tpi_sigma;
                    let w = smooth01((-0.5 - t) / 1.0); // 1 deep in floors
                    amp_mod *= 1.0 + (cfg.floor_damp - 1.0) * w;
                }
            }

            let tex = if cfg.tex_amp != 0.0 {
                cfg.tex_amp * fbm(ax / tw, ay / tw, s_tex, 4, 2.0,
                                  cfg.tex_gain.max(0.05))
            } else {
                0.0
            };
            let nug = if cfg.nugget_amp != 0.0 {
                cfg.nugget_amp * perlin2(p.x / (2.0 * res_m), p.y / (2.0 * res_m), s_nug)
            } else {
                0.0
            };
            g.set(xx, yy, z0 + amp_mod * (cfg.base_amp * nb + tex + nug));
        }
    }
    g
}

#[inline]
fn smooth01(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> NoiseLabConfig {
        NoiseLabConfig {
            seed: 7,
            base_amp: 12.0,
            base_wavelength: 300.0,
            octaves: 5,
            gain: 0.5,
            lacunarity: 2.0,
            warp_amp: 60.0,
            warp_wavelength: 400.0,
            warp2_amp: 20.0,
            ridged_mix: 0.3,
            redistribution: 1.4,
            tex_amp: 0.5,
            tex_wavelength: 20.0,
            tex_gain: 0.55,
            nugget_amp: 0.05,
            aniso_ratio: 1.8,
            floor_damp: 0.3,
            slope_gain: 0.4,
            grain_align: 0.5,
            dummy: 0.7,
        }
    }

    #[test]
    fn deterministic() {
        let a = generate_noise(&cfg(), 512.0, None, 4.0);
        let b = generate_noise(&cfg(), 512.0, None, 4.0);
        assert_eq!(crate::field_hash(&a), crate::field_hash(&b));
    }

    #[test]
    fn dummy_is_inert() {
        let mut c2 = cfg();
        c2.dummy = 0.123;
        let a = generate_noise(&cfg(), 512.0, None, 4.0);
        let b = generate_noise(&c2, 512.0, None, 4.0);
        assert_eq!(crate::field_hash(&a), crate::field_hash(&b));
    }

    #[test]
    fn modulation_params_inert_without_skeleton() {
        let mut c2 = cfg();
        c2.floor_damp = 1.0;
        c2.slope_gain = 0.0;
        c2.grain_align = 0.0;
        let a = generate_noise(&cfg(), 512.0, None, 4.0);
        let b = generate_noise(&c2, 512.0, None, 4.0);
        assert_eq!(crate::field_hash(&a), crate::field_hash(&b));
    }

    #[test]
    fn floor_damp_reduces_floor_texture_on_skeleton() {
        let skel_cfg = crate::preset("river_confluence").unwrap();
        let ext = skel_cfg.extent_m; // noise grid must index-align with the skeleton
        let sk = skeleton_fields(&skel_cfg, 10.0);
        let mut damped = cfg();
        damped.floor_damp = 0.1;
        let mut full = cfg();
        full.floor_damp = 1.0;
        let a = generate_noise(&damped, ext, Some(&sk), 10.0);
        let b = generate_noise(&full, ext, Some(&sk), 10.0);
        // residual variance in strong floors must shrink under damping
        let mut va = 0.0;
        let mut vb = 0.0;
        let mut cnt = 0.0;
        for i in 0..a.data.len() {
            if sk.tpi.data[i] < -1.0 * sk.tpi_sigma {
                let da = a.data[i] - sk.z.data[i];
                let db = b.data[i] - sk.z.data[i];
                va += da * da;
                vb += db * db;
                cnt += 1.0;
            }
        }
        assert!(cnt > 100.0, "need floor cells");
        assert!(va / cnt < 0.35 * (vb / cnt),
                "floor texture not damped: {} vs {}", va / cnt, vb / cnt);
    }
}
