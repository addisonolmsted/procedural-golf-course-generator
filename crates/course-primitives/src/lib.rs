//! Stage S1 — macro primitives. Lays down the tilt/ridge/valley/bench/plateau
//! priors and the structural metadata that orients everything downstream, then
//! emits contract C1 for the skeleton kernel to consume.
//!
//! Stage doc: `docs/stages/stage-01-macro-primitives.md`.
//! Output contract: `docs/contracts/C1-primitives-to-kernel.md`.

pub mod bench;
pub mod plateau;
pub mod ridge;
pub mod structure;
pub mod tilt;
pub mod valley;
