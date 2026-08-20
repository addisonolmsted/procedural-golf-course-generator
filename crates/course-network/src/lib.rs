//! Step 4 — the network, built in phases: trunks, then tributary tiers.
//!
//! Attempt 4, pipeline step **4** (with step 5's long profiles folded in —
//! elevations exist DURING construction, not after). See
//! `docs/network-first/README.md` and the N-phase plan.
//!
//! **Owns:** the 3-D network graph. Trunks are authored end-to-end paths with
//! long-wave meander; tributaries descend a proto-elevation field from high
//! ground and converge onto the trunks.
//!
//! **Expected failures** (discipline rule 6): every SURFACE metric — there is
//! still no surface. Network planform metrics are checkable HERE.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.

pub mod account;
pub mod trunk_path;

pub use account::{Node, Reach, ACCOUNT_RES_M};
pub use trunk_path::{TrunkPath, TrunkPathParams};

use course_draw::rng::stream;
use course_seed::RunIdentity;
use course_template::Template;

/// Phase N1's stream. Registered in `course_draw::rng`.
pub const NETWORK_TRUNKPATH: &str = "n4/network/trunkpath/v1";

/// The channel-extraction threshold — the corpus policy
/// (`extract_v2.CHANNEL_AREA_M2`), so `d2c`, density and Horton ratios mean
/// the same thing as the bands in `01-measurement-policy.md`.
pub const CHANNEL_AREA_M2: f64 = 6.0e4;

/// The network as of phase N1: trunks only.
pub struct Network {
    pub trunks: Vec<TrunkPath>,
}

impl Network {
    /// Total trunk length, metres.
    pub fn trunk_len_m(&self) -> f64 {
        self.trunks
            .iter()
            .map(|t| t.pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>())
            .sum()
    }
}

/// Build the network a template implies. Phase N1: trunks only.
pub fn build(id: &RunIdentity, t: &Template) -> Network {
    let mut rng = stream(id, NETWORK_TRUNKPATH);
    let relief_budget = t.relief_budget_m;
    let mut params = TrunkPathParams::default();
    let mut trunks: Vec<TrunkPath> = Vec::new();
    for k in &t.trunks {
        // A through-river drops gently across the tile — it is a mature
        // river, not a headwater stream. rv corpus relief is ~4.6 m over
        // 3 km. A headwater trunk climbs to a real fraction of the budget.
        params.head_relief_frac = if k.through {
            rng.range_f64(0.04, 0.12)
        } else {
            rng.range_f64(0.45, 0.65)
        };
        // Sectors make crossings rare, not impossible: two meanders can
        // still touch across a sector boundary. Deterministic ladder —
        // narrow the swing and stretch the wavelength until clean; a trunk
        // that cannot be placed cleanly is DROPPED and counted, never
        // silently overlapped.
        let mut placed = None;
        let mut p2 = TrunkPathParams { ..TrunkPathParams::default() };
        p2.head_relief_frac = params.head_relief_frac;
        for attempt in 0..4 {
            let shrink = 0.72_f64.powi(attempt);
            let cand = trunk_path::build_trunk(
                &mut rng,
                k.mouth,
                k.far,
                k.through,
                k.external_km2,
                &t.fields.relief_pred,
                relief_budget,
                &TrunkPathParams {
                    swing_rad: (p2.swing_rad.0 * shrink, p2.swing_rad.1 * shrink),
                    lam_m: (p2.lam_m.0 / shrink.max(0.5), p2.lam_m.1 / shrink.max(0.5)),
                    head_relief_frac: p2.head_relief_frac,
                    ..TrunkPathParams::default()
                },
            );
            if !crosses_any(&cand, &trunks) {
                placed = Some(cand);
                break;
            }
        }
        if let Some(tp) = placed {
            trunks.push(tp);
        }
    }
    Network { trunks }
}

fn segs_cross(a1: course_world::math::Vec2, a2: course_world::math::Vec2,
              b1: course_world::math::Vec2, b2: course_world::math::Vec2) -> bool {
    let d = |p: course_world::math::Vec2, q: course_world::math::Vec2,
             r: course_world::math::Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (d(b1, b2, a1), d(b1, b2, a2));
    let (d3, d4) = (d(a1, a2, b1), d(a1, a2, b2));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

fn crosses_any(cand: &TrunkPath, placed: &[TrunkPath]) -> bool {
    for other in placed {
        for wa in cand.pts.windows(2) {
            for wb in other.pts.windows(2) {
                if segs_cross(wa[0], wa[1], wb[0], wb[1]) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_draw::{generate, Archetype};
    use course_world::spline::Spine;

    fn net(seed: u64, a: Archetype) -> Network {
        let id = RunIdentity::from_seed(seed);
        let t = course_template::build(&id, &generate(&id, Some(a)));
        build(&id, &t)
    }

    #[test]
    fn deterministic() {
        let a = net(4, Archetype::Piedmont);
        let b = net(4, Archetype::Piedmont);
        assert_eq!(a.trunks.len(), b.trunks.len());
        for (x, y) in a.trunks.iter().zip(&b.trunks) {
            assert_eq!(x.pts.len(), y.pts.len());
            for (p, q) in x.pts.iter().zip(&y.pts) {
                assert_eq!(p.x, q.x);
                assert_eq!(p.y, q.y);
            }
        }
    }

    #[test]
    fn profile_is_monotone_and_zero_at_mouth() {
        for seed in 0..30u64 {
            for a in [Archetype::Piedmont, Archetype::RiverValley, Archetype::HillCountry] {
                for t in net(seed, a).trunks {
                    assert_eq!(t.z[0], 0.0, "{a} seed {seed}: mouth not at base level");
                    for w in t.z.windows(2) {
                        assert!(w[1] >= w[0], "{a} seed {seed}: profile not monotone");
                    }
                }
            }
        }
    }

    #[test]
    fn curves_meet_the_radius_floor() {
        // Smoothness is asserted, not eyeballed. The floor is the params
        // value; the assert allows a small margin for the border clamp.
        for seed in 0..30u64 {
            for a in [Archetype::Piedmont, Archetype::RiverValley] {
                for t in net(seed, a).trunks {
                    let r = Spine::new(t.pts.clone()).min_curvature_radius();
                    assert!(r >= 60.0, "{a} seed {seed}: min radius {r:.0} m");
                }
            }
        }
    }

    #[test]
    fn river_valley_trunk_is_through_and_end_to_end() {
        use course_world::world::EXTENT_M;
        for seed in 0..30u64 {
            let n = net(seed, Archetype::RiverValley);
            assert_eq!(n.trunks.len(), 1, "seed {seed}");
            let t = &n.trunks[0];
            assert!(t.through, "seed {seed}: rv trunk must be a through-river");
            let on_edge = |p: course_world::math::Vec2| {
                p.x <= 1.0 || p.y <= 1.0 || p.x >= EXTENT_M - 1.0 || p.y >= EXTENT_M - 1.0
            };
            assert!(on_edge(t.pts[0]), "seed {seed}: mouth off edge");
            assert!(on_edge(*t.pts.last().unwrap()), "seed {seed}: head off edge");
        }
    }

    #[test]
    fn trunks_do_not_cross() {
        // Few trunks, so the O(n^2) segment test is fine.
        fn segs_cross(a1: course_world::math::Vec2, a2: course_world::math::Vec2,
                      b1: course_world::math::Vec2, b2: course_world::math::Vec2) -> bool {
            let d = |p: course_world::math::Vec2, q: course_world::math::Vec2,
                     r: course_world::math::Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
            let (d1, d2) = (d(b1, b2, a1), d(b1, b2, a2));
            let (d3, d4) = (d(a1, a2, b1), d(a1, a2, b2));
            ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
        }
        for seed in 0..30u64 {
            for a in [Archetype::Piedmont, Archetype::GreatPlains, Archetype::HillCountry] {
                let n = net(seed, a);
                for i in 0..n.trunks.len() {
                    for j in i + 1..n.trunks.len() {
                        for wa in n.trunks[i].pts.windows(2) {
                            for wb in n.trunks[j].pts.windows(2) {
                                assert!(
                                    !segs_cross(wa[0], wa[1], wb[0], wb[1]),
                                    "{a} seed {seed}: trunks {i} and {j} cross"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn sandhills_has_no_trunks() {
        for seed in 0..10u64 {
            assert!(net(seed, Archetype::Sandhills).trunks.is_empty());
        }
    }
}
