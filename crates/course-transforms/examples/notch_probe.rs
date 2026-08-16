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
    let biome = match std::env::var("BIOME").as_deref() {
        Ok("piedmont") => BiomeId::Piedmont,
        Ok("heathland") => BiomeId::Heathland,
        Ok("hill_country") | Ok("hc") => BiomeId::HillCountry,
        Ok("great_plains") | Ok("plains") => BiomeId::GreatPlains,
        Ok("sandhills") => BiomeId::Sandhills,
        _ => BiomeId::RiverValley,
    };
    let seeds: Vec<u64> = if let Ok(s) = std::env::var("SEEDS") {
        s.split(',').filter_map(|v| v.trim().parse().ok()).collect()
    } else if std::env::var("ALL20").is_ok() {
        (1..=20).collect()
    } else {
        vec![3, 9, 17, 25, 31, 48]
    };
    for seed in seeds {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(
            id,
            &SpecOverridesV2 { forced_biome: Some(biome) },
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
        // ---- edge-hug attribution: S2 trunk line vs S4 water ----------
        let trunk_area = h
            .water
            .iter()
            .filter(|w| matches!(w.origin, hydrology::WaterPlaneOrigin::River))
            .count();
        let _ = trunk_area;
        let max_area = sk.channels.iter().map(|c| c.area_m2).fold(0.0f64, f64::max);
        let km2 = max_area / 1.0e6;
        let width_pers = hydrology::width_personality(id.course_scalar(hydrology::RIVER_WIDTH_SALT));
        let wscale = spec.dials.get("hydrology.channel_width_scale").copied().unwrap_or(1.0);
        let w_m = (8.5 * km2.sqrt() * width_pers * wscale)
            .clamp(hydrology::WIDTH_MIN_M, hydrology::WIDTH_MAX_M);
        let sb = id.course_scalar(hydrology::SWITCHBACK_SALT) < hydrology::SWITCHBACK_P;
        let clear = 2.0 * w_m;
        let spec8 = sk.flow_distance.spec;
        let n8 = spec8.nx as usize;
        let bd = |px_: f64, py_: f64| px_.min(py_).min(tile - px_).min(tile - py_);
        let s2_hug = sk.trunk_inlet.map(|inlet| {
            let mut cells = vec![inlet];
            let mut cur = inlet;
            let mut guard = 0;
            loop {
                let d = sk.flow_dir_rad.data[cur];
                if !d.is_finite() {
                    break;
                }
                let dx = libm::cos(d).round() as i64;
                let dy = libm::sin(d).round() as i64;
                let (y_, x_) = ((cur / n8) as i64, (cur % n8) as i64);
                let (nx2, ny2) = (x_ + dx, y_ + dy);
                if nx2 < 0 || ny2 < 0 || nx2 >= n8 as i64 || ny2 >= n8 as i64 {
                    break;
                }
                cur = ny2 as usize * n8 + nx2 as usize;
                cells.push(cur);
                guard += 1;
                if guard > n8 * 4 {
                    break;
                }
            }
            let total = cells.len();
            let term = (350.0 / spec8.cell_size) as usize;
            let mid = &cells[term.min(total)..total.saturating_sub(term)];
            if mid.is_empty() {
                return 0.0;
            }
            let hug = mid
                .iter()
                .filter(|&&i| {
                    let p = spec8.world_of((i % n8) as u32, (i / n8) as u32);
                    bd(p.x, p.y) < clear
                })
                .count();
            hug as f64 / mid.len() as f64
        });
        let base_edge = format!("{:?}", sk.meta.base_level.edge);
        println!(
            "seed {seed:2}: {planes:3} planes, shortfall W {w:6.1} E {e:6.1} S {s:6.1} N {n:6.1} | axis {axis} gap {worst_gap:5.1} m at {gap_at:6.1} | base {base_edge:2} w {w_m:4.1} m sb {sb:5} s2_trunk_hug<2w {:4.0}%",
            s2_hug.unwrap_or(f64::NAN) * 100.0
        );
        // CONNECTED COMPONENTS of all channel water (40 m dilation):
        // floating fragments near edges = extra components
        {
            let mut boxes: Vec<(f64, f64, f64, f64)> = Vec::new();
            for wb in &h.water {
                if !matches!(
                    wb.origin,
                    hydrology::WaterPlaneOrigin::River | hydrology::WaterPlaneOrigin::Creek
                ) {
                    continue;
                }
                let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                for p in &wb.polygon {
                    x0 = x0.min(p.x);
                    x1 = x1.max(p.x);
                    y0 = y0.min(p.y);
                    y1 = y1.max(p.y);
                }
                boxes.push((x0, x1, y0, y1));
            }
            let n = boxes.len();
            let mut parent: Vec<usize> = (0..n).collect();
            fn find(parent: &mut Vec<usize>, i: usize) -> usize {
                if parent[i] != i {
                    let r = find(parent, parent[i]);
                    parent[i] = r;
                }
                parent[i]
            }
            let d = 40.0;
            for i in 0..n {
                for j in (i + 1)..n {
                    let (a, b) = (boxes[i], boxes[j]);
                    if a.0 - d <= b.1 && b.0 - d <= a.1 && a.2 - d <= b.3 && b.2 - d <= a.3 {
                        let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                        parent[ri] = rj;
                    }
                }
            }
            use std::collections::HashMap;
            let mut comps: HashMap<usize, (usize, f64, f64)> = HashMap::new();
            for i in 0..n {
                let r = find(&mut parent, i);
                let e = comps.entry(r).or_insert((0, f64::MAX, f64::MAX));
                e.0 += 1;
                e.1 = e.1.min(boxes[i].0);
                e.2 = e.2.min(boxes[i].2);
            }
            if comps.len() > 1 {
                let mut sizes: Vec<(usize, f64, f64)> = comps.values().copied().collect();
                sizes.sort_by(|a, b| a.0.cmp(&b.0));
                println!(
                    "         !! {} water components; smallest {} planes near ({:.0},{:.0})",
                    comps.len(),
                    sizes[0].0,
                    sizes[0].1,
                    sizes[0].2
                );
            }
        }
        // creeks: same continuity check (dashed creeks on the piedmont
        // gallery). Caveat: two separate creek systems read as one big
        // "gap" between them — interpret jumps > ~200 m with a render.
        let mut civx: Vec<(f64, f64)> = Vec::new();
        let mut civy: Vec<(f64, f64)> = Vec::new();
        let mut cplanes = 0;
        for wb in &h.water {
            if !matches!(wb.origin, hydrology::WaterPlaneOrigin::Creek) {
                continue;
            }
            cplanes += 1;
            let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
            for p in &wb.polygon {
                x0 = x0.min(p.x);
                x1 = x1.max(p.x);
                y0 = y0.min(p.y);
                y1 = y1.max(p.y);
            }
            civx.push((x0, x1));
            civy.push((y0, y1));
        }
        if cplanes > 0 {
            let (csx, cgx, cpx) = gaps(civx);
            let (csy, cgy, cpy) = gaps(civy);
            let (caxis, cgap, cat, cspan) =
                if csx >= csy { ("x", cgx, cpx, csx) } else { ("y", cgy, cpy, csy) };
            println!(
                "         creeks: {cplanes:3} planes span {cspan:6.0} m | axis {caxis} worst gap {cgap:5.1} m at {cat:6.1}"
            );
        }
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
