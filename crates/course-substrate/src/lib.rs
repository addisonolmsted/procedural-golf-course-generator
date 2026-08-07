//! Stage S5 — site selection and substrate assembly. Chooses where on the
//! generated land the course actually goes, then gathers everything upstream
//! into contract C2, the routing substrate: height + exclusion masks + cost
//! fields + metadata + scorer preset.
//!
//! [`siting`] is the step that makes this more than bookkeeping. Real course
//! siting is an architect picking the good 150 acres out of 500; until now the
//! routable area was nailed to the whole core and routing took whatever
//! terrain it was handed. S5 scores the core and selects a 600 m play window
//! — translation only, no rotation — which improves the golf and is a large
//! source of between-seed variety at the same time.
//!
//! This is the pipeline's waist: S6 onward see only C2, never the stages that
//! produced it.
//!
//! Stage doc: `docs/stages/stage-05-siting-substrate.md`.
//! Output contract: `docs/contracts/C2-routing-substrate.md`.

pub mod assemble;
pub mod cost;
pub mod masks;
pub mod preset;
pub mod siting;
