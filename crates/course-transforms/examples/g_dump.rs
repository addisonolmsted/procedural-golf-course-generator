//! G-TERRAIN dumper: run the full S0→S4 chain and write the WATERED
//! surface — S4 heights with every water body composited as a flat plane
//! at its surface level, the way real lidar DTMs read water. This is the
//! honest comparison surface for the energy-distance gate and the P2
//! blind materials (the raw S3 dumps showed carved beds where real
//! rivers show flat water — a reviewer-confirmed tell).
//!
//!   cargo run --release -p course-transforms --example g_dump <out_dir> <seed> [<seed>...]
//!   → <out>/<biome>_<seed>.f32   (little-endian f32, n×n, 2 m)

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_transforms::hydrology;
use course_world::math::Vec2;

/// Stamp a water plane by FLOODING from the polygon's interior cells to
/// every connected cell below the surface level. Stamping only inside
/// the polygon clipped the plane at the 8 m traced outline and drew rim
/// lines mid-basin; the real shoreline is the level line where ground
/// rises above the surface, which is exactly where the flood stops.
fn stamp_water(h: &mut [f64], n2: usize, cell: f64, poly: &[Vec2], surface: f64) {
    if poly.len() < 3 {
        return;
    }
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in poly {
        x0 = x0.min(p.x);
        y0 = y0.min(p.y);
        x1 = x1.max(p.x);
        y1 = y1.max(p.y);
    }
    let (cx0, cy0) = ((x0 / cell).floor().max(0.0) as usize, (y0 / cell).floor().max(0.0) as usize);
    let (cx1, cy1) = (
        ((x1 / cell).ceil() as usize).min(n2 - 1),
        ((y1 / cell).ceil() as usize).min(n2 - 1),
    );
    let mut seeds: Vec<usize> = Vec::new();
    for cy in cy0..=cy1 {
        for cx in cx0..=cx1 {
            let (px, py) = (cx as f64 * cell, cy as f64 * cell);
            let mut crossings = 0;
            for i in 0..poly.len() {
                let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
                if (a.y > py) != (b.y > py) {
                    let x_at = a.x + (py - a.y) / (b.y - a.y) * (b.x - a.x);
                    if x_at > px {
                        crossings += 1;
                    }
                }
            }
            let i2 = cy * n2 + cx;
            if crossings % 2 == 1 && h[i2] < surface {
                seeds.push(i2);
            }
        }
    }
    // Grow to the natural shoreline (the level line). A distance cap
    // produced 45-degree BFS-diamond frontiers on run-away ponds whose
    // level exceeds their surroundings (S4 levels are not yet globally
    // spill-consistent) — so instead: fill the FULL level set, and if it
    // runs away (area > 6x the polygon) distrust the level and fall
    // back to the polygon-only stamp.
    let mut level: Vec<usize> = seeds.clone();
    let mut seen: std::collections::HashSet<usize> = seeds.iter().copied().collect();
    let limit = seeds.len() * 6 + 64;
    let mut runaway = false;
    while let Some(i) = level.pop() {
        if seen.len() > limit {
            runaway = true;
            break;
        }
        let (y, x) = (i / n2, i % n2);
        for (dy, dx) in [(0i64, 1i64), (0, -1), (1, 0), (-1, 0)] {
            let (yy, xx) = (y as i64 + dy, x as i64 + dx);
            if yy < 0 || xx < 0 || yy >= n2 as i64 || xx >= n2 as i64 {
                continue;
            }
            let j = yy as usize * n2 + xx as usize;
            if !seen.contains(&j) && h[j] < surface {
                seen.insert(j);
                level.push(j);
            }
        }
    }
    let stamp: Box<dyn Iterator<Item = &usize>> =
        if runaway { Box::new(seeds.iter()) } else { Box::new(seen.iter()) };
    for &i in stamp {
        h[i] = surface;
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "/tmp/g_dump".into());
    std::fs::create_dir_all(&out).unwrap();
    let seeds: Vec<u64> = std::env::args().skip(2).filter_map(|s| s.parse().ok()).collect();
    let dict = ["assets/dictionary_v2.bin", "../../assets/dictionary_v2.bin"]
        .iter()
        .find_map(|p| course_amplify::dictionary::Dictionary::load(std::path::Path::new(p)).ok())
        .expect("dictionary");
    for &seed in &seeds {
        for biome in BiomeId::ALL {
            let id = RunIdentity::from_seed(seed);
            let spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            let amp = course_amplify::generate(&spec, &sk, &dict, &id);
            let h = hydrology::generate(&spec, &sk, &amp, &id, &hydrology::DEFAULT_TRANSFORMS);
            let n2 = h.height.spec.nx as usize;
            let cell = h.height.spec.cell_size;
            let mut z = h.height.data.clone();
            for w in &h.water {
                stamp_water(&mut z, n2, cell, &w.polygon, w.surface_m);
            }
            let mut buf = Vec::with_capacity(z.len() * 4);
            for v in &z {
                buf.extend_from_slice(&(*v as f32).to_le_bytes());
            }
            std::fs::write(format!("{out}/{}_{seed}.f32", spec.biome.key()), &buf).unwrap();
        }
        eprintln!("seed {seed} done");
    }
}
