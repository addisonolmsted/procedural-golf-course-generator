//! Step 2 — trunk placement.
//!
//! Trunks are PLACED, not grown. The measurement behind that:
//! `docs/network-first/02-drainage-patterns.md` §4 — no system in a real
//! river-valley tile gathers even 30 % of the tile's own area, so a network
//! grown from the tile's 9 km² alone gives three or four comparable systems in
//! every archetype. **Trunk dominance is discharge, sourced outside the
//! window**, and it therefore has to be declared here.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::world::EXTENT_M;

/// Which tile edge drains. The other three are divides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    South,
    North,
    West,
    East,
}

impl Edge {
    pub fn from_index(i: usize) -> Edge {
        match i % 4 {
            0 => Edge::South,
            1 => Edge::North,
            2 => Edge::West,
            _ => Edge::East,
        }
    }
    /// A point at parameter `t` in [0,1] along the edge.
    pub fn point(self, t: f64) -> Vec2 {
        let u = t * EXTENT_M;
        match self {
            Edge::South => Vec2::new(u, 0.0),
            Edge::North => Vec2::new(u, EXTENT_M),
            Edge::West => Vec2::new(0.0, u),
            Edge::East => Vec2::new(EXTENT_M, u),
        }
    }
    /// Inward normal — the direction a trunk heads from its mouth.
    pub fn inward(self) -> f64 {
        match self {
            Edge::South => core::f64::consts::FRAC_PI_2,
            Edge::North => -core::f64::consts::FRAC_PI_2,
            Edge::West => 0.0,
            Edge::East => core::f64::consts::PI,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Trunk {
    /// Where it leaves the tile.
    pub mouth: Vec2,
    /// Heading INTO the tile at the mouth, radians.
    pub azimuth_rad: f64,
    /// Catchment entering here from outside the window, km². This is what
    /// makes a trunk a trunk.
    pub external_km2: f64,
}

pub struct TrunkParams {
    pub count: u32,
    pub external_inflow_km2: f64,
    /// Fraction of the total inflow the largest trunk takes. 1.0 = a single
    /// dominant river; near 1/count = comparable systems.
    pub dominance: f64,
}

/// Minimum separation between mouths. Below this the tile reads as one river
/// drawn twice — the failure `heartland` hit at 420 m, kept here.
pub const MOUTH_SEP_M: f64 = 420.0;

/// Minimum distance from a mouth to the nearest corner.
///
/// A mouth near a corner has almost no catchment behind it inside the tile,
/// and its trunk runs along the adjacent edge — which is the edge-hugging
/// trunk `heartland` spent a round diagnosing. Measured before this constant
/// existed: 23.6 % of mouths sat within 450 m of a corner, the closest at
/// 260 m. 500 m leaves 2000 m of usable edge, which still holds the four
/// mouths great plains can draw at `MOUTH_SEP_M`.
pub const MOUTH_CORNER_CLEARANCE_M: f64 = 500.0;

pub fn place(rng: &mut DetRng, p: &TrunkParams, relief_pred: &Grid<f64>, edge: Edge) -> Vec<Trunk> {
    if p.count == 0 {
        return Vec::new();
    }
    // Candidate mouths, scored by how LOW the predisposition field is just
    // inboard of the edge: a river leaves through a low, not over a rise.
    // 64 candidates is enough to resolve a 420 m separation on a 3 km edge.
    const CAND: usize = 64;
    let probe_in = 180.0;
    let (ic, is) = (math::cos(edge.inward()), math::sin(edge.inward()));
    let mut scored: Vec<(f64, f64)> = (0..CAND)
        .map(|i| {
            let c = MOUTH_CORNER_CLEARANCE_M / EXTENT_M;
            let t = c + (1.0 - 2.0 * c) * (i as f64 + 0.5) / CAND as f64;
            let m = edge.point(t);
            let probe = Vec2::new(m.x + ic * probe_in, m.y + is * probe_in);
            // low is good -> negate
            let mut s = -relief_pred.bilinear(probe);
            // a little noise so equal-relief edges do not always pick the
            // same t, and so the choice is not a pure argmax of a smooth field
            s += rng.range_f64(-0.15, 0.15);
            (t, s)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));

    let mut chosen: Vec<f64> = Vec::new();
    for (t, _) in scored {
        if chosen.len() >= p.count as usize {
            break;
        }
        let m = edge.point(t);
        if chosen.iter().all(|&c| edge.point(c).distance(m) >= MOUTH_SEP_M) {
            chosen.push(t);
        }
    }
    chosen.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

    // Split the declared inflow: the first trunk takes `dominance`, the rest
    // share what is left.
    let n = chosen.len();
    chosen
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let share = if n == 1 {
                1.0
            } else if i == 0 {
                p.dominance
            } else {
                (1.0 - p.dominance) / (n as f64 - 1.0)
            };
            let jitter = rng.range_f64(-0.35, 0.35);
            Trunk {
                mouth: edge.point(t),
                azimuth_rad: edge.inward() + jitter,
                external_km2: p.external_inflow_km2 * share,
            }
        })
        .collect()
}
