//! The stream-name registry. Every random draw in the pipeline goes through
//! `RunIdentity::stream(<name>)` with a name registered HERE and mirrored in
//! ARCHITECTURE.md (mirror enforced by test). Adding a stream is a reviewed
//! doc+code change; reusing a stream across purposes is forbidden — that is
//! what keeps steps independently deterministic.

/// Step 02: archetype draw (when not forced).
pub const ARCH_SELECT: &str = "arch/select/v1";
/// Step 02: prior quantile sampling.
pub const ARCH_PARAMS: &str = "arch/params/v1";
/// Step 03: primitive placement (counts, frames, phases).
pub const MACRO_PLACE: &str = "macro/place/v1";
/// Step 04: noise field lattice offsets.
pub const NOISE_FIELD: &str = "noise/field/v1";
/// Step 05: stochastic components of erosion/hydrology (if any).
pub const HYDRO: &str = "hydro/v1";
/// Step 06: cover clumping.
pub const COVER_CLUMP: &str = "cover/clump/v1";
/// Step 08: routing search.
pub const ROUTE: &str = "route/v1";
/// Step 09: earthworks jitter (bunker shapes etc.).
pub const EARTHWORKS: &str = "earthworks/v1";
/// Step 01 INTERNAL: gate-fail reroll sub-seeds. Keyed off the MASTER seed;
/// never opened via `RunIdentity::stream` (which keys off the attempt seed).
pub const REROLL: &str = "reroll/v1";
/// Fixtures and tests only — never used by real steps.
pub const FIXTURE: &str = "fixture/v1";

/// All registered streams (doc/test cross-check).
pub const ALL: [&str; 10] = [
    ARCH_SELECT,
    ARCH_PARAMS,
    MACRO_PLACE,
    NOISE_FIELD,
    HYDRO,
    COVER_CLUMP,
    ROUTE,
    EARTHWORKS,
    REROLL,
    FIXTURE,
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::det::DetRng;

    #[test]
    fn streams_unique_and_versioned() {
        for (i, a) in ALL.iter().enumerate() {
            assert!(a.ends_with("/v1"), "{a}");
            assert!(
                a.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '/' || c == '_'),
                "{a}"
            );
            for b in &ALL[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn streams_all_domain_split() {
        let firsts: Vec<u64> = ALL
            .iter()
            .map(|name| DetRng::new(7, name.as_bytes()).next_u64())
            .collect();
        for (i, a) in firsts.iter().enumerate() {
            for b in &firsts[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    /// The registry must appear verbatim in ARCHITECTURE.md (invariant 1).
    #[test]
    fn streams_mirrored_in_architecture_md() {
        let doc = include_str!("../../../ARCHITECTURE.md");
        for name in ALL {
            assert!(
                doc.contains(name),
                "stream `{name}` missing from ARCHITECTURE.md registry"
            );
        }
    }
}
