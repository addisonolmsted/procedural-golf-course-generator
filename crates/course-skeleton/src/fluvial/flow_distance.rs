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

use crate::kernel::Channel;

use super::trunk::{arc_len, point_at_arc};

/// A rasterized channel cell, ready to seed the Dijkstra.
#[derive(Clone, Copy, Debug)]
pub struct ChannelCell {
    pub lin: usize,
    /// Which channel this cell belongs to (index into the channel list).
    pub channel: u32,
    pub order: u8,
    /// The channel base elevation at this cell (monotone along network).
    pub z_channel: f64,
    /// Implied-surface elevation at this cell.
    pub implied: f64,
    /// Network arc to the outlet, metres (diagnostic).
    pub outlet_arc_m: f64,
}

/// Incision depth at a network position: deepest at the outlet, shallowing
/// upstream, never zero (a channel is always a cut).
fn depth(net_arc_m: f64, max_arc_m: f64, d_mouth_m: f64) -> f64 {
    let u = if max_arc_m > 0.0 { (net_arc_m / max_arc_m).clamp(0.0, 1.0) } else { 0.0 };
    1.0 + (d_mouth_m - 1.0) * libm::pow(1.0 - u, 0.7)
}

/// Rasterize the network with base elevations. `d_mouth_m` is the incision
/// depth at the outlet (integration-scaled by the caller).
pub fn rasterize(
    channels: &[Channel],
    spec: &GridSpec,
    implied: &Grid<f64>,
    base_elev_m: f64,
    d_mouth_m: f64,
) -> Vec<ChannelCell> {
    // Network arc from each channel's downstream end (pts[0]) to the outlet.
    let mut down_arc = vec![0.0f64; channels.len()];
    for (i, c) in channels.iter().enumerate() {
        let mut arc = 0.0;
        let mut cur = c.parent;
        let mut junction = c.junction_arc_m;
        while let Some(p) = cur {
            arc += junction;
            junction = channels[p as usize].junction_arc_m;
            cur = channels[p as usize].parent;
        }
        down_arc[i] = arc;
    }
    let max_net_arc = channels
        .iter()
        .enumerate()
        .map(|(i, c)| down_arc[i] + arc_len(&c.pts))
        .fold(0.0f64, f64::max);

    let cell = spec.cell_size;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let step = cell * 0.5;

    // Per-channel sampled (arc, z) so children can seed at the junction.
    let mut samples: Vec<Vec<(f64, f64)>> = Vec::with_capacity(channels.len());
    let mut best: Vec<Option<ChannelCell>> = vec![None; nx * ny];

    for (ci, c) in channels.iter().enumerate() {
        let total = arc_len(&c.pts);
        // Seed elevation at the downstream end.
        let mut z_prev = match c.parent {
            None => base_elev_m,
            Some(p) => {
                let ps = &samples[p as usize];
                let ja = c.junction_arc_m;
                // parent's z at the junction arc (samples are arc-sorted)
                match ps.binary_search_by(|(s, _)| s.total_cmp(&ja)) {
                    Ok(k) => ps[k].1,
                    Err(k) => {
                        let k = k.min(ps.len().saturating_sub(1));
                        ps.get(k).map_or(base_elev_m, |v| v.1)
                    }
                }
            }
        };
        let mut own: Vec<(f64, f64)> = Vec::new();
        let mut s = 0.0;
        while s <= total {
            let (p, _t) = point_at_arc(&c.pts, s);
            let net_arc = down_arc[ci] + s;
            let z_raw = implied.bilinear(p) - depth(net_arc, max_net_arc, d_mouth_m);
            // monotone upstream: never fall below the downstream elevation
            let z_ch = z_raw.max(z_prev + 0.02);
            z_prev = z_ch;
            own.push((s, z_ch));

            let (gx, gy) = ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64);
            if gx >= 0 && gy >= 0 && (gx as usize) < nx && (gy as usize) < ny {
                let lin = gy as usize * nx + gx as usize;
                let rec = ChannelCell {
                    lin,
                    channel: ci as u32,
                    order: c.order,
                    z_channel: z_ch,
                    implied: implied.bilinear(p),
                    outlet_arc_m: net_arc,
                };
                match &best[lin] {
                    // junction cells belong to the larger stream (smaller arc)
                    Some(b) if b.outlet_arc_m <= rec.outlet_arc_m => {}
                    _ => best[lin] = Some(rec),
                }
            }
            s += step;
        }
        samples.push(own);
    }
    let mut out: Vec<ChannelCell> = best.into_iter().flatten().collect();
    out.sort_by_key(|c| c.lin);
    out
}

/// Payload carried by the Dijkstra wavefront.
#[derive(Clone, Copy, Debug)]
pub struct Nearest {
    pub dist_m: f64,
    /// Base elevation of the nearest channel cell (or seed datum).
    pub z_channel: f64,
    /// Implied-surface elevation at that cell.
    pub implied_channel: f64,
    pub order: u8,
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
        Nearest { dist_m: f64::INFINITY, z_channel: 0.0, implied_channel: 0.0, order: 0 };
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
                Nearest { dist_m: 0.0, z_channel: elev, implied_channel: implied.bilinear(p), order: 0 },
            )
        })
        .collect()
}
