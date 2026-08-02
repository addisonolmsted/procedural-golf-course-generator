//! `course-world`: the shared geometry/grid/IO substrate for pipeline steps
//! 03+. Pure library — no RNG (that is `course-seed`), no step logic.
//!
//! Ported from the prior-art branches (`terrain-v2:golf-core`,
//! `terrain-v2:golf-landform` geometry modules, and
//! `archetype-pipeline:course-contracts` world constants + CGRID1), so the
//! math is the proven, deterministic version:
//!
//! - [`math`]: `Vec2` + libm-backed transcendentals (bit-stable cross-platform)
//! - [`ease`]: C1 blending helpers (smin/smax/ramp/eased_step/end_taper)
//! - [`grid`]: `Grid<T>` / `GridSpec` with explicit resolution
//! - [`profile`]: clamped piecewise-linear knot profiles
//! - [`spline`]: Catmull-Rom densify + arc-length `Spine` with true-distance
//!   projection (the swept-primitive substrate)
//! - [`noise`]: deterministic integer-hash gradient noise
//! - [`gridio`]: the `CGRID1` binary grid sidecar format
//! - [`world`]: the fixed 3 km world box + routable core + resolutions

pub mod ease;
pub mod grid;
pub mod gridio;
pub mod math;
pub mod noise;
pub mod profile;
pub mod spline;
pub mod world;

pub use grid::{Grid, GridSpec};
pub use math::Vec2;
pub use profile::Profile;
pub use spline::{Spine, SpineHit};
pub use world::{
    in_core, world_spec, CORE_MAX_M, CORE_MIN_M, EXTENT_M, RES_EARTHWORKS_M, RES_FULL_M,
    RES_PREVIEW_M,
};
