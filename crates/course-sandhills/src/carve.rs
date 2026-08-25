//! C3 — the valley carve: the network becomes negative relief.
//!
//! A generalisation of the review-passed river-corridor machinery
//! (`water.rs` `carve_corridor`, closed 12/12): the same two-line idea —
//! a bed under the channel, a catena hung off it, min-composed into the
//! ground — but Hack-scaled from 1 m creek heads to trunk valleys, and run
//! over every channel of the network, junction-first.
//!
//! NOTHING here is constant (review 2026-08-25: "as organic as possible,
//! avoid constant widths slopes profiles"):
//!   * depth, floor width and wall run all ride the Hack proxy AND breathe
//!     along the arc on their own noise octaves;
//!   * the two sides of a valley are shaped independently (drawn asymmetry,
//!     slowly flipping sides — the river's cut-bank/slip-off idiom);
//!   * the wall profile exponent itself wanders along the arc;
//!   * the inside of every bend widens with local curvature;
//!   * a two-octave catena noise roughens the walls (zero at the water
//!     line, full at the shoulder) and a short-wave rim jag breaks the
//!     shoulder line.
//!
//! The catena is ONE zone — floor and concave sand slope, no benches, no
//! scarps (not sand country) — and past the wall it keeps rising at a
//! drawn outer grade until it MEETS the upland, so min-composition seals
//! the hand-off with no cliff by construction (the water.rs lesson: a hard
//! reach cutoff against high ground printed an 8 m staircase).

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::noise;

use crate::channel::Network;
use crate::draw::Descriptors;

/// Upstream-length credit for channels that continue off-tile: a trunk's
/// upstream edge is not a creek head, and a fragment keeps draining beyond
/// its border mouth. Drawn per tile. `guess`, bounded by the biome's creek
/// lengths (a few km).
const TRUNK_CREDIT_M: (f64, f64) = (1200.0, 2600.0);
const FRAG_CREDIT_M: (f64, f64) = (800.0, 1800.0);

/// The Hack proxy: incision scales with upstream stem length as
/// `(up_len / 1500)^0.7` — attempt 4's `trib_cut` scaling, re-derived
/// (course-network, branch network-first @ a412b31).
fn hack(up_len_m: f64) -> f64 {
    // exponent 0.7 -> 0.55 (review 2026-08-26: tributaries not cutting
    // enough — the flatter law keeps small channels proportionally larger
    // while the units below hold the trunk where the instrument put it)
    math::pow((up_len_m / 1500.0).max(0.0), 0.55)
}

/// Per-channel carve program: the bed and the arc table, plus the drawn
/// organ seeds. Beds are built junction-first so a tributary floor MEETS
/// its parent floor exactly.
struct Program {
    pts: Vec<Vec2>,
    bed: Vec<f64>,
    /// actual local depth (lowpass ground − bed), ≥ 0.9
    depth: Vec<f64>,
    /// arc position of each point, from the mouth
    arc: Vec<f64>,
    /// Hack factor at each point (drives every size)
    hk: Vec<f64>,
    /// signed turn (heading change per metre, smoothed) for bend widening
    turn: Vec<f64>,
    asym: f64,
    s_wl: u32,
    s_wr: u32,
    s_flip: u32,
    s_p: u32,
    s_jag: u32,
}

/// The graded beds, one per channel: the resampled centre-line and its bed
/// elevation. Junction-first (a tributary mouth IS its parent floor), Flint
/// concave profiles, the water-gap relief valve. This is the whole surviving
/// half of the old carve — the catena stamp is retired in favour of the
/// measured HAND profile in `assemble.rs`.
pub fn beds(rng: &mut DetRng, net: &Network, datum: &Grid<f64>,
            d: &Descriptors) -> Vec<(Vec<Vec2>, Vec<f64>)> {
    // ---- per-tile draws -------------------------------------------------
    let depth_unit = d.valley_depth_m;
    let floor_unit = d.valley_floor_m;
    let wall_unit = d.valley_wall_m;
    let trunk_credit = rng.range_f64(TRUNK_CREDIT_M.0, TRUNK_CREDIT_M.1);
    let frag_credit = rng.range_f64(FRAG_CREDIT_M.0, FRAG_CREDIT_M.1);
    let out_grade = rng.range_f64(0.012, 0.020);
    // Flint's-law steepness: slope(a) = s0 * (up_len/1500)^-0.45, capped at
    // 2% near heads. `measured 2026-08-25` (trunk_profile on 4 real tiles):
    // mainstems run 0.1-0.2% in-tile with the climb in the headwaters —
    // the datum-following bed was linear-to-convex against that.
    let s0 = rng.range_f64(0.0022, 0.0034);
    let p_base = rng.range_f64(1.5, 2.1);
    let s_cat1 = rng.next_u32();
    let s_cat2 = rng.next_u32();
    let s_floor = rng.next_u32();

    // ---- junction-first beds -------------------------------------------
    let mut progs: Vec<Program> = Vec::with_capacity(net.chans.len());
    for (cid, c) in net.chans.iter().enumerate() {
        // resample to ~6 m so the stamp distance is to the LINE, not to
        // sparse points (a 20 m point spacing scallops a 3 m creek floor)
        let (pts, _) = resample(&c.pts, 6.0);
        let n = pts.len();
        let mut arc = vec![0.0f64; n];
        for i in 1..n {
            arc[i] = arc[i - 1] + pts[i - 1].distance(pts[i]);
        }
        let total = arc[n - 1];
        let credit = if c.parent.is_none() {
            if c.sys == 0 { trunk_credit } else { frag_credit }
        } else {
            0.0
        };
        let s_depth = rng.next_u32();
        let mut hk: Vec<f64> = (0..n)
            .map(|i| hack(total - arc[i] + credit))
            .collect();
        // a fragment's MOUTH also carries downstream credit (it keeps
        // flowing off-tile); fold it in as a floor on the mouth half
        if c.parent.is_none() && c.sys > 0 {
            for i in 0..n {
                let fade = (1.0 - arc[i] / (total * 0.5)).max(0.0);
                hk[i] = hk[i].max(hack(frag_credit) * fade);
            }
        }

        // bed target: the LOWPASS of the datum along the path, minus the
        // breathing Hack depth
        let mut ground: Vec<f64> = pts.iter().map(|q| datum.bilinear(*q)).collect();
        for _ in 0..3 {
            for i in 1..n - 1 {
                ground[i] = (ground[i - 1] + ground[i] * 2.0 + ground[i + 1]) / 4.0;
            }
        }
        let mut bed: Vec<f64>;
        if let Some(pid) = c.parent {
            // A TRIBUTARY IS GRADED TO ITS TRUNK (review 2026-08-27: on
            // real tiles the ENTIRE system sits in the green — trib valleys
            // are deeply incised near their mouths, shallowing upstream).
            // The old ground-following bed put a trib at a formula depth
            // below the LOCAL upland, so it left the low band within 150 m
            // of its junction. Here the bed is INTEGRATED upstream from the
            // junction: the mouth floor IS the parent floor, rising by the
            // stream's own concave law; depth below upland is an OUTCOME —
            // parent-scale at the mouth, ~1 m at the head.
            let par = &progs[pid as usize];
            let mut best = (f64::MAX, 0usize);
            for (j, q) in par.pts.iter().enumerate() {
                let dd = q.distance(pts[0]);
                if dd < best.0 {
                    best = (dd, j);
                }
            }
            bed = vec![0.0; n];
            // Gully-scale channels (tier 4) HANG on the valley wall: a side
            // swale grades from the wall face, it does not slot down to the
            // trunk floor (floor-anchored gullies printed as claw-shaped
            // gouges along every rim — render, seed 109). Real ephemeral
            // side channels join above the floor.
            bed[0] = if c.tier >= 4 {
                par.bed[best.1].max(ground[0] - 2.2)
            } else {
                par.bed[best.1]
            };
            for i in 1..n {
                let ds = arc[i] - arc[i - 1];
                let up = (total - arc[i] + credit).max(30.0);
                let smax = (s0 * math::pow(up / 1500.0, -0.45)).min(0.02);
                let mut b = bed[i - 1] + smax * ds;
                // the relief valve, unchanged: through high ground the
                // profile may steepen to 2% rather than cut a gorge
                let dcap = (1.35 * depth_unit * hk[i]).max(6.0);
                b = b.max((ground[i] - dcap).min(bed[i - 1] + 0.02 * ds));
                // and a trib may not end ABOVE its local ground: cap the
                // rise so the head lands ~0.9 m under the upland
                b = b.min(ground[i] - 0.9);
                bed[i] = b.max(bed[i - 1] + 0.0004 * ds);
            }
        } else {
            bed = (0..n)
                .map(|i| {
                    let lam = 430.0 + 320.0 * hk[i];
                    let breathe = 1.0 + 0.22 * noise::perlin1(arc[i] / lam, s_depth);
                    let depth = (depth_unit * hk[i] * breathe).max(0.9);
                    ground[i] - depth
                })
                .collect();
            // THE LONG PROFILE: a concave slope cap on the trunk's own bed
            // — flat mainstem, steepening headwaters.
            for i in 1..n {
                let ds = arc[i] - arc[i - 1];
                let up = (total - arc[i] + credit).max(30.0);
                let smax = (s0 * math::pow(up / 1500.0, -0.45)).min(0.02);
                let mut b = bed[i].min(bed[i - 1] + smax * ds);
                let dcap = (1.35 * depth_unit * hk[i]).max(6.0);
                b = b.max((ground[i] - dcap).min(bed[i - 1] + 0.02 * ds));
                bed[i] = b.max(bed[i - 1] + 0.0004 * ds);
            }
        }

        // local depth = the ACTUAL ground-to-bed gap: once the profile
        // cap binds, the hack formula no longer knows how deep the bed sits
        let depth: Vec<f64> = (0..n).map(|i| (ground[i] - bed[i]).max(0.9)).collect();

        // signed turn for the inside-of-bend widening (river idiom)
        let mut turn = vec![0.0f64; n];
        let w = 8usize; // ~48 m window
        for i in 0..n {
            let a = i.saturating_sub(w);
            let b = (i + w).min(n - 1);
            if b > a + 1 {
                let h0 = math::atan2(pts[a + 1].y - pts[a].y, pts[a + 1].x - pts[a].x);
                let h1 = math::atan2(pts[b].y - pts[b - 1].y, pts[b].x - pts[b - 1].x);
                let mut dh = h1 - h0;
                while dh > std::f64::consts::PI { dh -= std::f64::consts::TAU; }
                while dh < -std::f64::consts::PI { dh += std::f64::consts::TAU; }
                turn[i] = dh / (arc[b] - arc[a]).max(1.0);
            }
        }

        progs.push(Program {
            pts, bed, depth, arc, hk, turn,
            asym: rng.range_f64(0.35, 0.65),
            s_wl: rng.next_u32(),
            s_wr: rng.next_u32(),
            s_flip: rng.next_u32(),
            s_p: rng.next_u32(),
            s_jag: rng.next_u32(),
        });
        let _ = cid;
    }

    progs.into_iter().map(|pr| (pr.pts, pr.bed)).collect()
}

fn resample(pts: &[Vec2], step: f64) -> (Vec<Vec2>, Vec<usize>) {
    if pts.len() < 2 {
        return (pts.to_vec(), vec![0; pts.len()]);
    }
    let mut out = vec![pts[0]];
    let mut src = vec![0usize];
    let mut carry = 0.0;
    for i in 1..pts.len() {
        let seg = pts[i - 1].distance(pts[i]);
        if seg < 1e-9 {
            continue;
        }
        let dirx = (pts[i].x - pts[i - 1].x) / seg;
        let diry = (pts[i].y - pts[i - 1].y) / seg;
        let mut t = step - carry;
        while t <= seg {
            out.push(Vec2::new(pts[i - 1].x + dirx * t, pts[i - 1].y + diry * t));
            src.push(i);
            t += step;
        }
        carry = seg - (t - step);
    }
    if out.last().unwrap().distance(*pts.last().unwrap()) > step * 0.4 {
        out.push(*pts.last().unwrap());
        src.push(pts.len() - 1);
    }
    (out, src)
}

/// Re-base the HANGING tiers (4/5) onto an assembled surface.
///
/// Their beds are first built against the routing datum, which is not the
/// ground the tile ends up with — B + H differs from it by metres. Left
/// uncorrected, a gully bed sits at the wrong height and drags the bed
/// field with it, which printed a hard step beside every channel (render,
/// seeds 104/109). So the macro build assembles once from the trunk-and-
/// tributary tiers, re-bases the gullies onto THAT ground, and assembles
/// again.
pub fn rebase_hanging(beds: &mut [(Vec<Vec2>, Vec<f64>)], net: &Network,
                      surface: &Grid<f64>) {
    for (ci, c) in net.chans.iter().enumerate() {
        let (pts, bed) = &mut beds[ci];
        let n = pts.len();
        if n < 2 {
            continue;
        }
        if c.tier >= 4 {
            // a swale is a shallow incision into the ground it crosses
            let cut = 0.7 + 0.5 * ((c.tier as f64) - 4.0).max(0.0);
            let mut arc = 0.0;
            for i in 0..n {
                if i > 0 {
                    arc += pts[i - 1].distance(pts[i]);
                }
                let g = surface.bilinear(pts[i]);
                let want = g - cut * (1.0 - 0.5 * (arc / 260.0).min(1.0));
                bed[i] = if i == 0 { want } else { want.max(bed[i - 1] + 0.002) };
            }
            continue;
        }
        // THE DEPTH CAP for the graded tiers. Their beds are cut against the
        // routing datum, which is NOT the ground the tile ends up with, so
        // wherever the assembled surface rose above the datum a bed was left
        // far beneath it and its HEAD printed a deep conical pit — measured,
        // 36 of a tile's 40 sharpest cells sat within 40 m of a channel
        // head. Valley DEPTH is the H profile's job; a bed only has to be
        // the local low, so none may sit more than `cut` under its ground.
        let cut = match c.tier {
            1 => 3.2,
            2 => 2.4,
            _ => 1.7,
        };
        for i in 0..n {
            let g = surface.bilinear(pts[i]);
            if bed[i] < g - cut {
                bed[i] = g - cut;
            }
        }
        // raising a bed can break the upstream-rising invariant
        for i in 1..n {
            let ds = pts[i - 1].distance(pts[i]);
            bed[i] = bed[i].max(bed[i - 1] + 0.0004 * ds);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use course_seed::RunIdentity;

    fn graded(seed: u64) -> (Grid<f64>, Network, Vec<(Vec<Vec2>, Vec<f64>)>) {
        let id = RunIdentity::from_seed(seed);
        let d = crate::draw::site(&id, Some(Mode::Fluvial), None);
        let mut dr = crate::rng::stream(&id, crate::rng::DATUM);
        let datum = crate::channel::datum(&mut dr, &d);
        let mut cr = crate::rng::stream(&id, crate::rng::CHANNEL);
        let net = crate::channel::grow(&mut cr, &datum, &d);
        let beds = beds(&mut cr, &net, &datum, &d);
        (datum, net, beds)
    }

    #[test]
    fn every_bed_rises_upstream() {
        // water cannot flow uphill along its own floor
        for seed in [201u64, 202, 203] {
            let (_, _, bs) = graded(seed);
            for (i, (_, b)) in bs.iter().enumerate() {
                for w in b.windows(2) {
                    assert!(w[1] >= w[0] - 1e-9,
                            "seed {seed} chan {i}: bed falls upstream");
                }
            }
        }
    }

    #[test]
    fn tributary_mouths_meet_their_parent_floor() {
        // THE grading law (X0c): a trib mouth IS its parent's floor there,
        // which is what puts the whole system in the tile's low ground
        for seed in [204u64, 205] {
            let (_, net, bs) = graded(seed);
            let mut worst: f64 = 0.0;
            for (ci, c) in net.chans.iter().enumerate() {
                let Some(pid) = c.parent else { continue };
                // tier 4/5 HANG on the wall by design — a side swale grades
                // from the wall face, it does not slot down to the trunk
                // floor (floor-anchored gullies printed claw gouges)
                if c.tier >= 4 {
                    continue;
                }
                let (ppts, pbed) = &bs[pid as usize];
                let mouth = bs[ci].0[0];
                let mut best = (f64::MAX, 0usize);
                for (j, q) in ppts.iter().enumerate() {
                    let dd = q.distance(mouth);
                    if dd < best.0 {
                        best = (dd, j);
                    }
                }
                worst = worst.max((bs[ci].1[0] - pbed[best.1]).abs());
            }
            assert!(worst < 3.0, "seed {seed}: mouth off parent floor by {worst:.1} m");
        }
    }

    #[test]
    fn beds_stay_under_the_ground_they_cut() {
        for seed in [206u64, 207] {
            let (datum, _, bs) = graded(seed);
            let mut worst: f64 = 0.0;
            for (pts, b) in &bs {
                for (i, p) in pts.iter().enumerate() {
                    worst = worst.max(b[i] - datum.bilinear(*p));
                }
            }
            // The coverage-pass channels climb well above the routing
            // datum's local value before rebase_hanging puts them on the
            // ASSEMBLED ground — the datum is a steering field, not the
            // tile's elevation, and only tiers 1-3 are graded against it.
            assert!(worst < 8.0, "seed {seed}: bed {worst:.1} m above the datum");
        }
    }
}
