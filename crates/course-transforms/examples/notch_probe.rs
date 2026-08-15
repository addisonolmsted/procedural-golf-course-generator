//! Rim-notch check: per rv seed, how close does River water get to
//! each tile border? A healthy edge-to-edge river ends within the
//! outer mask frame (~6 m); the seed-31 rim-notch bug left it ~50 m
//! short at the west edge.
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology;

fn main() {
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for seed in [3u64, 9, 17, 25, 31, 48] {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(
            id,
            &SpecOverridesV2 { forced_biome: Some(BiomeId::RiverValley) },
        );
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        let amp = course_amplify::generate(&spec, &sk, &dict, &id);
        let h = hydrology::generate(&spec, &sk, &amp, &id, &hydrology::DEFAULT_TRANSFORMS);
        let tile = h.height.spec.cell_size * h.height.spec.nx as f64;
        let (mut w, mut e, mut s, mut n) = (f64::MAX, f64::MAX, f64::MAX, f64::MAX);
        let mut planes = 0;
        // per-plane bbox intervals on both axes; the river's principal
        // axis is the one whose union span is larger
        let mut ivx: Vec<(f64, f64)> = Vec::new();
        let mut ivy: Vec<(f64, f64)> = Vec::new();
        for wb in &h.water {
            if !matches!(wb.origin, hydrology::WaterPlaneOrigin::River) {
                continue;
            }
            planes += 1;
            let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for p in &wb.polygon {
                w = w.min(p.x);
                e = e.min(tile - p.x);
                s = s.min(p.y);
                n = n.min(tile - p.y);
                x0 = x0.min(p.x);
                x1 = x1.max(p.x);
                y0 = y0.min(p.y);
                y1 = y1.max(p.y);
            }
            ivx.push((x0, x1));
            ivy.push((y0, y1));
        }
        let gaps = |mut iv: Vec<(f64, f64)>| -> (f64, f64, f64) {
            iv.sort_by(|a, b| a.0.total_cmp(&b.0));
            let lo = iv.first().map_or(0.0, |v| v.0);
            let (mut worst, mut wat, mut hi) = (0.0f64, 0.0f64, f64::MIN);
            for (a, b) in iv {
                if hi > f64::MIN && a > hi && a - hi > worst {
                    worst = a - hi;
                    wat = hi;
                }
                hi = hi.max(b);
            }
            (hi - lo, worst, wat)
        };
        let (sx, gx, px) = gaps(ivx);
        let (sy, gy, py) = gaps(ivy);
        let (axis, worst_gap, gap_at) =
            if sx >= sy { ("x", gx, px) } else { ("y", gy, py) };
        println!(
            "seed {seed:2}: {planes:3} planes, shortfall W {w:6.1} E {e:6.1} S {s:6.1} N {n:6.1} | axis {axis} worst mid-gap {worst_gap:5.1} m at {gap_at:6.1}"
        );
        // seed 31 west-end transect: terrain + plane surfaces vs x
        if seed == 31 {
            let mut ry = f64::NAN;
            let mut wx = f64::MAX;
            for wb in &h.water {
                if !matches!(wb.origin, hydrology::WaterPlaneOrigin::River) {
                    continue;
                }
                for p in &wb.polygon {
                    if p.x < wx {
                        wx = p.x;
                        ry = p.y;
                    }
                }
            }
            let n2 = h.height.spec.nx as usize;
            let c2 = h.height.spec.cell_size;
            let yr = (ry / c2) as usize;
            for xm in (0..320).step_by(16) {
                let xc = xm / c2 as usize;
                // min height across a 30-cell y-window (channel low point)
                let mut hmin = f64::MAX;
                for yy in yr.saturating_sub(30)..(yr + 30).min(n2) {
                    hmin = hmin.min(h.height.data[yy * n2 + xc]);
                }
                let surf = h
                    .water
                    .iter()
                    .filter(|wb| {
                        matches!(wb.origin, hydrology::WaterPlaneOrigin::River)
                            && wb.polygon.iter().any(|p| (p.x - xm as f64).abs() < 26.0)
                    })
                    .map(|wb| wb.surface_m)
                    .fold(f64::NAN, f64::max);
                println!("   x {xm:3} m: channel low {hmin:7.2}  plane surface {surf:7.2}");
            }
        }
    }
}
