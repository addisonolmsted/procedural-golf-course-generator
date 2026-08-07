//! Stage S11 — validation. Playability simulation, sightlines checked against
//! the S10 enclosure model, the difficulty distribution across the eighteen,
//! and the per-stage performance budgets.
//!
//! Validation **reports**; it does not reject. Runtime never retries — a
//! course that scores badly here is a calibration bug to fix offline, not a
//! seed to throw away. See `docs/calibration/envelope-certification.md`.
//!
//! Stage doc: `docs/stages/stage-11-validation.md`.

pub mod difficulty;
pub mod golden;
pub mod perf;
pub mod playability;
pub mod sightlines;
