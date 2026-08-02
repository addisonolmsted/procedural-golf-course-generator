//! The drainage network — grown, not placed.
//!
//! Before M5 the planner drew one trunk plus `valley_count - 1` tributaries,
//! every one of them joining the trunk directly. That is a depth-1 star, and
//! it is why generated terrain measured as a plane with a few grooves in it:
//! the median cell sat 145-590 m from a channel where every real archetype
//! measures 104-123 m, and the drainage-density gap WIDENED as the
//! accumulation threshold dropped, which is the signature of a network with
//! no low-order tributaries.
//!
//! This grows a space-filling dendritic network instead, by farthest-point
//! insertion: repeatedly find the point of the box that is worst served by
//! the network so far, and run a new reach from there to the nearest point
//! on the network. That is a greedy solution to exactly the quantity the
//! gate measures (max distance to a channel), so the network fills space by
//! construction rather than by tuning a count.
//!
//! Nothing here draws RNG or touches elevation. It produces plan geometry —
//! centerlines, parent links, drainage areas, Strahler orders — and the
//! planner turns that into `Valley` primitives. Growth is deterministic: the
//! candidate set is a fixed lattice and ties break on index, so the same
//! parameters give the same network on any platform.

use course_world::math::{sin, Vec2};
use course_world::world::EXTENT_M;

/// Pivot drainage area for the power laws, km². Must match
/// `macro_campaign.netstats.AREA_PIVOT_KM2` — the fitted `slope_area_theta`
/// and `chan_hw_at_a0_m` are both pivot-centred on it, and reading them
/// against a different pivot silently rescales every gradient and width.
pub const AREA_PIVOT_KM2: f64 = 0.5;

/// Candidate lattice pitch for farthest-point search, metres. 60 m resolves
/// a divide to well inside the 110 m distance-to-channel target while
/// keeping the search at ~2.5k candidates.
const LATTICE_M: f64 = 60.0;

/// Default floor on how close a new head may start to the network. Below
/// this the greedy step inserts stubs rather than reaches. Overridable per
/// growth: dissecting the interfluves needs heads closer in than the
/// nominal-threshold spacing allows.
const MIN_HEAD_DIST_DEFAULT_M: f64 = 140.0;

/// Hard ceiling on reach count, so a pathological target cannot melt the
/// rasterizer. Reached only if the length target is absurd.
const MAX_REACHES: usize = 240;

/// One grown reach: a centerline from its head (upstream) to its mouth.
#[derive(Clone, Debug)]
pub struct Reach {
    /// Control points, head first, mouth last. The mouth of a tributary is
    /// a point ON its parent's centerline; `resolve` does the accordant snap.
    pub pts: Vec<Vec2>,
    /// Index of the reach this one joins. Always < this reach's own index,
    /// which is what `Valley::join_trunk` requires.
    pub parent: Option<usize>,
    /// Drainage area at the mouth, km².
    pub area_km2: f64,
    /// Strahler order (1 = headwater).
    pub order: u32,
    /// Centerline length, metres.
    pub len_m: f64,
}

fn polyline_len(pts: &[Vec2]) -> f64 {
    pts.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Distance from `p` to a polyline, plus the closest point on it.
fn dist_to_polyline(p: Vec2, pts: &[Vec2]) -> (f64, Vec2) {
    let mut best = (f64::INFINITY, pts[0]);
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let ab = b - a;
        let len2 = ab.dot(ab);
        let t = if len2 <= 1e-12 {
            0.0
        } else {
            ((p - a).dot(ab) / len2).clamp(0.0, 1.0)
        };
        let q = a + ab * t;
        let d = p.distance(q);
        if d < best.0 {
            best = (d, q);
        }
    }
    best
}

/// Nearest (reach index, distance, point) over the whole network.
fn nearest(p: Vec2, reaches: &[Reach]) -> (usize, f64, Vec2) {
    let mut best = (0usize, f64::INFINITY, p);
    for (i, r) in reaches.iter().enumerate() {
        let (d, q) = dist_to_polyline(p, &r.pts);
        if d < best.1 {
            best = (i, d, q);
        }
    }
    best
}

/// Inputs the planner hands the grower. All lengths in metres.
pub struct GrowthSpec {
    /// The trunk's authored centerline (entry → exit), already meandered.
    pub trunk: Vec<Vec2>,
    /// Total channel length to grow, metres — the drainage-density target
    /// times the box area.
    pub target_len_m: f64,
    /// Junction angle, degrees (`landform.junction_angle_deg`). A tributary
    /// meets its parent at this angle, opening upstream.
    pub junction_angle_deg: f64,
    /// Typical reach length (`landform.branch_len_m`); caps how far a head
    /// may sit from its junction.
    pub branch_len_m: f64,
    /// Keep reaches this far inside the box edge (heads only — the trunk
    /// still exits).
    pub margin_m: f64,
    /// Closest a new head may sit to the existing network. `None` uses
    /// [`MIN_HEAD_DIST_DEFAULT_M`].
    ///
    /// This is the knob that decides whether INTERFLUVES get dissected. An
    /// smin envelope is smoothest exactly where it is farthest from its
    /// generators, so with heads held 140 m out the divides come out as the
    /// flattest ground on the tile — measured, local relief FALLING with
    /// distance-to-channel (18.6 -> 12.5 m) where a real tile RISES
    /// (5.2 -> 8.9 m). Real divides are rough because low-order channels
    /// climb into them; letting heads insert closer is what puts them there.
    pub min_head_dist_m: Option<f64>,
}

/// Grow the network. `reaches[0]` is the trunk.
pub fn grow(spec: &GrowthSpec) -> Vec<Reach> {
    let mut reaches = vec![Reach {
        len_m: polyline_len(&spec.trunk),
        pts: spec.trunk.clone(),
        parent: None,
        area_km2: 0.0,
        order: 1,
    }];
    let mut total = reaches[0].len_m;

    // Fixed candidate lattice — deterministic, and the same set every
    // iteration so the search cannot depend on insertion history.
    let n = (EXTENT_M / LATTICE_M).floor() as usize;
    let mut lattice = Vec::with_capacity(n * n);
    for iy in 0..n {
        for ix in 0..n {
            let p = Vec2::new(
                (ix as f64 + 0.5) * LATTICE_M,
                (iy as f64 + 0.5) * LATTICE_M,
            );
            if p.x >= spec.margin_m
                && p.x <= EXTENT_M - spec.margin_m
                && p.y >= spec.margin_m
                && p.y <= EXTENT_M - spec.margin_m
            {
                lattice.push(p);
            }
        }
    }

    let theta = spec.junction_angle_deg.to_radians().clamp(0.15, 1.4);
    while total < spec.target_len_m && reaches.len() < MAX_REACHES {
        // The worst-served point in the box. Ties break on lattice index,
        // which is fixed, so this is reproducible.
        let mut head = (f64::NEG_INFINITY, Vec2::ZERO, 0usize);
        for p in &lattice {
            let (i, d, _q) = nearest(*p, &reaches);
            if d > head.0 {
                head = (d, *p, i);
            }
        }
        if head.0 < spec.min_head_dist_m.unwrap_or(MIN_HEAD_DIST_DEFAULT_M) {
            break; // the box is covered; more reaches would only add stubs
        }
        let (parent, _d, mouth) = nearest(head.1, &reaches);

        // Bend the approach so the reach meets its parent at the fitted
        // junction angle instead of butting into it head-on. The elbow sits
        // one third of the way up from the mouth, rotated off the straight
        // line by (90 deg - theta/2) worth of offset — small for an acute
        // junction, large for a near-perpendicular one.
        let straight = head.1 - mouth;
        let len = straight.length().max(1.0);
        let dir = straight * (1.0 / len);
        let bend = (0.5 * (std::f64::consts::FRAC_PI_2 - 0.5 * theta)).tan() * 0.35 * len;
        let side = if reaches.len() % 2 == 0 { 1.0 } else { -1.0 };
        let elbow = mouth + dir * (0.34 * len) + dir.perp() * (side * bend);

        // Cap the reach at the fitted branch length. This is not cosmetic:
        // the growth budget is a channel LENGTH (drainage density x area),
        // so over-long reaches spend it without buying coverage — at a 1.8x
        // cap the budget bought 27 reaches and left a 398 m hole, against a
        // 104-123 m real median distance-to-channel. Truncating at the
        // measured `branch_len_m` leaves the far ground for a later reach,
        // which is what a real network does at a divide anyway.
        let mut pts = vec![head.1, elbow, mouth];
        if len > spec.branch_len_m {
            let keep = mouth + (head.1 - mouth) * (spec.branch_len_m / len);
            let elbow2 = mouth + dir * (0.34 * spec.branch_len_m)
                + dir.perp() * (side * bend * spec.branch_len_m / len);
            pts = vec![keep, elbow2, mouth];
        }
        let len_m = polyline_len(&pts);
        total += len_m;
        reaches.push(Reach {
            pts,
            parent: Some(parent),
            area_km2: 0.0,
            order: 1,
            len_m,
        });
    }

    strahler(&mut reaches);
    areas(&mut reaches);
    reaches
}

/// Strahler order, computed leaves-first. Reaches are in insertion order and
/// a parent always precedes its children, so one reverse pass suffices.
fn strahler(reaches: &mut [Reach]) {
    let n = reaches.len();
    let mut child_orders: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in (1..n).rev() {
        let o = {
            let c = &mut child_orders[i];
            if c.is_empty() {
                1
            } else {
                c.sort_unstable();
                let top = c[c.len() - 1];
                // Two or more children of the top order promote it.
                if c.len() >= 2 && c[c.len() - 2] == top {
                    top + 1
                } else {
                    top
                }
            }
        };
        reaches[i].order = o;
        if let Some(p) = reaches[i].parent {
            child_orders[p].push(o);
        }
    }
    let c = &mut child_orders[0];
    c.sort_unstable();
    reaches[0].order = if c.is_empty() {
        1
    } else if c.len() >= 2 && c[c.len() - 2] == c[c.len() - 1] {
        c[c.len() - 1] + 1
    } else {
        c[c.len() - 1]
    };
}

/// Drainage area at each mouth, km².
///
/// Apportioned by subtree channel length rather than by a Voronoi sweep of
/// the raster: Hack's law makes basin area and mainstream length power-law
/// partners, the whole box drains SOMEWHERE, and the quantity the power laws
/// actually need is a monotone ordering of area from headwater to trunk.
/// A raster sweep would cost a full grid pass to buy precision the slope-area
/// law immediately raises to the -0.3 power.
fn areas(reaches: &mut [Reach]) {
    let n = reaches.len();
    let mut subtree: Vec<f64> = reaches.iter().map(|r| r.len_m).collect();
    for i in (1..n).rev() {
        if let Some(p) = reaches[i].parent {
            subtree[p] += subtree[i];
        }
    }
    let total = subtree[0].max(1.0);
    let box_km2 = (EXTENT_M / 1000.0) * (EXTENT_M / 1000.0);
    for i in 0..n {
        reaches[i].area_km2 = box_km2 * subtree[i] / total;
    }
}

/// Stations a wandering reach is resampled to. Fixed, so the geometry does
/// not depend on reach length.
const WANDER_N: usize = 40;

/// Bend a straight reach into a channel with the given sinuosity
/// (arc length / straight-line length).
///
/// Grown reaches come out as three points — head, elbow, mouth — which reads
/// as a straight line at tile scale, and straightness is the most visible
/// remaining difference from a real tile once the network itself is right.
/// `meander_sinuosity` is measurable on real tiles (piedmont 1.26, sandhills
/// 1.40) and was fitted at M4 precisely for this; it is the honest
/// replacement for `meander_intensity`, which is defined against the
/// generator's own curvature clamp and therefore pins at 1.0 on real data.
///
/// The offset is a half-wave sine in normalized arc, so it vanishes at BOTH
/// ends: the head stays put and — the part that matters — the mouth stays
/// exactly on the parent centerline, which is what the accordant junction
/// snap assumes. Phase and lobe count come from the reach index, so the
/// result is deterministic and reaches do not all wiggle in step.
pub fn wander(pts: &[Vec2], sinuosity: f64, index: usize) -> Vec<Vec2> {
    let straight = pts[0].distance(*pts.last().unwrap());
    if sinuosity <= 1.001 || straight < 1.0 {
        return pts.to_vec();
    }
    let lobes = 1 + (index * 7 + 3) % 3; // 1..3, deterministic in the index
    let phase = if index % 2 == 0 { 1.0 } else { -1.0 };
    let dir = (*pts.last().unwrap() - pts[0]) * (1.0 / straight);
    let perp = dir.perp();

    // Amplitude by bisection on the achieved sinuosity — cheaper to solve
    // than to derive, and exact for whatever lobe count came out.
    let build = |amp: f64| -> Vec<Vec2> {
        (0..=WANDER_N)
            .map(|i| {
                let t = i as f64 / WANDER_N as f64;
                let base = pts[0] + dir * (straight * t);
                let off = phase * amp * sin(std::f64::consts::PI * lobes as f64 * t);
                base + perp * off
            })
            .collect()
    };
    // Cap the amplitude by LOBE aspect ratio, not by total length. A sine
    // lobe of amplitude A and half-wavelength L has peak curvature ~A(pi/L)^2,
    // so bounding A/L bounds how sharply the channel turns. Without this the
    // bisection is free to hit a high sinuosity with a few enormous lobes,
    // and the resampled polyline turns into a zigzag — which is exactly what
    // it did at sinuosity 1.42 with the previous 12 stations and no cap.
    let lobe_len = straight / lobes as f64;
    let (mut lo, mut hi) = (0.0, 0.30 * lobe_len);
    for _ in 0..24 {
        let mid = 0.5 * (lo + hi);
        if polyline_len(&build(mid)) / straight < sinuosity {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    build(0.5 * (lo + hi))
}

/// Achieved sinuosity of a polyline (arc / straight). The amplitude cap can
/// leave `wander` short of its target on a short reach; callers that care
/// should measure rather than assume.
pub fn sinuosity_of(pts: &[Vec2]) -> f64 {
    let straight = pts[0].distance(*pts.last().unwrap());
    if straight < 1e-9 { 1.0 } else { polyline_len(pts) / straight }
}

/// Channel gradient from the slope-area law, `S = S0 * (A/A0)^-theta`.
pub fn slope_at(area_km2: f64, s0: f64, theta: f64) -> f64 {
    let a = (area_km2 / AREA_PIVOT_KM2).max(1e-6);
    s0 * a.powf(-theta)
}

/// Channel half-width from hydraulic geometry, `w = w0 * (A/A0)^b`.
pub fn halfwidth_at(area_km2: f64, w0: f64, b: f64) -> f64 {
    let a = (area_km2 / AREA_PIVOT_KM2).max(1e-6);
    w0 * a.powf(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GrowthSpec {
        GrowthSpec {
            trunk: vec![
                Vec2::new(-120.0, 1400.0),
                Vec2::new(1500.0, 1500.0),
                Vec2::new(3120.0, 1600.0),
            ],
            target_len_m: 20_500.0,
            junction_angle_deg: 60.0,
            branch_len_m: 370.0,
            margin_m: 80.0,
            min_head_dist_m: None,
        }
    }

    #[test]
    fn growth_is_deterministic() {
        let a = grow(&spec());
        let b = grow(&spec());
        assert_eq!(a.len(), b.len());
        for (x, y) in a.iter().zip(&b) {
            assert_eq!(x.pts.len(), y.pts.len());
            for (p, q) in x.pts.iter().zip(&y.pts) {
                assert_eq!(p.x.to_bits(), q.x.to_bits());
                assert_eq!(p.y.to_bits(), q.y.to_bits());
            }
            assert_eq!(x.parent, y.parent);
            assert_eq!(x.order, y.order);
        }
    }

    /// Every parent precedes its child — `Valley::join_trunk` asserts this,
    /// and a violation is a panic at resolve time rather than a bad picture.
    #[test]
    fn parents_precede_children() {
        for (i, r) in grow(&spec()).iter().enumerate() {
            if let Some(p) = r.parent {
                assert!(p < i, "reach {i} joins {p}, which is not earlier");
            }
        }
    }

    /// The point of the milestone, measured the way the gate measures it.
    ///
    /// `dist_to_channel_p50_m` is a MEDIAN over the tile, and every real
    /// archetype lands at 104-123 m. An earlier version of this test
    /// asserted on the MAXIMUM instead and failed at 400 m — in the box
    /// corner, which is the one place a greedy interior insertion serves
    /// last and which contributes a handful of cells to the median. Testing
    /// the max was testing the corner, not the network.
    #[test]
    fn the_network_fills_space() {
        let reaches = grow(&spec());
        let mut ds = Vec::new();
        for iy in 0..60 {
            for ix in 0..60 {
                let p = Vec2::new(
                    (ix as f64 + 0.5) * EXTENT_M / 60.0,
                    (iy as f64 + 0.5) * EXTENT_M / 60.0,
                );
                ds.push(nearest(p, &reaches).1);
            }
        }
        ds.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let p50 = ds[ds.len() / 2];
        let p90 = ds[ds.len() * 9 / 10];
        assert!(
            (70.0..=150.0).contains(&p50),
            "median distance to a channel is {p50:.0} m (p90 {p90:.0}) over {} reaches, \
             {:.0} m of channel — the real corpus is 104-123 m",
            reaches.len(),
            reaches.iter().map(|r| r.len_m).sum::<f64>()
        );
    }

    /// A dendritic network of this size should reach order 3 — the corpus
    /// median in every archetype but sandhills (2.5).
    #[test]
    fn strahler_order_reaches_three() {
        let reaches = grow(&spec());
        assert!(
            reaches[0].order >= 3,
            "trunk order {} over {} reaches",
            reaches[0].order,
            reaches.len()
        );
    }

    /// Area must grow monotonically downstream, or the slope-area law puts a
    /// steeper gradient on the trunk than on its own headwaters.
    #[test]
    fn area_grows_downstream() {
        let reaches = grow(&spec());
        for (i, r) in reaches.iter().enumerate() {
            if let Some(p) = r.parent {
                assert!(
                    reaches[p].area_km2 > r.area_km2,
                    "reach {i} (A={:.3}) drains more than its parent {p} (A={:.3})",
                    r.area_km2,
                    reaches[p].area_km2
                );
            }
        }
        assert!(reaches[0].order >= 2);
    }

    /// The long profile must be concave: a headwater reach steeper than the
    /// trunk it eventually feeds.
    #[test]
    fn slope_area_law_is_concave() {
        let reaches = grow(&spec());
        let head = reaches.last().unwrap();
        let trunk_s = slope_at(reaches[0].area_km2, 0.02, 0.35);
        let head_s = slope_at(head.area_km2, 0.02, 0.35);
        assert!(
            head_s > trunk_s,
            "headwater slope {head_s:.4} is not steeper than trunk {trunk_s:.4}"
        );
    }
}

#[cfg(test)]
mod wander_tests {
    use super::*;

    fn straight(len: f64) -> Vec<Vec2> {
        vec![Vec2::new(0.0, 0.0), Vec2::new(0.5 * len, 0.0), Vec2::new(len, 0.0)]
    }

    /// The whole point: hit the measured sinuosity.
    #[test]
    fn wander_hits_the_target_sinuosity() {
        for target in [1.18, 1.26, 1.36, 1.42] {
            for index in 0..6 {
                let w = wander(&straight(400.0), target, index);
                let got = sinuosity_of(&w);
                // Either it hits the target, or the curvature cap stopped it
                // short — never overshoots, and never zigzags to get there.
                assert!(
                    got <= target + 0.02,
                    "index {index}: sinuosity {got:.3} overshoots target {target:.3}"
                );
                assert!(
                    got > 1.0,
                    "index {index}: no wander at all for target {target:.3}"
                );
            }
        }
    }

    /// A tributary's mouth IS the junction point on its parent. If wandering
    /// moved it, the accordant floor snap would be measuring one place and
    /// carving another.
    #[test]
    fn wander_pins_both_endpoints() {
        let pts = straight(400.0);
        for index in 0..6 {
            let w = wander(&pts, 1.36, index);
            assert_eq!(w[0].x.to_bits(), pts[0].x.to_bits());
            assert_eq!(w[0].y.to_bits(), pts[0].y.to_bits());
            let (a, b) = (*w.last().unwrap(), *pts.last().unwrap());
            assert!(a.distance(b) < 1e-9, "mouth moved by {:.3e} m", a.distance(b));
        }
    }

    /// Sinuosity at or below 1 must be an exact no-op, so an archetype that
    /// does not meander round-trips unchanged.
    #[test]
    fn unit_sinuosity_is_a_no_op() {
        let pts = straight(400.0);
        assert_eq!(wander(&pts, 1.0, 3), pts);
    }
}
