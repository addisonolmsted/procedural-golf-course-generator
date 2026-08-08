//! The shared v2 vocabulary: the four guarded contracts plus the scalars and
//! provenance every stage reads. This crate holds *only* the new vocabulary —
//! the deterministic RNG kit lives in `course-seed` and the grid/spline/noise
//! substrate lives in `course-world`; neither is restated here.
//!
//! Contract docs: `docs/contracts/C0-delivery.md` (the external one — the
//! bundle handed to the frontend team), `docs/contracts/C1-primitives-to-kernel.md`,
//! `docs/contracts/C2-routing-substrate.md`,
//! `docs/contracts/C3-corridor-realization.md`.
//! Cross-cutting invariants: `ARCHITECTURE.md`.
//!
//! Every contract is constructed through a validating constructor and loaded
//! with loud errors; the [`fixtures`] module builds minimal valid instances so
//! any stage can be tested before its upstream neighbours exist.

pub mod biome;
pub mod contracts;
pub mod error;
pub mod fixtures;
pub mod metadata;
pub mod plasticity;
pub mod units;

pub use error::ContractError;
pub use plasticity::Plasticity;

/// Fixed 9-hole route. Typed constants, not prior data: par is an integer
/// sequence, identical across biomes, and contract-fixed — encoding it as a
/// sampled knob would let a data edit silently break the par-36 requirement.
/// THE authority; `course-spec` re-exports these.
pub const HOLE_COUNT: u32 = 9;
/// Par per hole in play order; sums to 36.
pub const PAR_SEQUENCE: [u32; 9] = [4, 4, 3, 5, 4, 3, 4, 5, 4];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn par_sequence_sums_to_36() {
        assert_eq!(PAR_SEQUENCE.len() as u32, HOLE_COUNT);
        assert_eq!(PAR_SEQUENCE.iter().sum::<u32>(), 36);
    }
}
