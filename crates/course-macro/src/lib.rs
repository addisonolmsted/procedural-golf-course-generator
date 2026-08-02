//! `course-macro` — step 03: macro landform. Turns a `CourseSpec` into the
//! `MacroSkeleton`: the base heightfield (before noise/erosion), the
//! conditioning fields downstream steps modulate on, and the structure graph
//! (drain / ridge / bench spines).
//!
//! Contract doc: `steps/03-macro-landform.md`. Randomness: only the
//! `macro/place/v1` stream, drawn exclusively inside [`planner`] in one
//! documented fixed order.
//!
//! Layering:
//! - [`config`] + [`prims`]: the analytic swept-primitive engine
//!   (`MacroConfig` → `resolve` → `height_at`), ported from
//!   `terrain-v2:golf-landform` — resolution-consistent by construction.
//! - [`plan`] + [`planner`]: params → [`plan::MacroPlan`] (the staged,
//!   viewer-inspectable IR) → `MacroConfig`.
//! - [`raster`] + [`fields`]: `Resolved` + plan → grids.
//! - [`skeleton`]: the output contract types.
//! - [`artifact`]: the on-disk `macro_skeleton/` directory.

pub mod artifact;
pub mod budget;
pub mod network;
pub mod config;
pub mod fields;
pub mod plan;
pub mod planner;
pub mod presets;
pub mod prims;
pub mod raster;
pub mod skeleton;

pub use config::{field_hash, generate, resolve, MacroConfig, Path, Resolved, Tilt};
pub use plan::MacroPlan;
pub use prims::bluff::Bluff;
pub use prims::bowl::{Bowl, BowlBoundary, Outlet};
pub use prims::meander::MeanderSpec;
pub use prims::valley::{Ridge, Valley, MIN_FALL_GRADIENT};
pub use skeleton::{MacroSkeleton, SkeletonFields, Spine, SpineKind, StructureGraph};

/// Serialized config schema version (bump on breaking schema change).
pub const SCHEMA_VERSION: u32 = 1;
/// Step-03 semantics version — folded into the artifact meta; bump on any
/// change that alters output bytes for a fixed seed (an explicit re-bless).
/// v2 (2026-07-27): campaign-pilot-1 prior; lower() re-anchors absolute
/// elevations (bowl rims, ridge crests, trunk floor/fall) to the effective
/// tilt; `BowlPlan.center` + `DrainagePlan.trunk_incision_m` added to
/// plan.json.
/// v3 (2026-07-28): campaign-pilot-2 prior (repaired valley-fall and
/// major-ridge estimators); ridge crests de-mesa'd — halfwidth now comes
/// from the measured `landform.ridge_crest_hw_m` knob and rounding scales
/// with it, instead of a length-derived 45 m halfwidth with fixed 12 m
/// rounding that flat-topped every ridge.
/// v4 (2026-08-01, M3): the core budget solve is core-localized. The old
/// solve enforced `core_relief_cap_m` by scaling every feature amplitude and
/// the regional tilt by `(cap/relief)²`, flattening the whole 3 km box to
/// satisfy a cap that governs only the middle 1.5 km — and overshooting
/// badly (mountain seed 1: 45 m cap, 60.6 m raw, 17.2 m result). [`budget`]
/// replaces it with staged bisections on `Tilt::core_tread`, `core_relax`
/// and a new `incision_scale`, all of which leave the outer ring alone. Four
/// defects fell out and are fixed here: `Bluff::apply` shadowed its core
/// weight so bench scarps ignored the solve entirely; `Tilt::tread_profile`
/// reversed slope near the knee at strong treads; the trunk's incision and
/// its through-core drop were budgeted separately against the same cap; and
/// tributary depth was budgeted not at all.
/// v5 (2026-08-02, M5 groundwork): `landform.drainage_density_fine` added to
/// the prior, and the core-relief lever on valleys changed from blending the
/// composed surface to scaling the WALL RISE — the only form that is safe
/// once a grown network puts drain floors inside the core.
pub const MACRO_VERSION: u32 = 5;

/// The one step-03 entry point: `CourseSpec` + resolution → skeleton (plus
/// the plan/config sidecars, returned for the lab and the artifact writer).
pub struct MacroResult {
    pub plan: plan::MacroPlan,
    pub config: MacroConfig,
    pub skeleton: MacroSkeleton,
}

pub fn generate_skeleton(spec: &course_spec::CourseSpec, res_m: f64) -> MacroResult {
    let plan = planner::plan(spec);
    let config = planner::lower(&plan);
    let resolved = resolve(&config);
    let skeleton = raster::rasterize(&resolved, &plan, res_m);
    MacroResult {
        plan,
        config,
        skeleton,
    }
}
