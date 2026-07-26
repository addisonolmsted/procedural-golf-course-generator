//! `course-contracts`: the shared contract layer of the archetype pipeline.
//!
//! Every pipeline stage (course-macro, course-noise, course-hydro, …) depends
//! on this crate and ONLY this crate for its input/output types, so stages can
//! be built and tested in parallel against the [`fixtures`] before their
//! upstream neighbors exist. The full contract spec lives in
//! `docs/archetype-pipeline/ARCHITECTURE.md`.
//!
//! - [`archetype`]: the five named archetypes + fixed switches
//! - [`spec`]: `CourseSpec` / `ParamPack` (stage 1–2 output; read by everyone)
//! - [`stages`]: artifact types for stages 3–10
//! - [`artifact`]: save/load + manifest + content hashing
//! - [`gridio`]: the `CGRID1` binary sidecar format
//! - [`rng`]: the DetRng channel registry
//! - [`fixtures`]: cheap deterministic stand-ins for every stage output

pub mod archetype;
pub mod artifact;
pub mod fixtures;
pub mod gridio;
pub mod rng;
pub mod spec;
pub mod stages;

pub use archetype::{ArchetypeId, HydrologyMode};
pub use artifact::{Artifact, Manifest};
pub use spec::{ArchetypePriors, CourseSpec, ParamPack};

/// Bump when any stage contract changes shape (folded into every manifest).
pub const PIPELINE_VERSION: u32 = 1;

/// Generated world: 3 km × 3 km, origin at the SW corner.
pub const EXTENT_M: f64 = 3000.0;
/// Routable core: the central 1.5 km × 1.5 km window `[CORE_MIN_M, CORE_MAX_M]²`.
pub const CORE_MIN_M: f64 = 750.0;
pub const CORE_MAX_M: f64 = 2250.0;

/// Canonical working resolution for stages 3–8 grids.
pub const RES_FULL_M: f64 = 2.0;
/// GUI / fast-iteration preview resolution (same code path, coarser grid).
pub const RES_PREVIEW_M: f64 = 8.0;
/// Stage-9 earthworks patches are local grids at this resolution.
pub const RES_EARTHWORKS_M: f64 = 0.5;

/// Grid spec covering the full extent at `res_m` (node convention: nx = n+1).
pub fn world_spec(res_m: f64) -> golf_core::GridSpec {
    let n = (EXTENT_M / res_m).round() as u32;
    golf_core::GridSpec::new(golf_core::Vec2 { x: 0.0, y: 0.0 }, res_m, n + 1, n + 1)
}

/// True if the world-space point is inside the routable core window.
pub fn in_core(p: golf_core::Vec2) -> bool {
    p.x >= CORE_MIN_M && p.x <= CORE_MAX_M && p.y >= CORE_MIN_M && p.y <= CORE_MAX_M
}

/// FNV-1a over f64 bit patterns — the workspace's golden-hash convention.
pub fn fnv_f64(data: &[f64]) -> u64 {
    let mut h = 1469598103934665603u64;
    for z in data {
        h ^= z.to_bits();
        h = h.wrapping_mul(1099511628211);
    }
    h
}

/// FNV-1a over raw bytes.
pub fn fnv_bytes(data: &[u8]) -> u64 {
    let mut h = 1469598103934665603u64;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}
