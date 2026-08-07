//! The shared v2 vocabulary: the three guarded contracts plus the scalars and
//! provenance every stage reads. This crate holds *only* the new vocabulary —
//! the deterministic RNG kit lives in `course-seed` and the grid/spline/noise
//! substrate lives in `course-world`; neither is restated here.
//!
//! Contract docs: `docs/contracts/C1-primitives-to-kernel.md`,
//! `docs/contracts/C2-routing-substrate.md`,
//! `docs/contracts/C3-corridor-realization.md`.
//! Cross-cutting invariants: `ARCHITECTURE.md`.

pub mod biome;
pub mod contracts;
pub mod metadata;
pub mod plasticity;
pub mod units;
