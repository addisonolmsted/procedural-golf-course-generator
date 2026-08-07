//! **v1 stage 01 — retained as prior art, superseded by v2's S1 and S2.**
//!
//! Site framing: decides WHERE in an implied larger landscape the 3 km window
//! sits and what structural skeleton it inherits — window-position class, base
//! level (where water leaves), regional tilt, structural grain, provinces, and
//! the trunk/branch/ridge/step skeleton. The artifact (`framing.json`) is
//! small, pure JSON — no grids.
//!
//! # Status under v2
//!
//! Its contract doc (`stages/01-site-framing.md`) was retired with the v1 doc
//! tree. This crate is kept because its skeleton construction is direct prior
//! art for the fluvial engine — read `generate.rs` (the order-sensitive draw
//! transcript and the per-family construction rules) before implementing
//! `docs/stages/stage-02-skeleton-kernel.md`. Retired once S1 and S2 are
//! verified; see `MIGRATION.md`.
//!
//! The reroll behaviour described below is also v1: randomness comes from the
//! STABLE `framing/v1` stream so framing survived gate-fail rerolls
//! byte-identically. v2 never retries, so nothing rerolls — see
//! `docs/00-architecture.md`.

mod framing;
mod generate;

pub use framing::{
    BaseLevel, Boundary, BoundaryKind, EdgeId, Framing, FramingError, Grain, Provinces, Skeleton,
    Tilt, Trunk, WindowClass, FRAMING_VERSION, MAX_BRANCHES, MAX_RIDGES, MAX_STEPS,
    STEP_AXIS_TOL_RAD,
};
pub use generate::{generate, TILT_TOL_RAD};
