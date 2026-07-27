//! RunIdentity: the step-01 artifact. One master seed + pipeline version +
//! attempt counter; everything downstream is a pure function of it.

use serde::{Deserialize, Serialize};

use crate::det::DetRng;
use crate::{MAX_ATTEMPTS, PIPELINE_VERSION, streams};

/// The identity of one generated course run. Serialized as `run.json`
/// (compact JSON, field order as declared here — the canonical bytes for the
/// byte-identity requirement).
///
/// `seed` is ALWAYS the master seed the run started from, across rerolls;
/// the per-attempt working seed is derived, never stored
/// (see [`RunIdentity::stream_seed`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunIdentity {
    pub seed: u64,
    /// Global contract version ([`PIPELINE_VERSION`]).
    pub pipeline_version: u32,
    /// 0 for the first try; >0 after gate rerolls (step 07).
    pub attempt: u32,
}

#[derive(Debug)]
pub enum SeedError {
    /// OS entropy was unavailable (only possible in `from_entropy`).
    Entropy(getrandom::Error),
    /// `reroll()` past [`MAX_ATTEMPTS`] — the run has failed for good.
    AttemptsExhausted { attempt: u32 },
    /// `from_json`: the bytes are not a valid `RunIdentity`.
    Json(serde_json::Error),
    /// `from_json`: a `run.json` written by a different contract version —
    /// loud by principle, never silently reinterpreted.
    VersionMismatch { found: u32 },
    /// `from_json`: `attempt` outside `0..MAX_ATTEMPTS` — no valid run ever
    /// records one, so the file is corrupt or hand-built wrong.
    AttemptOutOfRange { attempt: u32 },
}

impl std::fmt::Display for SeedError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SeedError::Entropy(e) => write!(f, "OS entropy unavailable: {e}"),
            SeedError::AttemptsExhausted { attempt } => {
                write!(
                    f,
                    "gate reroll limit reached at attempt {attempt} (max {MAX_ATTEMPTS})"
                )
            }
            SeedError::Json(e) => write!(f, "invalid run.json: {e}"),
            SeedError::VersionMismatch { found } => {
                write!(
                    f,
                    "run.json pipeline_version {found} != current {PIPELINE_VERSION}"
                )
            }
            SeedError::AttemptOutOfRange { attempt } => {
                write!(f, "run.json attempt {attempt} outside 0..{MAX_ATTEMPTS}")
            }
        }
    }
}

impl std::error::Error for SeedError {}

impl RunIdentity {
    /// Identity for a caller-supplied seed: attempt 0, current pipeline version.
    pub fn from_seed(seed: u64) -> Self {
        RunIdentity {
            seed,
            pipeline_version: PIPELINE_VERSION,
            attempt: 0,
        }
    }

    /// Draw a seed from OS entropy exactly ONCE and record it in the returned
    /// identity. The caller must persist `canonical_json()` (the `run.json`
    /// artifact) before running the pipeline; this crate never re-draws —
    /// calling `from_entropy` again is visibly a new run at the call site.
    pub fn from_entropy() -> Result<Self, SeedError> {
        let mut bytes = [0u8; 8];
        getrandom::fill(&mut bytes).map_err(SeedError::Entropy)?;
        Ok(Self::from_seed(u64::from_le_bytes(bytes)))
    }

    /// The next attempt after a gate fail (step 07). Same master seed,
    /// attempt + 1; errors once [`MAX_ATTEMPTS`] attempts have been used.
    pub fn reroll(&self) -> Result<Self, SeedError> {
        // >= comparison (not attempt + 1) so an out-of-range hand-built
        // identity errors instead of overflowing at u32::MAX.
        if self.attempt >= MAX_ATTEMPTS - 1 {
            return Err(SeedError::AttemptsExhausted {
                attempt: self.attempt,
            });
        }
        Ok(RunIdentity {
            attempt: self.attempt + 1,
            ..*self
        })
    }

    /// The seed all of this attempt's streams are keyed off: the master seed
    /// at attempt 0; for attempt a > 0, draw a-1 (0-based) of the master
    /// seed's `reroll/v1` stream — so the whole retry sequence replays
    /// deterministically from the master seed alone.
    pub fn stream_seed(&self) -> u64 {
        if self.attempt == 0 {
            self.seed
        } else {
            DetRng::new(self.seed, streams::REROLL.as_bytes()).nth_u64(u64::from(self.attempt) - 1)
        }
    }

    /// THE way a step obtains randomness. `name` must be registered in
    /// [`streams`]; `reroll/v1` is internal to step 01 (it keys off the
    /// master seed, not the attempt seed, so opening it here would be wrong).
    pub fn stream(&self, name: &str) -> DetRng {
        assert!(streams::ALL.contains(&name), "unregistered stream: {name}");
        assert_ne!(
            name,
            streams::REROLL,
            "reroll/v1 is internal to course-seed"
        );
        DetRng::new(self.stream_seed(), name.as_bytes())
    }

    /// The canonical `run.json` bytes: compact JSON, declared field order,
    /// no trailing newline. Integers only, so round-trips are exact.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("RunIdentity is always serializable")
    }

    /// Load and validate a `run.json`. Contract mismatches are loud: a
    /// different `pipeline_version` or an `attempt` outside
    /// `0..MAX_ATTEMPTS` is rejected, never silently run.
    pub fn from_json(s: &str) -> Result<Self, SeedError> {
        let id: RunIdentity = serde_json::from_str(s).map_err(SeedError::Json)?;
        if id.pipeline_version != PIPELINE_VERSION {
            return Err(SeedError::VersionMismatch {
                found: id.pipeline_version,
            });
        }
        if id.attempt >= MAX_ATTEMPTS {
            return Err(SeedError::AttemptOutOfRange {
                attempt: id.attempt,
            });
        }
        Ok(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn from_seed_is_attempt_zero() {
        let id = RunIdentity::from_seed(99);
        assert_eq!(id.seed, 99);
        assert_eq!(id.pipeline_version, PIPELINE_VERSION);
        assert_eq!(id.attempt, 0);
        assert_eq!(id.stream_seed(), 99);
    }

    #[test]
    fn reroll_chain_deterministic() {
        let build = || {
            let mut ids = vec![RunIdentity::from_seed(1234)];
            while let Ok(next) = ids.last().unwrap().reroll() {
                ids.push(next);
            }
            ids.iter().map(|id| id.stream_seed()).collect::<Vec<_>>()
        };
        let a = build();
        let b = build();
        assert_eq!(a, b);
        assert_eq!(a.len() as u32, MAX_ATTEMPTS);
        // Attempt a > 0 uses draw a-1 of the master reroll stream.
        for (attempt, &seed) in a.iter().enumerate().skip(1) {
            let manual = DetRng::new(1234, streams::REROLL.as_bytes()).nth_u64(attempt as u64 - 1);
            assert_eq!(seed, manual);
        }
    }

    #[test]
    fn reroll_bounded() {
        let mut id = RunIdentity::from_seed(5);
        for _ in 1..MAX_ATTEMPTS {
            id = id.reroll().unwrap();
        }
        assert_eq!(id.attempt, MAX_ATTEMPTS - 1);
        assert!(matches!(
            id.reroll(),
            Err(SeedError::AttemptsExhausted { attempt }) if attempt == MAX_ATTEMPTS - 1
        ));
    }

    /// Pins the reroll rule itself; bless only on an intentional
    /// RNG-contract change (pipeline_version bump).
    #[test]
    fn golden_reroll_sub_seeds() {
        const GOLDEN: [u64; 3] = [0x8757113c9d3a20ae, 0x57ab848989b5d383, 0x1a410f1654fe2588];
        let got: Vec<u64> = (1u32..=3)
            .map(|a| {
                RunIdentity {
                    seed: 1,
                    pipeline_version: PIPELINE_VERSION,
                    attempt: a,
                }
                .stream_seed()
            })
            .collect();
        assert_eq!(got, GOLDEN);
    }

    #[test]
    fn canonical_json_golden() {
        let id = RunIdentity::from_seed(1);
        assert_eq!(
            id.canonical_json(),
            r#"{"seed":1,"pipeline_version":1,"attempt":0}"#
        );
        let max = RunIdentity::from_seed(u64::MAX);
        assert_eq!(
            max.canonical_json(),
            format!(
                r#"{{"seed":{},"pipeline_version":1,"attempt":0}}"#,
                u64::MAX
            )
        );
        assert_eq!(RunIdentity::from_json(&max.canonical_json()).unwrap(), max);
    }

    #[test]
    fn json_roundtrip_bytes_stable() {
        let id = RunIdentity {
            seed: 0xDEAD_BEEF,
            pipeline_version: PIPELINE_VERSION,
            attempt: 3,
        };
        let bytes = id.canonical_json();
        let back = RunIdentity::from_json(&bytes).unwrap();
        assert_eq!(back, id);
        assert_eq!(back.canonical_json(), bytes);
        assert!(
            RunIdentity::from_json(r#"{"seed":1,"pipeline_version":1,"attempt":0,"x":1}"#).is_err()
        );
    }

    #[test]
    fn from_json_rejects_version_mismatch() {
        let err = RunIdentity::from_json(r#"{"seed":1,"pipeline_version":2,"attempt":0}"#);
        assert!(matches!(err, Err(SeedError::VersionMismatch { found: 2 })));
    }

    #[test]
    fn from_json_rejects_attempt_out_of_range() {
        let json = format!(r#"{{"seed":1,"pipeline_version":1,"attempt":{MAX_ATTEMPTS}}}"#);
        assert!(matches!(
            RunIdentity::from_json(&json),
            Err(SeedError::AttemptOutOfRange { attempt }) if attempt == MAX_ATTEMPTS
        ));
        // Last valid attempt still loads.
        let json = format!(
            r#"{{"seed":1,"pipeline_version":1,"attempt":{}}}"#,
            MAX_ATTEMPTS - 1
        );
        assert!(RunIdentity::from_json(&json).is_ok());
    }

    #[test]
    fn reroll_no_overflow_on_out_of_range_attempt() {
        let id = RunIdentity {
            seed: 1,
            pipeline_version: PIPELINE_VERSION,
            attempt: u32::MAX,
        };
        assert!(matches!(
            id.reroll(),
            Err(SeedError::AttemptsExhausted { attempt: u32::MAX })
        ));
    }

    #[test]
    #[should_panic(expected = "unregistered stream")]
    fn stream_rejects_unregistered() {
        RunIdentity::from_seed(1).stream("rogue/v1");
    }

    #[test]
    #[should_panic(expected = "internal to course-seed")]
    fn stream_rejects_reroll() {
        RunIdentity::from_seed(1).stream(streams::REROLL);
    }

    #[test]
    fn from_entropy_draws_distinct() {
        // One draw per call: two calls are two runs with different seeds.
        let a = RunIdentity::from_entropy().unwrap();
        let b = RunIdentity::from_entropy().unwrap();
        assert_ne!(a.seed, b.seed);
        assert_eq!(a.attempt, 0);
    }
}
