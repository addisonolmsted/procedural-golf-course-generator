//! `MacroPlan` — the staged intermediate representation the planner emits and
//! the macro-lab viewer renders. Every construction stage is inspectable
//! data: frame → drainage → uplands → basins → core budget. The plan is
//! lowered to a [`crate::MacroConfig`] (see [`crate::planner::lower`]) and
//! serialized as the `plan.json` advisory sidecar of the artifact.
//!
//! Downstream steps MUST NOT read the plan — the contract is the rasterized
//! skeleton + structure graph (`steps/03-macro-landform.md`).

use course_world::math::Vec2;
use serde::{Deserialize, Serialize};

use crate::prims::bluff::Bluff;
use crate::prims::bowl::Bowl;
use crate::prims::valley::{Ridge, Valley};

/// serde default for the budget solve's knobs: 1 = the plan as authored, so
/// a `plan.json` written before the knobs existed round-trips unchanged.
fn d_one() -> f64 {
    1.0
}

/// A box edge (outward normal names it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Edge {
    West,
    East,
    South,
    North,
}

impl Edge {
    pub fn outward(self) -> Vec2 {
        match self {
            Edge::West => Vec2::new(-1.0, 0.0),
            Edge::East => Vec2::new(1.0, 0.0),
            Edge::South => Vec2::new(0.0, -1.0),
            Edge::North => Vec2::new(0.0, 1.0),
        }
    }
}

/// Stage 1 — the regional frame: structural grain, tilt, and where drainage
/// leaves the box.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub base_elev_m: f64,
    /// Structural grain azimuth (radians, mod π; ridges/dune trains align).
    pub grain_az_rad: f64,
    /// Unit downhill direction of the regional tilt.
    pub downhill: Vec2,
    /// Authored regional grade (the `landform.tilt_grade` knob).
    pub tilt_grade: f64,
    /// Effective grade after the bench cascade absorbed excess core relief
    /// (== `tilt_grade` when no benches). The budget solve no longer touches
    /// it: scaling the regional grade destroys `relief_amp_m` — a FITTED
    /// quantity — to satisfy `core_relief_cap_m`, a design one. The solve
    /// turns `core_tread` instead, which redistributes the same drop.
    pub tilt_grade_eff: f64,
    /// Fraction of the regional grade kept ACROSS the core; the rest moves
    /// to the outer bands, conserving the end-to-end drop. Set by the budget
    /// solve (1 = as authored). See [`crate::budget`].
    #[serde(default = "d_one")]
    pub core_tread: f64,
    /// Feature-relief weight at the middle of the core, eased to 1 by the
    /// core shoulder. Set by the budget solve (1 = as authored), and only
    /// when `core_tread` alone could not reach the cap.
    #[serde(default = "d_one")]
    pub core_relax: f64,
    /// Low-order dome/basin curvature (metres, corners → center).
    pub curve_m: f64,
    /// The most-downhill box edge; `None` iff `closed_basin_only`.
    pub exit_edge: Option<Edge>,
    /// Derived mode (NOT an archetype branch): no drainage spines were drawn
    /// and closed basins exist, so internal drainage is legal.
    pub closed_basin_only: bool,
    /// Dune-train wavelength (0 = none); carried for the viewer — step 04
    /// reads the knob itself for the carrier waves.
    pub dune_wavelength_m: f64,
}

/// Stage 2 — the drainage skeleton. `valleys[0]` is the trunk (when any);
/// tributaries carry `join_trunk = Some(0)` and are accordance-snapped at
/// resolve time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct DrainagePlan {
    pub valleys: Vec<Valley>,
    /// True when the trunk was routed OUTSIDE the core: its longitudinal
    /// drop across a core crossing (fall × 1.5 km) would have blown the
    /// relief cap, so the creek skirts the core ring instead (holes play
    /// out to it; the core itself stays cap-compliant).
    pub tangent_routed: bool,
    /// Trunk incision below the local (effective-tilt) surface at its entry
    /// (0 when no trunk). Stored so lowering can re-anchor `floor_z0_m` to
    /// the EFFECTIVE tilt: the budget solve scales the tilt, and every
    /// absolute elevation authored against it must track that scaling.
    pub trunk_incision_m: f64,
    /// True when NO drain centerline anywhere feels the core weight — the
    /// exact precondition for [`crate::MacroConfig::core_relax_valleys`],
    /// which lets the budget solve ease valley WALLS out of the core while
    /// leaving every floor at full carve.
    ///
    /// Tested rather than inferred from `tangent_routed`. The core-protect
    /// shoulder reaches 1273 m from the box centre at its corners while a
    /// tangent trunk is placed 1150-1350 m out, so a tangent chord can still
    /// clip it — and where it does, easing the carve along the floor lifts
    /// that reach relative to its neighbours and ponds the drain (measured:
    /// piedmont seed 7 rose 2.53 m at u=0.42 under the proxy).
    #[serde(default)]
    pub floors_clear_of_core: bool,
    /// Uniform multiplier on `trunk_incision_m`, set by the budget solve as
    /// its last lever (1 = as authored).
    ///
    /// Unlike the core weight this is NOT core-localized — it shallows the
    /// creek along its whole length. It is still the right last resort:
    /// it touches one feature instead of the whole box, and it is the only
    /// lever that reaches a drain the core weight may not touch. And it is
    /// provably safe for drainage, which the core weight is not: scaling the
    /// incision shifts the entire floor by one constant, so every
    /// longitudinal slope is unchanged and a monotone drain stays monotone.
    #[serde(default = "d_one")]
    pub incision_scale: f64,
    /// True when this plan's drainage was GROWN as a network rather than
    /// placed as a lone creek. Selects the network shaping, the hillslope
    /// lift, and whether `lower()` applies the bench-cascade fall rescale.
    #[serde(default)]
    pub is_network: bool,
}

/// Normalized-arc spans of a spine that cross the routable core, plus the
/// height multiplier applied there ("steep features cross the core gently").
///
/// This is a PLACEMENT allocation, not a solve artifact: `dip` is a share of
/// the cap (half for a ridge, 0.3 for a scarp) decided when the feature is
/// drawn, and it is core-localized by construction — the spans stop at the
/// core footprint, so the feature stays full size everywhere else. The
/// budget solve no longer touches it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CoreDip {
    pub spans: Vec<(f64, f64)>,
    /// Height multiplier inside the spans (1 = no dip).
    pub dip: f64,
}

/// Stage 3a — a placed ridge.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RidgePlan {
    /// Ridge with a BASE emphasis profile (end tapers only); the core dip is
    /// applied at lowering time.
    pub ridge: Ridge,
    pub prominence_m: f64,
    pub core: CoreDip,
}

/// Stage 3b — one scarp of the bench cascade.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScarpPlan {
    /// Bluff with a constant-1 height profile; core dip applied at lowering.
    pub bluff: Bluff,
    pub core: CoreDip,
}

/// Stage 3 — uplands: ridges + the bench cascade (scarps stepping the
/// regional drop down in flat treads; the through-core tread is the
/// mountain archetype's routability lever).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct UplandsPlan {
    pub ridges: Vec<RidgePlan>,
    pub scarps: Vec<ScarpPlan>,
    /// Tread width kept clear of scarps through the core center (m).
    pub core_tread_w_m: f64,
}

/// Stage 4 — a placed closed basin.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BowlPlan {
    pub bowl: Bowl,
    /// Placement center — lowering re-anchors `rim_z_m` to the effective
    /// tilt here (the authored `bowl.rim_z_m` reflects the authored tilt).
    pub center: Vec2,
    /// True if the center lies in the core shoulder (depth is budgeted).
    pub in_core: bool,
    /// Depth multiplier from the budget solve (1 = untouched).
    pub depth_scale: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct BasinPlan {
    pub bowls: Vec<BowlPlan>,
}

/// Stage 5 — the core-relief budget record: what the cap was, what the
/// preview measured, and what the solve did about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct CoreBudget {
    pub cap_m: f64,
    /// Core relief of the raw plan (fixed 16 m preview), before the solve.
    pub relief_raw_m: f64,
    /// Core relief after the solve.
    pub relief_final_m: f64,
    /// Human-readable record of what the solve turned and what it bought.
    pub notes: Vec<String>,
}

/// The full staged plan. Serialized as `plan.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroPlan {
    pub frame: Frame,
    pub drainage: DrainagePlan,
    pub uplands: UplandsPlan,
    pub basins: BasinPlan,
    pub budget: CoreBudget,
}
