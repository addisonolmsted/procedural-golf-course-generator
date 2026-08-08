//! The stream-name registry (v3, the v2-pipeline rekey). Every random draw in
//! the pipeline goes through `RunIdentity::stream(<name>)` with a name
//! registered HERE and mirrored in ARCHITECTURE.md (mirror enforced by test).
//! Adding a stream is a reviewed doc+code change; reusing a stream across
//! purposes is forbidden — that is what keeps stages independently
//! deterministic.
//!
//! Registry v3 (`PIPELINE_VERSION` 4): the v2 stage set. The v2 pipeline
//! **never retries**, so every stream is `Stable` (master-seed keyed) and the
//! [`Scope`] distinction is vestigial-but-retained — removing it would rekey
//! every stream for no behavioural gain, and attempt is pinned at 0 anyway.
//! The v1 stage streams with no v2 owner (`mask/v1`, `strokes/v1`,
//! `forcing/v1`) were retired at this rekey; the retained names keep their
//! exact spellings, so attempt-0 draws are bit-identical across the bump.

/// Reroll behavior of a stream. Vestigial under v2 (attempt is pinned at 0,
/// where `Stable` and `Attempt` keys coincide) — see the module docs.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Scope {
    /// Keyed off the MASTER seed.
    Stable,
    /// Keyed off the attempt seed (== master at attempt 0, the only attempt
    /// v2 ever runs).
    Attempt,
}

/// S0: biome draw (when not forced).
pub const ARCH_SELECT: &str = "arch/select/v1";
/// S0: descriptor/θ sampling.
pub const ARCH_PARAMS: &str = "arch/params/v1";
/// v1 stage 01 site framing — retained while `course-framing` is; retired
/// when S1/S2 supersede it.
pub const FRAMING: &str = "framing/v1";
/// S1: macro structure (fields, discontinuities).
pub const PRIMITIVES: &str = "primitives/v1";
/// S2: trunk growth.
pub const SKELETON_TRUNK: &str = "skeleton/trunk/v1";
/// S2: tributary growth.
pub const SKELETON_TRIBUTARY: &str = "skeleton/tributary/v1";
/// S2: the structural modules.
pub const SKELETON_MODULE: &str = "skeleton/module/v1";
/// S3: amplification (patch selection, seams, fill).
pub const AMPLIFY: &str = "amplify/v1";
/// S4: hydrology & transforms.
pub const HYDRO: &str = "hydro/v1";
/// S5: siting + substrate assembly.
pub const SUBSTRATE: &str = "substrate/v1";
/// S6: routing search.
pub const ROUTE: &str = "route/v1";
/// S7: earthworks jitter.
pub const EARTHWORKS: &str = "earthworks/v1";
/// S8: hole layout passes.
pub const LAYOUT: &str = "layout/v1";
/// S9: micro re-pass noise.
pub const MICRO: &str = "micro/v1";
/// S10: cover clumping.
pub const COVER: &str = "cover/v1";
/// S10: zoning/aesthetics draws (palette, exaggeration).
pub const ZONING: &str = "zoning/v1";
/// S10: placement manifest jitter.
pub const PLACEMENT: &str = "placement/v1";
/// S11: playability-sim shot dispersion.
pub const VALIDATE: &str = "validate/v1";
/// INTERNAL: gate-fail reroll sub-seeds. Vestigial under v2; never opened via
/// `RunIdentity::stream`.
pub const REROLL: &str = "reroll/v1";
/// Fixtures and tests only — never used by real stages.
pub const FIXTURE: &str = "fixture/v1";

/// All registered streams with their scopes (doc/test cross-check).
pub const ALL: [(&str, Scope); 20] = [
    (ARCH_SELECT, Scope::Stable),
    (ARCH_PARAMS, Scope::Stable),
    (FRAMING, Scope::Stable),
    (PRIMITIVES, Scope::Stable),
    (SKELETON_TRUNK, Scope::Stable),
    (SKELETON_TRIBUTARY, Scope::Stable),
    (SKELETON_MODULE, Scope::Stable),
    (AMPLIFY, Scope::Stable),
    (HYDRO, Scope::Stable),
    (SUBSTRATE, Scope::Stable),
    (ROUTE, Scope::Stable),
    (EARTHWORKS, Scope::Stable),
    (LAYOUT, Scope::Stable),
    (MICRO, Scope::Stable),
    (COVER, Scope::Stable),
    (ZONING, Scope::Stable),
    (PLACEMENT, Scope::Stable),
    (VALIDATE, Scope::Stable),
    (REROLL, Scope::Stable),
    (FIXTURE, Scope::Stable),
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
        assert_eq!(scope(AMPLIFY), Some(Scope::Stable));
        assert_eq!(scope("mask/v1"), None); // retired at the v3 rekey
        assert_eq!(scope("rogue/v1"), None);
    }
}
