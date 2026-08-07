//! Stage S7 — earthmoving realization. Grades the corridor graph into the
//! surface, proportional to plasticity, sourcing fill from lakes-as-borrow so
//! cut and fill balance.
//!
//! The drainage repair is deliberately bounded: **single-pass
//! drainage-correct templates, then one Priority-Flood verify, then one
//! bounded breach-repair**. No unbounded loops, no iterate-until-clean — a
//! stage that can run long is a stage that can miss the frame budget.
//!
//! Stage doc: `docs/stages/stage-07-earthmoving.md`.
//! Input contract: `docs/contracts/C3-corridor-realization.md`.

pub mod borrow;
pub mod grading;
pub mod mass_balance;
pub mod repair;
pub mod templates;
pub mod verify;
