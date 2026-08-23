//! Attempt 5's stream registry.
//!
//! Two registries already exist and neither will accept our names:
//! `course-seed`'s own list enumerates the RETIRED v2 pipeline's stages (and is
//! mirrored against `ARCHITECTURE.md` by a test), and `course-draw`'s
//! enumerates attempt 4's. Rather than edit a registry we do not own, attempt 5
//! keeps its own list and builds the `DetRng` exactly the way
//! `RunIdentity::stream` does: `DetRng::new(stream_seed, name)`, i.e. ChaCha8
//! over `blake3(key ‖ name)`.
//!
//! **Reusing one stream for two purposes is forbidden** — that is what keeps
//! stages independently deterministic. Adding a name is a one-line change here
//! plus a row in `docs/sandhills/README.md` §7.

use course_seed::{DetRng, RunIdentity};

/// A0 — mode and the continuous descriptors.
pub const DRAW: &str = "n5/sand/draw/v1";
/// A1 — the wind direction field.
pub const WIND: &str = "n5/sand/wind/v1";
/// A2 — the crest network.
pub const CREST: &str = "n5/sand/crest/v1";
/// A4b — the hummock tier: individual dunes on the ridge belts.
pub const HUMMOCK: &str = "n5/sand/hummock/v1";
/// A5 — blowouts and deflation hollows.
pub const BLOWOUT: &str = "n5/sand/blowout/v1";
/// C2 — the channel network (fluvial mode).
pub const CHANNEL: &str = "n5/sand/channel/v1";
/// A6 / C4 — allogenic river, interdune lakes, wet bottoms.
pub const WATER: &str = "n5/sand/water/v1";
/// T — fabric.
pub const TEXTURE: &str = "n5/sand/texture/v1";

pub const REGISTRY: [&str; 8] = [DRAW, WIND, CREST, HUMMOCK, BLOWOUT, CHANNEL, WATER, TEXTURE];

/// THE way an attempt-5 stage obtains randomness. Panics on an unregistered
/// name, exactly as `RunIdentity::stream` does.
pub fn stream(id: &RunIdentity, name: &str) -> DetRng {
    assert!(
        REGISTRY.contains(&name),
        "unregistered attempt-5 stream: {name} (add it to course_sandhills::rng::REGISTRY)"
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
                assert_ne!(a, b, "duplicate stream name: {a}");
            }
        }
    }

    #[test]
    fn every_name_is_attempt_five() {
        // A name carried over from another attempt would silently share a
        // sequence with that attempt's stage.
        for n in REGISTRY {
            assert!(n.starts_with("n5/sand/"), "stream outside the n5 namespace: {n}");
        }
    }

    #[test]
    fn distinct_streams_give_distinct_sequences() {
        let id = RunIdentity::from_seed(1);
        assert_ne!(stream(&id, DRAW).next_u64(), stream(&id, WIND).next_u64());
    }

    #[test]
    #[should_panic(expected = "unregistered attempt-5 stream")]
    fn unregistered_name_panics() {
        stream(&RunIdentity::from_seed(1), "n5/sand/not/a/stream");
    }
}
