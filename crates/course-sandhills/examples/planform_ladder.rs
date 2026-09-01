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
    // (label, mig, reinject, erode, kernel_w)
    // Target from 55,281 real reaches: bend length 40/50/60 m, CV 0.38/0.46/0.55,
    // sinuosity 1.063/1.117/1.225.
    let rungs: [(&str, f64, usize, f64, f64); 9] = [
        ("shipped",        0.70, 20, 0.0, 12.0),
        ("erode_only",     0.70, 20, 1.0, 12.0),
        ("erode_hi",       0.70, 20, 1.6, 12.0),
        ("kernel_20",      0.70, 20, 1.0, 20.0),
        ("kernel_28",      0.70, 20, 1.0, 28.0),
        ("k20_erode_hi",   0.70, 20, 1.6, 20.0),
        ("k20_ero_mig9",   0.90, 20, 1.6, 20.0),
        ("k28_ero_mig9",   0.90, 20, 1.6, 28.0),
        ("k20_ero_r8",     0.90,  8, 1.6, 20.0),
    ];
    for (tag, mig, re, ero, ker) in rungs {
        let pf = migrate::grow_sweep(&axis, w, 1.0, 0xBEEF, 1.0, mig, re, ero, ker,
                                     |_| 0.0);
        let l: f64 = pf.p.windows(2).map(|q| q[0].distance(q[1])).sum();
        let chord = pf.p[0].distance(*pf.p.last().unwrap());
        eprintln!("{tag}: sinuosity {:.3}, {} oxbows", l / chord, pf.oxbows.len());
        txt.push_str(&format!("{tag}\n"));
        for p in &pf.p { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }
        for (i, ox) in pf.oxbows.iter().enumerate() {
            txt.push_str(&format!("{tag}_OX{i}\n"));
            for p in &ox.pts { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }
        }
    }
    std::fs::write(out, txt).unwrap();
}
