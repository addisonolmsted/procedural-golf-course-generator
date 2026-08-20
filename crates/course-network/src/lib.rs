//! Step 4 — fractal network growth.
//!
//! Attempt 4, pipeline step **4**. See `docs/network-first/README.md`.
//!
//! **Owns:** the 2-D network graph — headward branching from the placed
//! trunks, steered by the template's fields, to the corpus Horton ratios.
//!
//! **Expected failures** (discipline rule 6): every SURFACE metric — there is
//! still no surface. Network planform metrics are checkable HERE, and that is
//! the main practical payoff of the reorder.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.

pub mod account;
pub mod grow;

pub use account::{Reach, ACCOUNT_RES_M};
pub use grow::{GrowParams, GrowStats, Node};

use course_draw::rng::stream;
use course_seed::RunIdentity;
use course_template::Template;
use course_world::math::Vec2;

/// Step 4's stream. Registered in `course_draw::rng`.
pub const NETWORK_GROW: &str = "n4/network/grow/v1";

/// The channel-extraction threshold. **This is the corpus policy**
/// (`extract_v2.CHANNEL_AREA_M2`), so `d2c`, density and the Horton ratios
/// computed here mean the same thing as the bands in
/// `docs/network-first/01-measurement-policy.md`.
pub const CHANNEL_AREA_M2: f64 = 6.0e4;

pub struct Network {
    pub stats: grow::GrowStats,
    pub nodes: Vec<Node>,
    pub reaches: Vec<Reach>,
    /// Strahler order of the largest system.
    pub omega: u32,
}

impl Network {
    /// Channel nodes only — those at or above the extraction threshold.
    pub fn channel_nodes(&self) -> impl Iterator<Item = &Node> {
        self.nodes.iter().filter(|n| n.area_m2 >= CHANNEL_AREA_M2)
    }
    /// Total channel length, metres.
    pub fn channel_len_m(&self) -> f64 {
        self.reaches
            .iter()
            .map(|r| r.pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>())
            .sum()
    }
}

/// Grow the network a template implies.
pub fn build(id: &RunIdentity, t: &Template) -> Network {
    let mut rng = stream(id, NETWORK_GROW);

    // Seeds are the placed trunk mouths, heading inland.
    let seeds: Vec<(Vec2, f64)> = t.trunks.iter().map(|k| (k.mouth, k.azimuth_rad)).collect();

    // claim_m sets drainage density. The parallel-channel estimate (spacing
    // S gives density 1/S and d2c ~ S/4, so S ~ 4D) overshoots, because a
    // dendritic tree packs more length into the same spacing than a set of
    // parallel lines does. Measured on the claim_m ablation
    // (examples/grow_debug.rs): 2.6*D lands density in the 2.21-2.60 band.
    let p = GrowParams {
        claim_m: t.pattern.d2c_target_m * 1.4,
        w_grain: 0.20 + 1.6 * t.pattern.anisotropy,
        ..Default::default()
    };

    let (mut nodes, stats) = grow::grow(&mut rng, &seeds, &t.fields.relief_pred, &t.fields.grain_rad, &p);
    account::accumulate(&mut nodes);
    account::strahler(&mut nodes, CHANNEL_AREA_M2);
    let reaches = account::reaches(&nodes, CHANNEL_AREA_M2);
    let omega = nodes.iter().map(|n| n.order).max().unwrap_or(0);
    Network { nodes, reaches, omega, stats }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_draw::{generate, Archetype};

    fn net(seed: u64, a: Archetype) -> (Network, course_template::Template) {
        let id = RunIdentity::from_seed(seed);
        let t = course_template::build(&id, &generate(&id, Some(a)));
        (build(&id, &t), t)
    }

    #[test]
    fn deterministic() {
        let (a, _) = net(4, Archetype::Piedmont);
        let (b, _) = net(4, Archetype::Piedmont);
        assert_eq!(a.nodes.len(), b.nodes.len());
        for (x, y) in a.nodes.iter().zip(&b.nodes) {
            assert_eq!(x.p.x, y.p.x);
            assert_eq!(x.area_m2, y.area_m2);
        }
    }

    #[test]
    fn is_a_forest_no_loops() {
        // Structural, not policed: a node's parent is always laid before it,
        // so the receiver graph cannot contain a cycle.
        for seed in 0..25u64 {
            for a in Archetype::ALL {
                let (n, _) = net(seed, a);
                for (i, nd) in n.nodes.iter().enumerate() {
                    if let Some(p) = nd.parent {
                        assert!((p as usize) < i, "{a} seed {seed}: parent is not upstream-ordered");
                    }
                }
            }
        }
    }

    #[test]
    fn area_increases_downstream() {
        // Drained area is a sum over territory, so it can only grow toward a
        // mouth. If this fails the accumulation is wrong, and every Horton
        // and threshold number downstream of it is meaningless.
        for seed in 0..15u64 {
            for a in Archetype::ALL {
                let (n, _) = net(seed, a);
                for nd in &n.nodes {
                    if let Some(p) = nd.parent {
                        assert!(
                            n.nodes[p as usize].area_m2 >= nd.area_m2 - 1e-6,
                            "{a} seed {seed}: area drops downstream"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn every_seeded_trunk_starts_a_system() {
        for seed in 0..20u64 {
            let (n, t) = net(seed, Archetype::Piedmont);
            let mouths = n.nodes.iter().filter(|x| x.parent.is_none()).count();
            assert_eq!(mouths, t.trunks.len(), "seed {seed}");
        }
    }

    #[test]
    fn sandhills_grows_nothing() {
        // Aeolian: no trunks, so no network. The empty case is legal.
        for seed in 0..10u64 {
            let (n, _) = net(seed, Archetype::Sandhills);
            assert!(n.nodes.is_empty());
            assert!(n.reaches.is_empty());
        }
    }
}
