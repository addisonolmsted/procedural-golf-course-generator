//! The determinism bedrock of the course pipeline: one master `u64` seed, the
//! [`RunIdentity`] artifact, domain-split RNG streams
//! ([`RunIdentity::stream`] + the [`streams`] registry), and the deterministic
//! gate-fail reroll rule.
//!
//! Stage-agnostic and reused unchanged by the v2 pipeline. Conventions:
//! `docs/01-conventions.md`. Cross-cutting invariants: `ARCHITECTURE.md`.
//!
//! # The reroll rule is vestigial under v2
//!
//! [`MAX_ATTEMPTS`] and the `stable`/`attempt` stream scoping exist to support
//! gate-fail rerolls. The v2 pipeline samples only from offline-certified
//! envelopes and **never rejects or retries at runtime**
//! (`docs/calibration/envelope-certification.md`), so no v2 stage raises an
//! attempt. The machinery is retained deliberately: it costs nothing while
//! attempt is pinned to 0, and removing it would rekey every stream.
//!
//! # Platform stability (by construction)
//!
//! blake3 and ChaCha8 are pure integer code; the only float operation in
//! this crate is `(u64 >> 11) as f64 * 2^-53`, where both the conversion
//! (a 53-bit value) and the power-of-two multiply are IEEE-exact on every
//! platform. No `HashMap`, no wall clock, no platform intrinsics.
//!
//! # No feature flags
//!
//! Deliberate: optionality here would let downstream crates compile against
//! subtly different RNG behavior.

pub mod det;
mod identity;
pub mod streams;

pub use det::DetRng;
pub use identity::{RunIdentity, SeedError};

/// Global contract version. Any artifact-shape or RNG-contract change bumps
/// this (and re-blesses goldens) as an explicit reviewed event.
///
/// v2 (2026-08-02): registry v2 — two-tier stream scoping (stable streams
/// key off the master seed and survive rerolls) + the stage-era stream
/// renames. Attempt-0 draws are unchanged; attempt>0 stage-0/1 draws rekeyed.
///
/// v3 (2026-08-02): stage-01 artifact gains the structural skeleton
/// (`FRAMING_VERSION` 2). Header-only here: streams key on seed + name, so
/// this bump moves no draws.
///
/// v4 (2026-08-07): registry v3 — the v2-pipeline rekey. New stage streams
/// registered (primitives, skeleton/*, amplify, hydro, substrate, layout,
/// zoning, validate); `mask/strokes/forcing` retired; every stream `Stable`
/// (v2 never retries). Retained names keep their spellings, so attempt-0
/// draws are bit-identical; artifact headers change version only.
pub const PIPELINE_VERSION: u32 = 4;

/// Bound on gate-fail rerolls: attempts 0..MAX_ATTEMPTS, then the run fails
/// for good. Vestigial under v2 — see the crate docs; no v2 stage raises an
/// attempt.
pub const MAX_ATTEMPTS: u32 = 8;
