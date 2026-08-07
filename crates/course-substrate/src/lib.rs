//! Stage S5 — substrate assembly. Gathers everything upstream into contract
//! C2, the routing substrate: height + exclusion masks + cost fields +
//! metadata + scorer preset. This is the pipeline's waist — S6 onward see
//! only C2, never the stages that produced it.
//!
//! Stage doc: `docs/stages/stage-05-substrate-assembly.md`.
//! Output contract: `docs/contracts/C2-routing-substrate.md`.

pub mod assemble;
pub mod cost;
pub mod masks;
pub mod preset;
