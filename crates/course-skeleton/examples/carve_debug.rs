//! Checkpoint instrument for the derived-network carve: renders the
//! carving surface and its flow field at several iterations, plus the
//! extracted network, so the reviewer can judge convergence behaviour
//! BEFORE the engine is integrated.
//!
//!   cargo run --release -p course-skeleton --example carve_debug -- <out_dir> [seed]
//!
//! Writes per biome: `<biome>_<seed>_iterNN.png` (hillshade + log-accum
//! overlay) and `<biome>_<seed>_network.png` (extracted channels by
//! Strahler order over the carved hillshade).
use course_contracts::biome::BiomeId;
use course_seed::{streams, RunIdentity};
use course_skeleton::fluvial::carve::{self, CarveParams};
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::flow;
use course_world::grid::Grid;

const SNAPSHOTS: [usize; 5] = [0, 3, 7, 11, 15];

fn hillshade(z: &Grid<f64>, ex: f64) -> Vec<u8> {
    let (nx, ny) = (z.spec.nx as usize, z.spec.ny as usize);
    let cell = z.spec.cell_size;
    let mut out = vec![0u8; nx * ny];
    let (az, alt) = (315f64.to_radians(), 45f64.to_radians());
    for y in 0..ny {
        for x in 0..nx {
            let xm = (x + 1).min(nx - 1);
            let xp = x.saturating_sub(1);
            let ym = (y + 1).min(ny - 1);
            let yp = y.saturating_sub(1);
            let gx = (z.data[y * nx + xm] - z.data[y * nx + xp]) * ex / (2.0 * cell);
            let gy = (z.data[ym * nx + x] - z.data[yp * nx + x]) * ex / (2.0 * cell);
            let slope = (gx * gx + gy * gy).sqrt().atan();
            let aspect = libm::atan2(-gx, gy);
            let hs = alt.sin() * slope.cos() + alt.cos() * slope.sin() * (az - aspect).cos();
            out[y * nx + x] = (((hs + 0.15) / 1.15).clamp(0.0, 1.0) * 255.0) as u8;
        }
    }
    out
}

fn save_rgb(path: &str, nx: usize, ny: usize, px: &[u8]) {
    // y-up world -> y-down image
    let mut flipped = vec![0u8; px.len()];
    for y in 0..ny {
        let src = (ny - 1 - y) * nx * 3;
        let dst = y * nx * 3;
        flipped[dst..dst + nx * 3].copy_from_slice(&px[src..src + nx * 3]);
    }
    image::RgbImage::from_raw(nx as u32, ny as u32, flipped)
        .expect("buffer size")
        .save(path)
        .expect("write png");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out_dir = args.next().unwrap_or_else(|| "/tmp/s2_carve".into());
    let seed: u64 = args.next().map(|s| s.parse().unwrap()).unwrap_or(48);
    std::fs::create_dir_all(&out_dir).unwrap();

    for biome in BiomeId::ALL.iter() {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
        let c1 = course_primitives::generate(&spec, &id);
        let spec8 = c1.grid;
        let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
        let n = nx * ny;
        let mut implied = c1.tilt.clone();
        for (i, v) in implied.data.iter_mut().enumerate() {
            *v += c1.relief.data[i];
        }
        let density = spec.dials.get("skeleton.density_target").copied().unwrap_or(1.0);
        let relief_amp = spec.dials.get("primitives.relief_amp_m").copied().unwrap_or(8.0);
        // erodibility from hardness: soft ground cuts faster
        let erod: Vec<f64> = c1.hardness.data.iter().map(|h| (1.6 - h).clamp(0.3, 1.6)).collect();
        let keep: Vec<bool> = vec![false; n];
        let envf = |k: &str, d: f64| -> f64 {
            std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d)
        };
        let p = CarveParams {
            roughness_frac: envf("ROUGH", 0.12),
            // Threshold defaults to the CORPUS definition (extract_v2's
            // CHANNEL_AREA_M2 = 6.0e4 m²) — the same accumulation cut the
            // real d2c/density targets were measured with, so generated
            // and real numbers mean the same thing.
            k: if density >= 0.5 { envf("K", 0.9) } else { 0.0 },
            area_threshold_m2: envf("THRESH", carve::AREA_THRESHOLD_M2),
            incision_scale: 1.0,
            base_drop_m: envf("BASEDROP", 4.0),
            inflow_area_m2: envf("INFLOW", carve::INFLOW_PER_TRUNK_DIAL_M2)
                * spec.dials.get("skeleton.trunk_river").copied().unwrap_or(0.0),
            close_borders: std::env::var("OPEN").is_err(),
            derangement: ((0.5 - spec.dials.get("skeleton.integration").copied().unwrap_or(0.5)) * 1.6).clamp(0.0, 0.9),
            iters: envf("ITERS", 15.0) as usize,
            step_clamp_m: envf("CLAMP", 0.45),
            tributary_reach: 1.0,
            creep: envf("CREEP", 0.0),
            route_wander: 0.0,
            cut_spread_m: 0.0,
        channel_spread: 0.30,
        bank_angle_deg: 0.0,
        inflow_start_frac: 0.0,
        area_exp: 0.5,
        slope_init_floor: carve::SLOPE_INIT_CLAMP.0,
        shadow_prune_m: 0.0,
        route_meander: 0.0,
        majors_cap: carve::MAJORS_CAP,
        majors_boost: carve::MAJORS_K_BOOST,
        majors_iters: carve::MAJORS_ITERS,
        uplift_m: 0.0,
        };

        // Snapshots: rerun the carve with a truncated iteration budget by
        // calling the public entry with a scratch RNG each time (cheap at
        // 375^2, and keeps carve.rs free of debug hooks).
        let net_only = std::env::var("NET_ONLY").is_ok();
        for &it in SNAPSHOTS.iter().filter(|_| !net_only) {
            let mut rng = id.stream(streams::SKELETON_MODULE);
            let mut pp = CarveParams { ..p };
            let _ = carve::ITERS;
            // iteration count is a const in carve.rs; emulate partial
            // progress by scaling k (0 iters = untouched surface)
            pp.iters = it;
            let c = carve::carve(
                &spec8,
                &implied,
                &erod,
                &keep,
                c1.meta.base_level.edge,
                &mut rng,
                &pp,
                relief_amp,
            );
            let zf = flow::fill_depressions(&c.z);
            let (r, _) = flow::receivers(&zf, spec8.cell_size);
            let acc = flow::accumulate(&r);
            let shade = hillshade(&c.z, envf("EX", 1.8));
            let mut px = vec![0u8; n * 3];
            for i in 0..n {
                let a = (acc[i] as f64).ln() / 11.0;
                let blue = (a.clamp(0.0, 1.0) * 255.0) as u8;
                let s = shade[i];
                px[i * 3] = s.saturating_sub(blue / 2);
                px[i * 3 + 1] = s.saturating_sub(blue / 3);
                px[i * 3 + 2] = s.saturating_add(blue / 3);
            }
            save_rgb(
                &format!("{out_dir}/{}_{seed}_iter{it:02}.png", biome.key()),
                nx,
                ny,
                &px,
            );
        }

        // Extracted network over the fully carved surface.
        let mut rng = id.stream(streams::SKELETON_MODULE);
        let c = carve::carve(
            &spec8,
            &implied,
            &erod,
            &keep,
            c1.meta.base_level.edge,
            &mut rng,
            &p,
            relief_amp,
        );
        let shade = hillshade(&c.z, envf("EX", 1.8));
        let mut px = vec![0u8; n * 3];
        for i in 0..n {
            px[i * 3] = shade[i];
            px[i * 3 + 1] = shade[i];
            px[i * 3 + 2] = shade[i];
        }
        let overlay = std::env::var("NO_OVERLAY").is_err();
        let max_area = c.channels.iter().map(|c| c.area_m2).fold(1.0f64, f64::max);
        for ch in c.channels.iter().filter(|_| overlay) {
            // colour + width by DISCHARGE relative to this tile's biggest
            // channel — absolute cuts marked nearly everything dark
            let f = ch.area_m2 / max_area;
            let (r, g, b, thick) = if f >= 0.45 {
                (10u8, 50u8, 140u8, 2i64)
            } else if f >= 0.15 {
                (40, 110, 200, 1)
            } else {
                (120, 170, 235, 0)
            };
            for w in ch.pts.windows(2) {
                let steps = 8;
                for s in 0..=steps {
                    let t = s as f64 / steps as f64;
                    let wx = w[0].x + (w[1].x - w[0].x) * t;
                    let wy = w[0].y + (w[1].y - w[0].y) * t;
                    let cx = (wx / spec8.cell_size) as i64;
                    let cy = (wy / spec8.cell_size) as i64;
                    for dy in -thick.max(0)..=thick.max(0) {
                        for dx in -thick.max(0)..=thick.max(0) {
                            let (yy, xx) = (cy + dy, cx + dx);
                            if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                                continue;
                            }
                            let i = yy as usize * nx + xx as usize;
                            px[i * 3] = r;
                            px[i * 3 + 1] = g;
                            px[i * 3 + 2] = b;
                        }
                    }
                }
            }
        }
        // MAIN STEM: from the biggest outlet, walk upstream always taking
        // the largest-discharge donor. This is the "single characteristic
        // channel" a viewer looks for; Strahler order does not identify it
        // (a through-going trunk fed from outside the tile has huge area
        // but few upstream branches).
        if overlay {
            let mut outlet = (0.0f64, usize::MAX);
            for i in 0..n {
                if c.rec[i] < 0 && c.area[i] > outlet.0 {
                    outlet = (c.area[i], i);
                }
            }
            let mut donors: Vec<Vec<u32>> = vec![Vec::new(); n];
            for i in 0..n {
                let r = c.rec[i];
                if r >= 0 {
                    donors[r as usize].push(i as u32);
                }
            }
            let mut cur = outlet.1;
            let mut guard = 0;
            while cur != usize::MAX && guard < n {
                let (cy, cx) = ((cur / nx) as i64, (cur % nx) as i64);
                for dy in -2i64..=2 {
                    for dx in -2i64..=2 {
                        let (yy, xx) = (cy + dy, cx + dx);
                        if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                            continue;
                        }
                        let i = yy as usize * nx + xx as usize;
                        px[i * 3] = 200;
                        px[i * 3 + 1] = 30;
                        px[i * 3 + 2] = 30;
                    }
                }
                match donors[cur]
                    .iter()
                    .copied()
                    .filter(|&d| c.area[d as usize] >= p.area_threshold_m2)
                    .max_by(|&a, &b| c.area[a as usize].total_cmp(&c.area[b as usize]))
                {
                    Some(d) => cur = d as usize,
                    None => break,
                }
                guard += 1;
            }
        }
        save_rgb(
            &format!("{out_dir}/{}_{seed}_network.png", biome.key()),
            nx,
            ny,
            &px,
        );
        let n_ch = c.channels.len();
        let max_o = c.channels.iter().map(|c| c.order).max().unwrap_or(0);
        let len: f64 = c
            .channels
            .iter()
            .map(|c| {
                c.pts
                    .windows(2)
                    .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
                    .sum::<f64>()
            })
            .sum();
        // incision depth: how far the carve moved the surface, vs relief
        let mut dz: Vec<f64> = (0..n).map(|i| implied.data[i] + 0.0 - c.z.data[i]).collect();
        dz.sort_by(|a, b| a.total_cmp(b));
        let q = |p: f64| dz[((dz.len() - 1) as f64 * p) as usize];
        // d2c: mean distance to the nearest channel cell (crude sample)
        let ch_cells: Vec<usize> = (0..n).filter(|&i| c.channel_of[i].is_some()).collect();
        let mut d2c = 0.0;
        let mut cnt = 0.0;
        for i in (0..n).step_by(37) {
            let (y, x) = ((i / nx) as f64, (i % nx) as f64);
            let mut best = f64::INFINITY;
            for &cc in &ch_cells {
                let (cy, cx) = ((cc / nx) as f64, (cc % nx) as f64);
                let d = ((cy - y).powi(2) + (cx - x).powi(2)).sqrt();
                if d < best { best = d; }
            }
            if best.is_finite() { d2c += best * spec8.cell_size; cnt += 1.0; }
        }
        eprintln!(
            "{:14} seed {seed}: {n_ch:3} reaches, Ω={max_o}, {:.2} km/km², incision p50/p95 {:.1}/{:.1} m, mean d2c {:.0} m",
            biome.key(),
            len / 1000.0 / 9.0,
            q(0.5), q(0.95),
            if cnt > 0.0 { d2c / cnt } else { f64::NAN }
        );
    }
    eprintln!("wrote to {out_dir}");
}
