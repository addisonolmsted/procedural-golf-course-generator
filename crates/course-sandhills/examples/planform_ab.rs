//! Emit OLD (sine offset) vs NEW (migration) planforms for visual comparison.
//! Planform only: no terrain, no confinement, no carve.
use course_sandhills::{meander, migrate};
use course_world::math::{self, Vec2};

fn main() {
    let out = std::path::Path::new(&std::env::args().nth(1).unwrap()).to_path_buf();
    let w = 6.0_f64;                       // channel width, m
    let len = 1800.0_f64;                  // reach length, m
    // A gently curving valley axis, so neither model is judged on a ruler.
    let axis: Vec<Vec2> = (0..=((len / 3.0) as usize))
        .map(|i| {
            let s = i as f64 * 3.0;
            Vec2::new(s, 60.0 * math::sin(s / 900.0) + 25.0 * math::sin(s / 380.0))
        })
        .collect();

    let mut txt = String::new();
    // axis
    txt.push_str("AXIS\n");
    for p in &axis { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }

    // OLD: the shipped sine offset (lam/swing from a mid RiverStyle draw)
    for (tag, lam, swing) in [("OLD_A", 380.0, 1.2), ("OLD_B", 283.0, 1.7)] {
        txt.push_str(&format!("{tag}\n"));
        let mut phi = 0.0f64;
        let mut arc = 0.0f64;
        for k in 0..axis.len() - 1 {
            let (a, b) = (axis[k], axis[k + 1]);
            let seg = a.distance(b);
            let lo = k.saturating_sub(4);
            let hi = (k + 4).min(axis.len() - 1);
            let tv = Vec2::new(axis[hi].x - axis[lo].x, axis[hi].y - axis[lo].y);
            let tl = tv.length().max(1e-9);
            let perp = Vec2::new(-tv.y / tl, tv.x / tl);
            let lam_e = lam * (1.0 + 0.35 * course_world::noise::perlin1(arc / 640.0, 7));
            let amp = (swing * 34.0 * meander::amp_mod(arc, 0x27f1))
                .min(0.125 * lam_e * meander::amp_cap_scale());
            let off = amp * meander::shape(phi);
            txt.push_str(&format!("{:.2} {:.2}\n", a.x + perp.x * off, a.y + perp.y * off));
            phi += std::f64::consts::TAU * seg / lam_e;
            arc += seg;
        }
    }

    // NEW: grown planforms at three vigours
    for (tag, vig, salt) in [("NEW_A", 0.8_f64, 0xBEEFu32),
                             ("NEW_B", 1.0, 0x1234),
                             ("NEW_C", 1.3, 0x77AA)] {
        let pf = migrate::grow(&axis, w, 1.0, salt, vig, |_| 0.0);
        let steps: f64 = pf.p.windows(2).map(|q| q[0].distance(q[1])).sum();
        let chord = pf.p[0].distance(*pf.p.last().unwrap());
        eprintln!("{tag}: {} nodes, sinuosity {:.3}, {} oxbows",
                  pf.p.len(), steps / chord, pf.oxbows.len());
        txt.push_str(&format!("{tag}\n"));
        for p in &pf.p { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }
        for (i, ox) in pf.oxbows.iter().enumerate() {
            txt.push_str(&format!("{tag}_OX{i}\n"));
            for p in &ox.pts { txt.push_str(&format!("{:.2} {:.2}\n", p.x, p.y)); }
        }
    }
    std::fs::write(out, txt).unwrap();
}
