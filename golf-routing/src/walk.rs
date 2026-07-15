//! Green→next-tee walking paths: deterministic A* on the macro grid.
//!
//! Walks must never cross a line of play — they curve around, and the curved
//! length (not the straight line) is what the optimizer scores. Obstacles are
//! blocking water plus all play corridors inflated by `CORRIDOR_INFLATE`,
//! except within `EXEMPT_R` of the walk's own endpoints (a tee usually sits
//! beside the previous fairway). A strict no-crossing check runs on the final
//! polyline regardless of the exemption.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use golf_core::math::Vec2;
use golf_core::GridSpec;
use golf_terrain::CourseTerrain;

use crate::fields::cell_of;
use crate::geometry::{poly_len, seg_seg_intersect};
use crate::{BOX_MAX, BOX_MIN};

/// Play corridors block walking within this half-width, meters.
pub const CORRIDOR_INFLATE: f64 = 15.0;
/// Hard cap on a walk's curved length (atlas max is 583 m) — a longer detour
/// means the layout topology is wrong, not that the walk is long.
pub const WALK_CURVED_MAX: f64 = 620.0;
/// Corridor blocking is waived within this radius of the walk endpoints.
const EXEMPT_R: f64 = 60.0;
/// The strict crossing check ignores the first/last few meters of the walk
/// (it necessarily starts on the previous hole's green).
const TRIM: f64 = 8.0;

/// Static walkability (water + box), shared across all walk queries.
pub struct WalkGrid {
    pub spec: GridSpec,
    blocked: Vec<bool>,
    slope: Vec<f64>,
}

impl WalkGrid {
    pub fn new(ct: &CourseTerrain) -> Self {
        let spec = ct.macro_slope.spec;
        let n = spec.len();
        let mut blocked = vec![false; n];
        for y in 0..spec.ny {
            for x in 0..spec.nx {
                let i = spec.index(x, y);
                let p = spec.world_of(x, y);
                let out = p.x < BOX_MIN || p.x > BOX_MAX || p.y < BOX_MIN || p.y > BOX_MAX;
                blocked[i] = out || ct.water.is_blocking(i);
            }
        }
        WalkGrid {
            spec,
            blocked,
            slope: ct.macro_slope.data.clone(),
        }
    }
}

/// Cells within `CORRIDOR_INFLATE` of any play segment. Rebuilt when the set
/// of placed segments changes.
pub fn corridor_mask(spec: &GridSpec, segs: &[(Vec2, Vec2)]) -> Vec<bool> {
    let mut mask = vec![false; spec.len()];
    let cell = spec.cell_size;
    let pad = CORRIDOR_INFLATE + cell;
    for &(a, b) in segs {
        let x0 = (((a.x.min(b.x) - pad) / cell) as i64).clamp(0, spec.nx as i64 - 1);
        let x1 = (((a.x.max(b.x) + pad) / cell) as i64).clamp(0, spec.nx as i64 - 1);
        let y0 = (((a.y.min(b.y) - pad) / cell) as i64).clamp(0, spec.ny as i64 - 1);
        let y1 = (((a.y.max(b.y) + pad) / cell) as i64).clamp(0, spec.ny as i64 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = spec.world_of(x as u32, y as u32);
                if crate::geometry::point_seg_dist(p, a, b) <= CORRIDOR_INFLATE {
                    mask[y as usize * spec.nx as usize + x as usize] = true;
                }
            }
        }
    }
    mask
}

/// Does the walk polyline cross any play segment? The first/last `TRIM`
/// meters are excluded (the walk starts on a green / ends on a tee that
/// belongs to an adjacent line of play).
pub fn walk_crosses_play(walk: &[Vec2], segs: &[(Vec2, Vec2)]) -> bool {
    let total = poly_len(walk);
    if total <= 2.0 * TRIM {
        return false;
    }
    // Walk the polyline, accumulating arc length; test only the middle part.
    let mut acc = 0.0;
    for w in walk.windows(2) {
        let (mut a, mut b) = (w[0], w[1]);
        let seg_len = a.distance(b);
        let s0 = acc;
        let s1 = acc + seg_len;
        acc = s1;
        if s1 <= TRIM || s0 >= total - TRIM || seg_len == 0.0 {
            continue;
        }
        if s0 < TRIM {
            a = a.lerp(b, (TRIM - s0) / seg_len);
        }
        if s1 > total - TRIM {
            b = w[0].lerp(w[1], (total - TRIM - s0) / seg_len);
        }
        for &(p, q) in segs {
            if seg_seg_intersect(a, b, p, q) {
                return true;
            }
        }
    }
    false
}

/// Find a walking path `from` → `to`. Returns the smoothed polyline and its
/// length, or `None` if no non-crossing path exists.
pub fn find_walk(
    grid: &WalkGrid,
    corridor: &[bool],
    segs: &[(Vec2, Vec2)],
    from: Vec2,
    to: Vec2,
) -> Option<(Vec<Vec2>, f64)> {
    let spec = &grid.spec;
    let nx = spec.nx as usize;
    let start = cell_of(spec, from);
    let goal = cell_of(spec, to);

    let exempt = |i: usize| -> bool {
        let p = spec.world_of((i % nx) as u32, (i / nx) as u32);
        p.distance(from) <= EXEMPT_R || p.distance(to) <= EXEMPT_R
    };
    let is_blocked = |i: usize| -> bool {
        if grid.blocked[i] {
            return true;
        }
        corridor[i] && !exempt(i)
    };

    let path_cells = if start == goal {
        vec![start]
    } else {
        astar(grid, &is_blocked, start, goal)?
    };

    // Cell centers, with the exact endpoints substituted.
    let mut pts: Vec<Vec2> = Vec::with_capacity(path_cells.len() + 2);
    pts.push(from);
    for &c in &path_cells {
        pts.push(spec.world_of((c % nx) as u32, (c / nx) as u32));
    }
    pts.push(to);

    // String-pull: greedily skip to the furthest waypoint with a clear line
    // of sight (5 m sampling against the same blocked predicate).
    let los = |a: Vec2, b: Vec2| -> bool {
        let steps = (a.distance(b) / 5.0).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let p = a.lerp(b, k as f64 / steps as f64);
            if is_blocked(cell_of(spec, p)) {
                return false;
            }
        }
        true
    };
    let mut smooth: Vec<Vec2> = vec![pts[0]];
    let mut i = 0usize;
    while i + 1 < pts.len() {
        let mut j = pts.len() - 1;
        while j > i + 1 && !los(pts[i], pts[j]) {
            j -= 1;
        }
        smooth.push(pts[j]);
        i = j;
    }

    // Strict guarantee: the walk may not cross any line of play. If smoothing
    // introduced a crossing (possible inside the exempt zone), fall back to
    // the raw grid path; if even that crosses, the walk is infeasible.
    let final_pts = if walk_crosses_play(&smooth, segs) {
        if walk_crosses_play(&pts, segs) {
            return None;
        }
        pts
    } else {
        smooth
    };
    let len = poly_len(&final_pts);
    if len > WALK_CURVED_MAX {
        return None;
    }
    Some((final_pts, len))
}

/// Deterministic A* over the 8-connected macro grid. Returns cells start→goal
/// inclusive. Ties in f-cost break on cell index (BinaryHeap on (bits, idx)).
fn astar(
    grid: &WalkGrid,
    is_blocked: &dyn Fn(usize) -> bool,
    start: usize,
    goal: usize,
) -> Option<Vec<usize>> {
    let spec = &grid.spec;
    let nx = spec.nx as i64;
    let ny = spec.ny as i64;
    let n = spec.len();
    let cell = spec.cell_size;
    let diag = cell * std::f64::consts::SQRT_2;

    let center = |i: usize| spec.world_of((i as i64 % nx) as u32, (i as i64 / nx) as u32);
    let goal_p = center(goal);

    let mut g = vec![f64::INFINITY; n];
    let mut came = vec![u32::MAX; n];
    let mut open: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
    g[start] = 0.0;
    open.push(Reverse((center(start).distance(goal_p).to_bits(), start as u32)));

    const NBR: [(i64, i64); 8] = [
        (0, -1),
        (-1, 0),
        (1, 0),
        (0, 1),
        (-1, -1),
        (1, -1),
        (-1, 1),
        (1, 1),
    ];

    while let Some(Reverse((fbits, ci))) = open.pop() {
        let c = ci as usize;
        if c == goal {
            let mut path = vec![c];
            let mut cur = c;
            while came[cur] != u32::MAX {
                cur = came[cur] as usize;
                path.push(cur);
            }
            path.reverse();
            return Some(path);
        }
        // Stale entry?
        let f_here = f64::from_bits(fbits);
        if f_here > g[c] + center(c).distance(goal_p) + 1e-9 {
            continue;
        }
        let cx = c as i64 % nx;
        let cy = c as i64 / nx;
        for (k, &(dx, dy)) in NBR.iter().enumerate() {
            let x = cx + dx;
            let y = cy + dy;
            if x < 0 || y < 0 || x >= nx || y >= ny {
                continue;
            }
            let m = (y * nx + x) as usize;
            if m != goal && is_blocked(m) {
                continue;
            }
            let step = if k < 4 { cell } else { diag };
            let cost = step * (1.0 + 2.0 * grid.slope[m]);
            let ng = g[c] + cost;
            if ng < g[m] {
                g[m] = ng;
                came[m] = c as u32;
                let f = ng + center(m).distance(goal_p);
                open.push(Reverse((f.to_bits(), m as u32)));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use golf_terrain::{generate_course_with, macro_spec};

    /// Mild, dry, explicit-param terrain: the corridor tests assert ROUTER
    /// behavior at fixed coordinates, so they must not depend on what the
    /// current seed sampler draws there (a p(θ) re-fit is not a router change).
    fn test_course() -> golf_terrain::CourseTerrain {
        generate_course_with(
            &macro_spec(),
            4242,
            &golf_terrain::TerrainParams::default(),
            &golf_terrain::ErosionParams::default(),
            &golf_terrain::WaterParams::NONE,
        )
    }

    #[test]
    fn walk_routes_around_a_corridor() {
        let ct = test_course();
        let grid = WalkGrid::new(&ct);
        // A play segment lying between from/to forces a detour (short enough
        // that the detour stays under WALK_CURVED_MAX).
        let segs = vec![(Vec2::new(800.0, 870.0), Vec2::new(800.0, 1130.0))];
        let corridor = corridor_mask(&grid.spec, &segs);
        let from = Vec2::new(700.0, 1000.0);
        let to = Vec2::new(900.0, 1000.0);
        let (pts, len) = find_walk(&grid, &corridor, &segs, from, to).expect("walkable");
        assert!(!walk_crosses_play(&pts, &segs), "must not cross the line of play");
        // The curved distance must exceed the straight line (it detoured).
        assert!(len > from.distance(to) + 50.0, "detour length {len}");
        // Deterministic.
        let (pts2, len2) = find_walk(&grid, &corridor, &segs, from, to).unwrap();
        assert_eq!(pts.len(), pts2.len());
        assert_eq!(len.to_bits(), len2.to_bits());
    }

    #[test]
    fn short_hop_beside_own_green_is_allowed() {
        let ct = test_course();
        let grid = WalkGrid::new(&ct);
        // Play line ends at (1000,1000); the walk starts there (green) and
        // moves 60 m away — the exemption must let it leave the corridor zone.
        let segs = vec![(Vec2::new(700.0, 1000.0), Vec2::new(1000.0, 1000.0))];
        let corridor = corridor_mask(&grid.spec, &segs);
        let from = Vec2::new(1000.0, 1000.0);
        let to = Vec2::new(1050.0, 1040.0);
        let (pts, _len) = find_walk(&grid, &corridor, &segs, from, to).expect("walkable");
        assert!(!walk_crosses_play(&pts, &segs));
    }
}
