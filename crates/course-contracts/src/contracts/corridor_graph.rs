//! Contract C3 — routing → realization: the corridor graph and its budgets.
//!
//! Where the course stops being a search result and becomes a construction
//! plan; also the pipeline's streaming seam — every `Hole` is independently
//! realizable. No search residue crosses this boundary. Full semantics:
//! `docs/contracts/C3-corridor-realization.md`.

use crate::error::{hex32, ContractError};
use crate::plasticity::Plasticity;
use crate::{HOLE_COUNT, PAR_SEQUENCE};
use course_world::math::Vec2;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const C3_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Tee,
    Green,
    Junction,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
    pub id: NodeId,
    pub kind: NodeKind,
    pub at: Vec2,
    pub elev_m: f64,
}

/// A located fill source, assigned to exactly one hole (shared borrow would
/// couple two streaming units).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BorrowSite {
    pub at: Vec2,
    pub radius_m: f64,
    pub volume_m3: f64,
    /// True ⇒ the pit is intended to be a pond; S10 dresses it as one.
    pub becomes_water: bool,
}

/// One hole, self-contained: given C2 and this, S7/S8 can build it with no
/// other hole's output. That is the streaming guarantee.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Hole {
    /// 0..9, play order.
    pub index: u32,
    pub par: u32,
    pub tee: NodeId,
    pub green: NodeId,
    /// Corridor centreline, tee → green, world metres. A polyline — S8 fits
    /// the spline.
    pub spine: Vec<Vec2>,
    /// Per spine vertex; with `spine` defines the corridor envelope.
    pub halfwidth_m: Vec<f64>,
    /// This hole's share. A hole that overruns does not borrow from another
    /// hole — it grades less.
    pub earthwork_budget_m3: f64,
    pub borrow_sites: Vec<BorrowSite>,
    /// Rise/run, copied per hole so a `Hole` is self-contained.
    pub target_grade: f64,
    /// Corridors this one abuts — S9's seam list.
    pub neighbors: Vec<u32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Budgets {
    pub total_cut_m3: f64,
    pub total_fill_m3: f64,
    pub total_clearing_m3: f64,
    /// Streaming budget per hole (from `docs/02-performance-budget.md`).
    pub per_hole_ms: f64,
    /// S6 owns the radius; C3 records the result. `holes[0].tee` and
    /// `holes[8].green` must be within it of each other.
    pub clubhouse_radius_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CorridorGraph {
    pub c3_version: u32,
    pub seed: u64,
    #[serde(with = "hex32")]
    pub spec_digest: [u8; 32],
    /// Play order; `holes[0]` realizes first.
    pub holes: Vec<Hole>,
    pub nodes: Vec<Node>,
    pub budgets: Budgets,
    pub plasticity: Plasticity,
}

impl CorridorGraph {
    pub fn new(
        seed: u64,
        spec_digest: [u8; 32],
        holes: Vec<Hole>,
        nodes: Vec<Node>,
        budgets: Budgets,
        plasticity: Plasticity,
    ) -> Result<Self, ContractError> {
        let g = CorridorGraph {
            c3_version: C3_VERSION,
            seed,
            spec_digest,
            holes,
            nodes,
            budgets,
            plasticity,
        };
        g.validate()?;
        Ok(g)
    }

    fn node(&self, id: NodeId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    fn validate(&self) -> Result<(), ContractError> {
        if self.c3_version != C3_VERSION {
            return Err(ContractError::Version {
                what: "c3_version",
                found: self.c3_version,
                expected: C3_VERSION,
            });
        }
        // Exactly 9 holes, par per PAR_SEQUENCE (the authority), play order.
        if self.holes.len() != HOLE_COUNT as usize {
            return Err(ContractError::invariant(
                "holes",
                format!("must be exactly {HOLE_COUNT}, got {}", self.holes.len()),
            ));
        }
        for (i, h) in self.holes.iter().enumerate() {
            if h.index != i as u32 {
                return Err(ContractError::invariant(
                    "holes",
                    format!("hole {i} carries index {}", h.index),
                ));
            }
            if h.par != PAR_SEQUENCE[i] {
                return Err(ContractError::invariant(
                    "holes",
                    format!("hole {i}: par {} != PAR_SEQUENCE {}", h.par, PAR_SEQUENCE[i]),
                ));
            }
            if h.spine.len() < 2 {
                return Err(ContractError::invariant(
                    "spine",
                    format!("hole {i}: needs >= 2 vertices"),
                ));
            }
            if h.halfwidth_m.len() != h.spine.len() {
                return Err(ContractError::invariant(
                    "halfwidth_m",
                    format!("hole {i}: length must equal spine length"),
                ));
            }
            if h.halfwidth_m.iter().any(|w| !w.is_finite() || *w <= 0.0) {
                return Err(ContractError::invariant(
                    "halfwidth_m",
                    format!("hole {i}: all halfwidths must be finite and > 0"),
                ));
            }
            for v in &h.spine {
                if !course_world::world::in_core(*v) {
                    return Err(ContractError::invariant(
                        "spine",
                        format!("hole {i}: vertex outside the core"),
                    ));
                }
            }
            if !h.earthwork_budget_m3.is_finite() || h.earthwork_budget_m3 < 0.0 {
                return Err(ContractError::invariant(
                    "earthwork_budget_m3",
                    format!("hole {i}: must be finite and >= 0"),
                ));
            }
            for (k, n) in [("tee", h.tee), ("green", h.green)] {
                let node = self.node(n).ok_or_else(|| {
                    ContractError::invariant("nodes", format!("hole {i}: unknown {k} node"))
                })?;
                let want = if k == "tee" { NodeKind::Tee } else { NodeKind::Green };
                if node.kind != want {
                    return Err(ContractError::invariant(
                        "nodes",
                        format!("hole {i}: {k} node has kind {:?}", node.kind),
                    ));
                }
            }
            for nb in &h.neighbors {
                if *nb as usize >= HOLE_COUNT as usize || *nb == h.index {
                    return Err(ContractError::invariant(
                        "neighbors",
                        format!("hole {i}: bad neighbour {nb}"),
                    ));
                }
            }
        }
        // Budgets balance; borrow covers fill (S6 must not emit a plan S7
        // cannot fund).
        let b = &self.budgets;
        for (name, v) in [
            ("budgets.total_cut_m3", b.total_cut_m3),
            ("budgets.total_fill_m3", b.total_fill_m3),
            ("budgets.total_clearing_m3", b.total_clearing_m3),
            ("budgets.per_hole_ms", b.per_hole_ms),
            ("budgets.clubhouse_radius_m", b.clubhouse_radius_m),
        ] {
            if !v.is_finite() || v < 0.0 {
                return Err(ContractError::invariant(name, "must be finite and >= 0"));
            }
        }
        let hole_sum: f64 = self.holes.iter().map(|h| h.earthwork_budget_m3).sum();
        if hole_sum > b.total_cut_m3 + b.total_fill_m3 + 1e-6 {
            return Err(ContractError::invariant(
                "budgets",
                "sum of hole budgets exceeds total cut + fill",
            ));
        }
        let borrow_sum: f64 = self
            .holes
            .iter()
            .flat_map(|h| h.borrow_sites.iter())
            .map(|s| s.volume_m3)
            .sum();
        if borrow_sum + 1e-6 < b.total_fill_m3 {
            return Err(ContractError::invariant(
                "budgets",
                "available borrow does not cover required fill",
            ));
        }
        // The returning loop.
        let tee0 = self.node(self.holes[0].tee).unwrap().at;
        let green8 = self.node(self.holes[8].green).unwrap().at;
        let d = ((tee0.x - green8.x).powi(2) + (tee0.y - green8.y).powi(2)).sqrt();
        if d > b.clubhouse_radius_m + 1e-6 {
            return Err(ContractError::invariant(
                "returning_loop",
                format!("tee 0 to green 8 is {d:.1} m, radius {} m", b.clubhouse_radius_m),
            ));
        }
        Ok(())
    }

    // ---------------------------------------------------------- artifact io

    /// C3 is pure structure — a single JSON artifact.
    pub fn write_file(&self, path: &Path) -> Result<(), ContractError> {
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| ContractError::Malformed { what: e.to_string() })?;
        std::fs::write(path, json)?;
        Ok(())
    }

    pub fn read_file(path: &Path) -> Result<Self, ContractError> {
        let json = std::fs::read_to_string(path)?;
        let g: CorridorGraph =
            serde_json::from_str(&json).map_err(|e| ContractError::Malformed {
                what: format!("corridor graph: {e}"),
            })?;
        g.validate()?;
        Ok(g)
    }
}
