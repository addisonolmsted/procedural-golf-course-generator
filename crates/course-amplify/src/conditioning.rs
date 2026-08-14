//! The conditioning fields S3 looks patches up by — computed from the
//! skeleton's own surface with the SAME definitions the corpus extraction
//! used (`tools/macro_campaign/macro_campaign/extract_v2.py::decompose`),
//! so a generated cell's conditioning vector means the same thing as a
//! real cell's and the bucket lookup is comparing like with like:
//!
//!   lp400       = gaussian(z, σ = SIGMA_PER_L·400/cell)
//!   lp_slope    = |∇ lp400|
//!   tpi         = lp400 − uniform(lp400, 400 m window)
//!   relief_pos  = rank-normalized lp400 in [0, 1]
//!   dist        = the skeleton's flow_distance (metres to nearest channel)
//!
//! The bucket key is (lp_slope, tpi, relief_pos, log1p(dist)) — identical
//! to the builder's `cond_vec`.

use course_world::grid::Grid;

/// The spike's half-amplitude Gaussian band constant: σ per unit cut
/// wavelength. `sqrt(ln 2 / (2π²))`.
pub const SIGMA_PER_L: f64 = 0.18737;

/// Separable Gaussian blur with edge-clamped taps. Kernel radius 3σ.
pub fn gaussian_blur(src: &Grid<f64>, sigma_cells: f64) -> Grid<f64> {
    let (nx, ny) = (src.spec.nx as usize, src.spec.ny as usize);
    let r = (3.0 * sigma_cells).ceil() as i64;
    let mut kernel = Vec::with_capacity((2 * r + 1) as usize);
    let s2 = 2.0 * sigma_cells * sigma_cells;
    let mut sum = 0.0;
    for i in -r..=r {
        let w = libm::exp(-(i as f64) * (i as f64) / s2);
        kernel.push(w);
        sum += w;
    }
    for w in kernel.iter_mut() {
        *w /= sum;
    }
    // horizontal
    let mut tmp = vec![0.0f64; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let mut acc = 0.0;
            for (ki, w) in kernel.iter().enumerate() {
                let xx = (x as i64 + ki as i64 - r).clamp(0, nx as i64 - 1) as usize;
                acc += src.data[y * nx + xx] * w;
            }
            tmp[y * nx + x] = acc;
        }
    }
    // vertical
    let mut out = src.clone();
    for y in 0..ny {
        for x in 0..nx {
            let mut acc = 0.0;
            for (ki, w) in kernel.iter().enumerate() {
                let yy = (y as i64 + ki as i64 - r).clamp(0, ny as i64 - 1) as usize;
                acc += tmp[yy * nx + x] * w;
            }
            out.data[y * nx + x] = acc;
        }
    }
    out
}

/// Box (uniform) filter with an odd window, edge-clamped, separable.
pub fn uniform_filter(src: &Grid<f64>, w_cells: usize) -> Grid<f64> {
    let (nx, ny) = (src.spec.nx as usize, src.spec.ny as usize);
    let h = (w_cells / 2) as i64;
    let mut tmp = vec![0.0f64; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let mut acc = 0.0;
            for dx in -h..=h {
                let xx = (x as i64 + dx).clamp(0, nx as i64 - 1) as usize;
                acc += src.data[y * nx + xx];
            }
            tmp[y * nx + x] = acc / (2 * h + 1) as f64;
        }
    }
    let mut out = src.clone();
    for y in 0..ny {
        for x in 0..nx {
            let mut acc = 0.0;
            for dy in -h..=h {
                let yy = (y as i64 + dy).clamp(0, ny as i64 - 1) as usize;
                acc += tmp[yy * nx + x];
            }
            out.data[y * nx + x] = acc / (2 * h + 1) as f64;
        }
    }
    out
}

/// Per-cell conditioning planes at the grid's own resolution.
pub struct Conditioning {
    pub lp_slope: Vec<f64>,
    pub tpi: Vec<f64>,
    pub relief_pos: Vec<f64>,
    /// metres to the nearest channel (from the skeleton)
    pub dist_m: Vec<f64>,
    pub nx: usize,
    pub ny: usize,
}

impl Conditioning {
    pub fn compute(z: &Grid<f64>, dist_m: &[f64]) -> Conditioning {
        let (nx, ny) = (z.spec.nx as usize, z.spec.ny as usize);
        let cell = z.spec.cell_size;
        let lp400 = gaussian_blur(z, SIGMA_PER_L * 400.0 / cell);
        // slope of the lowpass
        let mut lp_slope = vec![0.0f64; nx * ny];
        for y in 0..ny {
            for x in 0..nx {
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(nx - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(ny - 1);
                let gx = (lp400.data[y * nx + xp] - lp400.data[y * nx + xm])
                    / (((xp - xm) as f64) * cell);
                let gy = (lp400.data[yp * nx + x] - lp400.data[ym * nx + x])
                    / (((yp - ym) as f64) * cell);
                lp_slope[y * nx + x] = (gx * gx + gy * gy).sqrt();
            }
        }
        // tpi
        let w = {
            let w = (400.0 / cell).round() as usize;
            (w | 1).max(3)
        };
        let uni = uniform_filter(&lp400, w);
        let tpi: Vec<f64> = lp400
            .data
            .iter()
            .zip(&uni.data)
            .map(|(a, b)| a - b)
            .collect();
        // rank-normalized relief position (stable sort = deterministic)
        let mut order: Vec<usize> = (0..nx * ny).collect();
        order.sort_by(|&a, &b| lp400.data[a].total_cmp(&lp400.data[b]).then(a.cmp(&b)));
        let mut relief_pos = vec![0.0f64; nx * ny];
        let denom = ((nx * ny) as f64 - 1.0).max(1.0);
        for (rank, &lin) in order.iter().enumerate() {
            relief_pos[lin] = rank as f64 / denom;
        }
        Conditioning {
            lp_slope,
            tpi,
            relief_pos,
            dist_m: dist_m.to_vec(),
            nx,
            ny,
        }
    }

    /// The 4-dim bucket key averaged over a patch footprint — identical to
    /// the dictionary builder's `cond_vec`.
    pub fn patch_key(&self, y0: usize, x0: usize, w: usize) -> [f64; 4] {
        let mut acc = [0.0f64; 4];
        let mut n = 0.0f64;
        for y in y0..(y0 + w).min(self.ny) {
            for x in x0..(x0 + w).min(self.nx) {
                let i = y * self.nx + x;
                acc[0] += self.lp_slope[i];
                acc[1] += self.tpi[i];
                acc[2] += self.relief_pos[i];
                acc[3] += self.dist_m[i];
                n += 1.0;
            }
        }
        for a in acc.iter_mut() {
            *a /= n.max(1.0);
        }
        [acc[0], acc[1], acc[2], libm::log1p(acc[3].max(0.0))]
    }
}
