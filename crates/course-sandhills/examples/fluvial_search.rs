//! Search seeds for the fluvial macro window closest to a target signature.
//!
//! Args: <out.tsv> <seed_lo> <seed_hi> <f0..f5 target>
//! Emits one line per seed: seed, best-window distance, then that window's
//! six features. The macro stage only (8 m) — texture cannot move any of
//! these statistics enough to matter, and it costs 40x more per tile.
use course_sandhills::{assemble::HandProfile, build_fluvial_macro};
use std::io::Write;

const WIN: usize = 174;          // 1392 m at 8 m, matching the atlas footprint
const STEP: usize = 22;

fn pct(v: &mut Vec<f64>, q: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() - 1) as f64 * q).round() as usize]
}

fn features(z: &[f64], nx: usize, oy: usize, ox: usize) -> [f64; 6] {
    let mut zs: Vec<f64> = Vec::with_capacity(WIN * WIN);
    let mut ss: Vec<f64> = Vec::with_capacity(WIN * WIN);
    let (mut lo, mut hi, mut sum) = (f64::MAX, f64::MIN, 0.0);
    let (mut n5, mut n12, mut n) = (0usize, 0usize, 0usize);
    for y in 0..WIN {
        for x in 0..WIN {
            let i = (oy + y) * nx + ox + x;
            let v = z[i];
            zs.push(v);
            lo = lo.min(v);
            hi = hi.max(v);
            sum += v;
            if y > 0 && y + 1 < WIN && x > 0 && x + 1 < WIN {
                let dzdx = (z[i + 1] - z[i - 1]) / 16.0;
                let dzdy = (z[i + nx] - z[i - nx]) / 16.0;
                let s = (dzdx * dzdx + dzdy * dzdy).sqrt() * 100.0;
                ss.push(s);
                if s < 5.0 { n5 += 1; }
                if s > 12.0 { n12 += 1; }
                n += 1;
            }
        }
    }
    let nf = n.max(1) as f64;
    [
        pct(&mut zs, 0.98) - pct(&mut zs, 0.02),
        pct(&mut ss, 0.50),
        pct(&mut ss, 0.90),
        n5 as f64 / nf,
        n12 as f64 / nf,
        (sum / (WIN * WIN) as f64 - lo) / (hi - lo).max(1e-9),
    ]
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let out = std::path::Path::new(&a[0]);
    let (lo, hi): (u64, u64) = (a[1].parse().unwrap(), a[2].parse().unwrap());
    // accept the six targets either as separate args or as one
    // space-separated string (zsh does not word-split unquoted expansions)
    let tgt: Vec<f64> = a[3..].iter()
        .flat_map(|s| s.split_whitespace())
        .filter_map(|v| v.parse().ok())
        .collect();
    assert_eq!(tgt.len(), 6, "need 6 target features, got {}", tgt.len());
    // normalisers: roughly the spread each feature shows across seeds, so no
    // single term dominates the distance
    let sc = [12.0, 2.5, 5.0, 0.18, 0.09, 0.06];
    let prof = HandProfile::load(std::path::Path::new(
        "assets/sandhills_hand_profile.txt")).expect("hand profile");
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    for seed in lo..hi {
        let id = course_seed::RunIdentity::from_seed(seed);
        let (_, _, asm) = build_fluvial_macro(&id, &prof);
        let nx = asm.height.spec.nx as usize;
        let ny = asm.height.spec.ny as usize;
        let z = &asm.height.data;
        let mut best = (f64::MAX, [0.0f64; 6]);
        let mut oy = 0;
        while oy + WIN <= ny {
            let mut ox = 0;
            while ox + WIN <= nx {
                let ft = features(z, nx, oy, ox);
                let d: f64 = (0..6).map(|k| {
                    let e = (ft[k] - tgt[k]) / sc[k];
                    e * e
                }).sum::<f64>().sqrt();
                if d < best.0 { best = (d, ft); }
                ox += STEP;
            }
            oy += STEP;
        }
        writeln!(f, "{seed}\t{:.4}\t{:.3}\t{:.3}\t{:.3}\t{:.4}\t{:.4}\t{:.4}",
                 best.0, best.1[0], best.1[1], best.1[2],
                 best.1[3], best.1[4], best.1[5]).unwrap();
    }
}
