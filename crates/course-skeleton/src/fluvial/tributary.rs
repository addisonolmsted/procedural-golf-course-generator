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

/// Minimum channel worth growing at all.
pub const MIN_CHANNEL_M: f64 = 260.0;

/// The pre-drawn transcript for the whole network, fixed size.
pub struct Draws {
    pub outlet_t: f64,
    pub trunk_steps: Vec<f64>,          // MAX_TRUNK_STEPS
    pub t1: Vec<(f64, f64, Vec<f64>)>,  // MAX_T1 × (arc jitter, angle jitter, steps)
    pub t2: Vec<(f64, f64, Vec<f64>)>,  // MAX_T2
    pub t3: Vec<(f64, f64, Vec<f64>)>,  // MAX_T3
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
        Draws { outlet_t, trunk_steps, t1, t2, t3 }
    }
}

/// Build the channel network. Returns channels in construction order
/// (trunk first), with Strahler orders assigned bottom-up afterwards.
pub fn build(
    steer: &Steer,
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

    assign_strahler(&mut channels);
    channels
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
