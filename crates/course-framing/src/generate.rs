//! The stage-01 generator: `CourseSpec` → `Framing`. Pure, infallible,
//! microseconds; no archetype or `hydrology_mode` branch anywhere
//! (ARCHITECTURE.md invariant 5) — every per-archetype difference arrives
//! through `framing.*` knob values.
//!
//! # The draw transcript (order-sensitive)
//!
//! The base fields come from the STABLE `framing/v1` stream as a FIXED
//! 14-uniform transcript. Draws 7–14 (province-boundary geometry) happen
//! unconditionally and are discarded when `provinces.count == 1`, so every
//! draw's position is outcome-independent. Reordering, adding, or removing
//! a draw is a `FRAMING_VERSION` bump + golden re-bless:
//!
//! ```text
//!  1  window        weighted pick over WindowClass::ALL (framing.w_*)
//!  2  edge          weighted pick over EdgeId::ALL (code-constant weights)
//!  3  tilt jitter   dir off the edge's outward direction, ≤ TILT_TOL_RAD
//!  4  grain spread  axis off tilt+π/2, ± framing.grain_spread_rad
//!  5  province      count = 2 iff u < framing.province_p
//!  6  kind          weighted pick over BoundaryKind::ALL (framing.k_*)
//!  7  entry         chord anchor: perpendicular offset from center, ±600 m
//!  8  skew          chord axis skew, ±0.15 rad
//!  9–14             3 control points × (lateral wobble, along-chord jitter)
//! ```
//!
//! v2 APPENDS four unconditional `next_u64` SUB-SEED draws, one per skeleton
//! family, in this order: trunk, branch, ridge, step. Each family then runs
//! its own `DetRng::new(sub_seed, b"framing/<family>/v1")` — derived RNGs,
//! not registry streams. This isolates the families: changing how ridges are
//! generated can never shift the trunk's draws. Because the sub-seeds are
//! appended AFTER draw 14 and the base fields consume no new θ knobs, every
//! v1 base field keeps its exact value at v2 (pinned by the
//! `base_fields_survive_v2` test).
//!
//! Per-family draw discipline: each family draws a FIXED budget sized by its
//! cap (`MAX_*`), using the first `n` results — the archive's "always draw
//! all candidates" rule, so a count change cannot shift later draws within
//! the family. Bounded loops are legal inside a family but none are needed;
//! the trunk's amplitude search is a draw-free bisection.

use course_seed::{streams, DetRng};
use course_spec::CourseSpec;
use course_world::ease::end_taper;
use course_world::math::{atan2, cos, sin, Vec2};
use course_world::noise::perlin1;
use course_world::spline::{catmull_rom, Spine};
use course_world::world::EXTENT_M;

use crate::framing::{
    BaseLevel, Boundary, BoundaryKind, EdgeId, Framing, Grain, Provinces, Skeleton, Tilt, Trunk,
    WindowClass, FRAMING_VERSION, MAX_BRANCHES, MAX_RIDGES, MAX_STEPS,
};

/// Stated contract tolerance: `regional_tilt.dir_rad` is within this angle
/// of `base_level.edge`'s outward direction (stage-01 hard requirement 1).
pub const TILT_TOL_RAD: f64 = std::f64::consts::FRAC_PI_4;

/// Outlet-edge weights in `EdgeId::ALL` order: cardinals twice as likely as
/// corners. Code constants, not knobs — no archetype differentiates on the
/// outlet edge today; promote to `framing.*` knobs the day a stage doc does.
const EDGE_WEIGHTS: [f64; 8] = [1.0, 1.0, 1.0, 1.0, 0.5, 0.5, 0.5, 0.5];

/// Chord anchor max perpendicular offset from the box center (m). Keeps the
/// anchor strictly inside the box, so the chord always crosses it.
const CHORD_OFFSET_MAX_M: f64 = 600.0;
/// Chord axis skew range (rad) — boundaries are not perfectly straight
/// family-wide.
const CHORD_SKEW_RAD: f64 = 0.15;
/// How far past the box each end of the curve extends (m) — enters-and-exits
/// by construction.
const EDGE_MARGIN_M: f64 = 60.0;
/// Along-chord jitter of the interior control points, as a fraction of the
/// chord length.
const ALONG_JITTER_FRAC: f64 = 0.05;
/// Arc-length resample step for the boundary curve (m).
const CURVE_STEP_M: f64 = 25.0;

impl BoundaryKind {
    /// Lateral wobble of the interior control points (m): a scarp is the
    /// straightest structure, a lithologic contact the loosest.
    fn wobble_m(self) -> f64 {
        match self {
            BoundaryKind::Scarp => 90.0,
            BoundaryKind::ValleyWall => 150.0,
            BoundaryKind::MaterialContact => 220.0,
        }
    }
}

/// Generate the stage-01 artifact. Infallible: a validated `CourseSpec`
/// guarantees every `framing.*` knob exists (a missing knob is a loud
/// `CourseSpec::param` panic, not a silent default).
pub fn generate(spec: &CourseSpec) -> Framing {
    let mut rng = spec.identity.stream(streams::FRAMING);
    let mut uniform = || rng.next_f64();

    // 1 — window class.
    let w: Vec<f64> = WindowClass::ALL
        .iter()
        .map(|c| spec.param(c.weight_knob()))
        .collect();
    let window = WindowClass::ALL[weighted_pick(&w, uniform())];

    // 2 — outlet edge.
    let edge = EdgeId::ALL[weighted_pick(&EDGE_WEIGHTS, uniform())];

    // 3 — regional tilt: downhill toward the outlet edge, jittered within
    // tolerance. Grade comes straight from θ (sampled by stage 00).
    let jitter = spec.param("framing.tilt_dir_jitter_rad").min(TILT_TOL_RAD);
    let out = edge.outward_dir();
    let tilt_dir = wrap_2pi(atan2(out.y, out.x) + pm1(uniform()) * jitter);
    let regional_tilt = Tilt {
        dir_rad: tilt_dir,
        grade: spec.param("framing.tilt_grade"),
    };

    // 4 — structural grain: an axis near contour-parallel (tilt + π/2),
    // spread by the archetype's looseness knob.
    let grain = Grain {
        dir_rad: wrap_axis(
            tilt_dir + std::f64::consts::FRAC_PI_2
                + pm1(uniform()) * spec.param("framing.grain_spread_rad"),
        ),
        anisotropy: spec.param("framing.grain_anisotropy"),
    };

    // 5 — province count.
    let two_provinces = uniform() < spec.param("framing.province_p");

    // 6 — boundary kind (drawn unconditionally; see module doc).
    let k: Vec<f64> = BoundaryKind::ALL
        .iter()
        .map(|c| spec.param(c.weight_knob()))
        .collect();
    let kind = BoundaryKind::ALL[weighted_pick(&k, uniform())];

    // 7–14 — boundary geometry (drawn unconditionally).
    let entry_u = uniform();
    let skew_u = uniform();
    let jitters: [(f64, f64); 3] = [
        (uniform(), uniform()),
        (uniform(), uniform()),
        (uniform(), uniform()),
    ];
    let boundary = boundary_curve(kind, &regional_tilt, &grain, entry_u, skew_u, &jitters);

    // 15–18 — one sub-seed per skeleton family (always drawn; families are
    // isolated from each other's draw counts).
    let trunk_seed = rng.next_u64();
    let branch_seed = rng.next_u64();
    let ridge_seed = rng.next_u64();
    let step_seed = rng.next_u64();

    let base = BaseCtx {
        window,
        edge,
        tilt: regional_tilt,
        grain,
    };
    let trunk = gen_trunk(trunk_seed, spec, &base);
    let branches = gen_branches(branch_seed, spec, trunk.as_ref());
    let ridges = gen_ridges(ridge_seed, spec, &base);
    let steps = gen_steps(step_seed, spec, &base);

    Framing {
        seed: spec.identity.seed,
        pipeline_version: spec.identity.pipeline_version,
        framing_version: FRAMING_VERSION,
        window,
        base_level: BaseLevel {
            edge,
            elev_m: -spec.param("framing.base_drop_m"),
        },
        regional_tilt,
        grain,
        provinces: if two_provinces {
            Provinces {
                count: 2,
                boundary: Some(boundary),
            }
        } else {
            Provinces {
                count: 1,
                boundary: None,
            }
        },
        skeleton: Skeleton {
            trunk,
            branches,
            ridges,
            steps,
        },
    }
}

/// Base fields the skeleton families read (all already decided).
struct BaseCtx {
    window: WindowClass,
    edge: EdgeId,
    tilt: Tilt,
    grain: Grain,
}

/// Expected-count knobs become integers without biasing the mean:
/// `floor(x)` plus a Bernoulli draw on the fraction.
fn stoch_round(x: f64, u: f64) -> usize {
    if !x.is_finite() || x <= 0.0 {
        return 0;
    }
    let f = x.floor();
    (f as usize) + usize::from(u < x - f)
}

/// Windows whose landscape reads as a terrace flight — the only ones that
/// carry step lines. Window is stage-01 data (not an archetype), so gating
/// on it is legal; every other archetype also has `topo_step_count` 0.
fn window_has_steps(w: WindowClass) -> bool {
    matches!(w, WindowClass::TerraceFlight | WindowClass::EscarpmentFace)
}

/// Angle of a vector — exposed for the artifact's step-axis validation.
pub(crate) fn atan2_of(v: Vec2) -> f64 {
    atan2(v.y, v.x)
}

// ---------------------------------------------------------------------------
// Trunk drainage corridor — `framing/trunk/v1`, 5 draws (fixed)
// ---------------------------------------------------------------------------

/// Arc-length step of every skeleton polyline (m).
const SKELETON_STEP_M: f64 = 25.0;
/// Lateral spread of the trunk's entry/exit anchors from the box center (m).
const TRUNK_ANCHOR_SPREAD_M: f64 = 500.0;
/// Bisection budget for the meander amplitude search (draw-free).
const SINUOSITY_ITERS: usize = 24;
/// Meander amplitude ceiling as a fraction of chord length — stops high
/// sinuosity being bought with a few enormous lobes.
const MEANDER_AMP_FRAC: f64 = 0.30;
/// Low-frequency jitter blended into the meander, at a wavelength
/// deliberately incommensurate with the sine so lobes never repeat exactly.
const MEANDER_JITTER: f64 = 0.35;
const MEANDER_JITTER_WAVE: f64 = 0.71;
/// Minimum bend radius in channel widths, and the post-scale budget.
const MIN_RADIUS_WIDTHS: f64 = 2.5;
const CURVATURE_SCALES: usize = 4;

fn gen_trunk(sub: u64, spec: &CourseSpec, b: &BaseCtx) -> Option<Trunk> {
    let mut rng = DetRng::new(sub, b"framing/trunk/v1");
    let present_u = rng.next_f64();
    let entry_u = rng.next_f64();
    let exit_u = rng.next_f64();
    let phase_u = rng.next_f64();
    let noise_seed = rng.next_u32();

    if present_u >= spec.param("framing.topo_trunk_p") {
        return None;
    }
    let halfwidth_m = spec.param("framing.topo_trunk_halfwidth_m");
    if halfwidth_m <= 0.0 {
        return None;
    }

    // Chord: down the outlet direction, entry and exit laterally offset so
    // the corridor is not always through the middle. Monotone by
    // construction — the chord IS the downhill direction and every lateral
    // displacement is perpendicular to it.
    let down = b.edge.outward_dir();
    let lat = down.perp();
    let center = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
    let entry_anchor = center + lat * (pm1(entry_u) * TRUNK_ANCHOR_SPREAD_M);
    let (t0, _) = line_box_span(entry_anchor, down);
    let entry = entry_anchor + down * (t0 - EDGE_MARGIN_M);
    let exit_anchor = center + lat * (pm1(exit_u) * TRUNK_ANCHOR_SPREAD_M);
    let (_, t1) = line_box_span(exit_anchor, down);
    let exit = exit_anchor + down * (t1 + EDGE_MARGIN_M);

    let chord = exit - entry;
    let len = chord.length();
    if len < 1.0 {
        return None;
    }
    let chord_dir = chord * (1.0 / len);
    let chord_perp = chord_dir.perp();
    let lambda = spec.param("framing.topo_trunk_wavelength_m").max(50.0);
    let half_waves = (2.0 * len / lambda).round().max(1.0);
    let phase = phase_u * std::f64::consts::TAU;

    // Offset shape at normalized arc t; amplitude scales it linearly, so the
    // sinuosity search below is a clean 1-D bisection.
    let shape = |t: f64| {
        let s = sin(std::f64::consts::PI * half_waves * t + phase);
        let j = perlin1(t * len / (MEANDER_JITTER_WAVE * lambda), noise_seed);
        (s + MEANDER_JITTER * j) * end_taper(t, 0.10)
    };
    let samples = ((len / SKELETON_STEP_M).ceil() as usize).max(8);
    let build = |amp: f64| -> Vec<Vec2> {
        (0..=samples)
            .map(|i| {
                let t = i as f64 / samples as f64;
                entry + chord_dir * (t * len) + chord_perp * (amp * shape(t))
            })
            .collect()
    };

    // Amplitude from a MEASURABLE target (arc/chord), found by bisection —
    // no draws, no rejection loop.
    let target = spec.param("framing.topo_trunk_sinuosity").max(1.0);
    let mut lo = 0.0;
    let mut hi = MEANDER_AMP_FRAC * len;
    for _ in 0..SINUOSITY_ITERS {
        let mid = 0.5 * (lo + hi);
        if sinuosity_of(&build(mid), len) < target {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let mut amp = 0.5 * (lo + hi);

    // Deterministic curvature guard: a channel cannot bend tighter than
    // ~2.5 channel widths without cutting itself off.
    let min_radius = MIN_RADIUS_WIDTHS * 2.0 * halfwidth_m;
    let mut pts = build(amp);
    for _ in 0..CURVATURE_SCALES {
        if Spine::new(pts.clone()).min_curvature_radius() >= 0.95 * min_radius {
            break;
        }
        amp *= 0.8;
        pts = build(amp);
    }

    Some(Trunk {
        spine: pts.into_iter().map(|p| [p.x, p.y]).collect(),
        halfwidth_m,
    })
}

/// Achieved sinuosity: path length over straight-line chord.
fn sinuosity_of(pts: &[Vec2], chord_len: f64) -> f64 {
    if chord_len <= 0.0 {
        return 1.0;
    }
    let arc: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    arc / chord_len
}

// ---------------------------------------------------------------------------
// Secondary valley corridors — `framing/branch/v1`, 1 + MAX_BRANCHES×3 draws
// ---------------------------------------------------------------------------

/// Where along the trunk mouths may sit (keeps them off both ends).
const BRANCH_ARC_LO: f64 = 0.15;
const BRANCH_ARC_SPAN: f64 = 0.70;
/// Elbow position from the mouth, as a fraction of branch length.
const BRANCH_ELBOW_FRAC: f64 = 0.34;
const BRANCH_ELBOW_OFFSET: f64 = 0.35;
/// Shortest branch worth emitting (m), and its margin inside the box.
const BRANCH_MIN_LEN_M: f64 = 100.0;
const BRANCH_BOX_MARGIN_M: f64 = 50.0;

fn gen_branches(sub: u64, spec: &CourseSpec, trunk: Option<&Trunk>) -> Vec<Vec<[f64; 2]>> {
    let mut rng = DetRng::new(sub, b"framing/branch/v1");
    let count_u = rng.next_f64();
    let draws: Vec<(f64, f64, f64)> = (0..MAX_BRANCHES)
        .map(|_| (rng.next_f64(), rng.next_f64(), rng.next_f64()))
        .collect();

    let Some(trunk) = trunk else {
        return Vec::new(); // draws consumed regardless
    };
    let n = stoch_round(spec.param("framing.topo_branch_count"), count_u).min(MAX_BRANCHES);
    if n == 0 {
        return Vec::new();
    }
    let spine = Spine::new(trunk.spine.iter().map(|p| Vec2::new(p[0], p[1])).collect());
    let base_len = spec.param("framing.topo_branch_len_m");
    let junction = spec.param("framing.topo_branch_junction_deg").to_radians();

    let mut out = Vec::with_capacity(n);
    for (k, &(pos_u, len_u, angle_u)) in draws.iter().take(n).enumerate() {
        // Stratified along the trunk: one mouth per stratum, bounded jitter
        // — quasi-regular spacing, never clumped.
        let a = (k as f64 + 0.35 + 0.3 * pos_u) / n as f64;
        let s = BRANCH_ARC_LO + BRANCH_ARC_SPAN * a;
        let mouth = spine.point_at(s);
        let tangent = spine.tangent_at(s);
        // Sides alternate deterministically by index (archive pattern) —
        // tributaries never all arrive from one bank.
        let side = if k % 2 == 0 { 1.0 } else { -1.0 };
        let theta = junction * (0.9 + 0.2 * angle_u) * side;
        // Upstream direction: back up the trunk, rotated out by the
        // junction angle.
        let up = rotate(tangent * -1.0, theta);

        let mut len = base_len * (0.75 + 0.5 * len_u);
        // Shrink (never move) so the head stays inside the box.
        len = len.min(max_len_inside(mouth, up, BRANCH_BOX_MARGIN_M));
        if len < BRANCH_MIN_LEN_M {
            continue;
        }
        let head = mouth + up * len;
        let elbow_dir = (mouth - head).normalized();
        let elbow = head
            + elbow_dir * (len * (1.0 - BRANCH_ELBOW_FRAC))
            + elbow_dir.perp() * (side * (0.5 * (std::f64::consts::FRAC_PI_2 - theta.abs() / 2.0)).tan() * BRANCH_ELBOW_OFFSET * len);
        let curve = catmull_rom(&[head, elbow, mouth], SKELETON_STEP_M);
        // Pin the mouth exactly: the resample can land just short of it and
        // the join tolerance is 0.5 m.
        let mut pts: Vec<[f64; 2]> = curve
            .into_iter()
            .filter(|p| in_box_pt(*p))
            .map(|p| [p.x, p.y])
            .collect();
        if pts.len() < 2 {
            continue;
        }
        *pts.last_mut().expect("non-empty") = [mouth.x, mouth.y];
        out.push(pts);
    }
    out
}

/// How far from `from` along `dir` one can travel and stay `margin` inside
/// the box.
fn max_len_inside(from: Vec2, dir: Vec2, margin: f64) -> f64 {
    let mut best = f64::INFINITY;
    for (a, d, lo, hi) in [
        (from.x, dir.x, margin, EXTENT_M - margin),
        (from.y, dir.y, margin, EXTENT_M - margin),
    ] {
        if d.abs() < 1e-12 {
            continue;
        }
        let t = if d > 0.0 { (hi - a) / d } else { (lo - a) / d };
        best = best.min(t.max(0.0));
    }
    best
}

fn in_box_pt(p: Vec2) -> bool {
    (0.0..=EXTENT_M).contains(&p.x) && (0.0..=EXTENT_M).contains(&p.y)
}

fn rotate(v: Vec2, a: f64) -> Vec2 {
    let (s, c) = (sin(a), cos(a));
    Vec2::new(v.x * c - v.y * s, v.x * s + v.y * c)
}

// ---------------------------------------------------------------------------
// Ridge / interfluve axes — `framing/ridge/v1`, 1 + MAX_RIDGES×5 draws
// ---------------------------------------------------------------------------

/// Axis jitter off the structural grain (rad) — small enough that a train of
/// axes still reads as one grain.
const RIDGE_AXIS_JITTER_RAD: f64 = 0.175;
/// Longitudinal jitter along the axis, as a fraction of the box.
const RIDGE_ALONG_JITTER_FRAC: f64 = 0.25;
/// Widest the whole ridge lattice may span, as a fraction of the box.
const RIDGE_LATTICE_SPAN_FRAC: f64 = 0.95;
/// How far an axis may drift sideways over its own length, as a fraction of
/// the lattice pitch (keeps neighbours from crossing).
const RIDGE_DRIFT_FRAC: f64 = 0.33;
/// Mid-point bow, as a fraction of axis length.
const RIDGE_BOW_FRAC: f64 = 0.10;
const RIDGE_BOX_MARGIN_M: f64 = 100.0;

fn gen_ridges(sub: u64, spec: &CourseSpec, b: &BaseCtx) -> Vec<Vec<[f64; 2]>> {
    let mut rng = DetRng::new(sub, b"framing/ridge/v1");
    let count_u = rng.next_f64();
    let draws: Vec<[f64; 5]> = (0..MAX_RIDGES)
        .map(|_| {
            [
                rng.next_f64(),
                rng.next_f64(),
                rng.next_f64(),
                rng.next_f64(),
                rng.next_f64(),
            ]
        })
        .collect();

    let n = stoch_round(spec.param("framing.topo_ridge_count"), count_u).min(MAX_RIDGES);
    if n == 0 {
        return Vec::new();
    }
    let spacing = spec.param("framing.topo_ridge_spacing_m");
    let len_frac = spec.param("framing.topo_ridge_len_frac");
    let center = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
    let downhill = Vec2::new(cos(b.tilt.dir_rad), sin(b.tilt.dir_rad));

    // ONE cross-grain axis for the whole lattice — the bands are measured
    // against the global grain, not each ridge's own jittered direction, so
    // a train stays a train instead of crossing itself.
    let grain_dir = Vec2::new(cos(b.grain.dir_rad), sin(b.grain.dir_rad));
    let cross = grain_dir.perp();
    // A train wider than the window would clamp several bands onto the same
    // edge; compress the pitch instead so the spacing stays even.
    let lattice_span = (n as f64 * spacing).min(RIDGE_LATTICE_SPAN_FRAC * EXTENT_M);

    let pitch = lattice_span / n as f64;
    let mut out = Vec::with_capacity(n);
    for (k, d) in draws.iter().take(n).enumerate() {
        let [lat_u, long_u, axis_u, len_u, bow_u] = *d;
        let half = 0.5 * len_frac * EXTENT_M * (0.75 + 0.5 * len_u);
        // Cap the axis jitter so an axis can never drift more than a third
        // of the lattice pitch over its own length — the difference between
        // a dune train and crossed sticks. Tight spacing ⇒ tight jitter.
        let jitter_cap = atan2(RIDGE_DRIFT_FRAC * pitch, half.max(1.0));
        let axis_a =
            b.grain.dir_rad + pm1(axis_u) * RIDGE_AXIS_JITTER_RAD.min(jitter_cap);
        let axis = Vec2::new(cos(axis_a), sin(axis_a));
        // Stratified lattice ACROSS the grain: the k-th axis sits in the
        // k-th band at the archetype's spacing, jittered inside it. Jitter
        // stays well under the pitch, which is what makes a dune train read
        // as a train rather than as scatter.
        let off = ((k as f64 + 0.35 + 0.3 * lat_u) / n as f64 - 0.5) * lattice_span;
        let along = pm1(long_u) * RIDGE_ALONG_JITTER_FRAC * EXTENT_M;
        let c = clamp_into_box(center + cross * off + grain_dir * along, RIDGE_BOX_MARGIN_M);

        let fwd = max_len_inside(c, axis, RIDGE_BOX_MARGIN_M).min(half);
        let back = max_len_inside(c, axis * -1.0, RIDGE_BOX_MARGIN_M).min(half);
        let (mut p0, mut p1) = (c - axis * back, c + axis * fwd);
        if (p1 - p0).length() < 60.0 {
            continue;
        }
        // Upstream endpoint first (crest falls with the regional tilt).
        if p0.dot(downhill) > p1.dot(downhill) {
            std::mem::swap(&mut p0, &mut p1);
        }
        let len = (p1 - p0).length();
        let mid = p0.lerp(p1, 0.5) + (p1 - p0).normalized().perp() * (pm1(bow_u) * RIDGE_BOW_FRAC * len);
        let mid = clamp_into_box(mid, RIDGE_BOX_MARGIN_M);
        let pts: Vec<[f64; 2]> = catmull_rom(&[p0, mid, p1], SKELETON_STEP_M)
            .into_iter()
            .filter(|p| in_box_pt(*p))
            .map(|p| [p.x, p.y])
            .collect();
        if pts.len() >= 2 {
            out.push(pts);
        }
    }
    out
}

fn clamp_into_box(p: Vec2, margin: f64) -> Vec2 {
    Vec2::new(
        p.x.clamp(margin, EXTENT_M - margin),
        p.y.clamp(margin, EXTENT_M - margin),
    )
}

// ---------------------------------------------------------------------------
// Terrace / bench step lines — `framing/step/v1`, 1 + MAX_STEPS×3 draws
// ---------------------------------------------------------------------------

/// The reserved central band (in normalized downhill coordinate) that risers
/// stay out of, so the middle of the site is not cut by a step.
const STEP_BAND_LO: f64 = 0.40;
const STEP_BAND_HI: f64 = 0.60;
/// Step chord skew off contour-parallel (rad) — well inside the artifact's
/// STEP_AXIS_TOL_RAD.
const STEP_SKEW_RAD: f64 = 0.12;
/// Mid-point bow as a fraction of chord length.
const STEP_BOW_FRAC: f64 = 0.06;

fn gen_steps(sub: u64, spec: &CourseSpec, b: &BaseCtx) -> Vec<Vec<[f64; 2]>> {
    let mut rng = DetRng::new(sub, b"framing/step/v1");
    let count_u = rng.next_f64();
    let draws: Vec<(f64, f64, f64)> = (0..MAX_STEPS)
        .map(|_| (rng.next_f64(), rng.next_f64(), rng.next_f64()))
        .collect();

    if !window_has_steps(b.window) {
        return Vec::new(); // draws consumed regardless
    }
    let n = stoch_round(spec.param("framing.topo_step_count"), count_u).min(MAX_STEPS);
    if n == 0 {
        return Vec::new();
    }
    let center = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
    let downhill = Vec2::new(cos(b.tilt.dir_rad), sin(b.tilt.dir_rad));
    let (d0, d1) = line_box_span(center, downhill);
    let span = d1 - d0;

    let mut out = Vec::with_capacity(n);
    for (k, &(pos_u, skew_u, bow_u)) in draws.iter().take(n).enumerate() {
        // Stratified along the fall line, then folded around the reserved
        // band so strata stay uniform while the middle stays clear.
        let a = (k as f64 + 0.35 + 0.3 * pos_u) / n as f64;
        let mut a2 = a * (1.0 - (STEP_BAND_HI - STEP_BAND_LO));
        if a2 >= STEP_BAND_LO {
            a2 += STEP_BAND_HI - STEP_BAND_LO;
        }
        let anchor = center + downhill * ((a2 - 0.5) * span);

        let axis_a = b.tilt.dir_rad + std::f64::consts::FRAC_PI_2 + pm1(skew_u) * STEP_SKEW_RAD;
        let axis = Vec2::new(cos(axis_a), sin(axis_a));
        let (t0, t1) = line_box_span(anchor, axis);
        let p0 = anchor + axis * (t0 - EDGE_MARGIN_M);
        let p1 = anchor + axis * (t1 + EDGE_MARGIN_M);
        let len = (p1 - p0).length();
        // Bow toward the fall line, but never far enough to enter the band.
        let dist_to_band = ((a2 - STEP_BAND_LO).abs().min((a2 - STEP_BAND_HI).abs())) * span;
        let bow = (STEP_BOW_FRAC * len).min(0.5 * dist_to_band) * pm1(bow_u);
        let mid = p0.lerp(p1, 0.5) + downhill * bow;
        let pts: Vec<[f64; 2]> = catmull_rom(&[p0, mid, p1], SKELETON_STEP_M)
            .into_iter()
            .map(|p| [p.x, p.y])
            .collect();
        if pts.len() >= 4 {
            out.push(pts);
        }
    }
    out
}

/// Map a uniform to `[-1, 1]`.
fn pm1(u: f64) -> f64 {
    2.0 * u - 1.0
}

/// Wrap an angle to `[0, 2π)`.
fn wrap_2pi(a: f64) -> f64 {
    let tau = 2.0 * std::f64::consts::PI;
    let r = a % tau;
    if r < 0.0 { r + tau } else { r }
}

/// Wrap an axis angle to `[0, π)` (a grain axis has no sign).
fn wrap_axis(a: f64) -> f64 {
    let r = a % std::f64::consts::PI;
    if r < 0.0 {
        r + std::f64::consts::PI
    } else {
        r
    }
}

/// Weighted categorical pick: cumulative walk with strict `<`, so a
/// zero-weight bucket is unreachable. `u ∈ [0,1)`; weights must not all be
/// zero (knob-authoring error — loud).
fn weighted_pick(weights: &[f64], u: f64) -> usize {
    let total: f64 = weights.iter().sum();
    assert!(
        total > 0.0 && total.is_finite(),
        "categorical weights must sum positive"
    );
    let target = u * total;
    let mut cum = 0.0;
    let mut last_nonzero = 0;
    for (i, &w) in weights.iter().enumerate() {
        if w > 0.0 {
            last_nonzero = i;
            cum += w;
            if target < cum {
                return i;
            }
        }
    }
    last_nonzero // float-edge fallback (u ≈ 1)
}

/// Build the province-boundary curve: a kind-oriented chord through the box,
/// extended `EDGE_MARGIN_M` past both sides, with three wobbled interior
/// control points, Catmull-Rom smoothed.
fn boundary_curve(
    kind: BoundaryKind,
    tilt: &Tilt,
    grain: &Grain,
    entry_u: f64,
    skew_u: f64,
    jitters: &[(f64, f64); 3],
) -> Boundary {
    // Chord axis by structural logic: a scarp runs along the contours, a
    // valley wall along the fall line, a material contact along the grain.
    let axis = match kind {
        BoundaryKind::Scarp => tilt.dir_rad + std::f64::consts::FRAC_PI_2,
        BoundaryKind::ValleyWall => tilt.dir_rad,
        BoundaryKind::MaterialContact => grain.dir_rad,
    } + pm1(skew_u) * CHORD_SKEW_RAD;
    let dir = Vec2::new(cos(axis), sin(axis));
    let center = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
    let anchor = center + dir.perp() * (pm1(entry_u) * CHORD_OFFSET_MAX_M);

    // Clip the infinite line anchor + t·dir to the box (slab method; the
    // anchor is strictly inside, so t0 < 0 < t1 always).
    let (t0, t1) = line_box_span(anchor, dir);
    let (t0, t1) = (t0 - EDGE_MARGIN_M, t1 + EDGE_MARGIN_M);
    let span = t1 - t0;

    let mut ctrl = Vec::with_capacity(5);
    ctrl.push(anchor + dir * t0);
    for (i, &(lat_u, along_u)) in jitters.iter().enumerate() {
        let frac = 0.25 * (i as f64 + 1.0) + pm1(along_u) * ALONG_JITTER_FRAC;
        let t = t0 + span * frac;
        ctrl.push(anchor + dir * t + dir.perp() * (pm1(lat_u) * kind.wobble_m()));
    }
    ctrl.push(anchor + dir * t1);

    let curve = catmull_rom(&ctrl, CURVE_STEP_M)
        .into_iter()
        .map(|p| [p.x, p.y])
        .collect();
    Boundary { curve, kind }
}

/// The `t` span over which `anchor + t·dir` lies inside `[0, EXTENT_M]²`.
/// Requires `anchor` strictly inside and `dir` a unit vector.
fn line_box_span(anchor: Vec2, dir: Vec2) -> (f64, f64) {
    let mut t0 = f64::NEG_INFINITY;
    let mut t1 = f64::INFINITY;
    for (a, d) in [(anchor.x, dir.x), (anchor.y, dir.y)] {
        if d.abs() < 1e-12 {
            continue; // parallel to this slab; anchor inside guarantees it
        }
        let (lo, hi) = ((0.0 - a) / d, (EXTENT_M - a) / d);
        let (lo, hi) = if lo <= hi { (lo, hi) } else { (hi, lo) };
        t0 = t0.max(lo);
        t1 = t1.min(hi);
    }
    debug_assert!(t0 < 0.0 && t1 > 0.0, "chord anchor must be inside the box");
    (t0, t1)
}
