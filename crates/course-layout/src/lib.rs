//! Stage S8 — hole layout. The ten-pass system, split between plan space
//! (where things go, in corridor coordinates) and surface space (what the
//! ground actually does). Spines, width profiles, zones, greens, hazards,
//! the feature budget, and the dispersion validation that keeps every hole
//! from converging on the same shape.
//!
//! Stage doc: `docs/stages/stage-08-hole-layout.md`.

pub mod dispersion;
pub mod greens;
pub mod hazards;
pub mod passes;
pub mod spine;
pub mod width;
pub mod zones;
