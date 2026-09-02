//! Step-1 planform sheet: the bend-train creek as labelled polylines, with
//! NO terrain -- synthetic corridors plus the trunk corridors of real fluvial
//! meander seeds (8 m macro only, no texture build). Rendered and measured by
//! `tools/aeolian/creek_sheet.py`.
//!
//!   cargo run --release -p course-sandhills --example planform_sheet -- <out.txt> [seed ...]
//!
//! Text format (what `creek_skew.py` already reads): a `NAME` line, then one
//! `x y` per line. `META name k=v ...` lines carry per-polyline numbers the
//! Python cannot compute (how many bends the room cap pinched to straights).
//! Every polyline is emitted HEAD-FIRST (flow order) so skew is measured the
//! way the corpus is.
use course_sandhills::planform::{self, Flow, Params};
use course_sandhills::{assemble::HandProfile, build_fluvial_macro_beds, rng, water::RiverStyle};
use course_world::math::{self, Vec2};
use std::fmt::Write as _;

fn emit(out: &mut String, name: &str, pts: &[Vec2]) {
    writeln!(out, "{name}").unwrap();
    for p in pts {
        writeln!(out, "{:.3} {:.3}", p.x, p.y).unwrap();
    }
}

fn axis_straight(len: f64) -> Vec<Vec2> {
    (0..=(len / 6.0) as usize).map(|i| Vec2::new(i as f64 * 6.0, 0.0)).collect()
}

fn axis_valley(len: f64) -> Vec<Vec2> {
    (0..=(len / 6.0) as usize).map(|i| {
        let s = i as f64 * 6.0;
        Vec2::new(s, 60.0 * math::sin(s / 900.0) + 25.0 * math::sin(s / 380.0))
    }).collect()
}

/// The shipped two-sine offset (water.rs:1431-1497 @ 3839dcb), with its
/// u-clamp expressed through `room_at` (`u < 0.34` <=> `room > 0`).
/// ABLATION ONLY -- deleted after sign-off.
fn sine_legacy(base: &[Vec2], m_lam: f64, m_swing: f64, m_phase: f64, m_seed: u32,
               room_at: &dyn Fn(Vec2) -> f64) -> Vec<Vec2> {
    let mut pos: Vec<(Vec2, Vec2, f64)> = Vec::new();
    let mut fac: Vec<f64> = Vec::new();
    let mut arc = 0.0;
    for k in 0..base.len().saturating_sub(1) {
        let (a, b) = (base[k], base[k + 1]);
        let seg = a.distance(b);
        if seg <= 1e-6 { continue; }
        let tang = Vec2::new(b.x - a.x, b.y - a.y).normalized();
        let perp = Vec2::new(-tang.y, tang.x);
        let n = (seg / 1.0).ceil().max(1.0) as usize;
        for j in 0..n {
            let t = j as f64 / n as f64;
            let p = Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            let s_here = arc + seg * t;
            let lam_e = m_lam * (1.0 + 0.35 * course_world::noise::perlin1(s_here / 640.0, m_seed));
            let off = m_swing * 26.0
                * (math::sin(std::f64::consts::TAU * s_here / lam_e + m_phase)
                   + 0.35 * math::sin(std::f64::consts::TAU * s_here / (lam_e * 2.7) + m_phase * 1.7));
            let mut f = 0.0;
            for step in 0..=8 {
                let cand = 1.0 - step as f64 / 8.0;
                let q = Vec2::new(p.x + perp.x * off * cand, p.y + perp.y * off * cand);
                if room_at(q) > 0.0 { f = cand; break; }
            }
            pos.push((p, perp, off));
            fac.push(f);
        }
        arc += seg;
    }
    let src = fac.clone();
    for i in 0..fac.len() {
        let lo = i.saturating_sub(12);
        let hi = (i + 13).min(src.len());
        fac[i] = src[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
    }
    pos.iter().zip(fac.iter())
        .map(|((p, perp, off), f)| Vec2::new(p.x + perp.x * off * f, p.y + perp.y * off * f))
        .collect()
}

/// Trace the floor edge either side of a base line by marching along the
/// normal until the room runs out.
fn edges(base: &[Vec2], room_at: &dyn Fn(Vec2) -> f64) -> (Vec<Vec2>, Vec<Vec2>) {
    let (mut l, mut r) = (Vec::new(), Vec::new());
    let n = base.len();
    for i in (0..n).step_by(2) {
        let lo = i.saturating_sub(2);
        let hi = (i + 2).min(n - 1);
        let tv = Vec2::new(base[hi].x - base[lo].x, base[hi].y - base[lo].y);
        let tl = tv.length().max(1e-9);
        let perp = Vec2::new(-tv.y / tl, tv.x / tl);
        for (sign, v) in [(1.0, &mut l), (-1.0, &mut r)] {
            let mut d = 0.0;
            let mut q = base[i];
            while d < 250.0 {
                let c = Vec2::new(base[i].x + perp.x * sign * d, base[i].y + perp.y * sign * d);
                if room_at(c) <= 0.0 { break; }
                q = c;
                d += 1.0;
            }
            v.push(q);
        }
    }
    (l, r)
}

fn synthetic(out: &mut String) {
    let mid = Params::from_draws(400.0, 1.0, std::f64::consts::PI, 5.0);
    let corridors: Vec<(&str, Vec<Vec2>, Box<dyn Fn(Vec2) -> f64>)> = vec![
        ("straight", axis_straight(3000.0), Box::new(|q: Vec2| 50.0 - q.y.abs())),
        ("pinch", axis_straight(3000.0), Box::new(|q: Vec2| {
            let w = if (600.0..800.0).contains(&q.x) { 12.0 }
                    else if (1500.0..1560.0).contains(&q.x) { 8.0 } else { 50.0 };
            w - q.y.abs()
        })),
        ("wide", axis_straight(3000.0), Box::new(|q: Vec2| {
            (50.0 + 70.0 * (q.x / 3000.0).clamp(0.0, 1.0)) - q.y.abs()
        })),
        ("valley", axis_valley(3000.0), Box::new(|q: Vec2| {
            let s = q.x;
            let ay = 60.0 * math::sin(s / 900.0) + 25.0 * math::sin(s / 380.0);
            45.0 - (q.y - ay).abs()
        })),
    ];
    for (name, axis, room) in &corridors {
        let (el, er) = edges(axis, room.as_ref());
        emit(out, &format!("CORR_{name}_AXIS"), axis);
        emit(out, &format!("CORR_{name}_EDGE_L"), &el);
        emit(out, &format!("CORR_{name}_EDGE_R"), &er);
        for salt in [0xBEEFu32, 0x1234, 0x77AA] {
            let pf = planform::bend_train(axis, Flow::HeadFirst, &mid, salt, room.as_ref());
            let tag = format!("OURS_{name}_s{salt:x}");
            writeln!(out, "META {tag} bends={} capped={} r_min={:.0} wobble={:.2}",
                     pf.n_bends, pf.n_capped, mid.r_min_m, mid.wobble_m).unwrap();
            emit(out, &tag, &pf.p);
        }
        // the shipped sine on the same corridor
        let sine = sine_legacy(axis, 380.0, 1.2, 0.0, 0xBEEF, room.as_ref());
        emit(out, &format!("SINE_{name}"), &sine);
    }
    // dial ablations on the straight corridor
    // "hier" is now the default (planform.rs constants); kept as an explicit
    // rung so a later default change still has this reference on the sheet
    let hier = Params { len_sigma: 0.70, amp_ratio: 0.14, r_min_m: 25.0, ..mid.clone() };
    let axis = axis_straight(3000.0);
    let room = |q: Vec2| 50.0 - q.y.abs();
    for (tag, p) in [
        ("rmin60", Params { r_min_m: 60.0, ..mid.clone() }),
        ("rmin35", Params { r_min_m: 35.0, ..mid.clone() }),
        ("thin_tail", Params { len_sigma: 0.45, amp_ratio: 0.09, r_min_m: 35.0, ..mid.clone() }),
        ("nowobble", Params { wobble_m: 0.0, ..mid.clone() }),
        ("lazy", Params::from_draws(700.0, 0.40, 0.0, 5.0)),
        ("loops", Params::from_draws(283.0, 1.70, std::f64::consts::TAU, 5.0)),
        ("gorge10", Params::from_draws(400.0, 1.0, std::f64::consts::PI, 10.0)),
        // the bold end: real 50 m bends at sinuosity 1.12 need apex radii of
        // ~25 m, so the curvature floor is the dial that lets amplitude out
        ("bold_r22", Params { r_min_m: 22.0, vigour: 1.4, ..mid.clone() }),
        ("bold_r18", Params { r_min_m: 18.0, vigour: 1.4, ..mid.clone() }),
        ("bold_r22_straight", Params { r_min_m: 22.0, vigour: 1.4, straightness: 0.60, ..mid.clone() }),
        // hierarchy: a heavier bend-length tail puts 100-250 m bends in the
        // train, and those carry the 20-40 m swings the real panels show
        ("hier", hier.clone()),
        ("hier_bold", Params { amp_ratio: 0.18, r_min_m: 22.0, ..hier.clone() }),
        ("hier_calm", Params { amp_ratio: 0.11, ..hier.clone() }),
    ] {
        let pf = planform::bend_train(&axis, Flow::HeadFirst, &p, 0xBEEF, &room);
        let name = format!("DIAL_{tag}_sbeef");
        writeln!(out, "META {name} bends={} capped={} r_min={:.0} wobble={:.2} len_mult={:.2} vigour={:.2} straightness={:.2}",
                 pf.n_bends, pf.n_capped, p.r_min_m, p.wobble_m, p.len_mult, p.vigour, p.straightness).unwrap();
        emit(out, &name, &pf.p);
    }
}

fn real_seed(out: &mut String, seed: u64, prof: &HandProfile) {
    let id = course_seed::RunIdentity::from_seed(seed);
    let (d, net, asm, beds) = build_fluvial_macro_beds(&id, prof);
    let Some(ti) = net.chans.iter().position(|c| c.tier == 1) else {
        eprintln!("seed {seed}: no trunk");
        return;
    };
    let (pts, _bed) = &beds[ti];
    // the WATER draws exactly as water::fluvial takes them
    let mut wr = rng::stream(&id, rng::WATER);
    let meander = wr.next_f64() < d.p_valley_creek;
    let mstyle = RiverStyle::PASSED[wr.below(RiverStyle::PASSED.len())];
    let m_lam = wr.range_f64(mstyle.lam.0, mstyle.lam.1);
    let m_swing = wr.range_f64(mstyle.swing.0, mstyle.swing.1);
    let m_phase = wr.range_f64(0.0, std::f64::consts::TAU);
    let m_seed = wr.next_u32();
    let hw = wr.range_f64(2.0, 2.5);
    if !meander {
        eprintln!("seed {seed}: not a meander seed (coin failed) -- drawing anyway");
    }
    let room = |q: Vec2| (0.34 - asm.u.bilinear(q)).max(0.0) * asm.w.bilinear(q);
    let prm = Params::from_draws(m_lam, m_swing, m_phase, 2.0 * hw);
    let pf = planform::bend_train(pts, Flow::MouthFirst, &prm, m_seed, &room);
    let sine = sine_legacy(pts, m_lam, m_swing, m_phase, m_seed, &room);
    let (el, er) = edges(pts, &room);
    // emit head-first
    let rev = |v: &[Vec2]| { let mut r = v.to_vec(); r.reverse(); r };
    emit(out, &format!("SEED_{seed}_AXIS"), &rev(pts));
    emit(out, &format!("SEED_{seed}_EDGE_L"), &rev(&el));
    emit(out, &format!("SEED_{seed}_EDGE_R"), &rev(&er));
    let tag = format!("SEED_{seed}_OURS");
    writeln!(out, "META {tag} bends={} capped={} r_min={:.0} wobble={:.2} len_mult={:.2} vigour={:.2} straightness={:.2} lam={:.0} swing={:.2} hw={:.2}",
             pf.n_bends, pf.n_capped, prm.r_min_m, prm.wobble_m, prm.len_mult, prm.vigour,
             prm.straightness, m_lam, m_swing, hw).unwrap();
    emit(out, &tag, &rev(&pf.p));
    let hier = Params { len_sigma: 0.45, amp_ratio: 0.09, r_min_m: 35.0, ..prm.clone() };
    let pfh = planform::bend_train(pts, Flow::MouthFirst, &hier, m_seed, &room);
    let tag = format!("SEED_{seed}_THIN");
    writeln!(out, "META {tag} bends={} capped={} r_min={:.0} len_sigma={:.2} amp_ratio={:.2}",
             pfh.n_bends, pfh.n_capped, hier.r_min_m, hier.len_sigma, hier.amp_ratio).unwrap();
    emit(out, &tag, &rev(&pfh.p));
    emit(out, &format!("SEED_{seed}_SINE"), &rev(&sine));
    eprintln!("seed {seed}: trunk {} pts, {} bends ({} capped), lam {m_lam:.0} swing {m_swing:.2}",
              pts.len(), pf.n_bends, pf.n_capped);
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut out = String::new();
    synthetic(&mut out);
    if a.len() > 1 {
        let prof = HandProfile::load(std::path::Path::new(
            "assets/sandhills_hand_profile.txt")).expect("hand profile");
        for s in &a[1..] {
            real_seed(&mut out, s.parse().unwrap(), &prof);
        }
    }
    std::fs::write(&a[0], out).unwrap();
}
