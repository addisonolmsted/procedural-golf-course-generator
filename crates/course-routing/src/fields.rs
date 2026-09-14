//! The 8 m derived stack — port of `siting.build_fields` (and the pad
//! kernel of `playability.pads`).
//!
//! Units: slope is RISE/RUN everywhere in this crate, as in `siting.py`.
//! `playability.pads` wants PERCENT; the conversion happens at that call
//! and nowhere else (the prototype's rule, kept).

use crate::img;
use crate::terrain::Terrain;
use crate::Fields;

// --- playability.py constants (the pad gates) -----------------------------
/// `playability.GREEN_R`: a green is ~30 m across.
pub const GREEN_R: f64 = 16.0;
/// `playability.TEE_R`.
pub const TEE_R: f64 = 9.0;
/// `playability.GREEN_RELIEF`: metres across the pad.
pub const GREEN_RELIEF: f64 = 1.4;

// --- siting.py slope gates (rise/run) --------------------------------------
/// `siting.GREEN_SLOPE_RR`: rise/run; playability GREEN_SLOPE is 5%.
pub const GREEN_SLOPE_RR: f64 = 0.05;
/// `siting.TEE_SLOPE_RR`.
pub const TEE_SLOPE_RR: f64 = 0.065;
/// `siting.FAIRWAY_SLOPE_RR`.
pub const FAIRWAY_SLOPE_RR: f64 = 0.08;

/// Python's `int(round(v))`: round half to EVEN (banker's), which is what
/// every `int(round(x_m / cell))` in the prototype does. It matters:
/// `round(100 / 8) = round(12.5) = 12` in Python, 13 with `f64::round`.
pub fn py_round(v: f64) -> i64 {
    v.round_ties_even() as i64
}

/// `playability.slope_pct(z, c)`: `hypot(gx, gy) * 100` from `np.gradient`.
fn slope_pct(z: &[f64], nx: usize, ny: usize, cell: f64) -> Vec<f64> {
    let (gy, gx) = img::gradient(z, nx, ny, cell);
    gx.iter().zip(&gy).map(|(a, b)| a.hypot(*b) * 100.0).collect()
}

/// `playability.pads(z, c, wet, radius, max_slope, max_relief)`: cells
/// where a disc of `radius` is flat enough (slope in PERCENT), dry, and —
/// when `max_relief` is given — low-relief (disc max - disc min).
/// `n = max(1, round(radius / c))` cells; the disc is `yy^2 + xx^2 <= n^2`.
pub fn pads(z: &[f64], nx: usize, ny: usize, cell: f64, wet: &[bool], radius: f64,
            max_slope_pct: f64, max_relief: Option<f64>) -> Vec<bool> {
    let n = py_round(radius / cell).max(1) as usize;
    let s = slope_pct(z, nx, ny, cell);
    let flat: Vec<bool> = s.iter().map(|v| *v <= max_slope_pct).collect();
    let mut ok = img::erode_disc(&flat, nx, ny, n);
    let dry_in: Vec<bool> = wet.iter().map(|w| !w).collect();
    let dry = img::erode_disc(&dry_in, nx, ny, n);
    for (o, d) in ok.iter_mut().zip(&dry) {
        *o = *o && *d;
    }
    if let Some(mr) = max_relief {
        let hi = img::max_filter_disc(z, nx, ny, n);
        let lo = img::min_filter_disc(z, nx, ny, n);
        for i in 0..ok.len() {
            ok[i] = ok[i] && (hi[i] - lo[i]) <= mr;
        }
    }
    ok
}

/// The prototype's `relief_pos` rank: `order = argsort(lp, kind="stable")`,
/// `rp[order] = arange(N) / max(N - 1, 1)`. Equal values get DISTINCT ranks
/// in index order (a stable rank, not the mid-rank of ties), so this is
/// written here rather than through `img::percentile_rank`, whose doc
/// describes the half-tie convention.
fn stable_rank(a: &[f64]) -> Vec<f64> {
    let n = a.len();
    let mut order: Vec<usize> = (0..n).collect();
    // stable sort on the value; equal values keep index order (argsort stable)
    order.sort_by(|&i, &j| a[i].partial_cmp(&a[j]).unwrap_or(std::cmp::Ordering::Equal));
    let denom = (n.max(2) - 1) as f64;
    let mut rp = vec![0.0; n];
    for (rank, &idx) in order.iter().enumerate() {
        rp[idx] = rank as f64 / denom;
    }
    rp
}

/// `siting.build_fields(z2, cell2, wet2, flow_accum8=None)`.
///
/// `k = round(8 / cell2)`; `z8 = z2[::k, ::k]` (stride sampling, so the
/// 1501-node 2 m grid gives 376 cells), slope via `np.gradient`,
/// `relief_pos` the stable rank of the 400 m Gaussian lowpass
/// (`sigma = 400 / pi / cell` cells), `tpi200 = z8 - uniform(z8, 25)`,
/// `subgrid_rough` the k x k block std edge-padded by one cell to the
/// strided shape, `wet8 = wet2[::k, ::k]`, `d_water` the EDT of `~wet8`
/// (inf on a dry tile), the pads, `pad_fair`, `room`.
pub fn build_fields(t: &Terrain) -> Fields {
    let cell2 = t.z2.spec.cell_size;
    let nx2 = t.z2.spec.nx as usize;
    let ny2 = t.z2.spec.ny as usize;
    let k = py_round(8.0 / cell2).max(1) as usize;
    let cell = cell2 * k as f64;
    // z2[::k, ::k] -> ceil(n / k) samples per axis
    let nx = nx2.div_ceil(k);
    let ny = ny2.div_ceil(k);
    let n = nx * ny;

    let mut z8 = vec![0.0; n];
    let mut wet8 = vec![false; n];
    for y in 0..ny {
        for x in 0..nx {
            let src = (y * k) * nx2 + x * k;
            z8[y * nx + x] = t.z2.data[src];
            wet8[y * nx + x] = t.wet2[src];
        }
    }

    let (gy, gx) = img::gradient(&z8, nx, ny, cell);
    let slope: Vec<f64> = gx.iter().zip(&gy).map(|(a, b)| a.hypot(*b)).collect();

    // relief_pos on a 400 m lowpass, stable rank (dtm_metrics convention)
    let lp = img::gaussian_filter(&z8, nx, ny, 400.0 / std::f64::consts::PI / cell);
    let relief_pos = stable_rank(&lp);

    let k200 = (py_round(200.0 / cell) | 1) as usize;
    let u = img::uniform_filter(&z8, nx, ny, k200);
    let tpi200: Vec<f64> = z8.iter().zip(&u).map(|(a, b)| a - b).collect();

    // block std at k x k, then edge-pad up to the strided shape
    // (z8 (strided) is one cell larger than the block reduce)
    let (rough_b, d0, d1) = img::block_reduce_std(&t.z2.data, nx2, ny2, k);
    let nyb = ny2 / k;
    let nxb = nx2 / k;
    debug_assert_eq!(d0 * d1, nyb * nxb, "img::block_reduce_std dims");
    let mut subgrid_rough = vec![0.0; n];
    for y in 0..ny {
        let by = y.min(nyb - 1);
        for x in 0..nx {
            let bx = x.min(nxb - 1);
            subgrid_rough[y * nx + x] = rough_b[by * nxb + bx];
        }
    }

    let d_water = if wet8.iter().any(|w| *w) {
        let dry: Vec<bool> = wet8.iter().map(|w| !w).collect();
        img::edt(&dry, nx, ny, cell)
    } else {
        vec![f64::INFINITY; n]
    };

    // pads() takes PERCENT; convert here and nowhere else.
    let pad_green = pads(&z8, nx, ny, cell, &wet8, GREEN_R, GREEN_SLOPE_RR * 100.0,
                         Some(GREEN_RELIEF));
    let pad_tee = pads(&z8, nx, ny, cell, &wet8, TEE_R, TEE_SLOPE_RR * 100.0, None);
    let pad_fair: Vec<bool> = slope.iter().zip(&wet8)
        .map(|(s, w)| *s <= FAIRWAY_SLOPE_RR && !w).collect();
    let room = img::edt(&pad_green, nx, ny, cell);

    Fields {
        cell, nx, ny, z8, slope, relief_pos, tpi200, subgrid_rough, wet8, d_water,
        pad_green, pad_tee, pad_fair, room,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn py_round_is_bankers() {
        assert_eq!(py_round(12.5), 12);   // halo = round(100 / 8)
        assert_eq!(py_round(2.5), 2);     // k20 = round(20 / 8) | 1 = 3
        assert_eq!(py_round(7.5), 8);     // sep = round(60 / 8)
        assert_eq!(py_round(87.5), 88);   // fluvial h = round(700 / 8)
        assert_eq!(py_round(118.75), 119);
        assert_eq!(py_round(181.25), 181);
        assert_eq!(py_round(93.75), 94);
        assert_eq!(py_round(281.25), 281);
        assert_eq!(py_round(1.125), 1);
    }

    #[test]
    fn stable_rank_matches_argsort_convention() {
        // ties get distinct ranks in index order; denominator is N - 1
        let r = stable_rank(&[3.0, 1.0, 1.0, 2.0]);
        assert_eq!(r, vec![1.0, 0.0, 1.0 / 3.0, 2.0 / 3.0]);
        assert_eq!(stable_rank(&[5.0]), vec![0.0]);
    }

    /// A 2 m synthetic tile in the generator's shape (1501 x 1501 nodes).
    pub(crate) fn synthetic_terrain(pond: bool) -> Terrain {
        use course_world::grid::{Grid, GridSpec};
        use course_world::math::Vec2;
        let n = 1501u32;
        let spec = GridSpec::new(Vec2::new(0.0, 0.0), 2.0, n, n);
        let mut z = vec![0.0; (n * n) as usize];
        let mut w = vec![f64::NAN; (n * n) as usize];
        for y in 0..n {
            for x in 0..n {
                let (xm, ym) = (x as f64 * 2.0, y as f64 * 2.0);
                let v = 12.0 * (xm / 310.0).sin() * (ym / 260.0).cos()
                    + 3.0 * (xm / 90.0 + ym / 70.0).sin();
                z[(y * n + x) as usize] = v;
                if pond && (xm - 1500.0).hypot(ym - 1400.0) < 60.0 {
                    w[(y * n + x) as usize] = v;
                }
            }
        }
        let h = Grid::from_data(spec, z);
        let wg = Grid::from_data(spec, w);
        Terrain::from_grids(crate::Mode::Aeolian, &h, &wg, Vec::new())
    }

    #[test]
    #[ignore = "needs the img kernels (gradient, gaussian, uniform, edt, disc filters), still being filled by the img agent"]
    fn build_fields_shapes_and_invariants() {
        let t = synthetic_terrain(false);
        let f = build_fields(&t);
        assert_eq!((f.nx, f.ny, f.cell), (376, 376, 8.0));
        assert_eq!(f.z8[0], t.z2.data[0]);
        assert_eq!(f.z8[5 * f.nx + 7], t.z2.data[20 * 1501 + 28]);
        assert!(f.d_water.iter().all(|d| d.is_infinite()), "dry tile -> d_water inf");
        assert!(f.relief_pos.iter().all(|r| (0.0..=1.0).contains(r)));
        for i in 0..f.nx * f.ny {
            assert_eq!(f.pad_fair[i], f.slope[i] <= FAIRWAY_SLOPE_RR);
            assert!(f.pad_green[i] || f.room[i] == 0.0);
            assert!(!f.pad_green[i] || f.slope[i] <= GREEN_SLOPE_RR + 1e-12);
        }
        let t = synthetic_terrain(true);
        let f = build_fields(&t);
        assert!(f.wet8.iter().any(|w| *w));
        assert!(f.d_water.iter().all(|d| d.is_finite()));
        for i in 0..f.nx * f.ny {
            assert!(!(f.wet8[i] && (f.pad_green[i] || f.pad_tee[i] || f.pad_fair[i])));
        }
    }
}
