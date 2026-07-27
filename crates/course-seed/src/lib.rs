//! Step 01 — Seed. The determinism bedrock of the course pipeline:
//! one master `u64` seed, the [`RunIdentity`] artifact, domain-split RNG
//! streams ([`RunIdentity::stream`] + the [`streams`] registry), and the
//! deterministic gate-fail reroll rule.
//!
//! Contract doc: `steps/01-seed.md`. Cross-cutting invariants:
//! `ARCHITECTURE.md`.
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
pub const PIPELINE_VERSION: u32 = 1;

/// Bound on gate-fail rerolls (step 07 triggers, step 01 owns the rule):
/// attempts 0..MAX_ATTEMPTS, then the run fails for good.
pub const MAX_ATTEMPTS: u32 = 8;
