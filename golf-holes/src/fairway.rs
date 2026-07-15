//! Fairway boundaries + contouring — the FairwayStudio spline-shape model.
//!
//! Boundary pipeline (after the user's CourseGenerator `SplineShape2`):
//! a **Catmull-Rom spine** through the routed points (doglegs emerge from how
//! the centerline curves), per-side widths from **smoothstep-interpolated
//! landmarks** (zero derivative at every knot → C¹ by construction) overlaid
//! with **low-frequency sinusoids** (organic edges) and a **meander offset**
//! (straight holes bend gently), then a **curvature clamp** (tanh
//! soft-saturation of the inside width toward `SAFETY/|κ|`) so tight bends
//! can never kink or self-intersect, a water-standoff clip, and
//! **semicircular end caps** (width tapers on a quarter-ellipse, so the
//! outline is smooth with no hard stop at the extents).
//!
//! Contouring: a 2 m absolute-surface patch — smooth macro base + drainage
//! crown + character-scaled undulation + the rare mound cluster, with an
//! ε-priority-flood drainage guard.

use golf_core::det::DetRng;
use golf_core::math::{self, Vec2};
use golf_core::spline::{Profile, SpineCurve};
use golf_core::Grid;
use golf_routing::Hole;
use golf_terrain::{noise, CourseTerrain};

use crate::{patch_spec, GreenShape, Patch};

/// Feather width outside the fairway boundary (blend back to rough), m.
const FEATHER: f64 = 6.0;
/// Hard floor on a side half-width before capping, m.
const MIN_W: f64 = 3.0;
/// Curvature-clamp constants (from the reference implementation).
const SAFETY: f64 = 0.85;
const TOTAL_SAFETY: f64 = 1.7;

/// Per-side dense width knots (everything baked: envelope, dogleg factors,
/// sinusoids, meander, curvature clamp, water clip), sampled with smoothstep
/// easing between knots, then multiplied by the end-cap taper. C¹ everywhere.
#[derive(Clone, Debug)]
pub struct FairwayWidth {
    pub start: f64,
    pub end: f64,
    /// Cap radii, meters of arc length over which the width tapers to zero
    /// on a quarter-ellipse (≈ semicircular noses).
    pub cap_start: f64,
    pub cap_end: f64,
    pub left: Vec<(f64, f64)>,
    pub right: Vec<(f64, f64)>,
}

/// Smoothstep-eased sampling of dense knots (their `evaluate_width` with
/// flat tangents): zero derivative at every knot → C¹. Shared with the
/// bunker spline shapes.
pub(crate) fn smooth_knots(knots: &[(f64, f64)], s: f64) -> f64 {
    if knots.is_empty() {
        return 0.0;
    }
    if s <= knots[0].0 {
        return knots[0].1;
    }
    let last = knots[knots.len() - 1];
    if s >= last.0 {
        return last.1;
    }
    for w in knots.windows(2) {
        let (s0, v0) = w[0];
        let (s1, v1) = w[1];
        if s <= s1 {
            let u = if s1 == s0 { 0.0 } else { (s - s0) / (s1 - s0) };
            let e = u * u * (3.0 - 2.0 * u);
            return v0 + (v1 - v0) * e;
        }
    }
    last.1
}

impl FairwayWidth {
    /// A fairway with no run-up ribbon (surround-only par 3s).
    pub fn none() -> Self {
        FairwayWidth {
            start: 1.0,
            end: 1.0,
            cap_start: 1.0,
            cap_end: 1.0,
            left: Vec::new(),
            right: Vec::new(),
        }
    }

    pub fn is_none(&self) -> bool {
        self.left.is_empty()
    }

    /// Effective half-width at normalized arc `s` (0 outside the extent;
    /// tapers to 0 at the extents on the cap ellipses).
    pub fn half_width(&self, len: f64, s: f64, left: bool) -> f64 {
        if self.is_none() || s < self.start || s > self.end {
            return 0.0;
        }
        let w = smooth_knots(if left { &self.left } else { &self.right }, s);
        let cap = |d_m: f64, r: f64| -> f64 {
            if d_m >= r {
                1.0
            } else {
                let t = (d_m / r.max(1e-9)).clamp(0.0, 1.0);
                (t * (2.0 - t)).max(0.0).sqrt()
            }
        };
        w * cap((s - self.start) * len, self.cap_start) * cap((self.end - s) * len, self.cap_end)
    }

    /// Envelope mid-width without noise asymmetry or caps — the stable
    /// quantity the report gates on.
    pub fn env_mid(&self, s: f64) -> f64 {
        0.5 * (smooth_knots(&self.left, s) + smooth_knots(&self.right, s))
    }
}

/// Geometry + shaping parameters for one fairway (pre-`HoleBuild`).
pub struct FairwayShape {
    pub spine: SpineCurve,
    pub fw: FairwayWidth,
    pub outline: Vec<Vec2>,
    pub bbox: (Vec2, Vec2),
    pub landing: Vec<(f64, Vec2)>,
}

/// Catmull-Rom centerline through the routed points (ghost-reflected ends),
/// resampled at ~6 m of uniform arc length. Doglegs become smooth curves.
pub fn smooth_spine(pts: &[Vec2]) -> SpineCurve {
    let n = pts.len();
    let reflect = |a: Vec2, b: Vec2| Vec2::new(2.0 * a.x - b.x, 2.0 * a.y - b.y);
    let cr = |p0: Vec2, p1: Vec2, p2: Vec2, p3: Vec2, t: f64| -> Vec2 {
        let t2 = t * t;
        let t3 = t2 * t;
        Vec2::new(
            0.5 * (2.0 * p1.x
                + (-p0.x + p2.x) * t
                + (2.0 * p0.x - 5.0 * p1.x + 4.0 * p2.x - p3.x) * t2
                + (-p0.x + 3.0 * p1.x - 3.0 * p2.x + p3.x) * t3),
            0.5 * (2.0 * p1.y
                + (-p0.y + p2.y) * t
                + (2.0 * p0.y - 5.0 * p1.y + 4.0 * p2.y - p3.y) * t2
                + (-p0.y + 3.0 * p1.y - 3.0 * p2.y + p3.y) * t3),
        )
    };
    // Dense uniform-t evaluation across all segments.
    let mut dense: Vec<Vec2> = Vec::with_capacity(64 * (n - 1) + 1);
    for i in 0..n - 1 {
        let p1 = pts[i];
        let p2 = pts[i + 1];
        let p0 = if i == 0 { reflect(pts[0], pts[1]) } else { pts[i - 1] };
        let p3 = if i + 2 < n { pts[i + 2] } else { reflect(pts[n - 1], pts[n - 2]) };
        for k in 0..64 {
            dense.push(cr(p0, p1, p2, p3, k as f64 / 64.0));
        }
    }
    dense.push(pts[n - 1]);
    // Uniform arc-length resample at ~8 m (the dense CR evaluation above
    // already captured the curvature; 8 m polylines of a smooth curve are
    // accurate and keep `project()` — linear in vertex count — cheap).
    let mut cum = vec![0.0f64];
    for w in dense.windows(2) {
        cum.push(cum.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *cum.last().unwrap();
    let m = ((total / 8.0).ceil() as usize).max(4);
    let mut out: Vec<Vec2> = Vec::with_capacity(m + 1);
    let mut j = 0usize;
    for k in 0..=m {
        let target = total * k as f64 / m as f64;
        while j + 1 < cum.len() - 1 && cum[j + 1] < target {
            j += 1;
        }
        let span = (cum[j + 1] - cum[j]).max(1e-12);
        let f = ((target - cum[j]) / span).clamp(0.0, 1.0);
        out.push(dense[j].lerp(dense[j + 1], f));
    }
    SpineCurve::new(out, Profile::constant(15.0), Profile::constant(FEATHER))
}

/// Chamfer distance (m) to blocking water on the macro grid — fairways may
/// cross creeks (carries) but must stand off ponds/lakes/rivers/wetlands.
pub fn blocking_dist(ct: &CourseTerrain) -> Grid<f64> {
    let spec = ct.water.class.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut d = Grid::filled(spec, 1.0e9f64);
    for i in 0..spec.len() {
        if ct.water.is_blocking(i) {
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

/// Signed discrete curvature along the spine polyline, smoothed with two
/// 1-2-1 binomial passes (positive = bends toward the left normal).
fn spine_curvature(spine: &SpineCurve) -> Vec<f64> {
    let pts = &spine.polyline;
    let n = pts.len();
    let mut k = vec![0.0f64; n];
    for i in 1..n - 1 {
        let d1 = pts[i] - pts[i - 1];
        let d2 = pts[i + 1] - pts[i];
        let cross = d1.x * d2.y - d1.y * d2.x;
        let l1 = d1.length();
        let l2 = d2.length();
        let denom = l1 * l2 * (l1 + l2);
        if denom > 1e-9 {
            k[i] = 2.0 * cross / denom;
        }
    }
    for _ in 0..2 {
        let prev = k.clone();
        for i in 1..n - 1 {
            k[i] = 0.25 * prev[i - 1] + 0.5 * prev[i] + 0.25 * prev[i + 1];
        }
    }
    k
}

fn tanh(x: f64) -> f64 {
    let e = math::exp(2.0 * x.clamp(-20.0, 20.0));
    (e - 1.0) / (e + 1.0)
}

/// Soft saturation toward `limit` (identity well below it).
fn soft_clamp(desired: f64, limit: f64) -> f64 {
    if !limit.is_finite() || limit <= 0.0 {
        desired
    } else {
        limit * tanh(desired / limit)
    }
}

/// All the RNG-drawn parameters of a fairway boundary — separated from
/// realization so the geometry can be re-realized against different water
/// (the terrain suggests water; the build's refinement gets the final say,
/// and the fairway must be clipped against the REFINED water, not the
/// suggestion).
pub struct FwDraws {
    pub ribbon: bool,
    pub start: f64,
    pub end: f64,
    pub env: Vec<(f64, f64)>,
    pub inside_f: f64,
    pub sin: [[(f64, f64, f64); 2]; 2], // [side][component] (amp, λ m, phase)
    pub meander: (f64, f64, f64),       // (amp m, λ m, phase)
    pub cap_start: f64,
    pub cap_end: f64,
}

/// Landing zones for a hole (deterministic, no draws).
pub fn landing_zones(hole: &Hole, spine: &SpineCurve) -> Vec<(f64, Vec2)> {
    let len = spine.length().max(1.0);
    let mut landing: Vec<(f64, Vec2)> = Vec::new();
    if hole.par >= 4 && len > 260.0 {
        let s1 = (220.0 / len).min(0.75);
        landing.push((s1, spine.point_at(s1)));
        if hole.par == 5 {
            landing.push((0.62, spine.point_at(0.62)));
        }
    }
    landing
}

/// Draw every fairway parameter (all RNG consumption lives here).
pub fn draw_params(
    hole: &Hole,
    spine: &SpineCurve,
    green: &GreenShape,
    rng: &mut DetRng,
) -> FwDraws {
    let len = spine.length().max(1.0);
    // Par 3s carry no run-up ribbon: the surround petal is their fairway.
    let ribbon = hole.par >= 4;
    let start = rng.range_f64(45.0, 90.0) / len;
    // End far enough in that the cap taper tucks under the green fringe.
    let end_m = (len - green.a.min(green.b) * 0.55).max(start * len + 25.0);
    let end = (end_m / len).min(1.0);
    let landing = landing_zones(hole, spine);

    // Strategy landmarks (smoothstep-eased between).
    let mut env: Vec<(f64, f64)> = Vec::new();
    {
        let w_start = rng.range_f64(12.0, 14.0);
        let w_land = rng.range_f64(16.0, 20.0);
        let w_deep = rng.range_f64(10.5, 13.0);
        let w_appr = rng.range_f64(9.0, 12.0);
        env.push((start, w_start));
        if let Some(&(s1, _)) = landing.first() {
            env.push(((s1 - 30.0 / len).max(start), w_land * 0.92));
            env.push((s1, w_land));
            env.push(((s1 + 30.0 / len).min(end), w_land * 0.92));
        }
        if hole.par == 5 {
            if let Some(&(s2, _)) = landing.get(1) {
                let w2 = rng.range_f64(15.0, 18.0);
                env.push(((s2 - 25.0 / len).max(start), w2 * 0.9));
                env.push((s2, w2));
            }
        } else if len > 320.0 {
            env.push(((290.0 / len).min(end - 0.05), w_deep));
        }
        env.push(((end - 35.0 / len).max(start), w_appr));
        env.push((end, w_appr * 0.95));
    }
    env.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    env.dedup_by(|a, b| (a.0 - b.0).abs() < 1e-9);

    // Straightness for the meander amplitude.
    let mut total_turn = 0.0f64;
    for k in 1..hole.pts.len() - 1 {
        let a = (hole.pts[k] - hole.pts[k - 1]).normalized();
        let b = (hole.pts[k + 1] - hole.pts[k]).normalized();
        total_turn += math::acos(a.dot(b).clamp(-1.0, 1.0));
    }
    let straightness = 1.0 - (total_turn / 0.5).clamp(0.0, 1.0);

    let inside_f = rng.range_f64(0.70, 0.80);
    let sin = [
        [
            (rng.range_f64(1.2, 3.0), rng.range_f64(70.0, 150.0), rng.range_f64(0.0, std::f64::consts::TAU)),
            (rng.range_f64(0.6, 1.6), rng.range_f64(40.0, 70.0), rng.range_f64(0.0, std::f64::consts::TAU)),
        ],
        [
            (rng.range_f64(1.2, 3.0), rng.range_f64(70.0, 150.0), rng.range_f64(0.0, std::f64::consts::TAU)),
            (rng.range_f64(0.6, 1.6), rng.range_f64(40.0, 70.0), rng.range_f64(0.0, std::f64::consts::TAU)),
        ],
    ];
    let meander = (
        rng.range_f64(2.0, 4.0) + rng.range_f64(3.0, 6.0) * straightness,
        rng.range_f64(150.0, 300.0),
        rng.range_f64(0.0, std::f64::consts::TAU),
    );
    let cap_start = env.first().map(|e| e.1).unwrap_or(10.0) * rng.range_f64(0.9, 1.3);
    let cap_end = env.last().map(|e| e.1).unwrap_or(10.0) * rng.range_f64(0.9, 1.3);

    FwDraws {
        ribbon,
        start,
        end,
        env,
        inside_f,
        sin,
        meander,
        cap_start,
        cap_end,
    }
}

/// Realize the fairway boundary from drawn parameters against a given
/// blocking-water distance field. Pure (no RNG) — called twice: once against
/// the terrain's water to detect intrusions, once against the refined water.
pub fn realize(
    hole: &Hole,
    spine: &SpineCurve,
    draws: &FwDraws,
    dblock: &Grid<f64>,
) -> FairwayShape {
    let spine = spine.clone();
    let len = spine.length().max(1.0);
    let (start, end) = (draws.start, draws.end);
    if !draws.ribbon {
        let tee = spine.point_at(0.0);
        return FairwayShape {
            spine,
            fw: FairwayWidth::none(),
            outline: Vec::new(),
            bbox: (tee, tee),
            landing: Vec::new(),
        };
    }
    let landing = landing_zones(hole, &spine);
    let env = &draws.env;

    // Dogleg factors: smooth cos² windows around each routed turn.
    let verts = &hole.pts;
    let mut turns: Vec<(f64, f64)> = Vec::new(); // (s, sign: + = left turn)
    {
        let mut acc = 0.0;
        for k in 1..verts.len() - 1 {
            acc += verts[k - 1].distance(verts[k]);
            let a = (verts[k] - verts[k - 1]).normalized();
            let b = (verts[k + 1] - verts[k]).normalized();
            let cross = a.perp().dot(b);
            if cross.abs() > 0.05 {
                turns.push((acc / len, cross.signum()));
            }
        }
    }
    let inside_f = draws.inside_f;
    let side_factor = |s: f64, left: bool| -> f64 {
        let mut f = 1.0;
        for &(sv, sign) in &turns {
            let d = (s - sv).abs() * len;
            if d < 70.0 {
                let t = math::cos(std::f64::consts::FRAC_PI_2 * d / 70.0);
                let win = t * t; // cos² window: C¹ at both ends
                let inside = (sign > 0.0) == left;
                f *= if inside { 1.0 + (inside_f - 1.0) * win } else { 1.0 + 0.10 * win };
            }
        }
        f
    };
    let (cap_start, cap_end) = (draws.cap_start, draws.cap_end);

    // --- Bake dense per-side knots: envelope → factors → sinusoids →
    // meander → curvature clamp → water clip. Two attempts: if the outline
    // still kinks (water clips can corner), soften the overlays and rebuild
    // (their editor's sharpness warning, resolved automatically). ---
    let kappa = spine_curvature(&spine);
    let mut overlay_scale = 1.0f64;
    let mut fw = FairwayWidth::none();
    let mut outline: Vec<Vec2> = Vec::new();
    for _attempt in 0..2 {
        let n_dense = ((end - start) * len / 8.0).ceil().max(3.0) as usize;
        let mut kl: Vec<(f64, f64)> = Vec::with_capacity(n_dense + 1);
        let mut kr: Vec<(f64, f64)> = Vec::with_capacity(n_dense + 1);
        for k in 0..=n_dense {
            let s = start + (end - start) * k as f64 / n_dense as f64;
            let base = smooth_knots(env, s);
            // Meander window keeps the ends anchored (tee aim + green meet).
            let d_end = ((s - start).min(end - s) * len).max(0.0);
            let mwin = math::smoothstep(0.0, 60.0, d_end);
            let (ma, ml, mp) = draws.meander;
            let m_off = overlay_scale * ma * mwin * math::sin(std::f64::consts::TAU * s * len / ml + mp);
            // Spine sample index for curvature.
            let si = ((s * (kappa.len() - 1) as f64) as usize).min(kappa.len() - 1);
            let kap = kappa[si];
            let mut w2 = [0.0f64; 2]; // [left, right]
            for (side, w_out) in w2.iter_mut().enumerate() {
                let left = side == 0;
                let mut w = base * side_factor(s, left);
                for &(amp, lam, ph) in &draws.sin[side] {
                    w += overlay_scale * amp * math::sin(std::f64::consts::TAU * s * len / lam + ph);
                }
                w += if left { m_off } else { -m_off };
                w = w.max(MIN_W);
                // Curvature clamp: inside of the bend saturates toward
                // SAFETY/|κ| (left is inside when κ > 0).
                let limit = if (kap > 0.0) == left && kap.abs() > 1e-9 {
                    SAFETY / kap.abs()
                } else {
                    f64::INFINITY
                };
                *w_out = soft_clamp(w, limit);
            }
            // Total-width guard on tight bends.
            if kap.abs() > 1e-6 {
                let total_limit = TOTAL_SAFETY / kap.abs();
                let total = w2[0] + w2[1];
                if total > total_limit {
                    let sc = total_limit / total;
                    w2[0] *= sc;
                    w2[1] *= sc;
                }
            }
            // Water standoff.
            let p = spine.point_at(s);
            let (_, nrm) = spine.frame_at(s);
            for (side, w) in w2.iter_mut().enumerate() {
                let dir = if side == 0 { nrm } else { nrm * -1.0 };
                for _ in 0..4 {
                    if dblock.bilinear(p + dir * *w) >= 3.0 {
                        break;
                    }
                    *w *= 0.78;
                }
                *w = w.max(MIN_W);
            }
            kl.push((s, w2[0]));
            kr.push((s, w2[1]));
        }
        fw = FairwayWidth {
            start,
            end,
            cap_start,
            cap_end,
            left: kl,
            right: kr,
        };

        // Outline (3 m steps; the caps taper the tips closed).
        let n_out = (((end - start) * len / 3.0).ceil() as usize).max(4);
        let mut left_pts = Vec::with_capacity(n_out + 1);
        let mut right_pts = Vec::with_capacity(n_out + 1);
        for k in 0..=n_out {
            let s = start + (end - start) * k as f64 / n_out as f64;
            let p = spine.point_at(s);
            let (_, nrm) = spine.frame_at(s);
            left_pts.push(p + nrm * fw.half_width(len, s, true));
            right_pts.push(p + nrm * -fw.half_width(len, s, false));
        }
        right_pts.reverse();
        outline = left_pts;
        outline.extend(right_pts);

        // Kink guard: sharpest interior turn of the outline. The cap tips
        // legitimately turn fast — skip the few points nearest each tip.
        let mut worst = 0.0f64;
        let n_pts = outline.len();
        let tip_a = 0usize;
        let tip_b = n_out; // index of the end tip in `outline`
        for i in 1..n_pts - 1 {
            if i.abs_diff(tip_a) <= 3 || i.abs_diff(tip_b) <= 3 || i.abs_diff(n_pts - 1) <= 3 {
                continue;
            }
            let a = (outline[i] - outline[i - 1]).normalized();
            let b = (outline[i + 1] - outline[i]).normalized();
            worst = worst.max(math::acos(a.dot(b).clamp(-1.0, 1.0)));
        }
        if worst <= 42.0f64.to_radians() {
            break;
        }
        overlay_scale *= 0.6;
    }

    let (mut lo, mut hi) = (
        Vec2::new(f64::INFINITY, f64::INFINITY),
        Vec2::new(f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for &p in &outline {
        lo = Vec2::new(lo.x.min(p.x), lo.y.min(p.y));
        hi = Vec2::new(hi.x.max(p.x), hi.y.max(p.y));
    }
    let m = FEATHER + 2.0;
    FairwayShape {
        spine,
        fw,
        outline,
        bbox: (lo - Vec2::new(m, m), hi + Vec2::new(m, m)),
        landing,
    }
}

/// Fairway contouring patch: 2 m raster over the fairway (+ feather ring).
/// Ribbon-less par 3s get an inert 2×2 zero-weight patch (keeps the fixed
/// fairways→tees→greens apply order intact).
pub fn patch(ct: &CourseTerrain, fws: &FairwayShape, mound: bool, rng: &mut DetRng) -> Patch {
    if fws.fw.is_none() {
        let spec = patch_spec(
            fws.spine.point_at(0.0) - Vec2::new(1.0, 1.0),
            fws.spine.point_at(0.0) + Vec2::new(1.0, 1.0),
            2.0,
        );
        let mut z = Grid::filled(spec, 0.0f64);
        for gy in 0..spec.ny {
            for gx in 0..spec.nx {
                let i = spec.index(gx, gy);
                z.data[i] = ct.macro_heights.bilinear(spec.world_of(gx, gy));
            }
        }
        return Patch { z, w: Grid::filled(spec, 0.0f64) };
    }
    let (lo, hi) = fws.bbox;
    let spec = patch_spec(lo, hi, 2.0);
    let len = fws.spine.length();
    let fw = &fws.fw;

    let crown = rng.range_f64(0.15, 0.35);
    let und_lambda = rng.range_f64(30.0, 60.0);
    let und_seed = rng.next_u32();
    // Character matching: rougher terrain rolls more.
    let mid = fws.spine.point_at(0.5 * (fw.start + fw.end));
    let rough_k = (ct.macro_slope.bilinear(mid) / 0.06).clamp(0.6, 1.6);
    let und_amp = rng.range_f64(0.15, 0.40) * rough_k;

    // The rare mound cluster: 2–4 gaussian bumps off-center near a landing.
    let mut mounds: Vec<(Vec2, f64, f64)> = Vec::new();
    if mound {
        let s0 = fws.landing.first().map(|l| l.0).unwrap_or(0.5);
        let count = 2 + rng.below(3);
        for _ in 0..count {
            let s = (s0 + rng.range_f64(-0.06, 0.10)).clamp(fw.start, fw.end);
            let v = rng.range_f64(-0.8, 0.8) * fw.half_width(len, s, true);
            let (_, nrm) = fws.spine.frame_at(s);
            let c = fws.spine.point_at(s) + nrm * v;
            mounds.push((c, rng.range_f64(0.6, 1.2), rng.range_f64(6.0, 12.0)));
        }
    }

    let mut z = Grid::filled(spec, 0.0f64);
    let mut w = Grid::filled(spec, 0.0f64);
    for gy in 0..spec.ny {
        for gx in 0..spec.nx {
            let p = spec.world_of(gx, gy);
            let i0 = spec.index(gx, gy);
            let base = ct.macro_heights.bilinear(p);
            // Every cell needs a sane surface: bilinear at the weight ring
            // mixes neighbor z values, so zeros would carve phantom steps.
            z.data[i0] = base;

            let (u, v) = fws.spine.project(p);
            let uc = u.clamp(fw.start, fw.end);
            let du_m = (u - uc).abs() * len;
            let hw = fw.half_width(len, uc, v >= 0.0);
            // Radial distance beyond the tips; plain side distance inside.
            let d_in = hw - math::hypot(v, du_m);
            if d_in <= -FEATHER {
                continue;
            }
            let weight = math::smoothstep(-FEATHER, 0.0, d_in);

            // Drainage crown across the fairway.
            let x = (v / hw.max(1.0)).clamp(-1.0, 1.0);
            let (_, nrm) = fws.spine.frame_at(uc);
            let cant = (ct.macro_heights.bilinear(p + nrm * 6.0)
                - ct.macro_heights.bilinear(p + nrm * -6.0))
                / 12.0;
            let crown_here = if cant.abs() >= 0.025 { 0.0 } else { crown * (1.0 - x * x) };
            // Soften extreme cross-cant toward ≤8% (partial counter-grade).
            let excess = (cant.abs() - 0.08).max(0.0);
            let cant_fix = -cant.signum() * excess * 0.5 * v.clamp(-hw.max(1.0), hw.max(1.0));
            let und = und_amp * noise::perlin2(p.x / und_lambda, p.y / und_lambda, und_seed);
            let mut zz = base + crown_here.max(0.0) + cant_fix + und;
            for &(c, h, r) in &mounds {
                let d2 = (p - c).dot(p - c);
                zz += h * math::exp(-d2 / (2.0 * r * r));
            }
            z.data[i0] = zz;
            w.data[i0] = weight;
        }
    }

    // Drainage guard: ε priority-flood over the fairway core. Every core
    // cell provably drains to the (fixed) boundary ring — interior dips fill
    // to their spill, which is exactly what drainage grading does. Heap keys
    // use the sign-flip total order for f64 (raw bits mis-sort negatives).
    {
        use std::cmp::Reverse;
        use std::collections::BinaryHeap;
        let (nx, ny) = (spec.nx as usize, spec.ny as usize);
        let core = |i: usize| w.data[i] >= 0.9;
        let key = |z: f64| -> u64 {
            let b = z.to_bits();
            if b & 0x8000_0000_0000_0000 != 0 {
                !b
            } else {
                b ^ 0x8000_0000_0000_0000
            }
        };
        let unkey = |k: u64| -> f64 {
            if k & 0x8000_0000_0000_0000 != 0 {
                f64::from_bits(k ^ 0x8000_0000_0000_0000)
            } else {
                f64::from_bits(!k)
            }
        };
        let mut open: BinaryHeap<Reverse<(u64, u32)>> = BinaryHeap::new();
        let mut visited = vec![false; spec.len()];
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                let edge = x == 0 || y == 0 || x == nx - 1 || y == ny - 1;
                if !core(i) || edge {
                    visited[i] = true;
                    let touches_core = !edge
                        && [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)]
                            .iter()
                            .any(|&(dx, dy)| {
                                let xx = x as i64 + dx;
                                let yy = y as i64 + dy;
                                xx >= 0
                                    && yy >= 0
                                    && xx < nx as i64
                                    && yy < ny as i64
                                    && core(yy as usize * nx + xx as usize)
                            });
                    if touches_core || edge {
                        open.push(Reverse((key(z.data[i]), i as u32)));
                    }
                }
            }
        }
        while let Some(Reverse((zk, iu))) = open.pop() {
            let zi = unkey(zk);
            let i = iu as usize;
            let (x, y) = (i % nx, i / nx);
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                let xx = x as i64 + dx;
                let yy = y as i64 + dy;
                if xx < 0 || yy < 0 || xx >= nx as i64 || yy >= ny as i64 {
                    continue;
                }
                let j = yy as usize * nx + xx as usize;
                if visited[j] || !core(j) {
                    continue;
                }
                visited[j] = true;
                if z.data[j] < zi + 0.002 {
                    z.data[j] = zi + 0.002;
                }
                open.push(Reverse((key(z.data[j]), j as u32)));
            }
        }
    }

    Patch { z, w }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catmull_spine_passes_through_routed_points() {
        let pts = vec![Vec2::new(0.0, 0.0), Vec2::new(200.0, 0.0), Vec2::new(350.0, 90.0)];
        let s = smooth_spine(&pts);
        assert!(s.polyline[0].distance(pts[0]) < 1e-6);
        assert!(s.polyline.last().unwrap().distance(pts[2]) < 1e-6);
        // The interior control point lies ON the curve (Catmull-Rom
        // interpolates), so some sample passes near it.
        let dmin = s
            .polyline
            .iter()
            .map(|p| p.distance(pts[1]))
            .fold(f64::INFINITY, f64::min);
        assert!(dmin < 4.0, "spine misses the dogleg vertex by {dmin}");
    }

    #[test]
    fn width_caps_taper_to_zero_and_are_smooth() {
        let fw = FairwayWidth {
            start: 0.2,
            end: 0.8,
            cap_start: 14.0,
            cap_end: 14.0,
            left: vec![(0.2, 14.0), (0.5, 16.0), (0.8, 12.0)],
            right: vec![(0.2, 14.0), (0.5, 16.0), (0.8, 12.0)],
        };
        let len = 400.0;
        assert_eq!(fw.half_width(len, 0.19, true), 0.0);
        assert!(fw.half_width(len, 0.2, true) < 1e-9);
        assert!(fw.half_width(len, 0.5, true) > 15.0);
        // Monotone growth through the cap (no oscillation).
        let mut prev = 0.0;
        for k in 0..=20 {
            let s = 0.2 + 0.035 * k as f64 / 20.0;
            let w = fw.half_width(len, s, true);
            assert!(w >= prev - 1e-9);
            prev = w;
        }
    }

    #[test]
    fn smooth_knots_is_c1_flat_at_knots() {
        let knots = vec![(0.0, 10.0), (0.5, 20.0), (1.0, 10.0)];
        // Derivative ≈ 0 at the knots (smoothstep easing).
        let e = 1e-4;
        let d0 = (smooth_knots(&knots, 0.5 + e) - smooth_knots(&knots, 0.5 - e)) / (2.0 * e);
        assert!(d0.abs() < 0.5, "kink at knot: slope {d0}");
        assert!((smooth_knots(&knots, 0.25) - 15.0).abs() < 1e-9); // smoothstep(0.5) = 0.5
    }

    #[test]
    fn soft_clamp_matches_reference_behavior() {
        assert!((soft_clamp(1.0, 100.0) - 1.0).abs() < 0.01);
        let v = soft_clamp(1000.0, 5.0);
        assert!(v <= 5.0 && v > 4.99);
    }
}
