//! Site-quality fields and candidate extraction on the macro grid.
//!
//! Everything here is derived read-only from `CourseTerrain` and is fully
//! deterministic (no RNG): the per-seed variety comes from the terrain itself
//! plus the DetRng-driven choices in construction. Candidates are kept as
//! sorted, Poisson-thinned lists so downstream index-based choices are stable.

use golf_core::{GridSpec, Vec2};
use golf_terrain::water::CLASS_DRY;
use golf_terrain::CourseTerrain;

use crate::{BOX_MAX, BOX_MIN};

/// One candidate site (green, tee, or clubhouse position).
#[derive(Clone, Copy, Debug)]
pub struct Site {
    pub pos: Vec2,
    pub cell: usize,
    pub score: f64,
}

/// Derived fields + thinned candidate lists.
pub struct Fields {
    pub spec: GridSpec,
    /// Meters to the nearest wet cell (chamfer distance; large where dry far).
    pub dist_water: Vec<f64>,
    /// Local prominence: z − mean(z, r≈60 m).
    pub prominence: Vec<f64>,
    /// Max slope in a 3×3 window (pad flatness for greens/tees).
    pub pad3: Vec<f64>,
    pub greens: Vec<Site>,
    pub tees: Vec<Site>,
    pub clubs: Vec<Site>,
    /// True if pad thresholds had to be relaxed to find enough candidates.
    pub relaxed: bool,
}

/// Pad-slope thresholds (rise/run) per site type.
const GREEN_PAD_MAX: f64 = 0.065;
const TEE_PAD_MAX: f64 = 0.085;
const CLUB_PAD_MAX: f64 = 0.055;

pub fn build(ct: &CourseTerrain) -> Fields {
    let spec = ct.macro_slope.spec;
    let n = spec.len();
    let nx = spec.nx as usize;
    let ny = spec.ny as usize;
    let cell = spec.cell_size;

    // --- distance to water (two-pass 3-4 chamfer over wet cells) ---
    let big = 1.0e9f64;
    let mut dist_water = vec![big; n];
    for (i, d) in dist_water.iter_mut().enumerate() {
        if ct.water.class.data[i] != CLASS_DRY {
            *d = 0.0;
        }
    }
    let orth = cell;
    let diag = cell * std::f64::consts::SQRT_2;
    // Forward pass (W, N, NW, NE), then backward (E, S, SE, SW).
    for y in 0..ny {
        for x in 0..nx {
            let i = y * nx + x;
            let mut d = dist_water[i];
            if x > 0 {
                d = d.min(dist_water[i - 1] + orth);
            }
            if y > 0 {
                d = d.min(dist_water[i - nx] + orth);
                if x > 0 {
                    d = d.min(dist_water[i - nx - 1] + diag);
                }
                if x + 1 < nx {
                    d = d.min(dist_water[i - nx + 1] + diag);
                }
            }
            dist_water[i] = d;
        }
    }
    for y in (0..ny).rev() {
        for x in (0..nx).rev() {
            let i = y * nx + x;
            let mut d = dist_water[i];
            if x + 1 < nx {
                d = d.min(dist_water[i + 1] + orth);
            }
            if y + 1 < ny {
                d = d.min(dist_water[i + nx] + orth);
                if x + 1 < nx {
                    d = d.min(dist_water[i + nx + 1] + diag);
                }
                if x > 0 {
                    d = d.min(dist_water[i + nx - 1] + diag);
                }
            }
            dist_water[i] = d;
        }
    }

    // --- prominence: z − box-blur(z, r=5 cells ≈ 62.5 m), separable ---
    let r = 5usize;
    let z = &ct.macro_heights.data;
    let mut tmp = vec![0.0f64; n];
    for y in 0..ny {
        for x in 0..nx {
            let x0 = x.saturating_sub(r);
            let x1 = (x + r).min(nx - 1);
            let mut s = 0.0;
            for xx in x0..=x1 {
                s += z[y * nx + xx];
            }
            tmp[y * nx + x] = s / (x1 - x0 + 1) as f64;
        }
    }
    let mut prominence = vec![0.0f64; n];
    for y in 0..ny {
        let y0 = y.saturating_sub(r);
        let y1 = (y + r).min(ny - 1);
        for x in 0..nx {
            let mut s = 0.0;
            for yy in y0..=y1 {
                s += tmp[yy * nx + x];
            }
            prominence[y * nx + x] = z[y * nx + x] - s / (y1 - y0 + 1) as f64;
        }
    }

    // --- pad flatness: max slope over 3×3 / 5×5 windows ---
    let sl = &ct.macro_slope.data;
    let win_max = |x: usize, y: usize, r: usize| -> f64 {
        let x0 = x.saturating_sub(r);
        let x1 = (x + r).min(nx - 1);
        let y0 = y.saturating_sub(r);
        let y1 = (y + r).min(ny - 1);
        let mut m = 0.0f64;
        for yy in y0..=y1 {
            for xx in x0..=x1 {
                m = m.max(sl[yy * nx + xx]);
            }
        }
        m
    };
    let mut pad3 = vec![0.0f64; n];
    let mut pad5 = vec![0.0f64; n];
    for y in 0..ny {
        for x in 0..nx {
            pad3[y * nx + x] = win_max(x, y, 1);
            pad5[y * nx + x] = win_max(x, y, 2);
        }
    }

    // --- scores ---
    // A relaxation pass widens pad thresholds when a seed is so steep or wet
    // that the base thresholds starve the candidate pools.
    let mut relaxed = false;
    let (greens, tees, clubs) = loop {
        let k = if relaxed { 1.5 } else { 1.0 };
        let mut gs: Vec<Site> = Vec::new();
        let mut ts: Vec<Site> = Vec::new();
        let mut cs: Vec<Site> = Vec::new();
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                let p = spec.world_of(x as u32, y as u32);
                let in_box = |m: f64| {
                    p.x >= BOX_MIN + m && p.x <= BOX_MAX - m && p.y >= BOX_MIN + m && p.y <= BOX_MAX - m
                };
                if ct.water.class.data[i] != CLASS_DRY {
                    continue;
                }
                let dw = dist_water[i];
                let prom = prominence[i];
                // Interest bands: near water reads as a feature, in it doesn't.
                let water_band = |lo: f64, hi: f64| -> f64 {
                    if dw < lo || dw > hi * 2.0 {
                        0.0
                    } else if dw <= hi {
                        1.0
                    } else {
                        1.0 - (dw - hi) / hi
                    }
                };
                if in_box(10.0) && pad3[i] < GREEN_PAD_MAX * k && dw > 18.0 {
                    let flat = 1.0 - pad3[i] / (GREEN_PAD_MAX * k);
                    let interest = 0.8 * water_band(20.0, 60.0);
                    // High ground carries a premium (greens on rises/shelves).
                    let perch = if prom > 0.5 { 1.0 * (prom / 6.0).min(1.0) } else { 0.0 };
                    gs.push(Site { pos: p, cell: i, score: 1.5 * flat + interest + perch });
                }
                if in_box(10.0) && pad3[i] < TEE_PAD_MAX * k && dw > 12.0 {
                    let flat = 1.2 * (1.0 - pad3[i] / (TEE_PAD_MAX * k));
                    // High ground carries a premium (elevated tees).
                    let elev = if prom > 0.0 { 1.1 * (prom / 6.0).min(1.0) } else { 0.0 };
                    let interest = 0.3 * water_band(25.0, 90.0);
                    ts.push(Site { pos: p, cell: i, score: flat + elev + interest });
                }
                if in_box(40.0) && pad5[i] < CLUB_PAD_MAX * k && dw > 25.0 {
                    let flat = 2.0 * (1.0 - pad5[i] / (CLUB_PAD_MAX * k));
                    let interest = 1.2 * water_band(30.0, 120.0);
                    let overlook = if prom > 1.0 { 0.8 * (prom / 8.0).min(1.0) } else { 0.0 };
                    let center = Vec2::new(1000.0, 1000.0);
                    let dc = p.distance(center);
                    let central = if dc > 550.0 { -0.004 * (dc - 550.0) } else { 0.0 };
                    cs.push(Site { pos: p, cell: i, score: flat + interest + overlook + central });
                }
            }
        }
        let gs = thin(gs, 45.0, 420);
        let ts = thin(ts, 40.0, 560);
        let cs = thin(cs, 100.0, 12);
        if (gs.len() >= 60 && ts.len() >= 90 && !cs.is_empty()) || relaxed {
            break (gs, ts, cs);
        }
        relaxed = true;
    };

    Fields {
        spec,
        dist_water,
        prominence,
        pad3,
        greens,
        tees,
        clubs,
        relaxed,
    }
}

/// Sort by (score desc, cell asc) and greedily keep sites at least `min_sep`
/// apart, capped at `max_n`. Deterministic: no RNG, total order.
fn thin(mut sites: Vec<Site>, min_sep: f64, max_n: usize) -> Vec<Site> {
    sites.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.cell.cmp(&b.cell))
    });
    let mut kept: Vec<Site> = Vec::new();
    for s in sites {
        if kept.len() >= max_n {
            break;
        }
        if kept.iter().all(|k| k.pos.distance(s.pos) >= min_sep) {
            kept.push(s);
        }
    }
    kept
}

/// Macro cell index containing world point `p` (cells span [x·cell, (x+1)·cell)).
pub fn cell_of(spec: &GridSpec, p: Vec2) -> usize {
    let cx = ((p.x / spec.cell_size) as i64).clamp(0, spec.nx as i64 - 1) as usize;
    let cy = ((p.y / spec.cell_size) as i64).clamp(0, spec.ny as i64 - 1) as usize;
    cy * spec.nx as usize + cx
}

#[cfg(test)]
mod tests {
    use super::*;
    use golf_terrain::{generate_course, macro_spec};

    #[test]
    fn fields_deterministic_and_candidates_in_box() {
        let (ct, _) = generate_course(&macro_spec(), 2024);
        let a = build(&ct);
        let b = build(&ct);
        assert_eq!(a.greens.len(), b.greens.len());
        assert!(a.greens.iter().zip(&b.greens).all(|(x, y)| x.cell == y.cell));
        assert!(!a.greens.is_empty() && !a.tees.is_empty() && !a.clubs.is_empty());
        for s in a.greens.iter().chain(&a.tees).chain(&a.clubs) {
            assert!(s.pos.x >= BOX_MIN && s.pos.x <= BOX_MAX);
            assert!(s.pos.y >= BOX_MIN && s.pos.y <= BOX_MAX);
            assert_eq!(ct.water.class.data[s.cell], CLASS_DRY);
        }
        // Thinning respects separation.
        for (i, s) in a.greens.iter().enumerate() {
            for t in &a.greens[i + 1..] {
                assert!(s.pos.distance(t.pos) >= 45.0 - 1e-9);
            }
        }
    }

    #[test]
    fn cell_of_roundtrip() {
        let spec = macro_spec();
        let p = spec.world_of(37, 101);
        assert_eq!(cell_of(&spec, p), 101 * spec.nx as usize + 37);
    }
}
