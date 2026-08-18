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

/// EXACT Euclidean distance to the nearest seed, with the seed's payload.
///
/// This replaced the Dijkstra above, and the reason is geometric rather
/// than numerical. Every cut in S2 is a function of this distance, so the
/// PLAN-VIEW BOUNDARY of every cut is one of its iso-contours. An
/// 8-connected Dijkstra's iso-contours are octagons — flat sides on the
/// grid axes and the diagonals — so a valley of roughly constant width
/// came out as a ribbon with straight, grid-aligned edges and square
/// corners: "square/rectangular cut outs on the surface" (review, three
/// rounds running). The true Euclidean field has circular iso-contours and
/// carries no grid signature at all.
///
/// Felzenszwalb–Huttenlocher: two separable passes of the 1-D lower
/// envelope of parabolas, O(n) total, tracking the arg-min so the payload
/// still rides along. Cheaper than the binary heap it replaces.
pub fn euclidean(spec: &GridSpec, seeds: &[(usize, Nearest)]) -> Vec<Nearest> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    let cell = spec.cell_size;
    let none = Nearest {
        dist_m: f64::INFINITY,
        z_channel: 0.0,
        implied_channel: 0.0,
        order: 0,
        area_m2: 0.0,
    };
    if seeds.is_empty() {
        return vec![none; n];
    }
    // A seed cell may be claimed twice (two polyline samples landing in
    // one cell); first writer wins, which is deterministic given the seed
    // order the caller builds.
    let mut payload: Vec<Option<Nearest>> = vec![None; n];
    for (lin, near) in seeds {
        if payload[*lin].is_none() {
            payload[*lin] = Some(*near);
        }
    }
    const FAR: f64 = 1.0e15;
    // ---- pass 1: down each column, in cell units ----------------------
    let mut d1 = vec![0.0f64; n];
    let mut y_of = vec![0usize; n];
    {
        let mut f = vec![0.0f64; ny];
        let mut v = vec![0usize; ny];
        let mut zb = vec![0.0f64; ny + 1];
        for x in 0..nx {
            for y in 0..ny {
                f[y] = if payload[y * nx + x].is_some() { 0.0 } else { FAR };
            }
            envelope(&f, &mut v, &mut zb, ny, |y, q| {
                d1[y * nx + x] = f[q] + ((y as f64 - q as f64) * (y as f64 - q as f64));
                y_of[y * nx + x] = q;
            });
        }
    }
    // ---- pass 2: along each row ---------------------------------------
    let mut out = vec![none; n];
    {
        let mut f = vec![0.0f64; nx];
        let mut v = vec![0usize; nx];
        let mut zb = vec![0.0f64; nx + 1];
        for y in 0..ny {
            f[..nx].copy_from_slice(&d1[y * nx..y * nx + nx]);
            envelope(&f, &mut v, &mut zb, nx, |x, q| {
                let d2 = f[q] + ((x as f64 - q as f64) * (x as f64 - q as f64));
                let lin = y * nx + x;
                if d2 >= FAR {
                    return;
                }
                let sy = y_of[y * nx + q];
                out[lin] = match payload[sy * nx + q] {
                    Some(p) => Nearest { dist_m: d2.max(0.0).sqrt() * cell, ..p },
                    None => none,
                };
            });
        }
    }
    out
}

/// One 1-D squared-distance transform: the lower envelope of the parabolas
/// `f(q) + (p − q)²`. Calls `emit(p, argmin_q)` for every p.
fn envelope(f: &[f64], v: &mut [usize], z: &mut [f64], n: usize, mut emit: impl FnMut(usize, usize)) {
    let mut k = 0usize;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..n {
        let mut s;
        loop {
            let p = v[k];
            s = ((f[q] + (q * q) as f64) - (f[p] + (p * p) as f64))
                / (2.0 * q as f64 - 2.0 * p as f64);
            if s <= z[k] && k > 0 {
                k -= 1;
            } else {
                break;
            }
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    let mut k = 0usize;
    for p in 0..n {
        while z[k + 1] < p as f64 {
            k += 1;
        }
        emit(p, v[k]);
    }
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
