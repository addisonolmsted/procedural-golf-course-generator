//! Sweep the irregularity dials and emit planforms for a ladder viewer.
//!
//! The dials that control BEND-WAVELENGTH irregularity (as opposed to overall
//! sinuosity, which is MIG_W_PER_ITER's job):
//!   REINJECT  -- how often fresh symmetry-breaking is added. More injections
//!                mean bends of more mixed AGE, and age sets size.
//!   SPREAD    -- the breadth of the seeding octave stack. A broad seed lets
//!                the instability select locally rather than globally.
//!   DIFF      -- diffusivity. Less smoothing preserves more small-scale
//!                variety; too little lets a node-scale comb through.
use course_sandhills::migrate;
use course_world::math::{self, Vec2};

fn main() {
    let out = std::path::Path::new(&std::env::args().nth(1).unwrap()).to_path_buf();
    let w = 6.0_f64;
    let len = 1800.0_f64;
    let axis: Vec<Vec2> = (0..=((len / 3.0) as usize))
        .map(|i| {
            let s = i as f64 * 3.0;
            Vec2::new(s, 60.0 * math::sin(s / 900.0) + 25.0 * math::sin(s / 380.0))
        })
        .collect();
    let mut txt = String::new();
    txt.push_str("AXIS\n");
    for p in &axis { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }

    // (label, reinject_every, spread_mult, diff_mult)
    // Finer sweep around the measured target (real CV p25/50/75 =
    // 0.38/0.46/0.55, real sinuosity 1.063/1.117/1.225): the coarse ladder
    // put `shipped` (CV 0.26) below the band and `reinject_6` (0.66) above it.
    // Push growth until CUTOFFS fire. Zero oxbows on every on-terrain seed,
    // and cutoffs are a primary irregularity engine -- each one deletes a
    // bend and leaves mismatched neighbours. Target: bend length 40-60 m,
    // CV 0.38-0.55, sinuosity 1.06-1.23 (55,281 real reaches).
    // A CONFINED belt is included because unconfined bends migrate apart
    // instead of necking: u = |offset from axis| / half_belt.
    let belt = 90.0_f64;
    let confine = |q: Vec2| -> f64 {
        // distance to the axis, normalised by the half-belt
        let mut best = f64::INFINITY;
        for k in 0..axis.len() - 1 {
            let (a, b) = (axis[k], axis[k + 1]);
            let ab = Vec2::new(b.x - a.x, b.y - a.y);
            let t = (((q.x - a.x) * ab.x + (q.y - a.y) * ab.y)
                     / ab.dot(ab).max(1e-9)).clamp(0.0, 1.0);
            let pr = Vec2::new(a.x + ab.x * t, a.y + ab.y * t);
            best = best.min(q.distance(pr));
        }
        best / belt
    };
    // (label, mig, iters_mult, erode, confined)
    let rungs: [(&str, f64, f64, f64, bool); 9] = [
        ("now_free",       0.70, 1.0, 1.0, false),
        ("now_belt",       0.70, 1.0, 1.0, true),
        ("mig12_belt",     1.20, 1.0, 1.0, true),
        ("mig18_belt",     1.80, 1.0, 1.0, true),
        ("mig12_it2",      1.20, 2.0, 1.0, true),
        ("mig18_it2",      1.80, 2.0, 1.0, true),
        ("mig18_it3",      1.80, 3.0, 1.0, true),
        ("mig25_it2",      2.50, 2.0, 1.0, true),
        ("mig18_it3_ero2", 1.80, 3.0, 2.0, true),
    ];
    for (tag, mig, itm, ero, conf) in rungs {
        let pf = if conf {
            migrate::grow_sweep_iters(&axis, w, 0.85, 0xBEEF, 1.0, mig, 20, ero,
                                      12.0, itm, &confine)
        } else {
            migrate::grow_sweep_iters(&axis, w, 1.0, 0xBEEF, 1.0, mig, 20, ero,
                                      12.0, itm, &|_| 0.0)
        };
        let l: f64 = pf.p.windows(2).map(|q| q[0].distance(q[1])).sum();
        let chord = pf.p[0].distance(*pf.p.last().unwrap());
        eprintln!("{tag}: sinuosity {:.3}, {} oxbows, {} nodes",
                  l / chord, pf.oxbows.len(), pf.p.len());
        txt.push_str(&format!("{tag}\n"));
        for p in &pf.p { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }
        for (i, ox) in pf.oxbows.iter().enumerate() {
            txt.push_str(&format!("{tag}_OX{i}\n"));
            for p in &ox.pts { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }
        }
    }
    std::fs::write(out, txt).unwrap();
}
