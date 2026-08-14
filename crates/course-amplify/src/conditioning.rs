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
    /// Doubled-angle structure-tensor planes of the TPI gradient
    /// (c2 = gx²−gy², s2 = 2·gx·gy, g2 = gx²+gy²) — summed over a
    /// footprint they give the local GRAIN axis + coherence. Doubled
    /// angles make every mean circular by construction (opposite
    /// gradients reinforce; axes, not directions). TPI, not lp400: a
    /// broad ramp (the tile-boundary drawdown, a macro tilt) has a
    /// near-perfectly coherent lp400 gradient but zero relief FABRIC —
    /// the first build read the construction ramp as strong grain on
    /// every terrain type. TPI keeps only the 64–400 m fabric, which is
    /// precisely the band the mid texture must align with.
    pub c2: Vec<f64>,
    pub s2: Vec<f64>,
    pub g2: Vec<f64>,
    /// UNIT doubled-angle vectors of the flow-distance gradient (zero
    /// where the gradient is degenerate or inside the frame). The
    /// distance field's level sets run along the channels, so near
    /// channels this is the "texture follows the river" axis — a wide
    /// flat bottomland has no TPI fabric at all, and the first build
    /// left the grain unrotated exactly where the reviewer looks for it.
    pub c2d: Vec<f64>,
    pub s2d: Vec<f64>,
    pub nx: usize,
    pub ny: usize,
}

/// e-folding distance for how far the channel-following axis reaches
/// before the TPI fabric takes over.
pub const DIST_AXIS_L_M: f64 = 250.0;

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
        // structure-tensor planes from the TPI gradient (see struct doc).
        // The construction frame gets no vote: the boundary rims are a
        // few SHARP raised rows, and under energy weighting they
        // dominated every border-touching window (near-1.0 coherence on
        // every terrain type). Same exclusion S4 applies to its basin
        // inventory (EDGE_FRAME_M).
        let frame = (48.0 / cell).ceil() as usize;
        let mut c2 = vec![0.0f64; nx * ny];
        let mut s2 = vec![0.0f64; nx * ny];
        let mut g2 = vec![0.0f64; nx * ny];
        for y in frame..ny.saturating_sub(frame) {
            for x in frame..nx.saturating_sub(frame) {
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(nx - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(ny - 1);
                let gx = (tpi[y * nx + xp] - tpi[y * nx + xm]) / (((xp - xm) as f64) * cell);
                let gy = (tpi[yp * nx + x] - tpi[ym * nx + x]) / (((yp - ym) as f64) * cell);
                let i = y * nx + x;
                c2[i] = gx * gx - gy * gy;
                s2[i] = 2.0 * gx * gy;
                g2[i] = gx * gx + gy * gy;
            }
        }
        // unit doubled-angle vectors of the flow-distance gradient
        let mut c2d = vec![0.0f64; nx * ny];
        let mut s2d = vec![0.0f64; nx * ny];
        for y in frame..ny.saturating_sub(frame) {
            for x in frame..nx.saturating_sub(frame) {
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(nx - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(ny - 1);
                let gx = (dist_m[y * nx + xp] - dist_m[y * nx + xm])
                    / (((xp - xm) as f64) * cell);
                let gy = (dist_m[yp * nx + x] - dist_m[ym * nx + x])
                    / (((yp - ym) as f64) * cell);
                let g = gx * gx + gy * gy;
                // the distance gradient is ~unit away from degeneracies
                // (channel lines, medial axes); keep only trustworthy cells
                if g > 0.25 {
                    let i = y * nx + x;
                    c2d[i] = (gx * gx - gy * gy) / g;
                    s2d[i] = 2.0 * gx * gy / g;
                }
            }
        }
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
            c2,
            s2,
            g2,
            c2d,
            s2d,
            nx,
            ny,
        }
    }

    /// Local target GRAIN axis [0, π) + coherence [0, 1] over a patch
    /// footprint. Two doubled-angle tensors blend by channel proximity:
    /// the TPI fabric tensor (valley/ridge grain) and the flow-distance
    /// tensor (level sets run along the channels). Near a channel the
    /// distance term dominates — that is the bottomland ridge-and-swale
    /// axis that follows a curving trunk, where TPI has nothing to say
    /// because the floor is flat. The +π/2 is the gradient→grain flip
    /// (contours run along the grain; the gradient runs across it) —
    /// same convention as `dictionary::patch_axis`, documented there.
    pub fn patch_axis(&self, y0: usize, x0: usize, w: usize) -> (f64, f64) {
        let (mut c2, mut s2, mut g2) = (0.0f64, 0.0f64, 0.0f64);
        let (mut cd, mut sd) = (0.0f64, 0.0f64);
        let (mut dsum, mut n) = (0.0f64, 0.0f64);
        for y in y0..(y0 + w).min(self.ny) {
            for x in x0..(x0 + w).min(self.nx) {
                let i = y * self.nx + x;
                c2 += self.c2[i];
                s2 += self.s2[i];
                g2 += self.g2[i];
                cd += self.c2d[i];
                sd += self.s2d[i];
                dsum += self.dist_m[i];
                n += 1.0;
            }
        }
        if n < 1.0 {
            return (0.0, 0.0);
        }
        // both terms are resultant-scaled unit vectors: |tpi term| =
        // fabric coherence, |dist term| = channel-axis coherence
        let (tc, ts) = if g2 > 1e-18 { (c2 / g2, s2 / g2) } else { (0.0, 0.0) };
        let (dc, ds) = (cd / n, sd / n);
        let p = libm::exp(-(dsum / n) / DIST_AXIS_L_M);
        let vx = (1.0 - p) * tc + p * dc;
        let vy = (1.0 - p) * ts + p * ds;
        let coh = (vx * vx + vy * vy).sqrt();
        if coh <= 1e-9 {
            return (0.0, 0.0);
        }
        let axis = (0.5 * libm::atan2(vy, vx) + std::f64::consts::FRAC_PI_2)
            .rem_euclid(std::f64::consts::PI);
        (axis, coh)
    }

    /// TPI-fabric coherence alone over a window — the kilometre-scale
    /// gate ingredient (the distance term is deliberately absent: over a
    /// big window the distance field wraps around many channels and its
    /// mixed axes would dilute a perfectly real fabric signal).
    pub fn fabric_coherence(&self, y0: usize, x0: usize, w: usize) -> f64 {
        let (mut c2, mut s2, mut g2) = (0.0f64, 0.0f64, 0.0f64);
        for y in y0..(y0 + w).min(self.ny) {
            for x in x0..(x0 + w).min(self.nx) {
                let i = y * self.nx + x;
                c2 += self.c2[i];
                s2 += self.s2[i];
                g2 += self.g2[i];
            }
        }
        if g2 <= 1e-18 {
            return 0.0;
        }
        (c2 * c2 + s2 * s2).sqrt() / g2
    }

    /// Channel-following strength over the FOOTPRINT: proximity ×
    /// local channel-axis coherence. Near a curving trunk this opens the
    /// rotation gate even where kilometre fabric is weak — bottomland
    /// grain is justified by the channel itself.
    pub fn channel_strength(&self, y0: usize, x0: usize, w: usize) -> f64 {
        let (mut cd, mut sd) = (0.0f64, 0.0f64);
        let (mut dsum, mut n) = (0.0f64, 0.0f64);
        for y in y0..(y0 + w).min(self.ny) {
            for x in x0..(x0 + w).min(self.nx) {
                let i = y * self.nx + x;
                cd += self.c2d[i];
                sd += self.s2d[i];
                dsum += self.dist_m[i];
                n += 1.0;
            }
        }
        if n < 1.0 {
            return 0.0;
        }
        libm::exp(-(dsum / n) / DIST_AXIS_L_M) * (cd * cd + sd * sd).sqrt() / n
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
