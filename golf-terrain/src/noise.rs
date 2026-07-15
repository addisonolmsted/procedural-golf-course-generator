//! Deterministic gradient (Perlin-style) noise and fBm.
//!
//! Cross-platform stable by construction: lattice gradients come from an integer
//! hash (only wrapping integer ops), and evaluation uses only +, -, * on f64
//! plus `floor` — no transcendentals, whose results vary between platforms. This
//! is what lets a `(seed, params)` pair produce a bit-identical field anywhere.

use golf_core::math::floor;

/// 8 gradient directions (axis-aligned + normalized diagonals).
const GRADS: [(f64, f64); 8] = [
    (1.0, 0.0),
    (-1.0, 0.0),
    (0.0, 1.0),
    (0.0, -1.0),
    (0.707_106_781_186_547_5, 0.707_106_781_186_547_5),
    (-0.707_106_781_186_547_5, 0.707_106_781_186_547_5),
    (0.707_106_781_186_547_5, -0.707_106_781_186_547_5),
    (-0.707_106_781_186_547_5, -0.707_106_781_186_547_5),
];

/// Integer hash of a lattice cell + seed -> pseudo-random u32.
#[inline]
fn hash2(ix: i32, iy: i32, seed: u32) -> u32 {
    let mut h = seed ^ 0x9E37_79B9;
    h = h.wrapping_add((ix as u32).wrapping_mul(0x85EB_CA6B));
    h ^= h >> 15;
    h = h.wrapping_add((iy as u32).wrapping_mul(0xC2B2_AE35));
    h ^= h >> 13;
    h = h.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 16;
    h
}

#[inline]
fn grad_dot(ix: i32, iy: i32, seed: u32, dx: f64, dy: f64) -> f64 {
    let g = GRADS[(hash2(ix, iy, seed) & 7) as usize];
    g.0 * dx + g.1 * dy
}

/// Quintic fade curve.
#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// 2D gradient noise at `(x, y)`, scaled to roughly `[-1, 1]`.
pub fn perlin2(x: f64, y: f64, seed: u32) -> f64 {
    let x0 = floor(x) as i32;
    let y0 = floor(y) as i32;
    let xf = x - x0 as f64;
    let yf = y - y0 as f64;
    let u = fade(xf);
    let v = fade(yf);

    let n00 = grad_dot(x0, y0, seed, xf, yf);
    let n10 = grad_dot(x0 + 1, y0, seed, xf - 1.0, yf);
    let n01 = grad_dot(x0, y0 + 1, seed, xf, yf - 1.0);
    let n11 = grad_dot(x0 + 1, y0 + 1, seed, xf - 1.0, yf - 1.0);

    let nx0 = n00 + u * (n10 - n00);
    let nx1 = n01 + u * (n11 - n01);
    let n = nx0 + v * (nx1 - nx0);

    // Raw range is about ±0.707; normalize toward ±1.
    n * std::f64::consts::SQRT_2
}

/// Fractional Brownian motion parameters (octave stacking).
#[derive(Clone, Copy, Debug)]
pub struct Fbm {
    pub octaves: u32,
    pub lacunarity: f64,
    pub gain: f64,
}

/// Summed-octave fBm at `(x, y)`, amplitude-normalized to about `[-1, 1]`.
/// Each octave uses a decorrelated seed so features don't align.
pub fn fbm(x: f64, y: f64, seed: u32, f: &Fbm) -> f64 {
    let mut amp = 1.0;
    let mut freq = 1.0;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..f.octaves {
        let s = seed.wrapping_add(o.wrapping_mul(0x9E37_79B9));
        sum += amp * perlin2(x * freq, y * freq, s);
        norm += amp;
        amp *= f.gain;
        freq *= f.lacunarity;
    }
    if norm > 0.0 {
        sum / norm
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perlin_is_deterministic() {
        for &(x, y) in &[(1.3, 2.7), (-4.1, 0.9), (100.5, -50.25)] {
            assert_eq!(perlin2(x, y, 42), perlin2(x, y, 42));
        }
    }

    #[test]
    fn perlin_zero_at_lattice_points() {
        // Gradient noise is exactly 0 at integer lattice points.
        for iy in -3..3 {
            for ix in -3..3 {
                assert!(perlin2(ix as f64, iy as f64, 7).abs() < 1e-12);
            }
        }
    }

    #[test]
    fn perlin_roughly_in_range() {
        let mut max = 0.0f64;
        let mut s = 0u32;
        for i in 0..5000 {
            let x = (i as f64) * 0.137;
            let y = (i as f64) * 0.0913;
            let n = perlin2(x, y, 1);
            max = max.max(n.abs());
            s = s.wrapping_add(1);
        }
        assert!(max <= 1.05, "normalized perlin should stay near [-1,1], got {max}");
        assert!(max > 0.3, "should exercise a real range");
    }

    #[test]
    fn fbm_deterministic_and_bounded() {
        let f = Fbm { octaves: 5, lacunarity: 2.0, gain: 0.5 };
        let a = fbm(3.2, 1.1, 99, &f);
        let b = fbm(3.2, 1.1, 99, &f);
        assert_eq!(a, b);
        assert!(a.abs() <= 1.05);
    }
}
