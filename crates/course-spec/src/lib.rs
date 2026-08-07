//! Stage S0 — biome selection and parameter sampling. Turns a
//! [`course_seed::RunIdentity`] into a [`CourseSpec`]: one archetype (drawn
//! from adjustable relative weights, or forced) plus every generation
//! parameter sampled from that archetype's prior quantile tables. After this
//! stage no downstream stage makes a biome decision — they only read
//! parameters (plus the one [`HydrologyMode`] switch).
//!
//! Stage doc: `docs/stages/stage-00-archetype-draw.md`. Cross-cutting
//! invariants: `ARCHITECTURE.md` — in particular "the biome is data, not
//! code": the knob set lives in `data/archetype_priors.json`, so adding a knob
//! (or re-fitting the prior) is a data edit + golden re-bless, never a schema
//! change here.
//!
//! # v2 status
//!
//! The crate is the right home for S0 but its contents are pre-v2: the five
//! archetypes and the fitted 88-knob prior are keyed to the retired archetype
//! set, and v2 samples from offline-certified envelopes rather than
//! independent quantile tables. Both are replaced when S0 is implemented —
//! see `MIGRATION.md`.

pub mod archetype;
pub mod prior;
pub mod spec;

pub use archetype::{ArchetypeId, HydrologyMode};
pub use prior::{PriorError, Priors, Quantiles};
pub use spec::{CourseSpec, SpecError, SpecOverrides};

/// Fixed 9-hole route (stage-00 hard requirement 2). Typed constants, not
/// prior data: par is an integer sequence, identical across archetypes, and
/// contract-fixed — encoding it as a sampled knob would let a prior edit
/// silently break the par-36 requirement.
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
