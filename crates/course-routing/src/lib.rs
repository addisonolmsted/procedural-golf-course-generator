//! Stage S6 — routing. Beam search then annealing, both on fixed iteration
//! budgets so the stage cannot blow its time budget. Greens are placed first;
//! penalties saturate rather than veto, so the search always returns a route.
//! Plasticity governs feasibility strictness and the fit-versus-cost balance.
//!
//! Reads only contract C2; emits contract C3.
//!
//! Stage doc: `docs/stages/stage-06-routing.md`.

pub mod anneal;
pub mod beam;
pub mod greens;
pub mod penalty;
pub mod scorer;
