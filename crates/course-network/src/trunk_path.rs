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
            // VALLEY-AXIS scale (user direction, U-series): a trunk is the
            // valley spine, not the watercourse. lambda >= 2 km, often much
            // higher — gently curving lines. The river's own 400-900 m
            // meander returns at hydrology, drawn INSIDE the valley floor.
            lam_m: (2000.0, 6000.0),
            swing_rad: (0.30, 0.85),
            head_relief_frac: 0.55,
            concavity: 1.45,
            // Raised 150 -> 500: creases live where the medial axis comes
            // close, and medial-axis distance scales with curvature radius.
            min_radius_m: 500.0,
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
        // Narrow corridor: at 0.22·len the low-controls could sit 900 m
        // off-axis, and the catmull baseline through them carried 70 m-radius
        // bends that no amount of point smoothing repairs (diffusion barely
        // touches low-frequency curvature). The valley axis wants gentle.
        let half_w = (len * 0.10).min(350.0);
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

/// Golf holes must sit 500 m off the border, so the trunk's landforms must
/// reach the playable middle: a trunk is REQUIRED to spend at least this
/// much arc inside the central 2×2 km square (user rule). Enforced by
/// bowing the baseline controls toward the centre until it holds — the
/// endpoints stay anchored on their edges.
pub const CORE_MIN_ARC_M: f64 = 500.0;

fn arc_inside_core(pts: &[Vec2]) -> f64 {
    let lo = 500.0;
    let hi = EXTENT_M - 500.0;
    pts.windows(2)
        .filter(|w| {
            let m = Vec2::new((w[0].x + w[1].x) * 0.5, (w[0].y + w[1].y) * 0.5);
            m.x >= lo && m.x <= hi && m.y >= lo && m.y <= hi
        })
        .map(|w| w[0].distance(w[1]))
        .sum()
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
    // Relax the control polygon: pull any control whose turn angle exceeds
    // ~22 deg toward its neighbours' midpoint. The radius floor must be in
    // the BASELINE; enforcement smoothing is only a belt.
    for _ in 0..24 {
        let mut worst = 0.0f64;
        for i in 1..base_pts.len() - 1 {
            let a = base_pts[i - 1];
            let b = base_pts[i];
            let c = base_pts[i + 1];
            let v1 = (b - a).normalized();
            let v2 = (c - b).normalized();
            let dot = v1.dot(v2).clamp(-1.0, 1.0);
            let ang = math::acos(dot);
            if ang > worst {
                worst = ang;
            }
            if ang > 0.38 {
                let mid = Vec2::new((a.x + c.x) * 0.5, (a.y + c.y) * 0.5);
                base_pts[i] = base_pts[i].lerp(mid, 0.35);
            }
        }
        if worst <= 0.38 {
            break;
        }
    }
    // CORE RULE: the FINAL wound path must spend CORE_MIN_ARC_M inside the
    // central 2x2 km square (holes sit 500 m off the border, and the trunk's
    // landforms must reach them). Enforced by bowing the interior baseline
    // controls toward the centre and REBUILDING with the same draws — the
    // first version checked a baseline probe, and the meander wind then
    // shifted the built path 48 m short of the requirement.
    let centre = Vec2::new(EXTENT_M * 0.5, EXTENT_M * 0.5);
    let mut pts: Vec<Vec2>;
    let mut attempts = 0;
    loop {
        let base = Spine::new(catmull_rom(&base_pts, 20.0));

        // --- long-wave meander in the heading domain
        let lam = rng.range_f64(p.lam_m.0, p.lam_m.1);
        // swing scales with lambda: the curvature bound is radius ~ lam/(2pi*swing*1.68),
        // so a fixed swing wastes the long wavelengths' headroom. Bound to the
        // radius floor with 15% margin.
        let swing_cap = lam / (p.min_radius_m * core::f64::consts::TAU * 1.68) * 0.85;
        let swing = rng.range_f64(p.swing_rad.0, p.swing_rad.1.min(swing_cap.max(p.swing_rad.0)));
        let phase = rng.range_f64(0.0, core::f64::consts::TAU);
        let phase2 = rng.range_f64(0.0, core::f64::consts::TAU);
        let total = base.length();
        let step = 20.0;
        let n = (total / step).ceil() as usize;

        let mut wpts: Vec<Vec2> = Vec::with_capacity(n + 1);
        let mut cur = mouth;
        wpts.push(cur);
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
            wpts.push(cur);
        }
        // --- re-anchor: winding drifts the endpoint; pull it back linearly
        let drift = far - *wpts.last().unwrap();
        let m = (wpts.len() - 1) as f64;
        for (i, q) in wpts.iter_mut().enumerate() {
            let t = i as f64 / m;
            *q = Vec2::new(q.x + drift.x * t, q.y + drift.y * t);
        }
        // The border taper keeps bends inside the tile; the drift re-anchor can
        // still leave a whisker outside, so clamp — and then ENFORCE the radius
        // floor by iterated 1-2-1 smoothing (endpoints pinned). Smoothing only
        // straightens, so it converges; the assert in the tests is what keeps
        // this honest rather than cosmetic.
        for q in wpts.iter_mut() {
            q.x = q.x.clamp(0.0, EXTENT_M);
            q.y = q.y.clamp(0.0, EXTENT_M);
        }
        for _ in 0..240 {
            if Spine::new(wpts.clone()).min_curvature_radius() >= p.min_radius_m {
                break;
            }
            let prev = wpts.clone();
            for i in 1..wpts.len() - 1 {
                wpts[i] = Vec2::new(
                    0.25 * prev[i - 1].x + 0.5 * prev[i].x + 0.25 * prev[i + 1].x,
                    0.25 * prev[i - 1].y + 0.5 * prev[i].y + 0.25 * prev[i + 1].y,
                );
            }
        }
        pts = wpts;

        if arc_inside_core(&pts) >= CORE_MIN_ARC_M || attempts >= 8 {
            break;
        }
        attempts += 1;
        let n_ctrl_pts = base_pts.len();
        for q in base_pts.iter_mut().skip(1).take(n_ctrl_pts.saturating_sub(2)) {
            *q = q.lerp(centre, 0.22);
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
