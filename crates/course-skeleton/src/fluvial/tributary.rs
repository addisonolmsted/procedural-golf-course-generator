//! Top-down Horton–Strahler network construction (stage-02 open question 3,
//! resolved top-down: the ratios hold by construction, so the hierarchy
//! invariant is structural rather than hoped-for).
//!
//! The plan: total channel length comes from `density_target` × window
//! area. The trunk takes its share first; the remainder is spent on a
//! fixed-ratio tree — each trunk-level tributary carries a subtree with
//! bifurcation RB=4 and length ratio RL≈2.1, three levels deep. Counts are
//! *computed* (deterministically, from the dials), draws are *fixed*: the
//! caller pre-draws the maximum transcript and growth consumes prefixes.
//!
//! Density ≈ 0 (sandhills) yields zero channels through the same formula —
//! the engine runs, the network is empty, and that is not a special case.

use course_world::grid::GridSpec;
use course_world::math::Vec2;

use super::trunk::{self, Steer, Stop, STEP_M};
use crate::kernel::Channel;

pub const RB: usize = 4; // bifurcation ratio target
pub const RL: f64 = 2.1; // length ratio target

/// Fixed maxima — the draw transcript is sized by these, never by dials.
pub const MAX_TRUNK_STEPS: usize = 34; // ~4 km
pub const MAX_T1: usize = 6; // trunk-level tributaries (order Ω-1)
pub const MAX_T2: usize = MAX_T1 * RB; // 24
pub const MAX_T3: usize = MAX_T2 * RB; // 96
pub const T1_STEPS: usize = 16;
pub const T2_STEPS: usize = 8;
pub const T3_STEPS: usize = 4;

/// Space-filling infill: headward order-1 fingers placed at the farthest
/// point from the network and grown downhill until they attach. This is
/// what makes the accumulation network hit the shared d2c invariant — the
/// hierarchy alone clusters around the trunk corridor.
pub const MAX_INFILL: usize = 56;
pub const INFILL_STEPS: usize = 30;
pub const INFILL_STEP_M: f64 = 110.0;
/// Stop placing fingers once every cell is within this of a channel.
pub const INFILL_PLACE_THRESH_M: f64 = 310.0;
/// A finger attaches when it comes this close to an existing channel.
pub const INFILL_ATTACH_M: f64 = 60.0;

/// Minimum channel worth growing at all.
pub const MIN_CHANNEL_M: f64 = 260.0;

/// The pre-drawn transcript for the whole network, fixed size.
pub struct Draws {
    pub outlet_t: f64,
    pub trunk_steps: Vec<f64>,          // MAX_TRUNK_STEPS
    pub t1: Vec<(f64, f64, Vec<f64>)>,  // MAX_T1 × (arc jitter, angle jitter, steps)
    pub t2: Vec<(f64, f64, Vec<f64>)>,  // MAX_T2
    pub t3: Vec<(f64, f64, Vec<f64>)>,  // MAX_T3
    pub infill: Vec<(f64, Vec<f64>)>,   // MAX_INFILL × (angle jitter, steps)
}

impl Draws {
    /// Consume the fixed transcripts. Draw counts are compile-time
    /// constants; nothing here depends on dials or geometry.
    ///
    /// `skeleton/trunk/v1`: 1 outlet + MAX_TRUNK_STEPS jitters.
    /// `skeleton/tributary/v1`: per level, count × (2 + steps) uniforms.
    pub fn from_streams(
        trunk_rng: &mut course_seed::DetRng,
        trib_rng: &mut course_seed::DetRng,
    ) -> Draws {
        let outlet_t = trunk_rng.next_f64();
        let trunk_steps = (0..MAX_TRUNK_STEPS).map(|_| trunk_rng.next_f64()).collect();
        let mut level = |count: usize, per: usize| -> Vec<(f64, f64, Vec<f64>)> {
            (0..count)
                .map(|_| {
                    let a = trib_rng.next_f64();
                    let b = trib_rng.next_f64();
                    let s = (0..per).map(|_| trib_rng.next_f64()).collect();
                    (a, b, s)
                })
                .collect()
        };
        let t1 = level(MAX_T1, T1_STEPS);
        let t2 = level(MAX_T2, T2_STEPS);
        let t3 = level(MAX_T3, T3_STEPS);
        let infill = (0..MAX_INFILL)
            .map(|_| {
                let a = trib_rng.next_f64();
                let s = (0..INFILL_STEPS).map(|_| trib_rng.next_f64()).collect();
                (a, s)
            })
            .collect();
        Draws { outlet_t, trunk_steps, t1, t2, t3, infill }
    }
}

/// Build the channel network. Returns channels in construction order
/// (trunk first), with Strahler orders assigned bottom-up afterwards.
pub fn build(
    steer: &Steer,
    spec8: &GridSpec,
    draws: &Draws,
    density_target_km_km2: f64,
    derangement: f64, // [0,1]: fraction of sub-trunk channels dropped (heathland)
) -> Vec<Channel> {
    let l_total_m = density_target_km_km2 * 9.0 * 1000.0; // 9 km² window
    if l_total_m < MIN_CHANNEL_M {
        return Vec::new();
    }

    // ---- trunk ---------------------------------------------------------
    let edge = steer.meta.base_level.edge;
    // Outlet in the middle 60% of the edge.
    let t = 0.2 + 0.6 * draws.outlet_t;
    let start = trunk::edge_point(edge, t);
    let trunk_budget = l_total_m.min(MAX_TRUNK_STEPS as f64 * STEP_M);
    let (mut trunk_pts, _stop) = trunk::grow(
        steer,
        start,
        trunk::edge_inward(edge),
        trunk_budget,
        STEP_M,
        &draws.trunk_steps,
        true,
    );
    // Negative integration (derangement) cuts the trunk's downstream reach:
    // a deranged network dangles in the interior instead of reaching base
    // level, which is what drops connectivity for heathland.
    if derangement > 0.0 && trunk_pts.len() > 2 {
        let cut = ((trunk_pts.len() as f64) * 0.55 * derangement).ceil() as usize;
        let cut = cut.max(1).min(trunk_pts.len() - 2);
        trunk_pts.drain(..cut);
    }
    if trunk_pts.len() < 2 {
        return Vec::new();
    }
    let lt = trunk::arc_len(&trunk_pts);
    let mut channels = vec![Channel {
        pts: trunk_pts,
        order: 1, // reassigned below
        parent: None,
        junction_arc_m: f64::NAN,
    }];

    // ---- how much tree the remaining budget buys -----------------------
    let l1 = lt * 0.45;
    let subtree_len = l1 * (1.0 + RB as f64 / RL + (RB * RB) as f64 / (RL * RL));
    let remaining = (l_total_m - lt).max(0.0);
    let n1 = if remaining >= 0.5 * l1 {
        (((remaining / subtree_len).round() as usize).max(1)).min(MAX_T1)
    } else {
        0
    };

    // ---- grow the hierarchy, level by level ----------------------------
    // Junctions along the parent alternate sides; arc positions are evenly
    // spread with jitter. Children sprout at ±(55°±15°) from the parent's
    // local upstream tangent.
    let mut level_parents: Vec<usize> = vec![0]; // trunk index
    let mut next_parents: Vec<usize> = Vec::new();
    let levels: [(usize, &Vec<(f64, f64, Vec<f64>)>, f64, f64); 3] = [
        (n1, &draws.t1, l1, STEP_M),
        (n1 * RB, &draws.t2, l1 / RL, 90.0),
        (n1 * RB * RB, &draws.t3, l1 / (RL * RL), 70.0),
    ];
    for (li, (count, level_draws, len, step)) in levels.iter().enumerate() {
        let count = (*count).min(level_draws.len());
        if count == 0 || level_parents.is_empty() {
            break;
        }
        let per_parent = (count / level_parents.len()).max(1);
        let mut spawned = 0usize;
        'outer: for (pi, &parent_idx) in level_parents.iter().enumerate() {
            let parent_pts = channels[parent_idx].pts.clone();
            let plen = trunk::arc_len(&parent_pts);
            for k in 0..per_parent {
                if spawned >= count {
                    break 'outer;
                }
                let (arc_u, ang_u, step_draws) = &level_draws[spawned];
                spawned += 1;
                // Deterministic derangement: drop a fraction of sub-trunk
                // channels by their position in the fixed transcript.
                if derangement > 0.0 {
                    let phase = (spawned as f64 * 0.6180339887498949) % 1.0;
                    if phase < derangement {
                        continue;
                    }
                }
                // arc position: spread children along the parent
                let frac = (k as f64 + 0.5) / per_parent as f64;
                let arc = plen * (0.12 + 0.72 * frac) + (arc_u - 0.5) * 0.15 * plen;
                let arc = arc.clamp(0.05 * plen, 0.95 * plen);
                let (jp, tangent) = trunk::point_at_arc(&parent_pts, arc);
                // alternate sides; ±55° ± 15° jitter
                let side = if (pi + k) % 2 == 0 { 1.0 } else { -1.0 };
                let ang = side * (0.96 + (ang_u - 0.5) * 0.52);
                let dir0 = trunk::rotate(tangent, ang);
                let (pts, stop) = trunk::grow(steer, jp, dir0, *len, *step, step_draws, false);
                let min_len = (0.55 * len).max(140.0);
                if trunk::arc_len(&pts) < min_len && stop != Stop::Scarp {
                    continue;
                }
                if pts.len() < 2 {
                    continue;
                }
                next_parents.push(channels.len());
                channels.push(Channel {
                    pts,
                    order: 1,
                    parent: Some(parent_idx as u32),
                    junction_arc_m: arc,
                });
            }
        }
        level_parents = std::mem::take(&mut next_parents);
        let _ = li;
    }

    // ---- space-filling infill ------------------------------------------
    // Only for integrated networks: infill IS integration, so a deranged
    // draw (derangement > 0) keeps its gaps and sandhills has nothing to
    // attach to. Fingers place at the farthest-from-network cell and grow
    // downhill until they attach — headward accretion, order-1 by nature.
    if derangement == 0.0 && !channels.is_empty() {
        grow_infill(steer, spec8, draws, &mut channels);
    }

    assign_strahler(&mut channels);
    channels
}

/// Stamp a channel's cells into the attach maps: which channel covers a
/// cell and the arc along it (from ITS downstream end, pts[0]).
fn stamp_channel(
    ci: usize,
    pts: &[Vec2],
    spec8: &GridSpec,
    chan_at: &mut [i32],
    arc_at: &mut [f64],
) {
    let cell = spec8.cell_size;
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    let total = trunk::arc_len(pts);
    let mut s = 0.0;
    while s <= total {
        let (p, _) = trunk::point_at_arc(pts, s);
        let (gx, gy) = ((p.x / cell).floor() as i64, (p.y / cell).floor() as i64);
        if gx >= 0 && gy >= 0 && (gx as usize) < nx && (gy as usize) < ny {
            let lin = gy as usize * nx + gx as usize;
            if chan_at[lin] < 0 {
                chan_at[lin] = ci as i32;
                arc_at[lin] = s;
            }
        }
        s += cell * 0.5;
    }
}

/// Two-pass 3-4 chamfer distance to the stamped channel cells, carrying the
/// nearest channel cell's linear index. Distances in metres (approximate).
fn chamfer(chan_at: &[i32], spec8: &GridSpec) -> (Vec<f64>, Vec<usize>) {
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    let unit = spec8.cell_size / 3.0;
    let big = f64::INFINITY;
    let mut d: Vec<f64> = chan_at.iter().map(|&c| if c >= 0 { 0.0 } else { big }).collect();
    let mut near: Vec<usize> = (0..nx * ny).map(|i| if chan_at[i] >= 0 { i } else { usize::MAX }).collect();
    let relax = |lin: usize, from: usize, w: f64, d: &mut Vec<f64>, near: &mut Vec<usize>| {
        if d[from] + w < d[lin] {
            d[lin] = d[from] + w;
            near[lin] = near[from];
        }
    };
    for y in 0..ny {
        for x in 0..nx {
            let lin = y * nx + x;
            if x > 0 {
                relax(lin, lin - 1, 3.0 * unit, &mut d, &mut near);
            }
            if y > 0 {
                relax(lin, lin - nx, 3.0 * unit, &mut d, &mut near);
                if x > 0 {
                    relax(lin, lin - nx - 1, 4.0 * unit, &mut d, &mut near);
                }
                if x + 1 < nx {
                    relax(lin, lin - nx + 1, 4.0 * unit, &mut d, &mut near);
                }
            }
        }
    }
    for y in (0..ny).rev() {
        for x in (0..nx).rev() {
            let lin = y * nx + x;
            if x + 1 < nx {
                relax(lin, lin + 1, 3.0 * unit, &mut d, &mut near);
            }
            if y + 1 < ny {
                relax(lin, lin + nx, 3.0 * unit, &mut d, &mut near);
                if x + 1 < nx {
                    relax(lin, lin + nx + 1, 4.0 * unit, &mut d, &mut near);
                }
                if x > 0 {
                    relax(lin, lin + nx - 1, 4.0 * unit, &mut d, &mut near);
                }
            }
        }
    }
    (d, near)
}

fn grow_infill(steer: &Steer, spec8: &GridSpec, draws: &Draws, channels: &mut Vec<Channel>) {
    let cell = spec8.cell_size;
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    let center = |lin: usize| {
        Vec2::new(
            ((lin % nx) as f64 + 0.5) * cell,
            ((lin / nx) as f64 + 0.5) * cell,
        )
    };
    let mut chan_at: Vec<i32> = vec![-1; nx * ny];
    let mut arc_at: Vec<f64> = vec![0.0; nx * ny];
    for ci in 0..channels.len() {
        let pts = channels[ci].pts.clone();
        stamp_channel(ci, &pts, spec8, &mut chan_at, &mut arc_at);
    }
    let margin = trunk::EDGE_MARGIN_M;
    let extent = course_world::world::EXTENT_M;
    // Failed placements are blacklisted so the argmax moves on — otherwise a
    // stuck corner cell would eat every remaining draw.
    let mut blocked = vec![false; nx * ny];

    for (ang_u, jits) in &draws.infill {
        let (dist, nearv) = chamfer(&chan_at, spec8);
        // Farthest un-blocked INTERIOR cell (growth needs room to move
        // without instantly exiting the edge margin), lowest index on ties.
        let inner = margin + INFILL_STEP_M;
        let (mut best, mut best_lin) = (0.0f64, usize::MAX);
        for lin in 0..nx * ny {
            if blocked[lin] {
                continue;
            }
            let c = center(lin);
            if c.x < inner || c.y < inner || c.x > extent - inner || c.y > extent - inner {
                continue;
            }
            if dist[lin] > best {
                best = dist[lin];
                best_lin = lin;
            }
        }
        if best < INFILL_PLACE_THRESH_M || best_lin == usize::MAX {
            break;
        }
        let mut p = center(best_lin);
        let mut prev = Vec2::new(0.0, 0.0);
        let mut pts: Vec<Vec2> = vec![p];
        let mut attach: Option<(i32, f64, Vec2)> = None;
        for (si, &u) in jits.iter().enumerate() {
            let pl = ((p.y / cell).floor() as usize).min(ny - 1) * nx
                + ((p.x / cell).floor() as usize).min(nx - 1);
            let nl = nearv[pl];
            if nl == usize::MAX {
                break;
            }
            if dist[pl] <= INFILL_ATTACH_M {
                attach = Some((chan_at[nl], arc_at[nl], center(nl)));
                break;
            }
            let t = center(nl);
            let dx = Vec2::new(t.x - p.x, t.y - p.y);
            let dl = (dx.x * dx.x + dx.y * dx.y).sqrt().max(1e-9);
            let to_net = Vec2::new(dx.x / dl, dx.y / dl);
            let g = steer.grad(steer.implied, p);
            let gl = (g.x * g.x + g.y * g.y).sqrt().max(1e-9);
            let down = Vec2::new(-g.x / gl, -g.y / gl);
            let mut dir = Vec2::new(
                1.3 * to_net.x + 0.6 * down.x + 0.8 * prev.x,
                1.3 * to_net.y + 0.6 * down.y + 0.8 * prev.y,
            );
            let dn = (dir.x * dir.x + dir.y * dir.y).sqrt().max(1e-9);
            dir = Vec2::new(dir.x / dn, dir.y / dn);
            let ang = (u - 0.5) * 0.8 + if si == 0 { (ang_u - 0.5) * 0.6 } else { 0.0 };
            dir = trunk::rotate(dir, ang);
            let q = Vec2::new(p.x + dir.x * INFILL_STEP_M, p.y + dir.y * INFILL_STEP_M);
            if q.x < margin || q.y < margin || q.x > extent - margin || q.y > extent - margin {
                break;
            }
            pts.push(q);
            prev = dir;
            p = q;
        }
        let Some((chan, arc, anchor)) = attach else {
            blocked[best_lin] = true;
            continue;
        };
        pts.push(anchor);
        pts.reverse(); // pts[0] = downstream junction end
        if trunk::arc_len(&pts) < 150.0 {
            blocked[best_lin] = true;
            continue;
        }
        let idx = channels.len();
        channels.push(Channel {
            pts: pts.clone(),
            order: 1,
            parent: Some(chan as u32),
            junction_arc_m: arc,
        });
        stamp_channel(idx, &pts, spec8, &mut chan_at, &mut arc_at);
    }
}

/// Bottom-up Strahler orders from the constructed tree.
fn assign_strahler(channels: &mut [Channel]) {
    let n = channels.len();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        if let Some(p) = channels[i].parent {
            children[p as usize].push(i);
        }
    }
    fn order_of(i: usize, children: &[Vec<usize>], memo: &mut [u8]) -> u8 {
        if memo[i] != 0 {
            return memo[i];
        }
        let kids = &children[i];
        let o = if kids.is_empty() {
            1
        } else {
            let mut orders: Vec<u8> =
                kids.iter().map(|&k| order_of(k, children, memo)).collect();
            orders.sort_unstable_by(|a, b| b.cmp(a));
            if orders.len() >= 2 && orders[0] == orders[1] {
                orders[0] + 1
            } else {
                orders[0].max(1)
            }
        };
        memo[i] = o;
        o
    }
    let mut memo = vec![0u8; n];
    for i in 0..n {
        order_of(i, &children, &mut memo);
    }
    for (i, ch) in channels.iter_mut().enumerate() {
        ch.order = memo[i];
    }
}

/// Achieved Horton ratios for diagnostics: geometric-mean bifurcation and
/// length ratios across adjacent orders (None with < 2 orders present).
pub fn horton_ratios(channels: &[Channel]) -> (Option<f64>, Option<f64>) {
    use std::collections::BTreeMap;
    let mut count: BTreeMap<u8, usize> = BTreeMap::new();
    let mut len: BTreeMap<u8, f64> = BTreeMap::new();
    for c in channels {
        *count.entry(c.order).or_default() += 1;
        *len.entry(c.order).or_default() += trunk::arc_len(&c.pts);
    }
    let orders: Vec<u8> = count.keys().copied().collect();
    if orders.len() < 2 {
        return (None, None);
    }
    let mut rb = Vec::new();
    let mut rl = Vec::new();
    for w in orders.windows(2) {
        let (lo, hi) = (w[0], w[1]);
        rb.push(count[&lo] as f64 / count[&hi] as f64);
        let mean_lo = len[&lo] / count[&lo] as f64;
        let mean_hi = len[&hi] / count[&hi] as f64;
        rl.push(mean_hi / mean_lo);
    }
    let gm = |v: &[f64]| libm::exp(v.iter().map(|x| libm::log(x.max(1e-9))).sum::<f64>() / v.len() as f64);
    (Some(gm(&rb)), Some(gm(&rl)))
}

/// The point of `Vec2` reexport for tests.
pub fn _vec2(x: f64, y: f64) -> Vec2 {
    Vec2::new(x, y)
}
