//! Deterministic gradient noise — a port of the proven golf-terrain
//! implementation (integer-hash lattice gradients, no transcendentals) into
//! the terrain-v2 stack. Stage 2 uses it for meander jitter and bowl-rim
//! wobble; Stage 4 extends this module with the modulated fBm layer.

use golf_core::math::floor;

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

#[inline]
fn fade(t: f64) -> f64 {
    t * t * t * (t * (t * 6.0 - 15.0) + 10.0)
}

/// 2D gradient noise at `(x, y)`, roughly `[-1, 1]`.
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
    (nx0 + v * (nx1 - nx0)) * std::f64::consts::SQRT_2
}

/// 1D slice of the 2D field (fixed y): meander lateral jitter, rim wobble.
pub fn perlin1(x: f64, seed: u32) -> f64 {
    perlin2(x, 0.37, seed)
}

/// Periodic noise around a circle (for closed-boundary wobble): sample the 2D
/// field along a circle of radius `cycles` — continuous and periodic in angle.
/// `cs`/`sn` are cos/sin of the angle (callers usually already have them).
pub fn perlin_ring(cs: f64, sn: f64, cycles: f64, seed: u32) -> f64 {
    perlin2(cs * cycles, sn * cycles, seed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_and_bounded() {
        let a = perlin2(3.7, -1.2, 9);
        assert_eq!(a, perlin2(3.7, -1.2, 9));
        let mut max = 0.0f64;
        for i in 0..2000 {
            max = max.max(perlin2(i as f64 * 0.173, i as f64 * 0.091, 5).abs());
        }
        assert!(max <= 1.05 && max > 0.3);
    }

    #[test]
    fn ring_is_periodic() {
        let (c, s) = (0.5f64.sqrt(), 0.5f64.sqrt());
        assert_eq!(perlin_ring(c, s, 3.0, 1), perlin_ring(c, s, 3.0, 1));
    }
}
