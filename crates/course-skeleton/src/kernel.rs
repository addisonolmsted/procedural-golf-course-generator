//! The `Skeleton` artifact and the kernel seam.
//!
//! `SkeletonKernel` is *the* future-biome seam (stage-02 doc): kernels II–V
//! (glacial, aeolian-dominant, karst, volcanic) implement the same trait,
//! consume the same C1, and produce the same `Skeleton`. Kernel I (fluvial)
//! serves all six base biomes; a kernel that grows no channels is a natural
//! outcome of the math (sandhills), not a special case.

use course_contracts::contracts::primitive_field::PrimitiveField;
use course_contracts::metadata::StructureMeta;
use course_seed::RunIdentity;
use course_spec::v2::SiteSpec;
use course_world::grid::Grid;
use course_world::math::Vec2;
use serde::{Deserialize, Serialize};

pub const SKELETON_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KernelId {
    /// The fluvial engine — all six base biomes.
    Fluvial,
}

/// One channel: an ordered polyline (upstream → downstream, world metres,
/// y-up) with its Strahler order and parent linkage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Channel {
    pub pts: Vec<Vec2>,
    /// Strahler order, by top-down construction.
    pub order: u8,
    /// Index of the channel this one joins; `None` = trunk.
    pub parent: Option<u32>,
    /// Arc position (m from the parent's upstream end) of the junction.
    pub junction_arc_m: f64,
    /// Drained area at the reach's downstream end (m²) — DISCHARGE, which
    /// is what makes a river a river. Strahler order counts branching, so
    /// a through-going trunk fed from outside the tile can carry 5 km² and
    /// still be order 2; width, rendering and floodplain must key off this
    /// instead (review: "the river valley trunk doesn't read as a trunk").
    #[serde(default)]
    pub area_m2: f64,
}

/// A closed-basin embryo — recorded because S3's polish and S7's repair both
/// need to know which pits are intended (stage-02, closed-basin module).
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BasinEmbryo {
    pub center: Vec2,
    pub radius_m: f64,
    pub depth_m: f64,
}

/// Achieved-vs-target diagnostics (stage-02 open question 5: the envelope
/// can only be corrected if S2 reports what it actually built).
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct SkeletonDiagnostics {
    pub target_density_km_km2: f64,
    pub achieved_density_km_km2: f64,
    pub channel_count: usize,
    pub bifurcation_ratio: Option<f64>,
    pub length_ratio: Option<f64>,
    /// Fraction of channel length in the connected component that reaches
    /// base level — 1.0 for integrated networks, low for heathland.
    pub connectivity: f64,
}

/// The S2 artifact (stage-02 doc, Inputs/Outputs).
pub struct Skeleton {
    pub skeleton_version: u32,
    pub kernel: KernelId,
    /// The base surface, 2 m — bilinear presentation of the 8 m truth
    /// (S2 owns nothing below 64 m, so 8 m Nyquist carries every band).
    pub height: Grid<f64>,
    /// DIRECTION [0,2π) toward the receiver, 8 m; NaN at outlets/pits.
    pub flow_dir_rad: Grid<f64>,
    /// Drained area, m², 8 m.
    pub flow_accum: Grid<f64>,
    /// Metres to the nearest channel cell, 8 m. With no channels: metres
    /// to the base-level edge (the only drainage datum that exists).
    pub flow_distance: Grid<f64>,
    /// [0,1] channel → divide, 8 m. 1.0 everywhere when no channels.
    pub flow_distance_norm: Grid<f64>,
    /// [0,1] relative elevation above the nearest channel base, 8 m.
    pub hillslope_position: Grid<f64>,
    pub channels: Vec<Channel>,
    /// Constructed trunk PROPOSALS, empty unless `skeleton.network_mode`
    /// is on. These are never the network — the network is `channels`,
    /// read off the built surface as always. They are kept so an
    /// instrument can ask whether the surface ended up carrying the trunk
    /// that was asked for, which is the acceptance test for the whole
    /// constructive path.
    pub constructed: Vec<Vec<Vec2>>,
    pub embryos: Vec<BasinEmbryo>,
    /// Derived from the flow field — never authored.
    pub divides: Vec<Vec<Vec2>>,
    /// Bit-identical to C1's meta (asserted by tests).
    /// 8 m linear index of the trunk INLET border cell (None when the
    /// trunk dial is 0). S4's river walks the receiver chain from here
    /// to the outlet — edge-to-edge by construction.
    pub trunk_inlet: Option<usize>,
    /// DIAGNOSTIC ONLY, 8 m: the tier-2 dendritic side-valley cells (1 =
    /// carved side-valley). The tier is purely morphological — it is not
    /// in `channels`/`flow_distance` and nothing downstream of S2 reads
    /// this field; probes and renders use it to draw the carved network.
    pub tier2: Option<Grid<u8>>,
    pub meta: StructureMeta,
    pub diagnostics: SkeletonDiagnostics,
}

/// The future-biome seam.
pub trait SkeletonKernel {
    fn id(&self) -> KernelId;
    fn generate(&self, spec: &SiteSpec, c1: &PrimitiveField, identity: &RunIdentity) -> Skeleton;
}
