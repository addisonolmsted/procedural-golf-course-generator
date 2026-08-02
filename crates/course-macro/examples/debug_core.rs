//! Per-component core-relief breakdown for a seed — the budget-solve
//! debugging tool (knocks out valleys/bowls/tilt/uplands one at a time to
//! attribute core relief when a cap failure is being diagnosed).
//! Usage: cargo run -p course-macro --example debug_core -- <seed> <archetype>
use std::collections::BTreeMap;

use course_macro::budget::core_relief_16m;
use course_macro::planner::{lower, plan};
use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let seed: u64 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
    let arch = match args.get(2).map(|s| s.as_str()).unwrap_or("florida_lowland") {
        "sandhills" => ArchetypeId::Sandhills,
        "piedmont" => ArchetypeId::Piedmont,
        "glacial_moraine" => ArchetypeId::GlacialMoraine,
        "mountain_bench" => ArchetypeId::MountainBench,
        _ => ArchetypeId::FloridaLowland,
    };
    let overrides = SpecOverrides { forced_archetype: Some(arch), params: BTreeMap::new() };
    let spec = CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &overrides).unwrap();
    let p = plan(&spec);
    println!("cap={:.2} raw={:.2} final={:.2}", p.budget.cap_m, p.budget.relief_raw_m, p.budget.relief_final_m);
    for n in &p.budget.notes { println!("  note: {n}"); }
    println!("tangent_routed={} n_valleys={} n_ridges={} n_scarps={} n_bowls={}",
        p.drainage.tangent_routed, p.drainage.valleys.len(), p.uplands.ridges.len(),
        p.uplands.scarps.len(), p.basins.bowls.len());
    println!("tilt_grade={:.5} eff={:.5} curve_m={:.3}", p.frame.tilt_grade, p.frame.tilt_grade_eff, p.frame.curve_m);
    let n_core = p.basins.bowls.iter().filter(|b| b.in_core).count();
    println!("bowls in_core={} / {}", n_core, p.basins.bowls.len());
    let cfg = lower(&p);
    let mut c = cfg.clone(); c.valleys.clear();
    println!("core relief full={:.2} no-valleys={:.2}", core_relief_16m(&cfg), core_relief_16m(&c));
    let mut c = cfg.clone(); c.bowls.clear();
    println!("            no-bowls={:.2}", core_relief_16m(&c));
    let mut c = cfg.clone(); c.tilt.grade_x = 0.0; c.tilt.grade_y = 0.0; c.tilt.curve_m = 0.0;
    println!("            no-tilt={:.2}", core_relief_16m(&c));
    let mut c = cfg.clone(); c.ridges.clear(); c.bluffs.clear();
    println!("            no-uplands={:.2}", core_relief_16m(&c));
    let mut c = cfg.clone(); c.valleys.clear(); c.bowls.clear(); c.ridges.clear(); c.bluffs.clear();
    println!("            tilt-only={:.2}", core_relief_16m(&c));
}
