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
    /// Probability weights for drawing 0/1/2/3 trunks.
    ///
    /// **All trunks are THROUGH-rivers** (user decision 2026-08-20): a
    /// trunk-scale valley is carved by a river whose catchment far exceeds
    /// the 3 km window — measured, real borders carry only 0.29 trunk-class
    /// outlets per edge — so every trunk crosses the tile and headwater
    /// starts are reserved for tributaries. Mostly 1, sometimes 2 (which may
    /// converge downstream), very rarely 3.
    pub trunk_weights: [f64; 4],
    /// How one-sided the terrace flight is. 0 = symmetric, 1 = all on one bank.
    pub terrace_asymmetry: Range,

    // ---- network: corpus-measured plausibility bands ----
    /// Target median distance-to-channel, metres.
    pub d2c_target_m: Range,
    /// Catchment entering at the trunk mouth from OUTSIDE the tile, km².
    /// Trunk dominance is discharge, not tile-internal area — measured in
    /// `02-drainage-patterns.md` §4.
    pub external_inflow_km2: Range,
    /// 1 = every branch reaches base level, 0 = every branch ends in a pit.
    pub integration: Range,
    // ---- T1 macro catena (corpus provenance: valley_profile.py, 12 tiles
    // per biome, transects at trunk-class channels; see
    // docs/network-first/valley_profile.txt). Measured rise-above-channel
    // and its power exponent differ sharply per biome, so these are real
    // discriminants, not style knobs.
    /// Catena power exponent: rise(d) ∝ d^this. corpus: piedmont 0.96
    /// (near-linear rolling), hc 0.62 (fast rise then benched), rv 0.47.
    pub catena_exp: Range,
    /// Rise above the trunk at 400 m, metres — the valley-wall scale.
    /// corpus: hc 55, piedmont 14, plains 6, sand 8, rv 1.8, heath 1.5.
    /// GOLF override on rv: the corpus floor is flat because the corpus is
    /// floodplain-only; rv's macro relief comes from the TERRACES instead.
    pub rise_400_m: Range,
    /// Valley-floor half-width, metres. corpus floor_hw: pied/hc 24,
    /// plains 48, sand 100, rv 160, heath 240. `floor_widen` multiplies
    /// this (the hc routability lever).
    pub floor_hw_m: Range,

    /// Scale on major-tributary spacing (and claim). >1 = sparser tribs.
    ///
    /// DESIGN-GOVERNED (03-macro-is-designed), user direction 2026-08-21:
    /// river valley much sparser — open floodplain corridors, not incised
    /// ravines crossing every fairway — piedmont/hill country denser.
    /// Recorded departure: the corpus measures rv's CHANNEL density highest
    /// of the six (2.60 km/km², bottomland sloughs and ditches); that is a
    /// fine-tier texture statistic, and the fine tiers (N3) still answer to
    /// the corpus band. This dial governs the MAJOR tier only.
    pub trib_spacing: Range,

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
            trunk_weights: [0.0, 0.62, 0.33, 0.05],
            external_inflow_km2: Range::new(3.0, 12.0),
            integration: Range::fixed(1.0),
            catena_exp: Range::new(0.85, 1.05),
            rise_400_m: Range::new(10.0, 18.0),
            floor_hw_m: Range::new(20.0, 32.0),
            trib_spacing: Range::new(0.70, 0.85),
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
            trunk_weights: [0.0, 0.55, 0.38, 0.07],
            external_inflow_km2: Range::new(2.0, 9.0),
            integration: Range::fixed(1.0),
            catena_exp: Range::new(0.60, 0.78),
            rise_400_m: Range::new(4.5, 8.5),
            floor_hw_m: Range::new(40.0, 60.0),
            trib_spacing: Range::new(1.10, 1.40),
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
            trunk_weights: [0.0, 1.0, 0.0, 0.0],
            external_inflow_km2: Range::new(40.0, 160.0),
            integration: Range::fixed(1.0),
            catena_exp: Range::new(0.42, 0.55),
            // corpus says 1.8 m -- floodplain-only corpus; terraces carry
            // the relief instead (golf, 03-macro-is-designed)
            rise_400_m: Range::new(1.5, 3.0),
            floor_hw_m: Range::new(130.0, 200.0),
            trib_spacing: Range::new(1.70, 2.20),
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
            trunk_weights: [0.0, 0.65, 0.30, 0.05],
            external_inflow_km2: Range::new(3.0, 14.0),
            integration: Range::fixed(1.0),
            catena_exp: Range::new(0.55, 0.70),
            rise_400_m: Range::new(40.0, 62.0),
            floor_hw_m: Range::new(20.0, 32.0),
            trib_spacing: Range::new(0.62, 0.80),
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
            trunk_weights: [0.55, 0.45, 0.0, 0.0],
            external_inflow_km2: Range::new(0.8, 3.0),
            integration: Range::new(0.02, 0.12),
            catena_exp: Range::new(0.68, 0.85),
            rise_400_m: Range::new(1.2, 2.2),
            floor_hw_m: Range::new(190.0, 280.0),
            trib_spacing: Range::new(1.20, 1.50),
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
            trunk_weights: [1.0, 0.0, 0.0, 0.0],
            external_inflow_km2: Range::fixed(0.0),
            integration: Range::fixed(0.0),
            catena_exp: Range::new(0.82, 1.00),
            rise_400_m: Range::new(6.0, 10.0),
            floor_hw_m: Range::new(80.0, 130.0),
            trib_spacing: Range::fixed(1.0),
            // the ONE archetype with real anisotropy -- dune trains
            anisotropy: Range::new(0.55, 0.85),
            resistance_response: Range::fixed(0.0),
            riser_m: Range::fixed(0.0),
        },
    }
}
