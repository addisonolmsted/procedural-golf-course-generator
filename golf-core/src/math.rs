//! Deterministic f64 math foundation.
//!
//! Every transcendental used in the generation path is routed through `libm`
//! (a portable, pure-Rust implementation) rather than `std`, because std's
//! `sin`/`cos`/`exp`/`pow` can differ bit-for-bit across platforms and would
//! break the cross-hardware determinism the cache relies on. `sqrt` is exempt:
//! IEEE-754 requires it to be correctly rounded, so it is deterministic already.

use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, Sub};

pub fn sin(x: f64) -> f64 {
    libm::sin(x)
}
pub fn cos(x: f64) -> f64 {
    libm::cos(x)
}
pub fn exp(x: f64) -> f64 {
    libm::exp(x)
}
pub fn pow(x: f64, y: f64) -> f64 {
    libm::pow(x, y)
}
pub fn ln(x: f64) -> f64 {
    libm::log(x)
}
pub fn floor(x: f64) -> f64 {
    libm::floor(x)
}
pub fn hypot(x: f64, y: f64) -> f64 {
    libm::hypot(x, y)
}
pub fn acos(x: f64) -> f64 {
    libm::acos(x)
}
pub fn atan2(y: f64, x: f64) -> f64 {
    libm::atan2(y, x)
}

pub fn erf(x: f64) -> f64 {
    libm::erf(x)
}

/// Clamp to [0, 1].
pub fn clamp01(x: f64) -> f64 {
    x.clamp(0.0, 1.0)
}

/// Hermite smoothstep from `edge0`->`edge1`, clamped. Returns 0..1.
pub fn smoothstep(edge0: f64, edge1: f64, x: f64) -> f64 {
    if edge0 == edge1 {
        return if x < edge0 { 0.0 } else { 1.0 };
    }
    let t = clamp01((x - edge0) / (edge1 - edge0));
    t * t * (3.0 - 2.0 * t)
}

/// A 2D point/vector in world XY, always f64. This is the only vector type used
/// in hashed generation paths; render-side f32 (`glam`) never crosses into here.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

impl Vec2 {
    pub const ZERO: Vec2 = Vec2 { x: 0.0, y: 0.0 };

    pub const fn new(x: f64, y: f64) -> Self {
        Vec2 { x, y }
    }

    pub fn dot(self, o: Vec2) -> f64 {
        self.x * o.x + self.y * o.y
    }

    /// Euclidean length. Uses `hypot` (correctly-rounded, deterministic).
    pub fn length(self) -> f64 {
        hypot(self.x, self.y)
    }

    pub fn distance(self, o: Vec2) -> f64 {
        (self - o).length()
    }

    /// Unit vector; returns ZERO for a zero-length input (deterministic guard).
    pub fn normalized(self) -> Vec2 {
        let len = self.length();
        if len == 0.0 {
            Vec2::ZERO
        } else {
            Vec2::new(self.x / len, self.y / len)
        }
    }

    /// Left-hand perpendicular (rotate +90°). Used to build corridor normals.
    pub fn perp(self) -> Vec2 {
        Vec2::new(-self.y, self.x)
    }

    pub fn lerp(self, o: Vec2, t: f64) -> Vec2 {
        Vec2::new(self.x + (o.x - self.x) * t, self.y + (o.y - self.y) * t)
    }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x + o.x, self.y + o.y)
    }
}
impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        Vec2::new(self.x - o.x, self.y - o.y)
    }
}
impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, s: f64) -> Vec2 {
        Vec2::new(self.x * s, self.y * s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perp_is_orthogonal() {
        let v = Vec2::new(3.0, 4.0);
        assert_eq!(v.dot(v.perp()), 0.0);
        assert_eq!(v.length(), 5.0);
    }

    #[test]
    fn normalized_zero_is_zero() {
        assert_eq!(Vec2::ZERO.normalized(), Vec2::ZERO);
    }

    #[test]
    fn lerp_endpoints() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(5.0, -2.0);
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
    }
}
