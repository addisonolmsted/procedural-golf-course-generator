//! X2 — the assembler: the surface IS the extraction's own representation.
//!
//! ```text
//!     z(p) = B(p) + H(u, W) + R(p)
//! ```
//!
//! * **B** — bed field: the exponentially weighted mean of nearby channel
//!   bed elevations. Smooth everywhere, so no attribution facets, and
//!   unbiased near a channel, unlike the harmonic diffusion first tried —
//!   that averaged in UPSTREAM beds, which are higher, and floated every
//!   divide up (measured: relief 30% over real, the excess on the crests).
//!
//! * **H** — the measured height-above-drainage profile, on the NORMALISED
//!   valley coordinate `u = d/(d+m)` (d to the nearest channel, m to the
//!   divide), with amplitude conditioned on the local half-width `W = d+m`.
//!   Shipped as `assets/sandhills_hand_profile.txt`, measured over the 30
//!   kept Carolina tiles by `tools/aeolian/build_handpack.py`.
//!
//!   Absolute distance was the wrong axis and failed twice: it over-built
//!   the headwaters (at fixed distance the median comes from wide main
//!   valleys, but crowded channels have no room for that height), and it
//!   destroyed the SHOULDER — the convex break where a flat interfluve
//!   turns down into its valley — because that break sits at a different
//!   distance in every valley, so pooling smeared it into a ramp. A valley
//!   edge IS a slope break; without it the drainage stopped reading. On the
//!   u axis every valley shares its shoulder, so the median keeps it, and
//!   the profile flattens at u→1, which rounds the divide crest from the
//!   data — real Carolina crests carry exactly the curvature of the ground
//!   beside them (measured 0.093 vs 0.093).
//!
//! * **R** — a modest residual calibrated to the measured per-(u,W) spread,
//!   wall-weighted. It is NOT asked to invent gullies: isotropic noise at
//!   the correct amplitude reads as mottling, and streaking it along the
//!   fall line prints starbursts (that field diverges at divides). Fine
//!   drainage is GEOMETRY — the tier-5 gullies and fingers in the network.
//!
//! The whole prototype history is in `tools/aeolian/build_handpack.py`,
//! which also runs the leave-one-out reconstruction that chose this
//! representation over the patch-quilt one.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::noise;

use crate::draw::Descriptors;
use crate::wind::macro_spec;

/// The measured profile table (`HTAB1`).
pub struct HandProfile {
    u_bins: usize,
    /// Half-width band centres, metres.
    w_centers: Vec<f64>,
    /// `h[band][u]` — median HAND, metres.
    h: Vec<Vec<f64>>,
    /// `s[band][u]` — residual spread, metres.
    s: Vec<Vec<f64>>,
}

impl HandProfile {
    pub fn load(path: &std::path::Path) -> std::io::Result<HandProfile> {
        let txt = std::fs::read_to_string(path)?;
        let mut u_bins = 24usize;
        let mut w_centers = Vec::new();
        let (mut h, mut s) = (Vec::new(), Vec::new());
        for line in txt.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line == "HTAB1" {
                continue;
            }
            let mut it = line.split_whitespace();
            match it.next() {
                Some("u_bins") => u_bins = it.next().unwrap().parse().unwrap(),
                Some("w_centers") => {
                    w_centers = it.map(|v| v.parse().unwrap()).collect()
                }
                Some("h") => h.push(it.map(|v| v.parse().unwrap()).collect()),
                Some("s") => s.push(it.map(|v| v.parse().unwrap()).collect()),
                _ => {}
            }
        }
        Ok(HandProfile { u_bins, w_centers, h, s })
    }

    /// Bilinear in (u, W) — continuous in both, so no band seams.
    fn eval(tab: &[Vec<f64>], centers: &[f64], u_bins: usize, u: f64, w: f64) -> f64 {
        let wc = w.clamp(centers[0], centers[centers.len() - 1]);
        let hi = centers.partition_point(|c| *c < wc).clamp(1, centers.len() - 1);
        let lo = hi - 1;
        let t = (wc - centers[lo]) / (centers[hi] - centers[lo]).max(1e-9);
        let x = u.clamp(0.0, 1.0) * u_bins as f64;
        let i0 = (x as usize).min(u_bins - 1);
        let fx = x - i0 as f64;
        let row = |b: usize| tab[b][i0] * (1.0 - fx) + tab[b][i0 + 1] * fx;
        row(lo) * (1.0 - t) + row(hi) * t
    }

    pub fn height(&self, u: f64, w: f64) -> f64 {
        Self::eval(&self.h, &self.w_centers, self.u_bins, u, w)
    }
    pub fn spread(&self, u: f64, w: f64) -> f64 {
        Self::eval(&self.s, &self.w_centers, self.u_bins, u, w)
    }
}

/// Everything the assembler derives from the network, kept for later stages
/// (texture grain, water, gates).
pub struct Assembled {
    pub height: Grid<f64>,
    /// Distance to the nearest channel, metres.
    pub dist: Grid<f64>,
    /// Normalised valley coordinate: 0 on a channel, 1 on a divide.
    pub u: Grid<f64>,
    /// Local valley half-width, metres.
    pub w: Grid<f64>,
    /// Bed field.
    pub bed: Grid<f64>,
}

const CELL: f64 = 8.0;

/// Two-pass chamfer distance with source carry.
fn chamfer(spec: course_world::grid::GridSpec, seed: &[(u32, u32, f64)])
    -> (Vec<f64>, Vec<f64>) {
    let big = 1e18f64;
    let mut d = vec![big; spec.len()];
    let mut v = vec![0.0f64; spec.len()];
    for &(x, y, val) in seed {
        let i = spec.index(x, y);
        d[i] = 0.0;
        v[i] = val;
    }
    // Borgefors 5x5 weights (5, 7, 11)/5: the 3x3 chamfer is ~5%
    // anisotropic and its error printed as polygonal facets on the
    // interfluves and straight filaments along the divides (first render).
    let (o, dg, kn) = (CELL, CELL * 1.4, CELL * 2.2);
    let fwd = [(-1i64, 0i64, o), (0, -1, o), (-1, -1, dg), (1, -1, dg),
               (-2, -1, kn), (-1, -2, kn), (1, -2, kn), (2, -1, kn)];
    let bwd = [(1i64, 0i64, o), (0, 1, o), (1, 1, dg), (-1, 1, dg),
               (2, 1, kn), (1, 2, kn), (-1, 2, kn), (-2, 1, kn)];
    for pass in 0..2 {
        let ys: Vec<u32> = if pass == 0 {
            (0..spec.ny).collect()
        } else {
            (0..spec.ny).rev().collect()
        };
        for &y in &ys {
            let xs: Vec<u32> = if pass == 0 {
                (0..spec.nx).collect()
            } else {
                (0..spec.nx).rev().collect()
            };
            for &x in &xs {
                let i = spec.index(x, y);
                for (dx, dy, w) in if pass == 0 { fwd } else { bwd } {
                    let (px, py) = (x as i64 + dx, y as i64 + dy);
                    if px < 0 || py < 0 || px >= spec.nx as i64 || py >= spec.ny as i64 {
                        continue;
                    }
                    let j = spec.index(px as u32, py as u32);
                    if d[j] + w < d[i] {
                        d[i] = d[j] + w;
                        v[i] = v[j];
                    }
                }
            }
        }
    }
    (d, v)
}

/// Five-point relaxation with EDGE REPLICATION.
///
/// The first version skipped the border ring (`1..ny-1`), so the outermost
/// row and column never relaxed at all: they kept raw nearest-cell values
/// while everything inside smoothed, and the mismatch printed as artifacts
/// right around the tile rim. Every field here — bed, distance, u, w, H —
/// runs through this, so the bug reached all of them.
fn blur(spec: course_world::grid::GridSpec, a: &mut Vec<f64>, passes: usize) {
    // DIAGNOSTIC ONLY (SAND_OLD_BLUR=1): the border-skipping form, kept so
    // the fix can be demonstrated against itself on identical seeds.
    if std::env::var("SAND_OLD_BLUR").is_ok() {
        for _ in 0..passes {
            let src = a.clone();
            for y in 1..spec.ny - 1 {
                for x in 1..spec.nx - 1 {
                    let i = spec.index(x, y);
                    a[i] = 0.2
                        * (src[i]
                            + src[spec.index(x - 1, y)]
                            + src[spec.index(x + 1, y)]
                            + src[spec.index(x, y - 1)]
                            + src[spec.index(x, y + 1)]);
                }
            }
        }
        return;
    }
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    for _ in 0..passes {
        let src = a.clone();
        for y in 0..ny {
            for x in 0..nx {
                let at = |dx: i64, dy: i64| -> f64 {
                    let px = (x + dx).clamp(0, nx - 1) as u32;
                    let py = (y + dy).clamp(0, ny - 1) as u32;
                    src[spec.index(px, py)]
                };
                a[spec.index(x as u32, y as u32)] =
                    0.2 * (at(0, 0) + at(-1, 0) + at(1, 0) + at(0, -1) + at(0, 1));
            }
        }
    }
}

/// Separable box blur — reaches long wavelengths cheaply. The 5-point
/// relaxation needs ~780 passes for a 200 m radius; three box passes get
/// there in three.
fn box_blur(spec: course_world::grid::GridSpec, a: &mut Vec<f64>, r: i64,
            passes: usize) {
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    for _ in 0..passes {
        for axis in 0..2 {
            let src = a.clone();
            for y in 0..ny {
                for x in 0..nx {
                    let mut sum = 0.0;
                    let mut n = 0.0;
                    for k in -r..=r {
                        let (px, py) = if axis == 0 {
                            ((x + k).clamp(0, nx - 1), y)
                        } else {
                            (x, (y + k).clamp(0, ny - 1))
                        };
                        sum += src[spec.index(px as u32, py as u32)];
                        n += 1.0;
                    }
                    a[spec.index(x as u32, y as u32)] = sum / n;
                }
            }
        }
    }
}

/// Limit the bed field's slope (a Lipschitz filter, swept like a chamfer).
///
/// B is the envelope of the valley FLOORS. Two floors 40 m apart cannot
/// differ by 20 m — that is a 60% grade, and no sand landscape holds it.
/// It happened anyway wherever a fine channel was graded beside a much
/// deeper one, and the step printed as a hard point artifact that survived
/// every earlier fix, because the fixes addressed how B is interpolated
/// rather than what it was interpolating BETWEEN. Capping the gradient
/// removes the whole class at once, and it is loose enough (22%) that no
/// real cross-valley bed gradient is touched.
fn slope_limit(spec: course_world::grid::GridSpec, a: &mut Vec<f64>, max_grade: f64) {
    if std::env::var("SAND_NO_SLOPELIMIT").is_ok() {
        return;                                  // DIAGNOSTIC ONLY
    }
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let (o, dg) = (max_grade * CELL, max_grade * CELL * std::f64::consts::SQRT_2);
    for _ in 0..2 {
        for pass in 0..2 {
            let ys: Vec<i64> = if pass == 0 { (0..ny).collect() } else { (0..ny).rev().collect() };
            for &y in &ys {
                let xs: Vec<i64> = if pass == 0 { (0..nx).collect() } else { (0..nx).rev().collect() };
                for &x in &xs {
                    let i = spec.index(x as u32, y as u32);
                    let nb: [(i64, i64, f64); 4] = if pass == 0 {
                        [(-1, 0, o), (0, -1, o), (-1, -1, dg), (1, -1, dg)]
                    } else {
                        [(1, 0, o), (0, 1, o), (1, 1, dg), (-1, 1, dg)]
                    };
                    for (dx, dy, lim) in nb {
                        let (px, py) = (x + dx, y + dy);
                        if px < 0 || py < 0 || px >= nx || py >= ny {
                            continue;
                        }
                        let j = spec.index(px as u32, py as u32);
                        if a[i] > a[j] + lim {
                            a[i] = a[j] + lim;
                        } else if a[i] < a[j] - lim {
                            a[i] = a[j] - lim;
                        }
                    }
                }
            }
        }
    }
}

/// Assemble the 8 m surface from the network and its beds.
pub fn assemble(rng: &mut DetRng, beds: &[(Vec<Vec2>, Vec<f64>)],
                tiers: &[u8], prof: &HandProfile, d: &Descriptors) -> Assembled {
    let spec = macro_spec();

    // --- rasterise channels: nearest-cell bed value ------------------------
    // TWO seed sets. Every channel point seeds the DISTANCE field — that
    // is what cuts valleys. Only the interior of a channel seeds the BED
    // field: a tip pinned to its own bed while its neighbours relax is a
    // point constraint with support on one side, and it stands as a spike
    // (measured, the sharpest cells on a tile clustered within 40 m of
    // channel heads; a row of them read as dark notches along a rim).
    // Fine channels are the worst offenders and get the widest exclusion,
    // since their beds are the least consistent with their surroundings.
    let mut seed: Vec<(u32, u32, f64)> = Vec::new();
    let mut bed_seed: Vec<(u32, u32, f64)> = Vec::new();
    let mut chan = vec![false; spec.len()];
    for (ci, (pts, bed)) in beds.iter().enumerate() {
        let tier = tiers.get(ci).copied().unwrap_or(1);
        let skip = if tier >= 4 { 4 } else { 2 };
        let last = pts.len().saturating_sub(1);
        for k in 0..pts.len().saturating_sub(1) {
            let (a, b) = (pts[k], pts[k + 1]);
            let seg = a.distance(b);
            let n = (seg / (CELL * 0.5)).ceil().max(1.0) as usize;
            for j in 0..=n {
                let t = j as f64 / n as f64;
                let p = Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
                let x = (p.x / CELL).round().clamp(0.0, (spec.nx - 1) as f64) as u32;
                let y = (p.y / CELL).round().clamp(0.0, (spec.ny - 1) as f64) as u32;
                let z = bed[k] + (bed[k + 1] - bed[k]) * t;
                seed.push((x, y, z));
                if k >= skip && k + skip <= last {
                    bed_seed.push((x, y, z));
                }
                chan[spec.index(x, y)] = true;
            }
        }
    }
    if bed_seed.is_empty() {
        bed_seed = seed.clone();
    }

    // --- distance + bed ----------------------------------------------------
    let (dist, _) = chamfer(spec, &seed);
    let (_, bed_near) = chamfer(spec, &bed_seed);

    // B: nearest-channel bed, then relaxed with the channels re-anchored
    // every pass. Enough passes (48 ≈ 55 m of spread) to dissolve the
    // Voronoi creases that printed the interfluves as flat polygonal
    // panels, while the anchoring keeps every channel exactly at its own
    // bed — so B is neither faceted nor biased.
    //
    // A partition-of-unity form (blur a bed numerator and a presence
    // denominator, divide) is smoother still but underflows far from the
    // network: the fallback boundary printed as hard black filaments
    // across the quiet margins. Relaxation has no crossover.
    let mut bedf = bed_near.clone();
    for _ in 0..48 {
        blur(spec, &mut bedf, 1);
        for &(x, y, z) in &bed_seed {
            bedf[spec.index(x, y)] = z;   // channel interiors hold their bed
        }
    }
    // Release the pin. Re-anchoring on the last pass leaves every channel
    // cell at exactly its bed while its neighbours sit where diffusion put
    // them, so any bed out of line with its surroundings — a head, above
    // all, which has neighbours on one side only — stands as a spike. That
    // is the singularity: measured, 36 of a tile's 40 sharpest cells lay
    // within 40 m of a channel head, ON the channel, and neither the bed
    // depth cap nor smoothing H moved them. Two free passes let the pinned
    // values settle into their own field.
    slope_limit(spec, &mut bedf, 0.22);
    blur(spec, &mut bedf, 2);

    // --- the divide field and the valley coordinate ------------------------
    // medial axis = where the distance field's gradient collapses
    let mut dsm = dist.clone();
    blur(spec, &mut dsm, 8);
    let mut med: Vec<(u32, u32, f64)> = Vec::new();
    let gat = |dsm: &Vec<f64>, x: i64, y: i64| -> f64 {
        dsm[spec.index(x.clamp(0, spec.nx as i64 - 1) as u32,
                       y.clamp(0, spec.ny as i64 - 1) as u32)]
    };
    for y in 0..spec.ny as i64 {
        for x in 0..spec.nx as i64 {
            let gx = (gat(&dsm, x + 1, y) - gat(&dsm, x - 1, y)) / (2.0 * CELL);
            let gy = (gat(&dsm, x, y + 1) - gat(&dsm, x, y - 1)) / (2.0 * CELL);
            if (gx * gx + gy * gy).sqrt() < 0.6
                && dist[spec.index(x as u32, y as u32)] > 24.0 {
                med.push((x as u32, y as u32, 0.0));
            }
        }
    }
    if med.is_empty() {
        med.push((spec.nx / 2, spec.ny / 2, 0.0));
    }
    let (mdist, _) = chamfer(spec, &med);

    // WANDER THE DIVIDE — by perturbing the distance to the MEDIAL AXIS,
    // never the distance to the channels.
    //
    // The profile is exactly symmetric about the medial axis, so the
    // midline between two channels is a perfect ridge and stays connected
    // everywhere: that, not density, is why our uplands carry a ridge tree
    // where real ones carry dashes (our fine drainage measures at real
    // density — p50 54-57 m against a real 43-48). Real divides wander,
    // because the two sides erode unequally.
    //
    // u = dist/(dist+mdist), so perturbing MDIST moves the crest laterally
    // while leaving the valleys alone: near a channel mdist is large and a
    // few tens of metres barely register, but at the crest mdist -> 0 and
    // the same perturbation moves the divide. This is the targeted form of
    // the warp that damaged the valleys when it was applied to dist.
    let (wa, wb) = (rng.next_u32(), rng.next_u32());
    let mut u = vec![0.0f64; spec.len()];
    let mut w = vec![0.0f64; spec.len()];
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let i = spec.index(x, y);
            let p = spec.world_of(x, y);
            let n = noise::perlin2(p.x / 300.0, p.y / 300.0, wa)
                + 0.6 * noise::perlin2(p.x / 130.0, p.y / 130.0, wb);
            let md = (mdist[i] + d.divide_wander * n).max(0.0);
            let ww = dist[i] + md;
            w[i] = dist[i] + mdist[i];        // half-width from the TRUE field
            u[i] = (dist[i] / ww.max(1e-6)).clamp(0.0, 1.0);
        }
    }
    // W is a property of the VALLEY, not of the cell: it must not jump
    // where the medial mask is ragged
    blur(spec, &mut w, 14);
    blur(spec, &mut u, 2);
    // re-anchor: blurring u lifted the channel cells off their own floor
    // (measured 1.34 m on seed 302), which partially fills the valleys —
    // u MUST be exactly 0 where a channel is
    for &(x, y, _) in &seed {
        u[spec.index(x, y)] = 0.0;
    }

    // --- H + residual ------------------------------------------------------
    let (s1, s2, s3) = (rng.next_u32(), rng.next_u32(), rng.next_u32());
    let rk = d.hand_resid;

    // H as a field, so its CONE APEXES can be rounded. A channel ENDPOINT
    // is a point source in the distance field: u rises radially around it,
    // so H forms a cone whose tip is a gradient singularity. Measured, 36
    // of a tile's 40 sharpest cells sat within 40 m of a channel head. Two
    // blur passes round the tips; the valley floors, being lines rather
    // than points, are barely touched.
    let mut hf = vec![0.0f64; spec.len()];
    for i in 0..spec.len() {
        hf[i] = prof.height(u[i], w[i]);
    }
    blur(spec, &mut hf, 2);

    // --- THE CAP (approach B) ----------------------------------------------
    // A Carolina interfluve IS the preserved relict sand cap; valleys are
    // incised INTO it. So the tops are referenced to a cap surface and only
    // the valleys are referenced to their beds.
    //
    // This is the one change that can remove the drainage dual, and the
    // reason is structural rather than aesthetic: while upland height is a
    // monotone function of distance-to-network, the aspect flips on the
    // medial axis everywhere and a ridge tree exists — measured, and proved
    // by approach A failing at double amplitude (see branch
    // sandhills-crest-A). The tops have to get their shape from something
    // the network does not determine.
    //
    // The cap is built as LOWPASS(bed + full valley depth) + an independent
    // deviation. The lowpass is long enough (≈ 500 m) to erase structure at
    // the valley-spacing scale, which is exactly the dual, while keeping the
    // regional trend so the two references agree on average and the blend
    // has no step to hide.
    let mut cap = vec![0.0f64; spec.len()];
    for i in 0..spec.len() {
        cap[i] = bedf[i] + prof.height(1.0, w[i]);
    }
    box_blur(spec, &mut cap, 62, 2);            // ≈ 500 m: erases the dual
    let (k1, k2) = (rng.next_u32(), rng.next_u32());
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let i = spec.index(x, y);
            let p = spec.world_of(x, y);
            cap[i] += d.upland_relief_m
                * (noise::perlin2(p.x / 700.0, p.y / 700.0, k1)
                    + 0.55 * noise::perlin2(p.x / 300.0, p.y / 300.0, k2));
        }
    }
    // the blend stays bed-dominated well past the shoulder: everything that
    // went wrong before went wrong near the valleys
    let mut wcap = vec![0.0f64; spec.len()];
    for i in 0..spec.len() {
        wcap[i] = math::smoothstep(0.74, 0.96, u[i]);
    }
    box_blur(spec, &mut wcap, 5, 2);

    let mut height = Grid::filled(spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let i = spec.index(x, y);
            let p = spec.world_of(x, y);
            let h = hf[i];
            // walls carry the roughness; floors and interfluve tops are calm
            let wall = math::exp(-((u[i] - 0.55) / 0.30).powi(2));
            // taper to nothing at the water line: forcing height = bed on
            // the channel CELLS while their neighbours kept a full residual
            // cut a one-cell slot down every channel (first render)
            let near = math::smoothstep(0.0, 0.14, u[i]);
            let amp = prof.spread(u[i], w[i]) * rk * near;
            // three octaves, none short enough to read as dimples at 8 m
            // (the first pass ran a 34 m octave and the interfluves came
            // out pebbled); sub-64 m fabric is the texture stage's job
            // Only the fine octave was wall-weighted, so the 165 m one
            // rode straight over the interfluve tops — measured, they
            // carried 1.37x real in the 150-250 m band, which is the
            // patterned grain the review picked out at "around 250 m".
            let top_r = math::smoothstep(0.70, 0.96, u[i]);
            let r = amp
                * (0.46 * (0.30 + 1.6 * wall)
                    * noise::perlin2(p.x / 78.0, p.y / 78.0, s1)
                    + 0.34 * (1.0 - 0.62 * top_r)
                        * noise::perlin2(p.x / 165.0, p.y / 165.0, s2)
                    + 0.20 * (1.0 - 0.35 * top_r)
                        * noise::perlin2(p.x / 340.0, p.y / 340.0, s3));
            // bed-referenced in the valleys, cap-referenced on the tops.
            // The cap form uses DEPTH BELOW THE CAP — the same measured
            // table, re-parameterised, not invented geometry.
            let z_bed = bedf[i] + h + r;
            let depth_below_cap = prof.height(1.0, w[i]) - h;
            let z_cap = cap[i] - depth_below_cap + r;
            height.data[i] = z_bed * (1.0 - wcap[i]) + z_cap * wcap[i];
        }
    }

    // NO hard clamp to the bed at channel cells. The smooth bed field plus
    // H(u=0) = 0 already puts every channel at its own floor; forcing the
    // cell value additionally cut a one-cell knife trench down every fine
    // gully, which printed as black filaments across the interfluves
    // (render, seeds 104/109). Small channels are wider than they are deep
    // and must be allowed to read that way.

    // SHARPEN THE VALLEYS. The H table is a MEDIAN over 30 tiles, and a
    // median of cross-sections is gentler than any of them: the shoulder
    // survives (that is what the u coordinate bought) but the whole
    // profile is softened. Measured, the assembled surface carried peak
    // curvature 0.57-1.11 against real tiles' 1.15-1.84, and adding
    // texture only made it fuzzier — 2 m grain cannot sharpen a 100 m
    // landform. An unsharp pass at VALLEY scale restores the edge, applied
    // to the landform before texture, where it belongs.
    // …and it is applied BY POSITION, not uniformly. A flat unsharp
    // sharpens the interfluve tops as much as the valley edges, which is
    // backwards on both counts (review 2026-08-25: "higher land can be
    // even smoother and the valley edges even sharper"). The break between
    // upland and valley lives around u ≈ 0.6, so the sharpening is
    // concentrated there and the tops get an extra lowpass instead.
    let sharp = d.valley_sharp;
    if sharp > 0.0 {
        // The lowpass has to be at VALLEY scale. At ~40 m this amplified
        // the fabric instead of the edge — it inflated the sub-64 m band
        // to 0.53 against a real 0.29-0.41 and added exactly the fuzz it
        // was meant to cure. A valley edge is a 100-200 m feature.
        let mut lp = height.data.clone();
        box_blur(spec, &mut lp, 13, 2);           // ~ 105 m
        // The tops need a genuinely LONG lowpass: measured, they carried
        // 1.37x real in the 150-250 m band, and that energy is LANDFORM,
        // not texture — thinning the residual's octaves barely touched it.
        let mut hi = height.data.clone();
        // review 2026-08-25: still too much presence at 150-250 m on the
        // tops, so the lowpass is longer again and the tops lean on it
        // harder
        box_blur(spec, &mut hi, 34, 3);           // ~ 270 m radius
        // The WEIGHTS are smoothed before use. u carries fine-scale
        // variation, and multiplying a large lowpass difference by a
        // jittery weight injects energy at exactly the scale we are trying
        // to remove — measured, the sub-64 m band ran 0.58-0.61 against a
        // real 0.29-0.41 until these were blurred.
        let mut w_edge = vec![0.0f64; spec.len()];
        let mut w_top = vec![0.0f64; spec.len()];
        for i in 0..spec.len() {
            w_edge[i] = math::exp(-((u[i] - 0.60) / 0.24).powi(2));
            w_top[i] = math::smoothstep(0.70, 0.96, u[i]);
        }
        box_blur(spec, &mut w_edge, 6, 2);
        box_blur(spec, &mut w_top, 6, 2);
        for i in 0..spec.len() {
            let edge = w_edge[i];
            let top = w_top[i];
            let z = height.data[i];
            // the wider (valley-scale) lowpass leaves a much larger
            // residual, so the coefficient is small: 2.1 against a 40 m
            // lowpass became 1.0 of sub-64 m band against a real 0.29-0.41
            // the shoulder can take more now the weights are smoothed
            // 0.78, was 0.95: the unsharp works off a 105 m lowpass, so it
            // feeds the same 32-128 m bands that measured hot. Trimmed with
            // the coarse quilt gain rather than instead of it, so the valley
            // edge keeps most of its definition.
            let sharpened = z + sharp * 0.78 * edge * (z - lp[i]);
            // interfluve tops relax toward their own long lowpass
            height.data[i] = sharpened * (1.0 - 0.90 * top) + hi[i] * (0.90 * top);
        }
    }

    let mut g_dist = Grid::filled(spec, 0.0f64);
    let mut g_u = Grid::filled(spec, 0.0f64);
    let mut g_w = Grid::filled(spec, 0.0f64);
    let mut g_bed = Grid::filled(spec, 0.0f64);
    g_dist.data = dist;
    g_u.data = u;
    g_w.data = w;
    g_bed.data = bedf;
    Assembled { height, dist: g_dist, u: g_u, w: g_w, bed: g_bed }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use course_seed::RunIdentity;

    fn build(seed: u64) -> Assembled {
        let id = RunIdentity::from_seed(seed);
        let d = crate::draw::site(&id, Some(Mode::Fluvial), None);
        let mut dr = crate::rng::stream(&id, crate::rng::DATUM);
        let datum = crate::channel::datum(&mut dr, &d);
        let mut cr = crate::rng::stream(&id, crate::rng::CHANNEL);
        let net = crate::channel::grow(&mut cr, &datum, &d);
        let beds = crate::carve::beds(&mut cr, &net, &datum, &d);
        let prof = HandProfile::load(std::path::Path::new(
            "../../assets/sandhills_hand_profile.txt"))
            .or_else(|_| HandProfile::load(std::path::Path::new(
                "assets/sandhills_hand_profile.txt")))
            .expect("hand profile");
        let mut ar = crate::rng::stream(&id, crate::rng::HAND);
        let tiers: Vec<u8> = net.chans.iter().map(|c| c.tier).collect();
        assemble(&mut ar, &beds, &tiers, &prof, &d)
    }

    #[test]
    fn the_profile_table_loads_and_is_monotone() {
        let prof = HandProfile::load(std::path::Path::new(
            "../../assets/sandhills_hand_profile.txt"))
            .or_else(|_| HandProfile::load(std::path::Path::new(
                "assets/sandhills_hand_profile.txt")))
            .unwrap();
        for w in [60.0, 210.0, 520.0] {
            assert_eq!(prof.height(0.0, w), 0.0, "HAND is zero on the channel");
            let mut last = -1.0;
            for k in 0..=20 {
                let h = prof.height(k as f64 / 20.0, w);
                assert!(h >= last - 1e-9, "profile dips at w={w}");
                last = h;
            }
        }
        // a wide valley is deeper than a crowded headwater one
        assert!(prof.height(1.0, 520.0) > 3.0 * prof.height(1.0, 60.0));
    }

    #[test]
    fn the_channels_are_the_low_ground() {
        // The invariant that matters, and the one the whole representation
        // exists to produce: ground ON the network sits well below ground
        // out on the interfluves. It is NOT "every channel cell equals its
        // bed" — channel TIPS are deliberately left unpinned, because a
        // point constraint with support on one side stands as a spike (the
        // singularities: the sharpest cells on a tile clustered within
        // 40 m of channel heads).
        for seed in [301u64, 302] {
            let a = build(seed);
            let (mut near, mut nn) = (0.0f64, 0usize);
            let (mut far, mut fnn) = (0.0f64, 0usize);
            for i in 0..a.height.data.len() {
                let d = a.dist.data[i];
                if d < 8.0 {
                    near += a.height.data[i];
                    nn += 1;
                } else if (80.0..160.0).contains(&d) {
                    far += a.height.data[i];
                    fnn += 1;
                }
            }
            assert!(nn > 100 && fnn > 100, "seed {seed}: too few samples");
            let drop = far / fnn as f64 - near / nn as f64;
            assert!(drop > 1.5, "seed {seed}: interfluves only {drop:.2} m above the channels");
        }
    }

    #[test]
    fn assembled_relief_is_in_the_corpus_band() {
        // real kept tiles: 44.6-62.3 m (p99-p1)
        for seed in [303u64, 304, 305] {
            let a = build(seed);
            let mut v = a.height.data.clone();
            v.sort_by(|x, y| x.partial_cmp(y).unwrap());
            let relief = v[(v.len() as f64 * 0.99) as usize]
                - v[(v.len() as f64 * 0.01) as usize];
            assert!(relief > 26.0 && relief < 78.0, "seed {seed}: relief {relief:.1}");
        }
    }

    #[test]
    fn the_valley_coordinate_spans_its_range() {
        let a = build(306);
        let mut v = a.u.data.clone();
        v.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let p10 = v[v.len() / 10];
        let p90 = v[v.len() * 9 / 10];
        assert!(p10 < 0.30, "u p10 {p10:.2} — channels not reached");
        assert!(p90 > 0.70, "u p90 {p90:.2} — divides not reached");
    }
}
