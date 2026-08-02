//! Raster cost at full 2 m resolution, per archetype and per primitive
//! family — the "can we afford N bowls?" question (steps/03).
//! `cargo run -p course-macro --example perf_raster --release`
use std::collections::BTreeMap;
use std::time::Instant;

use course_macro::planner::{lower, plan};
use course_macro::{generate_skeleton, resolve};
use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_world::math::Vec2;
use course_world::world::world_spec;

fn spec_for(a: ArchetypeId, seed: u64, params: BTreeMap<String, f64>) -> CourseSpec {
    let ov = SpecOverrides { forced_archetype: Some(a), params };
    CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &ov).unwrap()
}

/// Time a bare height sweep over the full grid for a given config.
fn sweep(cfg: &course_macro::MacroConfig, res_m: f64) -> (f64, usize) {
    let spec = world_spec(res_m);
    let r = resolve(cfg);
    let t = Instant::now();
    let mut acc = 0.0;
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            acc += r.height_at(Vec2::new(
                (x as f64 + 0.5) * spec.cell_size,
                (y as f64 + 0.5) * spec.cell_size,
            ));
        }
    }
    std::hint::black_box(acc);
    (t.elapsed().as_secs_f64(), (spec.nx * spec.ny) as usize)
}

fn main() {
    println!("full skeleton at 2 m (plan + raster + fields + spines):");
    for a in ArchetypeId::ALL {
        let spec = spec_for(a, 1, BTreeMap::new());
        let t = Instant::now();
        let r = generate_skeleton(&spec, 2.0);
        let dt = t.elapsed().as_secs_f64();
        println!(
            "  {:<16} {dt:6.2}s   bowls {:3}  ridges {:2}  valleys {:2}  scarps {:2}",
            a.key(),
            r.config.bowls.len(),
            r.config.ridges.len(),
            r.config.valleys.len(),
            r.config.bluffs.len()
        );
    }

    println!("\nbowl scaling (sandhills seed 1, basin_count overridden):");
    for n in [0.0, 12.0, 47.0, 78.0, 120.0] {
        let mut p = BTreeMap::new();
        p.insert("landform.basin_count".to_string(), n);
        let spec = spec_for(ArchetypeId::Sandhills, 1, p);
        let pl = plan(&spec);
        let cfg = lower(&pl);
        let (dt, cells) = sweep(&cfg, 2.0);
        println!(
            "  basin_count {n:5.0} -> {:3} bowls  {dt:6.2}s  ({:.0} ns/cell)",
            cfg.bowls.len(),
            dt * 1e9 / cells as f64
        );
    }

    // The M1 question: does the raster survive a 30-reach network? Forced
    // via valley_count, which the planner turns into a trunk + fan — the
    // per-valley cost is what matters here, not the topology.
    println!("\nvalley scaling (piedmont seed 1, valley_count overridden):");
    for n in [2.0, 8.0, 16.0, 30.0] {
        let mut p = BTreeMap::new();
        p.insert("landform.valley_count".to_string(), n);
        let spec = spec_for(ArchetypeId::Piedmont, 1, p);
        let t = Instant::now();
        let r = generate_skeleton(&spec, 2.0);
        println!(
            "  valley_count {n:5.0} -> {:2} valleys  {:6.2}s",
            r.config.valleys.len(),
            t.elapsed().as_secs_f64()
        );
    }

    println!("\nper-family cost (sandhills seed 1, families removed one at a time):");
    let spec = spec_for(ArchetypeId::Sandhills, 1, BTreeMap::new());
    let cfg = lower(&plan(&spec));
    let (full, _) = sweep(&cfg, 2.0);
    println!("  full                {full:6.2}s");
    for (label, mut c) in [
        ("without bowls", cfg.clone()),
        ("without ridges", cfg.clone()),
        ("without valleys", cfg.clone()),
        ("without bluffs", cfg.clone()),
    ] {
        match label {
            "without bowls" => c.bowls.clear(),
            "without ridges" => c.ridges.clear(),
            "without valleys" => c.valleys.clear(),
            _ => c.bluffs.clear(),
        }
        let (dt, _) = sweep(&c, 2.0);
        println!("  {label:<19} {dt:6.2}s   (family costs {:.2}s)", full - dt);
    }
}
