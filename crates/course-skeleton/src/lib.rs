//! Stage S2 — the skeleton kernel. The base six are all served by one fluvial
//! engine: trunk splines, tributary growth at the drainage-density spacing,
//! a flow-distance transform, derived divides, and catena-library elevation.
//! Five modules dial in biome identity on top of it.
//!
//! This stage *authors* structure. S4 erosion adds texture and never
//! restructures — see `docs/00-architecture.md` for why the pipeline makes
//! that trade. The [`kernel::SkeletonKernel`] trait is the future-biome seam:
//! kernels II-V plug in behind it without touching contract C1 or C2.
//!
//! Stage doc: `docs/stages/stage-02-skeleton-kernel.md`.

pub mod fluvial;
pub mod kernel;
pub mod modules;
