//! The `framing.json` artifact: types, canonical bytes, loud-load validation.

use course_seed::PIPELINE_VERSION;
use course_world::math::{acos, Vec2};
use course_world::world::EXTENT_M;
use serde::{Deserialize, Serialize};

use crate::generate::TILT_TOL_RAD;

/// Artifact-shape version of `framing.json`. Any field or draw-transcript
/// change bumps this (with `PIPELINE_VERSION`) and re-blesses the golden.
///
/// v2 (2026-08-02): adds [`Skeleton`] — the structural skeleton (trunk
/// drainage corridor, secondary valley corridors, ridge/interfluve axes,
/// terrace step lines). Base fields are unchanged and bit-identical to v1
/// (see `base_fields_survive_v2`).
pub const FRAMING_VERSION: u32 = 2;

/// Cap on each skeleton family — also the fixed candidate-draw budget in
/// the generator, so the transcript length is outcome-independent.
pub const MAX_BRANCHES: usize = 6;
pub const MAX_RIDGES: usize = 12;
pub const MAX_STEPS: usize = 6;
/// How far a step line's chord may deviate from contour-parallel.
pub const STEP_AXIS_TOL_RAD: f64 = 0.35;
/// Slack on the trunk's monotone-toward-the-outlet check.
const TRUNK_MONOTONE_EPS: f64 = 1e-6;
/// How close a branch mouth must sit to the trunk spine (m).
const BRANCH_JOIN_TOL_M: f64 = 0.5;
/// Shortest legal ridge axis (m).
const RIDGE_MIN_LEN_M: f64 = 50.0;

/// The stage-01 artifact. Serialized as `framing.json` (compact JSON, field
/// order as declared here — the canonical bytes).
///
/// The header is deliberately NOT a `RunIdentity`: framing is stable-scoped,
/// so the artifact must not carry `attempt` — hard requirement 2 (stable
/// across rerolls) is literal byte-identity over the whole attempt chain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Framing {
    /// The master seed (never the attempt seed).
    pub seed: u64,
    pub pipeline_version: u32,
    pub framing_version: u32,
    pub window: WindowClass,
    pub base_level: BaseLevel,
    pub regional_tilt: Tilt,
    pub grain: Grain,
    pub provinces: Provinces,
    pub skeleton: Skeleton,
}

/// The structural skeleton: WHERE the land's macro organization runs.
///
/// Guidance geometry ONLY — no relief amplitudes live here. Stage 04 reads
/// the matching `framing.topo_*_relief_m` / `_carve_m` / `_riser_m` knobs
/// from `CourseSpec.params` (same precedent as `framing.province_relief_m`)
/// and compiles these curves into shallow forcing biases: uplift
/// conditioning along ridge axes, initial carve along corridors, strata-K
/// phase at step lines. The Stage-05 LEM does the finishing — the retired
/// step-03 generator failed measurement precisely by authoring landforms
/// here (commit 0313432).
///
/// Every family may be empty; an all-empty skeleton is legal and common
/// (florida_lowland draws one every time).
///
/// Families may cross each other: grain-parallel ridges and contour-parallel
/// steps necessarily intersect a downhill trunk. Nothing is trimmed — the
/// precedence is a Stage-04 composition rule (the corridor carve composes
/// last, so water gaps and cut terraces EMERGE rather than being authored).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skeleton {
    /// The trunk drainage corridor, monotone toward `base_level.edge`.
    pub trunk: Option<Trunk>,
    /// Secondary valley corridors, stored head→mouth; the mouth lies ON the
    /// trunk spine. Empty whenever `trunk` is `None`.
    pub branches: Vec<Vec<[f64; 2]>>,
    /// Ridge / interfluve axes along the structural grain, upstream endpoint
    /// first, entirely inside the box.
    pub ridges: Vec<Vec<[f64; 2]>>,
    /// Terrace / bench step lines: contour-parallel chords that enter and
    /// exit the box, like a province boundary.
    pub steps: Vec<Vec<[f64; 2]>>,
}

impl Skeleton {
    /// The all-empty skeleton (the legal empty case).
    pub fn empty() -> Self {
        Skeleton {
            trunk: None,
            branches: Vec::new(),
            ridges: Vec::new(),
            steps: Vec::new(),
        }
    }
}

/// The trunk drainage corridor. `halfwidth_m` is PLANFORM geometry (the
/// width of the corridor on the map), not relief: Stage 02 keeps the mask
/// off it, Stage 03's trunk spline refines WITHIN it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Trunk {
    pub spine: Vec<[f64; 2]>,
    pub halfwidth_m: f64,
}

/// Where in the implied larger landscape the window sits. Declaration order
/// is the draw order over the `framing.w_*` weight knobs — do not reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowClass {
    ValleyFloor,
    Interfluve,
    EscarpmentFace,
    BasinMargin,
    PiedmontSlope,
    TerraceFlight,
}

impl WindowClass {
    pub const ALL: [WindowClass; 6] = [
        WindowClass::ValleyFloor,
        WindowClass::Interfluve,
        WindowClass::EscarpmentFace,
        WindowClass::BasinMargin,
        WindowClass::PiedmontSlope,
        WindowClass::TerraceFlight,
    ];

    /// The `framing.<knob>` weight knob for this class.
    pub fn weight_knob(self) -> &'static str {
        match self {
            WindowClass::ValleyFloor => "framing.w_valley_floor",
            WindowClass::Interfluve => "framing.w_interfluve",
            WindowClass::EscarpmentFace => "framing.w_escarpment_face",
            WindowClass::BasinMargin => "framing.w_basin_margin",
            WindowClass::PiedmontSlope => "framing.w_piedmont_slope",
            WindowClass::TerraceFlight => "framing.w_terrace_flight",
        }
    }
}

/// The box edge (or corner) water leaves through. Declaration order is the
/// draw order — do not reorder.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeId {
    N,
    E,
    S,
    W,
    CornerNe,
    CornerNw,
    CornerSe,
    CornerSw,
}

impl EdgeId {
    pub const ALL: [EdgeId; 8] = [
        EdgeId::N,
        EdgeId::E,
        EdgeId::S,
        EdgeId::W,
        EdgeId::CornerNe,
        EdgeId::CornerNw,
        EdgeId::CornerSe,
        EdgeId::CornerSw,
    ];

    /// Unit vector pointing out of the box through this edge — the downhill
    /// direction the regional tilt must agree with.
    pub fn outward_dir(self) -> Vec2 {
        const D: f64 = std::f64::consts::FRAC_1_SQRT_2;
        match self {
            EdgeId::N => Vec2::new(0.0, 1.0),
            EdgeId::E => Vec2::new(1.0, 0.0),
            EdgeId::S => Vec2::new(0.0, -1.0),
            EdgeId::W => Vec2::new(-1.0, 0.0),
            EdgeId::CornerNe => Vec2::new(D, D),
            EdgeId::CornerNw => Vec2::new(-D, D),
            EdgeId::CornerSe => Vec2::new(D, -D),
            EdgeId::CornerSw => Vec2::new(-D, -D),
        }
    }
}

/// Where water leaves the window. `elev_m` is in the local vertical datum
/// (0 = the nominal core reference elevation stage 04 builds around); for
/// `water_table` archetypes downstream consumers read it as the water-table
/// elevation with every edge draining to it — stage 01 itself is mode-free.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BaseLevel {
    pub edge: EdgeId,
    pub elev_m: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tilt {
    /// Downhill direction, radians CCW from +x, in `[0, 2π)`. Within
    /// [`TILT_TOL_RAD`] of `base_level.edge`'s outward direction.
    pub dir_rad: f64,
    /// Grade (rise/run), ≥ 0.
    pub grade: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grain {
    /// Structural-grain AXIS, radians in `[0, π)` (a grain has no sign).
    pub dir_rad: f64,
    /// Strength in `[0, 1]`.
    pub anisotropy: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provinces {
    /// 1 or 2; 2 iff `boundary` is present.
    pub count: u32,
    pub boundary: Option<Boundary>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BoundaryKind {
    Scarp,
    ValleyWall,
    MaterialContact,
}

impl BoundaryKind {
    pub const ALL: [BoundaryKind; 3] = [
        BoundaryKind::Scarp,
        BoundaryKind::ValleyWall,
        BoundaryKind::MaterialContact,
    ];

    /// The `framing.<knob>` weight knob for this kind.
    pub fn weight_knob(self) -> &'static str {
        match self {
            BoundaryKind::Scarp => "framing.k_scarp",
            BoundaryKind::ValleyWall => "framing.k_valley_wall",
            BoundaryKind::MaterialContact => "framing.k_material_contact",
        }
    }
}

/// A province-boundary curve across the box. The curve enters and exits: its
/// endpoints lie outside the 3 km box (no dead-end curves). The landform
/// relief it carries is NOT here — stages 03/04 read
/// `framing.province_relief_m` from `CourseSpec.params`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Boundary {
    pub curve: Vec<[f64; 2]>,
    pub kind: BoundaryKind,
}

#[derive(Debug)]
pub enum FramingError {
    /// The bytes are not a valid `Framing`.
    Json(serde_json::Error),
    /// Written by a different pipeline contract version — loud by principle.
    VersionMismatch { found: u32 },
    /// Written by a different framing artifact version.
    FramingVersionMismatch { found: u32 },
    /// A float field is NaN/inf, or out of its stated range.
    FieldOutOfRange { field: &'static str },
    /// `regional_tilt.dir_rad` is not within tolerance of the base edge.
    TiltInconsistent { angle_rad: f64 },
    /// `count`/`boundary` disagree, or `count` outside `{1, 2}`.
    BadProvinces { count: u32, has_boundary: bool },
    /// The boundary curve does not enter and exit the box, or is degenerate.
    BadBoundaryCurve,
    /// The trunk corridor is degenerate, non-monotone, or does not run to
    /// the base-level edge.
    BadTrunk { reason: &'static str },
    /// A branch is degenerate, out of bounds, or its mouth is not on the
    /// trunk spine.
    BadBranch { index: usize, reason: &'static str },
    /// A ridge axis is degenerate or out of bounds.
    BadRidge { index: usize, reason: &'static str },
    /// A step line is degenerate, does not cross the box, or is not
    /// contour-parallel within tolerance.
    BadStep { index: usize, reason: &'static str },
    /// A skeleton family exceeds its cap (the generator's draw budget).
    SkeletonCountExceeded { family: &'static str, count: usize },
}

impl std::fmt::Display for FramingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FramingError::Json(e) => write!(f, "invalid framing.json: {e}"),
            FramingError::VersionMismatch { found } => {
                write!(
                    f,
                    "framing.json pipeline_version {found} != current {PIPELINE_VERSION}"
                )
            }
            FramingError::FramingVersionMismatch { found } => {
                write!(
                    f,
                    "framing.json framing_version {found} != current {FRAMING_VERSION}"
                )
            }
            FramingError::FieldOutOfRange { field } => {
                write!(f, "framing.json field out of range: {field}")
            }
            FramingError::TiltInconsistent { angle_rad } => {
                write!(
                    f,
                    "regional_tilt.dir_rad is {angle_rad} rad off the base edge (tol {TILT_TOL_RAD})"
                )
            }
            FramingError::BadProvinces {
                count,
                has_boundary,
            } => {
                write!(
                    f,
                    "provinces.count {count} inconsistent with boundary present={has_boundary}"
                )
            }
            FramingError::BadBoundaryCurve => {
                write!(f, "province boundary curve does not enter and exit the box")
            }
            FramingError::BadTrunk { reason } => {
                write!(f, "skeleton trunk invalid: {reason}")
            }
            FramingError::BadBranch { index, reason } => {
                write!(f, "skeleton branch [{index}] invalid: {reason}")
            }
            FramingError::BadRidge { index, reason } => {
                write!(f, "skeleton ridge [{index}] invalid: {reason}")
            }
            FramingError::BadStep { index, reason } => {
                write!(f, "skeleton step [{index}] invalid: {reason}")
            }
            FramingError::SkeletonCountExceeded { family, count } => {
                write!(f, "skeleton {family} count {count} exceeds its cap")
            }
        }
    }
}

impl std::error::Error for FramingError {}

impl Framing {
    /// The canonical `framing.json` bytes: compact JSON, declared field
    /// order, no trailing newline. `float_roundtrip` is on workspace-wide,
    /// so the f64 fields round-trip byte-identically.
    pub fn canonical_json(&self) -> String {
        serde_json::to_string(self).expect("Framing is always serializable")
    }

    /// Load and validate a `framing.json`. Contract mismatches and
    /// inconsistent fields are loud, never silently run.
    pub fn from_json(s: &str) -> Result<Self, FramingError> {
        let f: Framing = serde_json::from_str(s).map_err(FramingError::Json)?;
        if f.pipeline_version != PIPELINE_VERSION {
            return Err(FramingError::VersionMismatch {
                found: f.pipeline_version,
            });
        }
        if f.framing_version != FRAMING_VERSION {
            return Err(FramingError::FramingVersionMismatch {
                found: f.framing_version,
            });
        }
        f.validate()?;
        Ok(f)
    }

    /// The internal-consistency checks (stage-01 hard requirement 1), also
    /// run by `from_json`.
    pub fn validate(&self) -> Result<(), FramingError> {
        fn finite(v: f64, field: &'static str) -> Result<f64, FramingError> {
            if v.is_finite() {
                Ok(v)
            } else {
                Err(FramingError::FieldOutOfRange { field })
            }
        }
        finite(self.base_level.elev_m, "base_level.elev_m")?;
        let grade = finite(self.regional_tilt.grade, "regional_tilt.grade")?;
        if grade < 0.0 {
            return Err(FramingError::FieldOutOfRange {
                field: "regional_tilt.grade",
            });
        }
        let aniso = finite(self.grain.anisotropy, "grain.anisotropy")?;
        if !(0.0..=1.0).contains(&aniso) {
            return Err(FramingError::FieldOutOfRange {
                field: "grain.anisotropy",
            });
        }
        let gdir = finite(self.grain.dir_rad, "grain.dir_rad")?;
        if !(0.0..std::f64::consts::PI).contains(&gdir) {
            return Err(FramingError::FieldOutOfRange {
                field: "grain.dir_rad",
            });
        }
        let tdir = finite(self.regional_tilt.dir_rad, "regional_tilt.dir_rad")?;
        if !(0.0..2.0 * std::f64::consts::PI).contains(&tdir) {
            return Err(FramingError::FieldOutOfRange {
                field: "regional_tilt.dir_rad",
            });
        }

        // Tilt points at the base edge within tolerance.
        let tilt_vec = Vec2::new(course_world::math::cos(tdir), course_world::math::sin(tdir));
        let angle = acos(tilt_vec.dot(self.base_level.edge.outward_dir()).clamp(-1.0, 1.0));
        if angle > TILT_TOL_RAD + 1e-12 {
            return Err(FramingError::TiltInconsistent { angle_rad: angle });
        }

        // Provinces: count ∈ {1,2}, 2 ⇔ boundary present, curve crosses.
        let has_boundary = self.provinces.boundary.is_some();
        let count_ok = matches!(
            (self.provinces.count, has_boundary),
            (1, false) | (2, true)
        );
        if !count_ok {
            return Err(FramingError::BadProvinces {
                count: self.provinces.count,
                has_boundary,
            });
        }
        if let Some(b) = &self.provinces.boundary {
            if b.curve.len() < 4 {
                return Err(FramingError::BadBoundaryCurve);
            }
            for p in &b.curve {
                if !(p[0].is_finite() && p[1].is_finite()) {
                    return Err(FramingError::FieldOutOfRange {
                        field: "provinces.boundary.curve",
                    });
                }
            }
            let inside = |p: &[f64; 2]| {
                (0.0..=EXTENT_M).contains(&p[0]) && (0.0..=EXTENT_M).contains(&p[1])
            };
            let first = &b.curve[0];
            let last = &b.curve[b.curve.len() - 1];
            let mid = &b.curve[b.curve.len() / 2];
            // Enters and exits: both endpoints outside, the middle inside.
            if inside(first) || inside(last) || !inside(mid) {
                return Err(FramingError::BadBoundaryCurve);
            }
        }

        self.validate_skeleton()?;
        Ok(())
    }

    /// Skeleton coherence. Only θ-free structural checks live here; spacing
    /// and reserved-band rules depend on knob values and are tested instead.
    fn validate_skeleton(&self) -> Result<(), FramingError> {
        let s = &self.skeleton;
        if s.branches.len() > MAX_BRANCHES {
            return Err(FramingError::SkeletonCountExceeded {
                family: "branches",
                count: s.branches.len(),
            });
        }
        if s.ridges.len() > MAX_RIDGES {
            return Err(FramingError::SkeletonCountExceeded {
                family: "ridges",
                count: s.ridges.len(),
            });
        }
        if s.steps.len() > MAX_STEPS {
            return Err(FramingError::SkeletonCountExceeded {
                family: "steps",
                count: s.steps.len(),
            });
        }

        // Trunk: crosses the box, runs monotonically toward the outlet.
        if let Some(t) = &s.trunk {
            if !t.halfwidth_m.is_finite() || t.halfwidth_m <= 0.0 {
                return Err(FramingError::BadTrunk {
                    reason: "halfwidth_m must be finite and positive",
                });
            }
            if t.spine.len() < 4 {
                return Err(FramingError::BadTrunk {
                    reason: "spine needs at least 4 points",
                });
            }
            if !all_finite(&t.spine) {
                return Err(FramingError::BadTrunk {
                    reason: "spine has a non-finite point",
                });
            }
            let first = pt(&t.spine[0]);
            let last = pt(&t.spine[t.spine.len() - 1]);
            let mid = pt(&t.spine[t.spine.len() / 2]);
            if in_box(first) || in_box(last) || !in_box(mid) {
                return Err(FramingError::BadTrunk {
                    reason: "spine must enter and exit the box",
                });
            }
            let chord = last - first;
            if chord.length() < 1.0 {
                return Err(FramingError::BadTrunk {
                    reason: "spine chord is degenerate",
                });
            }
            let chord_unit = chord.normalized();
            for w in t.spine.windows(2) {
                if (pt(&w[1]) - pt(&w[0])).dot(chord_unit) < -TRUNK_MONOTONE_EPS {
                    return Err(FramingError::BadTrunk {
                        reason: "spine reverses along its own chord (non-monotone)",
                    });
                }
            }
            // Runs to the outlet: chord agrees with the edge's outward
            // direction and the exit clears that edge's half-plane.
            let out = self.base_level.edge.outward_dir();
            if chord_unit.dot(out) <= 0.0 {
                return Err(FramingError::BadTrunk {
                    reason: "spine does not run toward base_level.edge",
                });
            }
            let center = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
            if (last - center).dot(out) < EXTENT_M / 2.0 - 1.0 {
                return Err(FramingError::BadTrunk {
                    reason: "spine stops before reaching the outlet edge",
                });
            }
        } else if !s.branches.is_empty() {
            return Err(FramingError::BadBranch {
                index: 0,
                reason: "branches present without a trunk",
            });
        }

        // Branches: inside the box, mouth on the trunk spine.
        for (i, b) in s.branches.iter().enumerate() {
            let bad = |reason| FramingError::BadBranch { index: i, reason };
            if b.len() < 2 {
                return Err(bad("needs at least 2 points"));
            }
            if !all_finite(b) {
                return Err(bad("has a non-finite point"));
            }
            if b.iter().any(|p| !in_box(pt(p))) {
                return Err(bad("leaves the box"));
            }
            let mouth = pt(&b[b.len() - 1]);
            let spine = &s.trunk.as_ref().expect("checked above").spine;
            if dist_to_polyline(mouth, spine) > BRANCH_JOIN_TOL_M {
                return Err(bad("mouth does not meet the trunk spine"));
            }
        }

        // Ridges: inside the box, long enough to mean anything.
        for (i, r) in s.ridges.iter().enumerate() {
            let bad = |reason| FramingError::BadRidge { index: i, reason };
            if r.len() < 2 {
                return Err(bad("needs at least 2 points"));
            }
            if !all_finite(r) {
                return Err(bad("has a non-finite point"));
            }
            if r.iter().any(|p| !in_box(pt(p))) {
                return Err(bad("leaves the box"));
            }
            if (pt(&r[r.len() - 1]) - pt(&r[0])).length() < RIDGE_MIN_LEN_M {
                return Err(bad("chord is shorter than the minimum axis length"));
            }
        }

        // Steps: cross the box, contour-parallel within tolerance.
        let contour = self.regional_tilt.dir_rad + std::f64::consts::FRAC_PI_2;
        for (i, st) in s.steps.iter().enumerate() {
            let bad = |reason| FramingError::BadStep { index: i, reason };
            if st.len() < 4 {
                return Err(bad("needs at least 4 points"));
            }
            if !all_finite(st) {
                return Err(bad("has a non-finite point"));
            }
            let first = pt(&st[0]);
            let last = pt(&st[st.len() - 1]);
            let mid = pt(&st[st.len() / 2]);
            if in_box(first) || in_box(last) || !in_box(mid) {
                return Err(bad("must enter and exit the box"));
            }
            let chord = last - first;
            if chord.length() < 1.0 {
                return Err(bad("chord is degenerate"));
            }
            let ang = crate::generate::atan2_of(chord);
            if axis_delta(ang, contour) > STEP_AXIS_TOL_RAD + 1e-12 {
                return Err(bad("chord is not contour-parallel within tolerance"));
            }
        }
        Ok(())
    }
}

fn pt(p: &[f64; 2]) -> Vec2 {
    Vec2::new(p[0], p[1])
}

fn all_finite(pts: &[[f64; 2]]) -> bool {
    pts.iter().all(|p| p[0].is_finite() && p[1].is_finite())
}

fn in_box(p: Vec2) -> bool {
    (0.0..=EXTENT_M).contains(&p.x) && (0.0..=EXTENT_M).contains(&p.y)
}

/// Smallest angle between two UNDIRECTED axes (mod π).
fn axis_delta(a: f64, b: f64) -> f64 {
    let pi = std::f64::consts::PI;
    let d = (a - b).rem_euclid(pi);
    d.min(pi - d)
}

/// Distance from a point to a polyline (linear scan — polylines here are a
/// few hundred points and this runs once per branch).
fn dist_to_polyline(p: Vec2, poly: &[[f64; 2]]) -> f64 {
    let mut best = f64::INFINITY;
    for w in poly.windows(2) {
        let (a, b) = (pt(&w[0]), pt(&w[1]));
        let ab = b - a;
        let len2 = ab.dot(ab);
        let t = if len2 > 0.0 {
            ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        best = best.min((p - (a + ab * t)).length());
    }
    best
}
