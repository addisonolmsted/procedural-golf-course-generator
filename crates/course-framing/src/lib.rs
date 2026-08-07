//! Stage 01 — Site Framing. Decides WHERE in an implied larger landscape the
//! 3 km window sits and what structural skeleton it inherits: window-position
//! class, base level (where water leaves), regional tilt, structural grain,
//! and provinces. A few categorical + vector draws that are the biggest
//! single source of between-seed macro variety.
//!
//! Contract doc: `stages/01-site-framing.md`. The artifact (`framing.json`)
//! is small, pure JSON — no grids. All randomness comes from the STABLE
//! `framing/v1` stream, so framing survives gate-fail rerolls byte-identically
//! (stages 2–3 resample; archetype, θ, and framing do not).

mod framing;
mod generate;

pub use framing::{
    BaseLevel, Boundary, BoundaryKind, EdgeId, Framing, FramingError, Grain, Provinces, Skeleton,
    Tilt, Trunk, WindowClass, FRAMING_VERSION, MAX_BRANCHES, MAX_RIDGES, MAX_STEPS,
    STEP_AXIS_TOL_RAD,
};
pub use generate::{generate, TILT_TOL_RAD};
