//! The site planner: `CourseSpec` params → [`MacroPlan`] → [`MacroConfig`].
//!
//! ALL `macro/place/v1` randomness is drawn here, in the fixed order below —
//! reordering, adding, or removing a draw is a `MACRO_VERSION` bump + golden
//! re-bless. The planner never sees the archetype id (ARCHITECTURE.md
//! invariant 5): archetype character emerges from `landform.*` knob values.
//!
//! DRAW ORDER (per plan() call):
//!   frame:   grain_u, tilt_jitter_u, curve_u, valley_count_round_u
//!   trunk:   side_u, offset_u, meander_seed, wall_asym_l_u, wall_asym_r_u
//!   trib i:  join_s_u, side_flip_u, angle_u, length_u, meander_seed,
//!            fall_u, wall_asym_l_u, wall_asym_r_u
//!   bench:   count_round_u, then per scarp: pos_u, bow_u, face_u, height_u
//!   ridges:  count_round_u, then per ridge: 4×(cx_u, cy_u), prominence_u,
//!            axis_u, len_u, bow_u, flank_l_u, flank_r_u
//!   basins:  count_round_u, then per cluster: 3×(cx_u, cy_u); then per
//!            basin: 6×(rad_u, ang_u), radius_u, depth_u, wobble_u,
//!            cycles_u, blob_seed(u32), ecc_u, lake_u
//!
//! The core budget solve ([`crate::budget`]) draws nothing.
//!
//! Placement doctrine — "routable but interesting" (steps/03 doc): gentle
//! features (creek corridors, low benches, dune grain) are WELCOME in the
//! routable core; steep features (scarps, high ridges, deep kettles) cross
//! it only with their height eased down over the crossing span (a
//! [`CoreDip`]), and the budget solve enforces the cap afterwards.

use std::f64::consts::{FRAC_PI_2, PI};

use course_seed::{streams, DetRng};
use course_spec::CourseSpec;
use course_world::ease::smoothstep;
use course_world::math::{atan2, cos, sin, Vec2};
use course_world::noise::perlin_ring;
use course_world::profile::Profile;
use course_world::spline::{catmull_rom, Spine};
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};

use crate::config::{MacroConfig, Path, Tilt};
use crate::network;
use crate::plan::{
    BasinPlan, BowlPlan, CoreBudget, CoreDip, DrainagePlan, Edge, Frame, MacroPlan, RidgePlan,
    ScarpPlan, UplandsPlan,
};
use crate::prims::bluff::Bluff;
use crate::prims::bowl::{Bowl, BowlBoundary, Outlet};
use crate::prims::meander::MeanderSpec;
use crate::prims::valley::{Ridge, Valley, MIN_FALL_GRADIENT};

/// `valley_dist_m` value when no drain spine exists (the legal empty case).
pub const NO_DRAIN_DIST_M: f64 = 10_000.0;

const BOX_CENTER: Vec2 = Vec2::new(1500.0, 1500.0);

/// Hillslope convexity used for every grown reach.
///
/// A real hillslope is convex over its upper half — creep moves soil in
/// proportion to slope and a divide is a no-flux boundary — and that
/// convexity is most of why 41% of a real piedmont tile classifies as
/// geomorphon RIDGE while a straight-walled version classifies at 24%. A
/// plane is "slope", not "ridge", however it is composed.
const WALL_CONVEXITY: f64 = 0.5;

/// Channel-width exponent. `landform.chan_hw_area_exp` is QUARANTINED (its
/// fitted sign is wrong — sandhills came out at -0.34, i.e. channels
/// narrowing downstream), so hydraulic geometry's value is used instead.
const CHAN_HW_AREA_EXP: f64 = 0.40;

/// Smooth-min blend band at a reach FLOOR, metres of elevation.
///
/// `smin` dips ~k/4 where two surfaces coincide, so every tributary mouth
/// scours a small hollow in its trunk — negligible with three mouths, worth
/// bounding with dozens.
///
/// Worth recording HOW this number was arrived at, because the first attempt
/// was wrong. Shrinking the band was tried while the trunk was still being
/// undercut by the regional PLANE (which fell faster than the trunk and sank
/// below its floor near the box edge); the change made the violation worse,
/// which correctly falsified junction scour as the dominant cause. Only once
/// the grade was tied to the trunk's own slope-area gradient did the
/// remaining dip move inside the box and become junction scour in fact —
/// and only then did shrinking the band help.
///
/// Except it did not: retried at 2.0 with the grade fixed, the violation went
/// 2.00 -> 2.04 m. So junction scour is NOT the mechanism at u=0.25 either,
/// and this stays at 3.0, its better value. TWO plausible mechanisms have now
/// been falsified by measurement; the remaining 2.04 m against a 2.0 m
/// tolerance is genuinely undiagnosed. `examples/diag_dip.rs` attributes a
/// dip to the valley responsible — start there, at u=0.25, and do not touch
/// the tolerance, which is the thing measuring the defect.
const REACH_SHOULDER_K_M: f64 = 3.0;

/// Probe lattice edge for the hillslope-lift percentile. Fixed, so the lift
/// is a deterministic function of the network rather than of resolution.
const LIFT_PROBE: usize = 120;

/// Point-to-segment distance (the lift probe's inner loop).
fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 <= 1e-12 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}
/// Spine margin outside the box so drains visibly exit it.
const EDGE_MARGIN_M: f64 = 120.0;

// ---------------------------------------------------------------------------
// small helpers
// ---------------------------------------------------------------------------

fn stoch_round(rng: &mut DetRng, x: f64) -> usize {
    let x = x.max(0.0);
    let f = x.floor();
    let frac = x - f;
    f as usize + usize::from(rng.next_f64() < frac)
}

fn rot(v: Vec2, a: f64) -> Vec2 {
    let (c, s) = (cos(a), sin(a));
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

/// Intersect the line `c + t*d` with the box grown by `margin`; returns the
/// upstream (`-t`) and downstream (`+t`) hits.
fn line_box_span(c: Vec2, d: Vec2, margin: f64) -> (Vec2, Vec2) {
    let (lo, hi) = (-margin, EXTENT_M + margin);
    let mut tmin = f64::NEG_INFINITY;
    let mut tmax = f64::INFINITY;
    for (c0, d0) in [(c.x, d.x), (c.y, d.y)] {
        if d0.abs() < 1e-9 {
            continue;
        }
        let t1 = (lo - c0) / d0;
        let t2 = (hi - c0) / d0;
        let (a, b) = if t1 <= t2 { (t1, t2) } else { (t2, t1) };
        tmin = tmin.max(a);
        tmax = tmax.min(b);
    }
    (c + d * tmin, c + d * tmax)
}

pub(crate) fn in_core_shoulder(p: Vec2, shoulder: f64) -> bool {
    p.x >= CORE_MIN_M - shoulder
        && p.x <= CORE_MAX_M + shoulder
        && p.y >= CORE_MIN_M - shoulder
        && p.y <= CORE_MAX_M + shoulder
}

/// Normalized-arc spans of a spine whose FOOTPRINT touches the core: a
/// station counts when it lies within `reach` of the core window (a ridge or
/// scarp spills relief sideways by roughly height/gradient, so the spine
/// need not enter the core to raise it). Fixed 200-station sampling.
fn core_cross_spans(spine: &Spine, reach: f64) -> Vec<(f64, f64)> {
    let n = 200usize;
    let mut spans = Vec::new();
    let mut start: Option<f64> = None;
    for k in 0..=n {
        let u = k as f64 / n as f64;
        let inside = in_core_shoulder(spine.point_at(u), reach);
        match (inside, start) {
            (true, None) => start = Some(u),
            (false, Some(s)) => {
                spans.push((s, u));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        spans.push((s, 1.0));
    }
    spans
}

/// Multiply `base` by the core dip: 1 outside the spans, `dip` inside, eased
/// over ±0.04 of normalized arc. Emitted as a dense 61-knot profile.
fn apply_dip(base: &Profile, cd: &CoreDip) -> Profile {
    if cd.spans.is_empty() || cd.dip >= 0.999 {
        return base.clone();
    }
    let e = 0.04;
    let n = 60usize;
    let mut knots = Vec::with_capacity(n + 1);
    for k in 0..=n {
        let u = k as f64 / n as f64;
        let mut factor: f64 = 1.0;
        for &(a, b) in &cd.spans {
            let cov = smoothstep(a - e, a + e, u) * (1.0 - smoothstep(b - e, b + e, u));
            factor = factor.min(1.0 - (1.0 - cd.dip) * cov);
        }
        knots.push((u, base.sample(u) * factor));
    }
    Profile::new(knots)
}


/// Does every drain centerline sit entirely outside the core-protect field?
///
/// Measured on the RESOLVED spines, which are what actually carve. The
/// authored paths are NOT a safe stand-in: junction snapping trims a
/// tributary's tail and overshoots it across the trunk, and that moved a
/// moraine tributary onto (672.5, 2398.9) — inside the shoulder corner by a
/// hair — while every authored point was clear.
///
/// Cheap despite the resolve: it runs once per plan, against the ~35 lowers
/// the bisection does. The flag changes no geometry, only whether the solve
/// may weight valleys, so lowering with it false to compute it is sound.
///
/// 801 stations here against the 401 that `contract::
/// relaxed_valleys_never_feel_the_core` checks, so the predicate is
/// strictly the more conservative of the two samplings.
fn resolved_floors_clear_of_core(plan: &MacroPlan) -> bool {
    let r = crate::resolve(&lower(plan));
    (0..r.n_valleys()).all(|i| {
        let sp = r.valley_spine(i);
        (0..=800).all(|k| crate::fields::core_protect_at(sp.point_at(k as f64 / 800.0)) <= 0.0)
    })
}

fn min_dist_to_spines(p: Vec2, spines: &[Spine]) -> f64 {
    let mut d = f64::INFINITY;
    for s in spines {
        d = d.min(s.project(p).d);
    }
    d
}

// ---------------------------------------------------------------------------
// plan()
// ---------------------------------------------------------------------------

/// Build the staged plan from the spec. Deterministic: same spec → same plan.
pub fn plan(spec: &CourseSpec) -> MacroPlan {
    let lf = |k: &str| spec.param(&format!("landform.{k}"));
    let mut rng = spec.identity.stream(streams::MACRO_PLACE);

    let relief = lf("relief_amp_m");
    let cap = lf("core_relief_cap_m");
    let grade = lf("tilt_grade");

    // ---- stage 1: frame -------------------------------------------------
    let grain_az = 0.06 + rng.next_f64() * (PI - 0.12);
    let tilt_az = grain_az + FRAC_PI_2 + (rng.next_f64() - 0.5) * 0.6;
    let downhill = Vec2::new(cos(tilt_az), sin(tilt_az));
    let curve_m = (rng.next_f64() - 0.5) * 0.15 * relief;
    let n_valleys = stoch_round(&mut rng, lf("valley_count"));
    let closed_basin_only = n_valleys == 0;
    let exit_edge = if closed_basin_only {
        None
    } else {
        let mut best = (Edge::East, f64::NEG_INFINITY);
        for e in [Edge::West, Edge::East, Edge::South, Edge::North] {
            let d = e.outward().dot(downhill);
            if d > best.1 {
                best = (e, d);
            }
        }
        Some(best.0)
    };
    // The authored tilt (floor anchors + rim elevations reference this; the
    // EFFECTIVE grade may be lower once the bench cascade absorbs relief).
    let authored_tilt = Tilt {
        core_tread: 1.0,
        grade_x: -grade * downhill.x,
        grade_y: -grade * downhill.y,
        curve_m,
    };
    let base_elev = 100.0;
    let tilt_at = |p: Vec2| authored_tilt.eval(p, base_elev, EXTENT_M);

    // ---- stage 2: drainage ----------------------------------------------
    let mut valleys: Vec<Valley> = Vec::new();
    let mut tangent_routed = false;
    let mut trunk_incision = 0.0;
    // Raised above the valley envelope in network mode; 0 on the
    // closed-basin path, where the tilted plane IS the base surface.
    let mut hillslope_lift = 0.0;
    let mut is_network = false;
    // Regional grade. In network mode this is OVERWRITTEN by the trunk's own
    // slope-area gradient — see the drainage stage.
    let mut tilt_grade_eff = grade;
    if n_valleys > 0 {
        let hw = lf("valley_halfwidth_m");
        let wall = lf("valley_wall_grade");
        let fall = lf("valley_fall_grad").max(grade * 1.05);
        let intensity = lf("meander_intensity");
        let wavelength_mult = lf("meander_wavelength_mult");
        // `valley_wall_grade` is a whole-hillslope MEAN. Feeding it in as the
        // NEAR-CHANNEL gradient of a convex profile makes the whole hillslope
        // too gentle — convexity then decays downward from the mean — so every
        // channel sits in a wide corridor of gentle ground that classifies as
        // slope rather than ridge, and ridge coverage is capped no matter what
        // else is tuned. A profile whose gradient decays to `1 - c` of its
        // channel value averages `channel * (1 - c/2)`, so invert that.
        let share = (0.6 * cap).max(1.2);
        // ...and the core must have ROOM for one. A network's hillslope
        // relief is `wall_grade x (half the channel spacing)`, and spacing
        // follows from the density target: at 5 km/km^2 channels sit ~200 m
        // apart, so divides stand ~100 m of hillslope above their channels.
        // If that alone exceeds the cap's drainage share, the archetype has
        // no room to be dissected and a network would be over-cap by
        // construction, before any solve runs.
        //
        // This is what florida needs. It is identity-zero on the
        // `valley_count` family, so every drainage knob it has is a POOLED
        // provisional inherited from archetypes with real networks — and a
        // flatwoods with a 4.2 m cap was growing 149 piedmont-ish reaches
        // implying 23 m of hillslope. The criterion is knob-driven (never
        // archetype id): an archetype whose fitted wall grade and cap leave
        // room gets a network, one that does not keeps its lone creek.
        let spacing_half = 1000.0 / (2.0 * lf("drainage_density_fine").max(0.5));
        let implied_hillslope = (wall / (1.0 - 0.5 * WALL_CONVEXITY)) * spacing_half;
        is_network = n_valleys >= 2 && implied_hillslope <= share;

        // Every one of these is NETWORK shaping and must be scoped to it. A
        // lone creek given a convex wall, a 33% steeper channel gradient and
        // a 60 m crest band carves a vast smooth corridor instead of a creek:
        // measured, a single florida trunk produced 11.3 m of core relief
        // against a 4.2 m cap. Convexity and a wide crest band only make
        // sense where two walls MEET at a divide, which is a property of a
        // network, not of one channel.
        let convexity = if is_network { WALL_CONVEXITY } else { 0.0 };
        let wall_channel = (wall / (1.0 - 0.5 * convexity)).clamp(0.02, 0.8);
        let crest_rise = wall_channel * 135.0;
        let crest_k = if is_network {
            Some((6.0 * crest_rise).clamp(10.0, 60.0))
        } else {
            None
        };

        // Trunk placement — the routability-vs-interest rule. A creek that
        // crosses the core spends core relief TWICE: once on the depth it
        // cuts below the local surface, and again on how far its floor falls
        // over the ~1.5 km crossing. Both come out of the same cap, so they
        // are budgeted together against one share.
        //
        // They used to be budgeted separately — incision capped at 0.6·cap
        // AND the crossing allowed up to 0.6·cap — which let the planner
        // author cores at 1.2·cap by construction. The old solve hid that by
        // scaling `tilt_grade_eff` globally, which also scaled every valley's
        // fall through `t_ratio`; now that the solve is core-localized the
        // double-count has nowhere to go and shows up as an over-cap plan.
        let cross_drop = fall * 1500.0;
        // Tangent when the crossing alone leaves no room for even a minimal
        // creek: the creek skirts the core ring instead, where core-edge
        // holes still play along it.
        tangent_routed = cross_drop + 1.2 > share;
        let incision = (0.25 * relief + 2.0).min(if tangent_routed {
            share
        } else {
            (share - cross_drop).max(1.2)
        });
        trunk_incision = incision;
        let (off_lo, off_hi) = if tangent_routed {
            (1150.0, 1350.0)
        } else {
            (150.0, 600.0)
        };
        let side_sign = if rng.next_f64() < 0.5 { -1.0 } else { 1.0 };
        let d_off = side_sign * (off_lo + (off_hi - off_lo) * rng.next_f64());
        let chord_c = BOX_CENTER + downhill.perp() * d_off;
        let (entry, exit) = line_box_span(chord_c, downhill, EDGE_MARGIN_M);
        let meander_seed = rng.next_u64();
        let asym_l = 0.7 + 0.7 * rng.next_f64();
        let asym_r = 0.7 + 0.7 * rng.next_f64();
        let width = (2.0 * hw + 2.0 * incision / wall.max(0.05)).clamp(40.0, 600.0);
        let trunk = Valley {
            path: Path::Meander(MeanderSpec {
                entry,
                exit,
                width_m: width,
                intensity,
                wavelength_mult,
                jitter: 0.3,
                seed: meander_seed,
            }),
            floor_z0_m: 0.0, // set below, once the hillslope lift is known
            fall_gradient: fall,
            floor_halfwidth: Profile::constant(hw),
            wall_grad_left: wall_channel * asym_l,
            wall_grad_right: wall_channel * asym_r,
            floor_round_m: (0.2 * hw).clamp(2.0, 12.0),
            shoulder_k_m: REACH_SHOULDER_K_M,
            crest_k_m: crest_k,
            crest_rise_m: crest_rise,
            wall_convexity: convexity,
            join_trunk: None,
            fall_profile: None,
        };
        let trunk_spine = trunk.path.to_spine();

        // ---- grow the network ------------------------------------------
        //
        // Not "place N tributaries": GROW, by farthest-point insertion, until
        // the box carries the drainage density the corpus measures. The old
        // construction drew one trunk plus `valley_count - 1` tributaries all
        // joining it directly — a depth-1 star — and that is why the median
        // cell sat 145-590 m from a channel against a real 104-123 m, and why
        // the drainage-density gap WIDENED as the accumulation threshold
        // dropped (the signature of a network with no low-order reaches).
        //
        // The FINE threshold is the target, not the nominal one: growing to
        // 2.25 km/km^2 leaves the interfluves bare at `ridge_mask_area_frac`
        // 0.297, and growing to the fitted fine density lands 0.412 against a
        // real 0.408. Interfluves are not placed anywhere in this file — they
        // are what the envelope leaves between channels.
        // A NETWORK needs at least two drainage lines. `valley_count` counts
        // connected components, and its median is 0 for the closed-basin
        // archetypes — but `stoch_round` of a median-0 knob still yields 1
        // occasionally, and that single draw was enough to grow a 149-reach
        // network on a florida flatwoods seed, using the POOLED provisional
        // network gradients it inherits from the identity-zero family. Those
        // are piedmont-ish, and a network's floors stack accordantly with
        // depth, so the headwaters ended up metres above the trunk on a tile
        // whose real core relief is 1.9 m.
        //
        // One drawn valley is a lone creek, not a network: growth target 0,
        // no hillslope lift, and the regional plane stays the base surface
        // exactly as before. Knob-driven, never archetype id.
        let box_km2 = (EXTENT_M / 1000.0) * (EXTENT_M / 1000.0);
        let stations: Vec<Vec2> =
            (0..=48).map(|i| trunk_spine.point_at(i as f64 / 48.0)).collect();
        let reaches = network::grow(&network::GrowthSpec {
            trunk: stations,
            target_len_m: if is_network {
                lf("drainage_density_fine").max(0.5) * box_km2 * 1000.0
            } else {
                0.0
            },
            junction_angle_deg: lf("junction_angle_deg"),
            branch_len_m: lf("branch_len_m").max(120.0),
            margin_m: 60.0,
            min_head_dist_m: None,
        });

        // ---- the hillslope lift ----------------------------------------
        //
        // Valley surfaces rise without bound and compose by smooth-min, so
        // the composed surface IS the lower envelope of every valley: a
        // distance-to-channel hillslope field with rounded divides, which is
        // the shape real fluvial terrain has (fitting `z = z(nearest channel)
        // + g*dist` explains R^2 0.87 moraine / 0.78 piedmont of a real tile,
        // against 0.33 / 0.56 for that tile's own best-fit plane).
        //
        // It produced nothing before only because the regional plane sat a
        // few metres above the floors, so smin kept the PLANE everywhere
        // except in the valley corridors — a plane with grooves in it.
        // Lifting the plane above the envelope is what makes the interfluves
        // emerge. p90 of the divide distance, NOT the max: keying off the
        // single worst-served corner let one point set the height of the
        // whole plane and left broad smooth domes wherever the network was
        // thin.
        let mut dists: Vec<f64> = Vec::with_capacity(LIFT_PROBE * LIFT_PROBE);
        for iy in 0..LIFT_PROBE {
            for ix in 0..LIFT_PROBE {
                let q = Vec2::new(
                    (ix as f64 + 0.5) * EXTENT_M / LIFT_PROBE as f64,
                    (iy as f64 + 0.5) * EXTENT_M / LIFT_PROBE as f64,
                );
                let mut d = f64::INFINITY;
                for r in &reaches {
                    for w in r.pts.windows(2) {
                        d = d.min(seg_dist(q, w[0], w[1]));
                    }
                }
                dists.push(d);
            }
        }
        dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
        // No lift without a network: with a lone creek the p90 "divide"
        // distance is most of the box, and lifting the plane by that would
        // invent relief out of nothing.
        if is_network {
            hillslope_lift = wall_channel * dists[dists.len() * 9 / 10];
        }

        // ---- lower the reaches -----------------------------------------
        let sinuosity = lf("meander_sinuosity").max(1.0);
        let theta = lf("slope_area_theta").clamp(0.05, 0.9);
        // The slope-area intercept, budgeted against the SAME cap share the
        // trunk's incision comes out of.
        //
        // A network spends core relief in two ways: the depth it cuts, and
        // how far its floors fall across the core. The second was budgeted
        // for the old single trunk (`cross_drop`) but not for a grown
        // network, whose trunk gradient comes from the slope-area law rather
        // than `valley_fall_grad`. Left unbudgeted it is the one term no
        // solve lever can reach — the core weight scales the wall RISE and so
        // is exactly zero at a floor, by design.
        //
        // It bites hardest where the network knobs are pooled provisionals
        // rather than the archetype's own: florida is identity-zero on the
        // `valley_count` family, so it inherits piedmont-ish gradients that
        // are far too steep for a flatwoods, and a 0.0154 intercept becomes
        // 0.036 on a headwater reach. Scaling the INTERCEPT preserves the
        // law's shape (theta untouched) — which is what a pivot-centred
        // parameterisation is for.
        let s0_raw = lf("net_fall_at_a0").max(MIN_FALL_GRADIENT);
        let trunk_fall_raw = network::slope_at(reaches[0].area_km2, s0_raw, theta);
        let fall_budget = (share - 1.2).max(0.5) / 1500.0;
        let s0 = if trunk_fall_raw > fall_budget {
            (s0_raw * fall_budget / trunk_fall_raw).max(MIN_FALL_GRADIENT)
        } else {
            s0_raw
        };
        let hw0 = lf("chan_hw_at_a0_m").max(2.0);
        // The trunk floor is RE-ANCHORED in `lower()` against the effective
        // tilt (which now carries the lift), so whatever is stored here is
        // overwritten. Kept at 0 rather than duplicating the formula in two
        // places that could drift apart.
        // THE REGIONAL GRADE IS THE MASTER STREAM'S GRADE.
        //
        // The hillslope lift offsets the plane above the valley envelope by a
        // constant, so the two must FALL AT THE SAME RATE or the offset only
        // holds at one end of the tile. They did not: `tilt_grade` is 0.0144
        // for piedmont while the trunk's slope-area gradient at the box's
        // 9 km^2 is 0.0092, so the plane outran the trunk and sank below its
        // floor downstream. That showed up as a drainage-monotonicity
        // violation — the composed height along the trunk dipped 8.6 m below
        // the trunk's own surface at u=0.945, with no valley anywhere near
        // it, because `smin` was keeping the PLANE.
        //
        // Reading the two independently was measuring one physical quantity
        // twice: at the largest scale a landscape's grade IS its master
        // stream's. Deriving it here removes the contradiction and leaves
        // `tilt_grade` shaping only the closed-basin path.
        if is_network {
            tilt_grade_eff = network::slope_at(reaches[0].area_km2, s0, theta);
        }
        valleys.push(trunk);
        for (i, r) in reaches.iter().enumerate().skip(1) {
            let al = 0.7 + 0.7 * rng.next_f64();
            let ar = 0.7 + 0.7 * rng.next_f64();
            let rhw = network::halfwidth_at(r.area_km2, hw0, CHAN_HW_AREA_EXP);
            valleys.push(Valley {
                path: Path::Points(network::wander(&r.pts, sinuosity, i)),
                // accordant: overwritten by the join_trunk snap at resolve
                floor_z0_m: 0.0,
                fall_gradient: network::slope_at(r.area_km2, s0, theta),
                floor_halfwidth: Profile::constant(rhw),
                wall_grad_left: wall_channel * al,
                wall_grad_right: wall_channel * ar,
                floor_round_m: (0.35 * rhw).clamp(2.0, 12.0),
                shoulder_k_m: REACH_SHOULDER_K_M,
                crest_k_m: crest_k,
                crest_rise_m: crest_rise,
                wall_convexity: convexity,
                join_trunk: r.parent,
                fall_profile: None,
            });
        }
    }
    // Placement-scoring spines (raw, pre-join — good enough for distances).
    let drain_spines: Vec<Spine> = valleys.iter().map(|v| v.path.to_spine()).collect();

    // ---- stage 3a: bench cascade ----------------------------------------
    // Excess regional drop is converted into discrete scarp steps with flat
    // treads (the terraces ARE the interest); the core center keeps a
    // scarp-free tread so the core stays routable. This is how a steep
    // archetype passes the core cap without flattening the whole map.
    let n_bench_knob = stoch_round(&mut rng, lf("bench_count"));
    let scarp_h_knob = lf("bench_scarp_h_m").max(1.0);
    let tread_w = lf("bench_tread_w_m").max(150.0);
    let mut scarps: Vec<ScarpPlan> = Vec::new();
    if n_bench_knob > 0 {
        // Two derivations, because the premise differs by mode.
        //
        // Closed-basin: the regional plane IS the surface, so a steep grade
        // genuinely has excess drop to convert into terraces, and the cascade
        // absorbs it — that is how a steep archetype passes the core cap.
        //
        // Network: the tilt no longer shapes anything (the envelope wins
        // everywhere once the lift clears it — measured, changing the grade
        // 0.0144 -> 0.0092 altered the output by nothing at all), so "excess
        // regional drop" has no referent. Benches then come straight from the
        // knobs the campaign fitted for them. Mountain needs this to stay
        // itself: its hillslope-field R^2 is only 0.36, i.e. it is
        // bench-dominated rather than fluvial, and the scarps ride on top of
        // the network envelope through the same additive step as before.
        let (n_scarps, h_each) = if n_valleys > 0 {
            (n_bench_knob, scarp_h_knob)
        } else {
            tilt_grade_eff = grade.min(0.4 * cap / 1500.0);
            let excess = ((grade - tilt_grade_eff) * EXTENT_M).max(0.0);
            let n = n_bench_knob.max((excess / (2.0 * scarp_h_knob)).ceil() as usize);
            (n, if excess > 0.5 { excess / n as f64 } else { scarp_h_knob })
        };
        // Scarp stations along the downhill axis, keeping |t| < half_clear
        // free (the through-core tread).
        let half_clear = (tread_w * 0.5).max(250.0);
        let band = 1250.0 - half_clear;
        for k in 0..n_scarps {
            let a = (k as f64 + 0.35 + 0.3 * rng.next_f64()) / n_scarps as f64;
            let s_lin = a * 2.0 * band;
            let t_pos = if s_lin < band {
                -1250.0 + s_lin
            } else {
                half_clear + (s_lin - band)
            };
            let center = BOX_CENTER + downhill * t_pos;
            let along = downhill.perp();
            let (a_pt, b_pt) = line_box_span(center, along, EDGE_MARGIN_M);
            // Bow the scarp, but never into the reserved core tread band.
            let bow_room = ((t_pos.abs() - half_clear - 30.0).max(0.0)).min(150.0);
            let bow = (rng.next_f64() - 0.5) * 2.0 * bow_room;
            let mid = a_pt.lerp(b_pt, 0.5) + downhill * bow;
            let face = 0.35 + 0.4 * rng.next_f64();
            let h = h_each * (0.8 + 0.4 * rng.next_f64());
            let pts = vec![a_pt, mid, b_pt];
            let spine = Spine::new(catmull_rom(&pts, 10.0));
            // Footprint reach: the eased face extends ~h/face_grad sideways.
            let spans = core_cross_spans(&spine, h / face + 50.0);
            let dip = ((0.3 * cap) / h).min(1.0);
            scarps.push(ScarpPlan {
                bluff: Bluff {
                    path: Path::Points(pts),
                    height_m: h,
                    height: Profile::constant(1.0),
                    face_grad: face,
                    taper_frac: 0.02,
                    // path runs along perp(downhill); its left side is the
                    // uphill side — terraces step DOWN in the downhill dir.
                    raise_left: true,
                },
                core: CoreDip { spans, dip },
            });
        }
    }

    // ---- stage 3b: ridges (interfluve rule) ------------------------------
    let n_ridges = stoch_round(&mut rng, lf("ridge_count"));
    let ridge_len_knob = lf("ridge_len_m").max(100.0);
    let crest_hw_knob = lf("ridge_crest_hw_m").max(4.0);
    let mut ridges: Vec<RidgePlan> = Vec::new();
    let mut placed_centers: Vec<Vec2> = Vec::new();
    for _ in 0..n_ridges {
        // Four candidate centers; keep the one farthest from drains (and
        // spread from already-placed ridges). If the ridge would be tall,
        // prefer distance from the core center too.
        let mut cands = [Vec2::ZERO; 4];
        for c in &mut cands {
            *c = Vec2::new(
                350.0 + 2300.0 * rng.next_f64(),
                350.0 + 2300.0 * rng.next_f64(),
            );
        }
        let prominence = relief * (0.45 + 0.3 * rng.next_f64());
        let mut best = (cands[0], f64::NEG_INFINITY);
        for c in cands {
            let dd = if drain_spines.is_empty() {
                1500.0
            } else {
                min_dist_to_spines(c, &drain_spines)
            };
            let ds = placed_centers
                .iter()
                .map(|q| q.distance(c))
                .fold(f64::INFINITY, f64::min);
            let mut score = dd.min(0.8 * ds);
            if prominence > 0.75 * cap {
                score += 0.5 * c.distance(BOX_CENTER);
            }
            if score > best.1 {
                best = (c, score);
            }
        }
        let c = best.0;
        placed_centers.push(c);
        let axis = grain_az + (rng.next_f64() - 0.5) * 0.35;
        let len = ridge_len_knob * (0.75 + 0.5 * rng.next_f64());
        let axd = Vec2::new(cos(axis), sin(axis));
        let clamp_pt = |p: Vec2| Vec2::new(
            p.x.clamp(100.0, EXTENT_M - 100.0),
            p.y.clamp(100.0, EXTENT_M - 100.0),
        );
        let mut p0 = clamp_pt(c - axd * (len * 0.5));
        let mut p1 = clamp_pt(c + axd * (len * 0.5));
        // upstream (higher-terrain) endpoint first, so fall_gradient >= 0
        if p0.dot(downhill) > p1.dot(downhill) {
            std::mem::swap(&mut p0, &mut p1);
        }
        let hw_var = rng.next_f64();
        let bow = (rng.next_f64() - 0.5) * 0.2 * len;
        let mid = p0.lerp(p1, 0.5) + (p1 - p0).normalized().perp() * bow;
        let flank = (prominence / (0.18 * len)).clamp(0.10, 0.55);
        let fl = flank * (0.8 + 0.4 * rng.next_f64());
        let fr = flank * (0.8 + 0.4 * rng.next_f64());
        // Crest halfwidth is measured, not derived from length: real crests
        // range from ~8 m (piedmont interfluves) to ~90 m (dune ridges), and
        // deriving it from length made every ridge the same 45 m-flat mesa.
        let hw_c = (crest_hw_knob * (0.75 + 0.5 * rng.next_f64())).clamp(6.0, 140.0);
        let dir01 = (p1 - p0).normalized();
        let pts = vec![p0, mid, p1];
        let spine = Spine::new(catmull_rom(&pts, 10.0));
        // Footprint reach: flanks descend at ~flank grade from the crest.
        let reach = prominence / fl.min(fr).max(0.05) + hw_c + 50.0;
        let spans = core_cross_spans(&spine, reach);
        let dip = ((0.5 * cap) / prominence).min(1.0);
        ridges.push(RidgePlan {
            ridge: Ridge {
                path: Path::Points(pts),
                crest_z0_m: tilt_at(p0) + prominence,
                fall_gradient: (grade * dir01.dot(downhill)).max(0.0),
                crest_halfwidth: Profile::new(vec![
                    (0.0, 0.45 * hw_c),
                    (0.28, 0.95 * hw_c),
                    (0.5, hw_c * (0.8 + 0.35 * hw_var)),
                    (0.72, 0.9 * hw_c),
                    (1.0, 0.45 * hw_c),
                ]),
                flank_grad_left: fl,
                flank_grad_right: fr,
                // Rounding scaled to the crest, not fixed: a 12 m radius on a
                // 45 m halfwidth leaves a 90 m plateau (the mesa look). At
                // ~0.9x halfwidth the crest reads as a rounded ridge.
                crest_round_m: (0.9 * hw_c).clamp(4.0, 60.0),
                base_k_m: 6.0,
                emphasis: Profile::new(vec![(0.0, 0.0), (0.18, 1.0), (0.82, 1.0), (1.0, 0.0)]),
            },
            prominence_m: prominence,
            core: CoreDip { spans, dip },
        });
    }

    // ---- stage 4: closed basins (kettles / blowouts / lakes-to-be) -------
    let n_basins = stoch_round(&mut rng, lf("basin_count"));
    let spacing = lf("basin_spacing_m").max(50.0);
    let b_depth = lf("basin_depth_m").max(0.5);
    let b_rad = lf("basin_radius_m").max(15.0);
    let ecc_knob = lf("blowout_eccentricity");
    let mut bowls: Vec<BowlPlan> = Vec::new();
    if n_basins > 0 {
        let n_clusters = (1 + n_basins / 5).min(3);
        let mut clusters: Vec<Vec2> = Vec::new();
        for _ in 0..n_clusters {
            let mut cands = [Vec2::ZERO; 3];
            for c in &mut cands {
                *c = Vec2::new(
                    500.0 + 2000.0 * rng.next_f64(),
                    500.0 + 2000.0 * rng.next_f64(),
                );
            }
            let mut best = (cands[0], f64::NEG_INFINITY);
            for c in cands {
                let dd = if drain_spines.is_empty() {
                    1500.0
                } else {
                    min_dist_to_spines(c, &drain_spines)
                };
                let dc = clusters
                    .iter()
                    .map(|q| q.distance(c))
                    .fold(f64::INFINITY, f64::min);
                let score = dd.min(0.7 * dc);
                if score > best.1 {
                    best = (c, score);
                }
            }
            clusters.push(best.0);
        }
        let mut accepted: Vec<(Vec2, f64)> = Vec::new(); // (center, radius)
        for b in 0..n_basins {
            let cl = clusters[b % n_clusters];
            // Six candidate darts (all drawn — fixed draw count), first
            // acceptable one wins; none acceptable → this basin is skipped.
            let mut darts = [Vec2::ZERO; 6];
            for d in &mut darts {
                let r = spacing * (0.4 + 1.8 * rng.next_f64());
                let a = 2.0 * PI * rng.next_f64();
                *d = cl + Vec2::new(cos(a), sin(a)) * r;
            }
            let radius = b_rad * (0.7 + 0.6 * rng.next_f64());
            let depth = b_depth * (0.7 + 0.6 * rng.next_f64());
            let wobble = 0.12 + 0.12 * rng.next_f64();
            let cycles = 1.0 + 2.5 * rng.next_f64();
            let blob_seed = rng.next_u32();
            let ecc = ecc_knob * (0.7 + 0.6 * rng.next_f64());
            let lake_u = rng.next_f64();
            let pos = darts.into_iter().find(|p| {
                p.x >= 200.0
                    && p.x <= EXTENT_M - 200.0
                    && p.y >= 200.0
                    && p.y <= EXTENT_M - 200.0
                    && accepted.iter().all(|(q, _)| q.distance(*p) >= spacing)
                    && (drain_spines.is_empty()
                        || min_dist_to_spines(*p, &drain_spines) >= 60.0 + radius)
            });
            let Some(center) = pos else { continue };
            accepted.push((center, radius));
            // Blowouts elongate along the structural grain; near-circular
            // basins use the cheaper Blob boundary.
            let boundary = if ecc > 0.05 {
                let n = 96usize;
                let mut pts = Vec::with_capacity(n);
                for k in 0..n {
                    let th = 2.0 * PI * k as f64 / n as f64;
                    let (cs, sn) = (cos(th), sin(th));
                    let r = radius
                        * (1.0 + ecc * cos(2.0 * (th - grain_az)))
                        * (1.0 + wobble * perlin_ring(cs, sn, cycles, blob_seed));
                    pts.push(Vec2::new(center.x + r * cs, center.y + r * sn));
                }
                BowlBoundary::Points(pts)
            } else {
                BowlBoundary::Blob {
                    center,
                    radius_m: radius,
                    wobble,
                    cycles,
                    seed: blob_seed,
                }
            };
            let outlet = if closed_basin_only || lake_u < 0.4 {
                Outlet::Lake
            } else {
                // Spillway on the downhill side of the rim.
                let phi = atan2(downhill.y, downhill.x);
                let at_s = (phi / (2.0 * PI)).rem_euclid(1.0);
                Outlet::Spillway {
                    at_s,
                    halfwidth_m: (0.5 * radius).max(30.0),
                    depth_m: 0.8 * depth,
                }
            };
            // The rim reaches ~1.5 radii out (wobble + eccentricity), so a
            // basin centered outside the core can still deepen it.
            let in_core = in_core_shoulder(center, 1.5 * radius + 150.0);
            let depth_scale = if in_core {
                ((0.4 * cap) / depth).min(1.0)
            } else {
                1.0
            };
            bowls.push(BowlPlan {
                center,
                bowl: Bowl {
                    boundary,
                    rim_z_m: tilt_at(center) - 0.15 * depth,
                    depth_m: depth,
                    inner_grad: 0.22,
                    rim_round_m: (0.12 * radius).clamp(4.0, 15.0),
                    floor_k_m: 3.0,
                    outer_grad: 0.30,
                    blend_k_m: 2.5,
                    outlet,
                },
                in_core,
                depth_scale,
            });
        }
    }

    let mut plan = MacroPlan {
        frame: Frame {
            // The hillslope lift rides in the base elevation, so every
            // absolute anchor `lower()` re-hangs on the effective tilt —
            // trunk floor, bowl rims, scarp toes — moves with it for free.
            base_elev_m: base_elev + hillslope_lift,
            grain_az_rad: grain_az,
            downhill,
            tilt_grade: grade,
            tilt_grade_eff,
            // The budget solve's knobs, in their as-authored position.
            core_tread: 1.0,
            core_relax: 1.0,
            curve_m,
            exit_edge,
            closed_basin_only,
            dune_wavelength_m: lf("dune_wavelength_m"),
        },
        drainage: DrainagePlan {
            floors_clear_of_core: false,
            incision_scale: 1.0,
            is_network,
            valleys,
            tangent_routed,
            trunk_incision_m: trunk_incision,
        },
        uplands: UplandsPlan {
            ridges,
            scarps,
            core_tread_w_m: if n_bench_knob > 0 { tread_w } else { 0.0 },
        },
        basins: BasinPlan { bowls },
        budget: CoreBudget {
            cap_m: cap,
            ..CoreBudget::default()
        },
    };
    // Needs the lowered config, so it runs on the assembled plan. The flag
    // changes no geometry — only whether the solve may weight valleys.
    plan.drainage.floors_clear_of_core = resolved_floors_clear_of_core(&plan);
    crate::budget::solve(&mut plan);
    plan
}

// ---------------------------------------------------------------------------
// lowering
// ---------------------------------------------------------------------------

/// Lower the plan to the analytic config. Pure — the viewer and the artifact
/// round-trip both call this.
/// Lower the plan to the analytic config. Every absolute elevation the
/// planner authored against the tilt (ridge crests, bowl rims, the trunk's
/// entry floor) is RE-ANCHORED here to the EFFECTIVE tilt: the budget solve
/// scales `tilt_grade_eff`, and anchors that kept authored-tilt elevations
/// would strand features at stale absolute heights — relief the solve could
/// never remove (the florida pilot failure mode).
pub fn lower(plan: &MacroPlan) -> MacroConfig {
    let f = &plan.frame;
    let tilt = Tilt {
        // The budget solve's tread. Because `tilt_eff_at` below is built
        // from THIS tilt, every absolute anchor (ridge crests, bowl rims,
        // the trunk's entry floor) re-resolves against the treaded surface
        // automatically — anchors that kept pure-plane elevations would
        // strand features above a flattened core, which is the failure the
        // re-anchoring rule exists to prevent.
        core_tread: f.core_tread,
        grade_x: -f.tilt_grade_eff * f.downhill.x,
        grade_y: -f.tilt_grade_eff * f.downhill.y,
        curve_m: f.curve_m,
    };
    let tilt_eff_at = |p| tilt.eval(p, f.base_elev_m, EXTENT_M);
    // Valleys were authored to fall parallel to the AUTHORED tilt; when the
    // budget solve scales the tilt, an unscaled fall would carve an
    // ever-deepening canyon across the flattened base (walls sweeping into
    // the core). Scale fall by the same ratio, floored at the monotone
    // minimum — cut depth stays ≈ incision everywhere, exactly as authored.
    // NOT applied in network mode. `t_ratio` exists for the closed-basin
    // path, where the bench cascade lowers the effective grade and valley
    // falls authored against the AUTHORED grade would otherwise carve an
    // ever-deepening canyon across a flattened base. In network mode the
    // falls come from the slope-area law and the effective grade is DERIVED
    // from the trunk, so the ratio is not a correction — it is a second,
    // contradictory rescaling. It measured 0.64 on piedmont, which slowed
    // the trunk below the plane and let the plane sit 16.7 m under its own
    // floor mid-box.
    let t_ratio = if plan.drainage.is_network {
        1.0
    } else if f.tilt_grade > 1e-12 {
        (f.tilt_grade_eff / f.tilt_grade).clamp(0.0, 1.0)
    } else {
        1.0
    };
    MacroConfig {
        schema_version: crate::SCHEMA_VERSION,
        extent_m: EXTENT_M,
        base_elev_m: f.base_elev_m,
        core_relax: f.core_relax,
        core_relax_valleys: plan.drainage.floors_clear_of_core,
        ridges: plan
            .uplands
            .ridges
            .iter()
            .map(|rp| {
                let (p0, p1) = path_endpoints(&rp.ridge.path);
                let dir01 = (p1 - p0).normalized();
                Ridge {
                    // Crest height ABOVE the local effective surface;
                    // emphasis keeps end tapers + the core dip.
                    crest_z0_m: tilt_eff_at(p0) + rp.prominence_m,
                    fall_gradient: (f.tilt_grade_eff * dir01.dot(f.downhill)).max(0.0),
                    emphasis: apply_dip(&rp.ridge.emphasis, &rp.core),
                    ..rp.ridge.clone()
                }
            })
            .collect(),
        bluffs: plan
            .uplands
            .scarps
            .iter()
            .map(|sp| Bluff {
                // Additive step — no absolute anchor to re-resolve.
                height: apply_dip(&sp.bluff.height, &sp.core),
                ..sp.bluff.clone()
            })
            .collect(),
        bowls: plan
            .basins
            .bowls
            .iter()
            .map(|bp| {
                let depth_eff = bp.bowl.depth_m * bp.depth_scale;
                Bowl {
                    rim_z_m: tilt_eff_at(bp.center) - 0.15 * depth_eff,
                    depth_m: depth_eff,
                    outlet: match bp.bowl.outlet {
                        Outlet::Spillway { at_s, halfwidth_m, depth_m } => Outlet::Spillway {
                            at_s,
                            halfwidth_m,
                            depth_m: depth_m * bp.depth_scale,
                        },
                        Outlet::Lake => Outlet::Lake,
                    },
                    ..bp.bowl.clone()
                }
            })
            .collect(),
        valleys: plan
            .drainage
            .valleys
            .iter()
            .enumerate()
            .map(|(i, v)| {
                // `incision_scale` shallows the whole drain system, not just
                // the trunk anchor. A tributary's depth below the terrain is
                // EMERGENT — its floor is accordance-snapped to the trunk and
                // then rises upstream by its own cumulative drop, so a steep
                // tributary (fall is drawn at 1.4-2.6x the trunk's) cuts as
                // deep as it likes with nothing budgeting it. That is the one
                // remaining way a plan can blow the core cap, so the drain
                // scale has to reach it. Still monotone: the floor clamps at
                // MIN_FALL_GRADIENT, so it keeps falling.
                let fall = (v.fall_gradient * t_ratio * plan.drainage.incision_scale)
                    .max(MIN_FALL_GRADIENT);
                if i == 0 {
                    let entry = match &v.path {
                        Path::Meander(m) => m.entry,
                        Path::Points(pts) => pts[0],
                    };
                    Valley {
                        // Incision is placement-capped (never scaled); only
                        // the anchor tracks the effective tilt. Tributary
                        // floors are join_trunk-snapped at resolve time.
                        floor_z0_m: tilt_eff_at(entry)
                            - plan.drainage.trunk_incision_m
                                * plan.drainage.incision_scale,
                        fall_gradient: fall,
                        ..v.clone()
                    }
                } else {
                    Valley {
                        fall_gradient: fall,
                        ..v.clone()
                    }
                }
            })
            .collect(),
        tilt,
    }
}

/// First/last control point of a path (meanders: the trend endpoints).
fn path_endpoints(path: &Path) -> (Vec2, Vec2) {
    match path {
        Path::Meander(m) => (m.entry, m.exit),
        Path::Points(pts) => (pts[0], *pts.last().unwrap()),
    }
}

