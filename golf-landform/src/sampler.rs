//! Stage 4T: the seeded macro-landform sampler — draw a full [`MacroConfig`]
//! from the calibrated prior. Deterministic (DetRng domain channels
//! `lf4t/<channel>/v1`, fixed draw order, per-item channels so one item's
//! draws never shift another's), physically plausible (drainage-area
//! bookkeeping down the trunk, hydraulic widths from A, slope–area fall
//! gradients, junction angles/spacings from the observed network), and
//! bounded everywhere: every rejection loop has a cap and a drop-and-record
//! fallback — a seed NEVER fails to produce a valid config.
//!
//! Draw order (stable across versions of this module unless SAMPLER_VERSION
//! bumps): archetype → tilt → trunk (endpoints, A, meander, cross-section) →
//! junction walk → per-tributary (geometry, cross-section, depth-2) → ridges
//! → bluffs → bowls → noise seed.

use golf_core::det::DetRng;
use golf_core::math::{atan2, cos, exp, ln, sin, Vec2};
use golf_core::spline::Profile;
use serde::{Deserialize, Serialize};

use crate::prior::{Block, Deciles, LandformPrior};
use crate::{
    Bluff, Bowl, BowlBoundary, MacroConfig, MeanderSpec, Outlet, Path, Ridge,
    Tilt, Valley, MIN_FALL_GRADIENT, SCHEMA_VERSION,
};

/// Fold into golden identity together with the prior fingerprint.
pub const SAMPLER_VERSION: u32 = 1;

const EXTENT_M: f64 = 3000.0;
const BASE_ELEV_M: f64 = 100.0;
const PI: f64 = std::f64::consts::PI;

/// Authoring budgets — MacroConfig cost is per-primitive; these caps bound
/// the config size, not the statistics (counts inside the caps come from the
/// calibrated per-km² tables).
const MAX_TRIBS: usize = 6;
const MAX_DEPTH2: usize = 2;
const MAX_RIDGES: usize = 5;
const MAX_BLUFFS: usize = 3;
const MAX_BOWLS: usize = 4;
const BOWL_ATTEMPTS: usize = 20;

/// What the sampler did with one seed — provenance for gates, gallery labels
/// and the art/biome contract (`palette_id` keys the versioned palette
/// manifest; each archetype maps to the real ecology of its calibration
/// tiles).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SampleReport {
    pub seed: u64,
    pub sampler_version: u32,
    pub prior_fingerprint: u64,
    pub archetype: usize,
    pub archetype_name: String,
    pub palette_id: String,
    pub tau: f64,
    pub a_mouth_km2: f64,
    pub trunk_meanders: bool,
    pub n_tribs: usize,
    pub n_depth2: usize,
    pub n_ridges: usize,
    pub n_bluffs: usize,
    pub n_bowls: usize,
    /// Bounded-rejection fallbacks taken (drop-and-record — never a failure).
    pub drops: Vec<String>,
    /// Seed for the Stage-1 noise layer (independent channel).
    pub noise_seed: u64,
    /// The archetype's calibrated noise defaults (KEEP-7 interim).
    pub noise_defaults: std::collections::BTreeMap<String, f64>,
}

struct Ctx<'a> {
    prior: &'a Block,
    tau: f64,
    drops: Vec<String>,
}

impl Ctx<'_> {
    fn dec(&self, d: &Deciles, u: f64) -> f64 {
        d.sample(u, self.tau)
    }
}

/// Box–Muller standard normal (libm ln/cos — platform-stable).
fn std_normal(rng: &mut DetRng) -> f64 {
    let u1 = rng.next_f64().max(1e-12);
    let u2 = rng.next_f64();
    (-2.0 * ln(u1)).sqrt() * cos(2.0 * PI * u2)
}

fn rotate(v: Vec2, ang: f64) -> Vec2 {
    let (c, s) = (cos(ang), sin(ang));
    Vec2::new(c * v.x - s * v.y, s * v.x + c * v.y)
}

/// Ray from the window center: intersection with the window boundary.
fn boundary_point(dir: Vec2, inset: f64) -> Vec2 {
    let c = 0.5 * EXTENT_M;
    let half = c - inset;
    let tx = if dir.x.abs() > 1e-12 { half / dir.x.abs() } else { f64::INFINITY };
    let ty = if dir.y.abs() > 1e-12 { half / dir.y.abs() } else { f64::INFINITY };
    let t = tx.min(ty);
    Vec2::new(c + dir.x * t, c + dir.y * t)
}

fn clamp_to_window(p: Vec2, inset: f64) -> Vec2 {
    Vec2::new(
        p.x.clamp(inset, EXTENT_M - inset),
        p.y.clamp(inset, EXTENT_M - inset),
    )
}

/// Proper (interior) segment–segment intersection point, if any.
fn seg_cross(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> Option<Vec2> {
    let r = a1 - a0;
    let s = b1 - b0;
    let denom = r.x * s.y - r.y * s.x;
    if denom.abs() < 1e-12 {
        return None;
    }
    let qp = b0 - a0;
    let t = (qp.x * s.y - qp.y * s.x) / denom;
    let u = (qp.x * r.y - qp.y * r.x) / denom;
    if t > 1e-9 && t < 1.0 - 1e-9 && u > 1e-9 && u < 1.0 - 1e-9 {
        Some(a0 + r * t)
    } else {
        None
    }
}

/// Does `poly` cross `other` anywhere outside `ignore_near` (a junction the
/// two legitimately share)? Channel corridors must not cross — a tributary
/// crossing another channel above grade would be a hanging channel.
fn polyline_crosses(poly: &[Vec2], other: &[Vec2], ignore_near: Option<(Vec2, f64)>) -> bool {
    for w in poly.windows(2) {
        for v in other.windows(2) {
            if let Some(x) = seg_cross(w[0], w[1], v[0], v[1]) {
                if let Some((c, r)) = ignore_near {
                    if x.distance(c) < r {
                        continue;
                    }
                }
                return true;
            }
        }
    }
    false
}

/// One correlated cross-section draw (the copula ties incision, halfwidth,
/// wall gradient, asymmetry and floor rounding together per valley).
struct CrossSection {
    hw_u: f64,
    inc_u: f64,
    wall_mean: f64,
    wall_asym: f64,
    floor_round: f64,
}

fn draw_cross_section(ctx: &Ctx, rng: &mut DetRng) -> CrossSection {
    let g: Vec<f64> = (0..ctx.prior.copula.chol.len())
        .map(|_| std_normal(rng))
        .collect();
    let u = ctx.prior.copula.correlate(&g);
    // field order pinned by the fit: incision, floor_halfwidth, wall_mean,
    // wall_asym, floor_round
    let h = &ctx.prior.hydraulic;
    CrossSection {
        inc_u: u[0],
        hw_u: u[1],
        wall_mean: ctx.dec(&h.wall_grad_mean_deciles, u[2]).clamp(0.02, 0.9),
        wall_asym: ctx.dec(&h.wall_asym_logratio_deciles, u[3]).clamp(-2.0, 2.0),
        floor_round: ctx.dec(&h.floor_round_deciles, u[4]).clamp(1.0, 20.0),
    }
}

/// Split a mean wall gradient + log asymmetry into (left, right) with the
/// arithmetic mean preserved exactly and l/r = exp(asym).
fn split_walls(mean: f64, asym: f64) -> (f64, f64) {
    let ea = exp(asym);
    let l = 2.0 * mean * ea / (1.0 + ea);
    let r = 2.0 * mean / (1.0 + ea);
    (l.max(0.02), r.max(0.02))
}

struct TribPlan {
    u_on_trunk: f64,
    side: f64,
    a_km2: f64,
    angle_deg: f64,
}

/// Draw a full MacroConfig + report from the prior at `seed`.
/// `archetype`: fix the archetype (else drawn by calibrated weight).
/// `tau`: temperature on all decile draws, clamped to [0.5, 1.2].
pub fn sample_macro(
    prior: &LandformPrior,
    seed: u64,
    archetype: Option<usize>,
    tau: f64,
) -> (MacroConfig, SampleReport) {
    let tau = tau.clamp(0.5, 1.2);

    // 1. archetype ------------------------------------------------------
    let mut arch_rng = DetRng::new(seed, b"lf4t/archetype/v1");
    let arch_u = arch_rng.next_f64(); // drawn even when fixed (stable order)
    let k = prior.archetypes.len().max(1);
    let arch_idx = match archetype {
        Some(i) => i.min(k - 1),
        None => {
            let total: f64 = prior
                .archetypes
                .iter()
                .map(|a| a.weight.unwrap_or(0.0).max(0.0))
                .sum();
            if total <= 0.0 {
                0
            } else {
                let mut acc = 0.0;
                let target = arch_u * total;
                let mut pick = k - 1;
                for (i, a) in prior.archetypes.iter().enumerate() {
                    acc += a.weight.unwrap_or(0.0).max(0.0);
                    if target < acc {
                        pick = i;
                        break;
                    }
                }
                pick
            }
        }
    };
    let block = &prior.archetypes[arch_idx];
    let mut ctx = Ctx { prior: block, tau, drops: Vec::new() };

    // 2. tilt (authors the hydrology) -----------------------------------
    let mut tilt_rng = DetRng::new(seed, b"lf4t/tilt/v1");
    let grade = ctx
        .dec(&block.tilt.grade_deciles, tilt_rng.next_f64())
        .clamp(0.0, 0.06);
    let az = tilt_rng.range_f64(0.0, 2.0 * PI);
    let tilt = Tilt {
        grade_x: grade * cos(az),
        grade_y: grade * sin(az),
        curve_m: 0.0,
    };
    // downhill = direction of steepest descent
    let downhill = if grade > 1e-9 {
        Vec2::new(-cos(az), -sin(az))
    } else {
        Vec2::new(1.0, 0.0) // flat: arbitrary but fixed
    };
    let tilt_z = |p: Vec2| -> f64 {
        let c = 0.5 * EXTENT_M;
        BASE_ELEV_M + tilt.grade_x * (p.x - c) + tilt.grade_y * (p.y - c)
    };

    // 3. trunk ----------------------------------------------------------
    let mut trunk_rng = DetRng::new(seed, b"lf4t/trunk/v1");
    let h = &block.hydraulic;
    // clamp the upper tail: some calibration tiles sit on truly big rivers
    // (trunk logA q100 ≈ 4.7 → 50,000 km²); a 3 km golf window authors at
    // most a large-river corridor, not the Mississippi
    let a_mouth = exp(
        ctx.dec(&block.network.trunk_mouth_log_a_deciles, trunk_rng.next_f64())
            * std::f64::consts::LN_10,
    )
    .clamp(0.05, 200.0);
    let dh_az = atan2(downhill.y, downhill.x);
    let exit_dir = rotate(Vec2::new(1.0, 0.0), dh_az + trunk_rng.range_f64(-25.0, 25.0) * PI / 180.0);
    let entry_dir = rotate(Vec2::new(1.0, 0.0), dh_az + PI + trunk_rng.range_f64(-25.0, 25.0) * PI / 180.0);
    let exit_p = boundary_point(exit_dir, 0.0);
    let entry_p = boundary_point(entry_dir, 0.0);

    // Meander width basis: the calibrated lambda/W multiplier was normalized
    // by the VALLEY top width, and the curvature clamp must scale with the
    // corridor actually carved — floor the regression's channel width at
    // 2.5x the median floor halfwidth so wide valleys can't wiggle at
    // sub-floor wavelengths (that renders as a beaded corridor).
    let w_trunk = h
        .top_width
        .eval(0.7 * a_mouth, 0.5, 1.0)
        .max(2.5 * h.floor_halfwidth.median(0.7 * a_mouth))
        .clamp(8.0, 340.0);
    let meander_u = trunk_rng.next_f64();
    let lam_mult = ctx.dec(&block.meander.wavelength_mult_deciles, trunk_rng.next_f64());
    let intensity = ctx
        .dec(&block.meander.intensity_deciles, trunk_rng.next_f64())
        .clamp(0.0, 1.0);
    let meander_seed = trunk_rng.next_u64();
    let sinuosity = ctx
        .dec(&block.meander.sinuosity_deciles, trunk_rng.next_f64())
        .max(1.0);
    let trunk_meanders = meander_u < block.meander.fit_frac.clamp(0.0, 1.0);
    let trunk_path = if trunk_meanders {
        Path::Meander(MeanderSpec {
            entry: entry_p,
            exit: exit_p,
            width_m: w_trunk,
            intensity,
            // calibrated lambda/W; MeanderSpec floors the multiplier at 4
            wavelength_mult: lam_mult.clamp(4.0, 30.0),
            jitter: 0.25, // not separately calibrated in v0.1
            seed: meander_seed,
        })
    } else {
        // non-meandering reaches still wander (calibrated sinuosity q10+ is
        // above 1.0) — a ruler-straight trunk reads as man-made
        Path::Meander(MeanderSpec {
            entry: entry_p,
            exit: exit_p,
            width_m: w_trunk,
            intensity: ((sinuosity - 1.0) / 0.5).clamp(0.03, 0.4),
            wavelength_mult: lam_mult.clamp(8.0, 20.0),
            jitter: 0.3,
            seed: meander_seed,
        })
    };
    let trunk_spine = trunk_path.to_spine();
    let trunk_len = trunk_spine.length();

    // 4. junction walk (drainage-area bookkeeping) ----------------------
    let mut net_rng = DetRng::new(seed, b"lf4t/network/v1");
    let p_left = net_rng.next_f64(); // Beta(1,1) side-balance draw
    let mut plans: Vec<TribPlan> = Vec::new();
    let mut arc = 0.1 * trunk_len;
    while plans.len() < MAX_TRIBS {
        let spacing = ctx
            .dec(&block.network.trunk_junction_spacing_m_deciles, net_rng.next_f64())
            .clamp(200.0, 2500.0);
        arc += spacing;
        if arc > 0.9 * trunk_len {
            break;
        }
        let ratio = exp(
            ctx.dec(&block.network.junction_area_logratio_deciles, net_rng.next_f64())
                * std::f64::consts::LN_10,
        )
        .clamp(0.005, 0.6);
        let side = if net_rng.next_f64() < p_left { 1.0 } else { -1.0 };
        let angle = ctx
            .dec(&block.network.junction_angle_deg_deciles, net_rng.next_f64())
            .clamp(15.0, 85.0);
        plans.push(TribPlan {
            u_on_trunk: arc / trunk_len,
            side,
            a_km2: ratio, // provisional: ratio of local trunk A (set below)
            angle_deg: angle,
        });
    }
    // Resolve absolute tributary areas so the mouth area is EXACTLY a_mouth:
    // lateral (unchanneled) inflow takes 10%, the entry reach carries the
    // rest minus the tributary increments (floored at 15% — a through-
    // flowing trunk enters the window already carrying drainage).
    let lateral = 0.1 * a_mouth;
    let mut trib_a: Vec<f64> = Vec::with_capacity(plans.len());
    {
        // first pass at nominal ratios of the running upstream area
        let mut sum = 0.0;
        for p in &plans {
            let local = (0.25 + 0.75 * p.u_on_trunk) * a_mouth;
            let a = (p.a_km2 * local).max(0.02);
            trib_a.push(a);
            sum += a;
        }
        let budget = 0.75 * a_mouth - lateral;
        if sum > budget && sum > 0.0 {
            let scale = (budget / sum).max(0.05);
            for a in trib_a.iter_mut() {
                *a *= scale;
            }
        }
    }
    let a_entry = (a_mouth - lateral - trib_a.iter().sum::<f64>()).max(0.15 * a_mouth);
    // piecewise upstream->downstream area profile of the trunk
    let area_at = |u: f64| -> f64 {
        let mut a = a_entry + lateral * u.clamp(0.0, 1.0);
        for (p, ta) in plans.iter().zip(&trib_a) {
            if u >= p.u_on_trunk {
                a += ta;
            }
        }
        a
    };

    // trunk cross-section (one correlated residual vector per valley)
    let mut xsec0 = DetRng::new(seed, b"lf4t/xsec/0/v1");
    let cs = draw_cross_section(&ctx, &mut xsec0);
    let fall_u = xsec0.next_f64();
    let (wl, wr) = split_walls(cs.wall_mean, cs.wall_asym);
    // residual-decile tails reach e^±2.8 (≈16x — blown fits and braided
    // reaches in the calibration); clamp draws to 3x either side of the
    // regression median, which preserves the calibrated IQR (resid q10/q90
    // ≈ ±1.1 → ~3x) while excluding the monsters
    let hw_at = |a: f64, u: f64| -> f64 {
        let med = h.floor_halfwidth.median(a);
        h.floor_halfwidth
            .eval(a, u, 1.0)
            .clamp((med / 3.0).max(2.0), (med * 3.0).min(260.0))
    };
    let inc_med = h.incision.median(area_at(0.0));
    let inc_entry = h
        .incision
        .eval(area_at(0.0), cs.inc_u, 1.0)
        .clamp((inc_med / 3.0).max(1.5), (inc_med * 3.0).min(60.0));
    let trunk_fall = block
        .hydraulic
        .fall_gradient
        .eval(area_at(0.5), fall_u, 1.0)
        .clamp(MIN_FALL_GRADIENT, 0.05);
    let trunk_hw = Profile::new(vec![
        (0.0, hw_at(area_at(0.0), cs.hw_u)),
        (0.5, hw_at(area_at(0.5), cs.hw_u)),
        (1.0, hw_at(area_at(1.0), cs.hw_u)),
    ]);
    let trunk_hw_max = trunk_hw.knots.iter().map(|k| k.1).fold(0.0, f64::max);
    let trunk = Valley {
        path: trunk_path,
        floor_z0_m: tilt_z(entry_p) - inc_entry,
        fall_gradient: trunk_fall,
        floor_halfwidth: trunk_hw,
        wall_grad_left: wl,
        wall_grad_right: wr,
        floor_round_m: cs.floor_round,
        shoulder_k_m: 4.0,
        join_trunk: None,
    };
    let trunk_corridor = trunk_hw_max + inc_entry / wl.min(wr) + cs.floor_round;

    // 5. tributaries ----------------------------------------------------
    let mut valleys: Vec<Valley> = vec![trunk];
    let mut trib_geoms: Vec<(Vec<Vec2>, f64, usize)> = Vec::new(); // (ctrl pts head->junction, side, valley idx)
    // every authored channel polyline (trunk spine + accepted trib/depth-2
    // controls) — the no-crossing and clearance oracle
    let mut authored: Vec<Vec<Vec2>> = vec![trunk_spine.pts.clone()];
    let mut n_depth2 = 0usize;
    for (i, plan) in plans.iter().enumerate() {
        let mut rng = DetRng::new(seed, format!("lf4t/trib/{i}/v1").as_bytes());
        let a_trib = trib_a[i];
        let junction = trunk_spine.point_at(plan.u_on_trunk);
        // trunk downstream tangent at the junction (finite difference)
        let t0 = trunk_spine.point_at((plan.u_on_trunk - 0.01).max(0.0));
        let t1 = trunk_spine.point_at((plan.u_on_trunk + 0.01).min(1.0));
        let tangent = (t1 - t0).normalized();
        // tributary approaches the junction making the drawn angle with the
        // downstream tangent, from the drawn side; upstream direction is the
        // reverse of the approach
        let approach = rotate(tangent, -plan.side * plan.angle_deg * PI / 180.0);
        let up = Vec2::new(-approach.x, -approach.y);
        let len = ctx
            .dec(&block.network.branch_len_m_deciles, rng.next_f64())
            .clamp(400.0, 2600.0);
        // head, clipped to the window with a 60 m inset
        let head_raw = junction + up * len;
        let head = clamp_to_window(head_raw, 60.0);
        let len_eff = head.distance(junction);
        if len_eff < 300.0 {
            ctx.drops.push(format!("trib{i}: clipped below 300 m — dropped"));
            continue;
        }
        // gentle bow: 2 interior control points with small perpendicular offsets
        let perp = up.perp();
        let bow = 0.08 * len_eff;
        let o1 = bow * (2.0 * rng.next_f64() - 1.0);
        let o2 = bow * (2.0 * rng.next_f64() - 1.0);
        let p1 = clamp_to_window(head.lerp(junction, 1.0 / 3.0) + perp * o1, 40.0);
        let p2 = clamp_to_window(head.lerp(junction, 2.0 / 3.0) + perp * o2, 40.0);
        // no-crossing rule: a channel may TOUCH another only at its own
        // junction — anywhere else would author a hanging channel
        let crosses = |c: &[Vec2]| -> bool {
            authored.iter().enumerate().any(|(ai, poly)| {
                let ignore = if ai == 0 { Some((junction, 150.0)) } else { None };
                polyline_crosses(c, poly, ignore)
            })
        };
        let mut ctrl = vec![head, p1, p2, junction];
        if crosses(&ctrl) {
            // retry without the bow, then give up on this junction
            let straight = vec![head, head.lerp(junction, 0.5), junction];
            if crosses(&straight) {
                ctx.drops.push(format!("trib{i}: crosses an authored channel — dropped"));
                continue;
            }
            ctx.drops.push(format!("trib{i}: bow crossed — straightened"));
            ctrl = straight;
        }

        let mut xr = DetRng::new(seed, format!("lf4t/xsec/{}/v1", valleys.len()).as_bytes());
        let tcs = draw_cross_section(&ctx, &mut xr);
        let tfall_u = xr.next_f64();
        let (twl, twr) = split_walls(tcs.wall_mean, tcs.wall_asym);
        let thw = hw_at(a_trib, tcs.hw_u);
        let tfall = block
            .hydraulic
            .fall_gradient
            .eval(a_trib, tfall_u, 1.0)
            .clamp(MIN_FALL_GRADIENT, 0.05);
        let idx = valleys.len();
        valleys.push(Valley {
            path: Path::Points(ctrl.clone()),
            floor_z0_m: 0.0, // overwritten by the accordant junction snap
            fall_gradient: tfall,
            floor_halfwidth: Profile::new(vec![(0.0, 0.75 * thw), (1.0, thw)]),
            wall_grad_left: twl,
            wall_grad_right: twr,
            floor_round_m: tcs.floor_round,
            shoulder_k_m: 4.0,
            join_trunk: Some(0),
        });
        trib_geoms.push((ctrl.clone(), plan.side, idx));
        authored.push(ctrl.clone());

        // depth-2: one sub-tributary with the calibrated probability
        if n_depth2 < MAX_DEPTH2
            && a_trib > 0.2
            && rng.next_f64() < block.network.depth2_frac.clamp(0.0, 1.0)
        {
            let r2 = exp(
                ctx.dec(&block.network.junction_area_logratio_deciles, rng.next_f64())
                    * std::f64::consts::LN_10,
            )
            .clamp(0.02, 0.5);
            let a2 = (r2 * a_trib).max(0.01);
            let ang2 = ctx
                .dec(&block.network.junction_angle_deg_deciles, rng.next_f64())
                .clamp(20.0, 80.0);
            let side2 = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
            // junction ~45% along the parent's authored polyline (ctrl may
            // be the bowed 4-pt or the straightened 3-pt variant)
            let j2 = if ctrl.len() == 4 {
                ctrl[1].lerp(ctrl[2], 0.35)
            } else {
                ctrl[0].lerp(ctrl[1], 0.9)
            };
            let pdir = (junction - head).normalized();
            let ap2 = rotate(pdir, -side2 * ang2 * PI / 180.0);
            let up2 = Vec2::new(-ap2.x, -ap2.y);
            let l2 = ctx
                .dec(&block.network.branch_len_m_deciles, rng.next_f64())
                .clamp(300.0, 1200.0);
            let h2 = clamp_to_window(j2 + up2 * l2, 60.0);
            let sub_ctrl = vec![h2, h2.lerp(j2, 0.5), j2];
            let sub_crosses = authored.iter().enumerate().any(|(ai, poly)| {
                let ignore = if ai + 1 == authored.len() {
                    Some((j2, 120.0)) // its own parent — junction touch is fine
                } else {
                    None
                };
                polyline_crosses(&sub_ctrl, poly, ignore)
            });
            if sub_crosses {
                ctx.drops.push(format!("trib{i}: depth-2 crosses a channel — dropped"));
            } else if h2.distance(j2) >= 250.0 {
                let mut x2 = DetRng::new(seed, format!("lf4t/xsec/{}/v1", valleys.len()).as_bytes());
                let cs2 = draw_cross_section(&ctx, &mut x2);
                let f2u = x2.next_f64();
                let (l2w, r2w) = split_walls(cs2.wall_mean, cs2.wall_asym);
                let hw2 = hw_at(a2, cs2.hw_u);
                authored.push(sub_ctrl.clone());
                valleys.push(Valley {
                    path: Path::Points(sub_ctrl),
                    floor_z0_m: 0.0,
                    fall_gradient: block
                        .hydraulic
                        .fall_gradient
                        .eval(a2, f2u, 1.0)
                        .clamp(MIN_FALL_GRADIENT, 0.05),
                    floor_halfwidth: Profile::new(vec![(0.0, 0.7 * hw2), (1.0, hw2)]),
                    wall_grad_left: l2w,
                    wall_grad_right: r2w,
                    floor_round_m: cs2.floor_round,
                    shoulder_k_m: 4.0,
                    join_trunk: Some(idx),
                });
                n_depth2 += 1;
            } else {
                ctx.drops.push(format!("trib{i}: depth-2 clipped — dropped"));
            }
        }
    }
    let n_tribs = trib_geoms.len();

    // distance from a point to any authored channel polyline (clearance)
    let valley_ctrls = authored;
    let dist_to_valleys = |p: Vec2| -> f64 {
        let mut best = f64::INFINITY;
        for poly in &valley_ctrls {
            for w in poly.windows(2) {
                let (a, b) = (w[0], w[1]);
                let ab = b - a;
                let len2 = ab.dot(ab);
                let t = if len2 == 0.0 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
                let d = p.distance(a + ab * t);
                if d < best {
                    best = d;
                }
            }
        }
        best
    };

    // 6. ridges (network-dual interfluves, then flank ridges) ------------
    let mut ridges: Vec<Ridge> = Vec::new();
    let mut ridge_ctrls: Vec<Vec<Vec2>> = Vec::new();
    {
        let mut cnt_rng = DetRng::new(seed, b"lf4t/ridges/v1");
        let target = (ctx.dec(&block.ridges.per_km2_deciles, cnt_rng.next_f64()) * 9.0)
            .round()
            .clamp(0.0, MAX_RIDGES as f64) as usize;
        // same-side adjacent tributary pairs -> interfluve midlines
        let mut by_side: Vec<(f64, usize)> = trib_geoms
            .iter()
            .enumerate()
            .map(|(gi, (_, side, _))| (*side, gi))
            .collect();
        by_side.sort_by(|a, b| {
            a.0.partial_cmp(&b.0)
                .unwrap()
                .then(plans[a.1].u_on_trunk.partial_cmp(&plans[b.1].u_on_trunk).unwrap())
        });
        let mut candidates: Vec<Vec<Vec2>> = Vec::new();
        for w in by_side.windows(2) {
            if w[0].0 != w[1].0 {
                continue; // different sides
            }
            let (ca, _, _) = &trib_geoms[w[0].1];
            let (cb, _, _) = &trib_geoms[w[1].1];
            // midline between matched fractions, head -> 60% toward junction
            let mid = |t: f64| -> Vec2 {
                let pa = ca[0].lerp(*ca.last().unwrap(), t);
                let pb = cb[0].lerp(*cb.last().unwrap(), t);
                Vec2::new(0.5 * (pa.x + pb.x), 0.5 * (pa.y + pb.y))
            };
            let pts = vec![mid(0.0), mid(0.3), mid(0.6)];
            if pts[0].distance(pts[2]) >= 250.0 {
                candidates.push(pts);
            }
        }
        // flank ridge candidates: parallel the trunk chord on each side,
        // bowed so the crest wanders like a real interfluve
        let chord_dir = (exit_p - entry_p).normalized();
        let cperp = chord_dir.perp();
        for side in [1.0f64, -1.0] {
            let off = ctx
                .dec(&block.ridges.spacing_m_deciles, cnt_rng.next_f64())
                .clamp(180.0, 900.0);
            let d = trunk_corridor + off;
            let bow1 = 90.0 * (2.0 * cnt_rng.next_f64() - 1.0);
            let bow2 = 90.0 * (2.0 * cnt_rng.next_f64() - 1.0);
            let a = clamp_to_window(entry_p.lerp(exit_p, 0.12) + cperp * (side * d), 50.0);
            let b = clamp_to_window(entry_p.lerp(exit_p, 0.35) + cperp * (side * d + bow1), 50.0);
            let c = clamp_to_window(entry_p.lerp(exit_p, 0.65) + cperp * (side * d + bow2), 50.0);
            let e = clamp_to_window(entry_p.lerp(exit_p, 0.88) + cperp * (side * d), 50.0);
            if a.distance(e) >= 400.0 {
                candidates.push(vec![a, b, c, e]);
            }
        }
        for (ci, pts) in candidates.into_iter().enumerate() {
            if ridges.len() >= target {
                break;
            }
            let mut rng = DetRng::new(seed, format!("lf4t/ridge/{ci}/v1").as_bytes());
            let prom = ctx
                .dec(&block.ridges.prominence_deciles, rng.next_f64())
                .clamp(2.0, 40.0);
            // cap below the measured q90: an 80 m crest with gentle flanks
            // and single-digit prominence composes as a pancake berm on the
            // bare skeleton (broad real interfluves need noise texture to
            // read as terrain — Stage-1 composition, not this primitive)
            let chw = ctx
                .dec(&block.ridges.crest_halfwidth_deciles, rng.next_f64())
                .clamp(6.0, 45.0);
            let fg = ctx
                .dec(&block.ridges.flank_grad_deciles, rng.next_f64())
                .clamp(0.02, 0.5);
            let asym = 1.0 + 0.4 * (rng.next_f64() - 0.5);
            // v1 forbids ridge-valley crossings: control points must clear
            // the corridors AND no segment may cross a channel polyline
            // (the composed smin would carve through anyway — this is an
            // authoring-clarity rule, not a safety one)
            let clearance = trunk_corridor.min(220.0) + chw + 20.0;
            if pts.iter().any(|p| dist_to_valleys(*p) < clearance)
                || valley_ctrls.iter().any(|poly| polyline_crosses(&pts, poly, None))
            {
                ctx.drops.push(format!("ridge{ci}: violates valley clearance — dropped"));
                continue;
            }
            let start = pts[0];
            let end = *pts.last().unwrap();
            let ln_arc = start.distance(end).max(1.0);
            ridge_ctrls.push(pts.clone());
            ridges.push(Ridge {
                path: Path::Points(pts),
                crest_z0_m: tilt_z(start) + prom,
                // crest parallels the tilt plane (constant prominence)
                fall_gradient: (tilt_z(start) - tilt_z(end)) / ln_arc,
                crest_halfwidth: Profile::constant(chw),
                flank_grad_left: fg * asym,
                flank_grad_right: fg / asym,
                crest_round_m: 6.0,
                base_k_m: 4.0,
                // end tapers + drawn saddle variation: a uniform crest over
                // kilometers reads as an engineered berm
                emphasis: Profile::new(vec![
                    (0.0, 0.25),
                    (0.2, 0.7 + 0.3 * rng.next_f64()),
                    (0.45, 0.6 + 0.4 * rng.next_f64()),
                    (0.7, 0.7 + 0.3 * rng.next_f64()),
                    (1.0, 0.25),
                ]),
            });
        }
        if ridges.len() < target {
            ctx.drops
                .push(format!("ridges: {} of {target} placed (candidate-limited)", ridges.len()));
        }
    }

    // 7. bluffs (outer banks of trunk bends) -----------------------------
    let mut bluffs: Vec<Bluff> = Vec::new();
    {
        let mut cnt_rng = DetRng::new(seed, b"lf4t/bluffs/v1");
        let target = (ctx.dec(&block.bluffs.per_km2_deciles, cnt_rng.next_f64()) * 9.0)
            .round()
            .clamp(0.0, MAX_BLUFFS as f64) as usize;
        if target > 0 && trunk_meanders {
            // find bend apexes: sample the spine, local extrema of side-sway
            let n = 60usize;
            let samples: Vec<Vec2> = (0..=n)
                .map(|i| trunk_spine.point_at(i as f64 / n as f64))
                .collect();
            let mut apexes: Vec<(usize, f64)> = Vec::new(); // (sample idx, turn sign)
            for i in 2..n - 1 {
                let a = samples[i - 2];
                let b = samples[i];
                let c = samples[i + 2];
                let ab = (b - a).normalized();
                let bc = (c - b).normalized();
                let cross = ab.x * bc.y - ab.y * bc.x;
                if cross.abs() > 0.12 {
                    apexes.push((i, cross.signum()));
                }
            }
            // keep well-separated apexes, alternate through them
            apexes.dedup_by(|a, b| a.0 - b.0 < 8 && a.1 == b.1);
            for (bi, (idx, turn)) in apexes.into_iter().take(target).enumerate() {
                let mut rng = DetRng::new(seed, format!("lf4t/bluff/{bi}/v1").as_bytes());
                // a bluff raises a whole half-plane terrace; the measured
                // q50 (22 m) stacks into architectural slabs inside 3 km —
                // cap the authored step and let noise roughen the face
                let height = ctx
                    .dec(&block.bluffs.height_deciles, rng.next_f64())
                    .clamp(3.0, 20.0);
                let fg = ctx
                    .dec(&block.bluffs.face_grad_deciles, rng.next_f64())
                    .clamp(0.08, 1.2);
                let u0 = (idx as f64 / n as f64 - 0.07).max(0.0);
                let u1 = (idx as f64 / n as f64 + 0.07).min(1.0);
                // outer bank = the side the curve turns AWAY from
                let outer = -turn;
                let offset = trunk_hw_max + cs.floor_round + height / fg + 15.0;
                let pa = trunk_spine.point_at(u0);
                let pm = trunk_spine.point_at(0.5 * (u0 + u1));
                let pb = trunk_spine.point_at(u1);
                let tang = (pb - pa).normalized();
                let out_dir = tang.perp() * outer;
                let path = vec![
                    clamp_to_window(pa + out_dir * offset, 20.0),
                    clamp_to_window(pm + out_dir * offset, 20.0),
                    clamp_to_window(pb + out_dir * offset, 20.0),
                ];
                if path[0].distance(path[2]) < 150.0 {
                    ctx.drops.push(format!("bluff{bi}: too short after clipping — dropped"));
                    continue;
                }
                bluffs.push(Bluff {
                    path: Path::Points(path),
                    height_m: height,
                    height: Profile::constant(1.0),
                    face_grad: fg,
                    taper_frac: 0.18,
                    // raised side = AWAY from the river (the terrace above
                    // the cut bank); the spine runs a->b with the river on
                    // the `-outer` side
                    raise_left: outer > 0.0,
                });
            }
        }
        if bluffs.len() < target {
            ctx.drops.push(format!(
                "bluffs: {} of {target} placed ({})",
                bluffs.len(),
                if trunk_meanders { "few qualifying bends" } else { "straight trunk" }
            ));
        }
    }

    // 8. bowls -----------------------------------------------------------
    let mut bowls: Vec<Bowl> = Vec::new();
    {
        let mut cnt_rng = DetRng::new(seed, b"lf4t/bowls/v1");
        let target = (ctx.dec(&block.bowls.per_km2_deciles, cnt_rng.next_f64()) * 9.0)
            .round()
            .clamp(0.0, MAX_BOWLS as f64) as usize;
        for bi in 0..target {
            let mut rng = DetRng::new(seed, format!("lf4t/bowl/{bi}/v1").as_bytes());
            let radius = ctx
                .dec(&block.bowls.radius_deciles, rng.next_f64())
                .clamp(25.0, 320.0);
            let depth = ctx
                .dec(&block.bowls.depth_deciles, rng.next_f64())
                .clamp(1.5, 30.0);
            // wobble/cycles jointly clamped below the measured tails: the
            // extractor's spill-contour wobble at high angular frequency
            // renders as flower petals, not a natural depression
            let wobble = ctx
                .dec(&block.bowls.wobble_deciles, rng.next_f64())
                .clamp(0.0, 0.45);
            let cycles = ctx
                .dec(&block.bowls.cycles_deciles, rng.next_f64())
                .clamp(1.5, 4.0);
            let inner = ctx
                .dec(&block.bowls.inner_grad_deciles, rng.next_f64())
                .clamp(0.04, 0.6)
                .max(1.3 * depth / radius); // guarantee a flat floor exists
            let lake_u = rng.next_f64();
            let blob_seed = rng.next_u32();
            let mut placed = false;
            for _ in 0..BOWL_ATTEMPTS {
                let inset = radius * (1.0 + wobble) + 80.0;
                let cx = rng.range_f64(inset, EXTENT_M - inset);
                let cy = rng.range_f64(inset, EXTENT_M - inset);
                let c = Vec2::new(cx, cy);
                let clear_v = radius + trunk_corridor.min(220.0) + 50.0;
                if dist_to_valleys(c) < clear_v {
                    continue;
                }
                let mut near_ridge = false;
                for rc in &ridge_ctrls {
                    for p in rc {
                        if p.distance(c) < radius + 60.0 {
                            near_ridge = true;
                            break;
                        }
                    }
                }
                if near_ridge {
                    continue;
                }
                let outlet = if lake_u < block.bowls.lake_frac.clamp(0.0, 1.0) {
                    Outlet::Lake
                } else {
                    // spillway notch faces downhill (blob arc starts at +x,
                    // runs counter-clockwise; arc fraction ~ angle fraction)
                    let mut at = atan2(downhill.y, downhill.x) / (2.0 * PI);
                    if at < 0.0 {
                        at += 1.0;
                    }
                    Outlet::Spillway {
                        at_s: at,
                        halfwidth_m: (0.35 * radius).clamp(15.0, 60.0),
                        depth_m: (0.35 * depth).clamp(1.0, 6.0),
                    }
                };
                bowls.push(Bowl {
                    boundary: BowlBoundary::Blob {
                        center: c,
                        radius_m: radius,
                        wobble,
                        cycles,
                        seed: blob_seed,
                    },
                    rim_z_m: tilt_z(c),
                    depth_m: depth,
                    inner_grad: inner,
                    rim_round_m: 8.0,
                    floor_k_m: 3.0,
                    outer_grad: 0.35,
                    blend_k_m: 2.5,
                    outlet,
                });
                placed = true;
                break;
            }
            if !placed {
                ctx.drops.push(format!("bowl{bi}: no clear site in {BOWL_ATTEMPTS} attempts — dropped"));
            }
        }
    }

    // 9-10. assemble + noise channel -------------------------------------
    let mut noise_rng = DetRng::new(seed, b"lf4t/noise/v1");
    let noise_seed = noise_rng.next_u64();
    let cfg = MacroConfig {
        schema_version: SCHEMA_VERSION,
        extent_m: EXTENT_M,
        base_elev_m: BASE_ELEV_M,
        tilt,
        ridges,
        bluffs,
        bowls,
        valleys,
    };
    let report = SampleReport {
        seed,
        sampler_version: SAMPLER_VERSION,
        prior_fingerprint: prior.fingerprint(),
        archetype: arch_idx,
        archetype_name: prior.archetype_names[arch_idx].clone(),
        palette_id: format!("palette/{}", prior.archetype_names[arch_idx]),
        tau,
        a_mouth_km2: a_mouth,
        trunk_meanders,
        n_tribs,
        n_depth2,
        n_ridges: cfg.ridges.len(),
        n_bluffs: cfg.bluffs.len(),
        n_bowls: cfg.bowls.len(),
        drops: ctx.drops,
        noise_seed,
        noise_defaults: block.noise_defaults.clone(),
    };
    (cfg, report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resolve;

    #[test]
    fn deterministic_same_seed_same_config() {
        let p = LandformPrior::builtin();
        let (a, ra) = sample_macro(&p, 7, None, 1.0);
        let (b, rb) = sample_macro(&p, 7, None, 1.0);
        assert_eq!(a, b);
        assert_eq!(ra.archetype, rb.archetype);
        assert_eq!(ra.noise_seed, rb.noise_seed);
    }

    #[test]
    fn seeds_vary() {
        let p = LandformPrior::builtin();
        let (a, _) = sample_macro(&p, 1, None, 1.0);
        let (b, _) = sample_macro(&p, 2, None, 1.0);
        assert_ne!(a, b);
    }

    #[test]
    fn archetype_override_is_respected() {
        let p = LandformPrior::builtin();
        for j in 0..p.archetypes.len() {
            let (_, r) = sample_macro(&p, 11, Some(j), 1.0);
            assert_eq!(r.archetype, j);
            assert_eq!(r.palette_id, format!("palette/{}", p.archetype_names[j]));
        }
    }

    /// The 500-seed fuzz gate: every seed resolves (no panic), joins are
    /// valid by construction, floors are strictly monotone over the authored
    /// arc, and everything stays inside a sane world envelope.
    #[test]
    fn fuzz_500_seeds_resolve_with_valid_invariants() {
        let p = LandformPrior::builtin();
        for seed in 0..500u64 {
            let tau = 0.5 + 0.7 * ((seed % 8) as f64 / 7.0);
            let (cfg, rep) = sample_macro(&p, seed, None, tau);
            // join ordering is a hard construction invariant
            for (i, v) in cfg.valleys.iter().enumerate() {
                if let Some(t) = v.join_trunk {
                    assert!(t < i, "seed {seed}: join_trunk {t} !< {i}");
                }
                assert!(v.fall_gradient >= MIN_FALL_GRADIENT);
            }
            assert!(cfg.valleys.len() <= 1 + MAX_TRIBS + MAX_DEPTH2);
            assert!(cfg.ridges.len() <= MAX_RIDGES);
            assert!(cfg.bluffs.len() <= MAX_BLUFFS);
            assert!(cfg.bowls.len() <= MAX_BOWLS);
            let r = resolve(&cfg); // must not panic
            // authored floors strictly descend to the junction/cap
            for vi in 0..r.n_valleys() {
                let (_, z0) = r.valley_floor(vi, 0.0);
                let (_, z1) = r.valley_floor(vi, 0.35);
                assert!(z1 < z0 + 1e-9, "seed {seed} valley {vi}: floor not descending");
            }
            let _ = rep.prior_fingerprint;
        }
    }

    #[test]
    fn temperature_narrows_spread() {
        let p = LandformPrior::builtin();
        let spread = |tau: f64| -> f64 {
            let mut mn = f64::INFINITY;
            let mut mx = f64::NEG_INFINITY;
            for seed in 0..40u64 {
                let (_, r) = sample_macro(&p, seed, Some(0), tau);
                mn = mn.min(r.a_mouth_km2);
                mx = mx.max(r.a_mouth_km2);
            }
            mx - mn
        };
        assert!(spread(0.5) < spread(1.2));
    }
}
