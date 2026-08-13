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
pub const MAX_INFILL: usize = 88;
pub const INFILL_STEPS: usize = 42;
pub const INFILL_STEP_M: f64 = 110.0;
/// Stop placing fingers once every cell is within this of a channel.
pub const INFILL_PLACE_THRESH_M: f64 = 300.0;
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
        Some(trunk::wander_from(start, 0)),
        None,
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
    // Smooth BEFORE anything anchors to it: a final smoothing pass moved
    // parents ~15 m sideways under already-anchored children and every
    // junction became a crossing candidate (measured ~8/seed). With
    // parents smoothed first, children anchor to FINAL geometry.
    let trunk_pts = chaikin(&trunk_pts, 2);
    let lt = trunk::arc_len(&trunk_pts);
    let mut channels = vec![Channel {
        pts: trunk_pts,
        order: 1, // reassigned below
        parent: None,
        junction_arc_m: f64::NAN,
        area_m2: 0.0,
    }];

    // Occupancy rasters, maintained from the trunk onward: every accepted
    // channel is stamped, and every subsequent tributary TRIMS at its first
    // touch of an existing channel — tributaries join, they never cross
    // (the river-valley review caught a crossing knot).
    let cell = spec8.cell_size;
    let (rnx, rny) = (spec8.nx as usize, spec8.ny as usize);
    let mut chan_at: Vec<i32> = vec![-1; rnx * rny];
    let mut arc_at: Vec<f64> = vec![0.0; rnx * rny];
    stamp_channel(0, &channels[0].pts, spec8, &mut chan_at, &mut arc_at);
    let cell_of = |q: Vec2| -> usize {
        ((q.y / cell).floor() as usize).min(rny - 1) * rnx
            + ((q.x / cell).floor() as usize).min(rnx - 1)
    };

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
                // Arc position: spread children along the parent, with the
                // two SIDES interleaved by half a spacing — evenly-spaced
                // alternating stubs land nearly opposite each other and a
                // mirror pair reads as one channel crossing the parent
                // (the review's "crossings" at seed 765 were exactly this;
                // the geometric counter finds zero true crossings).
                let side_phase = 0.5 * (((pi + k) % 2) as f64);
                let frac = (k as f64 + 0.25 + side_phase) / per_parent as f64;
                let arc = plen * (0.12 + 0.72 * frac) + (arc_u - 0.5) * 0.15 * plen;
                let arc = arc.clamp(0.05 * plen, 0.95 * plen);
                let (jp, tangent) = trunk::point_at_arc(&parent_pts, arc);
                let spawn_side_of = |q: Vec2| {
                    let v = Vec2::new(q.x - jp.x, q.y - jp.y);
                    tangent.x * v.y - tangent.y * v.x
                };
                // alternate sides; ±55° ± 15° jitter
                let side = if (pi + k) % 2 == 0 { 1.0 } else { -1.0 };
                let ang = side * (0.96 + (ang_u - 0.5) * 0.52);
                let dir0 = trunk::rotate(tangent, ang);
                let (grown, stop) = trunk::grow(
                    steer,
                    jp,
                    dir0,
                    *len,
                    *step,
                    step_draws,
                    false,
                    Some(trunk::wander_from(jp, (li as u64) << 32 | spawned as u64)),
                    Some(&parent_pts),
                );
                let spawn_side = spawn_side_of(Vec2::new(jp.x + dir0.x, jp.y + dir0.y));
                // Trim at the first touch of an existing channel. The ONLY
                // exemption is the PARENT's own cells within the first 60 m
                // (the trib starts on its parent and must leave its
                // corridor). An earlier draft exempted ALL channels for the
                // first 100 m — in a dense junction cluster that window is
                // exactly where every stub crossed every other (the seed-6
                // knot the review caught).
                let mut pts: Vec<Vec2> = Vec::new();
                let mut walked = 0.0;
                'trim: for wnd in std::iter::once(&grown[..]).flat_map(|s| s.windows(2)) {
                    if pts.is_empty() {
                        pts.push(wnd[0]);
                    }
                    let seg = Vec2::new(wnd[1].x - wnd[0].x, wnd[1].y - wnd[0].y);
                    let seg_len = (seg.x * seg.x + seg.y * seg.y).sqrt();
                    let nsub = (seg_len / 4.0).ceil() as usize;
                    for t in 1..=nsub {
                        let f = t as f64 / nsub as f64;
                        let q = Vec2::new(wnd[0].x + seg.x * f, wnd[0].y + seg.y * f);
                        // ±40 m separation neighbourhood: channels closer
                        // than that would have merged in reality — the
                        // review caught a tributary paralleling its trunk
                        // ~30 m away for a kilometre.
                        let qc = cell_of(q);
                        let (qy, qx) = ((qc / rnx) as i64, (qc % rnx) as i64);
                        let mut blocked_here = false;
                        'nb: for dy in -5i64..=5 {
                            for dx in -5i64..=5 {
                                let (yy, xx) = (qy + dy, qx + dx);
                                if yy < 0 || xx < 0 || yy >= rny as i64 || xx >= rnx as i64 {
                                    continue;
                                }
                                let occ = chan_at[yy as usize * rnx + xx as usize];
                                if occ >= 0 {
                                    let near_junction = walked + seg_len * f <= 130.0;
                                    let is_parent = occ as usize == parent_idx;
                                    if !(near_junction && is_parent) {
                                        blocked_here = true;
                                        break 'nb;
                                    }
                                }
                            }
                        }
                        // The parent exemption lets the child TOUCH its
                        // parent's corridor while leaving — but never
                        // CROSS to the far side (the instrumented counter
                        // showed every crossing was child × own parent
                        // inside this window).
                        if !blocked_here
                            && walked + seg_len * f <= 150.0
                            && spawn_side_of(q) * spawn_side < 0.0
                        {
                            blocked_here = true;
                        }
                        if blocked_here {
                            break 'trim;
                        }
                    }
                    walked += seg_len;
                    pts.push(wnd[1]);
                }
                let min_len = (0.55 * len).max(140.0);
                if trunk::arc_len(&pts) < min_len && stop != Stop::Scarp {
                    continue;
                }
                if pts.len() < 2 {
                    continue;
                }
                let pts = validated_child(
                    &pts,
                    &channels[parent_idx].pts,
                    arc,
                    &chan_at,
                    parent_idx as i32,
                    spec8,
                );
                let idx = channels.len();
                next_parents.push(idx);
                channels.push(Channel {
                    pts: pts.clone(),
                    order: 1,
                    parent: Some(parent_idx as u32),
                    junction_arc_m: arc,
                    area_m2: 0.0,
                });
                stamp_channel(idx, &pts, spec8, &mut chan_at, &mut arc_at);
            }
        }
        level_parents = std::mem::take(&mut next_parents);
        let _ = li;
    }

    // ---- space-filling infill ------------------------------------------
    // Only for integrated networks: infill IS integration, so a deranged
    // draw (derangement > 0) keeps its gaps. And it needs a REAL fluvial
    // budget: a vestigial 300 m trunk from a near-zero density draw
    // (sandhills reaches 0.05) must not unlock tile-wide finger growth.
    // NOTE: this guard was applied once before and silently lost in a
    // structural edit of this block — the D5 battery caught its absence
    // (sandhills seeds carrying 2.2 km/km² networks). If it goes missing
    // again, the battery will fail again; that is the intended tripwire.
    if derangement == 0.0
        && !channels.is_empty()
        && density_target_km_km2 >= 0.5
        && std::env::var("INFILL_OFF").is_err()
    {
        grow_infill(steer, spec8, draws, &mut channels, &mut chan_at, &mut arc_at);
    }

    // FINAL GUARANTEE: no channel segment properly crosses another. The
    // growth-time guards prevent almost everything, but three review
    // rounds showed each closed hole shifts a rare variant elsewhere —
    // so the property is enforced by direct sweep: any remaining crossing
    // trims the later-built channel at the crossing point (unless that
    // would strand one of its children's junctions, in which case the
    // earlier channel is tried, else it is left and will show in the
    // counter). Deterministic, exact, and future-proof against new
    // growth features.
    trim_crossings(&mut channels);

    assign_strahler(&mut channels);
    channels
}

/// Pick the best VALIDATED variant of a child's geometry: corrected +
/// smoothed if it clears the parent, else smoothed raw, else raw. The
/// mouth-correction guard alone was insufficient because Chaikin runs
/// after it and can displace the near-mouth curve across a parent that
/// sits closer than the smoothing displacement.
fn validated_child(
    raw: &[Vec2],
    parent_pts: &[Vec2],
    junction_arc_m: f64,
    chan_at: &[i32],
    parent_id: i32,
    spec8: &GridSpec,
) -> Vec<Vec2> {
    // First crossing segment index, skipping only the anchor-touching
    // FIRST segment (not a distance zone — distance zones were a 20 m
    // blind spot that three review rounds of crossings hid inside).
    let first_cross = |child: &[Vec2]| -> Option<usize> {
        for (sj, wj) in child.windows(2).enumerate() {
            for wi in parent_pts.windows(2) {
                if sj == 0 {
                    continue;
                }
                if proper_cross(wi[0], wi[1], wj[0], wj[1]) {
                    return Some(sj);
                }
            }
        }
        None
    };
    let v1 = chaikin(
        &enforce_mouth_angle(raw, parent_pts, junction_arc_m, chan_at, parent_id, spec8),
        2,
    );
    if first_cross(&v1).is_none() {
        return v1;
    }
    let v2 = chaikin(raw, 2);
    if first_cross(&v2).is_none() {
        return v2;
    }
    // Terminal variant, crossing-free BY CONSTRUCTION: truncate raw before
    // its first crossing, then shrink the first segment until even it
    // clears (a short stub beats any crossing).
    let mut v3: Vec<Vec2> = match first_cross(raw) {
        Some(sj) => raw[..(sj + 1).max(2)].to_vec(),
        None => raw.to_vec(),
    };
    for _ in 0..8 {
        let still = parent_pts
            .windows(2)
            .any(|w| proper_cross(w[0], w[1], v3[0], v3[1]));
        if !still {
            break;
        }
        v3[1] = Vec2::new(0.5 * (v3[0].x + v3[1].x), 0.5 * (v3[0].y + v3[1].y));
        v3.truncate(2);
    }
    v3
}

fn proper_cross(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
    let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let ln = |p: Vec2, q: Vec2| ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt().max(1e-9);
    let (lab, lcd) = (ln(a, b), ln(c, d));
    let (d1, d2) = (o(a, b, c) / lab, o(a, b, d) / lab);
    let (d3, d4) = (o(c, d, a) / lcd, o(c, d, b) / lcd);
    d1.abs() > 0.5
        && d2.abs() > 0.5
        && d3.abs() > 0.5
        && d4.abs() > 0.5
        && d1 * d2 < 0.0
        && d3 * d4 < 0.0
}

fn trim_crossings(channels: &mut Vec<Channel>) {
    let arc_upto = |pts: &[Vec2], k: usize| -> f64 {
        pts[..k]
            .windows(2)
            .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
            .sum()
    };
    for _sweep in 0..4 {
        let mut acted = false;
        for j in 1..channels.len() {
            // earliest crossing of channel j against any earlier channel,
            // remembering the earlier channel's own crossing segment for
            // the fallback.
            let mut cut: Option<(usize, usize, usize)> = None; // (sj, i, si)
            for i in 0..j {
                let (a_pts, b_pts) = (&channels[i].pts, &channels[j].pts);
                for (sj, wj) in b_pts.windows(2).enumerate() {
                    for (si, wi) in a_pts.windows(2).enumerate() {
                        if proper_cross(wi[0], wi[1], wj[0], wj[1]) {
                            if cut.is_none_or(|(c, _, _)| sj < c) {
                                cut = Some((sj, i, si));
                            }
                            break;
                        }
                    }
                }
            }
            let Some((sj, i, si)) = cut else { continue };
            // Prefer trimming the later channel; fall back to the earlier
            // one when the cut would strand one of its children.
            let strands = |idx: usize, keep: usize, chans: &[Channel]| {
                let new_len = arc_upto(&chans[idx].pts, keep);
                chans
                    .iter()
                    .any(|c| c.parent == Some(idx as u32) && c.junction_arc_m > new_len - 10.0)
            };
            let keep_j = (sj + 1).max(2);
            if !strands(j, keep_j, channels) {
                channels[j].pts.truncate(keep_j);
                // A crossing on the FIRST segment survives truncation (a
                // channel needs 2 points) — shrink the segment toward the
                // mouth until it clears the other channel.
                if sj == 0 {
                    let other = channels[i].pts.clone();
                    let ch = &mut channels[j];
                    for _ in 0..6 {
                        let still = other
                            .windows(2)
                            .any(|w| proper_cross(w[0], w[1], ch.pts[0], ch.pts[1]));
                        if !still {
                            break;
                        }
                        ch.pts[1] = Vec2::new(
                            0.5 * (ch.pts[0].x + ch.pts[1].x),
                            0.5 * (ch.pts[0].y + ch.pts[1].y),
                        );
                    }
                }
                acted = true;
                continue;
            }
            let keep_i = (si + 1).max(2);
            if !strands(i, keep_i, channels) {
                channels[i].pts.truncate(keep_i);
                acted = true;
                continue;
            }
            // Both trims would strand children: remove a WHOLE subtree —
            // whichever of the two is smaller by total length (removing
            // the later one unconditionally nuked trunk-level subtrees on
            // scarpy hill-country seeds and the invariant test caught the
            // sparse result). Always resolvable, deterministic.
            let subtree_len = |root: usize, chans: &[Channel]| -> f64 {
                let mut in_tree = vec![false; chans.len()];
                in_tree[root] = true;
                loop {
                    let mut grew = false;
                    for c in 0..chans.len() {
                        if in_tree[c] {
                            continue;
                        }
                        if let Some(p) = chans[c].parent {
                            if in_tree[p as usize] {
                                in_tree[c] = true;
                                grew = true;
                            }
                        }
                    }
                    if !grew {
                        break;
                    }
                }
                (0..chans.len())
                    .filter(|&c| in_tree[c])
                    .map(|c| trunk::arc_len(&chans[c].pts))
                    .sum()
            };
            // never remove the trunk's subtree (that is the whole network)
            let cand_j = subtree_len(j, channels);
            let cand_i = if channels[i].parent.is_some() {
                subtree_len(i, channels)
            } else {
                f64::INFINITY
            };
            let victim = if cand_i < cand_j { i } else { j };
            let mut kill = vec![false; channels.len()];
            kill[victim] = true;
            loop {
                let mut grew = false;
                for c in 0..channels.len() {
                    if kill[c] {
                        continue;
                    }
                    if let Some(p) = channels[c].parent {
                        if kill[p as usize] {
                            kill[c] = true;
                            grew = true;
                        }
                    }
                }
                if !grew {
                    break;
                }
            }
            let mut remap = vec![u32::MAX; channels.len()];
            let mut next = 0u32;
            for (idx, k) in kill.iter().enumerate() {
                if !k {
                    remap[idx] = next;
                    next += 1;
                }
            }
            let old: Vec<Channel> = std::mem::take(channels);
            for (idx, mut c) in old.into_iter().enumerate() {
                if kill[idx] {
                    continue;
                }
                if let Some(p) = c.parent {
                    c.parent = Some(remap[p as usize]);
                }
                channels.push(c);
            }
            acted = true;
            break; // indices changed: restart the sweep
        }
        if !acted {
            break;
        }
    }
}

/// Junction-angle enforcement (review: 39% of junctions measured > 80° —
/// the spawn angle is correct but the first thalweg steering step destroys
/// it). The mouth segment is rotated to leave the parent's DOWNSTREAM
/// tangent at 30–62°, preserving the side it departs on; Chaikin then
/// blends the correction into the rest of the path.
/// Returns the (possibly) modified points. The correction INSERTS a short
/// 40 m guide point at the corrected angle instead of swinging the whole
/// first segment (an earlier draft rotated pts[1] — up to a 100 m lever —
/// through unchecked territory and crossings spiked to ~8/seed), and the
/// two affected segments are collision-checked against the occupancy
/// raster; on any hit the original geometry is kept (a T-junction is
/// better than a crossing).
#[allow(clippy::too_many_arguments)]
fn enforce_mouth_angle(
    pts: &[Vec2],
    parent_pts: &[Vec2],
    junction_arc_m: f64,
    chan_at: &[i32],
    parent_id: i32,
    spec8: &GridSpec,
) -> Vec<Vec2> {
    if pts.len() < 2 {
        return pts.to_vec();
    }
    let (_, up_tan) = trunk::point_at_arc(parent_pts, junction_arc_m);
    let down = Vec2::new(-up_tan.x, -up_tan.y);
    let m = Vec2::new(pts[1].x - pts[0].x, pts[1].y - pts[0].y);
    let ml = (m.x * m.x + m.y * m.y).sqrt().max(1e-9);
    let mn = Vec2::new(m.x / ml, m.y / ml);
    // pts[0] is the mouth; flow runs toward pts[0], so enforce on the
    // flow-frame angle vs the parent's downstream tangent.
    let dot = (-mn.x) * down.x + (-mn.y) * down.y;
    let ang = dot.clamp(-1.0, 1.0).acos();
    let lo = 30f64.to_radians();
    let hi = 62f64.to_radians();
    if ang >= lo && ang <= hi {
        return pts.to_vec();
    }
    let side = if down.x * mn.y - down.y * mn.x >= 0.0 { 1.0 } else { -1.0 };
    // Reflect overshoot into the band (a hard clamp parked half the
    // mouths at exactly the ceiling).
    let target_ang = if ang > hi {
        hi - (ang - hi).min(hi - lo) * 0.8
    } else {
        lo
    };
    // Sign check, worked concretely: down=(1,0), child heading straight
    // up (mn=(0,1)) ⇒ side=+1. The guide must land on pts[1]'s side
    // (y>0), which requires flow_dir = rotate(down, −side·target) so that
    // the upstream direction −flow_dir has +y. The first version used
    // +side and put the guide on the OPPOSITE side of the parent — the
    // channel left at the enforced angle, hooked back, and crossed the
    // parent just upstream of every corrected junction (the review's
    // recurring pattern, ~3 crossings/seed measured).
    let flow_dir = trunk::rotate(down, -side * target_ang);
    let guide_len = ml.min(40.0);
    let guide = Vec2::new(pts[0].x - flow_dir.x * guide_len, pts[0].y - flow_dir.y * guide_len);
    let mut out = Vec::with_capacity(pts.len() + 1);
    out.push(pts[0]);
    out.push(guide);
    out.extend_from_slice(&pts[1..]);
    // Exact geometric guard: if either modified segment PROPERLY crosses
    // the parent polyline, revert (cell-based checks with parent
    // exemptions kept leaking one variant or another of this — the
    // review chased three of them; a crossing test cannot be argued with).
    let crosses_parent = |a: Vec2, b: Vec2| -> bool {
        for w in parent_pts.windows(2) {
            let o = |p: Vec2, q: Vec2, r: Vec2| {
                (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
            };
            let ln = |p: Vec2, q: Vec2| {
                ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt().max(1e-9)
            };
            let (lab, lcd) = (ln(a, b), ln(w[0], w[1]));
            let (d1, d2) = (o(a, b, w[0]) / lab, o(a, b, w[1]) / lab);
            let (d3, d4) = (o(w[0], w[1], a) / lcd, o(w[0], w[1], b) / lcd);
            if d1.abs() > 0.5 && d2.abs() > 0.5 && d3.abs() > 0.5 && d4.abs() > 0.5
                && d1 * d2 < 0.0
                && d3 * d4 < 0.0
            {
                return true;
            }
        }
        false
    };
    for w in out.windows(2).take(2) {
        if crosses_parent(w[0], w[1]) {
            return pts.to_vec();
        }
    }
    out
}

/// Endpoint-preserving Chaikin corner cutting.
fn chaikin(pts: &[Vec2], iters: usize) -> Vec<Vec2> {
    let mut cur = pts.to_vec();
    for _ in 0..iters {
        if cur.len() < 3 {
            break;
        }
        let mut out = Vec::with_capacity(cur.len() * 2);
        out.push(cur[0]);
        for w in cur.windows(2) {
            let (a, b) = (w[0], w[1]);
            // 0.85/0.15 weights bound the corner displacement to well
            // under the 40 m channel-separation floor, so smoothing can
            // never re-cross what growth kept apart (a rare-crossing
            // report traced here).
            out.push(Vec2::new(0.85 * a.x + 0.15 * b.x, 0.85 * a.y + 0.15 * b.y));
            out.push(Vec2::new(0.15 * a.x + 0.85 * b.x, 0.15 * a.y + 0.85 * b.y));
        }
        out.push(*cur.last().unwrap());
        cur = out;
    }
    cur
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

fn block_around(blocked: &mut [bool], lin: usize, nx: usize, ny: usize, r_cells: f64) {
    let (cy, cx) = ((lin / nx) as i64, (lin % nx) as i64);
    let r = r_cells.ceil() as i64;
    for dy in -r..=r {
        for dx in -r..=r {
            let (y, x) = (cy + dy, cx + dx);
            if y < 0 || x < 0 || y >= ny as i64 || x >= nx as i64 {
                continue;
            }
            blocked[y as usize * nx + x as usize] = true;
        }
    }
}

fn grow_infill(
    steer: &Steer,
    spec8: &GridSpec,
    draws: &Draws,
    channels: &mut Vec<Channel>,
    chan_at: &mut Vec<i32>,
    arc_at: &mut Vec<f64>,
) {
    let cell = spec8.cell_size;
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    let center = |lin: usize| {
        Vec2::new(
            ((lin % nx) as f64 + 0.5) * cell,
            ((lin / nx) as f64 + 0.5) * cell,
        )
    };
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
        // Fingers get the same meander wander as authored channels — they
        // were the straightest population (order-1 beelines down a smooth
        // gradient) in the planform measurement.
        let wander = trunk::wander_from(p, 0x1F1F ^ best_lin as u64);
        // Consecutive steps spent gliding in the 40–200 m corridor band of
        // an existing channel. Real approaches converge; a finger that has
        // ridden the band for ~300 m must COMMIT to the junction — the
        // escalating pull below is what finally killed the measured
        // 40%+ parallel-run fraction (real corpus: 3–11%).
        let mut in_band_steps = 0usize;
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
            // Tangential approach (review findings 5+6): aiming AT the
            // nearest channel arrives perpendicular — a T-junction with a
            // last-moment swerve. Within 220 m the finger blends in the
            // target channel's DOWNSTREAM tangent so it curves in and
            // joins at an acute angle like a real tributary.
            let mut tang = Vec2::new(0.0, 0.0);
            if dist[pl] < 220.0 {
                let parent = &channels[chan_at[nl] as usize];
                let (_, up_tan) = trunk::point_at_arc(&parent.pts, arc_at[nl]);
                let down_tan = Vec2::new(-up_tan.x, -up_tan.y);
                let wgt = 1.4 * (1.0 - dist[pl] / 220.0);
                tang = Vec2::new(down_tan.x * wgt, down_tan.y * wgt);
            }
            // The planform instrument caught the compass problem: with a
            // constant 1.3 weight, `to_net` (straight line to the nearest
            // channel) dominates the blend over kilometres, and neighbouring
            // fingers placed by the regular farthest-point argmax all
            // beeline down the same slope — the measured 43–46% parallel
            // fraction (vs 3–11% real) lived almost entirely in this
            // population. Real headwater streams follow the TERRAIN; the
            // network pull is only the endgame that closes the junction. So
            // the compass fades in below 600 m of the network and the
            // downhill term carries the far field.
            if dist[pl] > 40.0 && dist[pl] < 200.0 {
                in_band_steps += 1;
            } else {
                in_band_steps = 0;
            }
            let commit = in_band_steps.saturating_sub(3) as f64 * 0.6;
            let prox = 1.0 - (dist[pl] / 600.0).min(1.0);
            let w_net = 0.5 + 0.8 * prox + commit;
            let w_down = 1.0 - 0.4 * prox;
            let mut dir = Vec2::new(
                w_net * to_net.x + w_down * down.x + 0.8 * prev.x + tang.x,
                w_net * to_net.y + w_down * down.y + 0.8 * prev.y + tang.y,
            );
            let dn = (dir.x * dir.x + dir.y * dir.y).sqrt().max(1e-9);
            dir = Vec2::new(dir.x / dn, dir.y / dn);
            let ang = (u - 0.5) * 0.8 + if si == 0 { (ang_u - 0.5) * 0.6 } else { 0.0 };
            // Wander as an ABSOLUTE heading offset (a per-step turn is
            // diluted ~70% by the momentum term because the blend rebuilds
            // the heading each step). Fades out inside the tangential-
            // approach zone so the junction still closes at an acute angle.
            let wfade = (dist[pl] / 220.0).min(1.0);
            let th_head = trunk::wander_heading(&wander, si as f64 * INFILL_STEP_M);
            dir = trunk::rotate(dir, ang + th_head * wfade);
            // Fingers obey discontinuities, with one difference from the
            // authored tributaries: at a SCARP a finger deflects parallel
            // instead of truncating — drainage in front of a scarp runs
            // along its base to the water gap (subsequent streams), which
            // is how the upstream province keeps its network without any
            // channel crossing the scarp face.
            for disc in &steer.meta.discontinuities {
                let (_, dd) = trunk::nearest_side(&disc.curve, p);
                if dd > trunk::DISC_BAND_M {
                    continue;
                }
                match disc.kind {
                    course_contracts::biome::BoundaryKind::Scarp
                    | course_contracts::biome::BoundaryKind::ValleyWall => {
                        let tg = trunk::nearest_tangent(&disc.curve, p);
                        let along = dir.x * tg.x + dir.y * tg.y;
                        let sign = if along >= 0.0 { 1.0 } else { -1.0 };
                        dir = Vec2::new(sign * tg.x, sign * tg.y);
                    }
                    course_contracts::biome::BoundaryKind::MaterialContact => {
                        let g = steer.grad(steer.hardness, p);
                        let gl = (g.x * g.x + g.y * g.y).sqrt().max(1e-9);
                        dir = trunk::norm(Vec2::new(dir.x - 0.5 * g.x / gl, dir.y - 0.5 * g.y / gl));
                    }
                }
            }
            let mut q = Vec2::new(p.x + dir.x * INFILL_STEP_M, p.y + dir.y * INFILL_STEP_M);
            if q.x < margin || q.y < margin || q.x > extent - margin || q.y > extent - margin {
                // Slide along the margin instead of dying: near the box
                // edge the downhill term can point outward (the implied
                // surface dips toward base level) and killed fingers here
                // burned the whole draw budget on doomed corner retries —
                // the D5 battery caught the resulting sparse networks.
                if q.x < margin || q.x > extent - margin {
                    dir.x = 0.0;
                }
                if q.y < margin || q.y > extent - margin {
                    dir.y = 0.0;
                }
                let l = (dir.x * dir.x + dir.y * dir.y).sqrt();
                if l < 1e-9 {
                    break;
                }
                dir = Vec2::new(dir.x / l, dir.y / l);
                q = Vec2::new(p.x + dir.x * INFILL_STEP_M, p.y + dir.y * INFILL_STEP_M);
                if q.x < margin || q.y < margin || q.x > extent - margin || q.y > extent - margin {
                    break;
                }
            }
            // A step can jump clean over an 8 m channel line unless the
            // segment is sampled at raster pitch: every 4 m, attach at the
            // first channel cell touched, so fingers never cross channels.
            let mut hit = None;
            let n_sub = (INFILL_STEP_M / 4.0).ceil() as usize;
            for t in 1..=n_sub {
                let f = t as f64 / n_sub as f64;
                let m = Vec2::new(p.x + dir.x * INFILL_STEP_M * f, p.y + dir.y * INFILL_STEP_M * f);
                let ml = ((m.y / cell).floor() as usize).min(ny - 1) * nx
                    + ((m.x / cell).floor() as usize).min(nx - 1);
                if chan_at[ml] >= 0 {
                    hit = Some(ml);
                    break;
                }
            }
            if let Some(ml) = hit {
                attach = Some((chan_at[ml], arc_at[ml], center(ml)));
                break;
            }
            pts.push(q);
            prev = dir;
            p = q;
        }
        let Some((chan, arc, anchor)) = attach else {
            if std::env::var("INFILL_DBG").is_ok() {
                let last = pts.last().unwrap();
                eprintln!("FAIL from ({:.0},{:.0}) gap {:.0}: {} pts, ended ({:.0},{:.0})",
                    center(best_lin).x, center(best_lin).y, best, pts.len(), last.x, last.y);
            }
            // Blacklist a NEIGHBOURHOOD: a single-cell block just shuffled
            // the next placement 8 m over into the same doomed spot.
            block_around(&mut blocked, best_lin, nx, ny, 150.0 / cell);
            continue;
        };
        pts.push(anchor);
        pts.reverse(); // pts[0] = downstream junction end
        // Source-area offset: the placement point is the farthest cell from
        // the network — typically a crest. A channel head AT the summit
        // inverts the local anatomy (the review measured it), so the top
        // ~160 m of the path is hillslope, not channel.
        let mut drop_from = pts.len();
        let mut tail = 0.0;
        while drop_from > 2 {
            let d = Vec2::new(
                pts[drop_from - 1].x - pts[drop_from - 2].x,
                pts[drop_from - 1].y - pts[drop_from - 2].y,
            );
            tail += (d.x * d.x + d.y * d.y).sqrt();
            if tail > 160.0 {
                break;
            }
            drop_from -= 1;
        }
        pts.truncate(drop_from);
        if trunk::arc_len(&pts) < 150.0 {
            blocked[best_lin] = true;
            continue;
        }
        let pts = validated_child(&pts, &channels[chan as usize].pts, arc, chan_at, chan, spec8);
        let idx = channels.len();
        channels.push(Channel {
            pts: pts.clone(),
            order: 1,
            parent: Some(chan as u32),
            junction_arc_m: arc,
            area_m2: 0.0,
        });
        stamp_channel(idx, &pts, spec8, chan_at, arc_at);
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
