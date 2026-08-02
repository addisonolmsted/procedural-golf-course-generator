//! Rasterization: `Resolved` + `MacroPlan` → the `MacroSkeleton` grids and
//! structure graph. One code path at any resolution: every cell samples the
//! same analytic `height_at` / field functions ([`crate::fields`]), so a
//! coarse raster is a node-subset of a fine one (hard requirement 4).

use course_world::grid::Grid;
use course_world::world::world_spec;

use crate::config::Resolved;
use crate::fields;
use crate::plan::MacroPlan;
use crate::skeleton::{MacroSkeleton, SkeletonFields, Spine, SpineKind, StructureGraph};

/// Emit the structure graph from the RESOLVED geometry (drain spines are the
/// junction-snapped floor centerlines, ordered downstream; tributaries are
/// truncated at their junction so the overshoot tail is not published).
pub fn structure(r: &Resolved) -> StructureGraph {
    let mut spines = Vec::new();
    for i in 0..r.n_valleys() {
        let sp = r.valley_spine(i);
        let len = sp.length();
        let u_end = r
            .valley_arc_cap(i)
            .map(|cap| (cap / len).min(1.0))
            .unwrap_or(1.0);
        let n = ((len * u_end / 10.0).ceil() as usize).max(2);
        let pts = (0..=n)
            .map(|k| sp.point_at(u_end * k as f64 / n as f64))
            .collect();
        spines.push(Spine {
            kind: SpineKind::Drain,
            pts,
        });
    }
    for i in 0..r.n_ridges() {
        let sp = r.ridge_spine(i);
        let pts = sp.pts.iter().copied().step_by(2).collect();
        spines.push(Spine {
            kind: SpineKind::RidgeLine,
            pts,
        });
    }
    for i in 0..r.n_bluffs() {
        let sp = r.bluff_spine(i);
        let pts = sp.pts.iter().copied().step_by(2).collect();
        spines.push(Spine {
            kind: SpineKind::BenchEdge,
            pts,
        });
    }
    StructureGraph { spines }
}

/// Rasterize the skeleton at `res_m` on the world grid.
pub fn rasterize(r: &Resolved, plan: &MacroPlan, res_m: f64) -> MacroSkeleton {
    let gs = world_spec(res_m);
    let mut base = Grid::filled(gs, 0.0f64);
    let mut floor_damp = Grid::filled(gs, 0.0f64);
    let mut slope_gain = Grid::filled(gs, 0.0f64);
    let mut grain = Grid::filled(gs, 0.0f64);
    let mut vdist = Grid::filled(gs, 0.0f64);
    let mut core = Grid::filled(gs, 0.0f64);

    let grain_az = plan.frame.grain_az_rad;
    // One index per drain spine, built once for the whole raster — the
    // nearest-drain query is the only per-cell cost that scales with the
    // valley count. Bit-identical to the unindexed path.
    let drains = fields::DrainIndex::build(r);
    for y in 0..gs.ny {
        for x in 0..gs.nx {
            let p = gs.world_of(x, y);
            base.set(x, y, r.height_at(p));
            slope_gain.set(x, y, fields::slope_gain_at(r, p));
            let (d, hw, tangent) = fields::nearest_drain_indexed(r, p, Some(&drains));
            floor_damp.set(x, y, fields::floor_damp_at(d, hw));
            vdist.set(x, y, d);
            grain.set(x, y, fields::grain_at(grain_az, d, hw, tangent));
            core.set(x, y, fields::core_protect_at(p));
        }
    }

    MacroSkeleton {
        base_height: base,
        fields: SkeletonFields {
            floor_damp,
            slope_gain,
            grain_dir_rad: grain,
            valley_dist_m: vdist,
            core_protect: core,
        },
        structure: structure(r),
    }
}
