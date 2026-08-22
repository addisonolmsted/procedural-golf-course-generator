//! T2 — the tier-2 catena cuts: each tributary carved INTO the macro
//! surface, cut-only.
//!
//! The envelope rule of the macro holds here too: a cut can only LOWER the
//! surface (`min`-composition), so double-cutting a trunk valley is
//! unrepresentable — where the trib's notch target sits above the ground it
//! simply does nothing. The trunk floors are protected by construction: the
//! trib bed starts AT the trunk floor and rises monotonically, so its target
//! never dips below the floor it departs from.
//!
//! Where a trib climbs a valley-wall scarp, the monotone bed lags the
//! surface and the notch deepens into a ravine through the bluff — this is
//! the intended breakup of the long bluff runs (user decision, commit
//! 984b810: rarity comes from the tributary network, not sparser gates).

use course_network::Trib;
use course_world::grid::Grid;
use course_world::math;

/// Tier-2 notch geometry. Flat floor half-width, then a straight side
/// slope up to the local depth. Scaled-down catena, not a program: tier
/// cuts are texture-adjacent and the corpus zone stats don't resolve them.
const FHW_M: f64 = 8.0;
const SIDE_SLOPE: f64 = 0.30;
/// Shoulder softening band: the cut fades in over this much excess, so the
/// envelope boundary is C1 (no new member of the seam family).
const SOFT_M: f64 = 3.0;
/// Minimum bed climb per metre of arc — keeps the bed strictly monotone
/// (no ponds) without forcing it up walls faster than a real ravine grade.
const BED_MIN_GRADE: f64 = 0.0005;

/// Carve every tributary. `depth_cap` scales with the tile's relief budget:
/// deep ravines in hill country, shallow swales on a river-valley tread.
pub fn carve(height: &mut Grid<f64>, tribs: &[Trib], relief_budget_m: f64) {
    if tribs.is_empty() {
        return;
    }
    let cap = (0.10 * relief_budget_m).clamp(1.5, 6.0);
    let reach = FHW_M + cap / SIDE_SLOPE + SOFT_M;
    let spec = height.spec;
    let nn = spec.nx as usize;
    let res = spec.cell_size;

    // Pass 1: the min notch target per cell, rasterized per segment so the
    // cost is proportional to channel length, not tile area.
    let mut target = vec![f64::MAX; nn * nn];
    for tb in tribs {
        if tb.pts.len() < 2 {
            continue;
        }
        // Dense resample of the axis (6 m): the walk's 24 m spacing left
        // the bed piecewise-linear against a curving wall surface, and the
        // depth oscillated at the sampling period — ribbed notches on every
        // steep wall.
        let mut pts: Vec<math::Vec2> = Vec::new();
        for w in tb.pts.windows(2) {
            let seg_len = w[0].distance(w[1]);
            let k = (seg_len / 6.0).ceil().max(1.0) as usize;
            for j in 0..k {
                let f = j as f64 / k as f64;
                pts.push(math::Vec2::new(
                    w[0].x + (w[1].x - w[0].x) * f,
                    w[0].y + (w[1].y - w[0].y) * f,
                ));
            }
        }
        pts.push(*tb.pts.last().unwrap());
        let n = pts.len();
        // --- the bed: surface minus a head-tapering depth, monotone from
        // the mouth. Smoothed along arc so a scarp crossing reads as a
        // rounded knickpoint, not a pixel step in the notch floor.
        let mut arcs = Vec::with_capacity(n);
        let mut acc = 0.0;
        arcs.push(0.0);
        for i in 1..n {
            acc += pts[i].distance(pts[i - 1]);
            arcs.push(acc);
        }
        let total = acc.max(1.0);
        let surf: Vec<f64> = pts.iter().map(|q| height.bilinear(*q)).collect();
        let mut bed = Vec::with_capacity(n);
        let mut b = surf[0];
        bed.push(b);
        for i in 1..n {
            let taper = math::pow(1.0 - (arcs[i] / total).min(1.0), 0.6);
            let raw = surf[i] - cap * taper;
            let step = arcs[i] - arcs[i - 1];
            b = raw.max(b + BED_MIN_GRADE * step);
            bed.push(b);
        }
        for _ in 0..2 {
            let mut sm = bed.clone();
            for i in 1..n - 1 {
                sm[i] = (bed[i - 1] + 2.0 * bed[i] + bed[i + 1]) * 0.25;
            }
            // the mouth stays pinned to the trunk floor
            sm[0] = bed[0];
            bed = sm;
        }
        // A channel must stay INCISED the whole way: where the path crosses
        // a local surface dip the monotone bed rises above ground and the
        // notch dashed in and out (beaded look, hc gate). The floor is the
        // LOWER of the monotone bed and a constant shave below the local
        // surface — continuous by construction; the small closed dips this
        // allows in the notch floor are texture-stage hydrology's to re-cut.
        let min_inc: Vec<f64> = (0..n)
            .map(|i| {
                let taper = math::pow(1.0 - (arcs[i] / total).min(1.0), 0.6);
                let head_in = math::smoothstep(0.0, 120.0, arcs[i]);
                surf[i] - (0.35 * cap * taper).max(0.6) * head_in
            })
            .collect();

        // --- rasterize each segment's notch section into the target grid
        for i in 0..n - 1 {
            let (p0, p1) = (pts[i], pts[i + 1]);
            let (b0, b1) = (bed[i].min(min_inc[i]), bed[i + 1].min(min_inc[i + 1]));
            let lo_x = ((p0.x.min(p1.x) - reach) / res).floor().max(0.0) as usize;
            let hi_x = ((p0.x.max(p1.x) + reach) / res).ceil().min(nn as f64 - 1.0) as usize;
            let lo_y = ((p0.y.min(p1.y) - reach) / res).floor().max(0.0) as usize;
            let hi_y = ((p0.y.max(p1.y) + reach) / res).ceil().min(nn as f64 - 1.0) as usize;
            let seg = math::Vec2::new(p1.x - p0.x, p1.y - p0.y);
            let len2 = (seg.x * seg.x + seg.y * seg.y).max(1e-9);
            for gy in lo_y..=hi_y {
                for gx in lo_x..=hi_x {
                    let p = spec.world_of(gx as u32, gy as u32);
                    let tt = (((p.x - p0.x) * seg.x + (p.y - p0.y) * seg.y) / len2)
                        .clamp(0.0, 1.0);
                    let cx = p0.x + seg.x * tt;
                    let cy = p0.y + seg.y * tt;
                    let dx = p.x - cx;
                    let dy = p.y - cy;
                    let d = (dx * dx + dy * dy).sqrt();
                    if d > reach {
                        continue;
                    }
                    let tz = b0 + (b1 - b0) * tt + SIDE_SLOPE * (d - FHW_M).max(0.0);
                    let li = gy * nn + gx;
                    if tz < target[li] {
                        target[li] = tz;
                    }
                }
            }
        }
    }

    // Pass 2: apply, cut-only, with a C1 shoulder. Full excess is removed
    // once it exceeds SOFT_M; below that the cut eases in, so the envelope
    // boundary never prints a crease.
    for li in 0..nn * nn {
        let tz = target[li];
        if tz == f64::MAX {
            continue;
        }
        let h = height.data[li];
        let excess = h - tz;
        if excess <= 0.0 {
            continue;
        }
        height.data[li] = h - excess * math::smoothstep(0.0, SOFT_M, excess);
    }
}
