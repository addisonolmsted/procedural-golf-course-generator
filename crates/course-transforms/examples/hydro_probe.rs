//! Diagnose the two reviewer concerns on the S4 output:
//! 1. stripes/pixelated pockets — compare pre/post-floodplain hillshade
//!    crops around the trunk (no contour overlay, pure shading);
//! 2. basin polygons — tint the actual basin COMPONENT cells over the
//!    hillshade next to their convex hulls, and print per-basin depth /
//!    compactness / edge-contact stats.

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology;
use course_world::grid::Grid;
use image::{Rgba, RgbaImage};

fn hillshade_px(h: &Grid<f64>, x: usize, y: usize) -> u8 {
    let n = h.spec.nx as usize;
    let c = h.spec.cell_size;
    let (x0, x1) = (x.saturating_sub(1), (x + 1).min(n - 1));
    let (y0, y1) = (y.saturating_sub(1), (y + 1).min(n - 1));
    let gx = (h.data[y * n + x1] - h.data[y * n + x0]) / ((x1 - x0).max(1) as f64 * c) * 1.6;
    let gy = (h.data[y1 * n + x] - h.data[y0 * n + x]) / ((y1 - y0).max(1) as f64 * c) * 1.6;
    let sl = (gx * gx + gy * gy).sqrt().atan();
    let asp = libm::atan2(-gx, gy);
    let (az, alt) = (315.0f64.to_radians(), 45.0f64.to_radians());
    let v = alt.sin() * sl.cos() + alt.cos() * sl.sin() * (az - asp).cos();
    (((v + 0.15) / 1.15).clamp(0.0, 1.0) * 255.0) as u8
}

fn save_crop(h: &Grid<f64>, cx: usize, cy: usize, half: usize, path: &str) {
    let n = h.spec.nx as usize;
    let (x0, y0) = (cx.saturating_sub(half), cy.saturating_sub(half));
    let (x1, y1) = ((cx + half).min(n - 1), (cy + half).min(n - 1));
    let mut img = RgbaImage::new((x1 - x0) as u32, (y1 - y0) as u32);
    for y in y0..y1 {
        for x in x0..x1 {
            let v = hillshade_px(h, x, y);
            // image y flipped so north is up, matching the lab views
            img.put_pixel((x - x0) as u32, (y1 - 1 - y) as u32, Rgba([v, v, v, 255]));
        }
    }
    img.save(path).unwrap();
}

fn main() {
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    let seed = 48u64;
    let id = RunIdentity::from_seed(seed);
    let spec = SiteSpec::generate_builtin(
        id,
        &SpecOverridesV2 { forced_biome: Some(BiomeId::Piedmont) },
    );
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    let amp = course_amplify::generate(&spec, &sk, &dict, &id);
    let h = hydrology::generate(&spec, &sk, &amp, &id, &hydrology::DEFAULT_TRANSFORMS);
    let n2 = amp.height.spec.nx as usize;

    // ---- 1. floodplain artifact: crop around the biggest channel's mid ----
    let big = sk
        .channels
        .iter()
        .max_by(|a, b| a.area_m2.total_cmp(&b.area_m2))
        .unwrap();
    let mid = big.pts[big.pts.len() / 2];
    let (cx, cy) = ((mid.x / 2.0) as usize, (mid.y / 2.0) as usize);
    save_crop(&amp.height, cx, cy, 300, "/tmp/hydro_pre_flood.png");
    save_crop(&h.height, cx, cy, 300, "/tmp/hydro_post_flood.png");
    // how much did floodplain change, and how rough is the change?
    let mut changed = 0usize;
    let mut max_step = 0.0f64;
    for y in 1..n2 {
        for x in 1..n2 {
            let i = y * n2 + x;
            let d = h.height.data[i] - amp.height.data[i];
            if d.abs() > 1e-9 {
                changed += 1;
                let dl = (h.height.data[i] - h.height.data[i - 1])
                    - (amp.height.data[i] - amp.height.data[i - 1]);
                max_step = max_step.max(dl.abs());
            }
        }
    }
    println!(
        "floodplain touched {:.1}% of cells; max lateral STEP it introduced {:.2} m",
        100.0 * changed as f64 / (n2 * n2) as f64,
        max_step
    );

    // ---- 2. basins: tint actual component cells vs hulls -----------------
    // Recompute the same deep-mask the inventory uses.
    let spec8 = sk.flow_distance.spec;
    let n8 = spec8.nx as usize;
    let mut img = RgbaImage::new(n2 as u32 / 2, n2 as u32 / 2); // 4 m px
    for py in 0..n2 / 2 {
        for px in 0..n2 / 2 {
            let v = hillshade_px(&h.height, px * 2, py * 2);
            img.put_pixel(px as u32, (n2 / 2 - 1 - py) as u32, Rgba([v, v, v, 255]));
        }
    }
    // basin interiors: point-in-hull test per 8m cell is wrong (hull ≠
    // comp) — instead re-derive deep cells exactly as inventory_basins:
    let z8 = {
        let mut g = Grid::filled(spec8, f64::INFINITY);
        for y2 in 0..n2 {
            for x2 in 0..n2 {
                let i8 = (y2 / 4).min(n8 - 1) * n8 + (x2 / 4).min(n8 - 1);
                let v = h.height.data[y2 * n2 + x2];
                if v < g.data[i8] {
                    g.data[i8] = v;
                }
            }
        }
        g
    };
    let base8: Grid<f64> = {
        let mut g = Grid::filled(spec8, 0.0f64);
        for y in 0..n8 {
            for x in 0..n8 {
                let p = spec8.world_of(x as u32, y as u32);
                g.data[y * n8 + x] = sk.height.bilinear(p);
            }
        }
        g
    };
    let keep8: Vec<bool> = {
        let f = course_world::flow::fill_depressions(&base8);
        (0..n8 * n8).map(|i| f.data[i] > base8.data[i] + 0.45).collect()
    };
    let zf = course_world::flow::fill_depressions_masked(&z8, &keep8);
    for y in 0..n8 {
        for x in 0..n8 {
            let i = y * n8 + x;
            if zf.data[i] > z8.data[i] + 0.25 || keep8[i] {
                let p = spec8.world_of(x as u32, y as u32);
                let (px, py) = ((p.x / 4.0) as u32, (p.y / 4.0) as u32);
                for dy in 0..2u32 {
                    for dx in 0..2u32 {
                        let (qx, qy) = (
                            (px + dx).min(n2 as u32 / 2 - 1),
                            (n2 as u32 / 2 - 1).saturating_sub(py + dy),
                        );
                        let o = img.get_pixel(qx, qy).0;
                        img.put_pixel(
                            qx,
                            qy,
                            Rgba([o[0] / 2, (o[1] / 2).saturating_add(90), o[2] / 2, 255]),
                        );
                    }
                }
            }
        }
    }
    // hulls in red on top
    for b in &h.basins {
        let poly: Vec<(i64, i64)> = b
            .polygon
            .iter()
            .map(|p| ((p.x / 4.0) as i64, (n2 as i64 / 2 - 1) - (p.y / 4.0) as i64))
            .collect();
        for k in 0..poly.len() {
            let (a, bb) = (poly[k], poly[(k + 1) % poly.len()]);
            let steps = (a.0 - bb.0).abs().max((a.1 - bb.1).abs()).max(1);
            for s in 0..=steps {
                let x = a.0 + (bb.0 - a.0) * s / steps;
                let y = a.1 + (bb.1 - a.1) * s / steps;
                if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
                    img.put_pixel(x as u32, y as u32, Rgba([255, 60, 40, 255]));
                }
            }
        }
    }
    img.save("/tmp/hydro_basins.png").unwrap();

    // per-basin: depth, hull compactness, edge contact
    let hull_area = |poly: &[course_world::math::Vec2]| -> f64 {
        let mut a = 0.0;
        for k in 0..poly.len() {
            let (p, q) = (poly[k], poly[(k + 1) % poly.len()]);
            a += p.x * q.y - q.x * p.y;
        }
        a.abs() * 0.5
    };
    let mut rows: Vec<(f64, String)> = h
        .basins
        .iter()
        .map(|b| {
            let ha = hull_area(&b.polygon);
            let compact = if ha > 0.0 { b.area_m2 / ha } else { 1.0 };
            let touches_edge = b.polygon.iter().any(|p| {
                p.x < 40.0 || p.y < 40.0 || p.x > 3032.0 || p.y > 3032.0
            });
            (
                b.area_m2,
                format!(
                    "area {:7.1} ha  compact {:.2}  closed {}  edge {}",
                    b.area_m2 / 1e4,
                    compact,
                    b.closed,
                    touches_edge
                ),
            )
        })
        .collect();
    rows.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!("top basins of {}:", h.basins.len());
    for (_, r) in rows.iter().take(12) {
        println!("  {r}");
    }
    let edge_n = h
        .basins
        .iter()
        .filter(|b| {
            b.polygon
                .iter()
                .any(|p| p.x < 40.0 || p.y < 40.0 || p.x > 3032.0 || p.y > 3032.0)
        })
        .count();
    println!("basins touching the tile-edge frame: {edge_n}/{}", h.basins.len());
}
