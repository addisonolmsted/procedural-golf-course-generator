//! Planform instrument: window sinuosity + straight/parallel run lengths.
//!
//! Emits per-biome summaries in JSON. The SAME statistics are computed on
//! the real corpus by tools/macro_campaign scripts (traced D8 paths), so
//! the numbers are directly comparable: is the generated network straighter
//! or more parallel than real ground, or inside its band?
use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;

const SAMPLE_M: f64 = 10.0;
const WINDOWS_M: [f64; 2] = [300.0, 600.0];
const PAR_SAMPLE_M: f64 = 30.0;
const PAR_BAND: (f64, f64) = (40.0, 200.0);
const STRAIGHT_S: f64 = 1.01;

fn resample(pts: &[Vec2], step: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    if pts.len() < 2 {
        return out;
    }
    out.push(pts[0]);
    let mut need = step;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let seg = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let mut t0 = 0.0;
        while need <= seg - t0 {
            let t = t0 + need;
            out.push(Vec2 { x: a.x + (b.x - a.x) * t / seg, y: a.y + (b.y - a.y) * t / seg });
            t0 = t;
            need = step;
        }
        need -= seg - t0;
    }
    out
}

fn quant(v: &mut Vec<f64>, p: f64) -> f64 {
    if v.is_empty() {
        return f64::NAN;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * p) as usize]
}

fn main() {
    print!("{{");
    for (bi, biome) in BiomeId::ALL.iter().enumerate() {
        // window sinuosities keyed by (window, order-class 0/1/2 = o1/o2/o3+)
        // order-classes o1/o2/o3+ and index 3 = pooled (all orders)
        let mut sin: Vec<Vec<Vec<f64>>> = vec![vec![Vec::new(); 4]; WINDOWS_M.len()];
        let mut straight_runs: Vec<f64> = Vec::new(); // arc m of s<STRAIGHT_S runs (600 m window)
        let mut par_runs: Vec<f64> = Vec::new(); // arc m of parallel-band runs
        let mut total_len = 0.0f64;
        let mut par_len = 0.0f64; // channel length sitting in runs > 400 m
        for k in 0..20u64 {
            let id = RunIdentity::from_seed(41_000 + k);
            let spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            // ---- window sinuosity + straight runs, per channel ----
            for ch in &sk.channels {
                let s10 = resample(&ch.pts, SAMPLE_M);
                total_len += (s10.len().saturating_sub(1)) as f64 * SAMPLE_M;
                let oc = (ch.order.min(3) as usize) - 1;
                for (wi, wm) in WINDOWS_M.iter().enumerate() {
                    let n = (wm / SAMPLE_M) as usize;
                    if s10.len() <= n {
                        continue;
                    }
                    let step = (50.0 / SAMPLE_M) as usize;
                    let mut run = 0.0f64;
                    for i in (0..s10.len() - n).step_by(step) {
                        let (a, b) = (s10[i], s10[i + n]);
                        let chord = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                        let s = wm / chord.max(1e-9);
                        sin[wi][oc].push(s);
                        sin[wi][3].push(s);
                        if *wm == 600.0 {
                            if s < STRAIGHT_S {
                                run += 50.0;
                            } else if run > 0.0 {
                                straight_runs.push(run + 600.0);
                                run = 0.0;
                            }
                        }
                    }
                    if *wm == 600.0 && run > 0.0 {
                        straight_runs.push(run + 600.0);
                    }
                }
            }
            // ---- parallel runs: 30 m samples, nearest other-channel sample ----
            let sampled: Vec<Vec<Vec2>> =
                sk.channels.iter().map(|c| resample(&c.pts, PAR_SAMPLE_M)).collect();
            for (i, si) in sampled.iter().enumerate() {
                let mut run = 0.0f64;
                for p in si {
                    let mut dmin = f64::INFINITY;
                    for (j, sj) in sampled.iter().enumerate() {
                        if j == i {
                            continue;
                        }
                        // skip the own-junction contact: parent/child pairs
                        // touch at the mouth by construction
                        let related = sk.channels[i].parent == Some(j as u32)
                            || sk.channels[j].parent == Some(i as u32);
                        for q in sj {
                            let d = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
                            if related && d < PAR_BAND.0 {
                                continue;
                            }
                            dmin = dmin.min(d);
                        }
                    }
                    if dmin >= PAR_BAND.0 && dmin <= PAR_BAND.1 {
                        run += PAR_SAMPLE_M;
                    } else {
                        if run > 400.0 {
                            par_runs.push(run);
                            par_len += run;
                        }
                        run = 0.0;
                    }
                }
                if run > 400.0 {
                    par_runs.push(run);
                    par_len += run;
                }
            }
        }
        if bi > 0 {
            print!(",");
        }
        print!("\"{}\":{{", biome.key());
        for (wi, wm) in WINDOWS_M.iter().enumerate() {
            for oc in 0..4 {
                let v = &mut sin[wi][oc];
                let frac_str = if v.is_empty() {
                    f64::NAN
                } else {
                    v.iter().filter(|s| **s < STRAIGHT_S).count() as f64 / v.len() as f64
                };
                let tag = if oc == 3 { "all".to_string() } else { format!("o{}", oc + 1) };
                print!(
                    "\"w{}_{}\":{{\"n\":{},\"p10\":{:.4},\"p50\":{:.4},\"p90\":{:.4},\"frac_straight\":{:.3}}},",
                    *wm as u32,
                    tag,
                    v.len(),
                    quant(v, 0.10),
                    quant(v, 0.50),
                    quant(v, 0.90),
                    frac_str
                );
            }
        }
        let mut sr = straight_runs.clone();
        let mut pr = par_runs.clone();
        print!(
            "\"straight_run_p50\":{:.0},\"straight_run_p90\":{:.0},\"straight_run_max\":{:.0},",
            quant(&mut sr, 0.5),
            quant(&mut sr, 0.9),
            sr.last().copied().unwrap_or(f64::NAN)
        );
        print!(
            "\"par_run_p50\":{:.0},\"par_run_p90\":{:.0},\"par_run_max\":{:.0},\"par_frac\":{:.3}}}",
            quant(&mut pr, 0.5),
            quant(&mut pr, 0.9),
            pr.last().copied().unwrap_or(f64::NAN),
            if total_len > 0.0 { par_len / total_len } else { 0.0 }
        );
    }
    println!("}}");
}
