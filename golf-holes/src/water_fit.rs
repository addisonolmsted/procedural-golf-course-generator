//! Aesthetic water refinement around the built holes.
//!
//! The terrain generator *suggests* water; the build phase gets the final
//! say near play. Two rules, applied only to standing water / wetlands whose
//! connected region intersects a hole's bounding box (everything else keeps
//! the terrain's outlines bit-for-bit):
//!
//! 1. **Fairway intrusion**: water may nick a fairway edge (≤ ~4 m) but not
//!    bulge deep into it — unless the region is a genuine cross hazard
//!    (it extends beyond BOTH sides of the corridor), which stays.
//! 2. **Wacky shapes**: near holes, thin lobed outlines get extra smoothing
//!    with a *relaxed* protection band (only decisively-thick features are
//!    pinned), so one-cell arms and wiggles melt into rounder banks.
//!
//! This edits the display/course-level SDF outlines only — terrain heights,
//! entities, and routing semantics are untouched.

use golf_core::math::Vec2;
use golf_core::Grid;
use golf_terrain::water::{CLASS_POND, CLASS_WETLAND};
use golf_terrain::CourseTerrain;

use crate::HoleBuild;

/// Refined water outlines for the built course.
#[derive(Clone, Debug)]
pub struct WaterRefine {
    pub standing_sdf: Grid<f64>,
    pub wetland_sdf: Grid<f64>,
    pub bridges: Vec<(Vec2, Vec2, u8)>,
}

/// Blocking-water chamfer distance under the REFINED outlines. Fairways are
/// re-clipped against this: where the refinement trimmed water out of a
/// fairway, the ground is free again and the fairway is restored.
pub fn refined_blocking_dist(ct: &CourseTerrain, wr: &WaterRefine) -> Grid<f64> {
    use golf_terrain::water::CLASS_STREAM;
    let spec = ct.water.class.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut d = Grid::filled(spec, 1.0e9f64);
    for i in 0..spec.len() {
        let blocked = wr.standing_sdf.data[i] >= 0.0
            || wr.wetland_sdf.data[i] >= 0.0
            || (ct.water.class.data[i] == CLASS_STREAM && ct.water.depth.data[i] >= 0.75);
        if blocked {
            d.data[i] = 0.0;
        }
    }
    let orth = spec.cell_size;
    let diag = spec.cell_size * std::f64::consts::SQRT_2;
    for y in 0..ny {
        for x in 0..nx {
            let i = y * nx + x;
            let mut v = d.data[i];
            if x > 0 {
                v = v.min(d.data[i - 1] + orth);
            }
            if y > 0 {
                v = v.min(d.data[i - nx] + orth);
                if x > 0 {
                    v = v.min(d.data[i - nx - 1] + diag);
                }
                if x + 1 < nx {
                    v = v.min(d.data[i - nx + 1] + diag);
                }
            }
            d.data[i] = v;
        }
    }
    for y in (0..ny).rev() {
        for x in (0..nx).rev() {
            let i = y * nx + x;
            let mut v = d.data[i];
            if x + 1 < nx {
                v = v.min(d.data[i + 1] + orth);
            }
            if y + 1 < ny {
                v = v.min(d.data[i + nx] + orth);
                if x + 1 < nx {
                    v = v.min(d.data[i + nx + 1] + diag);
                }
                if x > 0 {
                    v = v.min(d.data[i + nx - 1] + diag);
                }
            }
            d.data[i] = v;
        }
    }
    d
}

/// How far into a fairway water may reach before it gets trimmed, m.
const NICK_MAX: f64 = 4.0;
/// Cross-hazard test: the region must reach this far beyond both edges.
const CROSS_BEYOND: f64 = 4.0;

pub fn refine(ct: &CourseTerrain, holes: &[HoleBuild]) -> WaterRefine {
    // Hole bounding boxes (fairway ∪ green ∪ surround ∪ tee, + margin).
    let mut bboxes: Vec<(Vec2, Vec2)> = Vec::new();
    for h in holes {
        let (mut lo, mut hi) = h.fairway_bbox;
        let mut cover = |p: Vec2, m: f64| {
            lo = Vec2::new(lo.x.min(p.x - m), lo.y.min(p.y - m));
            hi = Vec2::new(hi.x.max(p.x + m), hi.y.max(p.y + m));
        };
        for &p in &h.green.outline {
            cover(p, 15.0);
        }
        if let Some(su) = &h.surround {
            for &p in &su.shape.outline {
                cover(p, 8.0);
            }
        }
        cover(h.tee.center, h.tee.a + 10.0);
        bboxes.push((lo, hi));
    }
    let in_bbox = |p: Vec2| -> bool {
        bboxes
            .iter()
            .any(|(lo, hi)| p.x >= lo.x && p.x <= hi.x && p.y >= lo.y && p.y <= hi.y)
    };

    let standing_sdf = refine_family(ct, holes, &ct.water.standing_sdf, &in_bbox);
    let wetland_sdf = refine_family(ct, holes, &ct.water.wetland_sdf, &in_bbox);

    // Bridges survive only if both endpoints are still wet in their family's
    // refined field (untouched regions keep theirs automatically).
    let bridges = ct
        .water
        .water_bridges
        .iter()
        .copied()
        .filter(|&(a, b, fam)| {
            let f = if fam == CLASS_WETLAND { &wetland_sdf } else { &standing_sdf };
            let _ = CLASS_POND;
            f.bilinear(a) >= 0.0 && f.bilinear(b) >= 0.0
        })
        .collect();

    WaterRefine {
        standing_sdf,
        wetland_sdf,
        bridges,
    }
}

fn refine_family(
    ct: &CourseTerrain,
    holes: &[HoleBuild],
    orig: &Grid<f64>,
    in_bbox: &dyn Fn(Vec2) -> bool,
) -> Grid<f64> {
    let spec = orig.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    let cell = spec.cell_size;
    let wet: Vec<bool> = orig.data.iter().map(|&v| v >= 0.0).collect();
    if !wet.iter().any(|&b| b) {
        return orig.clone();
    }

    // Connected wet components (8-connected — bridged diagonals belong
    // together for shape decisions).
    let mut comp = vec![u32::MAX; n];
    let mut n_comp = 0u32;
    for start in 0..n {
        if !wet[start] || comp[start] != u32::MAX {
            continue;
        }
        let id = n_comp;
        n_comp += 1;
        let mut stack = vec![start];
        comp[start] = id;
        while let Some(c) = stack.pop() {
            let (x, y) = (c % nx, c / nx);
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let xx = x as i64 + dx;
                let yy = y as i64 + dy;
                if xx < 0 || yy < 0 || xx >= nx as i64 || yy >= ny as i64 {
                    continue;
                }
                let j = yy as usize * nx + xx as usize;
                if wet[j] && comp[j] == u32::MAX {
                    comp[j] = id;
                    stack.push(j);
                }
            }
        }
    }

    // Which components touch a hole bbox, and which cross which fairways.
    let mut touched = vec![false; n_comp as usize];
    for i in 0..n {
        if wet[i] && in_bbox(spec.world_of((i % nx) as u32, (i / nx) as u32)) {
            touched[comp[i] as usize] = true;
        }
    }
    // Per (component, hole): does the region reach beyond both fairway
    // edges? (left/right in the spine frame, anywhere along the ribbon).
    let mut crossing = vec![[false; 2]; n_comp as usize * holes.len()];
    for i in 0..n {
        if !wet[i] || !touched[comp[i] as usize] {
            continue;
        }
        let p = spec.world_of((i % nx) as u32, (i / nx) as u32);
        for (hi, h) in holes.iter().enumerate() {
            if h.fw.is_none() {
                continue;
            }
            let (lo, hb) = h.fairway_bbox;
            if p.x < lo.x - 30.0 || p.x > hb.x + 30.0 || p.y < lo.y - 30.0 || p.y > hb.y + 30.0 {
                continue;
            }
            let (u, v) = h.spine.project(p);
            if u < h.fw.start || u > h.fw.end {
                continue;
            }
            let hw = h.half_width(u, v >= 0.0);
            let slot = comp[i] as usize * holes.len() + hi;
            if v > hw + CROSS_BEYOND {
                crossing[slot][0] = true;
            }
            if v < -(hw + CROSS_BEYOND) {
                crossing[slot][1] = true;
            }
        }
    }

    // Cross-hazard components (reach beyond both edges of some fairway)
    // keep the STRICT protection band below — a thin wetland strip across a
    // fairway is a legitimate forced carry, not a wacky lobe.
    let mut comp_crosses = vec![false; n_comp as usize];
    for c in 0..n_comp as usize {
        for hi in 0..holes.len() {
            let s = crossing[c * holes.len() + hi];
            if s[0] && s[1] {
                comp_crosses[c] = true;
                break;
            }
        }
    }

    // Refined field: forced-dry where water bulges into a fairway of a
    // non-crossing region, then extra smoothing near holes with a relaxed
    // protection band.
    let mut field = orig.data.clone();
    let mut forced = vec![false; n];
    for i in 0..n {
        if !wet[i] || !touched[comp[i] as usize] {
            continue;
        }
        let p = spec.world_of((i % nx) as u32, (i / nx) as u32);
        for (hi, h) in holes.iter().enumerate() {
            if h.fw.is_none() {
                continue;
            }
            let (u, v) = h.spine.project(p);
            if u < h.fw.start || u > h.fw.end {
                continue;
            }
            let hw = h.half_width(u, v >= 0.0);
            let depth_into = hw - v.abs();
            if depth_into <= NICK_MAX {
                continue;
            }
            let slot = comp[i] as usize * holes.len() + hi;
            if !(crossing[slot][0] && crossing[slot][1]) {
                field[i] = field[i].min(-3.0);
                forced[i] = true;
                break;
            }
        }
    }

    // Extra smoothing (two binomial passes) blended in only where the cell
    // sits in a hole bbox and belongs to (or neighbors) a touched region.
    let mut smooth = field.clone();
    for _ in 0..2 {
        let mut tmp = smooth.clone();
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                let a = smooth[y * nx + x.saturating_sub(1)];
                let b = smooth[y * nx + (x + 1).min(nx - 1)];
                tmp[i] = 0.25 * a + 0.5 * smooth[i] + 0.25 * b;
            }
        }
        let mut out = tmp.clone();
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                let a = tmp[y.saturating_sub(1) * nx + x];
                let b = tmp[(y + 1).min(ny - 1) * nx + x];
                out[i] = 0.25 * a + 0.5 * tmp[i] + 0.25 * b;
            }
        }
        smooth = out;
    }
    let mut refined = Grid::from_data(spec, field.clone());
    for i in 0..n {
        let p = spec.world_of((i % nx) as u32, (i / nx) as u32);
        let near_hole = in_bbox(p);
        if !near_hole {
            refined.data[i] = orig.data[i];
            continue;
        }
        let mut v = smooth[i];
        // Relaxed protection: only decisively-thick features keep their
        // sign — thin lobes melt away (that's the point). Cross-hazard
        // components keep the strict half-cell protection so a thin strip
        // across a fairway (a forced carry) survives.
        let strict = wet[i] && comp_crosses[comp[i] as usize];
        let protect = if strict { 0.5 * cell } else { 1.5 * cell };
        let base = field[i];
        let flipped_wet = base >= protect && v < 0.05;
        let flipped_dry = base <= -protect && v > -0.05;
        if flipped_wet || flipped_dry {
            v = base;
        }
        if forced[i] {
            v = v.min(-2.0);
        }
        refined.data[i] = v;
    }
    let _ = ct;
    refined
}
