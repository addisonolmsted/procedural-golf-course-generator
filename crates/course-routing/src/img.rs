//! The image kernels the prototype took from `scipy.ndimage` and `numpy`,
//! on row-major `[y * nx + x]` slices. Each mirrors the SciPy/NumPy
//! semantics the Python calls relied on (border mode, footprint, tie
//! behaviour) -- see each function's doc for which. Exactness matters:
//! the thresholds downstream (`room >= 15 m`, `pad_room >= 25 m`, slope
//! gates) compare these outputs to real metres.
//!
//! Border modes, audited against every call site in `tools/golf/`
//! (2026-09-14): every `minimum_filter` / `maximum_filter` /
//! `uniform_filter` / `gaussian_filter` call uses SciPy's DEFAULT mode
//! `reflect` (`d c b a | a b c d | d c b a`, a periodic bounce of period
//! `2n` -- verified against SciPy 1.16 for kernels wider than the line);
//! only `percentile_filter` (`greens.field_score`) passes `mode="nearest"`.
//! Window centring follows SciPy's `origin=0`: a window of `size` covers
//! offsets `[-(size / 2), size - 1 - size / 2]`, so an even size leans one
//! cell towards the start (verified: size 4 = offsets -2..=1).
//!
//! Every function is deterministic and allocation-free apart from its
//! output and per-line scratch; the separable ones run their rows in
//! parallel (rayon) where the result is bit-identical to the serial form
//! (each row/column is an independent computation, so it always is).

use rayon::prelude::*;

// ---------------------------------------------------------------------------
// index helpers

/// SciPy `reflect` index: `d c b a | a b c d | d c b a`, period `2n`.
#[inline]
fn reflect(i: isize, n: usize) -> usize {
    if n == 1 {
        return 0;
    }
    let p = 2 * n as isize;
    let mut i = i % p;
    if i < 0 {
        i += p;
    }
    if i >= n as isize {
        (p - 1 - i) as usize
    } else {
        i as usize
    }
}

/// SciPy `nearest` index: clamp.
#[inline]
fn nearest(i: isize, n: usize) -> usize {
    if i < 0 {
        0
    } else if i >= n as isize {
        n - 1
    } else {
        i as usize
    }
}

/// Floor of the integer square root.
#[inline]
fn isqrt(v: usize) -> usize {
    let mut w = (v as f64).sqrt() as usize;
    while w * w > v {
        w -= 1;
    }
    while (w + 1) * (w + 1) <= v {
        w += 1;
    }
    w
}

/// `ny x nx` row-major -> `nx x ny` row-major.
fn transpose<T: Copy + Default + Send + Sync>(a: &[T], nx: usize, ny: usize) -> Vec<T> {
    let mut out = vec![T::default(); nx * ny];
    out.par_chunks_mut(ny).enumerate().for_each(|(x, col)| {
        for (y, c) in col.iter_mut().enumerate() {
            *c = a[y * nx + x];
        }
    });
    out
}

// ---------------------------------------------------------------------------
// 1-D sliding-window reductions (van Herk / Gil-Werman), reflect borders

/// `out[i] = op over line[reflect(i - size/2 ..= i + size - 1 - size/2)]`,
/// O(n) regardless of `size` (van Herk / Gil-Werman on the reflect-extended
/// line). `op` must be associative and commutative (min, max, and).
fn filt1d<T: Copy, F: Fn(T, T) -> T>(line: &[T], size: usize, op: &F, out: &mut [T]) {
    let n = line.len();
    debug_assert!(size >= 1 && out.len() == n);
    if size == 1 || n == 0 {
        out.copy_from_slice(line);
        return;
    }
    let left = size / 2;
    let m = n + size - 1; // extended length
    // pad the extended line to a whole number of blocks
    let nb = m.div_ceil(size);
    let mlen = nb * size;
    let mut ext: Vec<T> = Vec::with_capacity(mlen);
    for k in 0..mlen {
        ext.push(line[reflect(k as isize - left as isize, n)]);
    }
    // prefix (g) and suffix (h) reductions within each block of `size`
    let mut g = ext.clone();
    let mut h = ext.clone();
    for b in 0..nb {
        let s = b * size;
        for k in 1..size {
            g[s + k] = op(g[s + k - 1], ext[s + k]);
        }
        for k in (0..size - 1).rev() {
            h[s + k] = op(h[s + k + 1], ext[s + k]);
        }
    }
    for i in 0..n {
        // window ext[i .. i + size]
        out[i] = op(h[i], g[i + size - 1]);
    }
}

/// `filt1d` over every row, rows in parallel.
fn filt_rows<T, F>(a: &[T], nx: usize, ny: usize, size: usize, op: &F) -> Vec<T>
where
    T: Copy + Default + Send + Sync,
    F: Fn(T, T) -> T + Sync,
{
    let mut out = vec![T::default(); nx * ny];
    out.par_chunks_mut(nx)
        .zip(a.par_chunks(nx))
        .for_each(|(o, row)| filt1d(row, size, op, o));
    out
}

/// Separable rectangle reduction: rows with `w`, then columns with `h`.
fn rect_filter<T, F>(a: &[T], nx: usize, ny: usize, h: usize, w: usize, op: &F) -> Vec<T>
where
    T: Copy + Default + Send + Sync,
    F: Fn(T, T) -> T + Sync,
{
    assert_eq!(a.len(), nx * ny);
    let r = filt_rows(a, nx, ny, w, op);
    let t = transpose(&r, nx, ny);
    let c = filt_rows(&t, ny, nx, h, op);
    transpose(&c, ny, nx)
}

/// Disc reduction by row-run decomposition: the disc `yy^2 + xx^2 <= r^2`
/// is the union over `dy` of horizontal runs of half-width
/// `isqrt(r^2 - dy^2)`, so the answer is the reduction over `dy` of the
/// 1-D row filter (half-width `wx(dy)`) of row `reflect(y + dy)`. One 1-D
/// pass per distinct half-width (at most `r + 1`), then `2r + 1` row
/// combines: O(n * r), exact for any `r` (reflect along y is a row
/// permutation, reflect along x lives inside the 1-D pass).
fn disc_filter<T, F>(a: &[T], nx: usize, ny: usize, r: usize, op: &F) -> Vec<T>
where
    T: Copy + Default + Send + Sync,
    F: Fn(T, T) -> T + Sync,
{
    assert_eq!(a.len(), nx * ny);
    let r2 = r * r;
    let wx: Vec<usize> = (0..=r).map(|dy| isqrt(r2 - dy * dy)).collect();
    let mut grids: Vec<Option<Vec<T>>> = (0..=r).map(|_| None).collect();
    for &w in &wx {
        if grids[w].is_none() {
            grids[w] = Some(filt_rows(a, nx, ny, 2 * w + 1, op));
        }
    }
    let mut out = vec![T::default(); nx * ny];
    out.par_chunks_mut(nx).enumerate().for_each(|(y, row)| {
        let mut first = true;
        for dy in -(r as isize)..=(r as isize) {
            let yy = reflect(y as isize + dy, ny);
            let g = grids[wx[dy.unsigned_abs()]].as_ref().unwrap();
            let src = &g[yy * nx..(yy + 1) * nx];
            if first {
                row.copy_from_slice(src);
                first = false;
            } else {
                for x in 0..nx {
                    row[x] = op(row[x], src[x]);
                }
            }
        }
    });
    out
}

#[inline]
fn fmin(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

#[inline]
fn fmax(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}

// ---------------------------------------------------------------------------
// distance transform

/// Exact Euclidean distance transform (`scipy.ndimage.distance_transform_edt
/// (mask, sampling=cell)`): for every TRUE cell, the distance in metres to
/// the nearest FALSE cell; FALSE cells are 0. If no cell is FALSE every
/// entry is `f64::INFINITY` (SciPy returns the distance to the
/// out-of-bounds edge in that case, but every caller in the prototype
/// tests `isfinite` first, so infinity is what they want).
///
/// Felzenszwalb-Huttenlocher, two separable passes: a two-sweep 1-D
/// transform along x (rows in parallel), then the parabola lower envelope
/// along y (columns in parallel, via a transpose). All arithmetic on the
/// integer squared offsets is exact in f64, so the nearest feature is the
/// true nearest (ties resolve to the same squared distance either way),
/// and the final metres are formed the way SciPy forms them:
/// `sqrt((cell*dy)^2 + (cell*dx)^2)`.
pub fn edt(mask: &[bool], nx: usize, ny: usize, cell: f64) -> Vec<f64> {
    assert_eq!(mask.len(), nx * ny);
    if nx == 0 || ny == 0 {
        return Vec::new();
    }
    let inf = f64::INFINITY;
    // pass 1 along x: |dx| to the nearest FALSE in the row, INF if none
    let mut d1 = vec![inf; nx * ny];
    d1.par_chunks_mut(nx).zip(mask.par_chunks(nx)).for_each(|(o, row)| {
        let mut last: Option<usize> = None;
        for x in 0..nx {
            if !row[x] {
                last = Some(x);
            }
            if let Some(l) = last {
                o[x] = (x - l) as f64;
            }
        }
        last = None;
        for x in (0..nx).rev() {
            if !row[x] {
                last = Some(x);
            }
            if let Some(l) = last {
                let d = (l - x) as f64;
                if d < o[x] {
                    o[x] = d;
                }
            }
        }
    });
    // pass 2 along y on f = dx^2: lower envelope of parabolas
    let d1t = transpose(&d1, nx, ny); // nx rows of ny
    let mut outt = vec![inf; nx * ny];
    outt.par_chunks_mut(ny).zip(d1t.par_chunks(ny)).for_each(|(o, col)| {
        let n = ny;
        let mut v = vec![0usize; n];
        let mut z = vec![0f64; n + 1];
        let mut k = 0usize;
        let mut started = false;
        let f = |q: usize| -> f64 { col[q] * col[q] }; // exact: integer squares
        for q in 0..n {
            if !col[q].is_finite() {
                continue;
            }
            if !started {
                v[0] = q;
                z[0] = -inf;
                z[1] = inf;
                started = true;
                continue;
            }
            let fq = f(q) + (q * q) as f64;
            let mut s;
            loop {
                let p = v[k];
                s = (fq - (f(p) + (p * p) as f64)) / (2 * q - 2 * p) as f64;
                if s <= z[k] {
                    k -= 1; // z[0] = -inf guarantees k >= 1 here
                } else {
                    break;
                }
            }
            k += 1;
            v[k] = q;
            z[k] = s;
            z[k + 1] = inf;
        }
        if !started {
            return; // whole column INF
        }
        k = 0;
        for q in 0..n {
            while z[k + 1] < q as f64 {
                k += 1;
            }
            let p = v[k];
            let dy = (q as f64 - p as f64) * cell;
            let dx = col[p] * cell;
            o[q] = (dy * dy + dx * dx).sqrt();
        }
    });
    transpose(&outt, ny, nx)
}

// ---------------------------------------------------------------------------
// min / max filters

/// Minimum over a `size = (h, w)` rectangle centred on each cell
/// (`ndimage.minimum_filter(a, size=(h, w))`, mode `reflect`: `d c b a | a b c d`).
/// Callers: the window scan (`size=(h, w)` of ~182 x 119 cells),
/// `build_clubhouse_fields` / `preview_sites` (`k20 = 3`). O(n) in the size.
pub fn min_filter_rect(a: &[f64], nx: usize, ny: usize, h: usize, w: usize) -> Vec<f64> {
    rect_filter(a, nx, ny, h, w, &fmin)
}

/// Maximum over a `size = (h, w)` rectangle, mode `reflect`.
pub fn max_filter_rect(a: &[f64], nx: usize, ny: usize, h: usize, w: usize) -> Vec<f64> {
    rect_filter(a, nx, ny, h, w, &fmax)
}

/// Minimum over a disc footprint of radius `r` cells (`yy^2 + xx^2 <= r^2`),
/// mode `reflect` (`ndimage.minimum_filter(a, footprint=disc)`). Callers:
/// `playability.pads` relief (r = 2 for GREEN_R 16 m at 8 m).
pub fn min_filter_disc(a: &[f64], nx: usize, ny: usize, r: usize) -> Vec<f64> {
    disc_filter(a, nx, ny, r, &fmin)
}

/// Maximum over a disc footprint, mode `reflect`. Callers: `pads` relief,
/// and `build_clubhouse_fields` `near_green` / `near_tee` (on 0/1 masks,
/// r = 15 for CLUBHOUSE_RADIUS_M 120 m).
pub fn max_filter_disc(a: &[f64], nx: usize, ny: usize, r: usize) -> Vec<f64> {
    disc_filter(a, nx, ny, r, &fmax)
}

/// Boolean erosion by a disc: TRUE where every cell of the disc footprint
/// is TRUE (`ndimage.minimum_filter(mask, footprint=disc)` on a bool array,
/// mode `reflect`). The pad kernel of `playability.pads` (r = 2 green,
/// r = 1 tee at 8 m).
pub fn erode_disc(mask: &[bool], nx: usize, ny: usize, r: usize) -> Vec<bool> {
    disc_filter(mask, nx, ny, r, &|a: bool, b: bool| a & b)
}

/// Boolean dilation by a disc: TRUE where any cell of the disc footprint
/// is TRUE (`ndimage.maximum_filter(mask.astype(uint8), footprint=disc)`,
/// mode `reflect`; `near_green` / `near_tee` in `build_clubhouse_fields`).
pub fn dilate_disc(mask: &[bool], nx: usize, ny: usize, r: usize) -> Vec<bool> {
    disc_filter(mask, nx, ny, r, &|a: bool, b: bool| a | b)
}

// ---------------------------------------------------------------------------
// smoothing

/// SciPy's `uniform_filter1d`: running sum over the reflect-extended line,
/// `tmp / size` per output (the same accumulation order, so the rounding
/// matches to the ulp).
fn uniform1d(line: &[f64], size: usize, out: &mut [f64]) {
    let n = line.len();
    if size == 1 {
        out.copy_from_slice(line);
        return;
    }
    let left = size / 2;
    let m = n + size - 1;
    let ext: Vec<f64> = (0..m).map(|k| line[reflect(k as isize - left as isize, n)]).collect();
    let fs = size as f64;
    let mut tmp = 0.0;
    for &e in &ext[..size] {
        tmp += e;
    }
    out[0] = tmp / fs;
    for j in 0..n - 1 {
        tmp += ext[j + size] - ext[j];
        out[j + 1] = tmp / fs;
    }
}

/// Box mean over a `size` window (`ndimage.uniform_filter(a, size)`, mode
/// `reflect`), separable: axis 0 (y) first, then axis 1 (x), as SciPy
/// orders it. Callers pass odd sizes (`tpi200`: 25; `tpi60`: 7; the
/// clubhouse `dens`: 37) but any `size >= 1` is honoured with SciPy's
/// centring.
pub fn uniform_filter(a: &[f64], nx: usize, ny: usize, size: usize) -> Vec<f64> {
    assert_eq!(a.len(), nx * ny);
    assert!(size >= 1);
    let run = |g: &[f64], w: usize, l: usize| -> Vec<f64> {
        let mut out = vec![0.0; w * l];
        out.par_chunks_mut(w).zip(g.par_chunks(w)).for_each(|(o, row)| uniform1d(row, size, o));
        out
    };
    let t = transpose(a, nx, ny);
    let c = run(&t, ny, nx); // along y
    let back = transpose(&c, ny, nx);
    run(&back, nx, ny) // along x
}

/// SciPy's symmetric `correlate1d` inner loop: centre first, then the
/// outermost pair inward (`for jj = -size1 .. -1: (line[jj] + line[-jj]) * w[jj]`).
fn correlate1d_sym(line: &[f64], w: &[f64], out: &mut [f64]) {
    let n = line.len();
    let r = w.len() - 1; // w[0] centre, w[k] at offset +-k
    let ext: Vec<f64> = (0..n + 2 * r).map(|k| line[reflect(k as isize - r as isize, n)]).collect();
    for i in 0..n {
        let c = i + r;
        let mut acc = ext[c] * w[0];
        for k in (1..=r).rev() {
            acc += (ext[c - k] + ext[c + k]) * w[k];
        }
        out[i] = acc;
    }
}

/// Gaussian lowpass (`ndimage.gaussian_filter(a, sigma)`, truncate 4.0,
/// radius `int(4 * sigma + 0.5)`, weights `exp(-0.5 / sigma^2 * x^2)`
/// normalised to sum 1, mode `reflect`), separable y then x, `sigma` in
/// cells. Caller: `build_fields` lowpass at `400 / pi / 8` = 15.9 cells
/// (radius 64).
pub fn gaussian_filter(a: &[f64], nx: usize, ny: usize, sigma: f64) -> Vec<f64> {
    assert_eq!(a.len(), nx * ny);
    if sigma <= 1e-15 {
        return a.to_vec();
    }
    let radius = (4.0 * sigma + 0.5) as usize;
    let sigma2 = sigma * sigma;
    // phi(x) = exp(-0.5 / sigma2 * x^2), x integer, normalised (SciPy
    // `_gaussian_kernel1d`); the sum runs -r..=r in that order.
    let full: Vec<f64> = (-(radius as i64)..=radius as i64)
        .map(|x| (-0.5 / sigma2 * (x * x) as f64).exp())
        .collect();
    let s: f64 = full.iter().sum();
    let w: Vec<f64> = (0..=radius).map(|k| full[radius + k] / s).collect();
    let run = |g: &[f64], wd: usize, l: usize| -> Vec<f64> {
        let mut out = vec![0.0; wd * l];
        out.par_chunks_mut(wd).zip(g.par_chunks(wd)).for_each(|(o, row)| correlate1d_sym(row, &w, o));
        out
    };
    let t = transpose(a, nx, ny);
    let c = run(&t, ny, nx);
    let back = transpose(&c, ny, nx);
    run(&back, nx, ny)
}

// ---------------------------------------------------------------------------
// rank filter

/// Percentile over an ANNULUS footprint `r_in <= rho <= r_out` cells
/// (`ndimage.percentile_filter(a, pct, footprint=ring, mode="nearest")`),
/// the ring being `d2 >= r_in^2 & d2 <= r_out^2` on the integer offsets of
/// a `(2 * floor(r_out) + 1)^2` box -- BOTH bounds inclusive, as
/// `greens.field_score` builds it (`rr0 = 3, rr1 = 11` at 8 m; pass
/// `r_in = 25 / 8` to get `surround_relief`'s metric ring, whose lower
/// bound is `d >= 25 m`).
///
/// SciPy's `percentile_filter` is an ORDER STATISTIC, not NumPy's linear
/// interpolation: `rank = int(size * pct / 100)` (`size - 1` at 100), and
/// the output is the `rank`-th smallest footprint value (verified against
/// SciPy 1.16). That is what the prototype computed, so that is what this
/// does; `surround_relief` (per-candidate `np.percentile`) is the other
/// convention and is served by [`percentile`].
///
/// Per-cell gather + `select_nth_unstable` (O(size) per cell). The only
/// caller runs it twice over the window + 120 m halo + 11-cell margin:
/// ~235 x 170 = 40 k cells x ~350 ring cells = 14 M element-steps per
/// call, a few milliseconds.
pub fn percentile_filter_ring(a: &[f64], nx: usize, ny: usize, r_in: f64, r_out: f64, pct: f64) -> Vec<f64> {
    assert_eq!(a.len(), nx * ny);
    assert!((0.0..=100.0).contains(&pct), "percentile out of range");
    let rr = r_out.floor() as isize;
    let mut offs: Vec<(isize, isize)> = Vec::new();
    for dy in -rr..=rr {
        for dx in -rr..=rr {
            let d2 = (dy * dy + dx * dx) as f64;
            if d2 >= r_in * r_in && d2 <= r_out * r_out {
                offs.push((dy, dx));
            }
        }
    }
    let size = offs.len();
    assert!(size > 0, "empty ring footprint");
    // SciPy `_rank_filter`: rank = int(size * pct / 100), size - 1 at 100
    let rank = if pct == 100.0 { size - 1 } else { (size as f64 * pct / 100.0) as usize };
    let mut out = vec![0.0; nx * ny];
    out.par_chunks_mut(nx).enumerate().for_each(|(y, row)| {
        let mut buf = vec![0.0f64; size];
        for x in 0..nx {
            for (k, &(dy, dx)) in offs.iter().enumerate() {
                let yy = nearest(y as isize + dy, ny);
                let xx = nearest(x as isize + dx, nx);
                buf[k] = a[yy * nx + xx];
            }
            let (_, v, _) = buf.select_nth_unstable_by(rank, |p, q| p.partial_cmp(q).unwrap());
            row[x] = *v;
        }
    });
    out
}

// ---------------------------------------------------------------------------
// numpy

/// `np.gradient(a, cell)` -> `(gy, gx)`: central differences inside,
/// first-order one-sided at the borders (`edge_order=1`). A 1-cell axis
/// gets zero (NumPy would raise).
pub fn gradient(a: &[f64], nx: usize, ny: usize, cell: f64) -> (Vec<f64>, Vec<f64>) {
    assert_eq!(a.len(), nx * ny);
    let mut gy = vec![0.0; nx * ny];
    let mut gx = vec![0.0; nx * ny];
    let two = 2.0 * cell;
    gy.par_chunks_mut(nx).zip(gx.par_chunks_mut(nx)).enumerate().for_each(|(y, (oy, ox))| {
        let row = &a[y * nx..(y + 1) * nx];
        if ny >= 2 {
            let (up, dn, den) = if y == 0 {
                (0, 1, cell)
            } else if y == ny - 1 {
                (ny - 2, ny - 1, cell)
            } else {
                (y - 1, y + 1, two)
            };
            for x in 0..nx {
                oy[x] = (a[dn * nx + x] - a[up * nx + x]) / den;
            }
        }
        if nx >= 2 {
            ox[0] = (row[1] - row[0]) / cell;
            ox[nx - 1] = (row[nx - 1] - row[nx - 2]) / cell;
            for x in 1..nx - 1 {
                ox[x] = (row[x + 1] - row[x - 1]) / two;
            }
        }
    });
    (gy, gx)
}

/// `np.percentile(vals, p)` with NumPy's default `linear` method, formed
/// exactly as NumPy forms it (virtual index `n*q + (1 - q) - 1`, the
/// monotone `_lerp`: `a + (b-a)*g` below `g = 0.5`, `b - (b-a)*(1-g)`
/// above). `vals` need not be sorted (a copy is sorted). NaN-free input;
/// empty input returns NaN.
pub fn percentile(vals: &[f64], p: f64) -> f64 {
    let n = vals.len();
    if n == 0 {
        return f64::NAN;
    }
    let mut v = vals.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = p / 100.0;
    let virt = n as f64 * q + (1.0 - q) - 1.0;
    let (lo, hi, g) = if virt >= (n - 1) as f64 {
        (n - 1, n - 1, virt - virt.floor())
    } else if virt < 0.0 {
        (0, 0, virt - virt.floor())
    } else {
        let f = virt.floor();
        (f as usize, f as usize + 1, virt - f)
    };
    let (a, b) = (v[lo], v[hi]);
    let d = b - a;
    if g >= 0.5 {
        b - d * (1.0 - g)
    } else {
        a + d * g
    }
}

/// Percentile RANK of every cell among all cells, in `[0, 1]`, exactly as
/// `siting.build_fields` forms `relief_pos`:
/// `order = argsort(a, kind="stable"); rank[order] = arange(n) / max(n - 1, 1)`.
/// Ties therefore get DISTINCT ranks in raster order (the stable sort),
/// not a shared mid-rank -- that is the prototype's convention and the
/// one matched here.
pub fn percentile_rank(a: &[f64]) -> Vec<f64> {
    let n = a.len();
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|&i, &j| a[i].partial_cmp(&a[j]).unwrap_or(std::cmp::Ordering::Equal));
    let den = (n.max(2) - 1) as f64;
    let mut out = vec![0.0; n];
    for (r, &i) in order.iter().enumerate() {
        out[i] = r as f64 / den;
    }
    out
}

/// Summed-area table with a leading zero row and column: `sat[(y+1)*(nx+1)
/// + (x+1)] = sum of a[..=y, ..=x]`, accumulated as `siting._sat` does
/// (`cumsum` down axis 0, then along axis 1) so the rounding matches.
pub fn sat(a: &[f64], nx: usize, ny: usize) -> Vec<f64> {
    assert_eq!(a.len(), nx * ny);
    let w = nx + 1;
    let mut s = vec![0.0; (ny + 1) * w];
    // cumsum along axis 0
    let mut col = vec![0.0; nx];
    for y in 0..ny {
        for x in 0..nx {
            col[x] += a[y * nx + x];
            s[(y + 1) * w + x + 1] = col[x];
        }
    }
    // cumsum along axis 1
    for y in 1..=ny {
        let row = &mut s[y * w..(y + 1) * w];
        for x in 2..=nx {
            row[x] += row[x - 1];
        }
    }
    s
}

/// Mean over the `h x w` window whose top-left cell is `(y0, x0)`, from a
/// `sat` table (`siting._win_mean`: `(A - B - C + D) / (h * w)` in that
/// order).
pub fn win_mean(s: &[f64], nx: usize, y0: usize, x0: usize, h: usize, w: usize) -> f64 {
    let sw = nx + 1;
    let a = s[(y0 + h) * sw + x0 + w];
    let b = s[y0 * sw + x0 + w];
    let c = s[(y0 + h) * sw + x0];
    let d = s[y0 * sw + x0];
    (a - b - c + d) / (h * w) as f64
}

/// Std of each `k x k` block of the 2 m grid (`siting.block_reduce_std`),
/// population std (`ddof = 0`); the 8 m `subgrid_rough`. Returns
/// `(std, nx_out, ny_out)` with `ny_out = ny2 / k`, `nx_out = nx2 / k`
/// (the trailing partial blocks dropped, as the reshape does).
pub fn block_reduce_std(z2: &[f64], nx2: usize, ny2: usize, k: usize) -> (Vec<f64>, usize, usize) {
    assert_eq!(z2.len(), nx2 * ny2);
    assert!(k >= 1);
    let (ny, nx) = (ny2 / k, nx2 / k);
    let cnt = (k * k) as f64;
    let mut out = vec![0.0; nx * ny];
    out.par_chunks_mut(nx.max(1)).enumerate().for_each(|(by, row)| {
        for (bx, o) in row.iter_mut().enumerate() {
            let mut sum = 0.0;
            for dy in 0..k {
                for dx in 0..k {
                    sum += z2[(by * k + dy) * nx2 + bx * k + dx];
                }
            }
            let mean = sum / cnt;
            let mut ss = 0.0;
            for dy in 0..k {
                for dx in 0..k {
                    let d = z2[(by * k + dy) * nx2 + bx * k + dx] - mean;
                    ss += d * d;
                }
            }
            *o = (ss / cnt).sqrt();
        }
    });
    (out, nx, ny)
}

/// Least-squares plane `z = a + b*y + c*x` through the samples, returning
/// `(a, b, c)`: the normal equations solved on mean-centred coordinates
/// (a 2x2 system for the slopes, the intercept from the means), which is
/// the same plane `np.linalg.lstsq` returns on a 3-column design matrix
/// for any well-conditioned input (`greens.confirm_2m`). Fewer than three
/// samples, or collinear ones (singular 2x2), return the mean height and
/// zero slopes.
pub fn plane_fit(ys: &[f64], xs: &[f64], zs: &[f64]) -> (f64, f64, f64) {
    let n = zs.len();
    assert!(ys.len() == n && xs.len() == n);
    if n == 0 {
        return (0.0, 0.0, 0.0);
    }
    let nf = n as f64;
    let my = ys.iter().sum::<f64>() / nf;
    let mx = xs.iter().sum::<f64>() / nf;
    let mz = zs.iter().sum::<f64>() / nf;
    let (mut syy, mut sxx, mut sxy, mut syz, mut sxz) = (0.0, 0.0, 0.0, 0.0, 0.0);
    for i in 0..n {
        let (dy, dx, dz) = (ys[i] - my, xs[i] - mx, zs[i] - mz);
        syy += dy * dy;
        sxx += dx * dx;
        sxy += dx * dy;
        syz += dy * dz;
        sxz += dx * dz;
    }
    let det = syy * sxx - sxy * sxy;
    let scale = syy * sxx;
    if n < 3 || det.abs() <= 1e-12 * scale.max(f64::MIN_POSITIVE) {
        return (mz, 0.0, 0.0);
    }
    let b = (syz * sxx - sxz * sxy) / det;
    let c = (sxz * syy - syz * sxy) / det;
    (mz - b * my - c * mx, b, c)
}

/// Bilinear sample of a row-major grid at fractional cell coordinates
/// `(fy, fx)`, clamped to the grid (the prototype's `map_coordinates(order=1,
/// mode="nearest")` where it appears: outside the grid the nearest edge
/// value, which clamping reproduces exactly).
pub fn bilinear(a: &[f64], nx: usize, ny: usize, fy: f64, fx: f64) -> f64 {
    assert_eq!(a.len(), nx * ny);
    let fy = fy.max(0.0).min((ny - 1) as f64);
    let fx = fx.max(0.0).min((nx - 1) as f64);
    let y0 = fy.floor() as usize;
    let x0 = fx.floor() as usize;
    let y1 = (y0 + 1).min(ny - 1);
    let x1 = (x0 + 1).min(nx - 1);
    let ty = fy - y0 as f64;
    let tx = fx - x0 as f64;
    let top = a[y0 * nx + x0] * (1.0 - tx) + a[y0 * nx + x1] * tx;
    let bot = a[y1 * nx + x0] * (1.0 - tx) + a[y1 * nx + x1] * tx;
    top * (1.0 - ty) + bot * ty
}

// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: &[f64], b: &[f64], tol: f64) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(x, y)| (x.is_infinite() && y.is_infinite()) || (x - y).abs() <= tol)
    }

    #[test]
    fn reflect_is_periodic_bounce() {
        // d c b a | a b c d | d c b a   (n = 4)
        let seq: Vec<usize> = (-8..12).map(|i| reflect(i, 4)).collect();
        assert_eq!(seq, vec![0, 1, 2, 3, 3, 2, 1, 0, 0, 1, 2, 3, 3, 2, 1, 0, 0, 1, 2, 3]);
        assert_eq!(reflect(-5, 1), 0);
    }

    #[test]
    fn filt1d_even_window_leans_left() {
        // SciPy: size 4 covers offsets -2..=1; b = [5 1 9 2 7 3] -> [1 1 1 1 2 2]
        let b = [5.0, 1.0, 9.0, 2.0, 7.0, 3.0];
        let mut o = [0.0; 6];
        filt1d(&b, 4, &fmin, &mut o);
        assert_eq!(o, [1.0, 1.0, 1.0, 1.0, 2.0, 2.0]);
        filt1d(&b, 1, &fmin, &mut o);
        assert_eq!(o, b);
        // size wider than the line: min is the global min everywhere
        filt1d(&b, 13, &fmin, &mut o);
        assert!(o.iter().all(|&v| v == 1.0));
    }

    #[test]
    fn edt_hand() {
        // 3 x 4, one FALSE at (1, 1), cell 2: distances in metres
        let mut m = vec![true; 12];
        m[1 * 4 + 1] = false;
        let d = edt(&m, 4, 3, 2.0);
        let e = |dy: f64, dx: f64| (dy * dy + dx * dx).sqrt() * 2.0;
        assert_eq!(d[5], 0.0);
        assert!((d[0] - e(1.0, 1.0)).abs() < 1e-12);
        assert!((d[4] - 2.0).abs() < 1e-12);
        assert!((d[11] - e(1.0, 2.0)).abs() < 1e-12);
        // no FALSE -> infinity; all FALSE -> zero
        assert!(edt(&vec![true; 6], 3, 2, 8.0).iter().all(|v| v.is_infinite()));
        assert!(edt(&vec![false; 6], 3, 2, 8.0).iter().all(|&v| v == 0.0));
        // a column with no FALSE anywhere is still finite via other columns
        let m: Vec<bool> = (0..9).map(|i| i != 4).collect();
        let d = edt(&m, 3, 3, 1.0);
        assert!((d[0] - 2f64.sqrt()).abs() < 1e-12 && d[1] == 1.0);
    }

    #[test]
    fn edt_matches_brute_force() {
        // deterministic LCG masks vs O(n^2) brute force, several shapes
        let mut s = 12345u64;
        let mut rnd = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (s >> 33) as f64 / (1u64 << 31) as f64
        };
        for &(nx, ny) in &[(7usize, 9usize), (1, 13), (13, 1), (20, 17)] {
            let m: Vec<bool> = (0..nx * ny).map(|_| rnd() < 0.85).collect();
            let d = edt(&m, nx, ny, 8.0);
            for y in 0..ny {
                for x in 0..nx {
                    let mut best = f64::INFINITY;
                    for yy in 0..ny {
                        for xx in 0..nx {
                            if !m[yy * nx + xx] {
                                let (dy, dx) = ((yy as f64 - y as f64) * 8.0, (xx as f64 - x as f64) * 8.0);
                                best = best.min((dy * dy + dx * dx).sqrt());
                            }
                        }
                    }
                    let got = d[y * nx + x];
                    assert!((got - best).abs() < 1e-9 || (got.is_infinite() && best.is_infinite()), "{nx}x{ny} at ({y},{x}): {got} vs {best}");
                }
            }
        }
    }

    #[test]
    fn rect_and_disc_hand() {
        // 3 x 3 grid, values = index
        let a: Vec<f64> = (0..9).map(|i| i as f64).collect();
        let mx = max_filter_rect(&a, 3, 3, 3, 3);
        // centre sees everything; corner (0,0) reflect: rows -1->0, cols -1->0 => max over rows 0..1, cols 0..1 = 4
        assert_eq!(mx[4], 8.0);
        assert_eq!(mx[0], 4.0);
        let mn = min_filter_disc(&a, 3, 3, 1);
        // disc r=1 at centre: {1,3,4,5,7} -> 1; at (0,0): (-1,0)->(0,0),(0,-1)->(0,0),(1,0)=3,(0,1)=1 -> 0
        assert_eq!(mn[4], 1.0);
        assert_eq!(mn[0], 0.0);
        // disc r=2 covers 13 cells: the r=2 row half-widths are 0,2,2,2,0
        assert_eq!((0..=2).map(|dy| isqrt(4 - dy * dy)).collect::<Vec<_>>(), vec![2, 1, 0]);
        assert_eq!(isqrt(0), 0);
        assert_eq!(isqrt(15), 3);
        assert_eq!(isqrt(16), 4);
    }

    #[test]
    fn disc_matches_brute_force_reflect() {
        let mut s = 99u64;
        let mut rnd = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((s >> 33) % 7) as f64
        };
        for &r in &[1usize, 2, 4, 9, 16] {
            for &(nx, ny) in &[(7usize, 5usize), (12, 12)] {
                let a: Vec<f64> = (0..nx * ny).map(|_| rnd()).collect();
                let got_min = min_filter_disc(&a, nx, ny, r);
                let got_max = max_filter_disc(&a, nx, ny, r);
                let m: Vec<bool> = a.iter().map(|&v| v > 1.0).collect();
                let got_er = erode_disc(&m, nx, ny, r);
                let ri = r as isize;
                for y in 0..ny {
                    for x in 0..nx {
                        let (mut lo, mut hi, mut all) = (f64::INFINITY, f64::NEG_INFINITY, true);
                        for dy in -ri..=ri {
                            for dx in -ri..=ri {
                                if dy * dy + dx * dx <= ri * ri {
                                    let i = reflect(y as isize + dy, ny) * nx + reflect(x as isize + dx, nx);
                                    lo = lo.min(a[i]);
                                    hi = hi.max(a[i]);
                                    all &= m[i];
                                }
                            }
                        }
                        assert_eq!(got_min[y * nx + x], lo, "min r={r} {nx}x{ny} ({y},{x})");
                        assert_eq!(got_max[y * nx + x], hi, "max r={r} {nx}x{ny} ({y},{x})");
                        assert_eq!(got_er[y * nx + x], all, "erode r={r} {nx}x{ny} ({y},{x})");
                    }
                }
            }
        }
    }

    #[test]
    fn uniform_and_gaussian_hand() {
        // SciPy: uniform_filter1d([5 1 9 2 7 3], 4) = [3, 5, 4.25, 4.75, 5.25, 3.75]
        let b = [5.0, 1.0, 9.0, 2.0, 7.0, 3.0];
        let u = uniform_filter(&b, 6, 1, 4);
        assert!(close(&u, &[3.0, 5.0, 4.25, 4.75, 5.25, 3.75], 1e-12));
        // constant field is a fixed point of both
        let c = vec![2.5; 20];
        assert!(close(&uniform_filter(&c, 5, 4, 3), &c, 1e-12));
        assert!(close(&gaussian_filter(&c, 5, 4, 1.7), &c, 1e-12));
        // gaussian weights sum to one: an impulse's response sums to one
        let mut imp = vec![0.0; 21 * 21];
        imp[10 * 21 + 10] = 1.0;
        let g = gaussian_filter(&imp, 21, 21, 1.0);
        assert!((g.iter().sum::<f64>() - 1.0).abs() < 1e-12);
        assert!((g[10 * 21 + 10] - 0.15915494).abs() < 1e-6); // 1/(2 pi) for sigma 1
    }

    #[test]
    fn percentile_ring_hand() {
        // 1 x 5 line, ring 0 <= rho <= 1 (the 3-cross, but 1 row) mode nearest
        let a = [1.0, 5.0, 2.0, 4.0, 3.0];
        // footprint (2*1+1)^2 box with d2 <= 1: (0,0),(0,+-1),(+-1,0) = 5 cells; rows clamp to row 0
        // at x=0: values {1(x-1->0),1,5,1,1} sorted [1,1,1,1,5]; pct 90 -> rank int(4.5)=4 -> 5
        let p = percentile_filter_ring(&a, 5, 1, 0.0, 1.0, 90.0);
        assert_eq!(p[0], 5.0);
        // pct 50 -> rank 2 -> 1 ; at x=2: {5,2,4,2,2} sorted [2,2,2,4,5] rank 2 -> 2
        let p = percentile_filter_ring(&a, 5, 1, 0.0, 1.0, 50.0);
        assert_eq!(p[0], 1.0);
        assert_eq!(p[2], 2.0);
        // pct 100 -> rank size-1 -> max
        let p = percentile_filter_ring(&a, 5, 1, 0.0, 1.0, 100.0);
        assert_eq!(p[2], 5.0);
    }

    #[test]
    fn gradient_hand() {
        let a = [0.0, 1.0, 4.0, 9.0, 2.0, 3.0, 6.0, 11.0];
        let (gy, gx) = gradient(&a, 4, 2, 2.0);
        assert!(close(&gx[..4], &[0.5, 1.0, 2.0, 2.5], 1e-12));
        assert!(gy.iter().all(|&v| (v - 1.0).abs() < 1e-12));
    }

    #[test]
    fn percentile_hand() {
        let v = [3.0, 1.0, 2.0, 5.0];
        assert_eq!(percentile(&v, 0.0), 1.0);
        assert_eq!(percentile(&v, 100.0), 5.0);
        assert!((percentile(&v, 50.0) - 2.5).abs() < 1e-12);
        assert!((percentile(&v, 25.0) - 1.75).abs() < 1e-12);
        assert_eq!(percentile(&[7.0], 33.0), 7.0);
        assert!(percentile(&[], 50.0).is_nan());
    }

    #[test]
    fn percentile_rank_is_stable_argsort() {
        // ties get distinct ranks in raster order
        let a = [2.0, 1.0, 2.0, 0.0];
        let r = percentile_rank(&a);
        assert!(close(&r, &[2.0 / 3.0, 1.0 / 3.0, 1.0, 0.0], 1e-12));
        assert_eq!(percentile_rank(&[4.0]), vec![0.0]);
        assert!(close(&percentile_rank(&[1.0, 1.0, 1.0]), &[0.0, 0.5, 1.0], 1e-12));
    }

    #[test]
    fn sat_win_mean_block_std_hand() {
        let a: Vec<f64> = (1..=6).map(|i| i as f64).collect(); // 2 x 3
        let s = sat(&a, 3, 2);
        assert_eq!(s.len(), 12);
        assert_eq!(s[11], 21.0); // full sum
        assert_eq!(s[1 * 4 + 2], 3.0); // a[0,0]+a[0,1]
        assert!((win_mean(&s, 3, 0, 1, 2, 2) - 4.0).abs() < 1e-12); // {2,3,5,6}
        assert!((win_mean(&s, 3, 1, 0, 1, 3) - 5.0).abs() < 1e-12);
        // block std: 4 x 4 grid, k = 2, block values {0,0,2,2} -> std 1
        let z: Vec<f64> = (0..16).map(|i| if (i / 4) % 2 == 0 { 0.0 } else { 2.0 }).collect();
        let (b, nx, ny) = block_reduce_std(&z, 4, 4, 2);
        assert_eq!((nx, ny), (2, 2));
        assert!(b.iter().all(|&v| (v - 1.0).abs() < 1e-12));
        // odd sizes drop the partial block
        let (_, nx, ny) = block_reduce_std(&vec![0.0; 9 * 11], 11, 9, 4);
        assert_eq!((nx, ny), (2, 2));
    }

    #[test]
    fn plane_fit_recovers_exact_plane() {
        let ys = [0.0, 1.0, 2.0, 0.0, 5.0, 3.0];
        let xs = [0.0, 3.0, 1.0, 4.0, 2.0, 3.0];
        let zs: Vec<f64> = ys.iter().zip(&xs).map(|(y, x)| 1.5 + 0.03 * y - 0.05 * x).collect();
        let (a, b, c) = plane_fit(&ys, &xs, &zs);
        assert!((a - 1.5).abs() < 1e-12 && (b - 0.03).abs() < 1e-12 && (c + 0.05).abs() < 1e-12);
        // collinear samples: mean height, zero slopes
        let (a, b, c) = plane_fit(&[0.0, 1.0, 2.0], &[0.0, 1.0, 2.0], &[1.0, 2.0, 3.0]);
        assert_eq!((a, b, c), (2.0, 0.0, 0.0));
    }

    #[test]
    fn bilinear_hand() {
        let a = [0.0, 1.0, 2.0, 10.0, 11.0, 12.0]; // 2 x 3, a = 10 y + x
        assert!((bilinear(&a, 3, 2, 0.5, 1.5) - 6.5).abs() < 1e-12);
        assert_eq!(bilinear(&a, 3, 2, -3.0, 9.0), 2.0); // clamped -> nearest edge value
        assert_eq!(bilinear(&a, 3, 2, 1.0, 2.0), 12.0);
    }

    // ---- SciPy reference battery (needs kernels_ref.json from gen_ref.py) --

    fn ref_cases() -> Vec<serde_json::Value> {
        let path = std::env::var("COURSE_ROUTING_KERNEL_REF").expect("COURSE_ROUTING_KERNEL_REF");
        let txt = std::fs::read_to_string(path).expect("read reference json");
        serde_json::from_str(&txt).expect("parse")
    }

    fn f(v: &serde_json::Value) -> f64 {
        match v {
            serde_json::Value::String(s) if s == "inf" => f64::INFINITY,
            serde_json::Value::String(s) if s == "nan" => f64::NAN,
            _ => v.as_f64().unwrap(),
        }
    }

    /// A 2-D (rows) or 1-D JSON list, flattened row-major.
    fn grid_f(v: &serde_json::Value) -> Vec<f64> {
        v.as_array()
            .unwrap()
            .iter()
            .flat_map(|r| match r.as_array() {
                Some(row) => row.iter().map(f).collect::<Vec<_>>(),
                None => vec![f(r)],
            })
            .collect()
    }

    fn grid_b(v: &serde_json::Value) -> Vec<bool> {
        v.as_array().unwrap().iter().flat_map(|r| r.as_array().unwrap().iter().map(|x| x.as_i64().unwrap() != 0)).collect()
    }

    fn u(v: &serde_json::Value) -> usize {
        v.as_u64().unwrap() as usize
    }

    fn assert_close(name: &str, got: &[f64], want: &[f64], tol: f64) {
        assert_eq!(got.len(), want.len(), "{name}: length");
        for (i, (g, w)) in got.iter().zip(want).enumerate() {
            let ok = (g.is_infinite() && w.is_infinite()) || (g - w).abs() <= tol;
            assert!(ok, "{name}: at {i} got {g} want {w}");
        }
    }

    /// Timings on the prototype's grid sizes (376^2 at 8 m, 1504^2 at 2 m);
    /// `cargo test -p course-routing --release timings -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn timings() {
        use std::time::Instant;
        let mut s = 7u64;
        let mut rnd = || {
            s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            (s >> 33) as f64 / (1u64 << 31) as f64
        };
        for &(n, cell) in &[(376usize, 8.0), (1504usize, 2.0)] {
            let a: Vec<f64> = (0..n * n).map(|_| rnd() * 10.0).collect();
            let m: Vec<bool> = a.iter().map(|&v| v > 0.3).collect();
            let time = |name: &str, f: &dyn Fn()| {
                f(); // warm
                let t = Instant::now();
                let reps = 5;
                for _ in 0..reps {
                    f();
                }
                eprintln!("{n}x{n} {name}: {:.2} ms", t.elapsed().as_secs_f64() * 1e3 / reps as f64);
            };
            time("edt", &|| {
                std::hint::black_box(edt(&m, n, n, cell));
            });
            if n == 376 {
                for &r in &[1usize, 2, 15, 16] {
                    time(&format!("min_filter_disc r={r}"), &|| {
                        std::hint::black_box(min_filter_disc(&a, n, n, r));
                    });
                    time(&format!("erode_disc r={r}"), &|| {
                        std::hint::black_box(erode_disc(&m, n, n, r));
                    });
                }
                time("max_filter_rect 182x119", &|| {
                    std::hint::black_box(max_filter_rect(&a, n, n, 182, 119));
                });
                time("uniform_filter 25", &|| {
                    std::hint::black_box(uniform_filter(&a, n, n, 25));
                });
                time("gaussian_filter 15.9", &|| {
                    std::hint::black_box(gaussian_filter(&a, n, n, 400.0 / std::f64::consts::PI / 8.0));
                });
                time("percentile_filter_ring 3..11 (235x170)", &|| {
                    std::hint::black_box(percentile_filter_ring(&a[..235 * 170], 170, 235, 3.0, 11.0, 90.0));
                });
            }
        }
    }

    #[test]
    #[ignore]
    fn scipy_reference_battery() {
        let cases = ref_cases();
        let mut n = 0;
        for c in &cases {
            let kind = c["kind"].as_str().unwrap();
            let name = format!("{kind}#{n}");
            match kind {
                "edt" => {
                    let (nx, ny) = (u(&c["nx"]), u(&c["ny"]));
                    let got = edt(&grid_b(&c["mask"]), nx, ny, f(&c["cell"]));
                    assert_close(&name, &got, &grid_f(&c["expect"]), 1e-9);
                }
                "edt_allfalse_is_inf" => {
                    let got = edt(&grid_b(&c["mask"]), u(&c["nx"]), u(&c["ny"]), f(&c["cell"]));
                    assert!(got.iter().all(|v| v.is_infinite()), "{name}");
                }
                "min_rect" | "max_rect" => {
                    let (nx, ny, h, w) = (u(&c["nx"]), u(&c["ny"]), u(&c["h"]), u(&c["w"]));
                    let a = grid_f(&c["a"]);
                    let got = if kind == "min_rect" { min_filter_rect(&a, nx, ny, h, w) } else { max_filter_rect(&a, nx, ny, h, w) };
                    assert_close(&name, &got, &grid_f(&c["expect"]), 0.0);
                }
                "min_disc" | "max_disc" => {
                    let (nx, ny, r) = (u(&c["nx"]), u(&c["ny"]), u(&c["r"]));
                    let a = grid_f(&c["a"]);
                    let got = if kind == "min_disc" { min_filter_disc(&a, nx, ny, r) } else { max_filter_disc(&a, nx, ny, r) };
                    assert_close(&name, &got, &grid_f(&c["expect"]), 0.0);
                }
                "erode_disc" => {
                    let got = erode_disc(&grid_b(&c["mask"]), u(&c["nx"]), u(&c["ny"]), u(&c["r"]));
                    assert_eq!(got, grid_b(&c["expect"]), "{name}");
                }
                "uniform" => {
                    let got = uniform_filter(&grid_f(&c["a"]), u(&c["nx"]), u(&c["ny"]), u(&c["size"]));
                    assert_close(&name, &got, &grid_f(&c["expect"]), 1e-9);
                }
                "gaussian" => {
                    let got = gaussian_filter(&grid_f(&c["a"]), u(&c["nx"]), u(&c["ny"]), f(&c["sigma"]));
                    assert_close(&name, &got, &grid_f(&c["expect"]), 1e-9);
                }
                "pct_ring" => {
                    let got = percentile_filter_ring(&grid_f(&c["a"]), u(&c["nx"]), u(&c["ny"]), f(&c["r_in"]), f(&c["r_out"]), f(&c["pct"]));
                    assert_close(&name, &got, &grid_f(&c["expect"]), 0.0);
                }
                "gradient" => {
                    let (gy, gx) = gradient(&grid_f(&c["a"]), u(&c["nx"]), u(&c["ny"]), f(&c["cell"]));
                    assert_close(&name, &gy, &grid_f(&c["gy"]), 1e-9);
                    assert_close(&name, &gx, &grid_f(&c["gx"]), 1e-9);
                }
                "percentile" => {
                    let vals: Vec<f64> = c["vals"].as_array().unwrap().iter().map(f).collect();
                    let got = percentile(&vals, f(&c["p"]));
                    assert!((got - f(&c["expect"])).abs() <= 1e-9, "{name}: {got} vs {}", f(&c["expect"]));
                }
                "percentile_rank" => {
                    let got = percentile_rank(&grid_f(&c["a"]));
                    assert_close(&name, &got, &grid_f(&c["expect"]), 1e-12);
                }
                "sat" => {
                    let (nx, ny) = (u(&c["nx"]), u(&c["ny"]));
                    let s = sat(&grid_f(&c["a"]), nx, ny);
                    assert_close(&name, &s, &grid_f(&c["expect"]), 1e-9);
                    let (h, w) = (u(&c["h"]), u(&c["w"]));
                    let want = grid_f(&c["win_mean"]);
                    let mut got = Vec::new();
                    for y0 in 0..=ny - h {
                        for x0 in 0..=nx - w {
                            got.push(win_mean(&s, nx, y0, x0, h, w));
                        }
                    }
                    assert_close(&name, &got, &want, 1e-9);
                }
                "block_std" => {
                    let (b, nx, ny) = block_reduce_std(&grid_f(&c["a"]), u(&c["nx2"]), u(&c["ny2"]), u(&c["k"]));
                    assert_eq!((nx, ny), (u(&c["nx"]), u(&c["ny"])), "{name}");
                    assert_close(&name, &b, &grid_f(&c["expect"]), 1e-9);
                }
                "plane_fit" => {
                    let g = |k: &str| -> Vec<f64> { c[k].as_array().unwrap().iter().map(f).collect() };
                    let (a, b, cc) = plane_fit(&g("ys"), &g("xs"), &g("zs"));
                    let e = g("expect");
                    assert!((a - e[0]).abs() < 1e-8 && (b - e[1]).abs() < 1e-10 && (cc - e[2]).abs() < 1e-10, "{name}: {:?} vs {:?}", (a, b, cc), e);
                }
                "bilinear" => {
                    let a = grid_f(&c["a"]);
                    let (nx, ny) = (u(&c["nx"]), u(&c["ny"]));
                    let want: Vec<f64> = c["expect"].as_array().unwrap().iter().map(f).collect();
                    let got: Vec<f64> = c["pts"].as_array().unwrap().iter().map(|p| bilinear(&a, nx, ny, f(&p[0]), f(&p[1]))).collect();
                    assert_close(&name, &got, &want, 1e-9);
                }
                _ => continue, // geom kinds live in geom.rs
            }
            n += 1;
        }
        assert!(n >= 80, "only {n} img cases ran");
        eprintln!("img battery: {n} cases matched SciPy/NumPy");
    }
}
