//! The creek planform as a BEND TRAIN.
//!
//! ## Why a train and not a wave
//!
//! Both creek generators used to offset a smooth trunk line by
//! `A · [sin φ + 0.35 sin(φ/2.7)]` with `A` constant along the reach. Review,
//! twice: "too sinusoidal", "still artificial ... just due to the sinusoidal
//! nature". A closed-form periodic function of arc length has a CARRIER, and
//! every modulation bolted onto it (skew, amplitude envelope, wavelength
//! wobble) decorates the carrier without removing it. The eye reads the
//! carrier. The grown migration model that replaced it (reverted 52f3e4c,
//! record in `docs/calibration/creek-planform.md`) had no carrier but could
//! not reach real bend irregularity at a golf-usable amplitude.
//!
//! This module draws the creek the way the corpus DESCRIBES it: as a sequence
//! of individual bends and straight runs, each with its own length, amplitude
//! and apex skew drawn independently from the measured distributions. There
//! is no function of `s` with a period. Measured on 55,281 OSM
//! `waterway=stream` reaches over the corpus regions (10 m resample, 30 m
//! smoothing -- the same pipeline the sheet applies to this output):
//!
//! | statistic | p25 | p50 | p75 |
//! |---|---|---|---|
//! | bend length | 40 m | 50 m | 60 m |
//! | bend-length CV | 0.38 | 0.46 | 0.55 |
//! | sinuosity | 1.063 | 1.117 | 1.225 |
//! | apex skew (fraction of the bend from its UPSTREAM end) | 0.40 | 0.60 | 0.71 |
//! | length straighter than R = 150 m | -- | 59.7 % | -- |
//!
//! ## Contract
//!
//! * **Zero RNG draws.** There is no `DetRng` parameter, so the transcript
//!   cannot shift. Every random quantity is an integer hash of
//!   `(salt, element index, channel)`, or a `perlin1` of arc length for the
//!   slow envelopes. Changing the node count cannot move a draw.
//! * **The creek stays on its floor.** `room_at(q)` returns metres of lateral
//!   room from `q` to the floor edge; each bend's amplitude is capped against
//!   it ONCE, at the bend, so there is no per-sample snapping (the old
//!   `fac` clamp teleported the creek back to the centre-line and left gaps).
//! * **The creek does not sculpt anything.** This module emits a polyline and
//!   a station `t` on the ORIGINAL base line; the caller re-keys the bed
//!   through `t`, and the carve is the caller's (`water::cut_creek` profile).
//! * **Index direction is the caller's.** `Flow` says which way the index
//!   runs; skew leans DOWNSTREAM, and getting this backwards passes every
//!   statistic except the one the reviewer sees first (the migration model's
//!   recorded failure).

use course_world::math::{self, Vec2};
use course_world::noise;

/// Which way the base-line index runs. `carve::beds` builds channels
/// mouth-first; OSM ways (and the corpus skew statistic) run head-first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    MouthFirst,
    HeadFirst,
}

/// Character dials. Ranges are what `from_draws` produces; the sheet sweeps
/// beyond them.
#[derive(Clone, Debug)]
pub struct Params {
    /// Wet channel width, metres. Scales bend length (a wider creek makes
    /// longer bends) and the curvature floor.
    pub width_m: f64,
    /// Bend-length multiplier, 0.80..1.25.
    pub len_mult: f64,
    /// Amplitude multiplier, 0.70..1.40.
    pub vigour: f64,
    /// Probability that a bend is followed by a straight run, 0.35..0.60.
    pub straightness: f64,
    /// Probability that a bend repeats the previous bend's side (compound
    /// bends -- a real feature; two lobes on one bank).
    pub same_sign_p: f64,
    /// Curvature floor at the bend apex, metres.
    pub r_min_m: f64,
    /// Output node spacing, metres. 1.0 for the 2 m fluvial grid.
    pub sample_m: f64,
    /// `[1,2,1]/4` passes on the base line before offsetting. 0 if the
    /// caller already smoothed it.
    pub smooth_passes: usize,
    /// Sub-metre micro-wobble amplitude; real creeks are never ruler-straight.
    /// 0 = ablation.
    pub wobble_m: f64,
    /// Log-normal sigma of bend length. The corpus p50 of 50 m is dominated
    /// by small kinks; the swings the eye reads are the 100-250 m bends in
    /// the tail, so this is a look dial, not just an irregularity dial.
    pub len_sigma: f64,
    /// Median amplitude as a fraction of bend length.
    pub amp_ratio: f64,
}

// --- the measured distributions (starting points; tuned on the sheet) -----

/// Median bend length at width 5 m, metres (corpus p50 = 50).
pub const BEND_LEN_M: f64 = 58.0;
/// Log-normal sigma of bend length. `sqrt(ln(1 + CV^2))` at the corpus CV
/// 0.46 would be 0.45; the sheet (2026-09-01) chose the heavier tail, because
/// the corpus p50 is dominated by 40 m kinks while the swings the eye reads
/// on a real reach are the 100-250 m bends in the tail. The measured CV
/// still lands in band (0.36-0.45) because the 30 m pipeline splits long
/// bends at their own small inflections.
pub const BEND_LEN_SIGMA: f64 = 0.70;
pub const BEND_LEN_CLAMP: (f64, f64) = (22.0, 260.0);
/// Median straight-run length, metres, and its log-normal sigma. Together
/// with `straightness` these set the ~60 % straight fraction in ~120 m runs.
pub const STRAIGHT_LEN_M: f64 = 70.0;
pub const STRAIGHT_LEN_SIGMA: f64 = 0.6;
pub const STRAIGHT_LEN_CLAMP: (f64, f64) = (15.0, 350.0);
/// Amplitude as a fraction of bend length, median and log-normal sigma.
/// Sheet 2026-09-01: 0.09 read as "a line with wiggles"; 0.14 puts 20-40 m
/// swings on the long bends (real panels: 6-55 m amplitude p95) at 1.1 %
/// double-crossing and 15.8 m amplitude p95 on a straight corridor -- the
/// routing target. 0.18 was the bold end (1.7 %, 18.5 m).
pub const AMP_RATIO: f64 = 0.14;
pub const AMP_SIGMA: f64 = 0.5;
pub const AMP_RATIO_CLAMP: (f64, f64) = (0.02, 0.30);
/// Apex position from the UPSTREAM end: mean, gaussian sigma, clamp. The
/// clamp is the range over which the skew warp stays monotone.
pub const SKEW_MEAN: f64 = 0.55;
pub const SKEW_SIGMA: f64 = 0.16;
pub const SKEW_CLAMP: (f64, f64) = (0.30, 0.70);
/// Slow amplitude envelope: two non-harmonic scales.
pub const ENV: [(f64, f64); 2] = [(0.30, 420.0), (0.15, 150.0)];
/// Micro-wobble scales, metres.
pub const WOBBLE_L: (f64, f64) = (47.0, 83.0);
/// Fraction of the local room a bend may use.
pub const ROOM_FRAC: f64 = 0.80;
/// Below this amplitude a bend is not worth drawing; it becomes a straight.
pub const MIN_AMP_M: f64 = 0.6;
/// Offset taper at both ends, metres, so the creek meets the trunk line.
pub const TAPER_M: f64 = 40.0;
/// Box half-width, metres, of the join smoothing on the offset series.
pub const JOIN_SMOOTH_M: f64 = 5.0;
/// Draws consumed by this module: none, by type.
pub const DRAWS: usize = 0;

impl Params {
    /// The ONE mapping from the five WATER draws that used to feed the sine
    /// (`RiverStyle` lambda 260..700, swing 0.40..1.70, phase 0..tau) to
    /// character dials. The sheet and production both go through here so
    /// they cannot drift. `mstyle` (the style index) is not needed.
    pub fn from_draws(m_lam: f64, m_swing: f64, m_phase: f64, width_m: f64) -> Params {
        let f_lam = ((m_lam - 260.0) / (700.0 - 260.0)).clamp(0.0, 1.0);
        let f_sw = ((m_swing - 0.40) / (1.70 - 0.40)).clamp(0.0, 1.0);
        let f_ph = (m_phase / std::f64::consts::TAU).clamp(0.0, 1.0);
        Params {
            width_m,
            len_mult: 0.80 + 0.45 * f_lam,
            vigour: 0.70 + 0.70 * f_sw,
            straightness: 0.35 + 0.25 * f_ph,
            same_sign_p: 0.15,
            // Real 50 m bends at sinuosity 1.12 have apex radii of ~25 m
            // (R/W ~ 5 on a 5 m creek). 35 m capped every bend at ~6 m.
            r_min_m: (3.0 * width_m).max(25.0),
            sample_m: 1.0,
            smooth_passes: 6,
            wobble_m: 0.35,
            len_sigma: BEND_LEN_SIGMA,
            amp_ratio: AMP_RATIO,
        }
    }
}

/// A creek planform.
#[derive(Clone, Debug)]
pub struct Planform {
    /// Creek nodes, in the same index direction as the base line.
    pub p: Vec<Vec2>,
    /// Station on the ORIGINAL base line, arc-normalised to [0, 1],
    /// non-decreasing in the index. Re-key the bed through this.
    pub t: Vec<f64>,
    /// Signed lateral offset from the (smoothed) base at each node, metres.
    pub off: Vec<f64>,
    /// Bends drawn, and how many of them the room cap turned into straights.
    pub n_bends: usize,
    pub n_capped: usize,
}

#[derive(Clone, Copy, Debug)]
struct Element {
    s0: f64,
    len: f64,
    /// Signed amplitude; 0 for a straight run.
    amp: f64,
    /// Apex position in BASE-INDEX order.
    tau_s: f64,
}

// --- hashing: the only randomness ------------------------------------------

/// Same mixer idiom as `course_world::noise::hash2` (private there).
#[inline]
fn hash_u32(salt: u32, k: u32, ch: u32) -> u32 {
    let mut h = salt ^ 0x9E37_79B9;
    h = h.wrapping_add(k.wrapping_mul(0x85EB_CA6B));
    h ^= h >> 15;
    h = h.wrapping_add(ch.wrapping_mul(0xC2B2_AE35));
    h ^= h >> 13;
    h = h.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 16;
    h
}

/// Uniform in [0, 1).
#[inline]
fn hash01(salt: u32, k: u32, ch: u32) -> f64 {
    (hash_u32(salt, k, ch) >> 8) as f64 / (1u32 << 24) as f64
}

/// Standard normal by Box-Muller on two hash channels.
#[inline]
fn gauss(salt: u32, k: u32, ch: u32) -> f64 {
    let u1 = hash01(salt, k, ch).max(1e-12);
    let u2 = hash01(salt, k, ch + 1);
    (-2.0 * math::ln(u1)).sqrt() * math::cos(std::f64::consts::TAU * u2)
}

// --- geometry helpers -------------------------------------------------------

/// `[1,2,1]/4` on interior nodes, `n` passes. Provenance: `gorge.rs:423-433`
/// @ 3839dcb, where it raises the base line's bend radius above the offset
/// amplitude so the offset cannot fold into a cusp on the inside of a bend.
fn smooth_line(pts: &[Vec2], n: usize) -> Vec<Vec2> {
    let mut v = pts.to_vec();
    for _ in 0..n {
        let src = v.clone();
        for i in 1..src.len().saturating_sub(1) {
            v[i] = Vec2::new((src[i - 1].x + 2.0 * src[i].x + src[i + 1].x) * 0.25,
                             (src[i - 1].y + 2.0 * src[i].y + src[i + 1].y) * 0.25);
        }
    }
    v
}

/// Walk a polyline at a fixed interval, carrying two per-point values.
/// Provenance: `gorge.rs:570-594` @ 3839dcb (one value there).
fn resample2(pts: &[Vec2], a: &[f64], b: &[f64], step: f64)
    -> (Vec<Vec2>, Vec<f64>, Vec<f64>) {
    if pts.len() < 2 {
        return (pts.to_vec(), a.to_vec(), b.to_vec());
    }
    let (mut op, mut oa, mut ob) = (vec![pts[0]], vec![a[0]], vec![b[0]]);
    let mut carry = 0.0f64;
    for k in 0..pts.len() - 1 {
        let (p, q) = (pts[k], pts[k + 1]);
        let seg = p.distance(q);
        if seg <= 1e-9 {
            continue;
        }
        let mut t = step - carry;
        while t <= seg {
            let f = t / seg;
            op.push(Vec2::new(p.x + (q.x - p.x) * f, p.y + (q.y - p.y) * f));
            oa.push(a[k] + (a[k + 1] - a[k]) * f);
            ob.push(b[k] + (b[k + 1] - b[k]) * f);
            t += step;
        }
        carry = seg - (t - step);
    }
    // always land EXACTLY on the last node, so the creek reaches the
    // mouth/head and `t` ends at the last station (the re-keyed bed must
    // finish at the last bed value, not a hair short of it)
    let last = *pts.last().unwrap();
    if op.len() > 1 && op.last().unwrap().distance(last) <= step * 0.25 {
        op.pop();
        oa.pop();
        ob.pop();
    }
    op.push(last);
    oa.push(*a.last().unwrap());
    ob.push(*b.last().unwrap());
    (op, oa, ob)
}

/// Pull any node whose circumradius over `span` falls under `r_min` toward
/// its chord. Provenance: `gorge.rs:783-812` @ 3839dcb. Here it is the
/// backstop for the SUM of base-line wander and bend, which the per-bend cap
/// cannot see.
fn limit_curvature(line: &mut [Vec2], r_min: f64, span: usize) {
    if line.len() < 2 * span + 3 {
        return;
    }
    for _ in 0..24 {
        let src = line.to_vec();
        let mut worst = 0.0f64;
        for i in span..src.len() - span {
            let (a, b, c) = (src[i - span], src[i], src[i + span]);
            let (ab, bc, ca) = (a.distance(b), b.distance(c), c.distance(a));
            let area2 = ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs();
            if area2 < 1e-9 {
                continue;
            }
            let r = ab * bc * ca / (2.0 * area2);
            if r >= r_min {
                continue;
            }
            worst = worst.max(r_min / r);
            let k = (0.5 * (1.0 - r / r_min)).clamp(0.0, 0.5);
            let mid = Vec2::new((a.x + c.x) * 0.5, (a.y + c.y) * 0.5);
            line[i] = Vec2::new(b.x + (mid.x - b.x) * k, b.y + (mid.y - b.y) * k);
        }
        if worst <= 1.0 {
            break;
        }
    }
}

fn cum_arc(pts: &[Vec2]) -> Vec<f64> {
    let mut c = Vec::with_capacity(pts.len());
    let mut s = 0.0;
    c.push(0.0);
    for i in 1..pts.len() {
        s += pts[i - 1].distance(pts[i]);
        c.push(s);
    }
    c
}

/// The skewed bump: `sin(pi * w(tau))` with `w` a quadratic warp that puts
/// the apex at `tau_s`. Monotone for `tau_s` in (0.293, 0.707), hence
/// `SKEW_CLAMP`.
#[inline]
fn bump(tau: f64, tau_s: f64) -> f64 {
    let beta = (0.5 - tau_s) / (tau_s * (1.0 - tau_s));
    let w = tau + beta * tau * (1.0 - tau);
    math::sin(std::f64::consts::PI * w)
}

/// `dw/dtau` at the apex, for the curvature cap.
#[inline]
fn bump_slope_at_apex(tau_s: f64) -> f64 {
    let beta = (0.5 - tau_s) / (tau_s * (1.0 - tau_s));
    1.0 + beta * (1.0 - 2.0 * tau_s)
}

// --- the train ---------------------------------------------------------------

/// Build the creek planform over `base` (any spacing, in `flow` order).
/// `room_at(q)` = metres of lateral room from `q` to the floor edge.
pub fn bend_train(base: &[Vec2], flow: Flow, prm: &Params, salt: u32,
                  room_at: &dyn Fn(Vec2) -> f64) -> Planform {
    if base.len() < 2 {
        return Planform { p: base.to_vec(), t: vec![0.0; base.len()],
                          off: vec![0.0; base.len()], n_bends: 0, n_capped: 0 };
    }
    let ds = prm.sample_m.max(0.25);
    // station on the ORIGINAL base, before anything moves
    let t0 = {
        let c = cum_arc(base);
        let l = c.last().copied().unwrap_or(1.0).max(1e-9);
        c.iter().map(|s| s / l).collect::<Vec<f64>>()
    };
    let sm = smooth_line(base, prm.smooth_passes);
    let zero = vec![0.0; sm.len()];
    let (b, tb, _) = resample2(&sm, &t0, &zero, ds);
    let n = b.len();
    let cum = cum_arc(&b);
    let l = *cum.last().unwrap();
    if n < 4 || l < 2.0 * ds {
        return Planform { p: b, t: tb, off: vec![0.0; n], n_bends: 0, n_capped: 0 };
    }
    // left normal from a windowed tangent (+-8 m), so it rotates smoothly
    let win = ((8.0 / ds).ceil() as usize).max(1);
    let perp: Vec<Vec2> = (0..n).map(|i| {
        let lo = i.saturating_sub(win);
        let hi = (i + win).min(n - 1);
        let tv = Vec2::new(b[hi].x - b[lo].x, b[hi].y - b[lo].y);
        let tl = tv.length().max(1e-9);
        Vec2::new(-tv.y / tl, tv.x / tl)
    }).collect();
    let at = |s: f64| -> usize { ((s / ds).round() as usize).min(n - 1) };

    // --- the element sequence ---------------------------------------------
    let mut els: Vec<Element> = Vec::new();
    let mut s0 = 0.0f64;
    let mut k: u32 = 0;
    let mut last_sign = if hash01(salt, 0, 99) < 0.5 { 1.0 } else { -1.0 };
    let mut last_straight = false;
    let (mut n_bends, mut n_capped) = (0usize, 0usize);
    let len_w = (prm.width_m / 5.0).clamp(0.8, 2.0);
    while s0 < l {
        let straight = !last_straight && hash01(salt, k, 0) < prm.straightness;
        if straight {
            let len = (STRAIGHT_LEN_M * math::exp(STRAIGHT_LEN_SIGMA * gauss(salt, k, 1)))
                .clamp(STRAIGHT_LEN_CLAMP.0, STRAIGHT_LEN_CLAMP.1);
            els.push(Element { s0, len, amp: 0.0, tau_s: 0.5 });
            s0 += len;
            last_straight = true;
            k += 1;
            continue;
        }
        let lb = (BEND_LEN_M * prm.len_mult * len_w
                  * math::exp(prm.len_sigma * gauss(salt, k, 3)))
            .clamp(BEND_LEN_CLAMP.0, BEND_LEN_CLAMP.1);
        let mut sign = -last_sign;
        if hash01(salt, k, 5) < prm.same_sign_p {
            sign = last_sign;
        }
        let s_mid = s0 + 0.5 * lb;
        let env = 1.0
            + ENV[0].0 * noise::perlin1(s_mid / ENV[0].1, salt ^ 0x51ED_270B)
            + ENV[1].0 * noise::perlin1(s_mid / ENV[1].1, salt ^ 0x2C1B_3E77);
        let ratio = (prm.amp_ratio * prm.vigour * math::exp(AMP_SIGMA * gauss(salt, k, 6)))
            .clamp(AMP_RATIO_CLAMP.0, AMP_RATIO_CLAMP.1);
        let mut amp = ratio * lb * env.max(0.2);
        let tau_a = (SKEW_MEAN + SKEW_SIGMA * gauss(salt, k, 8))
            .clamp(SKEW_CLAMP.0, SKEW_CLAMP.1);
        let tau_s = match flow {
            Flow::HeadFirst => tau_a,
            Flow::MouthFirst => 1.0 - tau_a,
        };
        // curvature cap at the apex: |o''| = A pi^2 w'^2 / Lb^2 <= 1/r_min
        let wp = bump_slope_at_apex(tau_s);
        let a_curv = lb * lb / (std::f64::consts::PI.powi(2) * prm.r_min_m * wp * wp);
        amp = amp.min(a_curv);
        // room cap, evaluated ON the base line on the bend's side
        let mut room = f64::INFINITY;
        for tau in [0.25, tau_s, 0.75] {
            let i = at(s0 + tau * lb);
            room = room.min(room_at(b[i]));
        }
        amp = amp.min(ROOM_FRAC * room.max(0.0));
        // confirmation: the bump itself must sit inside the floor
        for _ in 0..6 {
            let mut ok = true;
            for tau in [0.1, 0.3, tau_s, 0.7, 0.9] {
                let i = at(s0 + tau * lb);
                let o = sign * amp * bump(tau, tau_s);
                let q = Vec2::new(b[i].x + perp[i].x * o, b[i].y + perp[i].y * o);
                if room_at(q) < 0.5 {
                    ok = false;
                    break;
                }
            }
            if ok {
                break;
            }
            amp *= 0.85;
        }
        n_bends += 1;
        if amp < MIN_AMP_M {
            // pinched: a straight of the same length, not a micro-bend
            n_capped += 1;
            els.push(Element { s0, len: lb, amp: 0.0, tau_s: 0.5 });
            last_straight = true;
        } else {
            els.push(Element { s0, len: lb, amp: sign * amp, tau_s });
            last_sign = sign;
            last_straight = false;
        }
        s0 += lb;
        k += 1;
    }

    // --- the offset series ---------------------------------------------------
    let mut off = vec![0.0f64; n];
    let mut e = 0usize;
    for i in 0..n {
        let s = cum[i];
        while e + 1 < els.len() && s >= els[e].s0 + els[e].len {
            e += 1;
        }
        let el = els[e];
        if el.amp != 0.0 {
            let tau = ((s - el.s0) / el.len).clamp(0.0, 1.0);
            off[i] = el.amp * bump(tau, el.tau_s);
        }
    }
    // G1 joins: a short box, twice
    let half = ((JOIN_SMOOTH_M / ds).round() as usize).max(1);
    for _ in 0..2 {
        let src = off.clone();
        for i in 0..n {
            let lo = i.saturating_sub(half);
            let hi = (i + half + 1).min(n);
            off[i] = src[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
        }
    }
    // micro-wobble and end taper
    for i in 0..n {
        let s = cum[i];
        let w = prm.wobble_m
            * (noise::perlin1(s / WOBBLE_L.0, salt ^ 0xA5A5_1234)
               + 0.7 * noise::perlin1(s / WOBBLE_L.1, salt ^ 0x5A5A_4321));
        let taper = math::smoothstep(0.0, TAPER_M, s) * math::smoothstep(0.0, TAPER_M, l - s);
        off[i] = (off[i] + w) * taper;
    }

    // --- position, fold guard, resample along the creek's own arc ----------
    let mut p: Vec<Vec2> = (0..n)
        .map(|i| Vec2::new(b[i].x + perp[i].x * off[i], b[i].y + perp[i].y * off[i]))
        .collect();
    let span = ((12.0 / ds).round() as usize).max(2);
    limit_curvature(&mut p, 0.85 * prm.r_min_m, span);
    let (p, t, off) = resample2(&p, &tb, &off, ds);
    Planform { p, t, off, n_bends, n_capped }
}

/// Interpolate per-base-node `values` at the planform's stations. With a
/// non-decreasing `t` and values that rise with the index (a mouth-first
/// bed), the result rises with the index too.
pub fn rekey(values: &[f64], base: &[Vec2], t: &[f64]) -> Vec<f64> {
    if values.len() != base.len() || values.is_empty() {
        return vec![values.first().copied().unwrap_or(0.0); t.len()];
    }
    let c = cum_arc(base);
    let l = c.last().copied().unwrap_or(1.0).max(1e-9);
    let st: Vec<f64> = c.iter().map(|s| s / l).collect();
    t.iter().map(|&tt| {
        let j = st.partition_point(|&s| s <= tt).min(st.len() - 1);
        if j == 0 {
            return values[0];
        }
        let (a, b) = (st[j - 1], st[j]);
        let f = if b - a > 1e-12 { ((tt - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 };
        values[j - 1] + (values[j] - values[j - 1]) * f
    }).collect()
}

// --- measurement (Rust port of the sheet's 10 m / 30 m pipeline) -------------
//
// The Python instruments in `tools/aeolian/creek_measure.py` are authoritative;
// this port exists so the tests can check bands without Python. Same steps:
// resample at 10 m, box-smooth over 3 samples, split at curvature-sign
// inflections, keep bends 25..1500 m.

/// Resample at `step` and box-smooth over `smooth/step` samples.
pub fn measured_line(p: &[Vec2], step: f64, smooth: f64) -> Vec<Vec2> {
    let c = cum_arc(p);
    let l = *c.last().unwrap_or(&0.0);
    let mut q: Vec<Vec2> = Vec::new();
    let mut s = 0.0;
    let mut j = 0usize;
    while s < l {
        while j + 1 < c.len() && c[j + 1] < s {
            j += 1;
        }
        let f = if c[j + 1] - c[j] > 1e-12 { (s - c[j]) / (c[j + 1] - c[j]) } else { 0.0 };
        q.push(p[j].lerp(p[j + 1], f));
        s += step;
    }
    let k = ((smooth / step) as usize).max(1);
    if k > 1 && q.len() >= k {
        let mut out = Vec::with_capacity(q.len() - k + 1);
        for i in 0..=q.len() - k {
            let mut acc = Vec2::new(0.0, 0.0);
            for m in 0..k {
                acc = acc + q[i + m];
            }
            out.push(acc * (1.0 / k as f64));
        }
        q = out;
    }
    q
}

/// Signed curvature at each interior node of an already-measured line.
pub fn signed_curvature(q: &[Vec2]) -> Vec<f64> {
    let n = q.len();
    let mut c = vec![0.0; n];
    for i in 1..n.saturating_sub(1) {
        let v1 = q[i] - q[i - 1];
        let v2 = q[i + 1] - q[i];
        let cr = v1.x * v2.y - v1.y * v2.x;
        let den = v1.length() * v2.length() * (v1 + v2).length();
        if den > 1e-9 {
            c[i] = 2.0 * cr / den;
        }
    }
    c
}

/// Bend arcs (between successive curvature inflections, 25..1500 m) and the
/// apex fraction of each, on the measured line.
pub fn bends(q: &[Vec2]) -> Vec<(f64, f64)> {
    let c = signed_curvature(q);
    let cum = cum_arc(q);
    let mut idx: Vec<usize> = Vec::new();
    for i in 1..c.len().saturating_sub(1) {
        if (c[i] > 0.0) != (c[i + 1] > 0.0) && c[i] != 0.0 && c[i + 1] != 0.0 {
            idx.push(i);
        }
    }
    let mut out = Vec::new();
    for w in idx.windows(2) {
        let (a, b) = (w[0], w[1]);
        if b - a < 3 {
            continue;
        }
        let arc = cum[b] - cum[a];
        if !(25.0 < arc && arc < 1500.0) {
            continue;
        }
        let mut pk = a;
        for i in a..b {
            if c[i].abs() > c[pk].abs() {
                pk = i;
            }
        }
        out.push((arc, (cum[pk] - cum[a]) / arc));
    }
    out
}

/// Fraction of measured length with curvature radius above `r_min`.
pub fn straight_fraction(q: &[Vec2], r_min: f64) -> f64 {
    let c = signed_curvature(q);
    let n = q.len();
    if n < 3 {
        return 1.0;
    }
    let mut flat = 0.0;
    let mut tot = 0.0;
    for i in 1..n - 1 {
        let ds = q[i].distance(q[i + 1]);
        tot += ds;
        if c[i].abs() < 1.0 / r_min {
            flat += ds;
        }
    }
    if tot > 0.0 { flat / tot } else { 1.0 }
}

pub fn sinuosity(p: &[Vec2]) -> f64 {
    let l = *cum_arc(p).last().unwrap_or(&0.0);
    let chord = p[0].distance(*p.last().unwrap()).max(1.0);
    l / chord
}

#[cfg(test)]
mod tests {
    use super::*;

    fn straight_axis(len: f64, ds: f64) -> Vec<Vec2> {
        let n = (len / ds) as usize + 1;
        (0..n).map(|i| Vec2::new(i as f64 * ds, 0.0)).collect()
    }

    fn valley_axis(len: f64, ds: f64) -> Vec<Vec2> {
        let n = (len / ds) as usize + 1;
        (0..n).map(|i| {
            let s = i as f64 * ds;
            Vec2::new(s, 60.0 * math::sin(s / 900.0) + 25.0 * math::sin(s / 380.0))
        }).collect()
    }

    fn prm() -> Params {
        Params::from_draws(400.0, 1.0, 2.0, 5.0)
    }

    fn median(v: &mut Vec<f64>) -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    }

    #[test]
    fn deterministic_bitwise() {
        let base = valley_axis(2000.0, 6.0);
        let a = bend_train(&base, Flow::HeadFirst, &prm(), 7, &|_| 50.0);
        let b = bend_train(&base, Flow::HeadFirst, &prm(), 7, &|_| 50.0);
        assert_eq!(a.p.len(), b.p.len());
        for i in 0..a.p.len() {
            assert_eq!(a.p[i].x, b.p[i].x);
            assert_eq!(a.p[i].y, b.p[i].y);
            assert_eq!(a.t[i], b.t[i]);
            assert_eq!(a.off[i], b.off[i]);
        }
        assert_eq!(DRAWS, 0);
    }

    #[test]
    fn spacing_holds_and_t_monotone() {
        let base = valley_axis(2500.0, 6.0);
        for salt in [1u32, 2, 3] {
            let pf = bend_train(&base, Flow::MouthFirst, &prm(), salt, &|_| 50.0);
            assert!(pf.p.len() > 1000);
            for w in pf.p.windows(2) {
                assert!(w[0].distance(w[1]) <= 1.5 * prm().sample_m + 1e-9);
            }
            for w in pf.t.windows(2) {
                assert!(w[1] >= w[0] - 1e-12, "t must be non-decreasing");
            }
            // a rising bed stays rising through rekey
            let bed: Vec<f64> = (0..base.len()).map(|i| 10.0 + 0.01 * i as f64).collect();
            let z = rekey(&bed, &base, &pf.t);
            for w in z.windows(2) {
                assert!(w[1] >= w[0] - 1e-12);
            }
            assert!((z[0] - 10.0).abs() < 1e-9);
            assert!((z[z.len() - 1] - bed[bed.len() - 1]).abs() < 1e-6);
        }
    }

    #[test]
    fn no_self_intersection() {
        let base = valley_axis(2500.0, 6.0);
        for salt in [11u32, 12, 13] {
            let pf = bend_train(&base, Flow::HeadFirst, &prm(), salt, &|_| 50.0);
            // coarse check: 4 m segments, O(n^2) on ~600 segments
            let q: Vec<Vec2> = pf.p.iter().step_by(4).copied().collect();
            for i in 0..q.len() - 1 {
                for j in i + 2..q.len() - 1 {
                    let (a, b, c, d) = (q[i], q[i + 1], q[j], q[j + 1]);
                    let cr = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
                    let (d1, d2, d3, d4) = (cr(a, b, c), cr(a, b, d), cr(c, d, a), cr(c, d, b));
                    assert!(!((d1 * d2 < 0.0) && (d3 * d4 < 0.0)),
                            "self-intersection at segments {i}/{j} salt {salt}");
                }
            }
        }
    }

    #[test]
    fn stays_inside_room() {
        // pinch corridor: 50 m room, 12 m over 600-800, 8 m over 1500-1560
        let base = straight_axis(3000.0, 6.0);
        let room = |q: Vec2| -> f64 {
            let w = if (600.0..800.0).contains(&q.x) { 12.0 }
                    else if (1500.0..1560.0).contains(&q.x) { 8.0 } else { 50.0 };
            w - q.y.abs()
        };
        let pf = bend_train(&base, Flow::HeadFirst, &prm(), 5, &room);
        for p in &pf.p {
            assert!(room(*p) > -0.6, "node outside the floor at {:?}", p);
        }
        assert!(pf.n_bends > 20);
    }

    #[test]
    fn curvature_floor() {
        let base = valley_axis(2500.0, 6.0);
        let pr = prm();
        let pf = bend_train(&base, Flow::HeadFirst, &pr, 21, &|_| 50.0);
        let span = 12usize;
        let mut worst = f64::INFINITY;
        for i in span..pf.p.len() - span {
            let (a, b, c) = (pf.p[i - span], pf.p[i], pf.p[i + span]);
            let (ab, bc, ca) = (a.distance(b), b.distance(c), c.distance(a));
            let area2 = ((b.x - a.x) * (c.y - a.y) - (b.y - a.y) * (c.x - a.x)).abs();
            if area2 < 1e-9 { continue; }
            worst = worst.min(ab * bc * ca / (2.0 * area2));
        }
        assert!(worst >= 0.75 * pr.r_min_m, "min radius {worst:.1} m");
    }

    #[test]
    fn bend_stats_in_band() {
        let base = straight_axis(3000.0, 6.0);
        let (mut lens, mut cvs, mut strs, mut sins) = (vec![], vec![], vec![], vec![]);
        for salt in 1..=5u32 {
            let pf = bend_train(&base, Flow::HeadFirst, &prm(), salt, &|_| 60.0);
            let q = measured_line(&pf.p, 10.0, 30.0);
            let bs = bends(&q);
            assert!(bs.len() >= 10, "salt {salt}: {} bends", bs.len());
            let mut l: Vec<f64> = bs.iter().map(|b| b.0).collect();
            let mean = l.iter().sum::<f64>() / l.len() as f64;
            let var = l.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / l.len() as f64;
            cvs.push(var.sqrt() / mean);
            lens.push(median(&mut l));
            strs.push(straight_fraction(&q, 150.0));
            sins.push(sinuosity(&q));
        }
        let (ml, mc, ms, msn) = (median(&mut lens), median(&mut cvs), median(&mut strs), median(&mut sins));
        eprintln!("bend len p50 {ml:.1}  CV {mc:.2}  straight%R150 {:.1}  sinuosity {msn:.3}", ms * 100.0);
        assert!((35.0..=80.0).contains(&ml), "bend length {ml}");
        assert!((0.30..=0.65).contains(&mc), "CV {mc}");
        assert!((0.45..=0.80).contains(&ms), "straight fraction {ms}");
    }

    #[test]
    fn skew_leans_downstream() {
        let base = straight_axis(3000.0, 6.0);
        let mut head = vec![];
        let mut mouth = vec![];
        for salt in 1..=5u32 {
            let pf = bend_train(&base, Flow::HeadFirst, &prm(), salt, &|_| 60.0);
            for (_, sk) in bends(&measured_line(&pf.p, 10.0, 30.0)) { head.push(sk); }
            // mouth-first: reverse to flow order before measuring, like OSM
            let pf = bend_train(&base, Flow::MouthFirst, &prm(), salt, &|_| 60.0);
            let mut rev = pf.p.clone();
            rev.reverse();
            for (_, sk) in bends(&measured_line(&rev, 10.0, 30.0)) { mouth.push(sk); }
        }
        let (h, m) = (median(&mut head), median(&mut mouth));
        eprintln!("skew p50 head-first {h:.3}  mouth-first (reversed) {m:.3}");
        assert!((0.52..=0.70).contains(&h), "head-first skew {h}");
        assert!((0.52..=0.70).contains(&m), "mouth-first skew {m}");
    }

    fn autocorr_peak(o: &[f64], ds: f64) -> f64 {
        let n = o.len();
        let mean = o.iter().sum::<f64>() / n as f64;
        let v: Vec<f64> = o.iter().map(|x| x - mean).collect();
        let var = v.iter().map(|x| x * x).sum::<f64>();
        // Any smooth signal correlates with itself at lags shorter than its
        // features (50 m bends read ~0.7 at 20 m). The carrier shows up
        // PAST the first zero-crossing: the sine peaks again at lambda, a
        // random train never recovers.
        let hi = ((600.0 / ds) as usize).min(n / 2);
        let mut best = 0.0f64;
        let mut crossed = false;
        for lag in 1..hi {
            let mut acc = 0.0;
            for i in 0..n - lag {
                acc += v[i] * v[i + lag];
            }
            let r = acc / var;
            if !crossed {
                if r <= 0.0 { crossed = true; }
                continue;
            }
            best = best.max(r);
        }
        best
    }

    #[test]
    fn aperiodic_offset() {
        let base = straight_axis(3000.0, 6.0);
        let pf = bend_train(&base, Flow::HeadFirst, &prm(), 9, &|_| 60.0);
        let ours = autocorr_peak(&pf.off, prm().sample_m);
        // the negative test: the old two-sine on the same arc
        let sine: Vec<f64> = (0..3000).map(|i| {
            let s = i as f64;
            math::sin(std::f64::consts::TAU * s / 380.0)
                + 0.35 * math::sin(std::f64::consts::TAU * s / (380.0 * 2.7))
        }).collect();
        let theirs = autocorr_peak(&sine, 1.0);
        eprintln!("autocorr peak ours {ours:.2}  sine {theirs:.2}");
        assert!(theirs > 0.6, "negative test: the sine must show a carrier, got {theirs:.2}");
        assert!(ours < 0.45, "offset has a carrier: peak {ours:.2}");
    }

    #[test]
    fn from_draws_maps_ranges() {
        let lo = Params::from_draws(260.0, 0.40, 0.0, 5.0);
        let hi = Params::from_draws(700.0, 1.70, std::f64::consts::TAU, 5.0);
        assert!((lo.len_mult - 0.80).abs() < 1e-9 && (hi.len_mult - 1.25).abs() < 1e-9);
        assert!((lo.vigour - 0.70).abs() < 1e-9 && (hi.vigour - 1.40).abs() < 1e-9);
        assert!((lo.straightness - 0.35).abs() < 1e-9 && (hi.straightness - 0.60).abs() < 1e-9);
        let out = Params::from_draws(100.0, 5.0, 100.0, 5.0);
        assert!(out.len_mult == 0.80 && out.vigour == 1.40 && out.straightness == 0.60);
        assert!((Params::from_draws(400.0, 1.0, 1.0, 10.0).r_min_m - 30.0).abs() < 1e-9);
        assert!((Params::from_draws(400.0, 1.0, 1.0, 5.0).r_min_m - 25.0).abs() < 1e-9);
    }
}
