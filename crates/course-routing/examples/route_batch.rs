//! Route a batch of dumped tiles with the Rust port and record what it did
//! -- the twin of `tools/golf/batch_route.py`, same record shape:
//!
//!   route_batch <dump_dir> <out.jsonl> [--prefix m_] [--seeds a,b,c]
//!               [--stride k --offset j] [--time]
//!
//! Reads `<dump_dir>/index_*.txt` (`seed mode lake_frac [water]`) for the
//! mode of each seed, loads each tile (`Terrain::load`), runs the chain the
//! library's `route_tile` runs (siting -> greens -> routing, generator-free)
//! and appends one JSON line per seed via `dump::record`. The timed region
//! is the chain only, as the Python's `t0` sits after the tile is read.
//! The chain is unrolled here rather than calling `route_tile` because the
//! record needs the pool size and the Siting even when no route survives,
//! and `route_tile` returns neither. `--time` prints per-stage seconds.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;
use std::time::Instant;

use course_routing::terrain::Terrain;
use course_routing::{dump, greens, route, siting, Candidate, Clubhouse, Mode, Route, Siting};

struct Timed {
    sit: Siting,
    pool: usize,
    route: Option<Route>,
    seconds: f64,
    stage: [f64; 3],
}

fn route_one(t: &Terrain) -> Timed {
    let t0 = Instant::now();
    let (sit, f, m, p) = siting::run_siting(t, siting::dims_for(t.mode), None);
    let t1 = Instant::now();
    let pool = greens::generate(t, &sit, &f, &m, &p, greens::N_TARGET, None);
    let t2 = Instant::now();
    let pool_fn = |ch: &Clubhouse| -> Vec<Candidate> {
        let mut s2 = sit.clone();
        s2.clubhouse = ch.clone();
        greens::generate(t, &s2, &f, &m, &p, greens::N_TARGET, None)
    };
    let r = route::run_routing(t, &sit, &f, &pool, Some(&pool_fn));
    let t3 = Instant::now();
    Timed {
        pool: pool.len(),
        sit,
        route: r,
        seconds: (t3 - t0).as_secs_f64(),
        stage: [(t1 - t0).as_secs_f64(), (t2 - t1).as_secs_f64(), (t3 - t2).as_secs_f64()],
    }
}

fn take_flag(a: &mut Vec<String>, name: &str) -> Option<String> {
    let i = a.iter().position(|s| s == name)?;
    if i + 1 >= a.len() {
        eprintln!("{name} needs a value");
        std::process::exit(2);
    }
    let v = a[i + 1].clone();
    a.drain(i..i + 2);
    Some(v)
}

fn fmt_opt<T: std::fmt::Display>(v: Option<T>) -> String {
    match v {
        Some(v) => v.to_string(),
        None => "None".to_string(),
    }
}

fn main() {
    let mut a: Vec<String> = std::env::args().skip(1).collect();
    let prefix = take_flag(&mut a, "--prefix").unwrap_or_else(|| "m_".to_string());
    let only: Option<Vec<String>> = take_flag(&mut a, "--seeds")
        .map(|s| s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect());
    let stride: usize = take_flag(&mut a, "--stride").map(|s| s.parse().expect("--stride int")).unwrap_or(1);
    let offset: usize = take_flag(&mut a, "--offset").map(|s| s.parse().expect("--offset int")).unwrap_or(0);
    let timing = if let Some(i) = a.iter().position(|s| s == "--time") {
        a.remove(i);
        true
    } else {
        false
    };
    if a.len() < 2 {
        eprintln!("usage: route_batch <dump_dir> <out.jsonl> [--prefix m_] [--seeds a,b,c] \
                   [--stride k --offset j] [--time]");
        std::process::exit(2);
    }
    let dump_dir = Path::new(&a[0]);
    let out_path = Path::new(&a[1]);

    // seed -> mode from every index_*.txt (sorted by file name so a seed
    // listed twice resolves the same way on every run)
    let mut index_files: Vec<std::path::PathBuf> = std::fs::read_dir(dump_dir)
        .unwrap_or_else(|e| { eprintln!("{}: {e}", dump_dir.display()); std::process::exit(1) })
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            let n = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            n.starts_with("index_") && n.ends_with(".txt")
        })
        .collect();
    index_files.sort();
    let mut modes: BTreeMap<String, String> = BTreeMap::new();
    for fpath in &index_files {
        let txt = std::fs::read_to_string(fpath).unwrap_or_default();
        for l in txt.lines() {
            let p: Vec<&str> = l.split_whitespace().collect();
            if p.len() >= 2 {
                modes.insert(p[0].to_string(), p[1].to_string());
            }
        }
    }
    let mut seeds: Vec<String> = match only {
        Some(s) => s,
        None => {
            let mut v: Vec<String> = modes.keys().cloned().collect();
            v.sort_by_key(|s| s.parse::<i64>().unwrap_or(i64::MAX));
            v
        }
    };
    seeds = seeds.into_iter().enumerate()
        .filter(|(k, _)| k % stride == offset).map(|(_, s)| s).collect();

    let mut out = std::fs::OpenOptions::new().append(true).create(true).open(out_path)
        .unwrap_or_else(|e| { eprintln!("{}: {e}", out_path.display()); std::process::exit(1) });
    for s in &seeds {
        let mode_s = modes.get(s).cloned().unwrap_or_default();
        let mode = if mode_s == "aeolian" { Mode::Aeolian } else { Mode::Fluvial };
        let seed: u64 = match s.parse() {
            Ok(v) => v,
            Err(_) => {
                let rec = dump::error_record(0, &mode_s, &format!("bad seed {s:?}"));
                writeln!(out, "{rec}").expect("write");
                println!("{s} {mode_s} routed=false (bad seed)");
                continue;
            }
        };
        let rec = match Terrain::load(dump_dir, &prefix, seed, mode) {
            Err(e) => dump::error_record(seed, &mode_s, &format!("load: {e}")),
            Ok(t) => {
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| route_one(&t)));
                match res {
                    Ok(tm) => {
                        if timing {
                            println!("  {s} stages: siting {:.2}s greens {:.2}s routing {:.2}s",
                                     tm.stage[0], tm.stage[1], tm.stage[2]);
                        }
                        dump::record(seed, &mode_s, tm.seconds, tm.pool, &tm.sit, tm.route.as_ref())
                    }
                    Err(p) => {
                        let msg = p.downcast_ref::<String>().cloned()
                            .or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string()))
                            .unwrap_or_else(|| "panic".to_string());
                        dump::error_record(seed, &mode_s, &msg)
                    }
                }
            }
        };
        writeln!(out, "{rec}").expect("write");
        out.flush().expect("flush");
        let pars = rec.get("pars").map(|v| v.to_string());
        let len = rec.get("total_length_m").map(|v| v.to_string());
        let x = rec.get("n_crossings").map(|v| v.to_string());
        let secs = rec.get("seconds").map(|v| v.to_string());
        println!("{s} {mode_s} routed={} pars={} len={} x={} {}s",
                 rec.get("routed").map(|v| v.to_string()).unwrap_or_default(),
                 fmt_opt(pars), fmt_opt(len), fmt_opt(x), fmt_opt(secs));
    }
}
