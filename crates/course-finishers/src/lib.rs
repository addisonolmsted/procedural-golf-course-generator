//! Stage S4 — simulation finishers. Texture-only erosion passes at per-biome
//! intensity. **A finisher never restructures**: it may not move a divide,
//! open a new channel, or change the drainage network S2 authored. That
//! boundary is the contract, and `validate` enforces it.
//!
//! Dev builds carry metric instrumentation; runtime builds have no gate —
//! parameters were certified offline (`docs/calibration/envelope-certification.md`).
//!
//! Stage doc: `docs/stages/stage-04-finishers.md`.

pub mod erosion;
pub mod instrument;
pub mod validate;
