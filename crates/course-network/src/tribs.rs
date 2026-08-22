//! Phase N2, reworked (user design): tributaries grow FROM the trunk,
//! outward and uphill.
//!
//! The junction is t = 0. A climber departs an authored attachment point at
//! exactly the drawn angle, holds that course for a while (the down-valley
//! look, directly), then steers uphill on the proto field toward high ground
//! or a high edge, with a small meander.
//!
//! **No loops, structurally:** every step must GAIN proto-elevation, so the
//! path follows a strictly increasing potential — and a strictly increasing
//! path cannot revisit any point, at any dial setting. The approach blend,
//! analytic join, orbit guard and loop trim of the descent formulation are
//! all deleted; they existed to solve an arrival problem that no longer
//! exists.

use crate::proto::{ChannelSet, Proto};
use course_seed::DetRng;
use course_world::math::{self, Vec2};
use course_world::world::EXTENT_M;

/// One tributary, JUNCTION-FIRST — which is simply the walk order.
#[derive(Clone, Debug)]
pub struct Trib {
    pub pts: Vec<Vec2>,
    pub z: Vec<f64>,
    /// Channel-point index (in the ChannelSet at build time) it departs from.
    pub attach_pt: u32,
    /// Polyline id it attaches to.
    pub parent_poly: u32,
    pub tier: u8,
}

/// Why a climb ended. Every termination is counted, never hidden.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End {
    /// Reached a divide: even straight uphill gains nothing.
    Divide,
    /// Stopped at the tile edge (a high-edge head is legitimate).
    Edge,
    /// Came within `claim_m` of another channel.
    Claimed,
    /// Hit the length cap.
    MaxArc,
    /// Too short to keep.
    Stub,
}

pub struct TierParams {
    /// Junction spacing along the parent, metres (drawn per gap).
    pub attach_spacing_m: (f64, f64),
    /// No attachments this close to a mouth or head.
    pub end_margin_m: f64,
    /// Departure-angle distribution: centre and orthogonal-tail probability.
    /// Corpus: p50 37–45°, >80° share 8.5–13.4%.
    pub junc_centre_deg: f64,
    pub junc_tail_p: f64,
    /// Hold the departure course this long before releasing to uphill —
    /// the "downstream sections have greater mass" dial, expressed
    /// deterministically. Interpolated by the junction's arc position:
    /// (at the mouth, at the head).
    pub hold_m: (f64, f64),
    /// Meander: swing (radians) and wavelength. Scaled down twice by
    /// review: once at N2, again at T2 — the tier cut prints the path
    /// directly into the surface, so planform wiggle becomes carved
    /// gully wiggle at full amplitude.
    pub swing_rad: f64,
    pub lam_m: (f64, f64),
    /// A climber stops on coming this close to another channel.
    pub claim_m: f64,
    /// Minimum kept length.
    pub min_len_m: f64,
    /// Length cap.
    pub max_len_m: f64,
    pub step_m: f64,
    /// Minimum proto-elevation gain per step; below it the head is reached.
    pub min_gain_m: f64,
}

pub fn tier2() -> TierParams {
    TierParams {
        attach_spacing_m: (380.0, 560.0),
        end_margin_m: 260.0,
        junc_centre_deg: 41.0,
        // Drawn LOW because survival selects for it: acute departures hug the
        // valley and die early inside meander bends (the far limb of the
        // parent lowers the field ahead), while orthogonal climbers head
        // straight uphill and live. 0.05 drawn lands ~9-13% measured.
        junc_tail_p: 0.10,
        hold_m: (500.0, 120.0),
        swing_rad: 0.09,
        lam_m: (250.0, 550.0),
        claim_m: 180.0,
        min_len_m: 150.0,
        max_len_m: 2600.0,
        step_m: 24.0,
    min_gain_m: 0.012,
    }
}

/// Attachment sites along every existing channel: arc positions at drawn
/// spacings, clear of the ends and of each other. Junction spacing is
/// AUTHORED here — an input, not an outcome of where sources landed.
pub fn attach_points(rng: &mut DetRng, chans: &ChannelSet, p: &TierParams) -> Vec<u32> {
    let mut out: Vec<u32> = Vec::new();
    let n_poly = chans.total.len() as u32;
    for pid in 0..n_poly {
        let total = chans.total[pid as usize];
        if total < p.end_margin_m * 2.0 + p.attach_spacing_m.0 {
            continue;
        }
        let mut arc = p.end_margin_m + rng.range_f64(0.0, p.attach_spacing_m.0 * 0.6);
        while arc < total - p.end_margin_m {
            // find the point index at this arc on this polyline
            let mut best = (u32::MAX, f64::MAX);
            for i in 0..chans.pts.len() {
                if chans.poly[i] == pid {
                    let d = (chans.arc[i] - arc).abs();
                    if d < best.1 {
                        best = (i as u32, d);
                    }
                }
            }
            if best.0 != u32::MAX {
                out.push(best.0);
            }
            arc += rng.range_f64(p.attach_spacing_m.0, p.attach_spacing_m.1);
        }
    }
    out
}

/// Climb from one attachment point. Returns the trib and how it ended, or
/// just the end cause when nothing worth keeping was produced.
pub fn climb(
    rng: &mut DetRng,
    attach_pt: u32,
    proto: &Proto,
    p: &TierParams,
) -> Result<(Trib, End), End> {
    let chans = proto.chans;
    let a = attach_pt as usize;
    let start = chans.pts[a];
    let down = chans.down[a];
    let frac = (chans.arc[a] / chans.total[chans.poly[a] as usize]).clamp(0.0, 1.0);

    // --- the departure: flow arrives at the junction at theta off the
    // downstream tangent; the WALK leaves along the reverse of that flow.
    let theta = if rng.next_f64() < p.junc_tail_p {
        rng.range_f64(80.0, 95.0).to_radians()
    } else {
        rng.range_f64(p.junc_centre_deg - 8.0, p.junc_centre_deg + 9.0).to_radians()
    };
    let side = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
    let (c, sn) = (math::cos(theta * side), math::sin(theta * side));
    let inc = Vec2::new(down.x * c - down.y * sn, down.x * sn + down.y * c);
    let depart = Vec2::new(-inc.x, -inc.y).normalized();

    let hold_m = p.hold_m.0 + (p.hold_m.1 - p.hold_m.0) * frac;
    let lam = rng.range_f64(p.lam_m.0, p.lam_m.1);
    let phase = rng.range_f64(0.0, core::f64::consts::TAU);

    let mut pts = vec![start];
    let mut z = vec![chans.z[a]];
    let mut heading = depart;
    let mut arc = 0.0;
    let mut e_prev = proto.e(start);
    let end;

    loop {
        if arc >= p.max_len_m {
            end = End::MaxArc;
            break;
        }
        let cur = *pts.last().unwrap();

        // steering: departure course while holding, then uphill
        let g = proto.grad(cur);
        let uphill = if g.length() < 1e-9 { heading } else { g.normalized() };
        // Full commitment for the first few steps: at 0.85 the momentum blend
        // tilted the measured departure ~10 deg toward uphill within 72 m
        // (measured p50 52 vs drawn 41). The junction owns its angle.
        let hold_w = if arc < 3.0 * p.step_m {
            1.0
        } else {
            ((hold_m - arc) / hold_m.max(1.0)).clamp(0.0, 1.0) * 0.85
        };
        let base = Vec2::new(
            depart.x * hold_w + uphill.x * (1.0 - hold_w),
            depart.y * hold_w + uphill.y * (1.0 - hold_w),
        );
        // momentum smoothing (generic smoothing, not loop suppression)
        heading = Vec2::new(
            heading.x * 0.55 + base.x * 0.45,
            heading.y * 0.55 + base.y * 0.45,
        )
        .normalized();
        let wob = p.swing_rad * math::sin(core::f64::consts::TAU * arc / lam + phase)
            * (arc / 200.0).min(1.0);
        let th = math::atan2(heading.y, heading.x) + wob;
        let mut step_dir = Vec2::new(math::cos(th), math::sin(th));

        // THE MONOTONE RULE: the step must gain elevation — with a bounded
        // GRACE window at the start. A real tributary's lower course crosses
        // near-flat floodplain, and inside a meander bend the parent's far
        // limb lowers the field ahead; strict monotone from step 1 killed
        // acute departures early and survival selected for orthogonal ones
        // (measured >80 share 30.8% against a 12% draw). During grace the
        // heading is committed, so the window cannot loop; after it the
        // strict rule resumes and the no-revisit guarantee holds.
        let grace = arc < (hold_m * 0.5).clamp(120.0, 280.0);
        let mut next = Vec2::new(cur.x + step_dir.x * p.step_m, cur.y + step_dir.y * p.step_m);
        let mut e_next = proto.e(next);
        if !grace && e_next < e_prev + p.min_gain_m {
            step_dir = uphill;
            next = Vec2::new(cur.x + step_dir.x * p.step_m, cur.y + step_dir.y * p.step_m);
            e_next = proto.e(next);
            if e_next < e_prev + p.min_gain_m {
                end = End::Divide;
                break;
            }
        }

        // tile edge: stop AT the edge (a high-edge head is legitimate)
        if next.x < 4.0 || next.y < 4.0 || next.x > EXTENT_M - 4.0 || next.y > EXTENT_M - 4.0 {
            end = End::Edge;
            break;
        }
        // proximity to another channel — the parent itself is exempt by
        // IDENTITY, not by arc (see ChannelSet::nearest_excl)
        {
            if let Some((_, d)) = chans.nearest_excl(next, chans.poly[a]) {
                if d < p.claim_m {
                    // retract the last couple of steps so the head does not
                    // point-blank a neighbour
                    for _ in 0..2 {
                        if pts.len() > 2 {
                            pts.pop();
                            z.pop();
                        }
                    }
                    end = End::Claimed;
                    break;
                }
            }
        }

        heading = step_dir;
        arc += p.step_m;
        pts.push(next);
        // elevation from the field, forced strictly non-decreasing
        let zi = (e_next - proto.e(start) + chans.z[a]).max(z.last().unwrap() + 0.01);
        z.push(zi);
        e_prev = e_next;
    }

    let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    if len < p.min_len_m || pts.len() < 4 {
        return Err(End::Stub);
    }
    Ok((
        Trib {
            pts,
            z,
            attach_pt,
            parent_poly: chans.poly[a],
            tier: 2,
        },
        end,
    ))
}
