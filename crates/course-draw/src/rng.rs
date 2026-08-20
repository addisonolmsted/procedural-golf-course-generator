//! Attempt 4's stream registry.
//!
//! `course-seed`'s own registry is mirrored against `ARCHITECTURE.md` by a
//! test and enumerates the RETIRED pipeline's stages, so `RunIdentity::stream`
//! panics on any name this branch invents. Rather than edit a registry we do
//! not own — and drag the v2 stage list along with it — attempt 4 keeps its
//! own list and builds the `DetRng` the same way `RunIdentity::stream` does:
//! `DetRng::new(key, name)`, i.e. ChaCha8 over `blake3(key ‖ name)`.
//!
//! Every stream is keyed off `stream_seed()`. Attempt 4 has no retries, so
//! `stream_seed() == seed` and the Stable/Attempt distinction has no content
//! here; keying off `stream_seed` rather than `seed` is the choice that stays
//! correct if retries are ever added.
//!
//! **Reusing one stream for two purposes is forbidden** — that is what keeps
//! steps independently deterministic. Adding a name is a one-line change here
//! plus a row in `docs/network-first/README.md`.

use course_seed::{DetRng, RunIdentity};

/// Step 0: which archetype.
pub const DRAW_SELECT: &str = "n4/draw/select/v1";
/// Step 0: the continuous descriptors.
pub const DRAW_PARAMS: &str = "n4/draw/params/v1";
/// Step 2: trunk mouths, azimuths, inflow.
pub const TEMPLATE_TRUNK: &str = "n4/template/trunk/v1";
/// Step 3: grain, resistance, relief predisposition.
pub const TEMPLATE_FIELDS: &str = "n4/template/fields/v1";
/// Step 3: escarpment traces.
pub const TEMPLATE_SCARP: &str = "n4/template/scarp/v1";

pub const REGISTRY: [&str; 5] = [
    DRAW_SELECT,
    DRAW_PARAMS,
    TEMPLATE_TRUNK,
    TEMPLATE_FIELDS,
    TEMPLATE_SCARP,
];

/// THE way an attempt-4 step obtains randomness. Panics on an unregistered
/// name, exactly as `RunIdentity::stream` does.
pub fn stream(id: &RunIdentity, name: &str) -> DetRng {
    assert!(
        REGISTRY.contains(&name),
        "unregistered attempt-4 stream: {name} (add it to course_draw::rng::REGISTRY)"
    );
    DetRng::new(id.stream_seed(), name.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_no_duplicates() {
        // A name used twice is two purposes sharing one stream.
        for (i, a) in REGISTRY.iter().enumerate() {
            for b in &REGISTRY[i + 1..] {
                assert_ne!(a, b, "duplicate stream name");
            }
        }
    }

    #[test]
    fn distinct_streams_give_distinct_sequences() {
        let id = RunIdentity::from_seed(1);
        let a = stream(&id, DRAW_SELECT).next_u64();
        let b = stream(&id, DRAW_PARAMS).next_u64();
        assert_ne!(a, b);
    }

    #[test]
    #[should_panic(expected = "unregistered attempt-4 stream")]
    fn unregistered_name_panics() {
        stream(&RunIdentity::from_seed(1), "n4/not/a/stream");
    }
}
