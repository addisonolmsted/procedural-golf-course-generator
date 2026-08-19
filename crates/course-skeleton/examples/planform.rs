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
/// NEAR-parallel: the tight companion to `par_frac`, and the one that caught
/// the defect `par_frac` missed. The old measure bands at 40-200 m and only
/// counts runs over 400 m, so a channel 150 m from its neighbour scores the
/// same as one 30 m away and a 300 m run scores nothing at all. Review saw
/// "some get close to intersecting"; at 60 m with no run minimum the
/// generated network reads 3-10x the corpus rate on the integrated biomes.
const NEAR_SAMPLE_M: f64 = 20.0;
const NEAR_BAND_M: f64 = 60.0;
/// Junction exemption. Two reaches that MEET are within a cell of each other
/// by construction, so proximity is only evidence of a defect away from the
/// other reach's ends. Measured against the sample's distance to those ends,
/// not to the nearest point, so a tributary running alongside its own parent
/// still counts once it is clear of the confluence.
const NEAR_END_EXEMPT_M: f64 = 90.0;

/// ~50 m moving average, matching `real_planform.py`'s `smooth_resample`
/// exactly. Without it the two sides are not comparable: a traced path is
/// a chain of cell centres and its D8 staircase alone reads as ~1.3
/// sinuosity, which is what the first post-integration run reported.
fn smooth(pts: &[Vec2], cell: f64) -> Vec<Vec2> {
    let k = ((50.0 / cell).round() as usize).max(3) | 1;
    let h = k / 2;
    let n = pts.len();
    if n <= k {
        return pts.to_vec();
    }
    (0..n)
        .map(|i| {
            let (mut sx, mut sy) = (0.0, 0.0);
            for j in 0..k {
                let idx = (i + j).saturating_sub(h).min(n - 1);
                sx += pts[idx].x;
                sy += pts[idx].y;
            }
            Vec2 { x: sx / k as f64, y: sy / k as f64 }
        })
        .collect()
}

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
        let mut par_rel_len = [0.0f64; 3]; // by dominant relation: pc/sib/unrel
        let mut near_len = 0.0f64; // length within NEAR_BAND_M of another reach
        let mut near_total = 0.0f64; // total length on the NEAR_SAMPLE_M grid
        for k in 0..20u64 {
            let id = RunIdentity::from_seed(41_000 + k);
            let mut spec =
                SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: Some(*biome) });
            // ladder overrides, same env names as `s2_network`
            for (ev, dial) in [
                ("INFLOW_START", "skeleton.inflow_start"),
                ("ROUGH", "skeleton.roughness_frac"),
                ("WANDER", "skeleton.route_wander"),
                ("CREEP", "skeleton.creep"),
                ("MEXP", "skeleton.area_exp"),
                ("AREA_TH", "skeleton.area_threshold_m2"),
                ("SLOPE_FLOOR", "skeleton.slope_init_floor"),
                ("PRUNE", "skeleton.shadow_prune_m"),
                ("WAVE_BETA", "primitives.wave_beta"),
                ("WAVE_SHARE", "primitives.wave_share"),
                ("WAVE_ISO", "primitives.wave_iso_frac"),
            ] {
                if let Ok(v) = std::env::var(ev) {
                    if let Ok(x) = v.parse::<f64>() {
                        spec.dials.insert(dial.into(), x);
                    }
                }
            }
            let c1 = course_primitives::generate(&spec, &id);
            let sk = course_skeleton::generate(&spec, &c1, &id);
            // ---- window sinuosity + straight runs, per channel ----
            for ch in &sk.channels {
                let s10 = resample(&smooth(&ch.pts, 8.0), SAMPLE_M);
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
                sk.channels.iter().map(|c| resample(&smooth(&c.pts, 8.0), PAR_SAMPLE_M)).collect();
            for (i, si) in sampled.iter().enumerate() {
                let mut run = 0.0f64;
                let mut run_rel = [0usize; 3]; // parent-child / sibling / unrelated votes
                for p in si {
                    let mut dmin = f64::INFINITY;
                    let mut rel = 2usize;
                    for (j, sj) in sampled.iter().enumerate() {
                        if j == i {
                            continue;
                        }
                        // skip the own-junction contact: parent/child pairs
                        // touch at the mouth by construction
                        let related = sk.channels[i].parent == Some(j as u32)
                            || sk.channels[j].parent == Some(i as u32);
                        let sib = sk.channels[i].parent.is_some()
                            && sk.channels[i].parent == sk.channels[j].parent;
                        for q in sj {
                            let d = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
                            if d < dmin {
                                dmin = d;
                                rel = if related { 0 } else if sib { 1 } else { 2 };
                            }
                        }
                    }
                    if dmin >= PAR_BAND.0 && dmin <= PAR_BAND.1 {
                        run += PAR_SAMPLE_M;
                        run_rel[rel] += 1;
                    } else {
                        if run > 400.0 {
                            par_runs.push(run);
                            par_len += run;
                            let dom = (0..3).max_by_key(|k| run_rel[*k]).unwrap();
                            par_rel_len[dom] += run;
                        }
                        run = 0.0;
                        run_rel = [0; 3];
                    }
                }
                if run > 400.0 {
                    par_runs.push(run);
                    par_len += run;
                    let dom = (0..3).max_by_key(|k| run_rel[*k]).unwrap();
                    par_rel_len[dom] += run;
                }
            }
            // ---- near-parallel: 20 m samples, 60 m band, no run minimum ----
            let near: Vec<Vec<Vec2>> =
                sk.channels.iter().map(|c| resample(&smooth(&c.pts, 8.0), NEAR_SAMPLE_M)).collect();
            for (i, si) in near.iter().enumerate() {
                near_total += si.len() as f64 * NEAR_SAMPLE_M;
                for p in si {
                    let mut hit = false;
                    for (j, sj) in near.iter().enumerate() {
                        if j == i || sj.is_empty() {
                            continue;
                        }
                        let end = |q: &Vec2| ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
                        if end(&sj[0]).min(end(&sj[sj.len() - 1])) <= NEAR_END_EXEMPT_M {
                            continue;
                        }
                        if sj.iter().any(|q| end(q) < NEAR_BAND_M) {
                            hit = true;
                            break;
                        }
                    }
                    if hit {
                        near_len += NEAR_SAMPLE_M;
                    }
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
            "\"par_run_p50\":{:.0},\"par_run_p90\":{:.0},\"par_run_max\":{:.0},\"par_frac\":{:.3},\"par_pc\":{:.3},\"par_sib\":{:.3},\"par_unrel\":{:.3},\"near_par_frac\":{:.4},\"net_km\":{:.1}}}",
            quant(&mut pr, 0.5),
            quant(&mut pr, 0.9),
            pr.last().copied().unwrap_or(f64::NAN),
            if total_len > 0.0 { par_len / total_len } else { 0.0 },
            if total_len > 0.0 { par_rel_len[0] / total_len } else { 0.0 },
            if total_len > 0.0 { par_rel_len[1] / total_len } else { 0.0 },
            if total_len > 0.0 { par_rel_len[2] / total_len } else { 0.0 },
            if near_total > 0.0 { near_len / near_total } else { 0.0 },
            near_total / 20_000.0
        );
    }
    println!("}}");
}
