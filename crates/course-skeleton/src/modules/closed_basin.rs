//! Closed-basin module: the point-process embryos that seed basins, plus
//! closed-basin tolerance in the flow field. Embryos are recorded in the
//! artifact — S3's polish and S7's repair both need to know which pits are
//! intended. The heathland kettle mechanism; zero for integrated biomes.
//!
//! Placement draws are FIXED-count (`MAX_EMBRYOS` triples); intensity only
//! decides how many of the pre-drawn candidates become pits, and candidates
//! landing on a channel are skipped (a kettle on a channel is a pond, which
//! is S4's business, not a landform).

use course_world::grid::Grid;
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

use crate::fluvial::flow_distance::Nearest;
use crate::kernel::BasinEmbryo;

pub const MAX_EMBRYOS: usize = 48;

/// Uniform draws consumed per embryo candidate (x, y, size).
pub const DRAWS_PER_EMBRYO: usize = 3;

/// Place embryos and stamp their pits into the 8 m base surface.
pub fn apply(
    z: &mut Grid<f64>,
    near: &[Nearest],
    draws: &[f64], // MAX_EMBRYOS * DRAWS_PER_EMBRYO uniforms
    intensity: f64,
    relief_budget_m: f64,
) -> Vec<BasinEmbryo> {
    if intensity <= 0.0 {
        return Vec::new();
    }
    let want = ((intensity * 32.0).round() as usize).min(MAX_EMBRYOS);
    let cell = z.spec.cell_size;
    let nx = z.spec.nx as usize;
    let ny = z.spec.ny as usize;
    // Local TPI of the CURRENT surface (~400 m box), for the high-ground
    // pock candidates: the reviewer's real-tile observation is circular
    // pock marks "taken out of higher points or right at the edge of a
    // ridge line", which uniform placement almost never produces. TPI
    // above the tile's q60 = the upper shoulder/ridge zone.
    let tpi = {
        let w = ((400.0 / cell) as usize | 1).max(3);
        let h = (w / 2) as i64;
        let mut mean = vec![0.0f64; nx * ny];
        let mut tmp = vec![0.0f64; nx * ny];
        for y in 0..ny {
            for x in 0..nx {
                let mut acc = 0.0;
                for dx in -h..=h {
                    let xx = (x as i64 + dx).clamp(0, nx as i64 - 1) as usize;
                    acc += z.data[y * nx + xx];
                }
                tmp[y * nx + x] = acc / (2 * h + 1) as f64;
            }
        }
        for y in 0..ny {
            for x in 0..nx {
                let mut acc = 0.0;
                for dy in -h..=h {
                    let yy = (y as i64 + dy).clamp(0, ny as i64 - 1) as usize;
                    acc += tmp[yy * nx + x];
                }
                mean[y * nx + x] = acc / (2 * h + 1) as f64;
            }
        }
        let mut t = vec![0.0f64; nx * ny];
        for i in 0..nx * ny {
            t[i] = z.data[i] - mean[i];
        }
        t
    };
    let tpi_q60 = {
        let mut v = tpi.clone();
        v.sort_by(|a, b| a.total_cmp(b));
        v[(v.len() - 1) * 6 / 10]
    };
    let mut out: Vec<BasinEmbryo> = Vec::new();
    for k in 0..MAX_EMBRYOS {
        if out.len() >= want {
            break;
        }
        let (ux, uy, us) = (
            draws[k * DRAWS_PER_EMBRYO],
            draws[k * DRAWS_PER_EMBRYO + 1],
            draws[k * DRAWS_PER_EMBRYO + 2],
        );
        let c = Vec2::new(
            (0.06 + 0.88 * ux) * EXTENT_M,
            (0.06 + 0.88 * uy) * EXTENT_M,
        );
        let (gx, gy) = ((c.x / cell) as usize, (c.y / cell) as usize);
        let lin = gy * nx + gx;
        // keep kettles off channels
        if near.get(lin).is_some_and(|n| n.dist_m < 90.0) {
            continue;
        }
        // Index split, transcript-safe (accept/skip on pre-drawn
        // candidates, never conditional draws): even candidates are the
        // classic uniform kettles (lows pond via S4's water table); odd
        // candidates are POCKS accepted only on high ground.
        let is_pock = k % 2 == 1;
        if is_pock && tpi.get(lin).is_none_or(|t| *t <= tpi_q60) {
            continue;
        }
        let radius = if is_pock { 40.0 + 70.0 * us } else { 70.0 + 130.0 * us };
        // Separation: overlapping stamped Gaussians double-deepen and the
        // drawn circles overlapped constantly (review). Real compound
        // kettles exist but as the exception; candidates too close to an
        // accepted embryo are skipped (no draws consumed conditionally —
        // all pre-drawn).
        let too_close = out.iter().any(|e: &BasinEmbryo| {
            let dx = e.center.x - c.x;
            let dy = e.center.y - c.y;
            (dx * dx + dy * dy).sqrt() < 0.85 * (e.radius_m + radius)
        });
        if too_close {
            continue;
        }
        let depth = if is_pock {
            (1.0 + 1.6 * us) * (relief_budget_m / 30.0).clamp(0.4, 1.6)
        } else {
            (1.6 + 2.6 * us) * (relief_budget_m / 30.0).clamp(0.4, 1.6)
        };
        stamp_pit(z, c, radius, depth);
        out.push(BasinEmbryo { center: c, radius_m: radius, depth_m: depth });
    }
    out
}

fn stamp_pit(z: &mut Grid<f64>, c: Vec2, radius_m: f64, depth_m: f64) {
    let cell = z.spec.cell_size;
    let (nx, ny) = (z.spec.nx as usize, z.spec.ny as usize);
    let r_cells = (radius_m * 1.6 / cell).ceil() as i64;
    let (cx, cy) = ((c.x / cell) as i64, (c.y / cell) as i64);
    for dy in -r_cells..=r_cells {
        for dx in -r_cells..=r_cells {
            let (x, y) = (cx + dx, cy + dy);
            if x < 0 || y < 0 || x >= nx as i64 || y >= ny as i64 {
                continue;
            }
            let px = (x as f64 + 0.0) * cell;
            let py = (y as f64 + 0.0) * cell;
            let d2 = (px - c.x) * (px - c.x) + (py - c.y) * (py - c.y);
            let s2 = radius_m * radius_m * 0.5;
            let bump = libm::exp(-d2 / (2.0 * s2));
            z.data[y as usize * nx + x as usize] -= depth_m * bump;
        }
    }
}
