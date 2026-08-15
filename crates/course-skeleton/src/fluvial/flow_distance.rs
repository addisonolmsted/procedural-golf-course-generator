//! Channel rasterization + the flow-distance transform — the coordinates
//! the catena is expressed in and S3 reconstructs against.
//!
//! Rasterization walks each channel polyline at sub-cell intervals onto the
//! 8 m grid and assigns every channel cell its BASE elevation: the implied
//! surface minus an incision depth that decays upstream, then made monotone
//! by walking upstream from each channel's downstream end (the trunk seeds
//! at base-level elevation; a tributary seeds at its parent's elevation at
//! the junction — parents are constructed first, so construction order is
//! processing order). Monotone-along-network is therefore structural, and
//! "every channel cell drains downhill" holds by construction.
//!
//! The transform is one multi-source Dijkstra (8-connected, metric weights)
//! from all channel cells at once — the stage doc's required approach. Each
//! wavefront carries its source payload, so every cell knows its nearest
//! channel's base elevation and implied elevation.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use course_world::grid::{Grid, GridSpec};
use course_world::math::Vec2;



/// Payload carried by the Dijkstra wavefront.
#[derive(Clone, Copy, Debug)]
pub struct Nearest {
    pub dist_m: f64,
    /// Base elevation of the nearest channel cell (or seed datum).
    pub z_channel: f64,
    /// Implied-surface elevation at that cell.
    pub implied_channel: f64,
    pub order: u8,
    /// Drained area of the nearest channel cell (m²); 0 for edge/divide
    /// seeds. Rides the wavefront so the catena can key bank geometry
    /// off DISCHARGE, not just Strahler order.
    pub area_m2: f64,
}

#[derive(PartialEq)]
struct K(f64);
impl Eq for K {}
impl PartialOrd for K {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for K {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.total_cmp(&other.0)
    }
}

/// Multi-source Dijkstra from `seeds`. Deterministic: heap keys are
/// (dist, insertion order).
pub fn dijkstra(spec: &GridSpec, seeds: &[(usize, Nearest)]) -> Vec<Nearest> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    let cell = spec.cell_size;
    let mut out = vec![
        Nearest { dist_m: f64::INFINITY, z_channel: 0.0, implied_channel: 0.0, order: 0, area_m2: 0.0 };
        n
    ];
    let mut heap: BinaryHeap<Reverse<(K, u64, usize)>> = BinaryHeap::new();
    let mut tick = 0u64;
    for (lin, near) in seeds {
        out[*lin] = *near;
        heap.push(Reverse((K(near.dist_m), tick, *lin)));
        tick += 1;
    }
    const D8: [(i64, i64); 8] = [
        (-1, -1),
        (-1, 0),
        (-1, 1),
        (0, -1),
        (0, 1),
        (1, -1),
        (1, 0),
        (1, 1),
    ];
    while let Some(Reverse((K(d), _, lin))) = heap.pop() {
        if d > out[lin].dist_m {
            continue;
        }
        let (y, x) = ((lin / nx) as i64, (lin % nx) as i64);
        for (dy, dx) in D8 {
            let (yy, xx) = (y + dy, x + dx);
            if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                continue;
            }
            let nlin = yy as usize * nx + xx as usize;
            let w = cell * if dy != 0 && dx != 0 { std::f64::consts::SQRT_2 } else { 1.0 };
            let nd = d + w;
            if nd < out[nlin].dist_m {
                out[nlin] = Nearest { dist_m: nd, ..out[lin] };
                heap.push(Reverse((K(nd), tick, nlin)));
                tick += 1;
            }
        }
    }
    out
}

/// Seed list for the no-channel degenerate case: the base-level edge cells
/// at the base elevation — the only drainage datum that exists.
pub fn edge_seeds(
    spec: &GridSpec,
    edge: course_contracts::metadata::Edge,
    elev: f64,
    implied: &Grid<f64>,
) -> Vec<(usize, Nearest)> {
    use course_contracts::metadata::Edge;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut lins: Vec<usize> = Vec::new();
    let push_row = |lins: &mut Vec<usize>, y: usize| {
        (0..nx).for_each(|x| lins.push(y * nx + x))
    };
    let push_col = |lins: &mut Vec<usize>, x: usize| {
        (0..ny).for_each(|y| lins.push(y * nx + x))
    };
    match edge {
        Edge::S => push_row(&mut lins, 0),
        Edge::N => push_row(&mut lins, ny - 1),
        Edge::W => push_col(&mut lins, 0),
        Edge::E => push_col(&mut lins, nx - 1),
        Edge::CornerSw => {
            push_row(&mut lins, 0);
            push_col(&mut lins, 0);
        }
        Edge::CornerSe => {
            push_row(&mut lins, 0);
            push_col(&mut lins, nx - 1);
        }
        Edge::CornerNw => {
            push_row(&mut lins, ny - 1);
            push_col(&mut lins, 0);
        }
        Edge::CornerNe => {
            push_row(&mut lins, ny - 1);
            push_col(&mut lins, nx - 1);
        }
    }
    lins.sort_unstable();
    lins.dedup();
    lins.into_iter()
        .map(|lin| {
            let (y, x) = (lin / nx, lin % nx);
            let p = Vec2::new((x as f64 + 0.5) * spec.cell_size, (y as f64 + 0.5) * spec.cell_size);
            (
                lin,
                Nearest { dist_m: 0.0, z_channel: elev, implied_channel: implied.bilinear(p), order: 0, area_m2: 0.0 },
            )
        })
        .collect()
}
