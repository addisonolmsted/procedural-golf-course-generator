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
    let want = ((intensity * 26.0).round() as usize).min(MAX_EMBRYOS);
    let cell = z.spec.cell_size;
    let nx = z.spec.nx as usize;
    let mut out = Vec::new();
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
        let radius = 70.0 + 130.0 * us;
        let depth = (1.6 + 2.6 * us) * (relief_budget_m / 30.0).clamp(0.4, 1.6);
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
