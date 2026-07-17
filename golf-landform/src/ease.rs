//! C1 blending/easing helpers — every seam in the primitive library goes
//! through these so composed surfaces stay C1 (the doc's shoulder requirement).
//!
//! All pure arithmetic (no transcendentals) — bit-stable cross-platform.

/// Quintic smoothstep on [e0, e1] (C2). Mirrors `golf_core::math::smoothstep`
/// but kept local so the crate's math surface is self-contained.
#[inline]
pub fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Polynomial smooth minimum with blend half-width `k` (meters of elevation).
/// Exact `min(a, b)` once `|a - b| >= k`; C1 everywhere. k <= 0 = hard min.
#[inline]
pub fn smin(a: f64, b: f64, k: f64) -> f64 {
    if k <= 0.0 {
        return a.min(b);
    }
    let h = (0.5 + 0.5 * (b - a) / k).clamp(0.0, 1.0);
    // lerp(b, a, h) - k*h*(1-h)
    b + (a - b) * h - k * h * (1.0 - h)
}

/// Polynomial smooth maximum (see [`smin`]).
#[inline]
pub fn smax(a: f64, b: f64, k: f64) -> f64 {
    -smin(-a, -b, k)
}

/// C1 "rounded ramp": 0 for `x <= 0`, slope `m` for `x >= r`, quadratic ease
/// over the rounding width `r`. The workhorse for valley walls (floor -> wall)
/// and bluff toes.
#[inline]
pub fn ramp(x: f64, m: f64, r: f64) -> f64 {
    if x <= 0.0 {
        0.0
    } else if r <= 0.0 || x >= r {
        m * (x - 0.5 * r.max(0.0))
    } else {
        // quadratic: slope rises linearly 0 -> m over [0, r]
        0.5 * m * x * x / r
    }
}

/// C1 eased unit step across `[-w/2, +w/2]` (0 below, 1 above): a bluff face.
#[inline]
pub fn eased_step(x: f64, w: f64) -> f64 {
    smoothstep(-0.5 * w, 0.5 * w, x)
}

/// Symmetric C1 window: 1 in the middle, easing to 0 over the terminal
/// `taper` fraction at each end of [0, 1]. Used for bluff height taper and
/// meander endpoint anchoring.
#[inline]
pub fn end_taper(s: f64, taper: f64) -> f64 {
    if taper <= 0.0 {
        return 1.0;
    }
    let t = taper.min(0.5);
    smoothstep(0.0, t, s) * smoothstep(1.0, 1.0 - t, s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smin_exact_outside_band() {
        assert_eq!(smin(0.0, 10.0, 2.0), 0.0);
        assert_eq!(smin(10.0, 0.0, 2.0), 0.0);
        assert!(smin(1.0, 1.1, 2.0) < 1.0); // inside the band: pulled below both
    }

    #[test]
    fn smin_is_c1_across_the_band() {
        // numeric derivative of x -> smin(x, 0, k) has no jump
        let k = 1.0;
        let d = |x: f64| (smin(x + 1e-6, 0.0, k) - smin(x - 1e-6, 0.0, k)) / 2e-6;
        let mut prev = d(-2.0);
        let mut x = -2.0;
        while x < 2.0 {
            x += 0.01;
            let cur = d(x);
            assert!((cur - prev).abs() < 0.05, "derivative jump at {x}");
            prev = cur;
        }
    }

    #[test]
    fn ramp_matches_asymptote_and_is_c1() {
        let (m, r) = (0.8, 4.0);
        assert_eq!(ramp(-1.0, m, r), 0.0);
        // beyond the rounding: exact line with half-r offset
        let x = 10.0;
        assert!((ramp(x, m, r) - m * (x - 0.5 * r)).abs() < 1e-12);
        // slope continuity at x=r
        let d_in = (ramp(r - 1e-7, m, r) - ramp(r - 2e-7, m, r)) / 1e-7;
        let d_out = (ramp(r + 2e-7, m, r) - ramp(r + 1e-7, m, r)) / 1e-7;
        assert!((d_in - d_out).abs() < 1e-4);
    }

    #[test]
    fn end_taper_anchors_ends() {
        assert_eq!(end_taper(0.0, 0.2), 0.0);
        assert_eq!(end_taper(1.0, 0.2), 0.0);
        assert_eq!(end_taper(0.5, 0.2), 1.0);
    }
}
