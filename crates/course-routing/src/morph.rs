//! Geomorphons and the morphology stack — port of
//! `tools/dtm_primitives/dtm_primitives/geomorphons.py` (`classify`,
//! `collar_px`, `saddle_axes`) and `siting.build_morphology`.
//!
//! Geomorphons (Jasiewicz & Stepinski 2013): 10-class landform
//! classification by 8-direction line-of-sight ternary patterns. Per
//! direction: zenith angle A = max elevation angle to any cell within the
//! lookup distance, nadir angle B = max depression angle. Ternary element:
//! +1 if B - A > t (land falls away — cell locally HIGH toward that
//! direction), -1 if A - B > t (land rises — cell locally LOW), else 0.
//! The (n_plus, n_minus) counts index the class lookup table. Classes:
//! 1 FL flat, 2 PK peak, 3 RI ridge, 4 SH shoulder, 5 SP spur, 6 SL slope,
//! 7 HL hollow, 8 FS footslope, 9 VL valley, 10 PT pit.

use std::f64::consts::PI;

use crate::fields::py_round;
use crate::{Fields, Morphology};

/// `lookup[n_plus][n_minus]` — Jasiewicz & Stepinski Table 1 orientation:
/// n_plus = directions where the cell is locally HIGH (land falls away),
/// n_minus = directions where the cell is locally LOW (land rises).
/// Verified by the analytic tests (cone->peak, pit, tent->ridge, V->valley).
pub const LOOKUP: [[u8; 9]; 9] = [
    // n_minus:  0   1   2   3   4   5   6   7   8      n_plus:
    [1, 1, 1, 8, 8, 9, 9, 9, 10],   // 0
    [1, 1, 8, 8, 8, 9, 9, 9, 0],    // 1
    [1, 4, 6, 6, 7, 7, 9, 0, 0],    // 2
    [4, 4, 6, 6, 6, 7, 0, 0, 0],    // 3
    [4, 4, 5, 6, 6, 0, 0, 0, 0],    // 4
    [3, 3, 5, 5, 0, 0, 0, 0, 0],    // 5
    [3, 3, 3, 0, 0, 0, 0, 0, 0],    // 6
    [3, 3, 0, 0, 0, 0, 0, 0, 0],    // 7
    [2, 0, 0, 0, 0, 0, 0, 0, 0],    // 8
];

/// `_DIRS`, `(dy, dx)` in array rows; bit k of the ternary masks.
pub const DIRS: [(i64, i64); 8] =
    [(-1, 0), (-1, 1), (0, 1), (1, 1), (1, 0), (1, -1), (0, -1), (-1, -1)];

/// Direction angles for `DIRS[k]`, radians: our grids are row 0 = SOUTH
/// (origin SW), so bearing = `atan2(dy, dx)` (`geomorphons.DIR_ANGLES`).
pub fn dir_angle(k: usize) -> f64 {
    let (dy, dx) = DIRS[k];
    (dy as f64).atan2(dx as f64)
}

/// Geomorphon class raster (`geomorphons.classify(..., return_dirs=True)`):
/// `(cls, plus_bits, minus_bits)`, `cls` u8 with 0 = undefined (never here:
/// the terrain is NaN-free), bit k of the masks = direction `DIRS[k]`.
///
/// BORDER SEMANTICS, carried from the prototype's `_shift`: the shifted
/// raster is `zs[y, x] = z[y - dy*s, x - dx*s]`, so the ray for `DIRS[k]`
/// visits `(y - dy*s, x - dx*s)` for `s` in `step0..=n_steps_max`, and a
/// ray that leaves the array sees nothing (those samples are skipped). The
/// counts are direction-agnostic and the saddle axes are axial, so the
/// reversal is invisible downstream — but bit k means exactly what it
/// meant in Python.
///
/// EDGE CAUTION: cells within `lookup_m` of the edge classify toward flat —
/// an artifact of the mask, not the terrain; `collar_px` excludes them.
pub fn classify(z: &[f64], nx: usize, ny: usize, cell: f64, lookup_m: f64, skip_m: f64,
                flat_deg: f64) -> (Vec<u8>, Vec<u8>, Vec<u8>) {
    let t = flat_deg.to_radians().tan();
    let n_steps_max = py_round(lookup_m / cell).max(2);
    let step0 = py_round(skip_m / cell).max(1);
    let n = nx * ny;
    let mut n_plus = vec![0u8; n];
    let mut n_minus = vec![0u8; n];
    let mut plus_bits = vec![0u8; n];
    let mut minus_bits = vec![0u8; n];
    for (k, &(dy, dx)) in DIRS.iter().enumerate() {
        let diag = ((dy * dy + dx * dx) as f64).sqrt();
        for y in 0..ny {
            for x in 0..nx {
                let idx = y * nx + x;
                let z0 = z[idx];
                if !z0.is_finite() {
                    continue;
                }
                let mut a = f64::NEG_INFINITY;   // max elevation tangent
                let mut b = f64::NEG_INFINITY;   // max depression tangent
                for s in step0..=n_steps_max {
                    let yy = y as i64 - dy * s;
                    let xx = x as i64 - dx * s;
                    if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                        continue;
                    }
                    let zs = z[yy as usize * nx + xx as usize];
                    let tanang = (zs - z0) / (s as f64 * cell * diag);
                    if tanang.is_finite() {
                        a = a.max(tanang);
                        b = b.max(-tanang);
                    }
                }
                let seen = a.is_finite() && b.is_finite();
                if !seen {
                    continue;
                }
                if (b - a) > t {          // land falls away -> locally high
                    n_plus[idx] += 1;
                    plus_bits[idx] |= 1 << k;
                }
                if (a - b) > t {          // land rises -> locally low
                    n_minus[idx] += 1;
                    minus_bits[idx] |= 1 << k;
                }
            }
        }
    }
    let cls = (0..n).map(|i| {
        if z[i].is_finite() { LOOKUP[n_plus[i] as usize][n_minus[i] as usize] } else { 0 }
    }).collect();
    (cls, plus_bits, minus_bits)
}

/// `geomorphons.collar_px(lookup_m, cell)`: edge collar inside which
/// classes are mask artifacts (`ceil(lookup_m / cell)`).
pub fn collar_px(lookup_m: f64, cell: f64) -> usize {
    (lookup_m / cell).ceil() as usize
}

/// `geomorphons.saddle_axes(plus_bits, minus_bits, axial_min=0.8)`:
/// `(saddle, fall_axis, rise_axis)`. A saddle is >= 2 rising and >= 2
/// falling directions, each set axially aligned (resultant length of
/// `e^{i 2 theta}` over the set > `axial_min`), the two axes within 30 deg
/// of square. Axes are in `[0, pi)`: OPEN (fall-away) and BLOCKED (ridge).
pub fn saddle_axes(plus_bits: &[u8], minus_bits: &[u8], axial_min: f64)
    -> (Vec<bool>, Vec<f64>, Vec<f64>) {
    let n = plus_bits.len();
    let e2: Vec<(f64, f64)> = (0..8).map(|k| {
        let a = 2.0 * dir_angle(k);
        (a.cos(), a.sin())
    }).collect();
    let mut saddle = vec![false; n];
    let mut fall = vec![0.0; n];
    let mut rise = vec![0.0; n];
    for i in 0..n {
        let (mut pr, mut pi_, mut mr, mut mi) = (0.0, 0.0, 0.0, 0.0);
        let (mut n_p, mut n_m) = (0i32, 0i32);
        for k in 0..8 {
            if (plus_bits[i] >> k) & 1 == 1 {
                pr += e2[k].0;
                pi_ += e2[k].1;
                n_p += 1;
            }
            if (minus_bits[i] >> k) & 1 == 1 {
                mr += e2[k].0;
                mi += e2[k].1;
                n_m += 1;
            }
        }
        let ax_p = pr.hypot(pi_) / (n_p.max(1) as f64);
        let ax_m = mr.hypot(mi) / (n_m.max(1) as f64);
        // np.angle(0) = 0, so an empty set gives axis 0
        let f = (pi_.atan2(pr) / 2.0).rem_euclid(PI);
        let r = (mi.atan2(mr) / 2.0).rem_euclid(PI);
        fall[i] = f;
        rise[i] = r;
        // perpendicularity of the two axes, in axial space
        let d = (f - r).rem_euclid(PI);
        let perp = d.min(PI - d);            // [0, pi/2]
        saddle[i] = n_p >= 2 && n_m >= 2 && ax_p > axial_min && ax_m > axial_min
            && perp > PI / 2.0 - PI / 6.0;   // within 30 deg of square
    }
    (saddle, fall, rise)
}

/// `siting.build_morphology(f)`: cls240 (lookup 240 m, skip 16 m, flat
/// 1 deg, with directions), cls80 (80 m, skip 8 m), saddles from the 240 m
/// ternary, collar `collar_px(240, cell)`.
pub fn build_morphology(f: &Fields) -> Morphology {
    let (cls240, pb, mb) = classify(&f.z8, f.nx, f.ny, f.cell, 240.0, 16.0, 1.0);
    let (cls80, _, _) = classify(&f.z8, f.nx, f.ny, f.cell, 80.0, 8.0, 1.0);
    let (saddle, fall_axis, rise_axis) = saddle_axes(&pb, &mb, 0.8);
    Morphology {
        cls240, cls80, saddle, fall_axis, rise_axis,
        collar: collar_px(240.0, f.cell),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid<F: Fn(f64, f64) -> f64>(n: usize, cell: f64, f: F) -> Vec<f64> {
        let c = (n as f64 - 1.0) / 2.0;
        let mut z = vec![0.0; n * n];
        for y in 0..n {
            for x in 0..n {
                z[y * n + x] = f((y as f64 - c) * cell, (x as f64 - c) * cell);
            }
        }
        z
    }

    #[test]
    fn cone_is_peak_and_inverted_cone_is_pit() {
        let n = 41;
        let cone = grid(n, 8.0, |y, x| -0.2 * y.hypot(x));
        let (cls, pb, mb) = classify(&cone, n, n, 8.0, 80.0, 8.0, 1.0);
        let c = (n / 2) * n + n / 2;
        assert_eq!(cls[c], 2, "cone apex -> peak");
        assert_eq!(pb[c], 0xFF);
        assert_eq!(mb[c], 0);
        let pit: Vec<f64> = cone.iter().map(|v| -v).collect();
        let (cls, _, _) = classify(&pit, n, n, 8.0, 80.0, 8.0, 1.0);
        assert_eq!(cls[c], 10, "inverted cone -> pit");
    }

    #[test]
    fn tent_is_ridge_and_v_is_valley() {
        let n = 41;
        let tent = grid(n, 8.0, |_y, x| -0.2 * x.abs());
        let (cls, _, _) = classify(&tent, n, n, 8.0, 80.0, 8.0, 1.0);
        let c = (n / 2) * n + n / 2;
        assert_eq!(cls[c], 3, "tent crest -> ridge");
        let vee: Vec<f64> = tent.iter().map(|v| -v).collect();
        let (cls, _, _) = classify(&vee, n, n, 8.0, 80.0, 8.0, 1.0);
        assert_eq!(cls[c], 9, "V floor -> valley");
    }

    #[test]
    fn saddle_surface_has_perpendicular_axes() {
        // z = 0.2 (x^2 - y^2) / 100: falls away along y, rises along x
        let n = 41;
        let z = grid(n, 8.0, |y, x| 0.002 * (x * x - y * y));
        let (_, pb, mb) = classify(&z, n, n, 8.0, 80.0, 8.0, 1.0);
        let (sad, fall, rise) = saddle_axes(&pb, &mb, 0.8);
        let c = (n / 2) * n + n / 2;
        assert!(sad[c], "hyperbolic paraboloid centre is a saddle");
        // fall axis along y (bearing pi/2), rise axis along x (0)
        assert!((fall[c] - PI / 2.0).abs() < 1e-9, "fall {}", fall[c]);
        assert!(rise[c].abs() < 1e-9 || (rise[c] - PI).abs() < 1e-9, "rise {}", rise[c]);
        // a plain slope is not a saddle
        let s = grid(n, 8.0, |y, _x| 0.1 * y);
        let (_, pb, mb) = classify(&s, n, n, 8.0, 80.0, 8.0, 1.0);
        let (sad, _, _) = saddle_axes(&pb, &mb, 0.8);
        assert!(!sad[c]);
    }

    #[test]
    fn edge_cells_see_nothing_and_flat_ground_is_flat() {
        let n = 12;
        let z = vec![5.0; n * n];
        let (cls, pb, mb) = classify(&z, n, n, 8.0, 80.0, 8.0, 1.0);
        assert!(cls.iter().all(|&c| c == 1));
        assert!(pb.iter().all(|&b| b == 0) && mb.iter().all(|&b| b == 0));
        assert_eq!(collar_px(240.0, 8.0), 30);
        assert_eq!(collar_px(80.0, 8.0), 10);
    }

    #[test]
    fn lookup_table_matches_python() {
        assert_eq!(LOOKUP[8][0], 2);
        assert_eq!(LOOKUP[0][8], 10);
        assert_eq!(LOOKUP[4][2], 5);
        assert_eq!(LOOKUP[2][4], 7);
        assert_eq!(LOOKUP[2][2], 6);
        assert_eq!(LOOKUP[0][0], 1);
    }
}
