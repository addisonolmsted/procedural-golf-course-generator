//! The core-relief budget solve.
//!
//! `core_relief_cap_m` governs the CENTRAL 1.5 km — the ground the holes are
//! routed over. It says nothing about the outer ring, which is scenery and
//! should keep the relief the prior fitted for it.
//!
//! The previous solve did not respect that distinction. When its targeted
//! in-core dips were not enough it fell back to multiplying every feature
//! amplitude AND the regional tilt by `(cap/relief)²`, and two things went
//! wrong at once: it destroyed relief the cap never governed, and — because
//! the square is a deliberate over-correction — it also threw away relief
//! the cap plainly allowed. Mountain seed 1 has a 45.0 m cap and a 60.6 m
//! raw core, and came out at 17.2 m. That single fallback is the largest
//! measured contributor to generated tiles reading as tilted planes.
//!
//! This solve turns three knobs instead, cheapest lever first:
//!
//! 1. [`crate::Tilt::core_tread`] — the core keeps a fraction of the
//!    regional grade and the outer bands take the rest, so the end-to-end
//!    box drop (and therefore `relief_amp_m`, a FITTED knob) is conserved
//!    exactly. Nearly free: it MOVES relief out of the core rather than
//!    deleting it, and a flatter core is what a routable core wants.
//! 2. `core_relax` → [`crate::Resolved::core_weight`] — eases FEATURE
//!    relief down through the `core_protect` shoulder. This costs something
//!    real (interest inside the core), so it runs only when the tread alone
//!    cannot reach the cap.
//! 3. `incision_scale` — shallows the drain system. The only knob here that
//!    is NOT core-localized, hence last; it exists because the core weight
//!    may not touch a drain whose floor lies in the core (that would pond
//!    it), and on the steep archetypes the drain is most of the core relief.
//!
//! All three run over a FIXED two rounds, because they interact: piedmont
//! seed 7's tread could not bite until the incision stage had shallowed the
//! drain whose floor is anchored to the tilt the tread reshapes. Each stage
//! keeps its knob only if it measurably helped, so extra rounds cannot make
//! the result worse.
//!
//! Determinism: fixed round and iteration counts, no RNG, no data-dependent
//! control flow beyond the under-cap early exits. When the raw plan is
//! already under cap every knob stays at exactly 1.0 and the lowered config
//! is bit-identical to what the pre-M3 planner produced — which is why the
//! M3 re-bless moved exactly one golden.

use course_world::world::world_spec;

use crate::config::MacroConfig;
use crate::plan::MacroPlan;
use crate::planner::{in_core_shoulder, lower};

/// Preview resolution for the cap measurement. Fixed, so the solve cannot
/// depend on the resolution the caller happens to rasterize at.
pub const BUDGET_RES_M: f64 = 16.0;

/// Bisection steps per stage. 8 halvings resolve a knob to ~4e-3, which is
/// already far finer than the 16 m preview can distinguish — the result is
/// decided by the geometry, not by the iteration count.
///
/// It was 16, which cost nothing when a plan held 2-4 valleys. M5 grows ~125,
/// and the solve runs up to 2 rounds x 3 stages x this many lowers, each
/// lowering, resolving and sampling ~8.8k core cells against every valley.
/// Halving it halves the dominant cost of `plan()` for a result the preview
/// grid cannot tell apart.
const BISECT_ITERS: usize = 8;

/// The core keeps at least this fraction of the regional grade. Not zero:
/// the tread is what sheet flow follows across the core, and a dead-level
/// core is not golf ground. (Valley floors fall by their own gradient, so
/// this floor is about surface drainage, not the drain network.)
pub const TREAD_MIN: f64 = 0.10;

/// Floor on the feature-relief weight in the core. Not zero: a core with no
/// feature relief at all is a putting green.
pub const RELAX_MIN: f64 = 0.05;

/// Core relief on the fixed preview grid — max minus min of the composed
/// surface over the core window (no shoulder).
pub fn core_relief_16m(cfg: &MacroConfig) -> f64 {
    let r = crate::resolve(cfg);
    let gs = world_spec(BUDGET_RES_M);
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for y in 0..gs.ny {
        for x in 0..gs.nx {
            let p = gs.world_of(x, y);
            if !in_core_shoulder(p, 0.0) {
                continue;
            }
            let z = r.height_at(p);
            lo = lo.min(z);
            hi = hi.max(z);
        }
    }
    if hi > lo {
        hi - lo
    } else {
        0.0
    }
}

/// Floor on the drain-system scale. Zero would delete the drainage the rest
/// of the pipeline needs, so there has to be one.
///
/// Tried at 0.15 for M5 and REVERTED. Core relief is not monotone in this
/// scale, so widening the range made the bisection return the floor, where
/// relief was worse than at 1.0 — and the keep-if-it-helped guard then
/// discarded the 0.25 setting that had been helping. A wider search space is
/// not automatically a better one when the objective is non-monotone.
///
/// With a grown network the residual the other
/// two levers cannot reach is the accordant FLOOR SPREAD — headwaters sit
/// above the trunk by the sum of their reach drops, which is what a drainage
/// network is, and neither the tread nor the core weight may touch a floor
/// (the weight scales the wall rise, which is exactly zero there, precisely
/// so drains cannot pond). Scaling every fall uniformly is the only lever
/// that reaches it and is monotonicity-safe. A 0.15-depth creek is still a
/// creek; the cap is a hard contract.
pub const INCISION_MIN: f64 = 0.25;

/// Which knob a bisection stage is turning.
#[derive(Clone, Copy)]
enum Knob {
    Tread,
    Relax,
    Incision,
}

fn set(plan: &mut MacroPlan, knob: Knob, v: f64) {
    match knob {
        Knob::Tread => plan.frame.core_tread = v,
        Knob::Relax => plan.frame.core_relax = v,
        Knob::Incision => plan.drainage.incision_scale = v,
    }
}

fn get(plan: &MacroPlan, knob: Knob) -> f64 {
    match knob {
        Knob::Tread => plan.frame.core_tread,
        Knob::Relax => plan.frame.core_relax,
        Knob::Incision => plan.drainage.incision_scale,
    }
}

fn relief_at(plan: &mut MacroPlan, knob: Knob, v: f64) -> f64 {
    set(plan, knob, v);
    core_relief_16m(&lower(plan))
}

/// Largest value of `knob` in `[lo, hi]` whose core relief meets `cap`.
///
/// Both knobs are monotone in the right direction — lowering either can only
/// move the core surface toward the un-featured, un-tilted one — so
/// bisection is well defined. `hi` is known to fail when this is called; if
/// `lo` fails too the loop never advances `a` and `lo` is returned, which is
/// the honest answer (the caller checks and records it).
///
/// The knob is left SET to the returned value, so the caller can measure the
/// result without re-lowering.
fn bisect(plan: &mut MacroPlan, knob: Knob, lo: f64, hi: f64, cap: f64) -> f64 {
    let (mut a, mut b) = (lo, hi);
    for _ in 0..BISECT_ITERS {
        let m = 0.5 * (a + b);
        if relief_at(plan, knob, m) <= cap {
            a = m;
        } else {
            b = m;
        }
    }
    set(plan, knob, a);
    a
}

/// Commit a stage's knob only if it actually lowered core relief, and record
/// what happened either way. Returns the relief now in force.
///
/// Neither knob is strictly monotone in core relief, so a stage CAN come out
/// worse than doing nothing. The tread is the case that bites: it reshapes
/// the tilt, and every absolute anchor — ridge crests, bowl rims, the trunk
/// floor — is re-hung on that reshaped surface by `lower`, so features move
/// with it. Measured on piedmont seed 7, treading to 0.1 took core relief
/// from 58.5 m UP to 59.2 m. Without this guard the bisection finds no
/// passing value, returns its floor, and the plan pays the full cost of a
/// fully-treaded core for a worse result.
fn keep_if_it_helped(
    plan: &mut MacroPlan,
    knob: Knob,
    value: f64,
    prior: f64,
    before: f64,
    label: &str,
    cap: f64,
) -> f64 {
    let after = core_relief_16m(&lower(plan));
    if after >= before {
        // Back to what this knob was BEFORE the stage — not to 1.0. Reverting
        // to 1.0 discards whatever an earlier round already bought: round 2's
        // incision stage re-derived the same 0.25 it had set in round 1, saw
        // no further gain, and "reverted" the drain to full depth, undoing an
        // 11.8 m improvement.
        set(plan, knob, prior);
        plan.budget.notes.push(format!(
            "{label}: left at {prior:.3} — {value:.3} bought nothing \
             ({before:.1} -> {after:.1} m)"
        ));
        return before;
    }
    plan.budget.notes.push(format!(
        "{label} {value:.3}: {before:.1} -> {after:.1} m (cap {cap:.1})"
    ));
    after
}

/// Enforce the core cap by relaxing the core, never the box.
pub fn solve(plan: &mut MacroPlan) {
    let cap = plan.budget.cap_m;
    plan.frame.core_tread = 1.0;
    plan.frame.core_relax = 1.0;
    plan.drainage.incision_scale = 1.0;
    let raw = core_relief_16m(&lower(plan));
    plan.budget.relief_raw_m = raw;
    if cap <= 0.0 || raw <= cap {
        // The plan as authored already fits. Leaving both knobs at exactly
        // 1.0 is what makes the un-bound seeds bit-identical to pre-M3.
        plan.budget.relief_final_m = raw;
        return;
    }

    // The three stages, cheapest lever first, over a FIXED two rounds.
    //
    // A second round is not belt-and-braces: the knobs interact, so a stage
    // that could do nothing in round 1 can become the effective one in round
    // 2. Piedmont seed 7 is the case — its core relief is 28.7 m of regional
    // tilt with a drain carved through it, and treading the tilt moved
    // nothing at first because the drain floor is anchored to that tilt and
    // simply moved with it. Only once the incision stage shallowed the drain
    // did the tread have anything to bite on.
    //
    // Each stage keeps its knob only if it actually helped, so extra rounds
    // can never make the result worse — and the count is fixed, so there is
    // no data-dependent control flow.
    let mut now = raw;
    for _round in 0..2 {
        if now <= cap {
            break;
        }
        // Stage 1 — tread the regional grade out of the core. Costs nothing
        // outside it: the drop is redistributed, not removed.
        let prior = get(plan, Knob::Tread);
        let v = bisect(plan, Knob::Tread, TREAD_MIN, 1.0, cap);
        now = keep_if_it_helped(plan, Knob::Tread, v, prior, now, "core tread", cap);
        if now <= cap {
            break;
        }
        // Stage 2 — feature relief inside the core.
        let prior = get(plan, Knob::Relax);
        let v = bisect(plan, Knob::Relax, RELAX_MIN, 1.0, cap);
        now = keep_if_it_helped(plan, Knob::Relax, v, prior, now, "core feature relief", cap);
        if now <= cap {
            break;
        }
        // Stage 3 — the drain. Last, because it is the only lever here that
        // is not core-localized, and it is needed only when the core weight
        // cannot reach the carve: either the drain runs through the core
        // (where easing it would pond the floor) or it grazes the shoulder,
        // which disqualifies the `core_relax_valleys` precondition entirely.
        let prior = get(plan, Knob::Incision);
        let v = bisect(plan, Knob::Incision, INCISION_MIN, 1.0, cap);
        now = keep_if_it_helped(plan, Knob::Incision, v, prior, now, "trunk incision", cap);
    }

    let final_m = core_relief_16m(&lower(plan));
    plan.budget.relief_final_m = final_m;
    if final_m > cap {
        // Say so rather than reaching for the box. What is left is valley
        // incision plus the minimum tread, and neither is this solve's to
        // take — an over-cap residual here is a PLACEMENT finding (a trunk
        // routed through the core too deep to be legal).
        plan.budget.notes.push(format!(
            "cap NOT met at the knob floors (tread {TREAD_MIN}, relax {RELAX_MIN}, \
             incision {INCISION_MIN}): {final_m:.1} m > {cap:.1} m"
        ));
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use course_seed::RunIdentity;
    use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};

    use super::*;
    use crate::planner::plan;

    fn plan_for(arch: ArchetypeId, seed: u64) -> MacroPlan {
        let ov = SpecOverrides {
            forced_archetype: Some(arch),
            params: BTreeMap::new(),
        };
        let spec = CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &ov).unwrap();
        plan(&spec)
    }

    /// The cap is an enforcement. Swept wide because the solve's floors are
    /// finite — if some seed cannot reach the cap by relaxing the core
    /// alone, that is a placement bug and this is where it surfaces.
    #[test]
    fn cap_is_met_on_every_archetype_and_seed() {
        for arch in ArchetypeId::ALL {
            for seed in 1..=40u64 {
                let p = plan_for(arch, seed);
                // 1.05 is the documented preview tolerance for hard
                // requirement 2 (steps/03), and what `contract.rs` has always
                // asserted. This test was stricter than the contract at 1.02,
                // which is a bug in the test rather than a stronger guarantee.
                assert!(
                    p.budget.relief_final_m <= p.budget.cap_m * 1.05,
                    "{arch:?} seed {seed}: {:.2} m > cap {:.2} m (raw {:.2}); notes {:?}",
                    p.budget.relief_final_m,
                    p.budget.cap_m,
                    p.budget.relief_raw_m,
                    p.budget.notes
                );
            }
        }
    }

    /// A plan that already fits must come out untouched — this is what makes
    /// the M3 golden movement diagnostic (only the seeds where the cap binds
    /// may shift) instead of a wholesale re-bless.
    #[test]
    fn an_unbound_plan_keeps_both_knobs_at_one() {
        let mut any = false;
        for arch in ArchetypeId::ALL {
            for seed in 1..=12u64 {
                let p = plan_for(arch, seed);
                if p.budget.relief_raw_m > p.budget.cap_m {
                    continue;
                }
                any = true;
                assert_eq!(p.frame.core_tread, 1.0, "{arch:?} seed {seed}");
                assert_eq!(p.frame.core_relax, 1.0, "{arch:?} seed {seed}");
                assert!(p.budget.notes.is_empty(), "{arch:?} seed {seed}");
            }
        }
        assert!(any, "no unbound seed in the sweep — the test proved nothing");
    }

    /// The point of the milestone: when the cap binds, the solve must land
    /// NEAR the cap rather than far under it. The old `(cap/relief)²`
    /// fallback routinely delivered a third of the allowance.
    #[test]
    fn a_bound_plan_uses_most_of_its_allowance() {
        let mut any = false;
        for arch in ArchetypeId::ALL {
            for seed in 1..=12u64 {
                let p = plan_for(arch, seed);
                if p.budget.relief_raw_m <= p.budget.cap_m {
                    continue;
                }
                any = true;
                assert!(
                    p.budget.relief_final_m >= 0.80 * p.budget.cap_m,
                    "{arch:?} seed {seed}: used only {:.1} of {:.1} m; notes {:?}",
                    p.budget.relief_final_m,
                    p.budget.cap_m,
                    p.budget.notes
                );
            }
        }
        assert!(any, "no bound seed in the sweep — the test proved nothing");
    }

    /// The whole reason the tread exists: satisfying a CORE cap must not
    /// shrink the box. Compared against the same plan with the solve's
    /// knobs forced back to 1.
    #[test]
    fn the_solve_does_not_flatten_the_outer_ring() {
        let box_relief = |p: &MacroPlan| {
            let cfg = lower(p);
            let r = crate::resolve(&cfg);
            let gs = world_spec(BUDGET_RES_M);
            let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
            for y in 0..gs.ny {
                for x in 0..gs.nx {
                    let z = r.height_at(gs.world_of(x, y));
                    lo = lo.min(z);
                    hi = hi.max(z);
                }
            }
            hi - lo
        };
        let mut any = false;
        for arch in ArchetypeId::ALL {
            for seed in 1..=12u64 {
                let p = plan_for(arch, seed);
                if p.budget.relief_raw_m <= p.budget.cap_m {
                    continue;
                }
                any = true;
                let mut raw = p.clone();
                raw.frame.core_tread = 1.0;
                raw.frame.core_relax = 1.0;
                let (solved, unsolved) = (box_relief(&p), box_relief(&raw));
                assert!(
                    solved >= 0.85 * unsolved,
                    "{arch:?} seed {seed}: box relief {solved:.1} m vs {unsolved:.1} m \
                     unsolved — the solve is still eating the outer ring"
                );
            }
        }
        assert!(any, "no bound seed in the sweep — the test proved nothing");
    }
}
