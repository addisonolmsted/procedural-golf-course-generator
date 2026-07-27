//! Hard-requirement 1 rehearsal: byte-identical double-run, and the reroll
//! sequence replaying deterministically from the master seed alone.

use course_seed::{RunIdentity, streams};

/// A mini "pipeline": open several streams off one identity, draw a mixed
/// u64/f64 transcript, and serialize everything into one byte buffer.
fn transcript(id: &RunIdentity) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(id.canonical_json().as_bytes());
    for name in [streams::ARCH_SELECT, streams::ARCH_PARAMS, streams::FIXTURE] {
        let mut rng = id.stream(name);
        for _ in 0..16 {
            out.extend_from_slice(&rng.next_u64().to_le_bytes());
            out.extend_from_slice(&rng.next_f64().to_le_bytes());
        }
    }
    out
}

#[test]
fn double_run_byte_identical() {
    let a = transcript(&RunIdentity::from_seed(0xC0FFEE));
    let b = transcript(&RunIdentity::from_seed(0xC0FFEE));
    assert_eq!(a, b);
}

#[test]
fn reroll_replay_from_master() {
    // Simulate 3 gate failures: attempts 0..=3.
    let mut first = Vec::new();
    let mut id = RunIdentity::from_seed(777);
    first.push(transcript(&id));
    for _ in 0..3 {
        id = id.reroll().unwrap();
        first.push(transcript(&id));
    }

    // Replay the whole sequence from the master seed alone.
    let mut replay_id = RunIdentity::from_seed(777);
    let mut replay = vec![transcript(&replay_id)];
    for _ in 0..3 {
        replay_id = replay_id.reroll().unwrap();
        replay.push(transcript(&replay_id));
    }

    assert_eq!(first, replay);
    // Attempts genuinely differ from each other (the reroll changed the streams).
    assert_ne!(first[0], first[1]);
    assert_ne!(first[1], first[2]);
}
