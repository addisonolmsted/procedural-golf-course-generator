//! Headless terrain dev tool.
//!
//!   cargo run -p xtask -- dump <dir> [seeds...]     # default-param height+slope PNGs
//!   cargo run -p xtask -- dump-sampled <dir> [seeds...]  # seed-sampled PNGs + params
//!   cargo run -p xtask -- stats [seeds...]          # stats table (default params)
//!   cargo run -p xtask -- golden                    # verify determinism golden
//!   cargo run -p xtask -- bless                     # write determinism golden
//!   cargo run -p xtask -- atlas-pack <html> [out]   # parkland_atlas.html -> atlas.bin
//!   cargo run -p xtask -- atlas-stats [bin]         # per-course reference stats
//!   cargo run -p xtask -- calibrate                 # print sampler calibration tables
//!   cargo run -p xtask -- sampler-check [n]         # sampled relief vs target report
//!   cargo run -p xtask -- search [n_seeds] [out]    # run the seed-matching search
//!   cargo run -p xtask -- match-dump <dir> [course] [rank]  # side-by-side match PNGs
//!   cargo run -p xtask -- slope-report [n]          # slope-grade aggregate vs atlas
//!   cargo run -p xtask -- atlas-routing [bin]       # real-course hole/routing statistics
//!   cargo run -p xtask -- routing-report [n]        # routed nines vs atlas gates
//!   cargo run -p xtask -- routing-analysis [n]      # gen-vs-atlas chart -> output/routing_analysis.svg
//!   cargo run -p xtask -- dump-routed <dir> [seeds...]  # terrain+water+routing PNGs
//!   cargo run -p xtask -- build-report [n]          # hole build-out gates (pins, widths, drainage)
//!   cargo run -p xtask -- dump-built <dir> [seeds...]   # built-course PNGs + green close-up grids
//!   cargo run -p xtask -- dump-hole <dir> <seed> [holes...]  # rotated 0.5 m/px per-hole views
//!
//! Heavy commands (calibrate, sampler-check, search, slope-report) want `--release`.

use golf_core::Grid;
use golf_terrain::{generate, world_spec, TerrainParams};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Preview resolution over the fixed 2 km × 2 km world.
const PREVIEW_N: u32 = 200;
/// Fixed resolution for the determinism golden.
const GOLDEN_N: u32 = 128;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(String::as_str).unwrap_or("stats");

    match cmd {
        "dump" => {
            let dir = args.get(1).cloned().unwrap_or_else(|| "preview".into());
            let seeds = parse_seeds(&args[2.min(args.len())..]);
            run_dump(Path::new(&dir), &seeds)
        }
        "dump-sampled" => {
            let dir = args.get(1).cloned().unwrap_or_else(|| "preview".into());
            let seeds = parse_seeds(&args[2.min(args.len())..]);
            run_dump_sampled(Path::new(&dir), &seeds)
        }
        "stats" => {
            print_stats(&parse_seeds(&args[1.min(args.len())..]));
            ExitCode::SUCCESS
        }
        "golden" => run_golden(),
        "bless" => {
            let hash = reference_hash();
            let path = golden_path();
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, format!("{hash}\n")).unwrap();
            println!("blessed terrain golden: {hash}");
            let ph = pipeline_hash();
            std::fs::write(pipeline_golden_path(), format!("{ph}\n")).unwrap();
            println!("blessed pipeline golden: {ph}");
            let rh = routing_hash();
            std::fs::write(routing_golden_path(), format!("{rh}\n")).unwrap();
            println!("blessed routing golden: {rh}");
            let hh = holes_hash();
            std::fs::write(holes_golden_path(), format!("{hh}\n")).unwrap();
            println!("blessed holes golden: {hh}");
            let lh = landform_hash();
            std::fs::write(landform_golden_path(), format!("{lh}\n")).unwrap();
            println!("blessed landform golden: {lh}");
            ExitCode::SUCCESS
        }
        "atlas-pack" => run_atlas_pack(&args[1..]),
        "atlas-stats" => run_atlas_stats(args.get(1).map(PathBuf::from)),
        "calibrate" => {
            run_calibrate();
            ExitCode::SUCCESS
        }
        "sampler-check" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
            run_sampler_check(n);
            ExitCode::SUCCESS
        }
        "search" => run_search_cmd(&args[1..]),
        "match-dump" => run_match_dump(&args[1..]),
        "slope-report" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(100_000);
            run_slope_report(n)
        }
        "erosion-bench" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(200);
            run_erosion_bench(n);
            ExitCode::SUCCESS
        }
        "erosion-sweep" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(40);
            run_erosion_sweep(n)
        }
        "atlas-water" => run_atlas_water(args.get(1).map(PathBuf::from)),
        "water-report" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(600);
            run_water_report(n)
        }
        "lake-capacity" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(20);
            run_lake_capacity(n)
        }
        "water-on-atlas" => run_water_on_atlas(args.get(1).map(PathBuf::from)),
        "atlas-routing" => run_atlas_routing(args.get(1).map(PathBuf::from)),
        "routing-report" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(500);
            run_routing_report(n)
        }
        "dump-routed" => {
            let dir = args.get(1).cloned().unwrap_or_else(|| "output/routed".into());
            let seeds = parse_seeds(&args[2.min(args.len())..]);
            run_dump_routed(Path::new(&dir), &seeds)
        }
        // Sample a generated course's composed surface + water mask onto a
        // plain grid so the Python `metrics` package can score synthetic terrain
        // with the exact same functions it runs on the real atlas.
        "dump-heightgrid" => {
            let out = args.get(1).cloned().unwrap_or_else(|| "output/heightgrid.bin".into());
            let seed = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2024);
            let cell = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(6.0);
            let extent = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1800.0);
            run_dump_heightgrid(Path::new(&out), seed, cell, extent)
        }
        // Batch forward pass for the calibration harness: read a JSON manifest
        // of {out, seed, cell, extent, terrain, erosion} items, generate each
        // with EXPLICIT params (generate_course_with, dry water), write HG01.
        "forward-grid" => {
            let manifest = args.get(1).cloned().unwrap_or_default();
            run_forward_grid(Path::new(&manifest))
        }
        "routing-analysis" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(400);
            run_routing_analysis(n, args.get(2).map(PathBuf::from))
        }
        "dump-built" => {
            let dir = args.get(1).cloned().unwrap_or_else(|| "output/built".into());
            let seeds = parse_seeds(&args[2.min(args.len())..]);
            run_dump_built(Path::new(&dir), &seeds)
        }
        "build-report" => {
            let n = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(300);
            run_build_report(n)
        }
        "dump-hole" => {
            let dir = args.get(1).cloned().unwrap_or_else(|| "output/holes".into());
            let seed = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(2024);
            let holes: Vec<usize> = args[3.min(args.len())..]
                .iter()
                .filter_map(|s| s.parse::<usize>().ok())
                .filter(|&h| (1..=9).contains(&h))
                .map(|h| h - 1)
                .collect();
            run_dump_hole(Path::new(&dir), seed, &holes)
        }
        "routing-probe" => {
            for &seed in &parse_seeds(&args[1..]) {
                let (ct, sp) = golf_terrain::generate_course(&golf_terrain::macro_spec(), seed);
                let f = golf_routing::fields::build(&ct);
                println!(
                    "seed {seed} [{}] relief {:.0}m water {:.1}%: {} greens, {} tees, {} clubs{}",
                    sp.mode.label(),
                    ct.terrain.relief_p2_p98(),
                    100.0 * ct.water.coverage,
                    f.greens.len(),
                    f.tees.len(),
                    f.clubs.len(),
                    if f.relaxed { " (relaxed pads)" } else { "" }
                );
            }
            ExitCode::SUCCESS
        }
        other => {
            eprintln!(
                "unknown command: {other} (dump | dump-sampled | stats | golden | bless | \
                 atlas-pack | atlas-stats | calibrate | sampler-check | search | match-dump)"
            );
            ExitCode::FAILURE
        }
    }
}

fn parse_seeds(args: &[String]) -> Vec<u64> {
    let seeds: Vec<u64> = args.iter().filter_map(|s| s.parse().ok()).collect();
    if seeds.is_empty() {
        vec![1, 2, 3]
    } else {
        seeds
    }
}

fn run_dump(dir: &Path, seeds: &[u64]) -> ExitCode {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let spec = world_spec(PREVIEW_N);
    let params = TerrainParams::default();
    println!("writing to {} (default params)", dir.display());
    for &seed in seeds {
        let t = generate(&spec, seed, &params);
        golf_viz::render_height(&t)
            .save(dir.join(format!("seed{seed}_height.png")))
            .unwrap();
        golf_viz::render_slope(&t)
            .save(dir.join(format!("seed{seed}_slope.png")))
            .unwrap();
    }
    print_stats(seeds);
    ExitCode::SUCCESS
}

fn run_dump_sampled(dir: &Path, seeds: &[u64]) -> ExitCode {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let spec = world_spec(256);
    println!("writing to {} (seed-sampled, full pipeline)", dir.display());
    println!(
        "{:>6} {:<9} {:>7} {:>7} {:>7} {:>5} {:>5} {:>6} {:>6} {:>22} {:>7} {:>7}",
        "seed", "mode", "target", "actual", "period", "oct", "gain", "p", "skew", "erosion Θ/g/α/c", "mslope", "play%"
    );
    for &seed in seeds {
        let (ct, sp) = golf_terrain::generate_course(&spec, seed);
        let t = &ct.terrain;
        golf_viz::render_height(t)
            .save(dir.join(format!("sampled{seed}_height.png")))
            .unwrap();
        golf_viz::render_slope(t)
            .save(dir.join(format!("sampled{seed}_slope.png")))
            .unwrap();
        golf_viz::render_flow(&ct.erosion.flow_area, &ct.erosion.lake_depth, 512)
            .save(dir.join(format!("sampled{seed}_flow.png")))
            .unwrap();
        golf_viz::render_height_water(&ct.macro_heights, &ct.water, 512, None)
            .save(dir.join(format!("sampled{seed}_water.png")))
            .unwrap();
        let s = t.stats();
        println!(
            "{:>6} {:<9} {:>7.1} {:>7.1} {:>7.0} {:>5} {:>5.2} {:>6.2} {:>6.2} {:>9.2}/{:.2}/{:.2}/{:.2} {:>7.4} {:>6.1}% water {:>4.1}%(⌖{:>4.1}) {}b/{}s",
            seed,
            sp.mode.label(),
            sp.relief_target,
            t.relief_p2_p98(),
            sp.params.base_period,
            sp.params.octaves,
            sp.params.gain,
            sp.params.redistribution,
            t.height_skewness(),
            sp.erosion.intensity,
            sp.erosion.deposition,
            sp.erosion.diffusion,
            sp.erosion.hardness_contrast,
            s.mean_slope,
            t.slope_fraction_below(golf_terrain::FAIRWAY_MAX_GRADE) * 100.0,
            100.0 * ct.water.coverage,
            100.0 * sp.water.coverage,
            ct.water.bodies.len(),
            ct.water.streams.iter().filter(|l| l.perennial && !l.pooled).count(),
        );
        let pooled = ct.water.streams.iter().filter(|l| l.pooled).count();
        if pooled > 0 {
            println!("        └ {pooled} lone stream(s) pooled into thin lakes");
        }
        if sp.water.river_area > 0.0 {
            println!(
                "        └ river course: external catchment {:.0} km²",
                sp.water.river_area / 1.0e6
            );
        }
    }
    ExitCode::SUCCESS
}

fn print_stats(seeds: &[u64]) {
    let spec = world_spec(PREVIEW_N);
    let params = TerrainParams::default();
    println!(
        "{:>5} {:>8} {:>8} {:>9} {:>9}",
        "seed", "min", "max", "meanSlope", "steep%"
    );
    for &seed in seeds {
        let s = generate(&spec, seed, &params).stats();
        println!(
            "{:>5} {:>8.2} {:>8.2} {:>9.4} {:>8.1}%",
            seed,
            s.min,
            s.max,
            s.mean_slope,
            s.steep_fraction * 100.0,
        );
    }
}

// ---------------------------------------------------------------------------
// Atlas commands
// ---------------------------------------------------------------------------

fn run_atlas_pack(args: &[String]) -> ExitCode {
    let Some(html_path) = args.first() else {
        eprintln!("usage: xtask atlas-pack <parkland_atlas.html> [out.bin]");
        return ExitCode::FAILURE;
    };
    let out = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(golf_atlas::binfmt::default_bin_path);
    let html = match std::fs::read_to_string(html_path) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("cannot read {html_path}: {e}");
            return ExitCode::FAILURE;
        }
    };
    let atlas = match golf_atlas::parse::parse_html(&html) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("parse failed: {e}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(e) = golf_atlas::binfmt::save(&atlas, &out) {
        eprintln!("cannot write {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    println!(
        "packed {} courses -> {} (fingerprint {:016x})",
        atlas.courses.len(),
        out.display(),
        atlas.fingerprint
    );
    for c in &atlas.courses {
        println!(
            "  {:<14} {:<10} {:>5.0}x{:<5.0}m  relief {:>5.1} m  {} holes",
            c.key,
            c.group.label(),
            c.wm,
            c.hm,
            c.relief(),
            c.holes.len()
        );
    }
    ExitCode::SUCCESS
}

fn load_atlas(bin: Option<PathBuf>) -> Result<golf_atlas::Atlas, ExitCode> {
    let path = bin.unwrap_or_else(golf_atlas::binfmt::default_bin_path);
    golf_atlas::binfmt::load(&path).map_err(|e| {
        eprintln!(
            "cannot load atlas from {}: {e}\nrun: cargo run -p xtask -- atlas-pack <parkland_atlas.html>",
            path.display()
        );
        ExitCode::FAILURE
    })
}

fn run_atlas_stats(bin: Option<PathBuf>) -> ExitCode {
    let atlas = match load_atlas(bin) {
        Ok(a) => a,
        Err(c) => return c,
    };
    println!(
        "{:<14} {:<9} {:>11} {:>9} {:>9} {:>8} {:>7} {:>7} {:>7}",
        "course", "group", "window(m)", "relief", "p2-p98", "mslope", "play%", "rhalf", "water%"
    );
    for c in &atlas.courses {
        let t8 = c.template(7.8125);
        let (lo, hi) = t8.min_max();
        let slope = golf_terrain::derive_slope(&t8.grid);
        let n = slope.data.len() as f64;
        let mslope = slope.data.iter().sum::<f64>() / n;
        let play = slope.data.iter().filter(|&&s| s < 0.12).count() as f64 / n;
        let t15 = c.template(15.625);
        let rhalf = correlation_half_length(&t15.grid);
        println!(
            "{:<14} {:<9} {:>5.0}x{:<5.0} {:>9.1} {:>9.1} {:>8.4} {:>6.1}% {:>7} {:>6.1}%",
            c.key,
            c.group.label(),
            t8.win_w,
            t8.win_h,
            hi - lo,
            t8.relief_p2_p98(),
            mslope,
            play * 100.0,
            rhalf.map(|r| format!("{r:.0}")).unwrap_or_else(|| "-".into()),
            c.waterpct,
        );
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Sampler calibration
// ---------------------------------------------------------------------------

/// Axes written into `sampler/tables.rs` (xtask is their source of truth).
const CAL_PERIODS: [f64; 7] = [250.0, 500.0, 800.0, 1200.0, 1600.0, 2000.0, 2400.0];
const CAL_GAINS: [f64; 5] = [0.50, 0.57, 0.64, 0.71, 0.78];
/// Redistribution strengths (e ≥ 1 branch; p < 1 mirrors by symmetry).
const CAL_REDIST: [f64; 6] = [1.0, 1.6, 2.5, 4.0, 6.5, 10.0];
/// Erosion-intensity axis of the pipeline relief table, and the base-period
/// axis measured at each period's mode-typical gain.
const CAL_ERODE_T: [f64; 6] = [0.5, 1.0, 2.0, 3.0, 4.5, 6.0];
const CAL_ERODE_PG: [(f64, f64); 3] = [(800.0, 0.66), (1400.0, 0.60), (2000.0, 0.54)];
const CAL_N: u32 = 128;
const CAL_SEEDS: u64 = 10;

fn unit_params(period: f64, gain: f64) -> TerrainParams {
    let (warp_amp, warp_period) = golf_terrain::sampler::calibration_warp(period);
    TerrainParams {
        amplitude: 1.0,
        base_period: period,
        octaves: golf_terrain::sampler::octaves_for(period),
        lacunarity: 2.0,
        gain,
        warp_amp,
        warp_period,
        redistribution: 1.0,
        ..TerrainParams::default()
    }
}

/// Measure the calibration tables and write `golf-terrain/src/sampler/tables.rs`.
fn run_calibrate() {
    let spec = world_spec(CAL_N);

    let mut k = [[0.0f64; 5]; 7];
    let mut c = [[0.0f64; 5]; 7];
    for (pi, &p) in CAL_PERIODS.iter().enumerate() {
        for (gi, &g) in CAL_GAINS.iter().enumerate() {
            let params = unit_params(p, g);
            let (mut ksum, mut csum) = (0.0, 0.0);
            let mut ccnt = 0u64;
            for seed in 0..CAL_SEEDS {
                let t = generate(&spec, seed, &params);
                ksum += t.relief_p2_p98();
                if let Some(rh) = correlation_half_length(&t.heights) {
                    csum += rh / p;
                    ccnt += 1;
                }
            }
            k[pi][gi] = ksum / CAL_SEEDS as f64;
            // Fields whose correlation never crosses in-window fall back to
            // the neighbor fill below.
            c[pi][gi] = if ccnt > 0 { csum / ccnt as f64 } else { 0.0 };
        }
    }
    // Fill any missing correlation cells from the previous period row.
    let mut prev_row = [0.2f64; 5];
    for row in c.iter_mut() {
        for (v, fallback) in row.iter_mut().zip(prev_row.iter()) {
            if *v == 0.0 {
                *v = *fallback;
            }
        }
        prev_row = *row;
    }

    // From here on, measurements run THROUGH the erosion pipeline (that is
    // the surface players see and the search scores) at a fixed reference
    // erosion. Pipeline calls are pricier — rayon over seeds.
    use rayon::prelude::*;
    const REF_EROSION: golf_terrain::ErosionParams = golf_terrain::ErosionParams {
        intensity: 2.2,
        deposition: 1.2,
        diffusion: 1.2,
        hardness_contrast: 0.0,
    };

    // Redistribution: skewness and relief multiplier per strength (e ≥ 1
    // basin branch; the plateau branch mirrors), measured through the
    // pipeline at REF_EROSION, relative to e = 1 at the same erosion (so the
    // erosion shrinkage itself stays in K_ERODE, not here).
    let (mut skew_p, mut kf_p) = ([0.0f64; 6], [0.0f64; 6]);
    let redist_seeds = 4 * CAL_SEEDS;
    let mid = (1200.0, 0.64);
    let pipeline_relief = |params: &TerrainParams, seed: u64| {
        let ct = golf_terrain::generate_course_with(&spec, seed, params, &REF_EROSION, &golf_terrain::WaterParams::NONE);
        (ct.terrain.relief_p2_p98(), ct.terrain.height_skewness())
    };
    let base: Vec<f64> = (0..redist_seeds)
        .into_par_iter()
        .map(|seed| pipeline_relief(&unit_params(mid.0, mid.1), seed).0)
        .collect();
    for (ri, &p) in CAL_REDIST.iter().enumerate() {
        let params = TerrainParams { redistribution: p, ..unit_params(mid.0, mid.1) };
        let (ssum, fsum) = (0..redist_seeds)
            .into_par_iter()
            .map(|seed| {
                let (r, s) = pipeline_relief(&params, seed);
                (s, r / base[seed as usize])
            })
            .reduce(|| (0.0, 0.0), |a, b| (a.0 + b.0, a.1 + b.1));
        skew_p[ri] = ssum / redist_seeds as f64;
        kf_p[ri] = fsum / redist_seeds as f64;
    }

    // Pipeline relief multiplier vs the un-eroded field, over (Θ, period) —
    // measured at each period's mode-typical gain. Deposition strength barely
    // moves retention (< ±3% measured), so it carries no axis.
    let erode_seeds = 2 * CAL_SEEDS;
    let mut k_erode = [[0.0f64; 3]; 6];
    for (pi, &(period, pgain)) in CAL_ERODE_PG.iter().enumerate() {
        let noise_base: Vec<f64> = (0..erode_seeds)
            .into_par_iter()
            .map(|seed| generate(&spec, seed, &unit_params(period, pgain)).relief_p2_p98())
            .collect();
        for (ti, &theta) in CAL_ERODE_T.iter().enumerate() {
            let ep = golf_terrain::ErosionParams {
                intensity: theta,
                deposition: 1.2,
                diffusion: 1.2,
                hardness_contrast: 0.0,
            };
            let sum: f64 = (0..erode_seeds)
                .into_par_iter()
                .map(|seed| {
                    let ct = golf_terrain::generate_course_with(
                        &spec,
                        seed,
                        &unit_params(period, pgain),
                        &ep,
                        &golf_terrain::WaterParams::NONE,
                    );
                    ct.terrain.relief_p2_p98() / noise_base[seed as usize]
                })
                .sum();
            k_erode[ti][pi] = sum / erode_seeds as f64;
        }
    }
    let (ke_min, ke_max) = k_erode
        .iter()
        .flatten()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &v| {
            (lo.min(v), hi.max(v))
        });
    eprintln!(
        "K_ERODE spread: {ke_min:.3}..{ke_max:.3} ({}: the table {} needed beyond ±8%)",
        if ke_max - ke_min > 0.16 || (1.0 - ke_min) > 0.08 { "SHIP" } else { "borderline" },
        if ke_max - ke_min > 0.16 || (1.0 - ke_min) > 0.08 { "IS" } else { "may not be" },
    );

    let mut out = String::new();
    out.push_str(
        "//! GENERATED by `cargo run -p xtask --release -- calibrate` — do not edit.\n",
    );
    out.push_str(&format!(
        "//! Measured on the {CAL_N}×{CAL_N} 2 km world, seeds 0..{CAL_SEEDS}, using the sampler's own\n//! octave/warp rules.\n\n"
    ));
    out.push_str("/// Base-period axis of the tables, meters.\n");
    out.push_str(&format!(
        "pub const CAL_PERIODS: [f64; 7] = [{}];\n",
        CAL_PERIODS.map(|v| format!("{v:.1}")).join(", ")
    ));
    out.push_str("/// Gain axis of the tables.\n");
    out.push_str(&format!(
        "pub const CAL_GAINS: [f64; 5] = [{}];\n\n",
        CAL_GAINS.map(|v| format!("{v:.2}")).join(", ")
    ));
    out.push_str("/// p2–p98 relief of a unit-amplitude field over the 2 km window.\n");
    push_table(&mut out, "K_RELIEF", &k);
    out.push_str(
        "\n/// Redistribution strength axis (the e ≥ 1 branch; the p < 1 side mirrors it\n/// exactly because the raw field is symmetric).\n",
    );
    out.push_str(&format!(
        "pub const CAL_REDIST: [f64; 6] = [{}];\n",
        CAL_REDIST.map(|v| format!("{v:.2}")).join(", ")
    ));
    out.push_str("/// Height skewness produced by each strength.\n");
    out.push_str(&format!(
        "pub const SKEW_REDIST: [f64; 6] = [{}];\n",
        skew_p.map(|v| format!("{v:.4}")).join(", ")
    ));
    out.push_str("/// Relief multiplier of each strength vs e=1.\n");
    out.push_str(&format!(
        "pub const K_REDIST: [f64; 6] = [{}];\n",
        kf_p.map(|v| format!("{v:.4}")).join(", ")
    ));
    out.push_str("\n/// Erosion-intensity axis of the pipeline relief table.\n");
    out.push_str(&format!(
        "pub const CAL_ERODE_T: [f64; 6] = [{}];\n",
        CAL_ERODE_T.map(|v| format!("{v:.1}")).join(", ")
    ));
    out.push_str(
        "/// Base-period axis of the pipeline relief table (measured at each period's\n/// mode-typical gain; deposition barely moves retention and has no axis).\n",
    );
    out.push_str(&format!(
        "pub const CAL_ERODE_P: [f64; 3] = [{}];\n",
        CAL_ERODE_PG.map(|(p, _)| format!("{p:.1}")).join(", ")
    ));
    out.push_str("/// Pipeline relief multiplier vs the un-eroded field.\n");
    out.push_str("pub const K_ERODE: [[f64; 3]; 6] = [\n");
    for row in &k_erode {
        out.push_str("    [");
        out.push_str(&row.map(|v| format!("{v:.4}")).join(", "));
        out.push_str("],\n");
    }
    out.push_str("];\n");
    out.push_str(
        "\n/// In-window correlation half-length as a fraction of base period. Measured\n\
         /// for reference; the p(θ) sampler draws base_period directly, so it is unread.\n\
         #[allow(dead_code)]\n",
    );
    push_table(&mut out, "C_RHALF", &c);

    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("golf-terrain/src/sampler/tables.rs");
    std::fs::write(&path, &out).unwrap();
    println!("{out}");
    println!("wrote {} — rebuild to pick it up", path.display());
}

fn push_table(out: &mut String, name: &str, t: &[[f64; 5]; 7]) {
    out.push_str(&format!("pub const {name}: [[f64; 5]; 7] = [\n"));
    for row in t {
        out.push_str("    [");
        out.push_str(&row.map(|v| format!("{v:.4}")).join(", "));
        out.push_str("],\n");
    }
    out.push_str("];\n");
}

/// Lag where the structure function D(r) crosses the field variance (ρ = 0.5).
fn correlation_half_length(g: &Grid<f64>) -> Option<f64> {
    let nx = g.spec.nx as usize;
    let ny = g.spec.ny as usize;
    let cell = g.spec.cell_size;
    let data = &g.data;
    let len = data.len() as f64;
    let mean = data.iter().sum::<f64>() / len;
    let var = data.iter().map(|z| (z - mean) * (z - mean)).sum::<f64>() / len;
    if var <= 0.0 {
        return None;
    }
    let lags = [1usize, 2, 3, 4, 6, 8, 11, 16, 22, 32, 45, 64, 90];
    let mut prev: Option<(f64, f64)> = None;
    for &klag in &lags {
        if klag >= nx.min(ny) {
            break;
        }
        let mut sum = 0.0;
        let mut cnt = 0usize;
        for y in 0..ny {
            for x in 0..nx - klag {
                let d = data[y * nx + x + klag] - data[y * nx + x];
                sum += d * d;
                cnt += 1;
            }
        }
        for y in 0..ny - klag {
            for x in 0..nx {
                let d = data[(y + klag) * nx + x] - data[y * nx + x];
                sum += d * d;
                cnt += 1;
            }
        }
        let dmean = sum / cnt as f64;
        let r = klag as f64 * cell;
        if dmean >= var {
            return Some(match prev {
                None => r,
                Some((r0, d0)) => {
                    let t = ((var - d0) / (dmean - d0).max(1e-12)).clamp(0.0, 1.0);
                    r0 * golf_core::math::pow(r / r0, t)
                }
            });
        }
        prev = Some((r, dmean));
    }
    None
}

/// Live atlas slope/flat/steep baseline per relief band, at the same 15.625
/// m/px the generated side below is measured at. Bands use the SAME
/// relief-p2p98 thresholds as the sampler's `ReliefMode` labels (<20 lowland,
/// <65 rolling), so the comparison is apples-to-apples. Replaces the old
/// hardcoded 27-course line — the 200-course parkland atlas has different
/// band statistics (notably rolling mslope ~0.08, not the old 0.059).
fn print_atlas_slope_baseline() {
    let atlas = match load_atlas(None) {
        Ok(a) => a,
        Err(_) => {
            println!("atlas baseline unavailable (assets/atlas.bin not found)");
            return;
        }
    };
    #[derive(Default, Clone, Copy)]
    struct B {
        n: u64,
        mslope: f64,
        play: f64,
        flat: f64,
        steep: f64,
        skew: f64,
    }
    let mut bands = [B::default(); 3];
    for c in &atlas.courses {
        let t = c.template(15.625);
        let relief = t.relief_p2_p98();
        let bi = if relief < 20.0 {
            0
        } else if relief < 65.0 {
            1
        } else {
            2
        };
        let slope = golf_terrain::derive_slope(&t.grid);
        let total = slope.data.len() as f64;
        let b = &mut bands[bi];
        b.n += 1;
        b.mslope += slope.data.iter().sum::<f64>() / total;
        b.play += slope.data.iter().filter(|&&s| s < 0.12).count() as f64 / total;
        b.flat += slope.data.iter().filter(|&&v| v < 0.02).count() as f64 / total;
        b.steep +=
            slope.data.iter().filter(|&&v| (0.12..0.25).contains(&v)).count() as f64 / total;
        // moment skew of the height distribution (same formula as
        // Terrain::height_skewness on the generated side)
        let d = &t.grid.data;
        let n = d.len() as f64;
        let mean = d.iter().sum::<f64>() / n;
        let (mut m2, mut m3) = (0.0, 0.0);
        for &z in d.iter() {
            let e = z - mean;
            m2 += e * e;
            m3 += e * e * e;
        }
        m2 /= n;
        m3 /= n;
        if m2 > 1e-12 {
            b.skew += m3 / golf_core::math::pow(m2, 1.5);
        }
    }
    let line = |b: &B| {
        if b.n == 0 {
            return "n=0".to_string();
        }
        let c = b.n as f64;
        format!(
            "n={} ms {:.3} play {:.0} flat {:.0} steep12-25 {:.0} skew {:+.2}",
            b.n,
            b.mslope / c,
            100.0 * b.play / c,
            100.0 * b.flat / c,
            100.0 * b.steep / c,
            b.skew / c
        )
    };
    println!(
        "atlas (200 courses, live @15.6 m/px, banded by relief p2-p98 <20/<65): lowland {} | rolling {} | mountain {}",
        line(&bands[0]),
        line(&bands[1]),
        line(&bands[2])
    );
}

/// Generated-relief-vs-target report over n sampled seeds.
fn run_sampler_check(n: u64) {
    let spec = world_spec(CAL_N);
    #[derive(Default, Clone, Copy)]
    struct Acc {
        n: u64,
        ratio: f64,
        mslope: f64,
        play: f64,
        flat: f64,
        steep: f64,
        rhalf: f64,
        rhalf_n: u64,
        skew: f64,
        skew_err: f64,
    }
    let mut by_mode = [Acc::default(); 3];
    let (mut rsum, mut rsq) = (0.0, 0.0);
    for seed in 0..n {
        let (ct, sp) = golf_terrain::generate_course(&spec, seed);
        let t = &ct.terrain;
        let actual = t.relief_p2_p98();
        let ratio = actual / sp.relief_target;
        rsum += ratio;
        rsq += ratio * ratio;
        let s = t.stats();
        let mi = match sp.mode {
            golf_terrain::ReliefMode::Lowland => 0,
            golf_terrain::ReliefMode::Rolling => 1,
            golf_terrain::ReliefMode::Mountain => 2,
        };
        let a = &mut by_mode[mi];
        a.n += 1;
        a.ratio += ratio;
        a.mslope += s.mean_slope;
        a.play += t.slope_fraction_below(golf_terrain::FAIRWAY_MAX_GRADE);
        let total = t.slope.data.len() as f64;
        a.flat += t.slope.data.iter().filter(|&&v| v < 0.02).count() as f64 / total;
        a.steep += t.slope.data.iter().filter(|&&v| (0.12..0.25).contains(&v)).count() as f64 / total;
        let sk = t.height_skewness();
        a.skew += sk;
        a.skew_err += (sk - sp.skew_target).abs();
        if let Some(rh) = correlation_half_length(&t.heights) {
            a.rhalf += rh;
            a.rhalf_n += 1;
        }
    }
    let mean = rsum / n as f64;
    let sd = (rsq / n as f64 - mean * mean).max(0.0).sqrt();
    println!("relief actual/target over {n} seeds (full pipeline): mean {mean:.3}, sd {sd:.3}");
    print_atlas_slope_baseline();
    for (i, label) in ["lowland", "rolling", "mountain"].iter().enumerate() {
        let a = by_mode[i];
        if a.n > 0 {
            let c = a.n as f64;
            println!(
                "  {label:<9} n={:<4} ratio {:.3}  mslope {:.4}  play {:.1}%  flat0-2 {:.1}%  steep12-25 {:.1}%  rhalf {:.0}  skew {:+.2} (|err| {:.2})",
                a.n,
                a.ratio / c,
                a.mslope / c,
                100.0 * a.play / c,
                100.0 * a.flat / c,
                100.0 * a.steep / c,
                a.rhalf / (a.rhalf_n.max(1) as f64),
                a.skew / c,
                a.skew_err / c,
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Seed search
// ---------------------------------------------------------------------------

fn run_search_cmd(args: &[String]) -> ExitCode {
    let atlas = match load_atlas(None) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let mut config = golf_match::SearchConfig::default();
    if let Some(n) = args.first().and_then(|s| s.parse::<u64>().ok()) {
        config.seed_count = n;
    }
    let out = args
        .get(1)
        .map(PathBuf::from)
        .unwrap_or_else(golf_match::default_results_path);

    let progress = Arc::new(golf_match::Progress::default());
    let printer = {
        let p = Arc::clone(&progress);
        std::thread::spawn(move || loop {
            std::thread::sleep(std::time::Duration::from_millis(500));
            let stage = p.stage.load(Ordering::Relaxed);
            if stage >= 3 {
                break;
            }
            eprint!(
                "\rstage {} {:>5.1}%   ",
                stage.max(1),
                p.fraction() * 100.0
            );
        })
    };

    let results = golf_match::run_search(&atlas, &config, &progress);
    progress.stage.store(3, Ordering::Relaxed);
    printer.join().ok();
    eprintln!();

    let Some(results) = results else {
        eprintln!("search cancelled");
        return ExitCode::FAILURE;
    };
    if let Err(e) = golf_match::save_results(&results, &out) {
        eprintln!("cannot write {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    println!(
        "searched {} seeds in {:.1}s -> {}",
        config.seed_count,
        results.elapsed_s,
        out.display()
    );
    println!("{:<14} {:>8} {:>8} {:>10}", "course", "seed", "rmse(m)", "offset(m)");
    for m in &results.matches {
        if let Some(b) = m.best.first() {
            println!(
                "{:<14} {:>8} {:>8.2} {:>4.0},{:<4.0}",
                m.course_key, b.seed, b.rmse, b.off_x, b.off_y
            );
        }
    }
    ExitCode::SUCCESS
}

/// Write side-by-side PNGs (course window vs matched seed window, shared
/// color scale) for a course from the cached search results.
///
///   xtask match-dump <dir> [course-key] [rank]
fn run_match_dump(args: &[String]) -> ExitCode {
    let dir = PathBuf::from(args.first().cloned().unwrap_or_else(|| "preview".into()));
    let atlas = match load_atlas(None) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let results = match golf_match::load_results(&golf_match::default_results_path()) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("no cached results ({e}); run: cargo run -p xtask --release -- search");
            return ExitCode::FAILURE;
        }
    };
    let keys: Vec<String> = match args.get(1) {
        Some(k) => vec![k.clone()],
        None => atlas.courses.iter().map(|c| c.key.clone()).collect(),
    };
    let rank: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0);
    std::fs::create_dir_all(&dir).ok();

    const MPP: f64 = 7.8125;
    for key in keys {
        let Some(course) = atlas.course(&key) else {
            eprintln!("unknown course {key}");
            continue;
        };
        let Some(score) = results
            .matches
            .iter()
            .find(|m| m.course_key == key)
            .and_then(|m| m.best.get(rank))
        else {
            eprintln!("{key}: no match at rank {rank}");
            continue;
        };
        let (win_w, win_h) = course.window_dims(results.config.stage2_mpp);
        let cgrid = course.window_grid(win_w, win_h, MPP);
        let cmean = cgrid.data.iter().sum::<f64>() / cgrid.data.len() as f64;
        let (mut g, mu, gen_water) = golf_match::regenerate_window(
            score.seed, win_w, win_h, score.off_x, score.off_y, MPP, cmean,
        );
        for z in &mut g.data {
            *z -= mu;
        }
        let (lo, hi) = {
            let mm = |d: &[f64]| {
                d.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
                    (a.min(v), b.max(v))
                })
            };
            let (c0, c1) = mm(&cgrid.data);
            let (g0, g1) = mm(&g.data);
            (c0.min(g0), c1.max(g1))
        };
        let mut course_img = golf_viz::render_height_grid(&cgrid, 512, Some((lo, hi)));
        golf_viz::tint_water_mask(&mut course_img, &course.water_window(win_w, win_h, MPP));
        course_img.save(dir.join(format!("{key}_course.png"))).unwrap();
        let mut gen_img = golf_viz::render_height_grid(&g, 512, Some((lo, hi)));
        golf_viz::tint_water_mask(&mut gen_img, &gen_water);
        gen_img
            .save(dir.join(format!("{key}_seed{}.png", score.seed)))
            .unwrap();
        println!(
            "{key}: seed {} rmse {:.2} m at ({:.0},{:.0}) -> {}",
            score.seed,
            score.rmse,
            score.off_x,
            score.off_y,
            dir.display()
        );
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Water: atlas analysis + generated-pool gates
// ---------------------------------------------------------------------------

/// Connected components of a boolean mask, 8-connected, deterministic order.
fn mask_components(mask: &[bool], nx: usize, ny: usize) -> Vec<Vec<u32>> {
    let n = nx * ny;
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    let mut stack = Vec::new();
    for s in 0..n {
        if !mask[s] || seen[s] {
            continue;
        }
        seen[s] = true;
        stack.push(s);
        let mut cells = Vec::new();
        while let Some(i) = stack.pop() {
            cells.push(i as u32);
            let (x, y) = (i % nx, i / nx);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                    if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                        continue;
                    }
                    let c = cy as usize * nx + cx as usize;
                    if mask[c] && !seen[c] {
                        seen[c] = true;
                        stack.push(c);
                    }
                }
            }
        }
        out.push(cells);
    }
    out
}

/// PCA elongation of a component (major/minor spread ratio).
fn elongation(cells: &[u32], nx: usize, cw: f64, ch: f64) -> f64 {
    if cells.len() < 4 {
        return 1.0;
    }
    let pts: Vec<(f64, f64)> = cells
        .iter()
        .map(|&c| ((c as usize % nx) as f64 * cw, (c as usize / nx) as f64 * ch))
        .collect();
    let n = pts.len() as f64;
    let (mx, my) = pts.iter().fold((0.0, 0.0), |a, p| (a.0 + p.0, a.1 + p.1));
    let (mx, my) = (mx / n, my / n);
    let (mut sxx, mut syy, mut sxy) = (0.0, 0.0, 0.0);
    for (x, y) in &pts {
        let (dx, dy) = (x - mx, y - my);
        sxx += dx * dx;
        syy += dy * dy;
        sxy += dx * dy;
    }
    let (sxx, syy, sxy) = (sxx / n, syy / n, sxy / n);
    let tr = sxx + syy;
    let det = sxx * syy - sxy * sxy;
    let disc = (tr * tr / 4.0 - det).max(0.0).sqrt();
    let hi = tr / 2.0 + disc;
    let lo = (tr / 2.0 - disc).max(1e-9);
    (hi / lo).sqrt()
}

/// Per-course water-body statistics from the atlas's real water masks — the
/// reference the sampler draws and the water-report gates are fit against.
fn run_atlas_water(bin: Option<PathBuf>) -> ExitCode {
    let atlas = match load_atlas(bin) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let n = golf_atlas::GRID_N;
    println!(
        "{:<14} {:<9} {:>7} {:>7} {:>6} {:>9} {:>9} {:>10}",
        "course", "group", "relief", "water%", "nbody", "maxshare", "medA(m2)", "streamlike%"
    );
    for c in &atlas.courses {
        let mask: Vec<bool> = c.water.iter().map(|&v| v != 0).collect();
        let (cw, ch) = (c.wm / n as f64, c.hm / n as f64);
        let cell_a = cw * ch;
        let comps: Vec<Vec<u32>> = mask_components(&mask, n, n)
            .into_iter()
            .filter(|cc| cc.len() >= 3)
            .collect();
        let total: f64 = comps.iter().map(|cc| cc.len() as f64 * cell_a).sum();
        let cov = 100.0 * total / (c.wm * c.hm);
        let (mut maxa, mut stream_a) = (0.0f64, 0.0f64);
        let mut areas: Vec<f64> = Vec::new();
        for cc in &comps {
            let a = cc.len() as f64 * cell_a;
            areas.push(a);
            maxa = maxa.max(a);
            if elongation(cc, n, cw, ch) > 3.5 {
                stream_a += a;
            }
        }
        areas.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = if areas.is_empty() { 0.0 } else { areas[areas.len() / 2] };
        println!(
            "{:<14} {:<9} {:>7.1} {:>7.1} {:>6} {:>9.2} {:>9.0} {:>10.1}",
            c.key,
            c.group.label(),
            c.relief(),
            cov,
            comps.len(),
            maxa / total.max(1e-9),
            med,
            100.0 * stream_a / total.max(1e-9),
        );
    }
    ExitCode::SUCCESS
}

/// Generated-pool water gates: coverage vs target and vs the atlas per mode,
/// dry fractions, body statistics, connectivity, flat-band regression check.
fn run_water_report(n: u64) -> ExitCode {
    use golf_terrain::water::{BodyClass, CLASS_DRY, CLASS_WETLAND};
    use rayon::prelude::*;

    #[derive(Default, Clone)]
    struct Acc {
        n: u64,
        cov_err: Vec<f64>,
        cov: f64,
        dry: u64,
        big: u64, // coverage > 8%
        bodies: f64,
        wetland_frac: f64,
        flat: f64,
        min_dry_cc: f64,
        min_dry_cc_river: f64,
        streams_len_m: f64,
        unanchored: u64,
        skinny: u64,
        body_len: Vec<f64>,
        body_ratio: Vec<f64>,
    }
    impl Acc {
        fn merge(mut self, o: Acc) -> Acc {
            self.n += o.n;
            self.cov_err.extend(o.cov_err);
            self.cov += o.cov;
            self.dry += o.dry;
            self.big += o.big;
            self.bodies += o.bodies;
            self.wetland_frac += o.wetland_frac;
            self.flat += o.flat;
            self.min_dry_cc = self.min_dry_cc.min(o.min_dry_cc);
            self.min_dry_cc_river = self.min_dry_cc_river.min(o.min_dry_cc_river);
            self.streams_len_m += o.streams_len_m;
            self.unanchored += o.unanchored;
            self.skinny += o.skinny;
            self.body_len.extend(o.body_len);
            self.body_ratio.extend(o.body_ratio);
            self
        }
        fn new() -> Acc {
            Acc {
                min_dry_cc: f64::INFINITY,
                min_dry_cc_river: f64::INFINITY,
                ..Default::default()
            }
        }
    }

    let spec = world_spec(128); // 15.625 m/px, matches slope-report
    let accs = (0..n)
        .into_par_iter()
        .fold(
            || [Acc::new(), Acc::new(), Acc::new()],
            |mut accs, seed| {
                let (ct, sp) = golf_terrain::generate_course(&spec, seed);
                let mi = match sp.mode {
                    golf_terrain::ReliefMode::Lowland => 0,
                    golf_terrain::ReliefMode::Rolling => 1,
                    golf_terrain::ReliefMode::Mountain => 2,
                };
                let a = &mut accs[mi];
                a.n += 1;
                let cov = ct.water.coverage;
                a.cov += cov;
                if sp.water.coverage > 0.0 {
                    a.cov_err.push((cov - sp.water.coverage).abs() * 100.0);
                } else {
                    a.dry += 1;
                }
                if cov > 0.08 {
                    a.big += 1;
                }
                a.bodies += ct.water.bodies.len() as f64;
                let wl = ct
                    .water
                    .class
                    .data
                    .iter()
                    .filter(|&&c| c == CLASS_WETLAND)
                    .count() as f64;
                let wet_total = ct
                    .water
                    .class
                    .data
                    .iter()
                    .filter(|&&c| c != CLASS_DRY)
                    .count() as f64;
                a.wetland_frac += if wet_total > 0.0 { wl / wet_total } else { 0.0 };
                // Flat band on the composed output (raw terrain).
                let total = ct.terrain.slope.data.len() as f64;
                a.flat += ct.terrain.slope.data.iter().filter(|&&s| s < 0.02).count() as f64
                    / total;
                // Routable connectivity: blocking water only (brooks/creeks
                // are carry hazards, not walls; river courses accept a split
                // world with Phase B crossings).
                let ncell = ct.water.class.data.len();
                let passable: Vec<bool> = (0..ncell).map(|i| !ct.water.is_blocking(i)).collect();
                let comps = mask_components(
                    &passable,
                    golf_terrain::MACRO_N as usize,
                    golf_terrain::MACRO_N as usize,
                );
                let biggest =
                    comps.iter().map(|c| c.len()).max().unwrap_or(0) as f64 / ncell as f64;
                if sp.water.river_area > 0.0 {
                    a.min_dry_cc_river = a.min_dry_cc_river.min(biggest);
                } else {
                    a.min_dry_cc = a.min_dry_cc.min(biggest);
                }
                for l in &ct.water.streams {
                    if l.perennial {
                        for k in 1..l.pts.len() {
                            a.streams_len_m += l.pts[k].distance(l.pts[k - 1]);
                        }
                    }
                }
                // Terrain-water contracts on the final layer: every stream
                // endpoint anchored (edge/body/junction) and no skinny
                // string-of-cells bodies.
                {
                    let (wnx, wny) =
                        (ct.water.class.spec.nx as usize, ct.water.class.spec.ny as usize);
                    let mut bodym = vec![false; wnx * wny];
                    for b in ct.water.bodies.iter().filter(|b| b.class != BodyClass::Wetland)
                    {
                        for &c in &b.cells {
                            bodym[c as usize] = true;
                        }
                    }
                    let mut occ = vec![u32::MAX; wnx * wny];
                    for (li, l) in ct.water.streams.iter().enumerate() {
                        if !l.perennial {
                            continue;
                        }
                        for &c in &l.cells {
                            if occ[c as usize] == u32::MAX {
                                occ[c as usize] = li as u32;
                            }
                        }
                    }
                    let anchored = |c: usize, li: usize| -> bool {
                        let (x, y) = (c % wnx, c / wnx);
                        if x <= 1 || y <= 1 || x >= wnx - 2 || y >= wny - 2 || bodym[c] {
                            return true;
                        }
                        (-1i32..=1).any(|dy| {
                            (-1i32..=1).any(|dx| {
                                let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                                cx >= 0
                                    && cy >= 0
                                    && (cx as usize) < wnx
                                    && (cy as usize) < wny
                                    && {
                                        let m = cy as usize * wnx + cx as usize;
                                        bodym[m] || (occ[m] != u32::MAX && occ[m] != li as u32)
                                    }
                            })
                        })
                    };
                    for (li, l) in ct.water.streams.iter().enumerate() {
                        if !l.perennial || l.pooled || l.cells.is_empty() {
                            continue;
                        }
                        if !anchored(l.cells[0] as usize, li)
                            || !anchored(*l.cells.last().unwrap() as usize, li)
                        {
                            a.unanchored += 1;
                        }
                    }
                    for b in ct.water.bodies.iter().filter(|b| b.class != BodyClass::Wetland)
                    {
                        let m = b.cells.len() as f64;
                        let (mut mx, mut my) = (0.0f64, 0.0f64);
                        for &c in &b.cells {
                            mx += (c as usize % wnx) as f64;
                            my += (c as usize / wnx) as f64;
                        }
                        mx /= m;
                        my /= m;
                        let (mut cxx, mut cyy, mut cxy) = (0.0f64, 0.0f64, 0.0f64);
                        for &c in &b.cells {
                            let dx = (c as usize % wnx) as f64 - mx;
                            let dy = (c as usize / wnx) as f64 - my;
                            cxx += dx * dx;
                            cyy += dy * dy;
                            cxy += dx * dy;
                        }
                        cxx = cxx / m + 1.0 / 12.0;
                        cyy = cyy / m + 1.0 / 12.0;
                        cxy /= m;
                        let half_tr = 0.5 * (cxx + cyy);
                        let disc = (half_tr * half_tr - (cxx * cyy - cxy * cxy))
                            .max(0.0)
                            .sqrt();
                        let l2 = (half_tr - disc).max(1e-9);
                        let aspect = ((half_tr + disc) / l2).sqrt();
                        let width_m = (12.0 * l2).sqrt() * 12.5;
                        let len_m = (12.0 * (half_tr + disc)).sqrt() * 12.5;
                        if (aspect > 4.0 && width_m < 18.0) || aspect > 9.5 {
                            a.skinny += 1;
                        }
                        a.body_len.push(len_m);
                        a.body_ratio.push(aspect);
                    }
                }
                accs
            },
        )
        .reduce(
            || [Acc::new(), Acc::new(), Acc::new()],
            |a, b| {
                let [a0, a1, a2] = a;
                let [b0, b1, b2] = b;
                [a0.merge(b0), a1.merge(b1), a2.merge(b2)]
            },
        );

    println!("water-report over {n} seeds (terrain water = LARGE features only — rivers/streams/lakes/large ponds; strategic small ponds come later in the hole phase. Lowland = hole-scale LAKE FIELDS targeting ~20% ± 10 coverage; rolling/mountain stay sparse, under the atlas totals of 2.9/~2)\n");
    for (i, label) in ["lowland", "rolling", "mountain"].iter().enumerate() {
        let a = &accs[i];
        if a.n == 0 {
            continue;
        }
        let c = a.n as f64;
        let mut errs = a.cov_err.clone();
        errs.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let pct = |v: &Vec<f64>, q: f64| {
            if v.is_empty() {
                0.0
            } else {
                v[((v.len() - 1) as f64 * q) as usize]
            }
        };
        let fmt_cc = |v: f64| {
            if v.is_finite() {
                format!("{:.0}%", 100.0 * v)
            } else {
                "-".into()
            }
        };
        println!(
            "  {label:<9} n={:<4} cov {:.2}%  |err| p50 {:.2} p95 {:.2}  dry {:.0}%  >8% {:.0}%  bodies {:.1}  wetland {:.0}%  flat0-2 {:.1}%  minRoutable {} (river {})  streams {:.1} km  unanchored {}  skinny {}",
            a.n,
            100.0 * a.cov / c,
            pct(&errs, 0.5),
            pct(&errs, 0.95),
            100.0 * a.dry as f64 / c,
            100.0 * a.big as f64 / c,
            a.bodies / c,
            100.0 * a.wetland_frac / c,
            100.0 * a.flat / c,
            fmt_cc(a.min_dry_cc),
            fmt_cc(a.min_dry_cc_river),
            a.streams_len_m / c / 1000.0,
            a.unanchored,
            a.skinny,
        );
        let mut lens = a.body_len.clone();
        lens.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let mut ratios = a.body_ratio.clone();
        ratios.sort_by(|x, y| x.partial_cmp(y).unwrap());
        if !lens.is_empty() {
            println!(
                "            bodies: len p10/50/90 {:.0}/{:.0}/{:.0} m   ratio p50/p90 {:.1}/{:.1}   n={}",
                pct(&lens, 0.10),
                pct(&lens, 0.50),
                pct(&lens, 0.90),
                pct(&ratios, 0.50),
                pct(&ratios, 0.90),
                lens.len(),
            );
        }
    }
    println!("\ngates: routable-CC >= 70% (river courses >= 55%); wetland 0%; unanchored 0; skinny 0; lowland cov ~20% with body len ~400±200 m, ratios ≤ 8.5; rolling/mountain flat0-2 within ±1.0 pt of pre-water baseline (22.6/13.7)");
    ExitCode::SUCCESS
}

/// Run OUR water build on the REAL course landforms and compare against each
/// course's actual water mask — the direct tuning instrument the sampler's
/// synthetic gates can't provide. Uses the course's true water% as the
/// coverage target, mode-typical structural params, and a routing-only
/// erosion pass (Θ=0) so the real terrain is untouched.
/// Probe: how much floodable lake capacity does deep-lowland terrain offer,
/// before and after erosion? Capacity = area in closed basins that qualify
/// as bodies (depth ≥ 0.4 m, ≥ min-body cells). The tuning instrument for
/// the lowland lake-field target (~20% coverage).
fn run_lake_capacity(n: u64) -> ExitCode {
    use golf_terrain::erosion;
    let spec = golf_terrain::macro_spec();
    let nx = spec.nx as usize;
    let ny = spec.ny as usize;
    let a0 = spec.cell_size * spec.cell_size;
    let world = (nx * ny) as f64 * a0;
    let min_cells = (4000.0 / a0).ceil() as usize;

    // Qualifying-basin stats from a lake_depth field: (qual %, total %, top sizes m²/depth).
    let stats = |lake_depth: &[f64]| -> (f64, f64, Vec<(f64, f64)>) {
        let nn = nx * ny;
        let deep: Vec<bool> = (0..nn).map(|i| lake_depth[i] > 1e-3).collect();
        let mut seen = vec![false; nn];
        let mut qual_cells = 0usize;
        let mut total_cells = 0usize;
        let mut tops: Vec<(f64, f64)> = Vec::new();
        let mut stack = Vec::new();
        for s in 0..nn {
            if !deep[s] || seen[s] {
                continue;
            }
            seen[s] = true;
            stack.push(s);
            let mut cells = 0usize;
            let mut dmax = 0.0f64;
            while let Some(i) = stack.pop() {
                cells += 1;
                dmax = dmax.max(lake_depth[i]);
                let (x, y) = (i % nx, i / nx);
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        let (cx, cy) = (x as i32 + dx, y as i32 + dy);
                        if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                            continue;
                        }
                        let c = cy as usize * nx + cx as usize;
                        if deep[c] && !seen[c] {
                            seen[c] = true;
                            stack.push(c);
                        }
                    }
                }
            }
            total_cells += cells;
            if dmax >= 0.4 && cells >= min_cells {
                qual_cells += cells;
                tops.push((cells as f64 * a0, dmax));
            }
        }
        tops.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
        tops.truncate(5);
        (
            100.0 * qual_cells as f64 * a0 / world,
            100.0 * total_cells as f64 * a0 / world,
            tops,
        )
    };

    println!("lake-capacity probe: deep-lowland seeds (lowland_k > 0.8), qualifying = depth ≥ 0.4 m, ≥ {min_cells} cells\n");
    let mut found = 0u64;
    let mut seed = 0u64;
    let mut sum_pre = 0.0;
    let mut sum_post = 0.0;
    while found < n && seed < 100_000 {
        let sp = golf_terrain::sample_params(seed);
        seed += 1;
        if sp.relief_target > 17.0 {
            continue;
        }
        found += 1;
        // Post-erosion (the field the water build sees).
        let mut h = golf_terrain::generate(&spec, sp.seed, &sp.params).heights;
        let outs = erosion::erode(&mut h, None, &sp.erosion);
        let (post_q, _post_t, tops) = stats(&outs.lake_depth.data);
        // Pre-erosion (raw noise, routing-only flood).
        let mut h2 = golf_terrain::generate(&spec, sp.seed, &sp.params).heights;
        let outs2 = erosion::erode(
            &mut h2,
            None,
            &golf_terrain::ErosionParams {
                intensity: 0.0,
                deposition: 0.0,
                diffusion: 0.0,
                hardness_contrast: 0.0,
            },
        );
        let (pre_q, pre_t, _) = stats(&outs2.lake_depth.data);
        sum_pre += pre_q;
        sum_post += post_q;
        // Realized coverage through the full water build (bodies only —
        // subtract the stream share for a like-for-like number).
        let (ct, _) = golf_terrain::generate_course(&golf_terrain::world_spec(128), sp.seed);
        let ncell = ct.water.class.data.len() as f64;
        let body_pct = 100.0
            * ct.water.bodies.iter().map(|b| b.cells.len()).sum::<usize>() as f64
            / ncell;
        let top_str: Vec<String> = tops
            .iter()
            .map(|(a, d)| format!("{:.1}ha/{:.1}m", a / 1e4, d))
            .collect();
        println!(
            "  seed {:<6} R {:>4.1}  skew {:>+5.2}  P {:>4.0}  pre {:>5.1}% (all {:>5.1})  post {:>5.1}%  target {:>4.1}%  realized {:>4.1}%  top: {}",
            sp.seed,
            sp.relief_target,
            sp.skew_target,
            sp.params.base_period,
            pre_q,
            pre_t,
            post_q,
            100.0 * sp.water.coverage,
            body_pct,
            top_str.join(" ")
        );
    }
    println!(
        "\nmean qualifying capacity: pre-erosion {:.1}%   post-erosion {:.1}%   (target ≥ ~25% for 20%-coverage draws)",
        sum_pre / found.max(1) as f64,
        sum_post / found.max(1) as f64
    );
    ExitCode::SUCCESS
}

fn run_water_on_atlas(bin: Option<PathBuf>) -> ExitCode {
    let atlas = match load_atlas(bin) {
        Ok(a) => a,
        Err(c) => return c,
    };
    println!(
        "{:<14} {:<9} {:>7} {:>9} {:>7} {:>7} {:>7}",
        "course", "group", "water%", "realized%", "IoU", "bodies", "real-n"
    );
    println!("(our natural-water build on the real terrain; real course water is partly artificial, so IoU is a floor, not a ceiling)");
    for c in &atlas.courses {
        let (win_w, win_h) = c.window_dims(12.5);
        let mut grid = c.window_grid(win_w, win_h, 12.5);
        // Routing only: no incision/deposition/diffusion on real terrain.
        let outs = golf_terrain::erosion::erode(
            &mut grid,
            None,
            &golf_terrain::ErosionParams {
                intensity: 0.0,
                deposition: 0.0,
                diffusion: 0.0,
                hardness_contrast: 0.0,
            },
        );
        let m = ((c.relief() - 18.0) / (110.0 - 18.0)).clamp(0.0, 1.0);
        let wp = golf_terrain::WaterParams {
            coverage: (c.waterpct as f64 / 100.0).min(0.18),
            stream_area_min: 8.0e5,
            runoff: 1.0,
            pond_share: 0.78,
            wetland_share_cap: 0.7 + (0.25 - 0.7) * m,
            lake_course: c.waterpct > 8.0,
            river_area: 0.0,
            noise_seed: 7,
            ..golf_terrain::WaterParams::NONE
        };
        let layer = golf_terrain::water::build(&mut grid, &outs, &wp);
        // IoU vs the real mask at the same window/resolution.
        let real = c.water_window(win_w, win_h, 12.5);
        let (mut inter, mut union) = (0usize, 0usize);
        for i in 0..real.data.len() {
            let a = real.data[i] >= 0.5;
            let b = layer.class.data[i] != golf_terrain::water::CLASS_DRY;
            if a && b {
                inter += 1;
            }
            if a || b {
                union += 1;
            }
        }
        let real_bodies = {
            let mask: Vec<bool> = real.data.iter().map(|&v| v >= 0.5).collect();
            mask_components(&mask, real.spec.nx as usize, real.spec.ny as usize)
                .into_iter()
                .filter(|cc| cc.len() >= 2)
                .count()
        };
        println!(
            "{:<14} {:<9} {:>7.1} {:>9.1} {:>7.2} {:>7} {:>7}",
            c.key,
            c.group.label(),
            c.waterpct,
            100.0 * layer.coverage,
            inter as f64 / union.max(1) as f64,
            layer.bodies.len(),
            real_bodies,
        );
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Erosion bench + sweep
// ---------------------------------------------------------------------------

/// Per-phase wall times for the macro pipeline over sampled + adversarial
/// seeds. Decides EPOCHS / flood-variant questions with data, and gates the
/// per-seed budget (p50 ≤ 10 ms, p95 ≤ 14 ms).
fn run_erosion_bench(n: u64) {
    use std::time::Instant;
    let mspec = golf_terrain::macro_spec();

    let mut t_noise = Vec::new();
    let mut t_erode = Vec::new();
    let mut t_water = Vec::new();
    let mut t_full = Vec::new();
    let bench = |seed: u64,
                 tp: &TerrainParams,
                 ep: &golf_terrain::ErosionParams,
                 wp: &golf_terrain::WaterParams,
                 tn: &mut Vec<f64>,
                 te: &mut Vec<f64>,
                 tw: &mut Vec<f64>,
                 tf: &mut Vec<f64>| {
        let t0 = Instant::now();
        let mut h = generate(&mspec, seed, tp).heights;
        tn.push(t0.elapsed().as_secs_f64() * 1e3);
        let t1 = Instant::now();
        let outs = golf_terrain::erosion::erode(&mut h, None, ep);
        te.push(t1.elapsed().as_secs_f64() * 1e3);
        let t15 = Instant::now();
        let _ = golf_terrain::water::build(&mut h, &outs, wp);
        tw.push(t15.elapsed().as_secs_f64() * 1e3);
        let t2 = Instant::now();
        let _ = golf_terrain::generate_course_with(&mspec, seed, tp, ep, wp);
        tf.push(t2.elapsed().as_secs_f64() * 1e3);
    };

    for seed in 0..n {
        let sp = golf_terrain::sample_params(seed);
        bench(
            seed,
            &sp.params,
            &sp.erosion,
            &sp.water,
            &mut t_noise,
            &mut t_erode,
            &mut t_water,
            &mut t_full,
        );
    }

    // Differential probes on one representative seed: where does time go?
    {
        use golf_terrain::erosion::{erode_epochs, ErosionParams};
        let sp = golf_terrain::sample_params(3);
        let base = generate(&mspec, 3, &sp.params).heights;
        let probe = |label: &str, ep: ErosionParams, epochs: usize| {
            let reps = 20;
            let t0 = Instant::now();
            for _ in 0..reps {
                let mut h = base.clone();
                let _ = erode_epochs(&mut h, None, &ep, epochs);
            }
            println!(
                "  probe {label:<26} {:>6.2} ms",
                t0.elapsed().as_secs_f64() * 1e3 / reps as f64
            );
        };
        probe("epochs=1", sp.erosion, 1);
        probe("epochs=8", sp.erosion, 8);
        probe("epochs=8, no diffusion", ErosionParams { diffusion: 0.0, ..sp.erosion }, 8);
        probe("epochs=8, no talus+diff", ErosionParams { diffusion: 0.0, intensity: 0.05, deposition: 0.0, ..sp.erosion }, 8);
    }
    // Adversarial: spike fields (e=8) and deep-plateau fields (e=1/8), with a
    // wet high-coverage water draw.
    let (mut a_noise, mut a_erode, mut a_water, mut a_full) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let wet = golf_terrain::WaterParams {
        coverage: 0.15,
        ..golf_terrain::WaterParams::NONE
    };
    for seed in 0..(n / 4).max(8) {
        for redist in [8.0, 0.125] {
            let tp = TerrainParams { amplitude: 150.0, redistribution: redist, ..Default::default() };
            let ep = golf_terrain::ErosionParams { intensity: 1.5, ..Default::default() };
            bench(
                seed,
                &tp,
                &ep,
                &wet,
                &mut a_noise,
                &mut a_erode,
                &mut a_water,
                &mut a_full,
            );
        }
    }

    let pct = |v: &mut Vec<f64>, q: f64| {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[((v.len() - 1) as f64 * q) as usize]
    };
    let report = |name: &str,
                  tn: &mut Vec<f64>,
                  te: &mut Vec<f64>,
                  tw: &mut Vec<f64>,
                  tf: &mut Vec<f64>| {
        println!(
            "{name:<12} noise p50 {:>5.2} ms | erode p50 {:>5.2} p95 {:>5.2} | water p50 {:>5.2} p95 {:>5.2} | full p50 {:>5.2} p95 {:>5.2}",
            pct(tn, 0.5),
            pct(te, 0.5),
            pct(te, 0.95),
            pct(tw, 0.5),
            pct(tw, 0.95),
            pct(tf, 0.5),
            pct(tf, 0.95),
        );
    };
    report("sampled", &mut t_noise, &mut t_erode, &mut t_water, &mut t_full);
    report("adversarial", &mut a_noise, &mut a_erode, &mut a_water, &mut a_full);
    println!(
        "budget: water pass <= 8 ms (incl. smooth SDF outlines + the 2.5 m \
         fine class raster); full pipeline stays ~27-32 ms at {}²",
        golf_terrain::MACRO_N
    );
}

/// Atlas slope-band shares per style group at 15.625 m/px (equal course
/// weight): the tuning targets.
fn atlas_band_targets(atlas: &golf_atlas::Atlas) -> [SlopeAgg; 3] {
    let mut groups = [SlopeAgg::new(), SlopeAgg::new(), SlopeAgg::new()];
    for c in &atlas.courses {
        let t = c.template(15.625);
        let slope = golf_terrain::derive_slope(&t.grid);
        groups[c.group.code() as usize].add(&slope.data);
    }
    groups
}

/// Grid-sweep erosion parameters against the atlas band targets, per mode.
/// Prints the best combos — these fit the sampler's conditioning constants.
fn run_erosion_sweep(n_per_mode: u64) -> ExitCode {
    use rayon::prelude::*;

    let atlas = match load_atlas(None) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let targets = atlas_band_targets(&atlas);

    // Collect n seeds per mode (their noise params come from the sampler;
    // erosion params are forced per combo).
    let mut mode_seeds: [Vec<u64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut seed = 0u64;
    while mode_seeds.iter().any(|v| (v.len() as u64) < n_per_mode) {
        let sp = golf_terrain::sample_params(seed);
        let mi = match sp.mode {
            golf_terrain::ReliefMode::Lowland => 0,
            golf_terrain::ReliefMode::Rolling => 1,
            golf_terrain::ReliefMode::Mountain => 2,
        };
        if (mode_seeds[mi].len() as u64) < n_per_mode {
            mode_seeds[mi].push(seed);
        }
        seed += 1;
    }

    const THETAS: [f64; 5] = [1.4, 2.2, 3.0, 4.0, 5.0];
    const GS: [f64; 4] = [1.0, 2.0, 3.5, 5.0];
    const ALPHAS: [f64; 3] = [1.2, 2.4, 3.4];
    const CS: [f64; 2] = [0.0, 0.6];
    let spec = world_spec(128); // 15.625 m/px — same as the atlas measurement

    // Weighted band-gap objective: double weight on the measured problem
    // bands (0–2% deficit, 12–18/18–25% mountain excess).
    const W: [f64; 8] = [2.0, 1.0, 1.0, 1.0, 1.0, 2.0, 2.0, 1.0];

    println!(
        "erosion sweep: {} combos × {} seeds/mode · objective = Σ w·|gen−atlas| area-pts (lower better)\n",
        THETAS.len() * GS.len() * ALPHAS.len() * CS.len(),
        n_per_mode
    );

    for (mi, label) in ["lowland", "rolling", "mountain"].iter().enumerate() {
        let a = &targets[mi];
        let an = a.n.max(1) as f64;
        let mut rows: Vec<(f64, String)> = Vec::new();
        for &c in &CS {
            for &alpha in &ALPHAS {
                for &g in &GS {
                    for &theta in &THETAS {
                        let ep = golf_terrain::ErosionParams {
                            intensity: theta,
                            deposition: g,
                            diffusion: alpha,
                            hardness_contrast: c,
                        };
                        let agg = mode_seeds[mi]
                            .par_iter()
                            .fold(SlopeAgg::new, |mut acc, &s| {
                                let sp = golf_terrain::sample_params(s);
                                let ct = golf_terrain::generate_course_with(&spec, s, &sp.params, &ep, &golf_terrain::WaterParams::NONE);
                                acc.add(&ct.terrain.slope.data);
                                acc
                            })
                            .reduce(SlopeAgg::new, |mut x, y| {
                                x.merge(&y);
                                x
                            });
                        let gn = agg.n.max(1) as f64;
                        let obj: f64 = W
                            .iter()
                            .zip(agg.bands.iter().zip(a.bands.iter()))
                            .map(|(w, (gb, ab))| w * (100.0 * (gb / gn - ab / an)).abs())
                            .sum();
                        rows.push((
                            obj,
                            format!(
                                "Θ {theta:<4} g {g:<3} α {alpha:<3} c {c:<3} → obj {obj:>6.1} · flat {:>4.1}% (atlas {:>4.1}) · steep12-25 {:>4.1}% ({:>4.1}) · mean {:.4} ({:.4})",
                                100.0 * agg.bands[0] / gn,
                                100.0 * a.bands[0] / an,
                                100.0 * (agg.bands[5] + agg.bands[6]) / gn,
                                100.0 * (a.bands[5] + a.bands[6]) / an,
                                agg.mean_slope / gn,
                                a.mean_slope / an,
                            ),
                        ));
                    }
                }
            }
        }
        rows.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
        println!("== {} ==", label.to_uppercase());
        for (_, line) in rows.iter().take(6) {
            println!("  {line}");
        }
        // Θ response curve: the best combo at each intensity (diagnoses
        // whether erosion is helping or hurting this mode).
        for &theta in &THETAS {
            let pat = format!("Θ {theta:<4}");
            if let Some((_, line)) = rows.iter().find(|(_, l)| l.starts_with(&pat)) {
                println!("  best@{line}");
            }
        }
        println!();
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Slope-grade aggregates: generated seed pool vs the atlas courses
// ---------------------------------------------------------------------------

/// Slope-grade bands (rise/run). The 12% edge is FAIRWAY_MAX_GRADE.
const SLOPE_BANDS: [(f64, f64); 8] = [
    (0.00, 0.02),
    (0.02, 0.04),
    (0.04, 0.06),
    (0.06, 0.08),
    (0.08, 0.12),
    (0.12, 0.18),
    (0.18, 0.25),
    (0.25, f64::INFINITY),
];

#[derive(Default, Clone, Copy)]
struct SlopeAgg {
    n: u64,
    bands: [f64; 8],
    mean_slope: f64,
    playable: f64,
    playable_min: f64,
    playable_max: f64,
}

impl SlopeAgg {
    fn new() -> Self {
        SlopeAgg {
            playable_min: f64::INFINITY,
            playable_max: f64::NEG_INFINITY,
            ..Default::default()
        }
    }

    fn add(&mut self, slopes: &[f64]) {
        let total = slopes.len().max(1) as f64;
        let mut bands = [0u64; 8];
        let mut sum = 0.0;
        let mut play = 0u64;
        for &s in slopes {
            sum += s;
            if s < golf_terrain::FAIRWAY_MAX_GRADE {
                play += 1;
            }
            for (i, &(lo, hi)) in SLOPE_BANDS.iter().enumerate() {
                if s >= lo && s < hi {
                    bands[i] += 1;
                    break;
                }
            }
        }
        for (acc, cnt) in self.bands.iter_mut().zip(bands.iter()) {
            *acc += *cnt as f64 / total;
        }
        let p = play as f64 / total;
        self.mean_slope += sum / total;
        self.playable += p;
        self.playable_min = self.playable_min.min(p);
        self.playable_max = self.playable_max.max(p);
        self.n += 1;
    }

    fn merge(&mut self, o: &SlopeAgg) {
        self.n += o.n;
        for (a, b) in self.bands.iter_mut().zip(o.bands.iter()) {
            *a += *b;
        }
        self.mean_slope += o.mean_slope;
        self.playable += o.playable;
        self.playable_min = self.playable_min.min(o.playable_min);
        self.playable_max = self.playable_max.max(o.playable_max);
    }
}

/// Compare the slope-grade distribution of `n` sampled seeds against the
/// atlas courses, per style band. Each seed / course counts once (equal
/// weight), sampled at the same 15.625 m/px.
fn run_slope_report(n: u64) -> ExitCode {
    use rayon::prelude::*;

    let atlas = match load_atlas(None) {
        Ok(a) => a,
        Err(c) => return c,
    };
    const MPP: f64 = 15.625;

    // Atlas side: per style group.
    let mut atlas_groups = [SlopeAgg::new(), SlopeAgg::new(), SlopeAgg::new(), SlopeAgg::new()];
    for c in &atlas.courses {
        let t = c.template(MPP);
        let slope = golf_terrain::derive_slope(&t.grid);
        let gi = c.group.code() as usize;
        atlas_groups[gi].add(&slope.data);
        atlas_groups[3].add(&slope.data);
    }

    // Generated side: the same seed range the search scans, at the same m/px.
    let spec = world_spec((golf_terrain::WORLD_SIZE / MPP).round() as u32);
    let gen_groups = (0..n)
        .into_par_iter()
        .fold(
            || [SlopeAgg::new(), SlopeAgg::new(), SlopeAgg::new(), SlopeAgg::new()],
            |mut acc, seed| {
                let (ct, sp) = golf_terrain::generate_course(&spec, seed);
                let gi = match sp.mode {
                    golf_terrain::ReliefMode::Lowland => 0,
                    golf_terrain::ReliefMode::Rolling => 1,
                    golf_terrain::ReliefMode::Mountain => 2,
                };
                acc[gi].add(&ct.terrain.slope.data);
                acc[3].add(&ct.terrain.slope.data);
                acc
            },
        )
        .reduce(
            || [SlopeAgg::new(), SlopeAgg::new(), SlopeAgg::new(), SlopeAgg::new()],
            |mut a, b| {
                for i in 0..4 {
                    a[i].merge(&b[i]);
                }
                a
            },
        );

    println!(
        "slope-grade share of area (each course/seed weighted equally, {MPP} m/px)\n\
         atlas = 27 surveyed courses · generated = {n} sampled seeds\n"
    );
    let labels = ["lowland", "rolling", "mountain", "ALL"];
    for gi in [3usize, 0, 1, 2] {
        let a = &atlas_groups[gi];
        let g = &gen_groups[gi];
        println!(
            "== {} — atlas n={}, generated n={} ==",
            labels[gi].to_uppercase(),
            a.n,
            g.n
        );
        println!("{:>10} {:>9} {:>11} {:>7}", "grade", "atlas", "generated", "diff");
        for (i, &(lo, hi)) in SLOPE_BANDS.iter().enumerate() {
            let name = if hi.is_infinite() {
                format!("{:.0}%+", lo * 100.0)
            } else {
                format!("{:.0}-{:.0}%", lo * 100.0, hi * 100.0)
            };
            let av = 100.0 * a.bands[i] / a.n.max(1) as f64;
            let gv = 100.0 * g.bands[i] / g.n.max(1) as f64;
            println!("{name:>10} {av:>8.1}% {gv:>10.1}% {:>+6.1}", gv - av);
        }
        println!(
            "{:>10} {:>8.4} {:>10.4}",
            "mean", a.mean_slope / a.n.max(1) as f64, g.mean_slope / g.n.max(1) as f64
        );
        println!(
            "{:>10} {:>8.1}% {:>10.1}%   (gen range {:.0}–{:.0}%)",
            "play<=12%",
            100.0 * a.playable / a.n.max(1) as f64,
            100.0 * g.playable / g.n.max(1) as f64,
            100.0 * g.playable_min,
            100.0 * g.playable_max,
        );
        println!();
    }
    println!(
        "reference: 12% = FAIRWAY_MAX_GRADE (steepest grade treated as fairway-buildable);\n\
         real-course guidance clusters fairways at 0-8% with most play under 12%."
    );
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Hole routing: atlas reference stats, generated-report gates, visual dumps
// ---------------------------------------------------------------------------

fn pctile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let i = q * (sorted.len() - 1) as f64;
    let i0 = i as usize;
    let i1 = (i0 + 1).min(sorted.len() - 1);
    sorted[i0] + (sorted[i1] - sorted[i0]) * (i - i0 as f64)
}

fn sorted(mut v: Vec<f64>) -> Vec<f64> {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

/// Measured atlas hole/routing statistics — the reference the routing
/// sampler, report gates, and the analysis chart are fit against.
struct AtlasRoutingStats {
    n_courses: usize,
    len_by_par: [Vec<f64>; 3],
    segs_by_par: [Vec<f64>; 3],
    /// Signed tee→green elevation change per par (m, + = uphill).
    dz_by_par: [Vec<f64>; 3],
    /// Tee elevation minus path elevation 137 m out (drive landing), m.
    drop_by_par: [Vec<f64>; 3],
    turns: Vec<f64>,
    walks: Vec<f64>,
    nine_totals: Vec<f64>,
    nines: u32,
    par36: u32,
    opener3: u32,
    opener5: u32,
    closer3: u32,
    b2b3: u32,
    b2b5: u32,
    comp_counts: std::collections::BTreeMap<(u32, u32, u32), u32>,
}

/// Elevation along an atlas hole: lat/lon → the survey frame (x from west,
/// y from north) → bilinear on the course heightmap.
fn atlas_hole_elevs(c: &golf_atlas::Course, pts_m: &[golf_core::math::Vec2], xy: &[(f64, f64)]) -> (f64, f64) {
    let z_at = |i: usize| c.sample(xy[i].0, xy[i].1);
    let z_tee = z_at(0);
    let z_green = z_at(xy.len() - 1);
    // Path point 137 m (150 yd) from the tee — the drive landing zone.
    let mut acc = 0.0;
    let mut z_drive = z_green;
    'outer: for k in 1..pts_m.len() {
        let seg = pts_m[k - 1].distance(pts_m[k]);
        if acc + seg >= 137.0 && seg > 0.0 {
            let t = (137.0 - acc) / seg;
            let x = xy[k - 1].0 + (xy[k].0 - xy[k - 1].0) * t;
            let y = xy[k - 1].1 + (xy[k].1 - xy[k - 1].1) * t;
            z_drive = c.sample(x, y);
            break 'outer;
        }
        acc += seg;
    }
    (z_green - z_tee, z_tee - z_drive)
}

fn measure_atlas_routing(atlas: &golf_atlas::Atlas) -> AtlasRoutingStats {
    let mut st = AtlasRoutingStats {
        n_courses: atlas.courses.len(),
        len_by_par: [Vec::new(), Vec::new(), Vec::new()],
        segs_by_par: [Vec::new(), Vec::new(), Vec::new()],
        dz_by_par: [Vec::new(), Vec::new(), Vec::new()],
        drop_by_par: [Vec::new(), Vec::new(), Vec::new()],
        turns: Vec::new(),
        walks: Vec::new(),
        nine_totals: Vec::new(),
        nines: 0,
        par36: 0,
        opener3: 0,
        opener5: 0,
        closer3: 0,
        b2b3: 0,
        b2b5: 0,
        comp_counts: Default::default(),
    };

    for c in &atlas.courses {
        // Local equirectangular projection (differences only).
        let lat_mid = (c.bbox[0] + c.bbox[2]) * 0.5;
        let kx = 111_320.0 * (lat_mid * std::f64::consts::PI / 180.0).cos();
        let ky = 110_540.0;
        let to_m = |lat: f64, lon: f64| golf_core::math::Vec2::new(lon * kx, lat * ky);
        // Survey frame for elevation sampling (orientation-agnostic).
        let (lat_n, lat_s) = (c.bbox[0].max(c.bbox[2]), c.bbox[0].min(c.bbox[2]));
        let (lon_w, lon_e) = (c.bbox[1].min(c.bbox[3]), c.bbox[1].max(c.bbox[3]));
        let to_xy = |lat: f64, lon: f64| -> (f64, f64) {
            (
                (lon - lon_w) / (lon_e - lon_w).max(1e-12) * c.wm,
                (lat_n - lat) / (lat_n - lat_s).max(1e-12) * c.hm,
            )
        };

        let mut holes: Vec<&golf_atlas::Hole> = c.holes.iter().collect();
        holes.sort_by_key(|h| h.ref_no);

        let hole_pts = |h: &golf_atlas::Hole| -> Vec<golf_core::math::Vec2> {
            h.pts_ll.iter().map(|&(la, lo)| to_m(la, lo)).collect()
        };

        for h in &holes {
            let pts = hole_pts(h);
            if pts.len() < 2 {
                continue;
            }
            let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
            if let Some(par @ 3..=5) = h.par {
                let bi = (par - 3) as usize;
                st.len_by_par[bi].push(len);
                st.segs_by_par[bi].push((pts.len() - 1) as f64);
                let xy: Vec<(f64, f64)> =
                    h.pts_ll.iter().map(|&(la, lo)| to_xy(la, lo)).collect();
                let (dz, drop) = atlas_hole_elevs(c, &pts, &xy);
                st.dz_by_par[bi].push(dz);
                st.drop_by_par[bi].push(drop);
            }
            for w in pts.windows(3) {
                let a = (w[1] - w[0]).normalized();
                let b = (w[2] - w[1]).normalized();
                let t = golf_core::math::acos(a.dot(b).clamp(-1.0, 1.0));
                st.turns.push(t * 180.0 / std::f64::consts::PI);
            }
        }
        for w in holes.windows(2) {
            let (a, b) = (hole_pts(w[0]), hole_pts(w[1]));
            if a.len() >= 2 && b.len() >= 2 && w[1].ref_no == w[0].ref_no + 1 {
                st.walks.push(a.last().unwrap().distance(b[0]));
            }
        }
        // Nines: ref 1..=9 and 10..=18 with full par data.
        for base in [1u32, 10] {
            let nine: Vec<&&golf_atlas::Hole> = holes
                .iter()
                .filter(|h| h.ref_no >= base && h.ref_no < base + 9)
                .collect();
            if nine.len() != 9 || nine.iter().any(|h| h.par.is_none()) {
                continue;
            }
            st.nines += 1;
            let pars: Vec<u32> = nine.iter().map(|h| h.par.unwrap()).collect();
            let psum: u32 = pars.iter().sum();
            if psum == 36 {
                st.par36 += 1;
            }
            let total: f64 = nine
                .iter()
                .map(|h| {
                    let pts = hole_pts(h);
                    pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>()
                })
                .sum();
            st.nine_totals.push(total);
            if pars[0] == 3 {
                st.opener3 += 1;
            }
            if pars[0] == 5 {
                st.opener5 += 1;
            }
            if pars[8] == 3 {
                st.closer3 += 1;
            }
            if pars.windows(2).any(|w| w[0] == 3 && w[1] == 3) {
                st.b2b3 += 1;
            }
            if pars.windows(2).any(|w| w[0] == 5 && w[1] == 5) {
                st.b2b5 += 1;
            }
            let n3 = pars.iter().filter(|&&p| p == 3).count() as u32;
            let n4 = pars.iter().filter(|&&p| p == 4).count() as u32;
            let n5 = pars.iter().filter(|&&p| p == 5).count() as u32;
            *st.comp_counts.entry((n3, n4, n5)).or_insert(0) += 1;
        }
    }
    st
}

fn run_atlas_routing(bin: Option<PathBuf>) -> ExitCode {
    let atlas = match load_atlas(bin) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let AtlasRoutingStats {
        n_courses,
        len_by_par,
        segs_by_par,
        dz_by_par,
        drop_by_par,
        turns,
        walks,
        nine_totals,
        nines,
        par36,
        opener3,
        opener5,
        closer3: _,
        b2b3,
        b2b5,
        comp_counts,
    } = measure_atlas_routing(&atlas);

    println!("atlas routing reference ({n_courses} courses)\n");
    println!("hole lengths (m)         p10   p50   p90   n   segs(mean)");
    for (i, name) in ["par 3", "par 4", "par 5"].iter().enumerate() {
        let l = sorted(len_by_par[i].clone());
        let segs_mean =
            segs_by_par[i].iter().sum::<f64>() / segs_by_par[i].len().max(1) as f64;
        println!(
            "  {name}              {:6.0}{:6.0}{:6.0} {:4}   {:.2}",
            pctile(&l, 0.10),
            pctile(&l, 0.50),
            pctile(&l, 0.90),
            l.len(),
            segs_mean
        );
    }
    println!("\nhole Δz tee→green (m, + uphill)   p10   p50   p90   min   max");
    for (i, name) in ["par 3", "par 4", "par 5"].iter().enumerate() {
        let d = sorted(dz_by_par[i].clone());
        println!(
            "  {name}                        {:6.1}{:6.1}{:6.1}{:6.1}{:6.1}",
            pctile(&d, 0.10),
            pctile(&d, 0.50),
            pctile(&d, 0.90),
            pctile(&d, 0.0),
            pctile(&d, 1.0)
        );
    }
    println!("drive drop tee→137 m (m, + = elevated tee)  p10   p50   p90   max");
    for (i, name) in ["par 3", "par 4", "par 5"].iter().enumerate() {
        let d = sorted(drop_by_par[i].clone());
        println!(
            "  {name}                                  {:6.1}{:6.1}{:6.1}{:6.1}",
            pctile(&d, 0.10),
            pctile(&d, 0.50),
            pctile(&d, 0.90),
            pctile(&d, 1.0)
        );
    }
    {
        let all_dz: Vec<f64> = dz_by_par.iter().flatten().copied().collect();
        let n = all_dz.len().max(1);
        let up10 = all_dz.iter().filter(|&&d| d > 10.0).count();
        let up20 = all_dz.iter().filter(|&&d| d > 20.0).count();
        let dn20 = all_dz.iter().filter(|&&d| d < -20.0).count();
        println!(
            "uphill >10 m: {:.1}%   uphill >20 m: {:.1}%   downhill >20 m: {:.1}%   (n={})",
            100.0 * up10 as f64 / n as f64,
            100.0 * up20 as f64 / n as f64,
            100.0 * dn20 as f64 / n as f64,
            n
        );
    }

    let t = sorted(turns);
    println!(
        "\ndogleg turns (deg): p50 {:.0}  p90 {:.0}  max {:.0}   (n={})",
        pctile(&t, 0.50),
        pctile(&t, 0.90),
        pctile(&t, 1.0),
        t.len()
    );
    let w = sorted(walks);
    println!(
        "green→next-tee walks (m): p10 {:.0}  p50 {:.0}  p90 {:.0}  max {:.0}",
        pctile(&w, 0.10),
        pctile(&w, 0.50),
        pctile(&w, 0.90),
        pctile(&w, 1.0)
    );
    let nt = sorted(nine_totals);
    println!(
        "nine total length (m): p10 {:.0}  p50 {:.0}  p90 {:.0}",
        pctile(&nt, 0.10),
        pctile(&nt, 0.50),
        pctile(&nt, 0.90)
    );
    println!("\nnines: {nines} (par-36: {par36})");
    for ((n3, n4, n5), cnt) in &comp_counts {
        println!(
            "  mix {n3}-{n4}-{n5}: {cnt} ({:.0}%)",
            100.0 * *cnt as f64 / nines.max(1) as f64
        );
    }
    let pc = |c: u32| 100.0 * c as f64 / nines.max(1) as f64;
    println!(
        "  opener par3 {:.0}%  opener par5 {:.0}%  b2b par3 {:.0}%  b2b par5 {:.0}%",
        pc(opener3),
        pc(opener5),
        pc(b2b3),
        pc(b2b5)
    );
    ExitCode::SUCCESS
}

struct RoutedSeed {
    seed: u64,
    mode: &'static str,
    routed: bool,
    reason: &'static str,
    stage: u8,
    violations: usize,
    pars: [u8; 9],
    lens: [f64; 9],
    walk_lens: [f64; 9],
    /// Tee→green elevation change per hole (m, + uphill).
    dzs: [f64; 9],
    /// Tee elevation over the 137 m drive point per hole (m).
    drops: [f64; 9],
    total_len: f64,
    route_ms: f64,
}

/// Generate + route one seed, measuring everything the report/analysis need.
fn route_one(seed: u64) -> RoutedSeed {
    let (ct, sp) = golf_terrain::generate_course(&golf_terrain::macro_spec(), seed);
    let mode = sp.mode.label();
    let t0 = std::time::Instant::now();
    match golf_routing::route(&ct, seed) {
        Ok(r) => {
            let ms = t0.elapsed().as_secs_f64() * 1e3;
            let viol = golf_routing::verify(&ct, &r);
            let mut lens = [0.0f64; 9];
            let mut wl = [0.0f64; 9];
            let mut dzs = [0.0f64; 9];
            let mut drops = [0.0f64; 9];
            for i in 0..9 {
                lens[i] = r.holes[i].len;
                wl[i] = r.holes[i].walk_len;
                dzs[i] = golf_routing::geometry::hole_dz(&ct, &r.holes[i].pts);
                drops[i] = golf_routing::geometry::drive_drop(&ct, &r.holes[i].pts);
            }
            RoutedSeed {
                seed,
                mode,
                routed: true,
                reason: "",
                stage: r.relax_stage,
                violations: viol.len(),
                pars: r.par_seq,
                lens,
                walk_lens: wl,
                dzs,
                drops,
                total_len: r.total_len,
                route_ms: ms,
            }
        }
        Err(e) => RoutedSeed {
            seed,
            mode,
            routed: false,
            reason: e.reason,
            stage: 255,
            violations: 0,
            pars: [0; 9],
            lens: [0.0; 9],
            walk_lens: [0.0; 9],
            dzs: [0.0; 9],
            drops: [0.0; 9],
            total_len: 0.0,
            route_ms: t0.elapsed().as_secs_f64() * 1e3,
        },
    }
}

/// Generate + route `n` seeds, verify every hard constraint, and gate the
/// realized distributions against the atlas reference and the design targets.
fn run_routing_report(n: u64) -> ExitCode {
    use rayon::prelude::*;
    let results: Vec<RoutedSeed> = (0..n).into_par_iter().map(route_one).collect();

    let routed: Vec<&RoutedSeed> = results.iter().filter(|r| r.routed).collect();
    let nr = routed.len();
    let routable = nr as f64 / n.max(1) as f64;
    let total_viol: usize = routed.iter().map(|r| r.violations).sum();

    // Sequence statistics.
    let mut comp = std::collections::BTreeMap::<u8, usize>::new();
    let (mut opener3, mut closer3, mut b2b3, mut b2b5) = (0usize, 0usize, 0usize, 0usize);
    for r in &routed {
        let n3 = r.pars.iter().filter(|&&p| p == 3).count() as u8;
        *comp.entry(n3).or_insert(0) += 1;
        if r.pars[0] == 3 {
            opener3 += 1;
        }
        if r.pars[8] == 3 {
            closer3 += 1;
        }
        if r.pars.windows(2).any(|w| w[0] == 3 && w[1] == 3) {
            b2b3 += 1;
        }
        if r.pars.windows(2).any(|w| w[0] == 5 && w[1] == 5) {
            b2b5 += 1;
        }
    }
    let f = |c: usize| c as f64 / nr.max(1) as f64;

    // Per-par lengths + elevations + within-course balance.
    let mut lens3 = Vec::new();
    let mut lens4 = Vec::new();
    let mut lens5 = Vec::new();
    let mut dz_par: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut drop_all: Vec<f64> = Vec::new();
    let mut balanced = 0usize;
    let mut walk_all: Vec<f64> = Vec::new();
    let mut totals: Vec<f64> = Vec::new();
    for r in &routed {
        let mut qmin = f64::INFINITY;
        let mut qmax = f64::NEG_INFINITY;
        for i in 0..9 {
            let (lo, hi) = match r.pars[i] {
                3 => {
                    lens3.push(r.lens[i]);
                    (120.0, 260.0)
                }
                4 => {
                    lens4.push(r.lens[i]);
                    (265.0, 490.0)
                }
                _ => {
                    lens5.push(r.lens[i]);
                    (425.0, 600.0)
                }
            };
            dz_par[(r.pars[i] - 3) as usize].push(r.dzs[i]);
            drop_all.push(r.drops[i]);
            let q = ((r.lens[i] - lo) / (hi - lo)).clamp(0.0, 1.0);
            qmin = qmin.min(q);
            qmax = qmax.max(q);
        }
        if qmax - qmin >= 0.45 {
            balanced += 1;
        }
        walk_all.extend_from_slice(&r.walk_lens);
        totals.push(r.total_len);
    }
    let l3 = sorted(lens3);
    let l4 = sorted(lens4);
    let l5 = sorted(lens5);
    let wa = sorted(walk_all);
    let tt = sorted(totals);
    let ms = sorted(routed.iter().map(|r| r.route_ms).collect());
    let balance_share = balanced as f64 / nr.max(1) as f64;

    let mut stage_hist = [0usize; 6];
    for r in &routed {
        stage_hist[(r.stage as usize).min(5)] += 1;
    }
    let mut fail_reasons = std::collections::BTreeMap::<&str, usize>::new();
    let mut fail_modes = std::collections::BTreeMap::<&str, usize>::new();
    let mut fail_seeds: Vec<u64> = Vec::new();
    for r in results.iter().filter(|r| !r.routed) {
        *fail_reasons.entry(r.reason).or_insert(0) += 1;
        *fail_modes.entry(r.mode).or_insert(0) += 1;
        fail_seeds.push(r.seed);
    }

    println!("routing report over {n} seeds\n");
    println!("routable: {nr}/{n} ({:.1}%)   relax stages 0..5: {}/{}/{}/{}/{}/{}",
        100.0 * routable, stage_hist[0], stage_hist[1], stage_hist[2], stage_hist[3],
        stage_hist[4], stage_hist[5]);
    for (r, c) in &fail_reasons {
        println!("  fail: {r} ×{c}");
    }
    if !fail_seeds.is_empty() {
        let modes: Vec<String> = fail_modes.iter().map(|(m, c)| format!("{m} ×{c}")).collect();
        println!("  failed modes: {}", modes.join(", "));
        if fail_seeds.len() <= 40 {
            println!("  failed seeds: {fail_seeds:?}");
        }
    }
    println!("hard-constraint violations on routed seeds: {total_viol}");
    println!("\npar mix: 2-5-2 {:.1}%  1-7-1 {:.1}%  3-3-3 {:.1}%  0-9-0 {:.1}%  4-1-4 {:.1}%",
        100.0 * f(*comp.get(&2).unwrap_or(&0)),
        100.0 * f(*comp.get(&1).unwrap_or(&0)),
        100.0 * f(*comp.get(&3).unwrap_or(&0)),
        100.0 * f(*comp.get(&0).unwrap_or(&0)),
        100.0 * f(*comp.get(&4).unwrap_or(&0)));
    println!("rarities: opener par3 {:.1}%  closer par3 {:.1}%  b2b par3 {:.1}%  b2b par5 {:.1}%",
        100.0 * f(opener3), 100.0 * f(closer3), 100.0 * f(b2b3), 100.0 * f(b2b5));
    println!("\nlengths (m)      p10   p50   p90     atlas p10/p50/p90");
    println!("  par 3        {:6.0}{:6.0}{:6.0}     143/176/216",
        pctile(&l3, 0.10), pctile(&l3, 0.50), pctile(&l3, 0.90));
    println!("  par 4        {:6.0}{:6.0}{:6.0}     321/395/447",
        pctile(&l4, 0.10), pctile(&l4, 0.50), pctile(&l4, 0.90));
    println!("  par 5        {:6.0}{:6.0}{:6.0}     453/507/555",
        pctile(&l5, 0.10), pctile(&l5, 0.50), pctile(&l5, 0.90));
    println!("nine total (m): p10 {:.0}  p50 {:.0}  p90 {:.0}   (atlas 3009/3274/3461)",
        pctile(&tt, 0.10), pctile(&tt, 0.50), pctile(&tt, 0.90));
    println!("length balance (quantile spread ≥ 0.45): {:.1}% of courses", 100.0 * balance_share);
    println!("\nhole Δz (m)      p10   p50   p90     atlas p10/p50/p90");
    println!("  par 3        {:6.1}{:6.1}{:6.1}     -5.9/-0.4/+5.3",
        pctile(&sorted(dz_par[0].clone()), 0.10),
        pctile(&sorted(dz_par[0].clone()), 0.50),
        pctile(&sorted(dz_par[0].clone()), 0.90));
    println!("  par 4        {:6.1}{:6.1}{:6.1}     -13.3/-0.1/+9.5",
        pctile(&sorted(dz_par[1].clone()), 0.10),
        pctile(&sorted(dz_par[1].clone()), 0.50),
        pctile(&sorted(dz_par[1].clone()), 0.90));
    println!("  par 5        {:6.1}{:6.1}{:6.1}     -14.1/0.0/+17.1",
        pctile(&sorted(dz_par[2].clone()), 0.10),
        pctile(&sorted(dz_par[2].clone()), 0.50),
        pctile(&sorted(dz_par[2].clone()), 0.90));
    let all_dz: Vec<f64> = dz_par.iter().flatten().copied().collect();
    let up10 = all_dz.iter().filter(|&&d| d > 10.0).count() as f64 / all_dz.len().max(1) as f64;
    let up20 = all_dz.iter().filter(|&&d| d > 20.0).count() as f64 / all_dz.len().max(1) as f64;
    let dr = sorted(drop_all.clone());
    println!("uphill >10 m: {:.1}% (atlas 9.7)   >20 m: {:.1}% (atlas 1.5)",
        100.0 * up10, 100.0 * up20);
    println!("drive drop (m): p10 {:.1}  p50 {:.1}  p90 {:.1}   (atlas -3.4/+1.1/+7.0)",
        pctile(&dr, 0.10), pctile(&dr, 0.50), pctile(&dr, 0.90));
    println!("\nwalks (m): p50 {:.0}  p90 {:.0}  max {:.0}   (atlas 68/128)",
        pctile(&wa, 0.50), pctile(&wa, 0.90), pctile(&wa, 1.0));
    println!("route time (ms): p50 {:.0}  p95 {:.0}  max {:.0}",
        pctile(&ms, 0.50), pctile(&ms, 0.95), pctile(&ms, 1.0));

    // Gates.
    let mut ok = true;
    let mut gate = |name: &str, pass: bool| {
        println!("{} {}", if pass { "OK  " } else { "FAIL" }, name);
        ok &= pass;
    };
    println!("\ngates:");
    gate("routable ≥ 95%", routable >= 0.95);
    gate("zero hard-constraint violations", total_viol == 0);
    gate("mix 2-5-2 in 55–70%", (0.55..=0.70).contains(&f(*comp.get(&2).unwrap_or(&0))));
    gate("mix 3-3-3 in 9–17%", (0.09..=0.17).contains(&f(*comp.get(&3).unwrap_or(&0))));
    gate("opener par3 in 1.5–6.5%", (0.015..=0.065).contains(&f(opener3)));
    gate("b2b par3 ≤ 5%", f(b2b3) <= 0.05);
    gate("b2b par5 in 2–9%", (0.02..=0.09).contains(&f(b2b5)));
    gate("par3 p50 in 155–200 m", (155.0..=200.0).contains(&pctile(&l3, 0.50)));
    gate("par4 p50 in 365–425 m", (365.0..=425.0).contains(&pctile(&l4, 0.50)));
    gate("par5 p50 in 480–535 m", (480.0..=535.0).contains(&pctile(&l5, 0.50)));
    gate("nine total p50 in 3050–3450 m", (3050.0..=3450.0).contains(&pctile(&tt, 0.50)));
    gate("length balance ≥ 90% of courses", balance_share >= 0.90);
    let dz_all_sorted = sorted(all_dz.clone());
    gate("hole Δz p50 in −4..+3 m", (-4.0..=3.0).contains(&pctile(&dz_all_sorted, 0.50)));
    gate("uphill >10 m ≤ 18% of holes", up10 <= 0.18);
    gate("uphill >20 m ≤ 4% of holes", up20 <= 0.04);
    gate("drive drop p50 in −1..+4 m", (-1.0..=4.0).contains(&pctile(&dr, 0.50)));
    gate("walk p50 ≤ 90 m", pctile(&wa, 0.50) <= 90.0);
    gate("walk p90 ≤ 170 m", pctile(&wa, 0.90) <= 170.0);
    gate("route time p50 ≤ 250 ms", pctile(&ms, 0.50) <= 250.0);
    gate("route time p95 ≤ 400 ms", pctile(&ms, 0.95) <= 400.0);

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Write a generated course's composed heightfield + water mask to a plain
/// binary grid (magic `HG01`, u32 ny, u32 nx, f32 cell, then ny*nx f32 heights
/// row 0 = north, then ny*nx u8 water). Consumed by `tools/metrics/synthetic.py`.
/// Sample a course's composed surface + water mask onto a centred `extent`
/// window at `cell` m and write the HG01 grid.
fn write_heightgrid(ct: &golf_terrain::CourseTerrain, out: &Path, cell: f64, extent: f64) {
    use golf_core::math::Vec2;
    use std::io::Write;
    let world = world_spec(1).cell_size; // WORLD_SIZE
    let n = (extent / cell).round().max(8.0) as u32;
    let lo = (world - extent) / 2.0;
    let mut heights: Vec<f32> = Vec::with_capacity((n * n) as usize);
    let mut water: Vec<u8> = Vec::with_capacity((n * n) as usize);
    for iy in 0..n {
        let y = lo + extent - (iy as f64 + 0.5) * cell; // north-first (row 0 = north)
        for ix in 0..n {
            let x = lo + (ix as f64 + 0.5) * cell;
            let p = Vec2::new(x, y);
            heights.push(ct.height_at(p) as f32);
            water.push(u8::from(ct.water.class_fine_at(p) != 0));
        }
    }
    if let Some(d) = out.parent() {
        std::fs::create_dir_all(d).ok();
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    f.write_all(b"HG01").unwrap();
    f.write_all(&n.to_le_bytes()).unwrap();
    f.write_all(&n.to_le_bytes()).unwrap();
    f.write_all(&(cell as f32).to_le_bytes()).unwrap();
    for h in &heights {
        f.write_all(&h.to_le_bytes()).unwrap();
    }
    f.write_all(&water).unwrap();
}

fn run_dump_heightgrid(out: &Path, seed: u64, cell: f64, extent: f64) -> ExitCode {
    let (ct, _sp) = golf_terrain::generate_course(&world_spec(256), seed);
    write_heightgrid(&ct, out, cell, extent);
    let n = (extent / cell).round().max(8.0) as u32;
    println!("wrote {} ({n}x{n} @ {cell} m, extent {extent} m) seed {seed}", out.display());
    ExitCode::SUCCESS
}

#[derive(serde::Deserialize)]
struct FwdItem {
    out: String,
    seed: u64,
    cell: f64,
    extent: f64,
    // Absent => use the CURRENT seed sampler's params (baseline), still dry.
    terrain: Option<golf_terrain::TerrainParams>,
    erosion: Option<golf_terrain::ErosionParams>,
}

/// Calibration forward pass: generate each manifest item with EXPLICIT params
/// (`generate_course_with`, dry water = pure morphology) and write its HG01
/// grid. Parallel across items.
fn run_forward_grid(manifest: &Path) -> ExitCode {
    use rayon::prelude::*;
    let text = match std::fs::read_to_string(manifest) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("cannot read manifest {}: {e}", manifest.display());
            return ExitCode::FAILURE;
        }
    };
    let items: Vec<FwdItem> = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("bad manifest JSON: {e}");
            return ExitCode::FAILURE;
        }
    };
    let n = items.len();
    let spec = world_spec(256);
    items.par_iter().for_each(|it| {
        let (tp, ep) = match (&it.terrain, &it.erosion) {
            (Some(t), Some(e)) => (*t, *e),
            _ => {
                // baseline: the current seed sampler's params (dry morphology)
                let sp = golf_terrain::sample_params(it.seed);
                (sp.params, sp.erosion)
            }
        };
        let ct = golf_terrain::generate_course_with(
            &spec, it.seed, &tp, &ep, &golf_terrain::WaterParams::NONE);
        write_heightgrid(&ct, Path::new(&it.out), it.cell, it.extent);
    });
    println!("forward-grid: wrote {n} grids");
    ExitCode::SUCCESS
}

fn run_dump_routed(dir: &Path, seeds: &[u64]) -> ExitCode {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    println!("writing to {}", dir.display());
    for &seed in seeds {
        let (ct, sp) = golf_terrain::generate_course(&golf_terrain::macro_spec(), seed);
        let mut img = golf_viz::render_height_water(&ct.macro_heights, &ct.water, 768, None);
        match golf_routing::route(&ct, seed) {
            Ok(r) => {
                golf_viz::draw_routing(&mut img, &r, golf_terrain::WORLD_SIZE);
                let pars: Vec<String> = r.par_seq.iter().map(|p| p.to_string()).collect();
                println!(
                    "seed {seed} [{}]: par {} | {:.0} m + {:.0} m walks | stage {} | holes: {}",
                    sp.mode.label(),
                    pars.join("-"),
                    r.total_len,
                    r.total_walk,
                    r.relax_stage,
                    r.holes
                        .iter()
                        .map(|h| format!("{:.0}", h.len))
                        .collect::<Vec<_>>()
                        .join("/")
                );
            }
            Err(e) => println!("seed {seed} [{}]: UNROUTABLE ({})", sp.mode.label(), e.reason),
        }
        img.save(dir.join(format!("routed{seed}.png"))).unwrap();
    }
    ExitCode::SUCCESS
}

/// Routing analysis chart: generated distributions vs the atlas ground truth
/// (par mix + rarities, yardages per par, hole elevation changes, drive
/// drops, nine totals, walks), stamped with every pipeline version. Writes
/// `output/routing_analysis.svg`.
fn run_routing_analysis(n: u64, bin: Option<PathBuf>) -> ExitCode {
    use rayon::prelude::*;
    let atlas = match load_atlas(bin) {
        Ok(a) => a,
        Err(c) => return c,
    };
    let at = measure_atlas_routing(&atlas);
    println!("routing {n} seeds…");
    let results: Vec<RoutedSeed> = (0..n).into_par_iter().map(route_one).collect();
    let routed: Vec<&RoutedSeed> = results.iter().filter(|r| r.routed).collect();
    let nr = routed.len();

    // Generated distributions.
    let mut comp = std::collections::BTreeMap::<u8, usize>::new();
    let (mut opener3, mut closer3, mut b2b3, mut b2b5) = (0usize, 0usize, 0usize, 0usize);
    let mut g_len: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut g_dz: [Vec<f64>; 3] = [Vec::new(), Vec::new(), Vec::new()];
    let mut g_drop: Vec<f64> = Vec::new();
    let mut g_tot: Vec<f64> = Vec::new();
    let mut g_walk: Vec<f64> = Vec::new();
    for r in &routed {
        let n3 = r.pars.iter().filter(|&&p| p == 3).count() as u8;
        *comp.entry(n3).or_insert(0) += 1;
        if r.pars[0] == 3 {
            opener3 += 1;
        }
        if r.pars[8] == 3 {
            closer3 += 1;
        }
        if r.pars.windows(2).any(|w| w[0] == 3 && w[1] == 3) {
            b2b3 += 1;
        }
        if r.pars.windows(2).any(|w| w[0] == 5 && w[1] == 5) {
            b2b5 += 1;
        }
        for i in 0..9 {
            let bi = (r.pars[i] - 3) as usize;
            g_len[bi].push(r.lens[i]);
            g_dz[bi].push(r.dzs[i]);
            g_drop.push(r.drops[i]);
        }
        g_tot.push(r.total_len);
        g_walk.extend_from_slice(&r.walk_lens);
    }
    let gpct = |c: usize| 100.0 * c as f64 / nr.max(1) as f64;

    // Atlas: mixes among par-36 nines (n₃ = n₅ is forced there), rarities
    // over all nines.
    let mut a_mix = [0.0f64; 5]; // by n3 = 0..4
    for (&(n3, n4, n5), &c) in &at.comp_counts {
        if 3 * n3 + 4 * n4 + 5 * n5 == 36 && (n3 as usize) < 5 {
            a_mix[n3 as usize] += c as f64;
        }
    }
    for m in &mut a_mix {
        *m = 100.0 * *m / at.par36.max(1) as f64;
    }
    let apct = |c: u32| 100.0 * c as f64 / at.nines.max(1) as f64;

    // ---- SVG ----
    let mut s = String::with_capacity(64 * 1024);
    let esc = |t: &str| t.replace('&', "&amp;").replace('<', "&lt;");
    s.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"960\" height=\"1010\" \
         viewBox=\"0 0 960 1010\" font-family=\"ui-monospace,Menlo,monospace\">\n\
         <rect width=\"960\" height=\"1010\" fill=\"#14171a\"/>\n\
         <text x=\"30\" y=\"36\" font-size=\"19\" fill=\"#eceff2\">Routed nines vs the Parkland Atlas</text>\n\
         <text x=\"30\" y=\"57\" font-size=\"11.5\" fill=\"#9aa4ad\">{}</text>\n",
        esc(&format!(
            "sampler v{} · erosion v{} · water v{} · routing v{} · {} seeds ({} routed, {:.1}%) · atlas fp {:016x} ({} courses, {} nines, {} par-36)",
            golf_terrain::SAMPLER_VERSION,
            golf_terrain::EROSION_VERSION,
            golf_terrain::WATER_VERSION,
            golf_routing::ROUTING_VERSION,
            n,
            nr,
            100.0 * nr as f64 / n.max(1) as f64,
            atlas.fingerprint,
            at.n_courses,
            at.nines,
            at.par36
        ))
    ));
    const GEN: &str = "#4aa3e8";
    const ATL: &str = "#c8b269";

    let panel_title = |s: &mut String, y: f64, t: &str| {
        s.push_str(&format!(
            "<text x=\"30\" y=\"{y}\" font-size=\"13\" fill=\"#dfe5ea\">{t}</text>\n"
        ));
    };
    // Legend.
    s.push_str(&format!(
        "<rect x=\"700\" y=\"26\" width=\"11\" height=\"11\" fill=\"{GEN}\"/>\
         <text x=\"716\" y=\"36\" font-size=\"11.5\" fill=\"#c6cdd3\">generated</text>\
         <rect x=\"806\" y=\"26\" width=\"11\" height=\"11\" fill=\"{ATL}\"/>\
         <text x=\"822\" y=\"36\" font-size=\"11.5\" fill=\"#c6cdd3\">atlas</text>\n"
    ));

    // Panel A: par mix + rarities bars.
    panel_title(&mut s, 88.0, "PAR MIX (of par-36 nines) + SEQUENCE RARITIES — %");
    {
        let cats: Vec<(&str, f64, f64)> = vec![
            ("2-5-2", gpct(*comp.get(&2).unwrap_or(&0)), a_mix[2]),
            ("1-7-1", gpct(*comp.get(&1).unwrap_or(&0)), a_mix[1]),
            ("3-3-3", gpct(*comp.get(&3).unwrap_or(&0)), a_mix[3]),
            ("0-9-0", gpct(*comp.get(&0).unwrap_or(&0)), a_mix[0]),
            ("4-1-4", gpct(*comp.get(&4).unwrap_or(&0)), a_mix[4]),
            ("open 3", gpct(opener3), apct(at.opener3)),
            ("close 3", gpct(closer3), apct(at.closer3)),
            ("b2b 3s", gpct(b2b3), apct(at.b2b3)),
            ("b2b 5s", gpct(b2b5), apct(at.b2b5)),
        ];
        let (y_base, h_max, vmax) = (238.0f64, 128.0f64, 100.0f64);
        s.push_str(&format!(
            "<line x1=\"60\" y1=\"{y_base}\" x2=\"930\" y2=\"{y_base}\" stroke=\"#3a4249\"/>\n"
        ));
        for (i, (name, g, a)) in cats.iter().enumerate() {
            let x0 = 72.0 + i as f64 * 96.0;
            for (k, (v, col)) in [(g, GEN), (a, ATL)].iter().enumerate() {
                let h = (**v / vmax) * h_max;
                let x = x0 + k as f64 * 22.0;
                s.push_str(&format!(
                    "<rect x=\"{x:.1}\" y=\"{:.1}\" width=\"18\" height=\"{h:.1}\" fill=\"{col}\"/>\n\
                     <text x=\"{:.1}\" y=\"{:.1}\" font-size=\"9.5\" fill=\"#c6cdd3\" text-anchor=\"middle\">{:.0}</text>\n",
                    y_base - h,
                    x + 9.0,
                    (y_base - h - 3.0).min(y_base - 3.0),
                    **v
                ));
            }
            s.push_str(&format!(
                "<text x=\"{:.1}\" y=\"{:.1}\" font-size=\"10.5\" fill=\"#9aa4ad\" text-anchor=\"middle\">{name}</text>\n",
                x0 + 20.0,
                y_base + 14.0
            ));
        }
    }

    // Box-whisker helpers.
    let xmap = |v: f64, vmin: f64, vmax: f64, x0: f64, x1: f64| -> f64 {
        x0 + (v - vmin) / (vmax - vmin) * (x1 - x0)
    };
    let boxrow = |s: &mut String, vals: &[f64], y: f64, vmin: f64, vmax: f64, x0: f64, x1: f64, col: &str| {
        if vals.is_empty() {
            return;
        }
        let v = sorted(vals.to_vec());
        let (p02, p10, p50, p90, p98) = (
            pctile(&v, 0.02),
            pctile(&v, 0.10),
            pctile(&v, 0.50),
            pctile(&v, 0.90),
            pctile(&v, 0.98),
        );
        let cl = |v: f64| v.clamp(vmin, vmax);
        let (w0, b0, m, b1, w1) = (
            xmap(cl(p02), vmin, vmax, x0, x1),
            xmap(cl(p10), vmin, vmax, x0, x1),
            xmap(cl(p50), vmin, vmax, x0, x1),
            xmap(cl(p90), vmin, vmax, x0, x1),
            xmap(cl(p98), vmin, vmax, x0, x1),
        );
        s.push_str(&format!(
            "<line x1=\"{w0:.1}\" y1=\"{y:.1}\" x2=\"{w1:.1}\" y2=\"{y:.1}\" stroke=\"{col}\" stroke-width=\"1.2\" opacity=\"0.7\"/>\n\
             <rect x=\"{b0:.1}\" y=\"{:.1}\" width=\"{:.1}\" height=\"11\" fill=\"{col}\" opacity=\"0.55\"/>\n\
             <rect x=\"{:.1}\" y=\"{:.1}\" width=\"2.6\" height=\"15\" fill=\"{col}\"/>\n",
            y - 5.5,
            (b1 - b0).max(1.0),
            m - 1.3,
            y - 7.5
        ));
    };
    let axis = |s: &mut String, y: f64, vmin: f64, vmax: f64, x0: f64, x1: f64, step: f64, zero_h: f64| {
        s.push_str(&format!(
            "<line x1=\"{x0}\" y1=\"{y}\" x2=\"{x1}\" y2=\"{y}\" stroke=\"#3a4249\"/>\n"
        ));
        let mut v = (vmin / step).ceil() * step;
        while v <= vmax + 1e-9 {
            let x = xmap(v, vmin, vmax, x0, x1);
            s.push_str(&format!(
                "<line x1=\"{x:.1}\" y1=\"{y}\" x2=\"{x:.1}\" y2=\"{:.1}\" stroke=\"#3a4249\"/>\n\
                 <text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"9.5\" fill=\"#9aa4ad\" text-anchor=\"middle\">{v:.0}</text>\n",
                y + 4.0,
                y + 15.0
            ));
            v += step;
        }
        if zero_h > 0.0 && vmin < 0.0 && vmax > 0.0 {
            let x = xmap(0.0, vmin, vmax, x0, x1);
            s.push_str(&format!(
                "<line x1=\"{x:.1}\" y1=\"{:.1}\" x2=\"{x:.1}\" y2=\"{y}\" stroke=\"#54606a\" stroke-dasharray=\"3 3\"/>\n",
                y - zero_h
            ));
        }
    };
    let row_label = |s: &mut String, y: f64, t: &str| {
        s.push_str(&format!(
            "<text x=\"30\" y=\"{:.1}\" font-size=\"11\" fill=\"#c6cdd3\">{t}</text>\n",
            y + 4.0
        ));
    };

    // Panel B: hole lengths per par.
    panel_title(&mut s, 292.0, "HOLE LENGTH — m (box p10–p90, tick p50, whiskers p2–p98)");
    {
        let (x0, x1, vmin, vmax) = (110.0, 930.0, 100.0, 640.0);
        let mut y = 314.0;
        for (i, name) in ["par 3", "par 4", "par 5"].iter().enumerate() {
            row_label(&mut s, y + 8.0, name);
            boxrow(&mut s, &g_len[i], y, vmin, vmax, x0, x1, GEN);
            boxrow(&mut s, &at.len_by_par[i], y + 16.0, vmin, vmax, x0, x1, ATL);
            y += 44.0;
        }
        axis(&mut s, y - 4.0, vmin, vmax, x0, x1, 100.0, 0.0);
    }

    // Panel C: hole elevation change.
    panel_title(&mut s, 490.0, "HOLE ELEVATION CHANGE tee→green — m (+ = uphill)");
    {
        let (x0, x1, vmin, vmax) = (110.0, 930.0, -40.0, 40.0);
        let mut y = 512.0;
        for (i, name) in ["par 3", "par 4", "par 5"].iter().enumerate() {
            row_label(&mut s, y + 8.0, name);
            boxrow(&mut s, &g_dz[i], y, vmin, vmax, x0, x1, GEN);
            boxrow(&mut s, &at.dz_by_par[i], y + 16.0, vmin, vmax, x0, x1, ATL);
            y += 44.0;
        }
        axis(&mut s, y - 4.0, vmin, vmax, x0, x1, 10.0, 136.0);
    }

    // Panel D: drive drop.
    panel_title(&mut s, 688.0, "DRIVE DROP — tee elevation over the 137 m landing point, m");
    {
        let (x0, x1, vmin, vmax) = (110.0, 930.0, -14.0, 22.0);
        let a_drop: Vec<f64> = at.drop_by_par.iter().flatten().copied().collect();
        row_label(&mut s, 712.0, "all");
        boxrow(&mut s, &g_drop, 708.0, vmin, vmax, x0, x1, GEN);
        boxrow(&mut s, &a_drop, 724.0, vmin, vmax, x0, x1, ATL);
        axis(&mut s, 744.0, vmin, vmax, x0, x1, 5.0, 44.0);
        s.push_str(
            "<text x=\"110\" y=\"776\" font-size=\"10.5\" fill=\"#8a949d\">atlas median sits ~+1 m above generated: real tee boxes are BUILT-UP pads — that platform belongs to the hole-detailing phase</text>\n",
        );
    }

    // Panel E: nine totals + walks.
    panel_title(&mut s, 812.0, "NINE TOTAL — m");
    s.push_str("<text x=\"530\" y=\"812\" font-size=\"13\" fill=\"#dfe5ea\">WALKS green→next tee — m</text>\n");
    {
        let (x0, x1, vmin, vmax) = (110.0, 460.0, 2800.0, 3700.0);
        boxrow(&mut s, &g_tot, 834.0, vmin, vmax, x0, x1, GEN);
        boxrow(&mut s, &at.nine_totals, 850.0, vmin, vmax, x0, x1, ATL);
        axis(&mut s, 870.0, vmin, vmax, x0, x1, 200.0, 0.0);
        let (x0, x1, vmin, vmax) = (560.0, 930.0, 0.0, 620.0);
        boxrow(&mut s, &g_walk, 834.0, vmin, vmax, x0, x1, GEN);
        boxrow(&mut s, &at.walks, 850.0, vmin, vmax, x0, x1, ATL);
        axis(&mut s, 870.0, vmin, vmax, x0, x1, 100.0, 0.0);
    }

    s.push_str(&format!(
        "<text x=\"30\" y=\"930\" font-size=\"10.5\" fill=\"#8a949d\">generated: {} holes over {} routed nines · atlas: {} holes over {} nines ({} courses) · all constraints re-verified per routed seed</text>\n\
         <text x=\"30\" y=\"948\" font-size=\"10.5\" fill=\"#8a949d\">3-3-3 mix is a deliberate design boost (game variety) over the atlas's 0% — every other distribution is fit to the atlas</text>\n",
        9 * nr,
        nr,
        at.len_by_par.iter().map(Vec::len).sum::<usize>(),
        at.nines,
        at.n_courses
    ));
    s.push_str("</svg>\n");

    let path = PathBuf::from("output/routing_analysis.svg");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, s).unwrap();
    println!("wrote {} ({} routed / {} seeds)", path.display(), nr, n);
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Hole build-out: report gates
// ---------------------------------------------------------------------------

struct BuiltSeed {
    det_ok: bool,
    pins_ok: usize,
    pin_slope_ok: usize,
    pinnable_min: f64,
    bunkers: usize,
    bunker_clean: bool,
    green_areas: Vec<(f64, f64)>, // (approach m, area m²)
    landing_widest: (usize, usize), // (ok, total) among par 4/5
    dogleg_inside_ok: (usize, usize),
    half_widths: Vec<f64>,
    pits: usize,
    continuity_ok: bool,
    pad_drops: Vec<f64>,
    build_ms: f64,
}

fn build_one(seed: u64) -> Option<BuiltSeed> {
    use golf_core::math::Vec2;
    let (ct, _) = golf_terrain::generate_course(&golf_terrain::macro_spec(), seed);
    let r = golf_routing::route(&ct, seed).ok()?;
    let t0 = std::time::Instant::now();
    let b = golf_holes::build(&ct, &r, seed);
    let build_ms = t0.elapsed().as_secs_f64() * 1e3;
    let b2 = golf_holes::build(&ct, &r, seed);
    let det_ok = b
        .patches
        .iter()
        .zip(&b2.patches)
        .all(|(p, q)| {
            p.z.data.iter().zip(&q.z.data).all(|(a, c)| a.to_bits() == c.to_bits())
                && p.w.data.iter().zip(&q.w.data).all(|(a, c)| a.to_bits() == c.to_bits())
        })
        && b.holes.iter().zip(&b2.holes).all(|(x, y)| x.pin == y.pin);

    let mut pins_ok = 0usize;
    let mut pin_slope_ok = 0usize;
    let mut pinnable_min = f64::INFINITY;
    let mut bunkers = 0usize;
    let mut bunker_clean = true;
    let mut green_areas = Vec::with_capacity(9);
    let mut half_widths = Vec::new();
    let mut pad_drops = Vec::with_capacity(9);
    let (mut lw_ok, mut lw_n) = (0usize, 0usize);
    let (mut dg_ok, mut dg_n) = (0usize, 0usize);

    for (i, h) in b.holes.iter().enumerate() {
        let rh = &r.holes[i];
        // Pin containment + margin (exact polygon distance).
        if h.green.rnorm(h.pin) < 1.0 && h.green.boundary_dist(h.pin) >= 2.6 {
            pins_ok += 1;
        }
        // Pin slope on the FINAL composed surface, 1.5 m disc, 0.5 m diffs.
        let mut smax = 0.0f64;
        for &(dx, dy) in &[(0.0, 0.0), (1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let c = h.pin + Vec2::new(dx, dy);
            let gx = (b.surface_at(&ct, c + Vec2::new(0.5, 0.0))
                - b.surface_at(&ct, c + Vec2::new(-0.5, 0.0)))
                .abs();
            let gy = (b.surface_at(&ct, c + Vec2::new(0.0, 0.5))
                - b.surface_at(&ct, c + Vec2::new(0.0, -0.5)))
                .abs();
            smax = smax.max(golf_core::math::hypot(gx, gy));
        }
        if smax <= 0.040 {
            pin_slope_ok += 1;
        }
        pinnable_min = pinnable_min.min(h.pinnable_share);

        let approach = if rh.par >= 4 {
            rh.pts[rh.pts.len() - 2].distance(*rh.pts.last().unwrap())
        } else {
            rh.len
        };
        green_areas.push((approach, h.green.area));

        // Fairway width strategy (par 4/5 only; the envelope mid-width is
        // the stable quantity — sinusoids/meander cancel across sides).
        if rh.par >= 4 && !h.fw.is_none() {
            lw_n += 1;
            let mid = |s: f64| h.fw.env_mid(s);
            let landing_s = h.landing.first().map(|l| l.0).unwrap_or(0.5);
            let w_land = mid(landing_s);
            let w_start = mid(h.fw.start + 0.02);
            let w_appr = mid(h.fw.end - 0.02);
            if w_land > w_start && w_land > w_appr {
                lw_ok += 1;
            }
            for k in 0..8 {
                let s = h.fw.start + (h.fw.end - h.fw.start) * (k as f64 + 0.5) / 8.0;
                half_widths.push(mid(s));
            }
            // Dogleg: inside vs outside at each routed turn. With the
            // Catmull-Rom spine the curvature clamp also narrows the inside;
            // measure the full half-widths at the turn.
            let mut acc = 0.0;
            for k in 1..rh.pts.len() - 1 {
                acc += rh.pts[k - 1].distance(rh.pts[k]);
                let a = (rh.pts[k] - rh.pts[k - 1]).normalized();
                let c = (rh.pts[k + 1] - rh.pts[k]).normalized();
                let cross = a.perp().dot(c);
                if cross.abs() < 0.05 {
                    continue;
                }
                dg_n += 1;
                let s_turn = (acc / h.spine.length()).clamp(h.fw.start, h.fw.end);
                let len = h.spine.length();
                let (wl, wr) = (
                    h.fw.half_width(len, s_turn, true),
                    h.fw.half_width(len, s_turn, false),
                );
                let (inside, outside) = if cross > 0.0 { (wl, wr) } else { (wr, wl) };
                if inside < outside {
                    dg_ok += 1;
                }
            }
        }

        // Drive drop from the pad surface to the 137 m point.
        let dir = (rh.pts[1] - rh.pts[0]).normalized();
        let p137 = rh.pts[0] + dir * 137.0_f64.min(rh.len * 0.8);
        pad_drops.push(h.tee.z - b.surface_at(&ct, p137));

        // Bunkers: count + basic sanity (never on the green or the pin).
        bunkers += h.bunkers.len();
        for bk in &h.bunkers {
            if bk.contains(h.pin) || bk.contains(h.green.center) {
                bunker_clean = false;
            }
        }
    }

    // Fairway interior pits (patches 0..9 are the fairways, apply order).
    let mut pits = 0usize;
    for p in &b.patches[0..9] {
        let s = p.z.spec;
        let (nx, ny) = (s.nx as usize, s.ny as usize);
        for y in 1..ny - 1 {
            for x in 1..nx - 1 {
                let i = y * nx + x;
                if p.w.data[i] < 0.9 {
                    continue;
                }
                let mut lowest = f64::INFINITY;
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1)] {
                    let j = (y as i64 + dy) as usize * nx + (x as i64 + dx) as usize;
                    lowest = lowest.min(p.z.data[j]);
                }
                if p.z.data[i] < lowest {
                    pits += 1;
                }
            }
        }
    }

    // Continuity probes outside every patch bbox.
    let continuity_ok = [(60.0, 60.0), (1950.0, 80.0), (100.0, 1900.0), (1980.0, 1960.0)]
        .iter()
        .all(|&(x, y)| {
            let p = Vec2::new(x, y);
            !b.patches.iter().any(|pa| pa.contains(p))
                && b.surface_at(&ct, p).to_bits() == ct.height_at(p).to_bits()
        });

    Some(BuiltSeed {
        det_ok,
        pins_ok,
        pin_slope_ok,
        pinnable_min,
        bunkers,
        bunker_clean,
        green_areas,
        landing_widest: (lw_ok, lw_n),
        dogleg_inside_ok: (dg_ok, dg_n),
        half_widths,
        pits,
        continuity_ok,
        pad_drops,
        build_ms,
    })
}

/// Build every routable seed and gate the realized hole build-out.
fn run_build_report(n: u64) -> ExitCode {
    use rayon::prelude::*;
    let results: Vec<Option<BuiltSeed>> = (0..n).into_par_iter().map(build_one).collect();
    let built: Vec<&BuiltSeed> = results.iter().flatten().collect();
    let nb = built.len();

    let det_fail = built.iter().filter(|b| !b.det_ok).count();
    let pins_bad: usize = built.iter().map(|b| 9 - b.pins_ok).sum();
    let pin_slope_bad: usize = built.iter().map(|b| 9 - b.pin_slope_ok).sum();
    let pinnable_min = built.iter().map(|b| b.pinnable_min).fold(f64::INFINITY, f64::min);
    let pits: usize = built.iter().map(|b| b.pits).sum();
    let cont_fail = built.iter().filter(|b| !b.continuity_ok).count();

    let areas: Vec<f64> = built
        .iter()
        .flat_map(|b| b.green_areas.iter().map(|&(_, a)| a))
        .collect();
    let areas_sorted = sorted(areas.clone());
    // Pearson correlation approach vs area.
    let pairs: Vec<(f64, f64)> = built.iter().flat_map(|b| b.green_areas.clone()).collect();
    let nn = pairs.len() as f64;
    let (mx, my) = (
        pairs.iter().map(|p| p.0).sum::<f64>() / nn,
        pairs.iter().map(|p| p.1).sum::<f64>() / nn,
    );
    let (mut cov, mut vx, mut vy) = (0.0, 0.0, 0.0);
    for &(x, y) in &pairs {
        cov += (x - mx) * (y - my);
        vx += (x - mx) * (x - mx);
        vy += (y - my) * (y - my);
    }
    let corr = cov / (vx.sqrt() * vy.sqrt()).max(1e-9);

    let hw = sorted(built.iter().flat_map(|b| b.half_widths.clone()).collect());
    let (lw_ok, lw_n) = built.iter().fold((0usize, 0usize), |acc, b| {
        (acc.0 + b.landing_widest.0, acc.1 + b.landing_widest.1)
    });
    let (dg_ok, dg_n) = built.iter().fold((0usize, 0usize), |acc, b| {
        (acc.0 + b.dogleg_inside_ok.0, acc.1 + b.dogleg_inside_ok.1)
    });
    let drops = sorted(built.iter().flat_map(|b| b.pad_drops.clone()).collect());
    let ms = sorted(built.iter().map(|b| b.build_ms).collect());

    println!("build report over {n} seeds ({nb} routed+built)\n");
    println!("pins outside green/margin: {pins_bad}   pin-slope violations (>4.0%): {pin_slope_bad}");
    println!("min pinnable share: {:.2}   fairway interior pits: {pits}", pinnable_min);
    println!(
        "green areas (m²): p10 {:.0}  p50 {:.0}  p90 {:.0}   corr(area, approach) = {:.2}",
        pctile(&areas_sorted, 0.10),
        pctile(&areas_sorted, 0.50),
        pctile(&areas_sorted, 0.90),
        corr
    );
    println!(
        "fairway half-widths (m): p10 {:.1}  p50 {:.1}  p90 {:.1}",
        pctile(&hw, 0.10),
        pctile(&hw, 0.50),
        pctile(&hw, 0.90)
    );
    println!(
        "landing widest: {lw_ok}/{lw_n} ({:.0}%)   dogleg inside narrower: {dg_ok}/{dg_n} ({:.0}%)",
        100.0 * lw_ok as f64 / lw_n.max(1) as f64,
        100.0 * dg_ok as f64 / dg_n.max(1) as f64
    );
    println!(
        "pad drive drop (m): p10 {:.1}  p50 {:.1}  p90 {:.1}   (atlas +1.1 median)",
        pctile(&drops, 0.10),
        pctile(&drops, 0.50),
        pctile(&drops, 0.90)
    );
    let bunker_counts = sorted(built.iter().map(|b| b.bunkers as f64).collect());
    let bunker_dirty = built.iter().filter(|b| !b.bunker_clean).count();
    println!(
        "bunkers per course: p10 {:.0}  p50 {:.0}  p90 {:.0}   green/pin conflicts: {bunker_dirty}",
        pctile(&bunker_counts, 0.10),
        pctile(&bunker_counts, 0.50),
        pctile(&bunker_counts, 0.90)
    );
    println!(
        "build time (ms): p50 {:.0}  p95 {:.0}  max {:.0}",
        pctile(&ms, 0.50),
        pctile(&ms, 0.95),
        pctile(&ms, 1.0)
    );

    let mut ok = true;
    let mut gate = |name: &str, pass: bool| {
        println!("{} {}", if pass { "OK  " } else { "FAIL" }, name);
        ok &= pass;
    };
    println!("\ngates:");
    gate("all builds deterministic", det_fail == 0);
    gate("every pin inside its green with ≥2.6 m margin", pins_bad == 0);
    gate("pin slope ≤ 4.0% on the composed surface", pin_slope_bad == 0);
    gate("pinnable share ≥ 18% on every green", pinnable_min >= 0.18 - 1e-9);
    gate("no fairway interior pits", pits == 0);
    gate("surface == base outside patches", cont_fail == 0);
    gate("green area p50 in 450–700 m²", (450.0..=700.0).contains(&pctile(&areas_sorted, 0.50)));
    gate("green area grows with approach (r > 0.25)", corr > 0.25);
    gate("fairway half-width p50 in 10–17 m", (10.0..=17.0).contains(&pctile(&hw, 0.50)));
    gate("landing widest on ≥ 70% of par 4/5", lw_ok as f64 >= 0.70 * lw_n as f64);
    gate("inside-dogleg narrower on ≥ 80% of turns", dg_ok as f64 >= 0.80 * dg_n.max(1) as f64);
    gate("pad drive drop p50 in 0.3–1.8 m", (0.3..=1.8).contains(&pctile(&drops, 0.50)));
    gate("bunkers per course p50 in 8–35", (8.0..=35.0).contains(&pctile(&bunker_counts, 0.50)));
    gate("no bunker on a green or pin", bunker_dirty == 0);
    gate("build time p50 ≤ 100 ms", pctile(&ms, 0.50) <= 100.0);

    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

// ---------------------------------------------------------------------------
// Hole build-out: visual dumps
// ---------------------------------------------------------------------------

fn run_dump_built(dir: &Path, seeds: &[u64]) -> ExitCode {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    println!("writing to {}", dir.display());
    for &seed in seeds {
        let (ct, sp) = golf_terrain::generate_course(&golf_terrain::macro_spec(), seed);
        let r = match golf_routing::route(&ct, seed) {
            Ok(r) => r,
            Err(e) => {
                println!("seed {seed} [{}]: UNROUTABLE ({})", sp.mode.label(), e.reason);
                continue;
            }
        };
        let t0 = std::time::Instant::now();
        let b = golf_holes::build(&ct, &r, seed);
        let build_ms = t0.elapsed().as_secs_f64() * 1e3;

        let mut img = golf_viz::render_height_water(&ct.macro_heights, &ct.water, 900, None);
        golf_viz::draw_build(&mut img, &b, golf_terrain::WORLD_SIZE);
        golf_viz::draw_routing(&mut img, &r, golf_terrain::WORLD_SIZE);
        img.save(dir.join(format!("built{seed}.png"))).unwrap();

        // 3×3 grid of green close-ups.
        let tile = 300u32;
        let mut grid = image::RgbaImage::new(3 * tile, 3 * tile);
        for i in 0..9usize {
            let close = golf_viz::render_green_closeup(&ct, &b, i, tile);
            let (ox, oy) = ((i % 3) as u32 * tile, (i / 3) as u32 * tile);
            for y in 0..tile {
                for x in 0..tile {
                    grid.put_pixel(ox + x, oy + y, *close.get_pixel(x, y));
                }
            }
        }
        grid.save(dir.join(format!("built{seed}_greens.png"))).unwrap();

        let areas: Vec<String> = b.holes.iter().map(|h| format!("{:.0}", h.green.area)).collect();
        println!(
            "seed {seed} [{}]: built in {:.0} ms | green m²: {} | tee lift avg {:.2} m",
            sp.mode.label(),
            build_ms,
            areas.join("/"),
            b.holes
                .iter()
                .map(|h| h.tee.z - ct.macro_heights.bilinear(h.tee.center))
                .sum::<f64>()
                / 9.0
        );
    }
    ExitCode::SUCCESS
}

/// Render hole-aligned 0.5 m/px views (the Hole viewer's imagery, headless).
fn run_dump_hole(dir: &Path, seed: u64, holes: &[usize]) -> ExitCode {
    if let Err(e) = std::fs::create_dir_all(dir) {
        eprintln!("cannot create {}: {e}", dir.display());
        return ExitCode::FAILURE;
    }
    let (ct, sp) = golf_terrain::generate_course(&golf_terrain::macro_spec(), seed);
    let r = match golf_routing::route(&ct, seed) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("seed {seed} [{}]: UNROUTABLE ({})", sp.mode.label(), e.reason);
            return ExitCode::FAILURE;
        }
    };
    let b = golf_holes::build(&ct, &r, seed);
    let list: Vec<usize> = if holes.is_empty() { (0..9).collect() } else { holes.to_vec() };
    for &h in &list {
        let img = golf_viz::render_hole_view(&ct, &b, &r, h, 0.1, true);
        let path = dir.join(format!("hole{}_{seed}.png", h + 1));
        img.save(&path).unwrap();
        println!(
            "hole {} (par {} · {:.0} m): {}x{} px -> {}",
            h + 1,
            r.holes[h].par,
            r.holes[h].len,
            img.width(),
            img.height(),
            path.display()
        );
    }
    ExitCode::SUCCESS
}

// ---------------------------------------------------------------------------
// Determinism golden
// ---------------------------------------------------------------------------

fn reference_hash() -> u64 {
    let t = generate(&world_spec(GOLDEN_N), 2024, &TerrainParams::default());
    fnv(&t.heights.data)
}

/// Full-pipeline golden: canonical macro heights ⊕ a composed 256² output
/// (exercises noise → erosion → compose, sampler included).
fn pipeline_hash() -> u64 {
    let (ct, _) = golf_terrain::generate_course(&world_spec(256), 2024);
    fnv(&ct.macro_heights.data) ^ fnv(&ct.terrain.heights.data).rotate_left(1)
}

fn pipeline_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/pipeline.hash")
}

fn fnv(data: &[f64]) -> u64 {
    let mut h = 1469598103934665603u64;
    for z in data {
        h ^= z.to_bits();
        h = h.wrapping_mul(1099511628211);
    }
    h
}

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/terrain.hash")
}

/// terrain-v2 landform golden: the barranca gate preset at 20 m (matches the
/// in-crate pin in golf-landform/tests/property.rs).
fn landform_hash() -> u64 {
    let cfg = golf_landform::preset("barranca").expect("preset exists");
    golf_landform::field_hash(&golf_landform::generate(&cfg, 20.0))
}

fn landform_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/landform.hash")
}

fn run_golden() -> ExitCode {
    let mut ok = true;
    let mut check = |name: &str, path: PathBuf, hash: u64| match std::fs::read_to_string(&path) {
        Ok(exp) => {
            if exp.trim() == hash.to_string() {
                println!("OK {name} golden matches: {hash}");
            } else {
                eprintln!("FAIL {name} golden mismatch: expected {}, got {hash}", exp.trim());
                ok = false;
            }
        }
        Err(_) => {
            eprintln!("no {name} golden; run: cargo run -p xtask -- bless");
            ok = false;
        }
    };
    check("terrain", golden_path(), reference_hash());
    check("pipeline", pipeline_golden_path(), pipeline_hash());
    check("routing", routing_golden_path(), routing_hash());
    check("holes", holes_golden_path(), holes_hash());
    check("landform", landform_golden_path(), landform_hash());
    if ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// Routing golden: every coordinate of the routed nine for seed 2024 (holes,
/// walks, clubhouse). Terrain goldens pin the input; this pins the router.
fn routing_hash() -> u64 {
    let (ct, _) = golf_terrain::generate_course(&golf_terrain::macro_spec(), 2024);
    match golf_routing::route(&ct, 2024) {
        Ok(r) => {
            let mut v: Vec<f64> = vec![r.clubhouse.x, r.clubhouse.y];
            for h in &r.holes {
                v.push(h.par as f64);
                for p in &h.pts {
                    v.push(p.x);
                    v.push(p.y);
                }
                for p in &h.walk_to_next {
                    v.push(p.x);
                    v.push(p.y);
                }
            }
            fnv(&v)
        }
        Err(_) => 0xDEAD,
    }
}

fn routing_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/routing.hash")
}

/// Build-out golden: every patch surface + pin + outline coordinate of the
/// built course for seed 2024.
fn holes_hash() -> u64 {
    let (ct, _) = golf_terrain::generate_course(&golf_terrain::macro_spec(), 2024);
    match golf_routing::route(&ct, 2024) {
        Ok(r) => {
            let b = golf_holes::build(&ct, &r, 2024);
            let mut v: Vec<f64> = Vec::new();
            for h in &b.holes {
                v.push(h.pin.x);
                v.push(h.pin.y);
                v.push(h.green.area);
                v.push(h.tee.z);
                for p in &h.fairway_outline {
                    v.push(p.x);
                    v.push(p.y);
                }
                for p in &h.green.outline {
                    v.push(p.x);
                    v.push(p.y);
                }
            }
            let mut h = fnv(&v);
            for p in &b.patches {
                h ^= fnv(&p.z.data).rotate_left(1) ^ fnv(&p.w.data).rotate_left(2);
            }
            h
        }
        Err(_) => 0xDEAD,
    }
}

fn holes_golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("golden/holes.hash")
}
