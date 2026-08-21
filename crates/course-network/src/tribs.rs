//! Phase N2 — tributaries: source-to-sink descent of the proto field.
//!
//! Sources sit on the HIGH ground (territory peaks — local maxima of
//! distance-to-network); each walks DOWNHILL on the proto field with
//! tier-scaled meander until it reaches a channel, blending into the
//! channel's downstream tangent so the junction lands at an acute angle.
//! Convergence is structural: channels are the field's valley floors, so a
//! descent ends on one because there is nowhere else downhill to go.

use crate::proto::{ChannelSet, Proto};
use course_seed::DetRng;
use course_world::math::{self, Vec2};
use course_world::world::EXTENT_M;

/// One tributary, JUNCTION-FIRST (same convention as a trunk's mouth-first).
#[derive(Clone, Debug)]
pub struct Trib {
    pub pts: Vec<Vec2>,
    pub z: Vec<f64>,
    /// Channel-point index (in the ChannelSet at build time) it joins at.
    pub attach_pt: u32,
    /// Polyline id it joins.
    pub parent_poly: u32,
    pub tier: u8,
}

pub struct TierParams {
    /// Meander wavelength band, metres.
    pub lam_m: (f64, f64),
    /// Heading swing, radians.
    pub swing_rad: f64,
    /// Minimum distance-to-network for a source.
    pub source_min_d_m: f64,
    /// Minimum spacing between accepted sources.
    pub source_sep_m: f64,
    /// A walker joins when this close to a channel.
    pub join_r_m: f64,
    /// Junction-angle target distribution: centre and the probability of an
    /// orthogonal-tail draw. Corpus: p50 37-45 deg, >80 deg share 8.5-13.4%.
    pub junc_centre_deg: f64,
    pub junc_tail_p: f64,
    /// Long-profile concavity for the tributary.
    pub concavity: f64,
    /// Step length, metres.
    pub step_m: f64,
}

pub fn tier2() -> TierParams {
    TierParams {
        lam_m: (300.0, 700.0),
        swing_rad: 0.40,
        source_min_d_m: 380.0,
        source_sep_m: 560.0,
        join_r_m: 70.0,
        // Drawn ~9 deg under the corpus centre: the bezier's curvature reads
        // slightly wide of the entry tangent at the 50 m baseline (drawn 41
        // measured 50.7). Tail up-weighted for the same reason in reverse.
        junc_centre_deg: 32.0,
        junc_tail_p: 0.13,
        concavity: 1.35,
        step_m: 26.0,
    }
}

/// Territory peaks: candidates on a coarse lattice, ranked by
/// distance-to-network, greedily accepted at `source_sep_m`.
pub fn sources(rng: &mut DetRng, chans: &ChannelSet, p: &TierParams, max_n: usize) -> Vec<Vec2> {
    let step = 90.0;
    let n = (EXTENT_M / step) as i64;
    let mut cand: Vec<(f64, Vec2)> = Vec::new();
    for gy in 1..n {
        for gx in 1..n {
            let q = Vec2::new(
                gx as f64 * step + rng.range_f64(-30.0, 30.0),
                gy as f64 * step + rng.range_f64(-30.0, 30.0),
            );
            if q.x < 120.0 || q.y < 120.0 || q.x > EXTENT_M - 120.0 || q.y > EXTENT_M - 120.0 {
                continue;
            }
            if let Some((_, d)) = chans.nearest(q) {
                if d >= p.source_min_d_m {
                    cand.push((d, q));
                }
            }
        }
    }
    cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(core::cmp::Ordering::Equal));
    let mut out: Vec<Vec2> = Vec::new();
    for (_, q) in cand {
        if out.len() >= max_n {
            break;
        }
        if out.iter().all(|o| o.distance(q) >= p.source_sep_m) {
            out.push(q);
        }
    }
    out
}

/// Why a walker died — the instrument, so a fix is measured and not argued.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Death {
    OffTile,
    Exhausted,
    Stub,
}

/// Walk one source downhill. Returns the trib or the reason it died —
/// the caller counts deaths, never hides them.
pub fn descend(
    rng: &mut DetRng,
    src: Vec2,
    proto: &Proto,
    p: &TierParams,
) -> Result<Trib, Death> {
    let chans = proto.chans;
    let lam = rng.range_f64(p.lam_m.0, p.lam_m.1);
    let phase = rng.range_f64(0.0, core::f64::consts::TAU);
    // junction target for THIS trib
    let theta = if rng.next_f64() < p.junc_tail_p {
        rng.range_f64(80.0, 95.0).to_radians()
    } else {
        rng.range_f64(p.junc_centre_deg - 8.0, p.junc_centre_deg + 9.0).to_radians()
    };

    let mut pts = vec![src];
    let g0 = proto.grad(src);
    let mut heading = if g0.length() < 1e-9 {
        Vec2::new(math::cos(rng.range_f64(0.0, 6.28)), math::sin(rng.range_f64(0.0, 6.28)))
    } else {
        Vec2::new(-g0.x, -g0.y).normalized()
    };
    let mut arc = 0.0;
    let mut attach: Option<(u32, f64)> = None;
    // Anti-orbit guard: if the walker has barely displaced over the last 30
    // steps it is circling a pocket of the field — force the analytic join
    // to the nearest channel instead of engraving a knot.
    let mut ring: Vec<Vec2> = Vec::new();

    for _ in 0..500 {
        ring.push(*pts.last().unwrap());
        if ring.len() > 30 {
            ring.remove(0);
        }
        let orbiting =
            ring.len() == 30 && ring[0].distance(*pts.last().unwrap()) < 0.30 * 30.0 * p.step_m;
        if orbiting {
            // TRIM the loop before joining: the guard used to force-join with
            // the orbit already walked, which engraved a knot into the path
            // (visible at the strong-gravity ladder rungs). Drop the circled
            // tail, then take the analytic join from clean geometry.
            let keep = pts.len().saturating_sub(26).max(1);
            pts.truncate(keep);
            ring.clear();
        }
        let cur = *pts.last().unwrap();
        if let Some((idx, d)) = chans.nearest(cur) {
            if d <= p.join_r_m * 2.2 || (orbiting && d <= 600.0) {
                // ANALYTIC final approach. Steering toward the target angle
                // only reaches it AT the junction, so the measured entry
                // (over the last ~50-100 m) stayed wide — p50 74 deg against
                // a 37-45 band. A quadratic Bezier whose control point lies
                // back along the desired incoming direction ENTERS at the
                // target angle by construction.
                let jp = chans.pts[idx as usize];
                let down = chans.down[idx as usize];
                let to_ch = Vec2::new(jp.x - cur.x, jp.y - cur.y).normalized();
                let rot = |sgn: f64| {
                    let (c, sn) = (math::cos(theta * sgn), math::sin(theta * sgn));
                    Vec2::new(down.x * c - down.y * sn, down.x * sn + down.y * c)
                };
                let (ra, rb) = (rot(1.0), rot(-1.0));
                let inc = if ra.dot(to_ch) >= rb.dot(to_ch) { ra } else { rb };
                let ctrl = Vec2::new(jp.x - inc.x * d * 0.55, jp.y - inc.y * d * 0.55);
                let nseg = (d / 18.0).ceil().max(2.0) as usize;
                for i in 1..=nseg {
                    let u = i as f64 / nseg as f64;
                    let a = cur.lerp(ctrl, u);
                    let b = ctrl.lerp(jp, u);
                    pts.push(a.lerp(b, u));
                }
                attach = Some((idx, d));
                break;
            }
            // --- steering: descend, smoothed. No gradual angle blend — the
            // analytic tail owns the junction. The blend was the near-parallel
            // machine: for an acute target its direction is nearly parallel
            // to the channel, so walkers crabbed alongside for hundreds of
            // metres before joining (measured near_par 7.0% against a
            // 1.3-3.1 corpus band).
            let g = proto.grad(cur);
            let downhill = if g.length() < 1e-9 { heading } else { Vec2::new(-g.x, -g.y).normalized() };
            // MOMENTUM, not a weighted sum re-derived each step. Near a
            // confluence the nearest-channel reference flips between the two
            // channels, the gradient rotates step to step, and a plain sum
            // let the walker orbit in a tight scribble (user report:
            // piedmont seed 2, the knot beside the junction). Momentum
            // integrates the flips away; the knot cannot form.
            let pull = proto.gravity.pull(chans, cur);
            heading = Vec2::new(
                heading.x * 0.62 + (downhill.x + pull.x) * 0.38,
                heading.y * 0.62 + (downhill.y + pull.y) * 0.38,
            )
            .normalized();
            let dir = heading;
            let wob = p.swing_rad * math::sin(core::f64::consts::TAU * arc / lam + phase);
            // taper the meander out approaching the junction zone
            let blend = ((p.join_r_m * 3.5 - d) / (p.join_r_m * 3.5)).clamp(0.0, 1.0);
            // meander rides on top, tapered out at the junction
            let taper = (1.0 - blend) * (arc / 250.0).min(1.0);
            let th = math::atan2(dir.y, dir.x) + wob * taper;
            let step = Vec2::new(math::cos(th), math::sin(th));
            let next = Vec2::new(cur.x + step.x * p.step_m, cur.y + step.y * p.step_m);
            if next.x < 8.0 || next.y < 8.0 || next.x > EXTENT_M - 8.0 || next.y > EXTENT_M - 8.0 {
                return Err(Death::OffTile);
            }
            heading = step;
            arc += p.step_m;
            pts.push(next);
        } else {
            return Err(Death::Exhausted);
        }
    }
    let Some((attach_pt, _)) = attach else { return Err(Death::Exhausted) };
    // land exactly on the channel point
    pts.push(chans.pts[attach_pt as usize]);
    if pts.len() < 6 {
        return Err(Death::Stub);
    }

    // --- junction-first ordering + long profile
    pts.reverse();
    let total: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    let zj = chans.z[attach_pt as usize];
    // head elevation from the field itself, floored so a trib always climbs
    let head_e = proto.e(*pts.last().unwrap());
    let rise = (head_e - zj).max(1.0);
    let mut z = Vec::with_capacity(pts.len());
    let mut acc = 0.0;
    for (i, q) in pts.iter().enumerate() {
        if i > 0 {
            acc += q.distance(pts[i - 1]);
        }
        z.push(zj + rise * math::pow((acc / total).clamp(0.0, 1.0), p.concavity));
    }
    Ok(Trib {
        pts,
        z,
        attach_pt,
        parent_poly: chans.poly[attach_pt as usize],
        tier: 2,
    })
}
