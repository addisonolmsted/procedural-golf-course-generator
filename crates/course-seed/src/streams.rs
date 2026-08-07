//! The stream-name registry (v2, scoped). Every random draw in the pipeline
//! goes through `RunIdentity::stream(<name>)` with a name registered HERE and
//! mirrored in ARCHITECTURE.md (mirror enforced by test). Adding a stream is
//! a reviewed doc+code change; reusing a stream across purposes is forbidden —
//! that is what keeps stages independently deterministic.
//!
//! v2 gives every stream a [`Scope`]: `Stable` streams (stages 0–1) key off
//! the MASTER seed and are identical on every attempt — archetype, θ, and
//! site framing survive gate-fail rerolls; `Attempt` streams (stages 2+) key
//! off the attempt seed and resample on every reroll.

/// Reroll behavior of a stream (ARCHITECTURE.md invariant 1).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Keyed off the MASTER seed: identical on every attempt (stages 0–1).
    Stable,
    /// Keyed off the attempt seed: resampled on every reroll (stages 2+).
    Attempt,
}

/// Stage 00: archetype draw (when not forced).
pub const ARCH_SELECT: &str = "arch/select/v1";
/// Stage 00: prior quantile sampling.
pub const ARCH_PARAMS: &str = "arch/params/v1";
/// Stage 01: site framing (window, base level, tilt, grain, provinces).
pub const FRAMING: &str = "framing/v1";
/// Stage 02: routability mask (noise field, crossings).
pub const MASK: &str = "mask/v1";
/// Stage 03: guidance strokes (trunk, boundary landform, hero, datums).
pub const STROKES: &str = "strokes/v1";
/// Stage 04: forcing-field compilation (any stochastic components).
pub const FORCING: &str = "forcing/v1";
/// Stage 07: cover clumping.
pub const COVER: &str = "cover/v1";
/// Stage 09: routing search.
pub const ROUTE: &str = "route/v1";
/// Stage 10: earthworks jitter (bunker shapes etc.).
pub const EARTHWORKS: &str = "earthworks/v1";
/// Stage 12: micro re-pass noise.
pub const MICRO: &str = "micro/v1";
/// Stage 13: placement manifest jitter.
pub const PLACEMENT: &str = "placement/v1";
/// Stage 00 INTERNAL: gate-fail reroll sub-seeds. Keyed off the MASTER seed;
/// never opened via `RunIdentity::stream`.
pub const REROLL: &str = "reroll/v1";
/// Fixtures and tests only — never used by real stages.
pub const FIXTURE: &str = "fixture/v1";

/// All registered streams with their scopes (doc/test cross-check).
pub const ALL: [(&str, Scope); 13] = [
    (ARCH_SELECT, Scope::Stable),
    (ARCH_PARAMS, Scope::Stable),
    (FRAMING, Scope::Stable),
    (MASK, Scope::Attempt),
    (STROKES, Scope::Attempt),
    (FORCING, Scope::Attempt),
    (COVER, Scope::Attempt),
    (ROUTE, Scope::Attempt),
    (EARTHWORKS, Scope::Attempt),
    (MICRO, Scope::Attempt),
    (PLACEMENT, Scope::Attempt),
    (REROLL, Scope::Stable),
    (FIXTURE, Scope::Attempt),
];

/// The scope of a registered stream; `None` for unregistered names.
pub fn scope(name: &str) -> Option<Scope> {
    ALL.iter().find(|(n, _)| *n == name).map(|(_, s)| *s)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::det::DetRng;

    #[test]
    fn streams_unique_and_versioned() {
        for (i, (a, _)) in ALL.iter().enumerate() {
            let (_, ver) = a.rsplit_once("/v").expect("stream has /v<digits> suffix");
            assert!(
                !ver.is_empty() && ver.chars().all(|c| c.is_ascii_digit()),
                "{a}"
            );
            assert!(
                a.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '/' || c == '_'),
                "{a}"
            );
            for (b, _) in &ALL[i + 1..] {
                assert_ne!(a, b);
            }
        }
    }

    #[test]
    fn streams_all_domain_split() {
        let firsts: Vec<u64> = ALL
            .iter()
            .map(|(name, _)| DetRng::new(7, name.as_bytes()).next_u64())
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
        for (name, _) in ALL {
            assert!(
                doc.contains(name),
                "stream `{name}` missing from ARCHITECTURE.md registry"
            );
        }
    }

    #[test]
    fn scope_lookup() {
        assert_eq!(scope(FRAMING), Some(Scope::Stable));
        assert_eq!(scope(MASK), Some(Scope::Attempt));
        assert_eq!(scope("rogue/v1"), None);
    }
}
