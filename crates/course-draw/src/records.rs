//! The per-archetype draw records.
//!
//! PROVISIONAL AND HAND-AUTHORED, in the shape a fitted envelope will take, so
//! the fit can swap in without a code change. Every range below carries its
//! provenance in a comment: `corpus` = measured in M0
//! (`docs/network-first/01-measurement-policy.md`,
//! `02-drainage-patterns.md`), `golf` = set by the macro-is-designed
//! decision (`03-macro-is-designed.md`), `guess` = neither, and therefore the
//! first thing to challenge when something looks wrong.

use crate::archetype::Archetype;

/// A closed range a descriptor is drawn from.
#[derive(Clone, Copy, Debug)]
pub struct Range {
    pub lo: f64,
    pub hi: f64,
}

impl Range {
    pub const fn new(lo: f64, hi: f64) -> Self {
        Range { lo, hi }
    }
    pub const fn fixed(v: f64) -> Self {
        Range { lo: v, hi: v }
    }
}

/// What one archetype draws. Field docs carry the provenance.
#[derive(Clone, Copy, Debug)]
pub struct Record {
    /// Relative draw weight.
    pub weight: f64,

    // ---- macro: DESIGN-GOVERNED (03-macro-is-designed.md) ----
    /// Total relief the tile is built to, metres.
    pub relief_budget_m: Range,
    /// Extra valley-floor width beyond the discharge law, as a multiplier.
    /// The hill-country routability lever; 1.0 is "no intervention".
    pub floor_widen: Range,
    /// Number of terrace treads on the trunk's valley side. 0 = no flight.
    pub terrace_steps: Range,
    /// How one-sided the terrace flight is. 0 = symmetric, 1 = all on one bank.
    pub terrace_asymmetry: Range,

    // ---- network: corpus-measured plausibility bands ----
    /// Target median distance-to-channel, metres.
    pub d2c_target_m: Range,
    /// Trunk mouths on the base edge.
    pub trunk_count: Range,
    /// Catchment entering at the trunk mouth from OUTSIDE the tile, km².
    /// Trunk dominance is discharge, not tile-internal area — measured in
    /// `02-drainage-patterns.md` §4.
    pub external_inflow_km2: Range,
    /// 1 = every branch reaches base level, 0 = every branch ends in a pit.
    pub integration: Range,

    // ---- steering fields (step 3) ----
    /// How strongly the grain axis biases growth. All six measured isotropic.
    pub anisotropy: Range,
    /// How strongly resistance bands deflect channels.
    pub resistance_response: Range,
    /// Riser height at a resistance contact, metres. The benching lever.
    pub riser_m: Range,
}

pub fn record(a: Archetype) -> Record {
    use Archetype::*;
    match a {
        // Reference case. corpus: relief p50 47.7, d2c 104.3, 3 systems.
        // golf: passes the proxy at only 30%, so a mild floor widen.
        Piedmont => Record {
            weight: 1.0,
            relief_budget_m: Range::new(35.0, 65.0),
            floor_widen: Range::new(1.0, 1.4),
            terrace_steps: Range::fixed(0.0),
            terrace_asymmetry: Range::fixed(0.0),
            d2c_target_m: Range::new(95.0, 115.0),
            trunk_count: Range::new(2.0, 3.0),
            external_inflow_km2: Range::new(0.0, 6.0),
            integration: Range::fixed(1.0),
            anisotropy: Range::new(0.05, 0.15),
            resistance_response: Range::new(0.0, 0.2),
            riser_m: Range::new(0.0, 1.5),
        },
        // corpus: relief 44.5, d2c 115.9 (the sparsest), density 2.21.
        // golf: passes at 83% -- leave the macro alone.
        GreatPlains => Record {
            weight: 1.0,
            relief_budget_m: Range::new(18.0, 40.0),
            floor_widen: Range::fixed(1.0),
            terrace_steps: Range::fixed(0.0),
            terrace_asymmetry: Range::fixed(0.0),
            d2c_target_m: Range::new(108.0, 125.0),
            trunk_count: Range::new(2.0, 4.0),
            external_inflow_km2: Range::new(0.0, 4.0),
            integration: Range::fixed(1.0),
            anisotropy: Range::new(0.05, 0.18),
            // the caprock: strong contact, shallow riser
            resistance_response: Range::new(0.3, 0.6),
            riser_m: Range::new(1.5, 4.0),
        },
        // corpus relief is 4.6 m and BELOW the 7.0 m golf floor -- rv fails the
        // proxy for being dull, not steep (03 §1). golf: the terrace flight
        // supplies the relief, deliberately one-sided.
        RiverValley => Record {
            weight: 1.0,
            relief_budget_m: Range::new(25.0, 60.0),
            floor_widen: Range::new(1.0, 1.3),
            terrace_steps: Range::new(2.0, 4.0),
            terrace_asymmetry: Range::new(0.6, 1.0),
            d2c_target_m: Range::new(90.0, 105.0),
            // ONE trunk. Dominance comes from the inflow, not the count.
            trunk_count: Range::fixed(1.0),
            external_inflow_km2: Range::new(40.0, 160.0),
            integration: Range::fixed(1.0),
            anisotropy: Range::new(0.08, 0.20),
            resistance_response: Range::new(0.1, 0.3),
            riser_m: Range::new(1.0, 3.0),
        },
        // corpus relief 74.4 and 0/27 tiles routable: contig 29.7 ha vs a
        // 124 ha floor. golf: the floor-widen lever, acceptance is the proxy.
        HillCountry => Record {
            weight: 1.0,
            relief_budget_m: Range::new(60.0, 110.0),
            floor_widen: Range::new(2.0, 3.2),
            terrace_steps: Range::fixed(0.0),
            terrace_asymmetry: Range::fixed(0.0),
            d2c_target_m: Range::new(100.0, 120.0),
            trunk_count: Range::new(1.0, 3.0),
            external_inflow_km2: Range::new(0.0, 12.0),
            integration: Range::fixed(1.0),
            anisotropy: Range::new(0.05, 0.15),
            // the identity: every slope a staircase
            resistance_response: Range::new(0.4, 0.8),
            riser_m: Range::new(3.0, 8.0),
        },
        // corpus relief 18.3, flat_floor 12.7% (the highest). Deranged:
        // integration near zero is the identity, and constructing it is free.
        Heathland => Record {
            weight: 1.0,
            relief_budget_m: Range::new(10.0, 28.0),
            floor_widen: Range::fixed(1.0),
            terrace_steps: Range::fixed(0.0),
            terrace_asymmetry: Range::fixed(0.0),
            d2c_target_m: Range::new(90.0, 110.0),
            trunk_count: Range::new(0.0, 1.0),
            external_inflow_km2: Range::fixed(0.0),
            integration: Range::new(0.02, 0.12),
            anisotropy: Range::new(0.05, 0.18),
            resistance_response: Range::fixed(0.0),
            riser_m: Range::fixed(0.0),
        },
        // No network at all -- StructureKind::Aeolian. The network dials are
        // present and inert; the dune train carries the structure.
        Sandhills => Record {
            weight: 1.0,
            relief_budget_m: Range::new(25.0, 55.0),
            floor_widen: Range::fixed(1.0),
            terrace_steps: Range::fixed(0.0),
            terrace_asymmetry: Range::fixed(0.0),
            d2c_target_m: Range::new(95.0, 115.0),
            trunk_count: Range::fixed(0.0),
            external_inflow_km2: Range::fixed(0.0),
            integration: Range::fixed(0.0),
            // the ONE archetype with real anisotropy -- dune trains
            anisotropy: Range::new(0.55, 0.85),
            resistance_response: Range::fixed(0.0),
            riser_m: Range::fixed(0.0),
        },
    }
}
