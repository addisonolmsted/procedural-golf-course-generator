//! `golf-core`: shared foundation for the procedural golf course generator.
//! Pure library — no I/O, no rendering. Deliberately small; this is the bones.
//!
//! - [`math`]: deterministic f64 vector + libm-backed transcendentals
//! - [`det`]: seeded, domain-split RNG (the determinism kit)
//! - [`grid`]: `Grid<T>` / `GridSpec` with explicit resolution
//! - [`spline`]: spine curve + width/falloff profiles (corridor primitives)
//!
//! Everything downstream (noise, slope, erosion, routing, …) builds on these.

pub mod det;
pub mod grid;
pub mod math;
pub mod spline;

pub use grid::{Grid, GridSpec};
pub use math::Vec2;
