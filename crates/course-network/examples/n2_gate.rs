//! Phase N2 gate: junction angles, near-parallel fraction, source survival.
//!
//! corpus: junction p50 37-45 deg | >80 deg 8.5-13.4% | near_par pooled 1.3-3.1%

use course_draw::{generate, Archetype};
use course_network::build;
use course_seed::RunIdentity;
use course_world::math::{self, Vec2};

/// Junction angle, corpus convention: the trib's incoming direction vs the
/// parent's downstream continuation, both averaged over ~100 m.
fn junction_angles(net: &course_network::Network) -> Vec<f64> {
    // Rebuild the incremental channel set EXACTLY as the build saw it —
    // trunks, then each trib in placement order — so attach_pt resolves
    // against the right geometry. Resolving against a trunks-only set
    // misattributed every trib-on-trib junction to a random trunk point,
    // which polluted the >80 deg share.
    let mut chans = course_network::proto::ChannelSet::new();
    for tk in &net.trunks {
        chans.add_polyline(&tk.pts, &tk.z);
    }
    let mut out = Vec::new();
    for tb in &net.tribs {
        let a = tb.attach_pt as usize;
        if a < chans.pts.len() {
            let k = 3.min(tb.pts.len() - 1);
            let p0 = tb.pts[0];
            let inc = Vec2::new(p0.x - tb.pts[k].x, p0.y - tb.pts[k].y);
            if inc.length() > 1e-6 {
                let down = chans.down[a];
                let c = inc.normalized().dot(down).clamp(-1.0, 1.0);
                out.push(math::acos(c).to_degrees());
            }
        }
        chans.add_polyline(&tb.pts, &tb.z);
    }
    out
}

/// Pooled near-parallel: 20 m samples, 60 m band, 90 m endpoint exemption —
/// the declared policy's constants (real_planform.py twin).
///
/// Measured over REACHES, not whole polylines: the corpus policy splits at
/// every confluence, so a junction is a reach ENDPOINT and the 90 m
/// exemption covers the approach zone. Measured over unsplit polylines the
/// same geometry double-counts every junction approach as a parallel run —
/// that error read 7.0% against a 1.3-3.1 band.
fn near_parallel(net: &course_network::Network) -> (f64, f64) {
    // gather polylines + the arc positions where children attach
    let mut polys: Vec<Vec<Vec2>> = Vec::new();
    for tk in &net.trunks {
        polys.push(tk.pts.clone());
    }
    for tb in &net.tribs {
        polys.push(tb.pts.clone());
    }
    // split points per polyline: nearest vertex to each junction
    let mut cuts: Vec<Vec<usize>> = vec![Vec::new(); polys.len()];
    for tb in &net.tribs {
        let jp = tb.pts[0];
        // find owning polyline + vertex
        let mut best = (usize::MAX, usize::MAX, f64::MAX);
        for (pi, poly) in polys.iter().enumerate() {
            for (vi, v) in poly.iter().enumerate() {
                let d = v.distance(jp);
                if d < best.2 {
                    best = (pi, vi, d);
                }
            }
        }
        if best.2 < 30.0 && !std::ptr::eq(&polys[best.0], &polys[0]) || best.2 < 30.0 {
            cuts[best.0].push(best.1);
        }
    }
    // convergent trunk junctions cut the primary too
    for tk in &net.trunks {
        if tk.joins.is_some() {
            let jp = tk.pts[0];
            let mut best = (usize::MAX, usize::MAX, f64::MAX);
            for (pi, poly) in polys.iter().enumerate() {
                for (vi, v) in poly.iter().enumerate() {
                    let d = v.distance(jp);
                    if d < best.2 {
                        best = (pi, vi, d);
                    }
                }
            }
            if best.2 < 30.0 {
                cuts[best.0].push(best.1);
            }
        }
    }
    let mut reaches: Vec<Vec<Vec2>> = Vec::new();
    for (pi, poly) in polys.iter().enumerate() {
        let mut cs = cuts[pi].clone();
        cs.sort_unstable();
        cs.dedup();
        let mut start = 0usize;
        for c in cs {
            if c > start + 1 {
                reaches.push(poly[start..=c].to_vec());
                start = c;
            }
        }
        if start + 1 < poly.len() {
            reaches.push(poly[start..].to_vec());
        }
    }

    let lines: Vec<Vec<Vec2>> = reaches.iter().map(|r| resample(r, 20.0)).collect();
    let mut near = 0.0;
    let mut total = 0.0;
    for (i, li) in lines.iter().enumerate() {
        total += (li.len() as f64) * 20.0;
        for s in li {
            let mut hit = false;
            for (j, lj) in lines.iter().enumerate() {
                if i == j {
                    continue;
                }
                let ends = [lj[0], *lj.last().unwrap()];
                for q in lj {
                    if s.distance(*q) <= 60.0 {
                        if ends.iter().all(|e| e.distance(*s) > 90.0) {
                            hit = true;
                        }
                        break;
                    }
                }
                if hit {
                    break;
                }
            }
            if hit {
                near += 20.0;
            }
        }
    }
    (near, total)
}

fn resample(pts: &[Vec2], step: f64) -> Vec<Vec2> {
    let sp = course_world::spline::Spine::new(pts.to_vec());
    let n = (sp.length() / step).max(1.0) as usize;
    (0..=n).map(|i| sp.point_at(i as f64 / n as f64)).collect()
}

fn main() {
    let seeds: Vec<u64> = {
        let v: Vec<u64> = std::env::args().skip(1).filter_map(|s| s.parse().ok()).collect();
        if v.is_empty() { (1..=12).collect() } else { v }
    };
    println!("N2 gate — corpus: junc p50 37-45 | >80 8.5-13.4% | near_par 1.3-3.1%\n");
    println!("{:<14} {:>6} {:>7} {:>8} {:>7} {:>7} {:>8} {:>6}",
             "archetype", "tribs", "stub", "junc p50", ">80%", "nearpar", "len km", "ms");
    for a in Archetype::ALL {
        let (mut nt, mut nd, mut nde, mut ndo, mut ang, mut nl, mut tl, mut lens) =
            (0u32, 0u32, 0u32, 0u32, Vec::new(), 0.0, 0.0, Vec::new());
        let mut ms = 0.0;
        for &s in &seeds {
            let id = RunIdentity::from_seed(s);
            let t = course_template::build(&id, &generate(&id, Some(a)));
            let t0 = std::time::Instant::now();
            let net = build(&id, &t);
            ms += t0.elapsed().as_secs_f64() * 1000.0;
            nt += net.tribs.len() as u32;
            nd += net.n_stub;
            nde += net.n_edge; ndo += net.n_claimed;
            ang.extend(junction_angles(&net));
            let (n, tot) = near_parallel(&net);
            nl += n;
            tl += tot;
            let mut len = net.trunk_len_m();
            for tb in &net.tribs {
                len += tb.pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
            }
            lens.push(len / 1000.0);
        }
        if ang.is_empty() {
            println!("{:<14} {:>6} {:>3}e/{:>3}c", a.key(), nt, nde, ndo);
            continue;
        }
        ang.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let p50 = ang[ang.len() / 2];
        let g80 = 100.0 * ang.iter().filter(|v| **v > 80.0).count() as f64 / ang.len() as f64;
        let np = 100.0 * nl / tl.max(1.0);
        let ml = lens.iter().sum::<f64>() / lens.len().max(1) as f64;
        println!("{:<14} {:>6} {:>3}e/{:>3}c {:>8.1} {:>7.1} {:>7.2} {:>8.2} {:>6.1}",
                 a.key(), nt, nde, ndo, p50, g80, np, ml, ms / seeds.len() as f64);
        let _ = nd;
    }
}
