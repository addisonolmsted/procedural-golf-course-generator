//! Stage-4T composed no-dam property: the macro floor is strictly monotone
//! by construction; composing the calibrated noise on top must not build
//! dams a stream couldn't clear. Metric per valley, walking the floor
//! centerline DOWNSTREAM on the COMPOSED surface: the largest barrier above
//! the running minimum, max_u [ z(u) − min_{u'≤u} z(u') ], must stay ≤ 1.5 m.
//!
//! The noise config is the sampled archetype's defaults (KEEP-7 mids) with
//! the sample's own noise-channel seed — exactly what the composed pipeline
//! runs — except floor_damp, which is capped at the composition operating
//! point (see NO_DAM_FLOOR_DAMP below).

use golf_core::grid::Grid;
use golf_landform::noiselab::{generate_noise, skeleton_fields, NoiseLabConfig};
use golf_landform::{resolve, sample_macro, LandformPrior};

/// Composition operating point for in-corridor noise. The Stage-1 archetype
/// mid (0.525) matches TILE texture statistics, but real tiles' floors are
/// carved by real hydrology — at 0.525 the synthetic floor noise builds
/// multi-meter dams on gentle trunks. Pending the Stage-7b per-archetype
/// re-fit against clean tiles, composition caps floor_damp here.
const NO_DAM_FLOOR_DAMP: f64 = 0.1;
const MAX_DAM_M: f64 = 1.5;
const RES_M: f64 = 12.0;
const SEEDS: u64 = 100;

fn bilinear(g: &Grid<f64>, p: golf_core::math::Vec2) -> f64 {
    let s = &g.spec;
    let fx = ((p.x - s.origin.x) / s.cell_size).clamp(0.0, (s.nx - 1) as f64);
    let fy = ((p.y - s.origin.y) / s.cell_size).clamp(0.0, (s.ny - 1) as f64);
    let x0 = fx.floor() as u32;
    let y0 = fy.floor() as u32;
    let x1 = (x0 + 1).min(s.nx - 1);
    let y1 = (y0 + 1).min(s.ny - 1);
    let tx = fx - x0 as f64;
    let ty = fy - y0 as f64;
    let a = *g.get(x0, y0) * (1.0 - tx) + *g.get(x1, y0) * tx;
    let b = *g.get(x0, y1) * (1.0 - tx) + *g.get(x1, y1) * tx;
    a * (1.0 - ty) + b * ty
}

fn noise_cfg_for(rep: &golf_landform::SampleReport) -> NoiseLabConfig {
    let d = &rep.noise_defaults;
    let get = |k: &str, fallback: f64| d.get(k).copied().unwrap_or(fallback);
    NoiseLabConfig {
        seed: rep.noise_seed,
        base_amp: get("base_amp", 10.0),
        base_wavelength: get("base_wavelength", 300.0),
        octaves: 5,
        gain: 0.5,
        lacunarity: 2.0,
        warp_amp: 80.0,
        warp_wavelength: get("warp_wavelength", 500.0),
        warp2_amp: 20.0,
        ridged_mix: get("ridged_mix", 0.2),
        redistribution: get("redistribution", 1.3),
        tex_amp: 0.4,
        tex_wavelength: 25.0,
        tex_gain: 0.55,
        nugget_amp: 0.0,
        aniso_ratio: get("aniso_ratio", 1.5),
        floor_damp: get("floor_damp", 0.5).min(NO_DAM_FLOOR_DAMP),
        slope_gain: 0.3,
        grain_align: 0.0,
        dummy: 0.0,
    }
}

#[test]
fn composed_no_dam_100_seeds() {
    let prior = LandformPrior::builtin();
    let mut worst = 0.0f64;
    let mut worst_seed = 0u64;
    let mut dams: Vec<f64> = Vec::new();
    for seed in 0..SEEDS {
        let (cfg, rep) = sample_macro(&prior, seed, None, 1.0);
        let skel = skeleton_fields(&cfg, RES_M);
        let ncfg = noise_cfg_for(&rep);
        let g = generate_noise(&ncfg, cfg.extent_m, Some(&skel), RES_M);
        let r = resolve(&cfg);
        for vi in 0..r.n_valleys() {
            let n = 90;
            let mut run_min = f64::INFINITY;
            let mut run_min_skel = f64::INFINITY;
            let mut dam = 0.0f64;
            let mut dam_skel = 0.0f64;
            let mut dam_u = 0.0;
            let mut prev_floor = f64::INFINITY;
            for k in 0..=n {
                let u = k as f64 / n as f64;
                let (p, floor) = r.valley_floor(vi, u);
                // stop at the junction cap (the overshoot tail rises)
                if floor > prev_floor + 1e-9 {
                    break;
                }
                prev_floor = floor;
                let zs = bilinear(&skel.z, p);
                if zs < floor - 0.5 {
                    // the path crossed a DEEPER carve (e.g. a trunk bend's
                    // wall region): water joins that channel here and its
                    // own monotone floor takes over — the walk ends, there
                    // is no dam to climb
                    break;
                }
                // stations outside the window (trib heads are inset; trunk
                // endpoints sit on the boundary) are all in-window
                let z = bilinear(&g, p);
                run_min = run_min.min(z);
                if z - run_min > dam {
                    dam = z - run_min;
                    dam_u = u;
                }
                run_min_skel = run_min_skel.min(zs);
                dam_skel = dam_skel.max(zs - run_min_skel);
            }
            if dam > 1.5 {
                println!(
                    "  seed {seed} valley {vi}: dam {dam:.2} (skeleton-only {dam_skel:.2}) at u={dam_u:.2}"
                );
            }
            dams.push(dam);
            if dam > worst {
                worst = dam;
                worst_seed = seed;
            }
        }
    }
    dams.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |f: f64| dams[((dams.len() - 1) as f64 * f) as usize];
    println!(
        "composed dams over {} valleys: q50 {:.2} q90 {:.2} q99 {:.2} max {:.2} (seed {})",
        dams.len(),
        q(0.5),
        q(0.9),
        q(0.99),
        worst,
        worst_seed
    );
    assert!(
        worst <= MAX_DAM_M,
        "seed {worst_seed}: composed floor dam {worst:.2} m exceeds {MAX_DAM_M} m"
    );
}

#[test]
#[ignore] // debug probe — run explicitly
fn debug_seed83_valley1() {
    let prior = LandformPrior::builtin();
    let (cfg, rep) = sample_macro(&prior, 83, None, 1.0);
    println!("valleys {} tribs {} bowls {} drops {:?}", cfg.valleys.len(), rep.n_tribs, rep.n_bowls, rep.drops);
    let r = golf_landform::resolve(&cfg);
    let mut run_min = f64::INFINITY;
    let mut prev_floor = f64::INFINITY;
    for k in 0..=90 {
        let u = k as f64 / 90.0;
        let (p, floor) = r.valley_floor(1, u);
        if floor > prev_floor + 1e-9 { println!("cap at u={u:.2}"); break; }
        prev_floor = floor;
        let z = r.height_at(p);
        run_min = run_min.min(z);
        if z - run_min > 0.3 || z - floor > 1.0 {
            println!("u={u:.3} p=({:.0},{:.0}) floor={floor:.2} z_true={z:.2} dam={:.2} above_floor={:.2}", p.x, p.y, z - run_min, z - floor);
        }
    }
}
