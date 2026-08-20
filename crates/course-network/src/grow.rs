//! Headward growth: the network is GROWN from the trunk mouths outward.
//!
//! Real channel networks extend by headward erosion — a head cuts back into
//! the divide until it runs out of catchment. So growth here runs
//! downstream-to-upstream, and `parent` points DOWNSTREAM, the receiver
//! convention the flow field uses.
//!
//! Two properties come from the process rather than from a rule, which is the
//! point of growing rather than extracting:
//!
//! - **No crossings and no loops.** A head dies when it comes within
//!   `claim_m` of any existing node, so two branches can never intersect and
//!   the result is a forest by construction.
//! - **Spacing.** A head may only advance into territory no other channel has
//!   claimed, so drainage density is set by `claim_m` rather than emerging
//!   from an erosion threshold and having to be tuned back.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::world::EXTENT_M;

#[derive(Clone, Debug)]
pub struct Node {
    pub p: Vec2,
    /// Index of the node one step DOWNSTREAM. `None` at a mouth.
    pub parent: Option<u32>,
    /// Strahler order; 0 until assigned.
    pub order: u32,
    /// Drained area, m²; 0 until accumulated.
    pub area_m2: f64,
}

pub struct GrowParams {
    /// Distance a head advances per step, metres.
    pub step_m: f64,
    /// A head dies if it comes within this of a node that is not its own
    /// recent history. This is the drainage-density dial.
    pub claim_m: f64,
    /// Branches per `claim_m` of channel. **Not a free parameter**: two
    /// children need about `claim_m` of separation before the spacing test
    /// stops exempting them, so branching faster than once per `claim_m`
    /// makes cousins collide and the tree collapses. Measured at
    /// branch_p = 0.16 with claim_m = 420 (a branch every 156 m against 420 m
    /// needed): 20-24 nodes and density 0.05. This is also what real networks
    /// do — tributaries join at intervals comparable to the drainage spacing,
    /// not several times inside it.
    pub branch_rate: f64,
    /// Angle a side tributary leaves the stem at, radians.
    ///
    /// Branching is ASYMMETRIC: the stem continues and a tributary sprouts
    /// beside it. Symmetric bifurcation was the first formulation and it is
    /// structurally incapable of matching a real network — **a strictly binary
    /// tree has a bifurcation ratio of exactly 2**, against a corpus band of
    /// 3.92-4.74, and no dial fixes that (a sweep over branch angle and rate
    /// moved Rb only between 1.5 and 2.4). It also splayed every confluence,
    /// because a symmetric split has no continuing stem for a tributary to
    /// join at an angle to. Real networks are stems with many short
    /// tributaries entering along their length, which is what produces both
    /// Rb > 2 and an acute junction angle.
    pub branch_angle_rad: f64,
    /// How strongly a head steers uphill on the relief predisposition.
    pub w_uphill: f64,
    /// How strongly a head steers AWAY from the nearest existing channel.
    ///
    /// This is the term that makes the tree space-fill, and it is the
    /// mechanism of headward erosion: a head cuts back into the interfluve —
    /// the ground no channel has claimed. Steering on uphill alone sends
    /// every head toward the same highs, where they converge and kill each
    /// other; measured, that produced 33-61 nodes and density 0.06-0.12
    /// against a 2.2-2.6 target.
    pub w_repel: f64,
    /// How strongly a head aligns to the structural grain (axial).
    pub w_grain: f64,
    /// How strongly a head keeps its heading.
    pub w_persist: f64,
    /// Random wander per step, radians.
    pub wander_rad: f64,
    /// Hard cap on nodes, so a bad dial cannot hang the stage.
    pub max_nodes: usize,
}

impl Default for GrowParams {
    fn default() -> Self {
        GrowParams {
            step_m: 26.0,
            claim_m: 150.0,
            branch_rate: 4.0,
            branch_angle_rad: 40.0_f64.to_radians(),
            w_uphill: 0.45,
            w_repel: 1.6,
            w_grain: 0.25,
            w_persist: 1.4,
            wander_rad: 0.30,
            max_nodes: 20_000,
        }
    }
}

/// A uniform-bucket index over node positions, so the spacing test is O(1)
/// per step instead of a scan over every node placed so far.
struct Buckets {
    cell: f64,
    n: usize,
    b: Vec<Vec<u32>>,
}

impl Buckets {
    fn new(cell: f64) -> Self {
        let n = (EXTENT_M / cell).ceil() as usize + 2;
        Buckets { cell, n, b: vec![Vec::new(); n * n] }
    }
    fn key(&self, p: Vec2) -> (usize, usize) {
        (
            ((p.x / self.cell).floor().max(0.0) as usize).min(self.n - 1),
            ((p.y / self.cell).floor().max(0.0) as usize).min(self.n - 1),
        )
    }
    fn insert(&mut self, p: Vec2, id: u32) {
        let (x, y) = self.key(p);
        self.b[y * self.n + x].push(id);
    }
    /// Summed repulsion from nearby claimed ground, each contribution weighted
    /// by how far away it is THROUGH THE TREE.
    ///
    /// The weight ramps from 0 at a junction to 1 at a tree distance of
    /// `2*claim_m` — the same ramp the spacing test uses, so the two rules
    /// agree about what counts as a neighbour. Both extremes were measured
    /// and both fail: relatedness-blind repulsion splays every confluence
    /// (junction angles 65-90 deg against a 37-45 band, and the branch angle
    /// barely moved them), while skipping relatives outright removes the force
    /// that separates a new pair at all and growth collapses to a tenth of the
    /// required density. The ramp is what lets a pair leave a junction at the
    /// branch angle and only then push apart.
    #[allow(clippy::too_many_arguments)]
    fn repel(
        &self, p: Vec2, arc: f64, nodes: &[Node], lin: &[u32], arc_of: &[f64],
        lin_parent: &std::collections::HashMap<u32, u32>,
        lin_birth: &std::collections::HashMap<u32, f64>,
        trail: &[u32], anc: &[u32], claim_m: f64,
    ) -> Vec2 {
        let (kx, ky) = self.key(p);
        let reach = claim_m * 2.0;
        let mut acc = Vec2::new(0.0, 0.0);
        for oy in -1i64..=1 {
            for ox in -1i64..=1 {
                let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 { continue; }
                for &id in &self.b[y as usize * self.n + x as usize] {
                    if trail.contains(&id) { continue; }
                    let q = nodes[id as usize].p;
                    let d = p.distance(q);
                    if d >= reach || d < 1e-6 { continue; }
                    let l = lin[id as usize];
                    let ramp = if anc.contains(&l) {
                        1.0 // own channel upstream: a real neighbour to avoid
                    } else {
                        let mut cur = l;
                        let mut split = 0.0;
                        let mut hops = 0;
                        loop {
                            if anc.contains(&cur) { break; }
                            let Some(&par) = lin_parent.get(&cur) else { break };
                            split = *lin_birth.get(&cur).unwrap_or(&0.0);
                            cur = par;
                            hops += 1;
                            if hops > 64 { break; }
                        }
                        let td = (arc - split).max(0.0) + (arc_of[id as usize] - split).max(0.0);
                        (td / (2.0 * claim_m)).clamp(0.0, 1.0)
                    };
                    if ramp <= 0.0 { continue; }
                    let w = ramp * (1.0 - d / reach);
                    let away = Vec2::new(p.x - q.x, p.y - q.y).normalized();
                    acc = Vec2::new(acc.x + away.x * w, acc.y + away.y * w);
                }
            }
        }
        if acc.length() > 1.0 { acc.normalized() } else { acc }
    }

    #[allow(dead_code)]
    fn near_pt(&self, p: Vec2, nodes: &[Node], exempt: &[u32]) -> (f64, Vec2) {
        let (kx, ky) = self.key(p);
        let mut best = (f64::MAX, p);
        for oy in -2i64..=2 {
            for ox in -2i64..=2 {
                let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 {
                    continue;
                }
                for &id in &self.b[y as usize * self.n + x as usize] {
                    if exempt.contains(&id) {
                        continue;
                    }
                    let q = nodes[id as usize].p;
                    let d = p.distance(q);
                    if d < best.0 {
                        best = (d, q);
                    }
                }
            }
        }
        best
    }

    /// Is this position too close to a channel it is not related to?
    ///
    /// **Required spacing ramps with distance through the TREE**, and this is
    /// the rule that makes a dendritic network possible at all. A uniform
    /// `claim_m` between every pair of channels forbids junctions, because a
    /// junction is precisely where two channels are close together. Measured
    /// under the uniform rule: two children diverging at +/-36 deg separate at
    /// 1.18x their arc length while branches arrive every `claim_m` of arc, so
    /// the margin was 15% AT EVERY SCALE and the tree collapsed to tens of
    /// nodes for any `claim_m` above ~60 m.
    ///
    /// Here two channels that split 200 m upstream may sit 100 m apart, while
    /// two whose common ancestor is a kilometre away must be a full `claim_m`
    /// apart. Required spacing is `claim_m * min(1, tree_dist / (2*claim_m))`.
    #[allow(clippy::too_many_arguments)]
    fn blocked(
        &self, p: Vec2, arc: f64, nodes: &[Node], lin: &[u32], arc_of: &[f64],
        lin_parent: &std::collections::HashMap<u32, u32>,
        lin_birth: &std::collections::HashMap<u32, f64>,
        trail: &[u32], anc: &[u32], claim_m: f64,
    ) -> bool {
        let (kx, ky) = self.key(p);
        for oy in -1i64..=1 {
            for ox in -1i64..=1 {
                let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 { continue; }
                for &id in &self.b[y as usize * self.n + x as usize] {
                    let l = lin[id as usize];
                    // same channel, or the step just laid: never blocking
                    if anc.contains(&l) || trail.contains(&id) { continue; }
                    let d = p.distance(nodes[id as usize].p);
                    if d >= claim_m { continue; }
                    // Walk the node's lineage up to the common ancestor.
                    let mut cur = l;
                    let mut split = 0.0;
                    let mut hops = 0;
                    loop {
                        if anc.contains(&cur) { break; }
                        let Some(&par) = lin_parent.get(&cur) else { break };
                        split = *lin_birth.get(&cur).unwrap_or(&0.0);
                        cur = par;
                        hops += 1;
                        if hops > 64 { break; }
                    }
                    let tree_dist = (arc - split).max(0.0) + (arc_of[id as usize] - split).max(0.0);
                    let need = claim_m * (tree_dist / (2.0 * claim_m)).clamp(0.0, 1.0);
                    if d < need { return true; }
                }
            }
        }
        false
    }

    /// Nearest distance to any node except those in `exempt`.
    #[allow(dead_code)]
    fn near(&self, p: Vec2, nodes: &[Node], exempt: &[u32]) -> f64 {
        let (kx, ky) = self.key(p);
        let mut best = f64::MAX;
        for oy in -1i64..=1 {
            for ox in -1i64..=1 {
                let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 {
                    continue;
                }
                for &id in &self.b[y as usize * self.n + x as usize] {
                    if exempt.contains(&id) {
                        continue;
                    }
                    let d = p.distance(nodes[id as usize].p);
                    if d < best {
                        best = d;
                    }
                }
            }
        }
        best
    }
}

struct Head {
    node: u32,
    dir: Vec2,
    /// The last few nodes on this head's own path, exempt from the spacing
    /// test — otherwise a head immediately collides with the step it just laid.
    trail: Vec<u32>,
    /// Arc distance from this head's mouth, metres.
    arc: f64,
    /// Every lineage on this head's path back to the mouth, including its own.
    ///
    /// A channel cannot collide with itself, so the whole ancestry is exempt
    /// from the spacing test. Exempting only the CURRENT lineage was a bug:
    /// the lineage resets at every branch, so a head hit its own upstream path
    /// as soon as it curved past the trail window — and with a 468 m trail
    /// against a 420 m claim, any curvature at all was fatal. Measured: heads
    /// lived 26 steps and the tree reached 30 nodes.
    anc: Vec<u32>,
    /// This head's lineage.
    lin: u32,
}

/// Uphill unit vector on a scalar field, by central difference.
fn uphill(f: &Grid<f64>, p: Vec2, h: f64) -> Vec2 {
    let gx = f.bilinear(Vec2::new(p.x + h, p.y)) - f.bilinear(Vec2::new(p.x - h, p.y));
    let gy = f.bilinear(Vec2::new(p.x, p.y + h)) - f.bilinear(Vec2::new(p.x, p.y - h));
    let v = Vec2::new(gx, gy);
    if v.length() < 1e-12 { Vec2::new(0.0, 0.0) } else { v.normalized() }
}

/// Why heads stopped — the instrument behind every growth fix so far.
#[derive(Default, Debug, Clone, Copy)]
pub struct GrowStats {
    pub died_offtile: u32,
    pub died_claimed: u32,
    pub branches: u32,
    pub steps: u32,
    pub rounds: u32,
}

/// Grow from the given seeds. Each seed is (position, initial heading).
pub fn grow(
    rng: &mut DetRng,
    seeds: &[(Vec2, f64)],
    relief: &Grid<f64>,
    grain: &Grid<f64>,
    p: &GrowParams,
) -> (Vec<Node>, GrowStats) {
    let mut nodes: Vec<Node> = Vec::new();
    let mut lineage: Vec<u32> = Vec::new();
    let mut arc_of: Vec<f64> = Vec::new();
    let mut lin_parent: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    let mut lin_birth: std::collections::HashMap<u32, f64> = std::collections::HashMap::new();
    let mut next_lin: u32 = 1 << 20;
    let mut buckets = Buckets::new(p.claim_m.max(16.0));
    let mut heads: Vec<Head> = Vec::new();
    let mut st = GrowStats::default();

    for &(pos, az) in seeds {
        let id = nodes.len() as u32;
        nodes.push(Node { p: pos, parent: None, order: 0, area_m2: 0.0 });
        lineage.push(id);
        arc_of.push(0.0);
        buckets.insert(pos, id);
        heads.push(Head {
            node: id,
            dir: Vec2::new(math::cos(az), math::sin(az)),
            trail: vec![id],
            arc: 0.0,
            anc: vec![id],
            lin: id,
        });
    }

    // Round-robin over heads so no single head races to the far side of the
    // tile and claims territory a neighbouring system should have had.
    while !heads.is_empty() && nodes.len() < p.max_nodes {
        st.rounds += 1;
        let mut next: Vec<Head> = Vec::new();
        for mut h in heads.drain(..) {
            if nodes.len() >= p.max_nodes {
                break;
            }
            let from = nodes[h.node as usize].p;

            let up = uphill(relief, from, 24.0);
            let th = grain.bilinear(from);
            let mut gv = Vec2::new(math::cos(th), math::sin(th));
            // Grain is AXIAL: pick whichever sense agrees with the heading.
            if gv.dot(h.dir) < 0.0 {
                gv = Vec2::new(-gv.x, -gv.y);
            }
            // Repulsion from the nearest claimed ground -- grow into the
            // interfluve, which is what a head actually does.
            let rep = buckets.repel(
                from, h.arc, &nodes, &lineage, &arc_of, &lin_parent, &lin_birth,
                &h.trail, &h.anc, p.claim_m,
            );
            let w = rng.range_f64(-p.wander_rad, p.wander_rad);
            let wob = Vec2::new(math::cos(w), math::sin(w));
            let steer = Vec2::new(
                up.x * p.w_uphill + gv.x * p.w_grain + h.dir.x * p.w_persist + rep.x * p.w_repel,
                up.y * p.w_uphill + gv.y * p.w_grain + h.dir.y * p.w_persist + rep.y * p.w_repel,
            );
            let mut dir = if steer.length() < 1e-9 { h.dir } else { steer.normalized() };
            // apply the wander as a rotation
            dir = Vec2::new(
                dir.x * wob.x - dir.y * wob.y,
                dir.x * wob.y + dir.y * wob.x,
            )
            .normalized();

            let to = Vec2::new(from.x + dir.x * p.step_m, from.y + dir.y * p.step_m);
            if to.x < 0.0 || to.y < 0.0 || to.x > EXTENT_M || to.y > EXTENT_M {
                st.died_offtile += 1;
                continue; // ran off the tile: this head is done
            }
            if buckets.blocked(
                to, h.arc + p.step_m, &nodes, &lineage, &arc_of, &lin_parent, &lin_birth,
                &h.trail, &h.anc, p.claim_m,
            ) {
                st.died_claimed += 1;
                continue; // territory already claimed: head dies
            }
            st.steps += 1;

            let id = nodes.len() as u32;
            nodes.push(Node { p: to, parent: Some(h.node), order: 0, area_m2: 0.0 });
            lineage.push(h.lin);
            arc_of.push(h.arc + p.step_m);
            buckets.insert(to, id);

            h.trail.push(id);
            let keep = (p.claim_m / p.step_m).ceil() as usize + 2;
            if h.trail.len() > keep {
                h.trail.remove(0);
            }
            h.node = id;
            h.dir = dir;
            h.arc += p.step_m;

            let branch_p = (p.step_m / p.claim_m.max(1e-6)) * p.branch_rate;
            if rng.next_f64() < branch_p {
                st.branches += 1;
                // The stem CONTINUES, keeping its lineage; a tributary sprouts
                // beside it with a new one.
                let trib = next_lin;
                next_lin += 1;
                lin_parent.insert(trib, h.lin);
                lin_birth.insert(trib, h.arc);
                let side = if rng.next_f64() < 0.5 { -1.0 } else { 1.0 };
                let (c, sn) = (
                    math::cos(p.branch_angle_rad * side),
                    math::sin(p.branch_angle_rad * side),
                );
                let mut anc = h.anc.clone();
                anc.push(trib);
                next.push(Head {
                    node: id,
                    dir: Vec2::new(dir.x * c - dir.y * sn, dir.x * sn + dir.y * c),
                    trail: h.trail.clone(),
                    arc: h.arc,
                    anc,
                    lin: trib,
                });
            }
            next.push(h);
        }
        heads = next;
    }
    (nodes, st)
}
