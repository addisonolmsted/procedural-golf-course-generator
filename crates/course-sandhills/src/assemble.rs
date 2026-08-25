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

fn blur(spec: course_world::grid::GridSpec, a: &mut Vec<f64>, passes: usize) {
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
    blur(spec, &mut bedf, 2);

    // --- the divide field and the valley coordinate ------------------------
    // medial axis = where the distance field's gradient collapses
    let mut dsm = dist.clone();
    blur(spec, &mut dsm, 8);
    let mut med: Vec<(u32, u32, f64)> = Vec::new();
    for y in 1..spec.ny - 1 {
        for x in 1..spec.nx - 1 {
            let gx = (dsm[spec.index(x + 1, y)] - dsm[spec.index(x - 1, y)]) / (2.0 * CELL);
            let gy = (dsm[spec.index(x, y + 1)] - dsm[spec.index(x, y - 1)]) / (2.0 * CELL);
            if (gx * gx + gy * gy).sqrt() < 0.6 && dist[spec.index(x, y)] > 24.0 {
                med.push((x, y, 0.0));
            }
        }
    }
    if med.is_empty() {
        med.push((spec.nx / 2, spec.ny / 2, 0.0));
    }
    let (mdist, _) = chamfer(spec, &med);

    let mut u = vec![0.0f64; spec.len()];
    let mut w = vec![0.0f64; spec.len()];
    for i in 0..spec.len() {
        let ww = dist[i] + mdist[i];
        w[i] = ww;
        u[i] = (dist[i] / ww.max(1e-6)).clamp(0.0, 1.0);
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
            let r = amp
                * (0.46 * (0.30 + 1.6 * wall)
                    * noise::perlin2(p.x / 78.0, p.y / 78.0, s1)
                    + 0.34 * noise::perlin2(p.x / 165.0, p.y / 165.0, s2)
                    + 0.20 * noise::perlin2(p.x / 340.0, p.y / 340.0, s3));
            height.data[i] = bedf[i] + h + r;
        }
    }

    // NO hard clamp to the bed at channel cells. The smooth bed field plus
    // H(u=0) = 0 already puts every channel at its own floor; forcing the
    // cell value additionally cut a one-cell knife trench down every fine
    // gully, which printed as black filaments across the interfluves
    // (render, seeds 104/109). Small channels are wider than they are deep
    // and must be allowed to read that way.

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
