//! 1D piecewise-linear profiles — width, falloff, and longitudinal elevation
//! profiles alike. Ported from `terrain-v2:golf-core/src/spline.rs`.

use serde::{Deserialize, Serialize};

/// A 1D profile defined by knots `(s, value)` with `s` ascending in [0, 1].
/// Sampled by clamped linear interpolation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Knots sorted by `s` ascending. Must be non-empty.
    pub knots: Vec<(f64, f64)>,
}

impl Profile {
    pub fn constant(v: f64) -> Self {
        Profile {
            knots: vec![(0.0, v)],
        }
    }

    pub fn new(knots: Vec<(f64, f64)>) -> Self {
        assert!(!knots.is_empty(), "profile needs at least one knot");
        debug_assert!(
            knots.windows(2).all(|w| w[0].0 <= w[1].0),
            "profile knots must be ascending in s"
        );
        Profile { knots }
    }

    /// Sample at `s`, clamping outside the knot range.
    pub fn sample(&self, s: f64) -> f64 {
        let k = &self.knots;
        if s <= k[0].0 {
            return k[0].1;
        }
        let last = k[k.len() - 1];
        if s >= last.0 {
            return last.1;
        }
        // Find the bracketing segment (linear scan; profiles are tiny).
        for w in k.windows(2) {
            let (s0, v0) = w[0];
            let (s1, v1) = w[1];
            if s <= s1 {
                let t = if s1 == s0 { 0.0 } else { (s - s0) / (s1 - s0) };
                return v0 + (v1 - v0) * t;
            }
        }
        last.1
    }

    /// Largest knot value (bbox padding for swept primitives).
    pub fn max_value(&self) -> f64 {
        self.knots.iter().map(|k| k.1).fold(f64::NEG_INFINITY, f64::max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_interpolates_and_clamps() {
        let p = Profile::new(vec![(0.0, 0.0), (0.5, 10.0), (1.0, 0.0)]);
        assert_eq!(p.sample(-1.0), 0.0);
        assert_eq!(p.sample(0.25), 5.0);
        assert_eq!(p.sample(0.5), 10.0);
        assert_eq!(p.sample(0.75), 5.0);
        assert_eq!(p.sample(2.0), 0.0);
    }

    #[test]
    fn constant_is_flat() {
        let p = Profile::constant(3.0);
        assert_eq!(p.sample(0.0), 3.0);
        assert_eq!(p.sample(0.7), 3.0);
        assert_eq!(p.max_value(), 3.0);
    }
}
