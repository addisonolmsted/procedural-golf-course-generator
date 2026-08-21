//! The proto-elevation field — the load-bearing object of the N-phase build.
//!
//! `E(p) = z(nearest channel point) + rise(distance) + w·relief_pred·budget`
//!
//! This is NOT the terrain; it is scaffolding that makes "tributaries converge
//! onto trunks" STRUCTURAL instead of steered: every channel already placed is
//! a valley floor of the field, so a walker descending `E` ends on a channel
//! because there is nowhere else downhill to go. The M2 maze happened because
//! growth had no such object — it organised by where there was room, not by
//! where water can go.
//!
//! The field updates INCREMENTALLY: each placed tributary joins the channel
//! set before the next one is routed, so later tributaries descend into
//! earlier ones — which is exactly the stem-with-side-branches structure that
//! gives real networks their bifurcation ratio.

use crate::account::Node;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::world::EXTENT_M;

/// All channel geometry placed so far, indexed for nearest-point queries.
pub struct ChannelSet {
    pub pts: Vec<Vec2>,
    pub z: Vec<f64>,
    /// Downstream unit direction at each point (toward the mouth).
    pub down: Vec<Vec2>,
    /// Which polyline each point belongs to.
    pub poly: Vec<u32>,
    /// Arc distance from the polyline's mouth, per point.
    pub arc: Vec<f64>,
    /// Total arc length per polyline id.
    pub total: Vec<f64>,
    cell: f64,
    n: usize,
    buckets: Vec<Vec<u32>>,
    next_poly: u32,
}

impl ChannelSet {
    pub fn new() -> Self {
        let cell = 150.0;
        let n = (EXTENT_M / cell).ceil() as usize + 2;
        ChannelSet {
            pts: Vec::new(),
            z: Vec::new(),
            down: Vec::new(),
            poly: Vec::new(),
            arc: Vec::new(),
            total: Vec::new(),
            cell,
            n,
            buckets: vec![Vec::new(); n * n],
            next_poly: 0,
        }
    }

    fn key(&self, p: Vec2) -> (usize, usize) {
        (
            ((p.x / self.cell).floor().max(0.0) as usize).min(self.n - 1),
            ((p.y / self.cell).floor().max(0.0) as usize).min(self.n - 1),
        )
    }

    /// Add a MOUTH-FIRST polyline with its per-point elevations. Returns its id.
    pub fn add_polyline(&mut self, pts: &[Vec2], z: &[f64]) -> u32 {
        assert_eq!(pts.len(), z.len());
        let id = self.next_poly;
        self.next_poly += 1;
        let mut acc = 0.0;
        for (i, &p) in pts.iter().enumerate() {
            if i > 0 {
                acc += p.distance(pts[i - 1]);
            }
            // downstream = toward the previous (lower-index) point
            let d = if i == 0 {
                if pts.len() > 1 {
                    Vec2::new(pts[0].x - pts[1].x, pts[0].y - pts[1].y).normalized()
                } else {
                    Vec2::new(1.0, 0.0)
                }
            } else {
                Vec2::new(pts[i - 1].x - p.x, pts[i - 1].y - p.y).normalized()
            };
            let idx = self.pts.len() as u32;
            self.pts.push(p);
            self.z.push(z[i]);
            self.down.push(d);
            self.poly.push(id);
            self.arc.push(acc);
            let (kx, ky) = self.key(p);
            self.buckets[ky * self.n + kx].push(idx);
        }
        self.total.push(acc.max(1.0));
        id
    }

    /// Nearest channel point: (index, distance). Ring-expands, exact.
    pub fn nearest(&self, p: Vec2) -> Option<(u32, f64)> {
        if self.pts.is_empty() {
            return None;
        }
        let (kx, ky) = self.key(p);
        let mut best = (u32::MAX, f64::MAX);
        let mut r = 1i64;
        loop {
            for oy in -r..=r {
                for ox in -r..=r {
                    if r > 1 && ox.abs() != r && oy.abs() != r {
                        continue;
                    }
                    let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                    if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 {
                        continue;
                    }
                    for &id in &self.buckets[y as usize * self.n + x as usize] {
                        let d = p.distance(self.pts[id as usize]);
                        if d < best.1 {
                            best = (id, d);
                        }
                    }
                }
            }
            if best.0 != u32::MAX && best.1 <= (r as f64 - 1.0).max(0.0) * self.cell {
                break;
            }
            if r > self.n as i64 {
                break;
            }
            r += 1;
        }
        (best.0 != u32::MAX).then_some(best)
    }

    /// The channel point `dist_m` DOWNSTREAM of `idx`, staying on its
    /// polyline (mouth-first storage: downstream = lower index).
    pub fn downstream_of(&self, idx: u32, dist_m: f64) -> u32 {
        let pid = self.poly[idx as usize];
        let mut i = idx as usize;
        let mut acc = 0.0;
        while i > 0 && self.poly[i - 1] == pid {
            acc += self.pts[i].distance(self.pts[i - 1]);
            i -= 1;
            if acc >= dist_m {
                break;
            }
        }
        i as u32
    }

    /// Nearest channel point excluding one polyline — the claim test for a
    /// climber, which must not be claimed by ITS OWN PARENT. The arc-based
    /// exemption ("ignore claims for the first 180 m walked") was identity-
    /// blind: an acute departure at 36-41 deg is still within 180 m of its
    /// parent until ~275 m of arc, so every acute climber died Claimed at
    /// exactly the first check while orthogonal ones cleared instantly —
    /// the entire measured tail bias (>80 share 30-37% vs 10-12% drawn) in
    /// one line. Post-grace monotone climbing cannot re-approach the parent
    /// (the field falls toward it), so a full exemption is safe.
    pub fn nearest_excl(&self, p: Vec2, skip_poly: u32) -> Option<(u32, f64)> {
        if self.pts.is_empty() {
            return None;
        }
        let (kx, ky) = self.key(p);
        let mut best = (u32::MAX, f64::MAX);
        let mut r = 1i64;
        loop {
            for oy in -r..=r {
                for ox in -r..=r {
                    if r > 1 && ox.abs() != r && oy.abs() != r {
                        continue;
                    }
                    let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                    if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 {
                        continue;
                    }
                    for &id in &self.buckets[y as usize * self.n + x as usize] {
                        if self.poly[id as usize] == skip_poly {
                            continue;
                        }
                        let d = p.distance(self.pts[id as usize]);
                        if d < best.1 {
                            best = (id, d);
                        }
                    }
                }
            }
            if best.0 != u32::MAX && best.1 <= (r as f64 - 1.0).max(0.0) * self.cell {
                break;
            }
            if r > self.n as i64 {
                break;
            }
            r += 1;
        }
        (best.0 != u32::MAX).then_some(best)
    }

    /// Emit the whole set as a node graph for the accounting machinery.
    /// Each polyline becomes a chain; `attach` gives, per polyline, the node
    /// index its mouth flows into (None for a trunk mouth).
    pub fn into_nodes(&self, attach: &[Option<u32>]) -> Vec<Node> {
        let mut nodes: Vec<Node> = Vec::with_capacity(self.pts.len());
        let mut start_of: Vec<u32> = Vec::new();
        let mut cur_poly = u32::MAX;
        for i in 0..self.pts.len() {
            let pid = self.poly[i];
            if pid != cur_poly {
                cur_poly = pid;
                start_of.push(nodes.len() as u32);
            }
            let parent = if self.poly[i] == cur_poly && i > 0 && self.poly[i - 1] == pid {
                Some(nodes.len() as u32 - 1)
            } else {
                attach.get(pid as usize).copied().flatten()
            };
            nodes.push(Node { p: self.pts[i], parent, order: 0, area_m2: 0.0 });
        }
        nodes
    }
}

impl Default for ChannelSet {
    fn default() -> Self {
        Self::new()
    }
}

/// The field itself.
pub struct Proto<'a> {
    pub chans: &'a ChannelSet,
    pub relief: &'a Grid<f64>,
    /// Rise coefficient: `rise(d) = k · d^0.6`.
    pub k_rise: f64,
    /// Macro weight on `relief_pred` (dimensionless × budget).
    pub w_macro: f64,
    pub budget_m: f64,
}

impl<'a> Proto<'a> {
    /// Ground climbs away from every channel and the valley floor slopes
    /// down-valley; the macro field tilts the interfluves so sources sit
    /// where the template said high ground is.
    pub fn e(&self, p: Vec2) -> f64 {
        let (zc, d) = match self.chans.nearest(p) {
            Some((i, d)) => (self.chans.z[i as usize], d),
            None => (0.0, 0.0),
        };
        zc + self.k_rise * math::pow(d.max(0.0), 0.6) + self.w_macro * self.relief.bilinear(p) * self.budget_m
    }

    /// Central-difference gradient.
    pub fn grad(&self, p: Vec2) -> Vec2 {
        let h = 35.0;
        let gx = self.e(Vec2::new(p.x + h, p.y)) - self.e(Vec2::new(p.x - h, p.y));
        let gy = self.e(Vec2::new(p.x, p.y + h)) - self.e(Vec2::new(p.x, p.y - h));
        Vec2::new(gx / (2.0 * h), gy / (2.0 * h))
    }
}
