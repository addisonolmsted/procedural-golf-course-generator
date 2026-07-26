//! DetRng channel registry. Every random draw in the pipeline goes through
//! `channel(seed, CH_*)` with a domain string registered HERE (and mirrored in
//! ARCHITECTURE.md). Adding a channel is a reviewed doc+code change; reusing a
//! stream across purposes is forbidden — that is what keeps stages
//! independently deterministic.

use golf_core::det::DetRng;

/// Stage 2: archetype draw (when not forced) + prior quantile sampling.
pub const CH_ARCHETYPE: &str = "cp/arch/v1";
pub const CH_PARAMS: &str = "cp/params/v1";
/// Stage 3: primitive placement (counts, frames, phases).
pub const CH_MACRO_PLACE: &str = "cp/macro/place/v1";
/// Stage 4: noise field lattice offsets.
pub const CH_NOISE_FIELD: &str = "cp/noise/field/v1";
/// Stage 5: stochastic components of erosion/hydrology (if any).
pub const CH_HYDRO: &str = "cp/hydro/v1";
/// Stage 6: cover clumping.
pub const CH_COVER_CLUMP: &str = "cp/cover/clump/v1";
/// Stage 8: routing search.
pub const CH_ROUTE: &str = "cp/route/v1";
/// Stage 9: earthworks jitter (bunker shapes etc.).
pub const CH_EARTHWORKS: &str = "cp/earthworks/v1";
/// Orchestrator: gate-fail reroll sub-seed stream.
pub const CH_REROLL: &str = "cp/reroll/v1";
/// Fixtures only — never used by real stages.
pub const CH_FIXTURE: &str = "cp/fixture/v1";

/// All registered channels (doc/test cross-check).
pub const ALL_CHANNELS: [&str; 10] = [
    CH_ARCHETYPE,
    CH_PARAMS,
    CH_MACRO_PLACE,
    CH_NOISE_FIELD,
    CH_HYDRO,
    CH_COVER_CLUMP,
    CH_ROUTE,
    CH_EARTHWORKS,
    CH_REROLL,
    CH_FIXTURE,
];

/// The one way to open a pipeline RNG stream.
pub fn channel(seed: u64, domain: &str) -> DetRng {
    DetRng::new(seed, domain.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_are_unique_and_versioned() {
        for (i, a) in ALL_CHANNELS.iter().enumerate() {
            assert!(a.starts_with("cp/") && a.ends_with("/v1"), "{a}");
            for b in &ALL_CHANNELS[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn streams_are_domain_split() {
        let mut a = channel(7, CH_MACRO_PLACE);
        let mut b = channel(7, CH_NOISE_FIELD);
        assert_ne!(a.next_u64(), b.next_u64());
    }
}
