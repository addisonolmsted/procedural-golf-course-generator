//! Headless dev driver (workplan B3 / milestone M2): run the pipeline, batch
//! it for calibration, and time it against the budget table. Complements
//! `stage-lab`, which is the interactive viewer.
//!
//! The stage set grows as stages land — [`STAGES`] is the registry. Today it
//! runs S0 (spec, v1 contents) and the retained v1 stage 01 (framing); the
//! `batch` subcommand is the `xtask forward-grid` replacement the calibration
//! loop calls, and its stdout rows are deterministic by construction (hashes
//! only; timings go to stderr).
//!
//! Docs: `ARCHITECTURE.md`, `docs/00-architecture.md`,
//! `docs/02-performance-budget.md`.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Instant;

use course_seed::RunIdentity;

/// One runnable stage: name, budget from `docs/02-performance-budget.md`
/// (ms), and a runner that writes its artifact(s) under `out/` (when given)
/// and returns the canonical bytes to hash.
struct Stage {
    name: &'static str,
    budget_ms: f64,
    run: fn(&RunIdentity, Option<&Path>) -> Result<Vec<u8>, String>,
}

/// The stage registry, pipeline order. Stages append here as they land.
const STAGES: &[Stage] = &[
    Stage {
        name: "spec",
        budget_ms: 1.0,
        run: run_spec,
    },
    Stage {
        name: "framing_v1",
        budget_ms: 120.0,
        run: run_framing,
    },
];

fn run_spec(id: &RunIdentity, out: Option<&Path>) -> Result<Vec<u8>, String> {
    let spec =
        course_spec::CourseSpec::generate_builtin(*id, &course_spec::SpecOverrides::default())
            .map_err(|e| format!("spec: {e:?}"))?;
    let json = spec.canonical_json();
    if let Some(dir) = out {
        std::fs::write(dir.join("spec.json"), &json).map_err(|e| e.to_string())?;
    }
    Ok(json.into_bytes())
}

fn run_framing(id: &RunIdentity, out: Option<&Path>) -> Result<Vec<u8>, String> {
    let spec =
        course_spec::CourseSpec::generate_builtin(*id, &course_spec::SpecOverrides::default())
            .map_err(|e| format!("spec: {e:?}"))?;
    let framing = course_framing::generate(&spec);
    let json = framing.canonical_json();
    if let Some(dir) = out {
        std::fs::write(dir.join("framing.json"), &json).map_err(|e| e.to_string())?;
    }
    Ok(json.into_bytes())
}

fn stage_cutoff(through: Option<&str>) -> Result<usize, String> {
    match through {
        None => Ok(STAGES.len()),
        Some(name) => STAGES
            .iter()
            .position(|s| s.name == name)
            .map(|i| i + 1)
            .ok_or_else(|| {
                format!(
                    "unknown stage `{name}`; known: {}",
                    STAGES
                        .iter()
                        .map(|s| s.name)
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }),
    }
}

// ------------------------------------------------------------- subcommands

fn cmd_run(seed: u64, out: Option<PathBuf>, through: Option<&str>) -> Result<(), String> {
    let id = RunIdentity::from_seed(seed);
    if let Some(dir) = &out {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let n = stage_cutoff(through)?;
    for stage in &STAGES[..n] {
        let t = Instant::now();
        let bytes = (stage.run)(&id, out.as_deref())?;
        let ms = t.elapsed().as_secs_f64() * 1e3;
        eprintln!(
            "{:>12}  {:>9.3} ms  blake3 {}",
            stage.name,
            ms,
            &blake3::hash(&bytes).to_hex().as_str()[..16]
        );
    }
    Ok(())
}

/// The forward-runner: one JSON line per seed with artifact hashes only —
/// byte-identical across invocations (timings deliberately excluded from
/// stdout; a summary goes to stderr).
fn cmd_batch(seeds: (u64, u64), through: Option<&str>) -> Result<(), String> {
    let n = stage_cutoff(through)?;
    let stdout = std::io::stdout();
    let mut w = stdout.lock();
    let t0 = Instant::now();
    let mut count = 0u64;
    for seed in seeds.0..=seeds.1 {
        let id = RunIdentity::from_seed(seed);
        let mut row = String::new();
        row.push_str(&format!("{{\"seed\":{seed}"));
        for stage in &STAGES[..n] {
            let bytes = (stage.run)(&id, None)?;
            row.push_str(&format!(
                ",\"{}\":\"{}\"",
                stage.name,
                blake3::hash(&bytes).to_hex()
            ));
        }
        row.push('}');
        writeln!(w, "{row}").map_err(|e| e.to_string())?;
        count += 1;
    }
    eprintln!(
        "batch: {count} seeds, {n} stages, {:.2} s",
        t0.elapsed().as_secs_f64()
    );
    Ok(())
}

/// Per-stage timing vs the budget table, median of `iters` runs.
fn cmd_time(seed: u64, iters: u32) -> Result<(), String> {
    let id = RunIdentity::from_seed(seed);
    println!(
        "{:>12}  {:>10}  {:>10}  {:>7}",
        "stage", "median ms", "budget ms", "status"
    );
    for stage in STAGES {
        let mut samples = Vec::with_capacity(iters as usize);
        for _ in 0..iters {
            let t = Instant::now();
            (stage.run)(&id, None)?;
            samples.push(t.elapsed().as_secs_f64() * 1e3);
        }
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = samples[samples.len() / 2];
        let status = if median <= stage.budget_ms { "ok" } else { "OVER" };
        println!(
            "{:>12}  {:>10.3}  {:>10.1}  {:>7}",
            stage.name, median, stage.budget_ms, status
        );
    }
    Ok(())
}

// -------------------------------------------------------------------- args

fn usage() -> String {
    "course-cli — headless pipeline driver\n\
     \n\
     USAGE:\n\
       course-cli run   --seed N [--out DIR] [--through STAGE]\n\
       course-cli batch --seeds A..B [--through STAGE]   (JSONL to stdout)\n\
       course-cli time  --seed N [--iters K]\n"
        .to_string()
}

fn arg_value<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .map(|s| s.as_str())
}

fn parse_seed_range(s: &str) -> Result<(u64, u64), String> {
    if let Some((a, b)) = s.split_once("..") {
        let lo: u64 = a.parse().map_err(|_| format!("bad range `{s}`"))?;
        let hi: u64 = b.parse().map_err(|_| format!("bad range `{s}`"))?;
        if lo > hi {
            return Err(format!("empty range `{s}`"));
        }
        Ok((lo, hi))
    } else {
        let v: u64 = s.parse().map_err(|_| format!("bad seed `{s}`"))?;
        Ok((v, v))
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("run") => {
            let seed = arg_value(&args, "--seed")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let out = arg_value(&args, "--out").map(PathBuf::from);
            cmd_run(seed, out, arg_value(&args, "--through"))
        }
        Some("batch") => match arg_value(&args, "--seeds").map(parse_seed_range) {
            Some(Ok(r)) => cmd_batch(r, arg_value(&args, "--through")),
            Some(Err(e)) => Err(e),
            None => Err("batch requires --seeds A..B".into()),
        },
        Some("time") => {
            let seed = arg_value(&args, "--seed")
                .and_then(|s| s.parse().ok())
                .unwrap_or(1);
            let iters = arg_value(&args, "--iters")
                .and_then(|s| s.parse().ok())
                .unwrap_or(9);
            cmd_time(seed, iters)
        }
        _ => {
            eprint!("{}", usage());
            std::process::exit(2);
        }
    };
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seed_range_parses() {
        assert_eq!(parse_seed_range("1..10").unwrap(), (1, 10));
        assert_eq!(parse_seed_range("7").unwrap(), (7, 7));
        assert!(parse_seed_range("9..3").is_err());
        assert!(parse_seed_range("x").is_err());
    }

    #[test]
    fn stage_cutoff_resolves() {
        assert_eq!(stage_cutoff(None).unwrap(), STAGES.len());
        assert_eq!(stage_cutoff(Some("spec")).unwrap(), 1);
        assert!(stage_cutoff(Some("nope")).is_err());
    }

    #[test]
    fn stage_runs_are_deterministic() {
        // The determinism contract for the calibration loop: same seed ⇒ the
        // same artifact bytes ⇒ the same hash, across repeated runs.
        let id = RunIdentity::from_seed(42);
        assert_eq!(run_spec(&id, None).unwrap(), run_spec(&id, None).unwrap());
        assert_eq!(
            run_framing(&id, None).unwrap(),
            run_framing(&id, None).unwrap()
        );
    }
}
