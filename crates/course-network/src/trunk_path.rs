//! Phase N1 — trunk paths: end-to-end, long-wave meander, long profile.
//!
//! A trunk is AUTHORED, and that is safe here where it failed in `heartland`:
//! there the authored line disagreed with an independently generated surface
//! ("the erosion declines to adopt it"); here the surface will be built FROM
//! the network, so there is no second authority to disagree with. Divides are
//! still never authored — they emerge at surface synthesis.
//!
//! Geometry: a Catmull-Rom baseline from the mouth through low ground to the
//! far end, then a heading-domain long-wave meander on top. The far end is a
//! far EDGE when the trunk carries external inflow (a through-river), or a
//! high-ground headwater start otherwise.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::spline::{catmull_rom, Spine};
use course_world::world::EXTENT_M;

/// A trunk as phase N1 emits it: planform + long profile.
#[derive(Clone, Debug)]
pub struct TrunkPath {
    /// Mouth-first, ~20 m spacing after densify. For a joining trunk the
    /// "mouth" is the junction on the primary.
    pub pts: Vec<Vec2>,
    /// Elevation per point, metres above the tile base level. `z[0]` is 0 for
    /// a trunk with its own mouth, the junction elevation for a joining one.
    pub z: Vec<f64>,
    /// Catchment entering at the HEAD from outside the tile, km².
    pub external_km2: f64,
    /// Index of the trunk this one converges onto, if any.
    pub joins: Option<u32>,
}

pub struct TrunkPathParams {
    /// Meander wavelength band, metres. Drawn per trunk.
    pub lam_m: (f64, f64),
    /// Heading swing, radians. Sets sinuosity: full-cycle sinuosity is about
    /// `1 + swing²/4`, so the 0.45–0.62 band gives 1.05–1.10 where the wind
    /// runs untapered.
    pub swing_rad: (f64, f64),
    /// Fraction of relief_budget the trunk head reaches. The long profile.
    pub head_relief_frac: f64,
    /// Long-profile concavity exponent: z = H·(arc/L)^this.
    pub concavity: f64,
    /// Hard floor on curve radius, metres. Asserted, not hoped.
    pub min_radius_m: f64,
}

impl Default for TrunkPathParams {
    fn default() -> Self {
        TrunkPathParams {
            lam_m: (900.0, 1800.0),
            swing_rad: (0.45, 0.62),
            head_relief_frac: 0.55,
            concavity: 1.45,
            min_radius_m: 150.0,
        }
    }
}

/// Sample the relief field along the corridor from `a` to `b` and return the
/// lowest point in each of `n` corridor bands — the interior control points
/// that pull the baseline through the lows.
fn low_controls(relief: &Grid<f64>, a: Vec2, b: Vec2, n: usize, rng: &mut DetRng) -> Vec<Vec2> {
    let dir = (b - a).normalized();
    let perp = dir.perp();
    let len = a.distance(b);
    let mut out = Vec::new();
    for i in 0..n {
        // band centre along the corridor, avoiding the ends
        let t0 = (i as f64 + 0.5) / n as f64;
        let centre = a.lerp(b, t0);
        // search across the corridor for the lowest relief_pred
        let half_w = (len * 0.22).min(900.0);
        let mut best = (f64::MAX, centre);
        let steps = 15;
        for k in 0..=steps {
            let off = -half_w + 2.0 * half_w * k as f64 / steps as f64;
            let p = Vec2::new(centre.x + perp.x * off, centre.y + perp.y * off);
            if p.x < 60.0 || p.y < 60.0 || p.x > EXTENT_M - 60.0 || p.y > EXTENT_M - 60.0 {
                continue;
            }
            // small draw so ties break differently per seed
            let v = relief.bilinear(p) + rng.range_f64(-0.02, 0.02);
            if v < best.0 {
                best = (v, p);
            }
        }
        out.push(best.1);
    }
    out
}

/// Build one trunk path from mouth to far end.
///
/// The meander is applied in the heading domain and then RE-ANCHORED: after
/// winding, the curve is linearly pulled so its endpoints land exactly on
/// mouth and head. Winding a heading integration drifts the endpoint; the
/// anchor restores it without kinking the curve.
pub fn build_trunk(
    rng: &mut DetRng,
    mouth: Vec2,
    far: Vec2,
    external_km2: f64,
    relief: &Grid<f64>,
    relief_budget_m: f64,
    p: &TrunkPathParams,
    // For a JOINING trunk: the upstream direction its first segment must
    // leave the junction along (set so the flow enters the primary at an
    // acute angle), and the junction elevation its profile starts from.
    join: Option<(Vec2, f64)>,
) -> TrunkPath {
    // --- baseline through the lows
    let n_ctrl = 2 + rng.below(2); // 2-3 interior controls
    let mut ctrl = low_controls(relief, mouth, far, n_ctrl, rng);
    let mut base_pts = vec![mouth];
    if let Some((up_dir, _)) = join {
        // pin the initial tangent: an extra control 320 m up the desired
        // approach direction, BEFORE the low controls — and drop any low
        // control that sits BEHIND that approach, which folds the baseline
        // into a hairpin at the junction (measured: 13 m radius) that the
        // smoother cannot heal against a pinned endpoint.
        base_pts.push(Vec2::new(mouth.x + up_dir.x * 320.0, mouth.y + up_dir.y * 320.0));
        ctrl.retain(|c| {
            let v = Vec2::new(c.x - mouth.x, c.y - mouth.y);
            v.length() > 500.0 && v.normalized().dot(up_dir) > 0.2
        });
    }
    base_pts.extend(ctrl);
    base_pts.push(far);
    let base = Spine::new(catmull_rom(&base_pts, 20.0));

    // --- long-wave meander in the heading domain
    let lam = rng.range_f64(p.lam_m.0, p.lam_m.1);
    let swing = rng.range_f64(p.swing_rad.0, p.swing_rad.1);
    let phase = rng.range_f64(0.0, core::f64::consts::TAU);
    let phase2 = rng.range_f64(0.0, core::f64::consts::TAU);
    let total = base.length();
    let step = 20.0;
    let n = (total / step).ceil() as usize;

    let mut pts = Vec::with_capacity(n + 1);
    let mut cur = mouth;
    pts.push(cur);
    let mut arc = 0.0;
    for _ in 0..n {
        let s = (arc / total).min(1.0);
        let tan = base.tangent_at(s);
        let base_th = math::atan2(tan.y, tan.x);
        // fundamental + weak second harmonic so it does not read as a
        // regular scallop (the heartland lesson, reused as knowledge).
        let wob = swing
            * (math::sin(core::f64::consts::TAU * arc / lam + phase)
                + 0.28 * math::sin(core::f64::consts::TAU * arc / (0.41 * lam) + phase2));
        // taper the wind near both ENDS so terminals stay honest, and near
        // the BORDERS so a bend is never pushed off the tile. The border
        // taper replaces a hard clamp, which creased bends into 36-43 m
        // curvature corners (measured) that a single smoothing pass could
        // not heal.
        let d_border = cur.x.min(cur.y).min(EXTENT_M - cur.x).min(EXTENT_M - cur.y);
        let taper = (arc / 400.0).min(1.0)
            * (((total - arc) / 400.0).clamp(0.0, 1.0))
            * (d_border / 300.0).clamp(0.0, 1.0);
        let th = base_th + wob * taper;
        cur = Vec2::new(cur.x + math::cos(th) * step, cur.y + math::sin(th) * step);
        arc += step;
        pts.push(cur);
    }
    // --- re-anchor: winding drifts the endpoint; pull it back linearly
    let drift = far - *pts.last().unwrap();
    let m = (pts.len() - 1) as f64;
    for (i, q) in pts.iter_mut().enumerate() {
        let t = i as f64 / m;
        *q = Vec2::new(q.x + drift.x * t, q.y + drift.y * t);
    }
    // The border taper keeps bends inside the tile; the drift re-anchor can
    // still leave a whisker outside, so clamp — and then ENFORCE the radius
    // floor by iterated 1-2-1 smoothing (endpoints pinned). Smoothing only
    // straightens, so it converges; the assert in the tests is what keeps
    // this honest rather than cosmetic.
    for q in pts.iter_mut() {
        q.x = q.x.clamp(0.0, EXTENT_M);
        q.y = q.y.clamp(0.0, EXTENT_M);
    }
    for _ in 0..240 {
        if Spine::new(pts.clone()).min_curvature_radius() >= p.min_radius_m {
            break;
        }
        let prev = pts.clone();
        for i in 1..pts.len() - 1 {
            pts[i] = Vec2::new(
                0.25 * prev[i - 1].x + 0.5 * prev[i].x + 0.25 * prev[i + 1].x,
                0.25 * prev[i - 1].y + 0.5 * prev[i].y + 0.25 * prev[i + 1].y,
            );
        }
    }

    // --- long profile: concave, zero at the mouth, monotone by construction
    let spine = Spine::new(pts.clone());
    let total = spine.length();
    let hh = relief_budget_m * p.head_relief_frac;
    let z0 = join.map(|(_, jz)| jz).unwrap_or(0.0);
    let mut z = Vec::with_capacity(pts.len());
    let mut acc = 0.0;
    for (i, q) in pts.iter().enumerate() {
        if i > 0 {
            acc += q.distance(pts[i - 1]);
        }
        z.push(z0 + hh * math::pow((acc / total).clamp(0.0, 1.0), p.concavity));
    }

    TrunkPath { pts, z, external_km2, joins: None }
}
