//! Position-seeded exemplar quilting — the realism bet, productionized.
//!
//! Every rule here was earned by a measured failure in the F3 QA and is
//! recorded in `tools/dictionary/report.md`:
//! 1. Poisson integration stays in CELL units end to end (a stray
//!    cell-size factor is a ×8 amplitude error at 8 m).
//! 2. RE-BAND-LIMIT after integration is mandatory — a patchwork of
//!    mutually inconsistent gradients pumps spurious above-band power.
//! 3. Spectral correction must be conservatively clamped (the QA's
//!    naive iterated equalizer compounded ×6/pass into a 60–250× blowup).
//! 4. The amplitude closer runs LAST, gain clamped to [0.5, 2.5].
//!
//! Order-freedom: the acceptance criteria require that permuting patch
//! processing order is bit-identical. Patch choice is therefore a pure
//! function of (seed, level, patch position) — never of the canvas built
//! so far — and composition is Hann overlap-add, which is a commutative
//! sum. No seam DP, no best-of-K against neighbours: those couple the
//! result to processing order.

use course_seed::RunIdentity;
use course_world::grid::Grid;

use crate::conditioning::{gaussian_blur, Conditioning, SIGMA_PER_L};
use crate::dictionary::{Dictionary, Level};

pub const PATCH: usize = 32;
pub const STRIDE: usize = 16;
/// Poisson multigrid: fixed V-cycles (never "until converged"). Three is
/// enough BECAUSE of the re-band-limit that follows: the slowly-converging
/// components of the Poisson solution are the low frequencies, and the
/// high-pass discards them — running more cycles polishes energy that is
/// thrown away two lines later (measured: 12 cycles cost ~700 ms at 2 m
/// for a fine_std difference of < 0.01 m).
pub const V_CYCLES: usize = 3;
/// Amplitude-closer clamp (F3 rule 4).
pub const GAIN_CLAMP: (f64, f64) = (0.5, 2.5);
/// Closer target multiplier per band. The bucket amp targets are MEDIAN
/// PATCH stds, and a tile's band std exceeds the median patch's (patch
/// means are removed per patch; quiet patches drag the median): measured
/// deficit was a consistent ×1.24–1.33 against the corpus tile targets
/// across all six biomes at fine, ~×1.05 at mid.
pub const CLOSER_GAIN_MID: f64 = 1.05;
pub const CLOSER_GAIN_FINE: f64 = 1.28;
/// Per-level spectral shelf gains (low, mid, high sub-band within the
/// level's own band), applied once, conservative. Calibrated against the
/// held-out reconstruction targets; the identity default is safe.
pub const SHELF_GAINS_MID: [f64; 3] = [1.0, 1.0, 1.0];
pub const SHELF_GAINS_FINE: [f64; 3] = [1.0, 1.0, 1.0];

/// Which dictionary level this pass runs.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Band {
    /// 64–400 m at 8 m cells.
    Mid,
    /// < 64 m at 2 m cells.
    Fine,
}

impl Band {
    pub fn name(self) -> &'static str {
        match self {
            Band::Mid => "mid",
            Band::Fine => "fine",
        }
    }
    /// Upper cut wavelength (m) for the re-band-limit.
    fn lam_cut_m(self) -> f64 {
        match self {
            Band::Mid => 400.0,
            Band::Fine => 64.0,
        }
    }
    fn closer_gain(self) -> f64 {
        match self {
            Band::Mid => CLOSER_GAIN_MID,
            Band::Fine => CLOSER_GAIN_FINE,
        }
    }
    fn shelf_gains(self) -> [f64; 3] {
        match self {
            Band::Mid => SHELF_GAINS_MID,
            Band::Fine => SHELF_GAINS_FINE,
        }
    }
}

/// Orientation-aligned pasting (G-TERRAIN tracked fix): patches carry a
/// source grain axis (computed at dictionary load), the conditioning
/// carries a target axis + coherence, and coherent positions pick the
/// candidate needing the least rotation, then rotate it the rest of the
/// way. Gated on DATA (coherence), never on biome.
const K_CANDIDATES: u64 = 4;
/// Below this target coherence the position takes the exact legacy path
/// (salt 0x51 pick, θ = 0.0, original height buffer — bit-identical to
/// the pre-orientation engine). "Must not force anisotropy where the
/// biome has none" (stage doc) falls out of the data.
///
/// Coherence is measured over COH_WINDOW_MULT× the patch footprint
/// (~1 km at mid), NOT the footprint itself: any smooth lowpass field
/// has near-constant gradient direction over one patch (every terrain
/// type reads 0.81–0.85 there — examples/coh_probe.rs), so footprint
/// coherence cannot see GRAIN. Grain that justifies rotating a patch is
/// axis consistency at the kilometre scale, where grainy terrain
/// (valley/ridge fabric, ~0.5–0.65 median) separates from isotropic
/// terrain (hummock fields, ~0.3). Thresholds sit in that gap — see
/// coh_probe for the measured table.
const COH_LO: f64 = 0.40;
/// Full rotation strength at/above this coherence; linear ramp between.
const COH_HI: f64 = 0.75;
/// Coherence window = this multiple of the patch footprint, centered.
const COH_WINDOW_MULT: usize = 4;
/// Fine band rotates too: with per-position precompute the cost is one
/// 32×32 resample per chosen patch (~10 ms at fine), and the micro
/// texture following the same fabric as the mid band is what a curving
/// valley grain reads as up close.
const ROTATE_FINE: bool = true;

/// Deterministic per-position pick: a pure function of (seed, level,
/// patch row, patch col, salt) — the position-seeding the stage doc
/// requires. Layout order can never matter because nothing else feeds in.
fn pick(seed: u64, band: Band, py: usize, px: usize, n: usize, salt: u64) -> usize {
    let mut h = seed ^ salt.rotate_left(17);
    h ^= (py as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h ^= (px as u64).rotate_left(32).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= match band {
        Band::Mid => 0x51D2_C4A7,
        Band::Fine => 0xA3E9_11B5,
    };
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h % n.max(1) as u64) as usize
}

fn patch_grid(n: usize) -> Vec<usize> {
    let mut xs: Vec<usize> = (0..).map(|i| i * STRIDE).take_while(|&x| x + PATCH <= n).collect();
    if let Some(&last) = xs.last() {
        if last + PATCH != n {
            xs.push(n - PATCH);
        }
    }
    xs
}

/// Nearest non-empty bucket by L1 distance in bin space — the runtime
/// fallback for conditioning combinations the harvest never saw (D3
/// measured ~100% coverage, so this path is rare but must exist).
fn nearest_bucket(level: &Level, want: u32) -> Option<u32> {
    if level.buckets.contains_key(&want) {
        return Some(want);
    }
    let dims: Vec<u32> = level.edges.iter().map(|e| e.len() as u32 + 1).collect();
    let unpack = |mut id: u32| -> Vec<i64> {
        let mut v = vec![0i64; dims.len()];
        for k in (0..dims.len()).rev() {
            v[k] = (id % dims[k]) as i64;
            id /= dims[k];
        }
        v
    };
    let me = unpack(want);
    level
        .buckets
        .keys()
        .min_by_key(|&&k| {
            let kv = unpack(k);
            let d: i64 = kv.iter().zip(&me).map(|(a, b)| (a - b).abs()).sum();
            (d, k)
        })
        .copied()
}

/// One quilting pass: synthesize the band's residual on `spec`-shaped
/// ground. `cond` is always the 8 m conditioning; `cond_scale` maps this
/// grid's cells to conditioning cells (1 for mid, 4 for fine).
#[allow(clippy::too_many_arguments)]
pub fn quilt(
    dict: &Dictionary,
    biome_key: &str,
    band: Band,
    grid_n: usize,
    cell_m: f64,
    cond: &Conditioning,
    cond_scale: usize,
    identity: &RunIdentity,
) -> Vec<f64> {
    let level = &dict.biomes[biome_key][band.name()];
    let n = grid_n * grid_n;
    let mut gx = vec![0.0f64; n];
    let mut gy = vec![0.0f64; n];
    let mut wsum = vec![0.0f64; n];
    let mut amp_t = vec![0.0f64; n];
    // Hann window (matches the builder's spike-derived geometry)
    let win: Vec<f64> = {
        let mut w = vec![0.0f64; PATCH * PATCH];
        for y in 0..PATCH {
            for x in 0..PATCH {
                let hy = 0.5 - 0.5 * libm::cos(std::f64::consts::TAU * y as f64 / PATCH as f64);
                let hx = 0.5 - 0.5 * libm::cos(std::f64::consts::TAU * x as f64 / PATCH as f64);
                w[y * PATCH + x] = hy * hx + 1e-6;
            }
        }
        w
    };
    let seed = identity.stream_seed();
    let cond_w = PATCH / cond_scale.max(1);

    // ---- choose every patch, in any order (pure per-position function) --
    // The choice table is filled in whatever order `positions` arrives —
    // the permuted-order acceptance test hands it shuffled — and the
    // result cannot differ because each entry is a pure function of
    // (seed, band, position). Composition below is GATHER, not scatter:
    // every output cell sums its ≤4 covering patches in canonical
    // row-major patch order, so float summation order is fixed regardless
    // of processing order and the output is bit-identical.
    let origins = patch_grid(grid_n);
    let np = origins.len();
    let mut positions: Vec<(usize, usize)> = Vec::with_capacity(np * np);
    for pyi in 0..np {
        for pxi in 0..np {
            positions.push((pyi, pxi));
        }
    }
    let chosen = choose_patches(
        dict, level, band, cond, cond_scale, cond_w, &origins, &positions, seed,
    );
    // Rotated buffers, one optional slot per position (None = θ was
    // exactly 0.0, gather aliases the patch's own heights — the exact
    // fast path). Built from the choice table alone, so content is
    // independent of processing order.
    let rotated: Vec<Option<Vec<f32>>> = chosen
        .iter()
        .map(|c| match c {
            Some(ch) if ch.theta != 0.0 => Some(rotate_patch(&ch.patch.heights, ch.theta)),
            _ => None,
        })
        .collect();

    // per-axis coverage: which patch indices cover a given coordinate
    let coverage: Vec<Vec<u16>> = {
        let mut cov = vec![Vec::new(); grid_n];
        for (pi, &o) in origins.iter().enumerate() {
            for c in o..(o + PATCH).min(grid_n) {
                cov[c].push(pi as u16);
            }
        }
        cov
    };

    for gy_c in 0..grid_n {
        for gx_c in 0..grid_n {
            let g = gy_c * grid_n + gx_c;
            let (mut sx, mut sy, mut sw, mut sa) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for &pyi in &coverage[gy_c] {
                for &pxi in &coverage[gx_c] {
                    let idx = pyi as usize * np + pxi as usize;
                    let Some(ch) = &chosen[idx] else {
                        continue;
                    };
                    let amp = ch.amp_p50;
                    let y = gy_c - origins[pyi as usize];
                    let x = gx_c - origins[pxi as usize];
                    let i = y * PATCH + x;
                    let xp = (x + 1).min(PATCH - 1);
                    let xm = x.saturating_sub(1);
                    let yp = (y + 1).min(PATCH - 1);
                    let ym = y.saturating_sub(1);
                    let ph: &[f32] = match &rotated[idx] {
                        Some(r) => r,
                        None => &ch.patch.heights,
                    };
                    let pgx = (ph[y * PATCH + xp] as f64 - ph[y * PATCH + xm] as f64)
                        / (xp - xm).max(1) as f64;
                    let pgy = (ph[yp * PATCH + x] as f64 - ph[ym * PATCH + x] as f64)
                        / (yp - ym).max(1) as f64;
                    let w = win[i];
                    sx += pgx * w;
                    sy += pgy * w;
                    sw += w;
                    sa += amp * w;
                }
            }
            let w = sw.max(1e-9);
            gx[g] = sx / w;
            gy[g] = sy / w;
            amp_t[g] = sa / w;
            wsum[g] = sw;
        }
    }
    let _ = &wsum;

    // ---- Poisson integrate (cell units, fixed multigrid cycles) --------
    let mut z = poisson_multigrid(&gx, &gy, grid_n);

    // ---- re-band-limit (F3 rule 2) --------------------------------------
    let mut zg = Grid {
        spec: course_world::grid::GridSpec::new(
            course_world::math::Vec2::new(0.0, 0.0),
            cell_m,
            grid_n as u32,
            grid_n as u32,
        ),
        data: std::mem::take(&mut z),
    };
    // 3-pass box approximation of the Gaussian (O(n) per pass): the exact
    // tap-based blur at 2 m was ~200 ms of the budget for a spectral
    // difference far below anything the band statistics can see.
    let sigma = SIGMA_PER_L * band.lam_cut_m() / cell_m;
    let box_w = ((sigma * 1.153) as usize * 2 + 1).max(3);
    let mut lp = zg.data.clone();
    for _ in 0..3 {
        lp = box_filter(&lp, grid_n, box_w);
    }
    for i in 0..n {
        zg.data[i] -= lp[i];
    }

    // ---- conservative spectral shelves (F3 rule 3) ----------------------
    let gains = band.shelf_gains();
    if gains != [1.0, 1.0, 1.0] {
        let cut_a = band.lam_cut_m() / 2.5;
        let cut_b = band.lam_cut_m() / 6.5;
        let la = gaussian_blur(&zg, SIGMA_PER_L * cut_a / cell_m);
        let lb = gaussian_blur(&zg, SIGMA_PER_L * cut_b / cell_m);
        for i in 0..n {
            let low = la.data[i];
            let mid = lb.data[i] - la.data[i];
            let high = zg.data[i] - lb.data[i];
            zg.data[i] = low * gains[0] + mid * gains[1] + high * gains[2];
        }
    }

    // ---- amplitude closer LAST (F3 rule 4) ------------------------------
    let local = local_std(&zg.data, grid_n, PATCH * 2);
    for i in 0..n {
        let gain = (amp_t[i] * band.closer_gain() / local[i].max(1e-6))
            .clamp(GAIN_CLAMP.0, GAIN_CLAMP.1);
        zg.data[i] *= gain;
    }
    zg.data
}

/// One selected patch: which, how loud, and how far to rotate it.
pub struct Chosen<'d> {
    pub patch: &'d crate::dictionary::Patch,
    pub amp_p50: f64,
    /// Rotation applied at paste time, radians CCW in grid-index space;
    /// exactly 0.0 on the legacy path (aliases the original buffer).
    pub theta: f64,
}

/// Shortest signed axis difference a→b, in (−π/2, π/2].
fn axis_delta(from: f64, to: f64) -> f64 {
    let mut d = (to - from).rem_euclid(std::f64::consts::PI);
    if d > std::f64::consts::FRAC_PI_2 {
        d -= std::f64::consts::PI;
    }
    d
}

/// Fill the per-position choice table. Public for the permuted-order
/// acceptance test, which passes a shuffled `positions` list and asserts
/// the final surface is bit-identical. Everything here — including the
/// rotation angle — is a pure function of (seed, band, position,
/// conditioning); the canvas never feeds back.
#[allow(clippy::too_many_arguments)]
pub fn choose_patches<'d>(
    _dict: &'d Dictionary,
    level: &'d crate::dictionary::Level,
    band: Band,
    cond: &Conditioning,
    cond_scale: usize,
    cond_w: usize,
    origins: &[usize],
    positions: &[(usize, usize)],
    seed: u64,
) -> Vec<Option<Chosen<'d>>> {
    let np = origins.len();
    let mut chosen: Vec<Option<Chosen>> = Vec::with_capacity(np * np);
    chosen.resize_with(np * np, || None);
    let rotate_band = matches!(band, Band::Mid) || ROTATE_FINE;
    for &(pyi, pxi) in positions {
        let (y0, x0) = (origins[pyi], origins[pxi]);
        let key = cond.patch_key(y0 / cond_scale, x0 / cond_scale, cond_w.max(1));
        let want = Dictionary::bucket_of(level, &key);
        let Some(bid) = nearest_bucket(level, want) else { continue };
        let bucket = &level.buckets[&bid];
        let w = cond_w.max(1);
        let (yc, xc) = (y0 / cond_scale, x0 / cond_scale);
        // local axis from the footprint (follows a curving valley),
        // coherence from the big centered window (demands real grain)
        let (taxis, _) = cond.patch_axis(yc, xc, w);
        // constant-size window, shifted inward at borders — a clamped
        // (shrunken) window degenerates toward footprint scale and
        // reads falsely coherent there
        let big = (w * COH_WINDOW_MULT).min(cond.nx.min(cond.ny));
        let (yb, xb) = (
            (yc + w / 2).saturating_sub(big / 2).min(cond.ny - big),
            (xc + w / 2).saturating_sub(big / 2).min(cond.nx - big),
        );
        let (_, tcoh) = cond.patch_axis(yb, xb, big);
        let sel = if !rotate_band || tcoh < COH_LO {
            // legacy path, bit-identical to the pre-orientation engine
            let pi = pick(seed, band, pyi, pxi, bucket.patches.len(), 0x51);
            Chosen { patch: &bucket.patches[pi], amp_p50: bucket.amp_p50, theta: 0.0 }
        } else {
            // K candidates; cheapest = least rotation of a trustworthy
            // axis. An isotropic patch's axis means nothing, so its cost
            // is a flat 45°·(1−coh) — coherent near-aligned patches win.
            let mut best: Option<(f64, usize)> = None;
            for k in 0..K_CANDIDATES {
                let pi = pick(seed, band, pyi, pxi, bucket.patches.len(), 0x51 + k);
                let p = &bucket.patches[pi];
                let cost = axis_delta(p.axis_rad, taxis).abs() * p.coherence
                    + std::f64::consts::FRAC_PI_4 * (1.0 - p.coherence);
                // strict < keeps the EARLIEST candidate on ties — the
                // deterministic tiebreak
                if best.map_or(true, |(bc, _)| cost < bc) {
                    best = Some((cost, pi));
                }
            }
            let pi = best.expect("K_CANDIDATES >= 1").1;
            let p = &bucket.patches[pi];
            let ramp = ((tcoh - COH_LO) / (COH_HI - COH_LO)).clamp(0.0, 1.0);
            Chosen {
                patch: p,
                amp_p50: bucket.amp_p50,
                theta: ramp * axis_delta(p.axis_rad, taxis),
            }
        };
        chosen[pyi * np + pxi] = Some(sel);
    }
    chosen
}

/// Resample a PATCH×PATCH buffer rotated by `theta` (CCW in grid-index
/// space) about the patch centre: inverse-rotate each output coordinate,
/// bilinear-read, edge-clamp. Rotating HEIGHTS once per chosen position
/// (not gradients per cell) keeps the gather's exact finite-difference
/// code untouched — the rotated surface's gradients are automatically
/// consistent, which the Poisson integration requires. The Hann window
/// stays in the OUTPUT frame (partition of unity is a property of the
/// output tiling, not the patch content). Corner starvation from the
/// missing source corners is masked by the window's ~0 edge weight.
fn rotate_patch(heights: &[f32], theta: f64) -> Vec<f32> {
    let c = libm::cos(theta);
    let s = libm::sin(theta);
    let ctr = (PATCH as f64 - 1.0) * 0.5;
    let mut out = vec![0.0f32; PATCH * PATCH];
    let hi = (PATCH - 1) as f64;
    for y in 0..PATCH {
        for x in 0..PATCH {
            let dx = x as f64 - ctr;
            let dy = y as f64 - ctr;
            let u = (c * dx + s * dy + ctr).clamp(0.0, hi);
            let v = (-s * dx + c * dy + ctr).clamp(0.0, hi);
            let (x0, y0) = (u as usize, v as usize);
            let (x1, y1) = ((x0 + 1).min(PATCH - 1), (y0 + 1).min(PATCH - 1));
            let (tx, ty) = (u - x0 as f64, v - y0 as f64);
            let a = heights[y0 * PATCH + x0] as f64 * (1.0 - tx)
                + heights[y0 * PATCH + x1] as f64 * tx;
            let b = heights[y1 * PATCH + x0] as f64 * (1.0 - tx)
                + heights[y1 * PATCH + x1] as f64 * tx;
            out[y * PATCH + x] = (a * (1.0 - ty) + b * ty) as f32;
        }
    }
    out
}

/// Local standard deviation over a w×w window (running-sum box filters).
fn local_std(a: &[f64], n: usize, w: usize) -> Vec<f64> {
    let mean = box_filter(a, n, w);
    let sq: Vec<f64> = a.iter().map(|v| v * v).collect();
    let mean_sq = box_filter(&sq, n, w);
    mean.iter()
        .zip(&mean_sq)
        .map(|(m, ms)| (ms - m * m).max(1e-12).sqrt())
        .collect()
}

/// O(n) separable box filter, edge-clamped, odd-ish window.
pub fn box_filter(a: &[f64], n: usize, w: usize) -> Vec<f64> {
    let h = (w / 2) as i64;
    let clamp = |v: i64| v.clamp(0, n as i64 - 1) as usize;
    let mut tmp = vec![0.0f64; n * n];
    for y in 0..n {
        // running sum along x
        let mut acc = 0.0;
        for dx in -h..=h {
            acc += a[y * n + clamp(dx)];
        }
        tmp[y * n] = acc;
        for x in 1..n {
            acc += a[y * n + clamp(x as i64 + h)];
            acc -= a[y * n + clamp(x as i64 - 1 - h)];
            tmp[y * n + x] = acc;
        }
    }
    let mut out = vec![0.0f64; n * n];
    let denom = ((2 * h + 1) * (2 * h + 1)) as f64;
    for x in 0..n {
        let mut acc = 0.0;
        for dy in -h..=h {
            acc += tmp[clamp(dy) * n + x];
        }
        out[x] = acc / denom;
        for y in 1..n {
            acc += tmp[clamp(y as i64 + h) * n + x];
            acc -= tmp[clamp(y as i64 - 1 - h) * n + x];
            out[y * n + x] = acc / denom;
        }
    }
    out
}

/// Least-squares height from per-cell gradients: Neumann Poisson via a
/// fixed number of multigrid V-cycles. Everything in CELL units — no
/// physical cell size appears anywhere (F3 rule 1).
fn poisson_multigrid(gx: &[f64], gy: &[f64], n: usize) -> Vec<f64> {
    // divergence
    let mut rhs = vec![0.0f64; n * n];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let dgx = if x > 0 { gx[i] - gx[i - 1] } else { gx[i] };
            let dgy = if y > 0 { gy[i] - gy[i - n] } else { gy[i] };
            rhs[i] = dgx + dgy;
        }
    }
    let mut z = vec![0.0f64; n * n];
    for _ in 0..V_CYCLES {
        v_cycle(&mut z, &rhs, n);
    }
    // remove mean (Neumann null space)
    let mean = z.iter().sum::<f64>() / (n * n) as f64;
    for v in z.iter_mut() {
        *v -= mean;
    }
    z
}

fn v_cycle(z: &mut [f64], rhs: &[f64], n: usize) {
    if n <= 24 {
        for _ in 0..40 {
            gauss_seidel(z, rhs, n);
        }
        return;
    }
    for _ in 0..2 {
        gauss_seidel(z, rhs, n);
    }
    // residual
    let mut res = vec![0.0f64; n * n];
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let mut lap = 0.0;
            let mut deg = 0.0f64;
            if x > 0 {
                lap += z[i - 1];
                deg += 1.0;
            }
            if x + 1 < n {
                lap += z[i + 1];
                deg += 1.0;
            }
            if y > 0 {
                lap += z[i - n];
                deg += 1.0;
            }
            if y + 1 < n {
                lap += z[i + n];
                deg += 1.0;
            }
            res[i] = rhs[i] - (lap - deg * z[i]);
        }
    }
    // restrict (full weighting by 2×2 average), recurse, prolong
    let m = n.div_ceil(2);
    let mut res_c = vec![0.0f64; m * m];
    for y in 0..m {
        for x in 0..m {
            let mut acc = 0.0;
            let mut cnt = 0.0;
            for dy in 0..2 {
                for dx in 0..2 {
                    let (yy, xx) = (y * 2 + dy, x * 2 + dx);
                    if yy < n && xx < n {
                        acc += res[yy * n + xx];
                        cnt += 1.0;
                    }
                }
            }
            // scale: coarse Laplacian spans 2 cells, so rhs scales by 4
            res_c[y * m + x] = acc / cnt * 4.0;
        }
    }
    let mut e_c = vec![0.0f64; m * m];
    v_cycle(&mut e_c, &res_c, m);
    for y in 0..n {
        for x in 0..n {
            z[y * n + x] += e_c[(y / 2) * m + (x / 2)];
        }
    }
    for _ in 0..2 {
        gauss_seidel(z, rhs, n);
    }
}

fn gauss_seidel(z: &mut [f64], rhs: &[f64], n: usize) {
    for y in 0..n {
        for x in 0..n {
            let i = y * n + x;
            let mut lap = 0.0;
            let mut deg = 0.0f64;
            if x > 0 {
                lap += z[i - 1];
                deg += 1.0;
            }
            if x + 1 < n {
                lap += z[i + 1];
                deg += 1.0;
            }
            if y > 0 {
                lap += z[i - n];
                deg += 1.0;
            }
            if y + 1 < n {
                lap += z[i + n];
                deg += 1.0;
            }
            z[i] = (lap - rhs[i]) / deg.max(1.0);
        }
    }
}
