//! Green boundaries, contouring, enforced pinnability, and pin placement.
//!
//! Shapes use one evaluator (superellipse × petal modulation) covering the
//! four families; size scales with approach length; orientation is strategic
//! (open / angled / guarded around water). Contouring layers a tilted plane,
//! tiers, gentle features, and micro noise — then a fixed relaxation loop
//! **guarantees** a pinnable region before the single pin is chosen.

use golf_core::det::DetRng;
use golf_core::math::{self, Vec2};
use golf_core::spline::SpineCurve;
use golf_core::Grid;
use golf_routing::construct::normal;
use golf_routing::Hole;
use golf_terrain::water::CLASS_DRY;
use golf_terrain::{noise, CourseTerrain};

use crate::fairway::FairwayShape;
use crate::{patch_spec, GreenShape, Patch, FRINGE_M};

/// Pinnable rules: max slope over the local disc, minimum edge margin.
pub const PIN_MAX_SLOPE: f64 = 0.033;
pub const PIN_EDGE_MARGIN: f64 = 2.7;
/// Required pinnable share of green area and minimum pocket size.
const PINNABLE_SHARE: f64 = 0.18;
const POCKET_M2: f64 = 12.0;
/// Feather beyond the fringe back to base terrain, m.
const GREEN_FEATHER: f64 = 4.0;

/// Draw a green shape for a routed hole. `dblock` is the course-wide
/// blocking-water distance field.
pub fn shape(
    ct: &CourseTerrain,
    hole: &Hole,
    spine: &SpineCurve,
    dblock: &golf_core::Grid<f64>,
    rng: &mut DetRng,
) -> GreenShape {
    let center0 = *hole.pts.last().unwrap();
    let approach = if hole.par >= 4 {
        hole.pts[hole.pts.len() - 2].distance(center0)
    } else {
        hole.len
    };
    let target_area = (420.0 + 1.1 * approach + 90.0 * normal(rng)).clamp(350.0, 900.0);

    // Adjacent hazard? Any water within 30 m guards the green.
    let guard_dir = {
        let spec = ct.water.class.spec;
        let cell = spec.cell_size;
        let r = (30.0 / cell).ceil() as i64;
        let cx = (center0.x / cell) as i64;
        let cy = (center0.y / cell) as i64;
        let mut best: Option<(f64, Vec2)> = None;
        for dy in -r..=r {
            for dx in -r..=r {
                let x = cx + dx;
                let y = cy + dy;
                if x < 0 || y < 0 || x >= spec.nx as i64 || y >= spec.ny as i64 {
                    continue;
                }
                let i = y as usize * spec.nx as usize + x as usize;
                if ct.water.class.data[i] == CLASS_DRY {
                    continue;
                }
                let p = spec.world_of(x as u32, y as u32);
                let d = p.distance(center0);
                if d <= 30.0 && best.map(|(bd, _)| d < bd).unwrap_or(true) {
                    best = Some((d, (p - center0).normalized()));
                }
            }
        }
        best.map(|(_, dir)| dir)
    };

    // Family draw: petal shapes only (user direction) — the r(θ) harmonic
    // form. Most greens use centered harmonics (i = 2..4); guarded greens
    // sometimes add the strong first harmonic (kidney bulge away from the
    // hazard). The superellipse exponent stays 2.
    let u = rng.next_f64();
    let n = 2.0;
    let mut coefs = [(0.0f64, 0.0f64); 4];
    let aspect;
    if guard_dir.is_some() && u < 0.25 {
        // Kidney petal: strong first harmonic bulging away from the hazard.
        aspect = rng.range_f64(1.10, 1.50);
        coefs[0] = (rng.range_f64(0.18, 0.30), 0.0); // phase set after rot below
        coefs[1] = (rng.range_f64(0.08, 0.15), rng.range_f64(0.0, std::f64::consts::TAU));
    } else {
        // Petal: harmonics i = 2..4.
        aspect = rng.range_f64(1.05, 1.40);
        let mut c = [
            rng.range_f64(0.05, 0.18),
            rng.range_f64(0.04, 0.12),
            rng.range_f64(0.02, 0.08),
        ];
        let sum: f64 = c.iter().sum();
        if sum > 0.35 {
            for v in &mut c {
                *v *= 0.35 / sum;
            }
        }
        for (k, &ci) in c.iter().enumerate() {
            coefs[k + 1] = (ci, rng.range_f64(0.0, std::f64::consts::TAU));
        }
    }

    // Strategic orientation. Approach direction INTO the green.
    let dir_in = spine.tangent_at(1.0);
    let approach_ang = math::atan2(dir_in.y, dir_in.x);
    let style = rng.next_f64();
    let rot = if let (Some(g), true) = (guard_dir, style < 0.45) {
        // Guarded: long axis across the hazard direction.
        math::atan2(g.y, g.x) + std::f64::consts::FRAC_PI_2
    } else if style < 0.60 {
        approach_ang // open: deep green along the approach
    } else {
        approach_ang + rng.range_f64(0.5, 0.9) * if rng.next_f64() < 0.5 { 1.0 } else { -1.0 }
    };

    let mut gs = GreenShape {
        center: center0,
        rot,
        a: aspect,
        b: 1.0,
        n,
        coefs,
        outline: Vec::new(),
        area: 0.0,
        guard_dir,
    };
    // Kidney bulge away from the hazard: ρ max where cos(θ+φ₁)=1 at θ = −φ₁.
    if gs.coefs[0].0 != 0.0 {
        let away = guard_dir.map(|g| g * -1.0).unwrap_or(dir_in);
        let local = math::atan2(away.y, away.x) - rot;
        gs.coefs[0].1 = -local;
    }

    // Normalize to the target area, then fit against water/box (shrink-nudge).
    let scale_to = |gs: &mut GreenShape, area: f64| {
        let mut acc = 0.0;
        let m = 256;
        for k in 0..m {
            let th = k as f64 / m as f64 * std::f64::consts::TAU;
            let r = gs.rho(th);
            acc += 0.5 * r * r * (std::f64::consts::TAU / m as f64);
        }
        let cur = gs.a * gs.b * acc;
        let s = (area / cur.max(1e-9)).sqrt();
        gs.a *= s;
        gs.b *= s;
    };
    scale_to(&mut gs, target_area);

    for _ in 0..4 {
        let bad = (0..64).any(|k| {
            let p = gs.boundary(k as f64 / 64.0 * std::f64::consts::TAU);
            !golf_routing::geometry::in_box(p, 10.0) || dblock.bilinear(p) < 3.0
        });
        if !bad {
            break;
        }
        // Nudge away from the hazard, then shrink.
        if let Some(g) = gs.guard_dir {
            gs.center = gs.center + g * -2.5;
        }
        gs.a *= 0.93;
        gs.b *= 0.93;
    }

    // Final outline + area.
    let m = 128;
    gs.outline = (0..m)
        .map(|k| gs.boundary(k as f64 / m as f64 * std::f64::consts::TAU))
        .collect();
    let mut acc = 0.0;
    for k in 0..m {
        let a = gs.outline[k];
        let b = gs.outline[(k + 1) % m];
        acc += a.x * b.y - b.x * a.y;
    }
    gs.area = 0.5 * acc.abs();
    gs
}

/// Strategic fairway-cut surround for a par 3: ONE region — a slightly
/// larger copy of the green's own petal shape (same rotation and
/// harmonics, scaled radius) whose center is shifted a small strategic
/// distance (well inside the green, never near its edge). The overlap ring
/// outside the green/fringe is the mown fairway collar; the offset makes
/// it generous on the runoff side and tight on the far side.
#[derive(Clone, Debug)]
pub struct GreenSurround {
    pub shape: GreenShape,
}

/// Build the par-3 surround petal. The offset direction prefers downhill
/// (where runoff carries missed greens); on near-flat sites it falls back
/// to the front (toward the tee). Scaled down until clear of water.
pub fn surround(
    ct: &CourseTerrain,
    gs: &GreenShape,
    dir_in: Vec2,
    dblock: &golf_core::Grid<f64>,
    rng: &mut DetRng,
) -> GreenSurround {
    // Downhill = where runoff (and missed greens) collect.
    let g = 8.0;
    let grad = Vec2::new(
        ct.macro_heights.bilinear(gs.center + Vec2::new(g, 0.0))
            - ct.macro_heights.bilinear(gs.center + Vec2::new(-g, 0.0)),
        ct.macro_heights.bilinear(gs.center + Vec2::new(0.0, g))
            - ct.macro_heights.bilinear(gs.center + Vec2::new(0.0, -g)),
    );
    let slope = grad.length() / (2.0 * g);
    let dir = if slope > 0.015 {
        grad.normalized() * -1.0
    } else {
        dir_in * -1.0 // front: toward the tee
    };

    let min_axis = gs.a.min(gs.b);
    // Slight shift — the surround center stays deep inside the green.
    let offset = dir * (min_axis * rng.range_f64(0.15, 0.30));
    // Radius growth: the collar band beyond the fringe.
    let mut k = 1.0 + rng.range_f64(7.0, 13.0) / min_axis;

    let build = |k: f64, offset: Vec2| -> GreenShape {
        let mut s = GreenShape {
            center: gs.center + offset,
            rot: gs.rot,
            a: gs.a * k,
            b: gs.b * k,
            n: gs.n,
            coefs: gs.coefs,
            outline: Vec::new(),
            area: 0.0,
            guard_dir: None,
        };
        let m = 64;
        s.outline = (0..m)
            .map(|i| s.boundary(i as f64 / m as f64 * std::f64::consts::TAU))
            .collect();
        s
    };
    // Water clip: shrink until the outline stays ≥3 m dry (the green itself
    // already satisfies this, so k → 1 always converges).
    let mut shape = build(k, offset);
    for _ in 0..4 {
        let wetted = shape.outline.iter().any(|&p| dblock.bilinear(p) < 3.0);
        if !wetted {
            break;
        }
        k = 1.0 + (k - 1.0) * 0.65;
        shape = build(k, offset * 0.65);
    }
    GreenSurround { shape }
}

/// Contouring parameters (rebuilt each enforcement pass with scaled knobs).
struct ContourDraw {
    z0: f64,
    tilt_dir: Vec2,
    tilt: f64,
    cross: f64,
    tiers: Vec<(Vec2, Vec2, f64, f64)>, // (chord point, chord normal, step, riser half-width)
    features: Vec<(Vec2, f64, f64)>,    // (center, amp signed, radius)
    micro_seed: u32,
}

fn z_green(gs: &GreenShape, d: &ContourDraw, p: Vec2) -> f64 {
    let mut z = d.z0
        + d.tilt * (p - gs.center).dot(d.tilt_dir)
        + d.cross * (p - gs.center).dot(d.tilt_dir.perp());
    for &(c, n, step, rw) in &d.tiers {
        let sd = (p - c).dot(n);
        z += step * math::smoothstep(-rw, rw, sd);
    }
    for &(c, amp, r) in &d.features {
        let d2 = (p - c).dot(p - c);
        z += amp * math::exp(-d2 / (2.0 * r * r));
    }
    z + 0.04 * noise::perlin2(p.x / 10.0, p.y / 10.0, d.micro_seed)
}

fn on_riser(d: &ContourDraw, p: Vec2) -> bool {
    d.tiers
        .iter()
        .any(|&(c, n, _, rw)| ((p - c).dot(n)).abs() < rw + 0.3)
}

/// Build the green patch, enforce pinnability, and place the pin. Returns
/// the realized pinnable share of the green area alongside.
pub fn patch_and_pin(
    ct: &CourseTerrain,
    gs: &GreenShape,
    fw: &FairwayShape,
    rng: &mut DetRng,
) -> (Patch, Vec2, u8, f64) {
    // Bbox over the outline + fringe + feather.
    let (mut lo, mut hi) = (
        Vec2::new(f64::INFINITY, f64::INFINITY),
        Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for &p in &gs.outline {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let m = FRINGE_M + GREEN_FEATHER + 1.5;
    let spec = patch_spec(lo - Vec2::new(m, m), hi + Vec2::new(m, m), 0.5);

    // Base plane from the local landform, tilted toward the approach.
    let dir_in = fw.spine.tangent_at(1.0);
    let z0 = ct.macro_heights.bilinear(gs.center);
    let mut tilt = rng.range_f64(0.010, 0.020);
    let mut cross = rng.range_f64(-0.010, 0.010);
    let mag = math::hypot(tilt, cross);
    if mag > 0.022 {
        tilt *= 0.022 / mag;
        cross *= 0.022 / mag;
    }

    // Tiers.
    let tu = rng.next_f64();
    let n_tiers = if tu < 0.55 { 0 } else if tu < 0.88 { 1 } else { 2 };
    let chord_ang = rng.range_f64(0.0, std::f64::consts::TAU);
    let chord_n = Vec2::new(math::cos(chord_ang), math::sin(chord_ang));
    let axis = gs.a.max(gs.b);
    let mut tiers: Vec<(Vec2, Vec2, f64, f64)> = Vec::new();
    for t in 0..n_tiers {
        let off = if n_tiers == 1 {
            rng.range_f64(-0.15, 0.15)
        } else {
            -0.3 + 0.6 * t as f64
        };
        tiers.push((
            gs.center + chord_n * (off * axis),
            chord_n,
            rng.range_f64(0.25, 0.55),
            rng.range_f64(1.25, 2.0),
        ));
    }

    // Gentle internal features.
    let n_feat = 1 + rng.below(3);
    let mut features: Vec<(Vec2, f64, f64)> = Vec::new();
    for _ in 0..n_feat {
        let th = rng.range_f64(0.0, std::f64::consts::TAU);
        let rr = rng.range_f64(0.0, 0.6);
        let c = gs.boundary(th).lerp(gs.center, 1.0 - rr).lerp(gs.center, 0.4);
        let amp = rng.range_f64(0.10, 0.25) * if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
        features.push((c, amp, rng.range_f64(4.0, 8.0)));
    }
    let micro_seed = rng.next_u32();

    let mut draw = ContourDraw {
        z0,
        tilt_dir: dir_in,
        tilt,
        cross,
        tiers,
        features,
        micro_seed,
    };

    // Enforcement: measure pinnable share, scale knobs down until it holds.
    let nx = spec.nx as usize;
    let ny = spec.ny as usize;
    let mut z = Grid::filled(spec, 0.0f64);
    // rnorm is the rasterization hot path — evaluate once per cell.
    let mut rn_buf = vec![0.0f64; spec.len()];
    let mut inner = vec![false; spec.len()]; // strictly inside the green
    let mut margin_ok = vec![false; spec.len()];
    for gy in 0..spec.ny {
        for gx in 0..spec.nx {
            let p = spec.world_of(gx, gy);
            let i = spec.index(gx, gy);
            let rn = gs.rnorm(p);
            rn_buf[i] = rn;
            inner[i] = rn <= 1.0;
            // Radial distance to the boundary along the center ray: p sits at
            // fraction rn of its boundary ray, so the remaining radial run is
            // |p−center|·(1/rn − 1). A 0.85 factor guards mild petal
            // concavity; the chosen pin is re-verified exactly.
            if rn > 0.02 && rn <= 1.0 {
                let radial = p.distance(gs.center) * (1.0 / rn - 1.0);
                margin_ok[i] = 0.85 * radial >= PIN_EDGE_MARGIN;
            } else {
                margin_ok[i] = rn <= 0.02; // effectively the center: deep inside
            }
        }
    }

    let mut pinnable = vec![false; spec.len()];
    let mut final_share = 0.0f64;
    for pass in 0..5 {
        let r_out = 1.0 + (FRINGE_M + GREEN_FEATHER) / gs.a.min(gs.b).max(1.0);
        for gy in 0..spec.ny {
            for gx in 0..spec.nx {
                let p = spec.world_of(gx, gy);
                let i = spec.index(gx, gy);
                // Sane default beyond the feather (bilinear mixes neighbors;
                // w = 0 there, so the cheap macro base suffices).
                z.data[i] = if rn_buf[i] > r_out {
                    ct.macro_heights.bilinear(p)
                } else {
                    z_green(gs, &draw, p)
                };
            }
        }
        // Pinnable: slope ≤ PIN_MAX_SLOPE over a 1.5 m disc, margin, off-riser.
        let cell = spec.cell_size;
        let slope_at = |x: usize, y: usize, z: &Grid<f64>| -> f64 {
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(nx - 1);
            let ym = y.saturating_sub(1);
            let yp = (y + 1).min(ny - 1);
            let dzdx = (z.data[y * nx + xp] - z.data[y * nx + xm]) / ((xp - xm).max(1) as f64 * cell);
            let dzdy = (z.data[yp * nx + x] - z.data[ym * nx + x]) / ((yp - ym).max(1) as f64 * cell);
            math::hypot(dzdx, dzdy)
        };
        let r_disc = 3usize; // 1.5 m at 0.5 m cells
        let mut count = 0usize;
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                pinnable[i] = false;
                if !margin_ok[i] {
                    continue;
                }
                let p = spec.world_of(x as u32, y as u32);
                if on_riser(&draw, p) {
                    continue;
                }
                let mut ok = true;
                'disc: for dy in -(r_disc as i64)..=r_disc as i64 {
                    for dx in -(r_disc as i64)..=r_disc as i64 {
                        if dx * dx + dy * dy > (r_disc * r_disc) as i64 {
                            continue;
                        }
                        let xx = (x as i64 + dx).clamp(0, nx as i64 - 1) as usize;
                        let yy = (y as i64 + dy).clamp(0, ny as i64 - 1) as usize;
                        if slope_at(xx, yy, &z) > PIN_MAX_SLOPE {
                            ok = false;
                            break 'disc;
                        }
                    }
                }
                if ok {
                    pinnable[i] = true;
                    count += 1;
                }
            }
        }
        let area_pin = count as f64 * cell * cell;
        let share = area_pin / gs.area.max(1.0);
        final_share = share;
        // Largest 4-connected pocket.
        let mut seen = vec![false; spec.len()];
        let mut max_pocket = 0usize;
        for start in 0..spec.len() {
            if !pinnable[start] || seen[start] {
                continue;
            }
            let mut stack = vec![start];
            seen[start] = true;
            let mut size = 0usize;
            while let Some(c) = stack.pop() {
                size += 1;
                let (x, y) = (c % nx, c / nx);
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let xx = x as i64 + dx;
                    let yy = y as i64 + dy;
                    if xx < 0 || yy < 0 || xx >= nx as i64 || yy >= ny as i64 {
                        continue;
                    }
                    let j = yy as usize * nx + xx as usize;
                    if pinnable[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
            max_pocket = max_pocket.max(size);
        }
        let pocket_ok = max_pocket as f64 * cell * cell >= POCKET_M2;
        if share >= PINNABLE_SHARE && pocket_ok {
            break;
        }
        // Relax knobs, hardest offenders first; final pass flattens features.
        match pass {
            0 => {
                for f in &mut draw.features {
                    f.1 *= 0.7;
                }
            }
            1 => {
                for t in &mut draw.tiers {
                    t.2 *= 0.8;
                }
            }
            2 => {
                draw.tilt *= 0.8;
                draw.cross *= 0.8;
            }
            _ => {
                draw.features.clear();
                draw.tiers.clear();
            }
        }
    }

    // Weight + fringe blend to base.
    let min_axis = gs.a.min(gs.b).max(1.0);
    let r_fringe = 1.0 + FRINGE_M / min_axis;
    let r_feather = 1.0 + (FRINGE_M + GREEN_FEATHER) / min_axis;
    let mut w = Grid::filled(spec, 0.0f64);
    for gy in 0..spec.ny {
        for gx in 0..spec.nx {
            let p = spec.world_of(gx, gy);
            let i = spec.index(gx, gy);
            let rn = rn_buf[i];
            if rn > r_feather {
                continue;
            }
            if rn > r_fringe {
                let t = (rn - r_fringe) / (r_feather - r_fringe);
                let tt = math::smoothstep(0.0, 1.0, t);
                z.data[i] += (ct.height_at(p) - z.data[i]) * tt;
                w.data[i] = 1.0 - tt;
            } else {
                w.data[i] = 1.0;
            }
        }
    }

    // Pin: strategic argmax over pinnable cells.
    let approach = fw.spine.length();
    let target_r = 0.30 + 0.20 * ((approach - 150.0) / 300.0).clamp(0.0, 1.0);
    let tier_pref = if draw.tiers.is_empty() {
        None
    } else {
        Some(rng.below(draw.tiers.len() + 1))
    };
    let mut scored: Vec<(f64, usize)> = Vec::new();
    for y in 0..ny {
        for x in 0..nx {
            let i = y * nx + x;
            if !pinnable[i] {
                continue;
            }
            let p = spec.world_of(x as u32, y as u32);
            let rn = rn_buf[i];
            let mut score = -0.6 * (rn - target_r).abs();
            if let Some(g) = gs.guard_dir {
                score += 0.5 * (p - gs.center).normalized().dot(g) * rn;
            }
            if let Some(pref) = tier_pref {
                let t_here = draw
                    .tiers
                    .iter()
                    .filter(|&&(c, n, _, _)| (p - c).dot(n) > 0.0)
                    .count();
                if t_here == pref {
                    score += 0.4;
                }
            }
            score += 0.03 * noise::perlin2(p.x / 6.0, p.y / 6.0, draw.micro_seed ^ 0x9e37);
            scored.push((score, i));
        }
    }
    // Best-scored candidate that also passes the EXACT boundary-distance
    // margin (the raster pass uses a cheap radial bound that can under-guard
    // strongly concave petal boundaries). Deterministic order.
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.cmp(&b.1))
    });
    let mut pick: Option<usize> = None;
    for &(_, i) in &scored {
        let p = spec.world_of((i % nx) as u32, (i / nx) as u32);
        if gs.boundary_dist(p) >= PIN_EDGE_MARGIN {
            pick = Some(i);
            break;
        }
    }
    // The enforcement loop guarantees pinnable cells exist; fall back to the
    // center defensively anyway.
    let (pin, tier) = match pick {
        Some(i) => {
            let p = spec.world_of((i % nx) as u32, (i / nx) as u32);
            let t = draw
                .tiers
                .iter()
                .filter(|&&(c, n, _, _)| (p - c).dot(n) > 0.0)
                .count() as u8;
            (p, t)
        }
        None => (gs.center, 0),
    };

    (Patch { z, w }, pin, tier, final_share)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unit_green(coefs: [(f64, f64); 4], n: f64) -> GreenShape {
        let mut gs = GreenShape {
            center: Vec2::new(500.0, 500.0),
            rot: 0.7,
            a: 16.0,
            b: 11.0,
            n,
            coefs,
            outline: Vec::new(),
            area: 0.0,
            guard_dir: None,
        };
        gs.outline = (0..128)
            .map(|k| gs.boundary(k as f64 / 128.0 * std::f64::consts::TAU))
            .collect();
        let mut acc = 0.0;
        for k in 0..128 {
            let a = gs.outline[k];
            let b = gs.outline[(k + 1) % 128];
            acc += a.x * b.y - b.x * a.y;
        }
        gs.area = 0.5 * acc.abs();
        gs
    }

    #[test]
    fn rnorm_is_one_on_boundary() {
        let gs = unit_green([(0.0, 0.0), (0.12, 1.0), (0.06, 2.0), (0.03, 0.5)], 2.0);
        for k in 0..32 {
            let th = k as f64 / 32.0 * std::f64::consts::TAU;
            let p = gs.boundary(th);
            let rn = gs.rnorm(p);
            assert!((rn - 1.0).abs() < 1e-6, "θ {th}: rnorm {rn}");
        }
        assert!(gs.rnorm(gs.center) < 0.05);
    }

    #[test]
    fn superellipse_area_close_to_expected() {
        // Pure ellipse: area = πab.
        let gs = unit_green([(0.0, 0.0); 4], 2.0);
        let expect = std::f64::consts::PI * 16.0 * 11.0;
        assert!((gs.area - expect).abs() / expect < 0.01, "{} vs {expect}", gs.area);
    }
}
