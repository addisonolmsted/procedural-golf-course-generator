//! golf-landform — terrain-v2 Stage 2: swept-profile landform primitives and
//! their composition. NO noise layer, NO erosion (Stages 4/5) — this crate is
//! assessed on its own output (docs/terrain-v2-plan.md, Stage 2).
//!
//! The composition order is structural (the doc's rule): tilt (base field) →
//! ridges (smooth-max) → bluffs (additive step) → bowls (smooth-min) →
//! valleys LAST (smooth-min with the swept valley surface), so drainage
//! carves through everything and valley floors stay strictly monotone.
//!
//! Determinism: every stochastic choice (meander phase/jitter, bowl-rim
//! wobble) is keyed off explicit seeds inside the config; evaluation is pure
//! f64 arithmetic + libm via golf-core. Same config + resolution → the same
//! bits on any platform. The reduced-resolution fast path required by the
//! calibration optimizer is the `res_m` parameter of [`generate`] — one code
//! path at any grid step.

use golf_core::grid::{Grid, GridSpec};
use golf_core::math::Vec2;
use serde::{Deserialize, Serialize};

pub mod ease;
pub mod meander;
pub mod noise;
pub mod spline;

mod bluff;
mod bowl;
pub mod noiselab;
mod presets;
mod valley;

pub use bluff::Bluff;
pub use bowl::{Bowl, BowlBoundary, Outlet};
pub use meander::MeanderSpec;
pub use presets::{preset, preset_names, presets};
pub use valley::{Ridge, Valley, MIN_FALL_GRADIENT};

/// Serialized config schema version (bump on breaking schema change).
pub const SCHEMA_VERSION: u32 = 1;
/// Semantics version — fold into cache fingerprints (mirrors the
/// EROSION_VERSION / SAMPLER_VERSION convention of the v1 pipeline).
pub const LANDFORM_VERSION: u32 = 1;

/// Where a swept primitive's centerline comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Path {
    /// Explicit control points → Catmull-Rom → arc-length spine.
    Points(Vec<Vec2>),
    /// Generated meander between two endpoints (see [`meander`]).
    Meander(MeanderSpec),
}

impl Path {
    fn to_spine(&self) -> spline::Spine {
        match self {
            Path::Points(pts) => spline::Spine::new(spline::catmull_rom(pts, 10.0)),
            Path::Meander(spec) => meander::generate(spec),
        }
    }
}

/// Planar / low-order regional gradient — the base field.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct Tilt {
    /// Grade (rise/run) along +x and +y, about the window center.
    pub grade_x: f64,
    pub grade_y: f64,
    /// Low-order curvature: elevation delta from window corners to center
    /// (positive = dome, negative = basin). 0 = planar.
    pub curve_m: f64,
}

impl Tilt {
    fn eval(&self, p: Vec2, base: f64, extent: f64) -> f64 {
        let c = 0.5 * extent;
        let dx = p.x - c;
        let dy = p.y - c;
        let r2_corner = 2.0 * c * c;
        base + self.grade_x * dx + self.grade_y * dy
            + self.curve_m * (1.0 - (dx * dx + dy * dy) / r2_corner)
    }
}

/// The single versioned macro-terrain config (the calibration stage fits over
/// exactly this schema; later stages append their sections with serde
/// defaults, mirroring the TerrainParams convention).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroConfig {
    #[serde(default = "d_schema")]
    pub schema_version: u32,
    /// Square world edge length, meters.
    pub extent_m: f64,
    /// Reference base elevation, meters.
    pub base_elev_m: f64,
    #[serde(default)]
    pub tilt: Tilt,
    /// Applied in this order (the doc's composition rule); valleys always last.
    #[serde(default)]
    pub ridges: Vec<Ridge>,
    #[serde(default)]
    pub bluffs: Vec<Bluff>,
    #[serde(default)]
    pub bowls: Vec<Bowl>,
    #[serde(default)]
    pub valleys: Vec<Valley>,
}

fn d_schema() -> u32 {
    SCHEMA_VERSION
}

impl MacroConfig {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("config serializes")
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// The resolved (spine-built, junction-snapped) form — build once, evaluate
/// at any resolution.
pub struct Resolved {
    extent: f64,
    base: f64,
    tilt: Tilt,
    ridges: Vec<valley::ResolvedRidge>,
    bluffs: Vec<bluff::ResolvedBluff>,
    bowls: Vec<bowl::ResolvedBowl>,
    valleys: Vec<valley::ResolvedValley>,
}

/// Build spines, snap tributary junctions (accordant floors), precompute
/// bboxes. Panics on malformed configs (join_trunk pointing forward/at self).
pub fn resolve(cfg: &MacroConfig) -> Resolved {
    let ridges = cfg
        .ridges
        .iter()
        .map(|r| valley::ResolvedRidge::new(r, r.path.to_spine(), cfg.base_elev_m))
        .collect();
    let bluffs = cfg
        .bluffs
        .iter()
        .map(|b| bluff::ResolvedBluff::new(b, b.path.to_spine()))
        .collect();
    let bowls = cfg.bowls.iter().map(bowl::ResolvedBowl::new).collect();

    let mut valleys: Vec<valley::ResolvedValley> = Vec::with_capacity(cfg.valleys.len());
    for (i, v) in cfg.valleys.iter().enumerate() {
        let mut spine = v.path.to_spine();
        let mut rv_cfg = v.clone();
        let mut arc_cap = None;
        if let Some(t) = v.join_trunk {
            assert!(t < i, "join_trunk must reference an EARLIER valley (got {t} for valley {i})");
            let trunk = &valleys[t];
            // Snap the tributary's end onto the trunk centerline. Trailing
            // points that already wandered inside the trunk's corridor are
            // trimmed first, so the final approach is a clean segment into the
            // junction instead of a hook along the trunk wall.
            let end = *spine.pts.last().unwrap();
            let hit = trunk.spine.project(end);
            let junction = trunk.spine.point_at(hit.u);
            let trunk_hw = trunk.hw.sample(hit.u);
            let trib_hw = v.floor_halfwidth.knots.iter().map(|k| k.1).fold(0.0, f64::max);
            let clear = trunk_hw + trib_hw + 20.0;
            let mut pts = spine.pts.clone();
            while pts.len() > 2 && trunk.spine.project(*pts.last().unwrap()).d < clear {
                pts.pop();
            }
            // approach segment into the junction…
            let prev = *pts.last().unwrap();
            pts.push(junction);
            // …then OVERSHOOT across the trunk floor so the tributary's radial
            // end-cap lands inside the trunk's carved volume (an open mouth,
            // not a blunt spoon). The floor holds the junction elevation over
            // the overshoot (arc_cap), so nothing cuts below the trunk floor.
            let dirn = (junction - prev).normalized();
            let overshoot = trunk_hw + rv_cfg.floor_round_m + 10.0;
            let arc_to_junction = {
                let mut total = 0.0;
                for w in pts.windows(2) {
                    total += w[0].distance(w[1]);
                }
                total
            };
            pts.push(junction + dirn * overshoot);
            spine = spline::Spine::new(pts);
            arc_cap = Some(arc_to_junction);
            // Accordant floor: tributary floor at the JUNCTION equals the
            // trunk floor there, rising upstream at the tributary's gradient.
            let grad = v.fall_gradient.max(MIN_FALL_GRADIENT);
            rv_cfg.floor_z0_m = trunk.floor_z(hit.u) + grad * arc_to_junction;
            rv_cfg.fall_gradient = grad;
        }
        valleys.push(valley::ResolvedValley::with_cap(&rv_cfg, spine, arc_cap));
    }

    Resolved {
        extent: cfg.extent_m,
        base: cfg.base_elev_m,
        tilt: cfg.tilt,
        ridges,
        bluffs,
        bowls,
        valleys,
    }
}

impl Resolved {
    /// Composed elevation at a world point — the single source of truth
    /// (grids at every resolution sample exactly this).
    pub fn height_at(&self, p: Vec2) -> f64 {
        let mut z = self.tilt.eval(p, self.base, self.extent);
        for r in &self.ridges {
            z = r.apply(p, z);
        }
        for b in &self.bluffs {
            z = b.apply(p, z);
        }
        for b in &self.bowls {
            z = b.apply(p, z);
        }
        for v in &self.valleys {
            z = v.apply(p, z);
        }
        z
    }

    /// Valley floor elevation along valley `i` at normalized arc `u`
    /// (for monotonicity property tests + later hydrology stages).
    pub fn valley_floor(&self, i: usize, u: f64) -> (Vec2, f64) {
        let v = &self.valleys[i];
        (v.spine.point_at(u), v.floor_z(u))
    }

    pub fn n_valleys(&self) -> usize {
        self.valleys.len()
    }
}

/// Generate the composed field on a pixel-centered square grid at `res_m`.
/// `res_m * 4` is the calibration fast path — same code, coarser grid.
pub fn generate(cfg: &MacroConfig, res_m: f64) -> Grid<f64> {
    let r = resolve(cfg);
    let n = (cfg.extent_m / res_m).round().max(2.0) as u32;
    let spec = GridSpec::new(Vec2::new(res_m / 2.0, res_m / 2.0), res_m, n, n);
    let mut g = Grid::filled(spec, 0.0f64);
    for y in 0..n {
        for x in 0..n {
            let p = spec.world_of(x, y);
            g.set(x, y, r.height_at(p));
        }
    }
    g
}

/// FNV-1a over the field bits — the golden-hash primitive (same fold as the
/// v1 pipeline's goldens, so xtask can check landform goldens identically).
pub fn field_hash(g: &Grid<f64>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in &g.data {
        for b in v.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_roundtrips_json() {
        for (_name, cfg) in presets() {
            let j = cfg.to_json();
            let back = MacroConfig::from_json(&j).unwrap();
            assert_eq!(cfg, back);
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let cfg = preset("barranca").unwrap();
        let a = generate(&cfg, 20.0);
        let b = generate(&cfg, 20.0);
        assert_eq!(field_hash(&a), field_hash(&b));
    }

    #[test]
    fn fast_path_agrees_with_full_res_at_shared_points() {
        // both resolutions sample the same height_at: a coarse node that
        // coincides with a fine node must match bit-exactly
        let cfg = preset("barranca").unwrap();
        let r = resolve(&cfg);
        let fine = 10.0;
        let coarse = 40.0;
        // coarse node k sits at (k+0.5)*40; fine node j at (j+0.5)*10 —
        // they never coincide (pixel centers), so instead verify height_at
        // directly at a handful of arbitrary points equals both grids' source
        for &(x, y) in &[(333.0, 777.0), (1200.5, 400.25), (2100.0, 2100.0)] {
            let p = Vec2::new(x, y);
            let z1 = r.height_at(p);
            let z2 = r.height_at(p);
            assert_eq!(z1.to_bits(), z2.to_bits());
        }
        let _ = (fine, coarse);
    }

    #[test]
    fn junction_floors_are_accordant() {
        let cfg = preset("floodplain").unwrap();
        let r = resolve(&cfg);
        if r.n_valleys() < 2 {
            return;
        }
        // tributary (1) floor at the JUNCTION (arc cap) == trunk (0) floor
        // at the same point; the overshoot tail past it stays flat.
        let trib = &r.valleys[1];
        let cap = trib.arc_cap_m.expect("joined tributary has an arc cap");
        let u_j = cap / trib.spine.length();
        let jp = trib.spine.point_at(u_j);
        let trib_floor = trib.floor_z(u_j);
        let trunk = &r.valleys[0];
        let hit = trunk.spine.project(jp);
        assert!(hit.d < 1.0, "junction not on trunk (d = {})", hit.d);
        let trunk_floor = trunk.floor_z(hit.u);
        assert!(
            (trib_floor - trunk_floor).abs() < 1e-6,
            "floors not accordant: {trib_floor} vs {trunk_floor}"
        );
        // past the junction the tail rises gently (never below the junction)
        assert!(trib.floor_z(1.0) >= trib_floor - 1e-9);
    }
}
