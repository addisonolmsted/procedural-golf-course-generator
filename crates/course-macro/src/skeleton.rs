//! Step 03 output contract types (`steps/03-macro-landform.md`), ported from
//! `archetype-pipeline:course-contracts/src/stages.rs`. Grids ride as CGRID1
//! sidecars (see [`crate::artifact`]); the structure graph is serde JSON.

use course_world::{Grid, Vec2};
use serde::{Deserialize, Serialize};

/// Per-cell conditioning fields downstream steps modulate on. All on the same
/// spec as `base_height`, all dimensionless in [0,1] except where noted.
#[derive(Clone, Debug, PartialEq)]
pub struct SkeletonFields {
    /// Noise suppression near drainage floors (1 = fully damped). The no-dam
    /// guard: step 04 MUST scale noise amplitude down by this.
    pub floor_damp: Grid<f64>,
    /// Extra noise amplitude on structurally steep ground.
    pub slope_gain: Grid<f64>,
    /// Local structural grain direction (radians, 0 = +x, undirected mod π).
    pub grain_dir_rad: Grid<f64>,
    /// Distance (m) to the nearest drainage spine.
    pub valley_dist_m: Grid<f64>,
    /// 1 inside the routable core fading to 0 outside — steps may use it to
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

/// The macro structure the terrain is organized around. Step 05 conditions
/// erosion on drain spines; steps 07/08 read it for corridor hints.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Default)]
pub struct StructureGraph {
    pub spines: Vec<Spine>,
}

/// Step 03 output.
#[derive(Clone, Debug, PartialEq)]
pub struct MacroSkeleton {
    pub base_height: Grid<f64>,
    pub fields: SkeletonFields,
    pub structure: StructureGraph,
}

/// Slope magnitude (rise/run) via central differences. Shared helper for the
/// gate/viewer; the skeleton's own `slope_gain` field is computed analytically
/// in [`crate::fields`], not from this raster derivative.
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
    use course_world::GridSpec;

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

    #[test]
    fn spine_kind_serializes_snake_case() {
        let s = Spine {
            kind: SpineKind::BenchEdge,
            pts: vec![Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0)],
        };
        let j = serde_json::to_string(&s).unwrap();
        assert!(j.contains("bench_edge"), "{j}");
    }
}
