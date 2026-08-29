//! Contract C2 — the routing substrate, the waist of the pipeline.
//!
//! S6 onward see this and only this: the router must do its job knowing
//! nothing about how the terrain was made. Self-sufficiency is an invariant —
//! a substrate loaded from disk is a complete input to S6 with no other file
//! required. Full field-by-field semantics:
//! `docs/contracts/C2-routing-substrate.md`.

use crate::error::{hex32, ContractError};
use crate::metadata::{StructureMeta, WaterPlaneOrigin};
use crate::plasticity::Plasticity;
use crate::units::check_direction;
use course_world::math::Vec2;
use course_world::world::{CORE_MAX_M, CORE_MIN_M};
use course_world::{Grid, GridSpec};
use serde::{Deserialize, Serialize};
use std::path::Path;

pub const C2_VERSION: u32 = 3;

/// Bounds on the play-window sides. The window is a PER-BIOME axis-aligned
/// RECTANGLE — `ScorerPreset::{play_long_m, play_short_m}` — placed in
/// either orientation (long side east-west or north-south), chosen by S5's
/// scan. Two measurements drove this (2026-08-28/29):
///
/// * Corridor packing over 12 seeds/archetype: 600 m square fits a 9-hole
///   route on 2/12 aeolian seeds; ~140 ha reaches 11/12. Dune courses
///   sprawl; parkland is compact. Per-biome AREA.
/// * The PCA-oriented footprints of the three true 9-hole courses in the
///   real corpus: 1517x487, 1222x304, 1357x628 m — RIBBONS with aspect
///   2.2-4.0, median aspect 1.70 across all 20 courses. A square is the
///   wrong container; its long axis cannot hold a real nine.
///
/// This is not the rotation the docs rejected: the rectangle stays
/// axis-aligned and translation-only, so the nesting ladder and the
/// deliverable format are untouched. Terrain grain is random per seed, so
/// offering both orientations recovers most of what free rotation would.
///
/// The 750 m terrain margin stays structural for ANY legal size: the window
/// must lie inside the core `[750, 2250]^2`, so the margin is enforced by
/// containment. Centre freedom per axis is ±(1500 − side)/2.
pub const PLAY_LONG_MIN_M: f64 = 600.0;
pub const PLAY_LONG_MAX_M: f64 = 1450.0;
pub const PLAY_SHORT_MIN_M: f64 = 300.0;

/// Centre freedom along one axis implied by that axis's side length.
pub fn play_center_range_m(side_m: f64) -> f64 {
    ((CORE_MAX_M - CORE_MIN_M) - side_m) / 2.0
}

/// An axis-aligned rect in world metres. Never rotated — deliverable
/// heightmaps are plain grids with no transform attached.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    pub min: Vec2,
    pub max: Vec2,
}

impl Rect {
    pub fn contains(&self, p: Vec2) -> bool {
        p.x >= self.min.x && p.x <= self.max.x && p.y >= self.min.y && p.y <= self.max.y
    }

    pub fn center(&self) -> Vec2 {
        Vec2::new(
            0.5 * (self.min.x + self.max.x),
            0.5 * (self.min.y + self.max.y),
        )
    }
}

/// Validate a play window against the biome's declared rectangle: sides must
/// be {long, short} in EITHER orientation, inside the core. Core containment
/// IS the 750 m terrain-margin guarantee and bounds each centre coordinate
/// by `play_center_range_m(side)`, so no separate centre check exists to
/// drift out of sync (`docs/contracts/C2-routing-substrate.md`).
pub fn check_play_window(w: &Rect, long_m: f64, short_m: f64)
    -> Result<(), ContractError> {
    let tol = 1e-6;
    if !(PLAY_LONG_MIN_M - tol..=PLAY_LONG_MAX_M + tol).contains(&long_m)
        || short_m < PLAY_SHORT_MIN_M - tol
        || short_m > long_m + tol
    {
        return Err(ContractError::invariant(
            "play_window",
            format!("dims {long_m} x {short_m} outside                      [{PLAY_LONG_MIN_M}, {PLAY_LONG_MAX_M}] x                      [{PLAY_SHORT_MIN_M}, long]"),
        ));
    }
    let side_x = w.max.x - w.min.x;
    let side_y = w.max.y - w.min.y;
    let fits = ((side_x - long_m).abs() <= tol && (side_y - short_m).abs() <= tol)
        || ((side_x - short_m).abs() <= tol && (side_y - long_m).abs() <= tol);
    if !fits {
        return Err(ContractError::invariant(
            "play_window",
            format!("must be {long_m} x {short_m} in either orientation,                      got {side_x} x {side_y}"),
        ));
    }
    if w.min.x < CORE_MIN_M - tol
        || w.min.y < CORE_MIN_M - tol
        || w.max.x > CORE_MAX_M + tol
        || w.max.y > CORE_MAX_M + tol
    {
        return Err(ContractError::invariant(
            "play_window",
            "must lie inside the core [750, 2250]^2",
        ));
    }
    Ok(())
}

/// The exclusion masks, all at `grid_cost`. `protected` and `out_of_bounds`
/// are the ONLY hard booleans in C2 — everything else is soft, because hard
/// constraints are the mechanism that forces retries and v2 has none.
#[derive(Clone, Debug, PartialEq)]
pub struct ExclusionMasks {
    /// `[0, 1]` fractional water coverage per cell.
    pub water: Grid<f64>,
    /// m; meaningful only where `water > 0`.
    pub water_surface_m: Grid<f64>,
    /// Hard: may not be routed through or graded. Stored as u8 (0/1).
    pub protected: Grid<u8>,
    /// Hard: outside the core, or otherwise unusable. Stored as u8 (0/1).
    pub out_of_bounds: Grid<u8>,
    /// `[0, 1]` SOFT slope/material infeasibility.
    pub unbuildable: Grid<f64>,
}

/// The cost fields, all at `grid_cost`, non-negative and finite. `borrow` is
/// a *resource*, not a cost — the scorer subtracts it.
#[derive(Clone, Debug, PartialEq)]
pub struct CostFields {
    /// m³ per cell to make it playable.
    pub earthwork: Grid<f64>,
    /// m³-equivalent to clear cover.
    pub clearing: Grid<f64>,
    /// m³ of fill obtainable from this cell.
    pub borrow: Grid<f64>,
    /// Dimensionless ponding/erosion exposure. Also read by S7.
    pub drainage_risk: Grid<f64>,
}

/// The provenance thread plus flow, at `grid_cost`. `grain_axis_rad` and
/// `wind_azimuth_rad` are bit-identical to their C1 values — S5 may not
/// recompute them.
#[derive(Clone, Debug, PartialEq)]
pub struct SubstrateMeta {
    /// DIRECTION `[0, 2π)` per cell.
    pub flow_dir_rad: Grid<f64>,
    /// Upstream contributing area, m².
    pub flow_accum: Grid<f64>,
    pub grain_axis_rad: f64,
    pub wind_azimuth_rad: f64,
    pub water_plane_origin: WaterPlaneOrigin,
    pub plasticity: Plasticity,
}

/// The biome's routing weights, carried as data. S6 contains no biome
/// knowledge; every number it needs arrives here.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScorerPreset {
    pub w_earthwork: f64,
    pub w_clearing: f64,
    pub w_drainage: f64,
    pub w_fit: f64,
    /// `[0, 1]` on `masks.water`.
    pub water_exclude_threshold: f64,
    /// Rise/run.
    pub target_grade_fairway: f64,
    pub target_grade_green: f64,
    /// Target lengths, play order. 9 because the course is a 9-hole par-36
    /// round (`crate::PAR_SEQUENCE` is the authority on par).
    pub hole_length_m: [f64; 9],
    /// `[0, 1]`, plasticity-derived.
    pub feasibility_strictness: f64,
    /// Play-window rectangle in metres, per-biome data; placed in either
    /// orientation by S5 (see the measurements on `PLAY_LONG_MIN_M`'s doc).
    pub play_long_m: f64,
    pub play_short_m: f64,
}

impl ScorerPreset {
    pub fn validate(&self) -> Result<(), ContractError> {
        use crate::units::{check_finite, check_unit};
        for (name, v) in [
            ("preset.w_earthwork", self.w_earthwork),
            ("preset.w_clearing", self.w_clearing),
            ("preset.w_drainage", self.w_drainage),
            ("preset.w_fit", self.w_fit),
            ("preset.target_grade_fairway", self.target_grade_fairway),
            ("preset.target_grade_green", self.target_grade_green),
            ("preset.play_long_m", self.play_long_m),
            ("preset.play_short_m", self.play_short_m),
        ] {
            check_finite(name, v)?;
            if v < 0.0 {
                return Err(ContractError::invariant(name, "must be >= 0"));
            }
        }
        check_unit("preset.water_exclude_threshold", self.water_exclude_threshold)?;
        check_unit("preset.feasibility_strictness", self.feasibility_strictness)?;
        for (i, l) in self.hole_length_m.iter().enumerate() {
            if !l.is_finite() || *l <= 0.0 {
                return Err(ContractError::invariant(
                    "preset.hole_length_m",
                    format!("hole {i}: must be finite and > 0"),
                ));
            }
        }
        Ok(())
    }
}

/// Contract C2. See the module docs; construction is the only path in.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutingSubstrate {
    pub c2_version: u32,
    pub seed: u64,
    pub spec_digest: [u8; 32],
    /// The height rung (2 m in production).
    pub grid_full: GridSpec,
    /// The cost rung — exactly `grid_full` coarsened 4×, nesting.
    pub grid_cost: GridSpec,
    /// m, at `grid_full`. Under water this is the BED, not the surface.
    pub height: Grid<f64>,
    pub play_window: Rect,
    pub masks: ExclusionMasks,
    pub cost: CostFields,
    pub meta: SubstrateMeta,
    pub preset: ScorerPreset,
}

impl RoutingSubstrate {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        seed: u64,
        spec_digest: [u8; 32],
        grid_full: GridSpec,
        grid_cost: GridSpec,
        height: Grid<f64>,
        play_window: Rect,
        masks: ExclusionMasks,
        cost: CostFields,
        meta: SubstrateMeta,
        preset: ScorerPreset,
    ) -> Result<Self, ContractError> {
        let rs = RoutingSubstrate {
            c2_version: C2_VERSION,
            seed,
            spec_digest,
            grid_full,
            grid_cost,
            height,
            play_window,
            masks,
            cost,
            meta,
            preset,
        };
        rs.validate()?;
        Ok(rs)
    }

    fn validate(&self) -> Result<(), ContractError> {
        if self.c2_version != C2_VERSION {
            return Err(ContractError::Version {
                what: "c2_version",
                found: self.c2_version,
                expected: C2_VERSION,
            });
        }
        // Rungs nest: cost = full coarsened 4x, same origin, node-aligned.
        let f = &self.grid_full;
        let c = &self.grid_cost;
        let ok = f.origin == c.origin
            && (c.cell_size - 4.0 * f.cell_size).abs() < 1e-9
            && f.nx >= 1
            && c.nx >= 1
            && (f.nx - 1) == 4 * (c.nx - 1)
            && (f.ny - 1) == 4 * (c.ny - 1);
        if !ok {
            return Err(ContractError::invariant(
                "grids",
                "grid_cost must be grid_full coarsened exactly 4x (nesting rungs)",
            ));
        }
        if self.height.spec != *f {
            return Err(ContractError::invariant("height", "spec must be grid_full"));
        }
        if self.height.data.iter().any(|v| !v.is_finite()) {
            return Err(ContractError::invariant("height", "contains non-finite"));
        }
        check_play_window(&self.play_window, self.preset.play_long_m, self.preset.play_short_m)?;

        let at_cost = |name: &'static str, spec: &GridSpec| -> Result<(), ContractError> {
            if spec != c {
                return Err(ContractError::invariant(name, "spec must be grid_cost"));
            }
            Ok(())
        };
        at_cost("masks.water", &self.masks.water.spec)?;
        at_cost("masks.water_surface_m", &self.masks.water_surface_m.spec)?;
        at_cost("masks.protected", &self.masks.protected.spec)?;
        at_cost("masks.out_of_bounds", &self.masks.out_of_bounds.spec)?;
        at_cost("masks.unbuildable", &self.masks.unbuildable.spec)?;
        at_cost("cost.earthwork", &self.cost.earthwork.spec)?;
        at_cost("cost.clearing", &self.cost.clearing.spec)?;
        at_cost("cost.borrow", &self.cost.borrow.spec)?;
        at_cost("cost.drainage_risk", &self.cost.drainage_risk.spec)?;
        at_cost("meta.flow_dir_rad", &self.meta.flow_dir_rad.spec)?;
        at_cost("meta.flow_accum", &self.meta.flow_accum.spec)?;

        for (name, g) in [
            ("masks.water", &self.masks.water),
            ("masks.unbuildable", &self.masks.unbuildable),
        ] {
            if g.data.iter().any(|v| !(0.0..=1.0).contains(v)) {
                return Err(ContractError::invariant("masks", format!("{name} out of [0, 1]")));
            }
        }
        for (name, g) in [
            ("masks.protected", &self.masks.protected),
            ("masks.out_of_bounds", &self.masks.out_of_bounds),
        ] {
            if g.data.iter().any(|v| *v > 1) {
                return Err(ContractError::invariant("masks", format!("{name} must be 0/1")));
            }
        }
        // Costs: non-negative and FINITE. An infinity is a hard constraint
        // smuggled into a soft field.
        for (name, g) in [
            ("cost.earthwork", &self.cost.earthwork),
            ("cost.clearing", &self.cost.clearing),
            ("cost.borrow", &self.cost.borrow),
            ("cost.drainage_risk", &self.cost.drainage_risk),
            ("meta.flow_accum", &self.meta.flow_accum),
        ] {
            if g.data.iter().any(|v| !v.is_finite() || *v < 0.0) {
                return Err(ContractError::invariant(
                    "cost",
                    format!("{name} must be finite and >= 0"),
                ));
            }
        }
        if self
            .meta
            .flow_dir_rad
            .data
            .iter()
            .any(|v| !v.is_finite() || !(0.0..crate::units::TAU).contains(v))
        {
            return Err(ContractError::invariant(
                "meta.flow_dir_rad",
                "directions must be finite in [0, 2\u{3c0})",
            ));
        }
        crate::units::check_axis("meta.grain_axis_rad", self.meta.grain_axis_rad)?;
        check_direction("meta.wind_azimuth_rad", self.meta.wind_azimuth_rad)?;
        self.preset.validate()
    }

    /// Bit-identity check against the originating C1 metadata — the
    /// provenance-thread invariant. S5's tests call this.
    pub fn metadata_matches(&self, c1_meta: &StructureMeta) -> bool {
        self.meta.grain_axis_rad.to_bits() == c1_meta.grain_axis_rad.to_bits()
            && self.meta.wind_azimuth_rad.to_bits() == c1_meta.wind_azimuth_rad.to_bits()
    }

    // ---------------------------------------------------------- artifact io

    pub fn write_dir(&self, dir: &Path) -> Result<(), ContractError> {
        std::fs::create_dir_all(dir)?;
        let mut grids: Vec<GridRef> = Vec::new();
        let wf = |name: &str, g: &Grid<f64>| -> Result<GridRef, ContractError> {
            let path = dir.join(format!("{name}.cgrid"));
            course_world::gridio::write_grid_f32(&path, g)?;
            let bytes = std::fs::read(&path)?;
            Ok(GridRef {
                name: name.to_string(),
                blake3: blake3::hash(&bytes).to_hex().to_string(),
            })
        };
        grids.push(wf("height", &self.height)?);
        grids.push(wf("water", &self.masks.water)?);
        grids.push(wf("water_surface_m", &self.masks.water_surface_m)?);
        grids.push(wf("unbuildable", &self.masks.unbuildable)?);
        grids.push(wf("earthwork", &self.cost.earthwork)?);
        grids.push(wf("clearing", &self.cost.clearing)?);
        grids.push(wf("borrow", &self.cost.borrow)?);
        grids.push(wf("drainage_risk", &self.cost.drainage_risk)?);
        grids.push(wf("flow_dir_rad", &self.meta.flow_dir_rad)?);
        grids.push(wf("flow_accum", &self.meta.flow_accum)?);
        for (name, g) in [
            ("protected", &self.masks.protected),
            ("out_of_bounds", &self.masks.out_of_bounds),
        ] {
            let path = dir.join(format!("{name}.cgrid"));
            course_world::gridio::write_grid_u8(&path, g)?;
            let bytes = std::fs::read(&path)?;
            grids.push(GridRef {
                name: name.to_string(),
                blake3: blake3::hash(&bytes).to_hex().to_string(),
            });
        }
        let header = C2Header {
            c2_version: self.c2_version,
            seed: self.seed,
            spec_digest: self.spec_digest,
            grid_full: self.grid_full,
            grid_cost: self.grid_cost,
            play_window: self.play_window,
            grain_axis_rad: self.meta.grain_axis_rad,
            wind_azimuth_rad: self.meta.wind_azimuth_rad,
            water_plane_origin: self.meta.water_plane_origin,
            plasticity: self.meta.plasticity,
            preset: self.preset.clone(),
            grids,
        };
        let json = serde_json::to_string_pretty(&header)
            .map_err(|e| ContractError::Malformed { what: e.to_string() })?;
        std::fs::write(dir.join("substrate.json"), json)?;
        Ok(())
    }

    pub fn read_dir(dir: &Path) -> Result<Self, ContractError> {
        let json = std::fs::read_to_string(dir.join("substrate.json"))?;
        let h: C2Header = serde_json::from_str(&json).map_err(|e| ContractError::Malformed {
            what: format!("substrate.json: {e}"),
        })?;
        if h.c2_version != C2_VERSION {
            return Err(ContractError::Version {
                what: "c2_version",
                found: h.c2_version,
                expected: C2_VERSION,
            });
        }
        let check = |name: &str| -> Result<std::path::PathBuf, ContractError> {
            let path = dir.join(format!("{name}.cgrid"));
            let gr = h
                .grids
                .iter()
                .find(|g| g.name == name)
                .ok_or_else(|| ContractError::Malformed {
                    what: format!("missing grid `{name}` in manifest"),
                })?;
            let bytes = std::fs::read(&path)?;
            if blake3::hash(&bytes).to_hex().to_string() != gr.blake3 {
                return Err(ContractError::HashMismatch {
                    file: format!("{name}.cgrid"),
                });
            }
            Ok(path)
        };
        let rf = |name: &str| -> Result<Grid<f64>, ContractError> {
            Ok(course_world::gridio::read_grid_f32(&check(name)?)?)
        };
        let ru = |name: &str| -> Result<Grid<u8>, ContractError> {
            Ok(course_world::gridio::read_grid_u8(&check(name)?)?)
        };
        let rs = RoutingSubstrate {
            c2_version: h.c2_version,
            seed: h.seed,
            spec_digest: h.spec_digest,
            grid_full: h.grid_full,
            grid_cost: h.grid_cost,
            height: rf("height")?,
            play_window: h.play_window,
            masks: ExclusionMasks {
                water: rf("water")?,
                water_surface_m: rf("water_surface_m")?,
                protected: ru("protected")?,
                out_of_bounds: ru("out_of_bounds")?,
                unbuildable: rf("unbuildable")?,
            },
            cost: CostFields {
                earthwork: rf("earthwork")?,
                clearing: rf("clearing")?,
                borrow: rf("borrow")?,
                drainage_risk: rf("drainage_risk")?,
            },
            meta: SubstrateMeta {
                flow_dir_rad: rf("flow_dir_rad")?,
                flow_accum: rf("flow_accum")?,
                grain_axis_rad: h.grain_axis_rad,
                wind_azimuth_rad: h.wind_azimuth_rad,
                water_plane_origin: h.water_plane_origin,
                plasticity: h.plasticity,
            },
            preset: h.preset,
        };
        rs.validate()?;
        Ok(rs)
    }
}

#[derive(Serialize, Deserialize)]
struct C2Header {
    c2_version: u32,
    seed: u64,
    #[serde(with = "hex32")]
    spec_digest: [u8; 32],
    grid_full: GridSpec,
    grid_cost: GridSpec,
    play_window: Rect,
    grain_axis_rad: f64,
    wind_azimuth_rad: f64,
    water_plane_origin: WaterPlaneOrigin,
    plasticity: Plasticity,
    preset: ScorerPreset,
    grids: Vec<GridRef>,
}

#[derive(Serialize, Deserialize)]
struct GridRef {
    name: String,
    blake3: String,
}
