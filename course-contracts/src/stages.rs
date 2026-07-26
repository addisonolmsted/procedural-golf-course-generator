//! Artifact types for stages 3–10. These are THE contracts: a stage crate may
//! add private machinery freely, but what crosses a stage boundary is exactly
//! what is defined here. Grids ride as CGRID1 sidecars (see [`crate::artifact`]);
//! everything else is serde JSON.

use std::collections::BTreeMap;

use golf_core::{Grid, Vec2};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Stage 3 — MacroSkeleton
// ---------------------------------------------------------------------------

/// Per-cell conditioning fields downstream stages modulate on. All on the same
/// spec as `base_height`, all dimensionless in [0,1] except where noted.
#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonFields {
    /// Noise suppression near drainage floors (1 = fully damped). The no-dam
    /// guard: stage 4 MUST scale noise amplitude down by this.
    pub floor_damp: Grid<f64>,
    /// Extra noise amplitude on structurally steep ground.
    pub slope_gain: Grid<f64>,
    /// Local structural grain direction (radians, 0 = +x, undirected mod π).
    pub grain_dir_rad: Grid<f64>,
    /// Distance (m) to the nearest drainage spine.
    pub valley_dist_m: Grid<f64>,
    /// 1 inside the routable core fading to 0 outside — stages may use it to
    /// bias feature intensity away from the core.
    pub core_protect: Grid<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpineKind {
    /// Ordered downstream; `base_height` along it MUST fall monotonically.
    Drain,
    RidgeLine,
    BenchEdge,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Spine {
    pub kind: SpineKind,
    pub pts: Vec<Vec2>,
}

/// The macro structure the terrain is organized around. Stage 5 conditions
/// erosion on drain spines; stage 7/8 read it for corridor hints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct StructureGraph {
    pub spines: Vec<Spine>,
}

/// Stage 3 output.
#[derive(Clone, Debug, PartialEq)]
pub struct MacroSkeleton {
    pub base_height: Grid<f64>,
    pub fields: SkeletonFields,
    pub structure: StructureGraph,
}

// ---------------------------------------------------------------------------
// Stage 4 — ComposedHeight
// ---------------------------------------------------------------------------

/// Stage 4 output: skeleton + conditioned noise, same spec as the skeleton.
#[derive(Clone, Debug, PartialEq)]
pub struct ComposedHeight {
    pub height: Grid<f64>,
}

// ---------------------------------------------------------------------------
// Stage 5 — HydroBundle
// ---------------------------------------------------------------------------

/// The canonical D8 flow tree over the eroded height. `rcv[i] == i` marks an
/// outlet (boundary) or an intentionally retained pit (kettle/lake floor).
#[derive(Clone, Debug, PartialEq)]
pub struct FlowModel {
    /// Steepest-descent receiver index per cell.
    pub rcv: Vec<u32>,
    /// Cell indices in stable top-down processing order (uphill before
    /// downhill; ties broken by index).
    pub order: Vec<u32>,
    /// Depression-handled elevation used for routing.
    pub zfill: Grid<f64>,
    /// EFFECTIVE drained area (m²) per cell — already scaled by runoff
    /// (rainfall × (1 − infiltration)), so sandhills accumulate ~nothing.
    pub flow_area_m2: Grid<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stream {
    /// Ordered downstream, world coords.
    pub pts: Vec<Vec2>,
    pub strahler: u32,
    pub width_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Lake {
    /// Closed outline, world coords, CCW.
    pub outline: Vec<Vec2>,
    pub surface_elev_m: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Wetland {
    pub outline: Vec<Vec2>,
}

/// Vector hydrography derived from the flow tree. EVERY collection may be
/// empty — an infiltrated sandhills course has no surface water at all, and
/// all consumers must be correct in that case.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct HydroGraph {
    pub streams: Vec<Stream>,
    pub lakes: Vec<Lake>,
    pub wetlands: Vec<Wetland>,
}

/// Stage 5 output.
#[derive(Clone, Debug, PartialEq)]
pub struct HydroBundle {
    /// Post-erosion terrain (replaces stage 4's height downstream).
    pub height: Grid<f64>,
    pub flow: FlowModel,
    pub graph: HydroGraph,
}

// ---------------------------------------------------------------------------
// Stage 6 — CoverMap
// ---------------------------------------------------------------------------

/// Cell cover classes. Stored in a `Grid<u8>` via `as u8` / [`CoverClass::from_u8`].
/// The (class, archetype) pair is the art layer's lookup key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
#[serde(rename_all = "snake_case")]
pub enum CoverClass {
    Turf = 0,
    Rough = 1,
    Sand = 2,
    Wetland = 3,
    Water = 4,
    Rock = 5,
    Forest = 6,
}

impl CoverClass {
    pub const COUNT: usize = 7;

    pub fn from_u8(v: u8) -> Option<CoverClass> {
        use CoverClass::*;
        [Turf, Rough, Sand, Wetland, Water, Rock, Forest].into_iter().find(|c| *c as u8 == v)
    }

    /// Can a hole corridor pass over this class without earthworks?
    pub fn routable(self) -> bool {
        matches!(self, CoverClass::Turf | CoverClass::Rough | CoverClass::Sand)
    }
}

/// Stage 6 output. `class` holds `CoverClass as u8`; `canopy` is tree-cover
/// density 0..1 (independent of class: forest floor vs scattered specimens).
#[derive(Clone, Debug, PartialEq)]
pub struct CoverMap {
    pub class: Grid<u8>,
    pub canopy: Grid<f64>,
}

// ---------------------------------------------------------------------------
// Stage 7 — GateReport
// ---------------------------------------------------------------------------

/// Stage 7 output: pure analysis of the core window, no terrain mutation.
/// `metrics` keys are the gate's published metric names (see ARCHITECTURE.md);
/// `reasons` is non-empty iff `pass` is false.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GateReport {
    pub pass: bool,
    pub metrics: BTreeMap<String, f64>,
    pub reasons: Vec<String>,
}

// ---------------------------------------------------------------------------
// Stage 8 — RoutingPlan
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct HolePlan {
    pub par: u32,
    pub tee: Vec2,
    /// Turn point for doglegs (par 4/5); None = straight.
    pub dogleg: Option<Vec2>,
    pub green: Vec2,
    /// Closed corridor outline, world coords, CCW.
    pub corridor: Vec<Vec2>,
}

/// Stage 8 output: 9 holes in play order (par sum 36), returning nine —
/// hole 1 tees off near the clubhouse and hole 9 greens back to it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RoutingPlan {
    pub clubhouse: Vec2,
    pub holes: Vec<HolePlan>,
}

// ---------------------------------------------------------------------------
// Stage 9 — EarthworksDelta
// ---------------------------------------------------------------------------

/// A local height edit: `delta_m` on its own fine grid (RES_EARTHWORKS_M),
/// added onto the base terrain. Edges MUST feather to 0.
#[derive(Clone, Debug, PartialEq)]
pub struct HeightPatch {
    pub delta_m: Grid<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeatureKind {
    TeePad,
    GreenComplex,
    Bunker,
    Swale,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EarthFeature {
    pub kind: FeatureKind,
    /// 1-based hole number this feature belongs to (0 = course-wide).
    pub hole: u32,
    pub center: Vec2,
    pub radius_m: f64,
}

/// Stage 9 output.
#[derive(Clone, Debug, PartialEq)]
pub struct EarthworksDelta {
    pub patches: Vec<HeightPatch>,
    pub features: Vec<EarthFeature>,
}

impl EarthworksDelta {
    /// Final terrain sample: base height plus every patch covering `p`.
    pub fn height_at(&self, base: &Grid<f64>, p: Vec2) -> f64 {
        let mut z = base.bilinear(p);
        for patch in &self.patches {
            let s = &patch.delta_m.spec;
            let (w, h) = (s.cell_size * (s.nx - 1) as f64, s.cell_size * (s.ny - 1) as f64);
            if p.x >= s.origin.x
                && p.x <= s.origin.x + w
                && p.y >= s.origin.y
                && p.y <= s.origin.y + h
            {
                z += patch.delta_m.bilinear(p);
            }
        }
        z
    }
}

// ---------------------------------------------------------------------------
// Stage 10 — DrainageReport
// ---------------------------------------------------------------------------

/// Stage 10 output: the post-earthworks drainage audit. `repair_patches`
/// (swales/drains) are ADDITIONAL height patches applied after stage 9's.
#[derive(Clone, Debug, PartialEq)]
pub struct DrainageReport {
    pub pass: bool,
    pub metrics: BTreeMap<String, f64>,
    pub repair_patches: Vec<HeightPatch>,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Slope magnitude (rise/run) via central differences.
pub fn slope_grid(h: &Grid<f64>) -> Grid<f64> {
    let s = h.spec;
    let mut out = Grid::filled(s, 0.0);
    let d = s.cell_size;
    for y in 0..s.ny {
        for x in 0..s.nx {
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(s.nx - 1);
            let ym = y.saturating_sub(1);
            let yp = (y + 1).min(s.ny - 1);
            let gx = (h.get(xp, y) - h.get(xm, y)) / (d * (xp - xm).max(1) as f64);
            let gy = (h.get(x, yp) - h.get(x, ym)) / (d * (yp - ym).max(1) as f64);
            out.set(x, y, (gx * gx + gy * gy).sqrt());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use golf_core::GridSpec;

    #[test]
    fn cover_class_round_trips() {
        for v in 0..CoverClass::COUNT as u8 {
            assert_eq!(CoverClass::from_u8(v).unwrap() as u8, v);
        }
        assert_eq!(CoverClass::from_u8(200), None);
    }

    #[test]
    fn slope_of_a_plane_is_its_grade() {
        let spec = GridSpec::new(Vec2 { x: 0.0, y: 0.0 }, 10.0, 11, 11);
        let mut h = Grid::filled(spec, 0.0);
        for y in 0..11 {
            for x in 0..11 {
                h.set(x, y, 0.03 * (x as f64) * 10.0);
            }
        }
        let sl = slope_grid(&h);
        assert!((sl.get(5, 5) - 0.03).abs() < 1e-12);
    }
}
