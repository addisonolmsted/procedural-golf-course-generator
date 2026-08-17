//! Dendritic micro-gully synthesis at 2 m.
//!
//! The groove audit (2026-08, real-holdout lidar vs generated) showed the
//! defining texture of dissected terrain is CONNECTED 10-100 m gully
//! trees draining into the channel network: real groove connectivity is
//! 62-88% by length with strong downslope alignment, while the generated
//! surface carried the same groove density as disconnected, weakly
//! aligned fragments (15-50%). Those trees cannot come from the 8 m S2
//! carve (a 10-20 m gully is at/under Nyquist there, and the catena
//! imposes a ~100 m minimum valley footprint), and the assemble step
//! deliberately lowpasses the base at 64 m — so the trees are grown here,
//! on the 2 m surface, after the channel-restore clamp.
//!
//! Mechanics (pure functions of position + stream seed — deterministic,
//! order-independent stamping via min()):
//! - SEEDS: every 8 m channel cell flips a position-keyed coin with
//!   p = (8 / SEED_SPACING_M) * density, then offsets to the bank toe
//!   perpendicular to local flow.
//! - GROWTH: steps upslope along a blend of the smoothed 2 m gradient
//!   and the reversed flow direction, with hash jitter; stops on flat
//!   ground, at the reach limit, at the tile margin, or when it merges
//!   into an already-carved gully (merging is the point — it builds the
//!   tree).
//! - CARVE: cut-only stamp below the SMOOTHED surface; depth and width
//!   taper head-ward (u^1.2 / u^0.7), cross-section (1-|t/hw|^1.5) with
//!   a smoothstep lip so nothing daylights as a hard edge; depth
//!   modulated along-arc at ~9 m correlation (the real knick spacing).
//!   The mouth overshoots a few metres toward the channel so the notch
//!   opens into the bank instead of hanging.

use course_world::grid::Grid;
use course_world::math::Vec2;

const SEED_SPACING_M: f64 = 28.0;
const SEED_TOE_M: f64 = 12.0;
const STEP_M: f64 = 2.0;
const SMOOTH_SIGMA_CELLS: f64 = 4.0; // 8 m on the 2 m grid
const GRAD_W: f64 = 0.6;
const FLOW_W: f64 = 0.4;
const JITTER_RAD: f64 = 0.17; // ~10 deg
const MIN_SLOPE: f64 = 0.006;
const TRUNK_MIN_M: f64 = 30.0;
const BRANCH_FRACS: [f64; 2] = [0.35, 0.7];
const BRANCH_ANGLE_LO: f64 = 0.44; // 25 deg
const BRANCH_ANGLE_HI: f64 = 0.70; // 40 deg
const CHILD_LEN_FRAC: f64 = 0.6;
const MOUTH_W_LO: f64 = 6.0;
const MOUTH_W_HI: f64 = 20.0;
const DEPTH_LOGNORM_SIGMA: f64 = 0.46;
const DEPTH_CLAMP_M: f64 = 1.6;
const KNICK_CORR_M: f64 = 9.0;
const KNICK_AMP: f64 = 0.35;
const LIP_FRAC: f64 = 0.8; // smoothstep lip over the outer 20% of hw
const MOUTH_OVERSHOOT_M: f64 = 6.0;
const EDGE_MARGIN_M: f64 = 30.0;
const GULLY_SALT: u64 = 0x6011_7EE5;
const MAX_GEN: u32 = 3;
const NEXTGEN_SPACING_M: f64 = 24.0;
const NEXTGEN_P: f64 = 0.65;

struct Params {
    density: f64,
    depth_p50: f64,
    reach_m: f64,
    max_len_m: f64,
    branch_p: f64,
}

fn hash01(seed: u64, x: i64, y: i64, salt: u64) -> f64 {
    let mut z = seed
        ^ (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
        ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F)
        ^ salt.rotate_left(17);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    (z >> 11) as f64 / (1u64 << 53) as f64
}

/// 1-D value noise in [-1, 1] over arc, knot spacing KNICK_CORR_M.
fn knick(seed: u64, tree: u64, arc: f64) -> f64 {
    let k = (arc / KNICK_CORR_M).floor();
    let t = arc / KNICK_CORR_M - k;
    let a = hash01(seed, k as i64, tree as i64, GULLY_SALT ^ 0xA1);
    let b = hash01(seed, k as i64 + 1, tree as i64, GULLY_SALT ^ 0xA1);
    let s = t * t * (3.0 - 2.0 * t);
    2.0 * (a + (b - a) * s) - 1.0
}

/// Grow + carve the dendritic gully trees. `height` is the 2 m surface
/// AFTER the channel-restore clamp. No-op when the density dial is 0.
pub fn carve_gullies(
    height: &mut Grid<f64>,
    base_lp64: &[f64],
    flow_distance: &Grid<f64>,
    flow_dir_rad: &Grid<f64>,
    dials: &std::collections::BTreeMap<String, f64>,
    seed: u64,
) {
    let p = Params {
        density: dials.get("amplify.gully_density").copied().unwrap_or(0.0),
        depth_p50: dials.get("amplify.gully_depth_p50_m").copied().unwrap_or(0.45),
        reach_m: dials.get("amplify.gully_reach_m").copied().unwrap_or(180.0),
        max_len_m: dials.get("amplify.gully_max_len_m").copied().unwrap_or(110.0),
        branch_p: dials.get("amplify.gully_branch_prob").copied().unwrap_or(0.6),
    };
    if p.density <= 0.0 {
        return;
    }
    let n2 = height.spec.nx as usize;
    let cell2 = height.spec.cell_size;
    let n8 = flow_distance.spec.nx as usize;

    // smoothed surface: gradient reference + carve datum (three box
    // passes ~ Gaussian sigma 4 cells)
    let zs = {
        let box_w = ((SMOOTH_SIGMA_CELLS * 1.153) as usize * 2 + 1).max(3);
        let mut lp = height.data.clone();
        for _ in 0..3 {
            lp = crate::synth::box_filter(&lp, n2, box_w);
        }
        lp
    };

    let bil8 = |v: &[f64], p: Vec2| -> f64 {
        // NODE convention (world_of = x*cell): no half-cell shift
        let cx = (p.x / 8.0).clamp(0.0, (n8 - 1) as f64);
        let cy = (p.y / 8.0).clamp(0.0, (n8 - 1) as f64);
        let (x0, y0) = (cx as usize, cy as usize);
        let (x1, y1) = ((x0 + 1).min(n8 - 1), (y0 + 1).min(n8 - 1));
        let (tx, ty) = (cx - x0 as f64, cy - y0 as f64);
        let a = v[y0 * n8 + x0] * (1.0 - tx) + v[y0 * n8 + x1] * tx;
        let b = v[y1 * n8 + x0] * (1.0 - tx) + v[y1 * n8 + x1] * tx;
        a * (1.0 - ty) + b * ty
    };
    // STEERING gradient reads the 64 m-lowpassed base — the pure
    // hillslope. Steering on the 8 m-smoothed final surface let the
    // ±0.4 m texture bumps walk the trunks in circles; real gullies run
    // straight down the hillslope fall line. Sampled at ±6 m so crest
    // detection stays crisp.
    let grad2 = |pt: Vec2| -> (f64, f64) {
        let x = (pt.x / cell2).clamp(3.0, (n2 - 4) as f64) as usize;
        let y = (pt.y / cell2).clamp(3.0, (n2 - 4) as f64) as usize;
        let gx = (base_lp64[y * n2 + x + 3] - base_lp64[y * n2 + x - 3]) / (6.0 * cell2);
        let gy = (base_lp64[(y + 3) * n2 + x] - base_lp64[(y - 3) * n2 + x]) / (6.0 * cell2);
        (gx, gy)
    };
    let tile = cell2 * n2 as f64;

    // carved-cell mask for merge termination (grown as trees stamp)
    let mut carved = vec![false; n2 * n2];

    // one polyline = (points every STEP_M walking MOUTH -> HEAD)
    struct Tree {
        pts: Vec<Vec2>,
        mouth_w: f64,
        depth_mouth: f64,
        id: u64,
        level: u32,
    }
    let mut trees: Vec<Tree> = Vec::new();

    // ---- seed + grow: multi-generation work queue ---------------------
    // Generation 1 mouths sit on the S2 channel cells (row-major order).
    // Each accepted tree then enqueues generation+1 mouths along its own
    // trunk — gullies seeding gullies is what lets the network reach the
    // interfluves (the S2 network runs ~2.4 km/km², spacing ~450 m; a
    // single 150 m generation only decorated the trunk lines).
    struct Mouth {
        pos: Vec2,
        heading: (f64, f64),
        depth: f64,
        width: f64,
        len_draw: f64,
        level: u32,
        id: u64,
        /// steering-target rotation, decaying over the first ~50 m of
        /// arc: a child's shared smoothed-gradient target otherwise
        /// converges it straight back onto its parent's upper trunk
        /// (measured: 90% of level-2 trunks merged before 18 m)
        bias: f64,
    }
    let mut queue: std::collections::VecDeque<Mouth> = std::collections::VecDeque::new();
    let p_seed = (8.0 / SEED_SPACING_M) * p.density;
    for y8 in 0..n8 {
        for x8 in 0..n8 {
            if flow_distance.data[y8 * n8 + x8] > 0.0 {
                continue;
            }
            if hash01(seed, x8 as i64, y8 as i64, GULLY_SALT) >= p_seed {
                continue;
            }
            let id = (y8 * n8 + x8) as u64;
            let p8 = flow_distance.spec.world_of(x8 as u32, y8 as u32);
            let a = flow_dir_rad.data[y8 * n8 + x8];
            if !a.is_finite() {
                continue;
            }
            let side = if hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0x51DE) < 0.5 {
                1.0
            } else {
                -1.0
            };
            let (nx, ny) = (-libm::sin(a) * side, libm::cos(a) * side);
            let toe = SEED_TOE_M * (0.7 + 0.6 * hash01(seed, x8 as i64, y8 as i64, 0x70E));
            let mouth = Vec2::new(p8.x + nx * toe, p8.y + ny * toe);
            let len_draw = TRUNK_MIN_M
                + (p.max_len_m - TRUNK_MIN_M)
                    * hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0x1E4);
            let mouth_w = MOUTH_W_LO
                + (MOUTH_W_HI - MOUTH_W_LO)
                    * hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0x3D);
            let u1 = hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0xD1).max(1e-9);
            let u2 = hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0xD2);
            let gauss = (-2.0 * u1.ln()).sqrt() * libm::cos(std::f64::consts::TAU * u2);
            let depth_mouth =
                (p.depth_p50 * libm::exp(DEPTH_LOGNORM_SIGMA * gauss)).min(DEPTH_CLAMP_M);
            queue.push_back(Mouth {
                pos: mouth,
                heading: (nx, ny),
                depth: depth_mouth,
                width: mouth_w,
                len_draw,
                level: 1,
                id,
                bias: 0.0,
            });
        }
    }
    // Interior seeds: concave, sloped swale points anywhere on the tile
    // (the S2 network runs sparse — channel-cell seeding alone left the
    // interfluves bare, groove audit round 9). Their connectivity is
    // guaranteed by the OUTLET WALK below: every tree first descends to
    // the drainage; failures are discarded whole.
    let p_int = 0.030 * p.density;
    for y8 in 2..n8 - 2 {
        for x8 in 2..n8 - 2 {
            let d = flow_distance.data[y8 * n8 + x8];
            if d < 40.0 {
                continue;
            }
            if hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0x1A7E) >= p_int {
                continue;
            }
            let p8 = flow_distance.spec.world_of(x8 as u32, y8 as u32);
            let (gx, gy) = grad2(p8);
            let slope = (gx * gx + gy * gy).sqrt();
            if slope < 0.012 {
                continue;
            }
            // concavity on the hillslope surface (8 m Laplacian > 0)
            let lap = {
                let x = (p8.x / cell2) as usize;
                let y = (p8.y / cell2) as usize;
                if x < 4 || y < 4 || x >= n2 - 4 || y >= n2 - 4 {
                    continue;
                }
                base_lp64[y * n2 + x + 4] + base_lp64[y * n2 + x - 4]
                    + base_lp64[(y + 4) * n2 + x]
                    + base_lp64[(y - 4) * n2 + x]
                    - 4.0 * base_lp64[y * n2 + x]
            };
            if lap <= 0.0 {
                continue;
            }
            let id = (y8 * n8 + x8) as u64 | 0x4000_0000;
            let len_draw = TRUNK_MIN_M
                + (p.max_len_m - TRUNK_MIN_M)
                    * hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0x1E4);
            let mouth_w = MOUTH_W_LO
                + (MOUTH_W_HI - MOUTH_W_LO)
                    * hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0x3D);
            let u1 = hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0xD1).max(1e-9);
            let u2 = hash01(seed, x8 as i64, y8 as i64, GULLY_SALT ^ 0xD2);
            let gauss = (-2.0 * u1.ln()).sqrt() * libm::cos(std::f64::consts::TAU * u2);
            let depth_mouth =
                (p.depth_p50 * libm::exp(DEPTH_LOGNORM_SIGMA * gauss)).min(DEPTH_CLAMP_M);
            queue.push_back(Mouth {
                pos: p8,
                heading: (gx / slope, gy / slope),
                depth: depth_mouth,
                width: mouth_w,
                len_draw,
                level: 1,
                id,
                bias: 0.0,
            });
        }
    }
    while let Some(m) = queue.pop_front() {
        if m.pos.x < EDGE_MARGIN_M
            || m.pos.y < EDGE_MARGIN_M
            || m.pos.x > tile - EDGE_MARGIN_M
            || m.pos.y > tile - EDGE_MARGIN_M
        {
            continue;
        }
        // OUTLET WALK: descend from the seed to the drainage first —
        // steepest hillslope descent nudged toward the channel. A tree
        // whose descent stalls before reaching a channel corridor or an
        // existing gully is discarded whole (connectivity by
        // construction). Channel-toe seeds satisfy the target at once.
        let mut outlet: Vec<Vec2> = Vec::new();
        {
            let mut cur = m.pos;
            let mut ok = bil8(&flow_distance.data, cur) < 16.0;
            let (mut ohx, mut ohy) = (0.0f64, 0.0f64);
            let mut oarc = 0.0;
            while !ok && oarc < 300.0 {
                let (gx, gy) = grad2(cur);
                let slope = (gx * gx + gy * gy).sqrt();
                if slope < 0.004 {
                    break;
                }
                let ddx = bil8(&flow_distance.data, Vec2::new(cur.x + 4.0, cur.y))
                    - bil8(&flow_distance.data, Vec2::new(cur.x - 4.0, cur.y));
                let ddy = bil8(&flow_distance.data, Vec2::new(cur.x, cur.y + 4.0))
                    - bil8(&flow_distance.data, Vec2::new(cur.x, cur.y - 4.0));
                let dl8 = (ddx * ddx + ddy * ddy).sqrt().max(1e-9);
                let mut tx = -0.7 * gx / slope - 0.3 * ddx / dl8;
                let mut ty = -0.7 * gy / slope - 0.3 * ddy / dl8;
                let tl = (tx * tx + ty * ty).sqrt().max(1e-9);
                tx /= tl;
                ty /= tl;
                if oarc == 0.0 {
                    ohx = tx;
                    ohy = ty;
                } else {
                    ohx = 0.5 * ohx + 0.5 * tx;
                    ohy = 0.5 * ohy + 0.5 * ty;
                    let hl = (ohx * ohx + ohy * ohy).sqrt().max(1e-9);
                    ohx /= hl;
                    ohy /= hl;
                }
                cur = Vec2::new(cur.x + STEP_M * ohx, cur.y + STEP_M * ohy);
                oarc += STEP_M;
                if cur.x < EDGE_MARGIN_M
                    || cur.y < EDGE_MARGIN_M
                    || cur.x > tile - EDGE_MARGIN_M
                    || cur.y > tile - EDGE_MARGIN_M
                {
                    break;
                }
                outlet.push(cur);
                let ci = (cur.y / cell2) as usize * n2 + (cur.x / cell2) as usize;
                if bil8(&flow_distance.data, cur) < 16.0
                    || (oarc > 6.0 && carved[ci.min(n2 * n2 - 1)])
                {
                    ok = true;
                }
            }
            if !ok {
                continue;
            }
        }
        // grow trunk mouth -> head
        let mut pts = vec![m.pos];
        let mut cur = m.pos;
        let mut arc = 0.0;
        let (mut hx, mut hy) = m.heading;
        let reach = p.reach_m * m.level as f64;
        while arc < m.len_draw {
            let (gx, gy) = grad2(cur);
            let slope = (gx * gx + gy * gy).sqrt();
            if slope < MIN_SLOPE {
                break;
            }
            // away-from-channel = uphill of the flow-DISTANCE field
            // (reversed flow_dir pointed upstream ALONG the channel and
            // grew worms hugging the banks — first build)
            let ddx = bil8(&flow_distance.data, Vec2::new(cur.x + 4.0, cur.y))
                - bil8(&flow_distance.data, Vec2::new(cur.x - 4.0, cur.y));
            let ddy = bil8(&flow_distance.data, Vec2::new(cur.x, cur.y + 4.0))
                - bil8(&flow_distance.data, Vec2::new(cur.x, cur.y - 4.0));
            let dl8 = (ddx * ddx + ddy * ddy).sqrt().max(1e-9);
            let (ux, uy) = (gx / slope, gy / slope);
            let mut tx = GRAD_W * ux + FLOW_W * ddx / dl8;
            let mut ty = GRAD_W * uy + FLOW_W * ddy / dl8;
            // smooth low-frequency wiggle (12 m knots), not per-step
            // noise — a random walk curled the trunks
            let j = knick(seed, m.id ^ 0x77, arc * KNICK_CORR_M / 12.0) * JITTER_RAD
                + m.bias * (1.0 - arc / 50.0).max(0.0);
            let (cj, sj) = (libm::cos(j), libm::sin(j));
            let (rx, ry) = (tx * cj - ty * sj, tx * sj + ty * cj);
            tx = rx;
            ty = ry;
            // heading momentum: curves, not kinks
            let tl = (tx * tx + ty * ty).sqrt().max(1e-9);
            // crest stop: when the uphill target has swung behind the
            // current heading the trunk has topped its slope — carrying
            // on curled the trunks into hooks (first build's C-shapes)
            if arc > 10.0 && (hx * tx + hy * ty) / tl < -0.1 {
                break;
            }
            hx = 0.5 * hx + 0.5 * tx / tl;
            hy = 0.5 * hy + 0.5 * ty / tl;
            let hl = (hx * hx + hy * hy).sqrt().max(1e-9);
            hx /= hl;
            hy /= hl;
            cur = Vec2::new(cur.x + STEP_M * hx, cur.y + STEP_M * hy);
            if cur.x < EDGE_MARGIN_M
                || cur.y < EDGE_MARGIN_M
                || cur.x > tile - EDGE_MARGIN_M
                || cur.y > tile - EDGE_MARGIN_M
            {
                break;
            }
            if bil8(&flow_distance.data, cur) > reach {
                break;
            }
            let ci = (cur.y / cell2) as usize * n2 + (cur.x / cell2) as usize;
            let merged = carved[ci.min(n2 * n2 - 1)];
            pts.push(cur);
            arc += STEP_M;
            // a child mouth STARTS on its parent's carved cells; only
            // merges past the first few steps end the trunk
            if merged && arc > 10.0 {
                break;
            }
        }
        let min_len = if m.level == 1 { TRUNK_MIN_M } else { 18.0 };
        if (pts.len() as f64 - 1.0) * STEP_M < min_len {
            continue;
        }
        // full polyline runs channel-end -> seed -> head so the depth
        // taper u is anchored at the true drainage end
        if !outlet.is_empty() {
            outlet.reverse();
            outlet.extend(pts);
            pts = outlet;
        }
        let trunk_len = (pts.len() as f64 - 1.0) * STEP_M;
        let trunk_pts = pts.clone();
        // Short crest-stopped trees: THIN them probabilistically rather
        // than shallow them uniformly. Full-depth 30 m stubs in rows read
        // as manufactured dashes; uniformly-shallowed stubs dropped below
        // the 0.25 m groove-detection floor and left nothing for the
        // fragment population to chain into (the low-relief-vs-high-relief
        // connectivity split, groove audit round 8). Survivors keep most
        // of their depth and an irregular rhythm.
        let lf = ((trunk_len - 24.0) / 56.0).clamp(0.0, 1.0);
        let keep_p = lf.max(0.2);
        if hash01(seed, m.id as i64, 0x1F5, GULLY_SALT ^ 0x1F5) > keep_p {
            continue;
        }
        let depth_eff = m.depth * (0.6 + 0.4 * lf * lf * (3.0 - 2.0 * lf));
        let mut all =
            vec![Tree { pts, mouth_w: m.width, depth_mouth: depth_eff, id: m.id, level: m.level }];
        // in-tree branches part-way up the trunk
        for (bi, bf) in BRANCH_FRACS.iter().enumerate() {
            if hash01(seed, m.id as i64, bi as i64, GULLY_SALT ^ 0xB0) >= p.branch_p {
                continue;
            }
            let root_idx = ((bf * trunk_len / STEP_M) as usize).min(all[0].pts.len() - 2);
            let root = all[0].pts[root_idx];
            let (ax, ay) = {
                let a2 = all[0].pts[root_idx + 1];
                (a2.x - root.x, a2.y - root.y)
            };
            let ang = BRANCH_ANGLE_LO
                + (BRANCH_ANGLE_HI - BRANCH_ANGLE_LO)
                    * hash01(seed, m.id as i64, bi as i64, GULLY_SALT ^ 0xC0);
            let sgn = if hash01(seed, m.id as i64, bi as i64, GULLY_SALT ^ 0xE0) < 0.5 {
                ang
            } else {
                -ang
            };
            let (ca, sa) = (libm::cos(sgn), libm::sin(sgn));
            let (mut dx, mut dy) = (ax * ca - ay * sa, ax * sa + ay * ca);
            let child_len = (trunk_len * (1.0 - bf) * CHILD_LEN_FRAC).max(16.0);
            let mut cpts = vec![root];
            let mut ccur = root;
            let mut carc = 0.0;
            while carc < child_len {
                let (gx, gy) = grad2(ccur);
                let slope = (gx * gx + gy * gy).sqrt();
                if slope < MIN_SLOPE {
                    break;
                }
                let (ux, uy) = (gx / slope, gy / slope);
                let dl0 = (dx * dx + dy * dy).sqrt().max(1e-9);
                let mx = 0.35 * ux + 0.65 * dx / dl0;
                let my = 0.35 * uy + 0.65 * dy / dl0;
                let dl = (mx * mx + my * my).sqrt().max(1e-9);
                dx = mx / dl;
                dy = my / dl;
                ccur = Vec2::new(ccur.x + STEP_M * dx, ccur.y + STEP_M * dy);
                if ccur.x < EDGE_MARGIN_M
                    || ccur.y < EDGE_MARGIN_M
                    || ccur.x > tile - EDGE_MARGIN_M
                    || ccur.y > tile - EDGE_MARGIN_M
                    || bil8(&flow_distance.data, ccur) > reach
                {
                    break;
                }
                cpts.push(ccur);
                carc += STEP_M;
            }
            if (cpts.len() as f64 - 1.0) * STEP_M >= 16.0 {
                let up = 1.0 - *bf;
                let d_at_root = depth_eff * libm::pow(up.max(0.0), 1.2).max(0.15);
                all.push(Tree {
                    pts: cpts,
                    mouth_w: (m.width * 0.6).max(MOUTH_W_LO),
                    depth_mouth: d_at_root.min(depth_eff),
                    id: m.id ^ (0xB << bi),
                    level: m.level,
                });
            }
        }
        // mark carved mask so later trees can merge-terminate into this
        for t in &all {
            for pt in &t.pts {
                let cx = (pt.x / cell2) as usize;
                let cy = (pt.y / cell2) as usize;
                for oy in cy.saturating_sub(2)..=(cy + 2).min(n2 - 1) {
                    for ox in cx.saturating_sub(2)..=(cx + 2).min(n2 - 1) {
                        carved[oy * n2 + ox] = true;
                    }
                }
            }
        }
        // enqueue the NEXT generation along this trunk: tributary mouths
        // whose depth matches the parent's local profile (continuous
        // junctions), heading rotated off the parent axis
        if m.level < MAX_GEN {
            let mut karc = NEXTGEN_SPACING_M;
            while karc < trunk_len {
                let k = (karc / STEP_M) as usize;
                if k + 1 >= trunk_pts.len() {
                    break;
                }
                let key = (m.id as i64, (karc as i64) << 3 | m.level as i64);
                if hash01(seed, key.0, key.1, GULLY_SALT ^ 0xF7) < NEXTGEN_P * p.density {
                    let root = trunk_pts[k];
                    let (ax, ay) = {
                        let a2 = trunk_pts[k + 1];
                        let l = ((a2.x - root.x).powi(2) + (a2.y - root.y).powi(2))
                            .sqrt()
                            .max(1e-9);
                        ((a2.x - root.x) / l, (a2.y - root.y) / l)
                    };
                    let ang = 0.9
                        + 0.5 * hash01(seed, key.0, key.1, GULLY_SALT ^ 0xF8); // 52-80 deg
                    let sgn =
                        if hash01(seed, key.0, key.1, GULLY_SALT ^ 0xF9) < 0.5 { ang } else { -ang };
                    let (ca, sa) = (libm::cos(sgn), libm::sin(sgn));
                    let u_here = 1.0 - karc / trunk_len;
                    let d_here = depth_eff * libm::pow(u_here.max(0.0), 1.2);
                    queue.push_back(Mouth {
                        pos: root,
                        heading: (ax * ca - ay * sa, ax * sa + ay * ca),
                        depth: (d_here * 0.85).max(0.12),
                        width: (m.width * 0.7).max(4.0),
                        len_draw: (m.len_draw * 0.8).max(24.0),
                        level: m.level + 1,
                        id: m.id
                            ^ ((karc as u64) << 17)
                            ^ 0x9E37_79B9,
                        bias: sgn,
                    });
                }
                karc += NEXTGEN_SPACING_M;
            }
        }
        trees.extend(all);
    }

    if std::env::var("GULLY_TRACE").is_ok() {
        let total_len: f64 = trees.iter().map(|t| (t.pts.len() as f64 - 1.0) * STEP_M).sum();
        let by: Vec<usize> =
            (1..=MAX_GEN).map(|g| trees.iter().filter(|t| t.level == g).count()).collect();
        eprintln!(
            "  gullies: {} trees (levels {:?}), total {:.0} m, mean len {:.0} m",
            trees.len(),
            by,
            total_len,
            total_len / trees.len().max(1) as f64
        );
    }
    // ---- carve: cut-only min() stamp below the smoothed surface -------
    // Datum FROZEN before any cutting: taking min(zs, height) live made
    // overlapping trees' cuts cumulative (converging outlet runs dug
    // 6.8 m pits into channel beds — caught by the channel-profile
    // invariant). Frozen, min() stamping is idempotent.
    let datum: Vec<f64> =
        zs.iter().zip(height.data.iter()).map(|(a, b)| a.min(*b)).collect();
    for t in &trees {
        let total = (t.pts.len() as f64 - 1.0) * STEP_M;
        if total <= 0.0 {
            continue;
        }
        for (k, pt) in t.pts.iter().enumerate() {
            let arc = k as f64 * STEP_M;
            // u: 1 at the mouth (k=0), 0 at the head; the mouth-ward
            // overshoot keeps u pinned at 1 so the notch opens into the
            // bank at full depth
            let u = (1.0 - (arc - MOUTH_OVERSHOOT_M).max(0.0) / total).clamp(0.0, 1.0);
            let hw = 1.0 + (0.5 * t.mouth_w - 1.0) * libm::pow(u, 0.7);
            // real gullies HOLD incision along the run and die fast at
            // the head — a power-law taper left most of the trunk under
            // the 0.25 m detection floor and the lines read chopped
            let head_arc = total + MOUTH_OVERSHOOT_M - arc;
            let ht = (head_arc / 14.0).clamp(0.0, 1.0);
            let d0 = t.depth_mouth * (0.55 + 0.45 * u) * (ht * ht * (3.0 - 2.0 * ht));
            let d = d0 * (1.0 + KNICK_AMP * knick(seed, t.id, arc));
            if d < 0.03 {
                continue;
            }
            let r = (hw / cell2).ceil() as i64;
            let (cx, cy) = ((pt.x / cell2) as i64, (pt.y / cell2) as i64);
            for oy in (cy - r).max(0)..=(cy + r).min(n2 as i64 - 1) {
                for ox in (cx - r).max(0)..=(cx + r).min(n2 as i64 - 1) {
                    let (wx, wy) = (ox as f64 * cell2, oy as f64 * cell2);
                    let dist = ((wx - pt.x).powi(2) + (wy - pt.y).powi(2)).sqrt();
                    let tt = dist / hw;
                    if tt >= 1.0 {
                        continue;
                    }
                    // (1-t^1.5) core with a smoothstep lip on the outer 20%
                    let mut cut = d * (1.0 - libm::pow(tt, 1.5));
                    if tt > LIP_FRAC {
                        let s = (1.0 - (tt - LIP_FRAC) / (1.0 - LIP_FRAC)).clamp(0.0, 1.0);
                        cut *= s * s * (3.0 - 2.0 * s);
                    }
                    let i2 = oy as usize * n2 + ox as usize;
                    // fade at the channel bed: the mouth opens into the
                    // bank toe, the bed itself is S4's word (and the
                    // restore clamp's invariant)
                    if cut < 0.02 {
                        continue; // target=datum alone is a stealth smoothing cut
                    }
                    let mut target = datum[i2] - cut;
                    // inside the restore-clamp feather the mouth may
                    // still notch through — but floor-capped so the
                    // channel-profile invariant (±CHANNEL_TOL_M of the
                    // smoothed base) survives; a hard gate here left
                    // every mouth hanging 13 m short of the drainage
                    let dc = bil8(&flow_distance.data, Vec2::new(wx, wy));
                    if dc < 14.0 {
                        target = target.max(base_lp64[i2] - crate::CHANNEL_TOL_M * 0.95);
                    }
                    if height.data[i2] > target {
                        height.data[i2] = target;
                    }
                }
            }
        }
    }
}
