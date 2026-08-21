//! Steps 1-3 — structural template, trunk placement, macro FIELDS.
//!
//! Attempt 4, pipeline steps **1-3**. See `docs/network-first/README.md`.
//!
//! **Owns:** the drainage pattern and the macro constraints it implies; 1-3
//! trunk mouths on the base edge; grain, resistance bands, escarpment traces,
//! relief predisposition. FIELDS, NOT TERRAIN — no heightfield exists until
//! step 6.
//!
//! **Expected failures** (discipline rule 6): every terrain metric. The M1
//! test is blind legibility — a reviewer names the drainage pattern from a
//! field render.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.

pub mod fields;
pub mod trunks;

pub use fields::{FieldParams, MacroFields};
pub use trunks::{Edge, Trunk, TrunkParams};

use course_draw::rng::{self, stream};
use course_draw::{Archetype, SiteDraw, StructureKind};
use course_seed::RunIdentity;

/// The pattern dials step 4 grows against.
///
/// All six Heartland archetypes measured DENDRITIC
/// (`docs/network-first/02-drainage-patterns.md` §2): junction medians
/// 37-45 deg, orthogonal share 8.5-13.4 %, orientation near-isotropic. The
/// dials exist because future packs need them (Highlands' incised structural
/// valleys, Desert's parallel fans) and because they cost nothing at their
/// defaults — but **they are not what separates the base six**, and tuning
/// them per archetype is work that should not be done.
#[derive(Clone, Copy, Debug)]
pub struct PatternDials {
    /// 0 dendritic, 1 parallel. Measured 0.08-0.15 across all six.
    pub anisotropy: f64,
    /// Target confluence angle, radians. Measured 37-45 deg everywhere.
    pub junction_target_rad: f64,
    /// 0 cut through hard bands, 1 deflect along them.
    pub resistance_response: f64,
    /// 1 every branch reaches base level, 0 every branch ends in a pit.
    pub integration: f64,
    /// Target median distance-to-channel, metres.
    pub d2c_target_m: f64,
}

/// What steps 1-3 hand to step 4.
pub struct Template {
    pub archetype: Archetype,
    /// Carried through for the trunk long profile.
    pub relief_budget_m: f64,
    /// Major-tributary spacing scale, from the draw. >1 = sparser.
    pub trib_spacing_scale: f64,
    pub structure: StructureKind,
    pub pattern: PatternDials,
    pub base_edge: Edge,
    pub trunks: Vec<Trunk>,
    pub fields: MacroFields,
    /// Escarpment traces: the resistance contacts strong enough to bench.
    pub scarps: Vec<Vec<course_world::math::Vec2>>,
}

/// Shortest scarp worth carrying. Below this it is a blob, not a line.
pub const SCARP_MIN_LEN_M: f64 = 300.0;
/// Shortest riser that counts as a bench. Piedmont's veneer draws risers down
/// to a few centimetres; a 0.3 m step is a texture feature, not a landform,
/// and tracing it produced 28-56 "scarps" on the archetype documented as
/// having a thin veneer and slight benching.
pub const SCARP_MIN_RISER_M: f64 = 1.5;

pub fn build(id: &RunIdentity, draw: &SiteDraw) -> Template {
    let d = &draw.d;

    // --- step 3: the fields. Built FIRST because trunk placement reads the
    // relief predisposition to put mouths in lows.
    let mut fr = stream(id, rng::TEMPLATE_FIELDS);
    // Band period from the riser height: a thick stack with tall risers steps
    // less often than a thin one. 0 riser -> no meaningful banding.
    let fields = fields::build(
        &mut fr,
        &FieldParams {
            anisotropy: d.anisotropy,
            resistance_response: d.resistance_response,
            riser_m: d.riser_m,
            relief_budget_m: d.relief_budget_m,
        },
    );

    // --- step 2: trunks.
    let mut tr = stream(id, rng::TEMPLATE_TRUNK);
    let base_edge = Edge::from_index(tr.below(4));
    // One trunk takes nearly all the inflow; with several, the first still
    // leads but the others are real rivers.
    let dominance = if d.trunk_count <= 1 { 1.0 } else { 0.62 };
    let trunks = trunks::place(
        &mut tr,
        &TrunkParams {
            count: d.trunk_count,
            external_inflow_km2: d.external_inflow_km2,
            dominance,
        },
        &fields.relief_pred,
        base_edge,
    );

    // --- step 3b: scarps, on the stream that owns them.
    let mut _sr = stream(id, rng::TEMPLATE_SCARP);
    let scarps = if d.riser_m >= SCARP_MIN_RISER_M {
        fields::scarp_traces(&fields, SCARP_MIN_LEN_M)
    } else {
        Vec::new()
    };

    Template {
        archetype: draw.archetype,
        relief_budget_m: d.relief_budget_m,
        trib_spacing_scale: d.trib_spacing_scale,
        structure: draw.structure,
        pattern: PatternDials {
            anisotropy: d.anisotropy,
            junction_target_rad: 41.0_f64.to_radians(),
            resistance_response: d.resistance_response,
            integration: d.integration,
            d2c_target_m: d.d2c_target_m,
        },
        base_edge,
        trunks,
        fields,
        scarps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_world::world::EXTENT_M;

    fn tpl(seed: u64, a: Archetype) -> Template {
        let id = RunIdentity::from_seed(seed);
        build(&id, &course_draw::generate(&id, Some(a)))
    }

    #[test]
    fn deterministic() {
        let a = tpl(5, Archetype::Piedmont);
        let b = tpl(5, Archetype::Piedmont);
        assert_eq!(a.trunks.len(), b.trunks.len());
        for (x, y) in a.trunks.iter().zip(&b.trunks) {
            assert_eq!(x.mouth.x, y.mouth.x);
            assert_eq!(x.mouth.y, y.mouth.y);
            assert_eq!(x.external_km2, y.external_km2);
        }
        assert_eq!(a.scarps.len(), b.scarps.len());
    }

    #[test]
    fn trunk_count_and_separation_hold() {
        for seed in 0..60u64 {
            for a in Archetype::ALL {
                let t = tpl(seed, a);
                let want = course_draw::generate(&RunIdentity::from_seed(seed), Some(a)).d.trunk_count;
                // The drawn count is a CEILING, not a target. A mouth is only
                // accepted if a divide stands between it and every mouth
                // already placed (MIN_DIVIDE_PROMINENCE), so a tile whose base
                // edge has one broad low genuinely supports one trunk however
                // many were drawn. Measured fulfilment 61-80%.
                assert!(t.trunks.len() <= want as usize, "{a} seed {seed}: too many trunks");
                // The wide floor must actually bind, not just the divide rule.
                // Joining trunks SHARE the primary's mouth by design, so
                // mouth-based rules apply only to trunks with their own outlet.
                let own: Vec<_> = t.trunks.iter().filter(|k| k.joins.is_none()).collect();
                for i in 0..own.len() {
                    for j in i + 1..own.len() {
                        let d = own[i].mouth.distance(own[j].mouth);
                        assert!(d >= trunks::MOUTH_SEP_M - 1e-6, "{a} seed {seed}: {d:.0} m apart");
                    }
                }
                assert!(want == 0 || !t.trunks.is_empty(), "{a} seed {seed}: no trunk at all");
            }
        }
    }

    #[test]
    fn mouths_sit_on_the_base_edge_and_off_the_corners() {
        for seed in 0..40u64 {
            let t = tpl(seed, Archetype::Piedmont);
            for tk in t.trunks.iter().filter(|k| k.joins.is_none()) {
                let on_edge = match t.base_edge {
                    Edge::South => tk.mouth.y == 0.0,
                    Edge::North => tk.mouth.y == EXTENT_M,
                    Edge::West => tk.mouth.x == 0.0,
                    Edge::East => tk.mouth.x == EXTENT_M,
                };
                assert!(on_edge, "seed {seed}: mouth off the base edge");
                let along = match t.base_edge {
                    Edge::South | Edge::North => tk.mouth.x,
                    _ => tk.mouth.y,
                };
                let corner = along.min(EXTENT_M - along);
                assert!(
                    corner >= trunks::MOUTH_CORNER_CLEARANCE_M - 1e-6,
                    "seed {seed}: mouth {corner:.0} m from a corner"
                );
            }
        }
    }

    #[test]
    fn river_valley_concentrates_its_inflow() {
        for seed in 0..30u64 {
            let t = tpl(seed, Archetype::RiverValley);
            assert_eq!(t.trunks.len(), 1);
            assert!(t.trunks[0].external_km2 >= 40.0, "seed {seed}");
        }
    }

    #[test]
    fn benched_archetypes_get_scarps_and_flat_ones_do_not() {
        // hill country's identity is that every slope is a staircase;
        // piedmont has a thin veneer and heathland none at all.
        let hc: usize = (0..12).map(|s| tpl(s, Archetype::HillCountry).scarps.len()).sum();
        let heath: usize = (0..12).map(|s| tpl(s, Archetype::Heathland).scarps.len()).sum();
        assert!(hc > 0, "hill country grew no scarps");
        assert_eq!(heath, 0, "heathland should have no strata");
    }

    #[test]
    fn fields_are_in_range() {
        for seed in 0..20u64 {
            for a in Archetype::ALL {
                let t = tpl(seed, a);
                for v in &t.fields.resistance_hint.data {
                    assert!((0.0..=1.0).contains(v), "{a}: resistance_hint {v}");
                }
                for v in &t.fields.relief_pred.data {
                    assert!((-1.0..=1.0).contains(v), "{a}: relief_pred {v}");
                }
            }
        }
    }

    #[test]
    fn adjacent_mouths_are_separated_by_a_divide() {
        // The review concern this rule exists for: two mouths in one low grow
        // into the same trunk side by side and may cross. Distance does not
        // test for that -- a divide does. Before the rule, 43% of adjacent
        // pairs had NO high between them at 420 m separation.
        use course_world::math::Vec2;
        for a in [Archetype::Piedmont, Archetype::GreatPlains, Archetype::HillCountry] {
            for seed in 0..80u64 {
                let t = tpl(seed, a);
                let own: Vec<_> = t.trunks.iter().filter(|k| k.joins.is_none()).cloned().collect();
                if own.len() < 2 {
                    continue;
                }
                let (ic, is) = (
                    course_world::math::cos(t.base_edge.inward()),
                    course_world::math::sin(t.base_edge.inward()),
                );
                let g = |along: f64| {
                    let m = match t.base_edge {
                        Edge::South => Vec2::new(along, 0.0),
                        Edge::North => Vec2::new(along, EXTENT_M),
                        Edge::West => Vec2::new(0.0, along),
                        Edge::East => Vec2::new(EXTENT_M, along),
                    };
                    t.fields.relief_pred.bilinear(Vec2::new(m.x + ic * 180.0, m.y + is * 180.0))
                };
                let mut al: Vec<f64> = own.iter().map(|k| match t.base_edge {
                    Edge::South | Edge::North => k.mouth.x,
                    _ => k.mouth.y,
                }).collect();
                al.sort_by(|x, y| x.partial_cmp(y).unwrap());
                for w in al.windows(2) {
                    let n = ((w[1] - w[0]) / 16.0).ceil().max(2.0) as usize;
                    let mut peak = f64::MIN;
                    for i in 1..n {
                        peak = peak.max(g(w[0] + (w[1] - w[0]) * i as f64 / n as f64));
                    }
                    let prom = peak - g(w[0]).max(g(w[1]));
                    assert!(
                        prom >= trunks::MIN_DIVIDE_PROMINENCE - 1e-6,
                        "{a} seed {seed}: mouths share a low (prominence {prom:.3})"
                    );
                }
            }
        }
    }

    #[test]
    fn sandhills_places_no_trunks() {
        for seed in 0..20u64 {
            let t = tpl(seed, Archetype::Sandhills);
            assert_eq!(t.structure, StructureKind::Aeolian);
            assert!(t.trunks.is_empty());
        }
    }
}
