//! Drained area, Strahler order, and reaches.
//!
//! The corpus bands (`docs/network-first/01-measurement-policy.md`) are all
//! measured on channels extracted at an ACCUMULATION THRESHOLD, so the grown
//! tree has to carry a real drained area before it can be compared to them.
//! Area comes from territory: every cell of the tile is assigned to its
//! nearest node, and those areas are summed downstream.

use course_world::math::Vec2;

/// One node of the network graph. `parent` points DOWNSTREAM — the receiver
/// convention. (Lived in `grow.rs` until the M2 maze was deleted; the graph
/// currency outlived the algorithm that first produced it.)
#[derive(Clone, Debug)]
pub struct Node {
    pub p: Vec2,
    /// Index of the node one step downstream. `None` at a mouth or a pit.
    pub parent: Option<u32>,
    /// Strahler order; 0 until assigned.
    pub order: u32,
    /// Drained area, m²; 0 until accumulated.
    pub area_m2: f64,
}
use course_world::world::EXTENT_M;

/// Cell size the territory assignment runs at. 8 m matches the declared
/// planform policy, so `d2c` and density mean the same thing here as on the
/// corpus.
pub const ACCOUNT_RES_M: f64 = 8.0;

/// Assign every cell to its nearest node and sum areas downstream.
pub fn accumulate(nodes: &mut [Node]) {
    if nodes.is_empty() {
        return;
    }
    let n = (EXTENT_M / ACCOUNT_RES_M).round() as usize + 1;
    let cell_area = ACCOUNT_RES_M * ACCOUNT_RES_M;

    // Bucket the nodes so the nearest-node query is local.
    let bs = 120.0_f64.max(ACCOUNT_RES_M);
    let bn = (EXTENT_M / bs).ceil() as usize + 2;
    let mut buckets: Vec<Vec<u32>> = vec![Vec::new(); bn * bn];
    let key = |p: Vec2| {
        (
            ((p.x / bs).floor().max(0.0) as usize).min(bn - 1),
            ((p.y / bs).floor().max(0.0) as usize).min(bn - 1),
        )
    };
    for (i, nd) in nodes.iter().enumerate() {
        let (x, y) = key(nd.p);
        buckets[y * bn + x].push(i as u32);
    }

    let mut own = vec![0.0f64; nodes.len()];
    for gy in 0..n {
        for gx in 0..n {
            let p = Vec2::new(gx as f64 * ACCOUNT_RES_M, gy as f64 * ACCOUNT_RES_M);
            let (kx, ky) = key(p);
            let mut best = (f64::MAX, u32::MAX);
            // Widen the search until something is found; a sparse network can
            // leave a cell with no node in its own or adjacent buckets.
            // Expand until the best distance is provably inside the searched
            // ring; stopping at the FIRST hit can miss a nearer node one ring out.
            let mut r = 1i64;
            while r <= bn as i64 {
                for oy in -r..=r {
                    for ox in -r..=r {
                        if ox.abs() != r && oy.abs() != r && r > 1 {
                            continue; // only the new ring
                        }
                        let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                        if x < 0 || y < 0 || x >= bn as i64 || y >= bn as i64 {
                            continue;
                        }
                        for &id in &buckets[y as usize * bn + x as usize] {
                            let d = p.distance(nodes[id as usize].p);
                            if d < best.0 {
                                best = (d, id);
                            }
                        }
                    }
                }
                if best.1 != u32::MAX && best.0 <= (r as f64) * bs {
                    break;
                }
                r += 1;
            }
            if best.1 != u32::MAX {
                own[best.1 as usize] += cell_area;
            }
        }
    }

    // Sum downstream. Children are always higher-indexed than their parent
    // (a node is pushed after the node it flows to), so one reverse pass does it.
    for (i, o) in own.iter().enumerate() {
        nodes[i].area_m2 = *o;
    }
    for i in (0..nodes.len()).rev() {
        if let Some(par) = nodes[i].parent {
            let a = nodes[i].area_m2;
            nodes[par as usize].area_m2 += a;
        }
    }
}

/// Strahler order over the sub-tree of nodes at or above `threshold_m2`.
/// Nodes below the threshold get order 0 — they are real ground features but
/// not channels, and the corpus battery would not see them either.
pub fn strahler(nodes: &mut [Node], threshold_m2: f64) {
    let n = nodes.len();
    let is_ch: Vec<bool> = nodes.iter().map(|x| x.area_m2 >= threshold_m2).collect();
    // children counts among channel nodes
    let mut kids: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 0..n {
        if !is_ch[i] {
            continue;
        }
        if let Some(p) = nodes[i].parent {
            if is_ch[p as usize] {
                kids[p as usize].push(i as u32);
            }
        }
    }
    // children are higher-indexed, so a forward pass over reversed indices works
    for i in (0..n).rev() {
        if !is_ch[i] {
            nodes[i].order = 0;
            continue;
        }
        let mut best = 0u32;
        let mut count = 0u32;
        for &k in &kids[i] {
            let o = nodes[k as usize].order;
            if o > best {
                best = o;
                count = 1;
            } else if o == best && o > 0 {
                count += 1;
            }
        }
        nodes[i].order = if best == 0 { 1 } else if count >= 2 { best + 1 } else { best };
    }
}

#[derive(Clone, Debug)]
pub struct Reach {
    pub pts: Vec<Vec2>,
    pub order: u32,
    /// Area at the downstream end.
    pub area_m2: f64,
}

/// Maximal constant-order runs between confluences — the same reach
/// definition the corpus side uses, so Horton ratios are comparable.
pub fn reaches(nodes: &[Node], threshold_m2: f64) -> Vec<Reach> {
    let n = nodes.len();
    let is_ch: Vec<bool> = nodes.iter().map(|x| x.area_m2 >= threshold_m2).collect();
    let mut donors = vec![0u32; n];
    for i in 0..n {
        if !is_ch[i] {
            continue;
        }
        if let Some(p) = nodes[i].parent {
            if is_ch[p as usize] {
                donors[p as usize] += 1;
            }
        }
    }
    let mut out = Vec::new();
    for start in 0..n {
        if !is_ch[start] || donors[start] == 1 {
            continue; // mid-reach cell, not a start
        }
        let mut pts = vec![nodes[start].p];
        let mut cur = start;
        loop {
            let Some(par) = nodes[cur].parent else { break };
            let par = par as usize;
            if !is_ch[par] {
                break;
            }
            pts.push(nodes[par].p);
            if donors[par] >= 2 {
                break; // the confluence starts its own reach
            }
            cur = par;
        }
        if pts.len() >= 2 {
            out.push(Reach { pts, order: nodes[start].order, area_m2: nodes[cur].area_m2 });
        }
    }
    out
}
