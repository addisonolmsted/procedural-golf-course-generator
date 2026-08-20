//! Step 0 — archetype draw: which archetype, which structure kind, and the
//! descriptors the rest of the pipeline is built to.
//!
//! Attempt 4, pipeline step **0**. See `docs/network-first/README.md`.
//!
//! **Owns:** the categorical draw and the continuous descriptors; nothing
//! spatial.
//!
//! **Expected failures** (discipline rule 6 — as binding as the targets):
//! every terrain metric. This stage emits no geometry.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.

pub mod archetype;
pub mod records;
pub mod rng;

pub use archetype::{Archetype, StructureKind};
pub use records::{Range, Record};

use course_seed::{DetRng, RunIdentity};
use rng::stream;

/// What step 0 hands to step 1.
#[derive(Clone, Copy, Debug)]
pub struct SiteDraw {
    pub archetype: Archetype,
    pub structure: StructureKind,
    pub d: Descriptors,
}

/// The drawn continuous descriptors. Units are in the field names.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Descriptors {
    pub relief_budget_m: f64,
    pub floor_widen: f64,
    pub terrace_steps: u32,
    pub terrace_asymmetry: f64,
    pub d2c_target_m: f64,
    pub trunk_count: u32,
    pub external_inflow_km2: f64,
    pub integration: f64,
    pub anisotropy: f64,
    pub resistance_response: f64,
    pub riser_m: f64,
}

fn draw(rng: &mut DetRng, r: Range) -> f64 {
    if r.lo == r.hi {
        // Still advance the stream: a fixed descriptor must not shift the
        // draws that follow it, or adding a range to one archetype would
        // rekey every other descriptor.
        let _ = rng.next_f64();
        r.lo
    } else {
        rng.range_f64(r.lo, r.hi)
    }
}

/// Draw a site. `forced` overrides the archetype WITHOUT changing the
/// transcript — the selection stream still advances, so seed N's descriptors
/// are the same whether or not the archetype was forced.
pub fn generate(id: &RunIdentity, forced: Option<Archetype>) -> SiteDraw {
    let mut sel = stream(id, rng::DRAW_SELECT);
    let total: f64 = Archetype::ALL.iter().map(|a| records::record(*a).weight).sum();
    let mut t = sel.next_f64() * total;
    let mut picked = Archetype::ALL[Archetype::ALL.len() - 1];
    for a in Archetype::ALL {
        let w = records::record(a).weight;
        if t < w {
            picked = a;
            break;
        }
        t -= w;
    }
    let archetype = forced.unwrap_or(picked);

    let rec = records::record(archetype);
    let mut p = stream(id, rng::DRAW_PARAMS);
    let d = Descriptors {
        relief_budget_m: draw(&mut p, rec.relief_budget_m),
        floor_widen: draw(&mut p, rec.floor_widen),
        terrace_steps: draw(&mut p, rec.terrace_steps).round() as u32,
        terrace_asymmetry: draw(&mut p, rec.terrace_asymmetry),
        d2c_target_m: draw(&mut p, rec.d2c_target_m),
        trunk_count: {
            // categorical over the weights; one draw, same as the old range,
            // so the descriptor transcript keeps its shape
            let total: f64 = rec.trunk_weights.iter().sum();
            let mut t = p.next_f64() * total;
            let mut n = 0u32;
            for (k, w) in rec.trunk_weights.iter().enumerate() {
                if t < *w {
                    n = k as u32;
                    break;
                }
                t -= w;
            }
            n
        },
        external_inflow_km2: draw(&mut p, rec.external_inflow_km2),
        integration: draw(&mut p, rec.integration),
        anisotropy: draw(&mut p, rec.anisotropy),
        resistance_response: draw(&mut p, rec.resistance_response),
        riser_m: draw(&mut p, rec.riser_m),
    };
    SiteDraw { archetype, structure: archetype.structure(), d }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn draw_is_deterministic() {
        let id = RunIdentity::from_seed(7);
        assert_eq!(generate(&id, None).d, generate(&id, None).d);
    }

    #[test]
    fn forcing_does_not_move_the_transcript() {
        // Forcing must not shift descriptors, or a forced-archetype gallery
        // would not show what an unforced seed produces.
        let id = RunIdentity::from_seed(11);
        let free = generate(&id, None);
        let forced = generate(&id, Some(free.archetype));
        assert_eq!(free.d, forced.d);
    }

    #[test]
    fn descriptors_land_inside_their_records() {
        for seed in 0..200u64 {
            let id = RunIdentity::from_seed(seed);
            for a in Archetype::ALL {
                let s = generate(&id, Some(a));
                let r = records::record(a);
                let inside = |v: f64, g: Range| v >= g.lo - 1e-9 && v <= g.hi + 1e-9;
                assert!(inside(s.d.relief_budget_m, r.relief_budget_m), "{a} relief {}", s.d.relief_budget_m);
                assert!(inside(s.d.d2c_target_m, r.d2c_target_m), "{a} d2c");
                assert!(inside(s.d.integration, r.integration), "{a} integration");
                assert!(inside(s.d.anisotropy, r.anisotropy), "{a} anisotropy");
                assert!(inside(s.d.floor_widen, r.floor_widen), "{a} floor_widen");
            }
        }
    }

    #[test]
    fn sandhills_is_aeolian_and_carries_no_network() {
        let id = RunIdentity::from_seed(3);
        let s = generate(&id, Some(Archetype::Sandhills));
        assert_eq!(s.structure, StructureKind::Aeolian);
        assert_eq!(s.d.trunk_count, 0);
        assert_eq!(s.d.integration, 0.0);
    }

    #[test]
    fn river_valley_has_one_trunk_and_real_external_inflow() {
        // Trunk dominance is DISCHARGE, not tile-internal catchment --
        // docs/network-first/02-drainage-patterns.md section 4.
        for seed in 0..50u64 {
            let s = generate(&RunIdentity::from_seed(seed), Some(Archetype::RiverValley));
            assert_eq!(s.d.trunk_count, 1);
            assert!(s.d.external_inflow_km2 >= 40.0);
            assert!(s.d.terrace_steps >= 2);
        }
    }

    #[test]
    fn trunk_counts_follow_the_weights() {
        let mut counts = [0usize; 4];
        for seed in 0..400u64 {
            let s = generate(&RunIdentity::from_seed(seed), Some(Archetype::Piedmont));
            counts[s.d.trunk_count as usize] += 1;
        }
        // piedmont weights [0, .62, .33, .05]: mostly 1, sometimes 2, rare 3
        assert_eq!(counts[0], 0);
        assert!(counts[1] > counts[2] && counts[2] > counts[3], "{counts:?}");
        assert!(counts[3] < 50, "{counts:?}");
    }

    #[test]
    fn every_archetype_clears_the_golf_relief_floor() {
        // 03-macro-is-designed.md: the 64 real courses put the relief band
        // floor at 7.0 m. River valley's CORPUS median is 4.6 -- the record
        // is what lifts it, and that is the decision working.
        for a in Archetype::ALL {
            assert!(records::record(a).relief_budget_m.lo >= 7.0, "{a}");
        }
    }
}
