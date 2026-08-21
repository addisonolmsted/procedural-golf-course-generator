//! T1 — the macro surface: the first real heightmap of attempt 4.
//!
//! Built FROM the trunks and the template fields, so the network and the
//! terrain cannot disagree (the whole point of the terrain-first
//! restructure). Composition is an ENVELOPE, never a sum:
//!
//!   z(p) = min over trunks[ z_trunk(nearest) + rise(d) ]  + interfluve + benches
//!
//! min-composition makes double-cutting unrepresentable: where two valleys
//! overlap, the surface is whichever is lower, and junctions blend into a
//! natural confluence bowl. (heartland composed valleys additively and its
//! border-moat bug lived exactly there.)
//!
//! Per-archetype character comes from the drawn record dials, with the
//! provenance split recorded in `records.rs`: measured catena shape
//! (corpus), terraces and bluffs and floor widening (golf — the macro is
//! designed for gameplay, 03-macro-is-designed §8).

use course_draw::Descriptors;
use course_network::TrunkPath;
use course_seed::DetRng;
use course_template::Template;
use course_world::grid::{Grid, GridSpec};
use course_world::math::{self, Vec2};
use course_world::spline::{SegIndex, Spine};
use course_world::world::EXTENT_M;

/// Macro resolution. 8 m, the working grid of the whole middle pipeline.
pub const RES_M: f64 = 8.0;

pub struct MacroSurface {
    pub height: Grid<f64>,
    /// Distance to the nearest trunk, kept for the tier machinery.
    pub d_trunk: Grid<f64>,
}

struct TrunkGeom {
    spine: Spine,
    ix: SegIndex,
    /// z per spine POINT (parallel to spine pts).
    z: Vec<f64>,
    /// Cumulative arc per spine point. z lookups MUST go through arc:
    /// `u * (len-1)` assumes equal arc per index, and the 1600 m virtual
    /// extension segments occupy ONE index each — the index-fraction mapping
    /// skewed every bed lookup (measured: bed 4.18 m read as 2.87 at a
    /// trunk head, surface 1.31 m below its own bed).
    cum: Vec<f64>,
    /// Which side (sign of cross product) carries the terrace flight.
    terrace_side: f64,
    both_sides: bool,
}

/// The interfluve weight on `relief_pred`, as a fraction of the relief
/// budget at full ramp. Rolling ground between valleys.
const W_INTERFLUVE: f64 = 0.35;

pub fn build(
    rng: &mut DetRng,
    t: &Template,
    d: &Descriptors,
    trunks: &[TrunkPath],
) -> MacroSurface {
    let n = (EXTENT_M / RES_M).round() as u32 + 1;
    let spec = GridSpec::new(Vec2::new(0.0, 0.0), RES_M, n, n);

    // --- per-trunk geometry + terrace draws
    let geoms: Vec<TrunkGeom> = trunks
        .iter()
        .map(|tk| {
            // Extend both ends virtually along their tangents: distance to a
            // FINITE polyline turns into distance-to-endpoint past the head,
            // and the surface grows a bullseye around it. z extends flat.
            let mut pts = tk.pts.clone();
            let mut zs = tk.z.clone();
            let ext = 1600.0;
            if pts.len() >= 2 {
                let d0 = (pts[0] - pts[1]).normalized();
                pts.insert(0, pts[0] + d0 * ext);
                zs.insert(0, zs[0]);
                let m = pts.len();
                let d1 = (pts[m - 1] - pts[m - 2]).normalized();
                pts.push(pts[m - 1] + d1 * ext);
                zs.push(*zs.last().unwrap());
            }
            let mut cum = Vec::with_capacity(pts.len());
            let mut acc = 0.0;
            cum.push(0.0);
            for w in pts.windows(2) {
                acc += w[0].distance(w[1]);
                cum.push(acc);
            }
            let spine = Spine::new(pts);
            let ix = SegIndex::for_spine(&spine);
            TrunkGeom {
                ix,
                z: zs,
                cum,
                spine,
                terrace_side: if rng.next_f64() < 0.5 { 1.0 } else { -1.0 },
                both_sides: rng.next_f64() > d.terrace_asymmetry,
            }
        })
        .collect();

    let steps = d.terrace_steps as f64;
    let tread_w = if steps > 0.0 { rng.range_f64(90.0, 150.0) } else { 0.0 };
    let riser_h = if steps > 0.0 {
        (d.relief_budget_m * 0.55 / steps).clamp(4.0, 20.0)
    } else {
        0.0
    };
    let bench_period = (d.riser_m * 3.2).max(1.0);
    let bench_k = d.resistance_response;
    let fhw = d.floor_hw_m * d.floor_widen;
    let cap_m = d.relief_budget_m * 0.80;
    let r400 = d.rise_400_m;
    let cexp = d.catena_exp;

    // ---- pass 1: per-trunk projection fields, then SMOOTH the distance.
    // Projecting to a polyline leaves the distance field C0 across the
    // medial axis and across segment switches on the outside of bends —
    // its gradient jumps, and every profile applied to it prints a radial
    // CREASE (user report, and the same class as heartland's "boxy cut").
    // A small Gaussian on d (and on the bed reference) restores C1 before
    // any profile touches it.
    let nn = n as usize;
    struct TF {
        dist: Vec<f64>,
        zbed: Vec<f64>,
        terraced: Vec<bool>,
    }
    let mut tfs: Vec<TF> = Vec::with_capacity(geoms.len());
    for g in &geoms {
        let mut dist = vec![0.0f64; nn * nn];
        let mut zbed = vec![0.0f64; nn * nn];
        let mut terr = vec![false; nn * nn];
        for gy in 0..nn {
            for gx in 0..nn {
                let p = spec.world_of(gx as u32, gy as u32);
                let hit = g.spine.project_with(&g.ix, p);
                dist[gy * nn + gx] = hit.d;
                let arc = hit.u * g.spine.length();
                // binary search the cum table, interp z by arc
                let i = match g.cum.binary_search_by(|c| c.partial_cmp(&arc).unwrap()) {
                    Ok(i) => i.min(g.z.len() - 2),
                    Err(i) => i.saturating_sub(1).min(g.z.len() - 2),
                };
                let seg = (g.cum[i + 1] - g.cum[i]).max(1e-9);
                let f = ((arc - g.cum[i]) / seg).clamp(0.0, 1.0);
                zbed[gy * nn + gx] = g.z[i] + (g.z[i + 1] - g.z[i]) * f;
                terr[gy * nn + gx] =
                    steps > 0.0 && (hit.side * g.terrace_side > 0.0 || g.both_sides);
            }
        }
        // Smooth ONLY the distance. Smoothing the bed reference mixed the
        // downstream limb's lower z into cells near a bend and pulled the
        // surface 1.3 m BELOW the bed at a trunk head (bed-preservation
        // test). The creases live in dist's gradient; zbed is already smooth
        // along the channel, and its jump across the medial axis is a small
        // Δz the softmin knee absorbs.
        gauss(&mut dist, nn, 2);
        tfs.push(TF { dist, zbed, terraced: terr });
    }

    // ---- pass 2: compose the envelope + interfluve + benches
    let mut height = Grid::filled(spec, 0.0);
    let mut d_trunk = Grid::filled(spec, f64::MAX);
    for gy in 0..nn {
        for gx in 0..nn {
            let p = spec.world_of(gx as u32, gy as u32);
            let li = gy * nn + gx;

            let mut cands: [f64; 4] = [f64::MAX; 4];
            let mut dmin = f64::MAX;
            for (ti, tf) in tfs.iter().enumerate() {
                let dist = tf.dist[li];
                if dist < dmin {
                    dmin = dist;
                }
                let u = u_of(dist, fhw);
                let rise = if dist <= fhw {
                    0.0
                } else if tf.terraced[li] {
                    let k = (u / tread_w).floor().min(steps);
                    let frac = ((u / tread_w) - k).clamp(0.0, 1.0);
                    let soft = math::smoothstep(0.72, 1.0, frac);
                    let stair = (k + soft) * riser_h;
                    if k >= steps {
                        stair + catena(u - steps * tread_w, r400, cexp, fhw, cap_m)
                    } else {
                        stair
                    }
                } else {
                    catena(u, r400, cexp, fhw, cap_m)
                };
                cands[ti.min(3)] = tf.zbed[li] + rise;
            }
            // softmin envelope with a 6 m knee
            let hard = cands.iter().cloned().fold(f64::MAX, f64::min);
            let mut z_env = if hard == f64::MAX { 0.0 } else {
                const K: f64 = 6.0;
                let mut num = 0.0;
                let mut den = 0.0;
                for c in cands.iter().take(tfs.len()) {
                    let w = math::exp(-(c - hard) / K);
                    num += c * w;
                    den += w;
                }
                num / den.max(1e-12)
            };
            if tfs.is_empty() {
                dmin = EXTENT_M;
            }

            // interfluve: high ground between valleys; on a ZERO-TRUNK tile
            // (heathland's 45% draw) this IS the macro
            let ramp = math::smoothstep(fhw + 40.0, fhw + 320.0, dmin);
            z_env += W_INTERFLUVE
                * d.relief_budget_m
                * (t.fields.relief_pred.bilinear(p) * 0.5 + 0.5)
                * ramp;
            let mut z = z_env;

            if bench_k > 0.05 && dmin > fhw {
                let hard_here = t.fields.strata.hardness_at(p, z);
                let cell_f = z / bench_period;
                let k = cell_f.floor();
                let frac = cell_f - k;
                let soft = math::smoothstep(0.72, 0.97, frac);
                let quant = (k + soft) * bench_period;
                let w = (bench_k * 1.25).min(0.95) * hard_here * ramp;
                z = z * (1.0 - w) + quant * w;
            }

            if std::env::var("T1_DEBUG").is_ok() {
                let px = std::env::var("T1_DEBUG").unwrap();
                let parts: Vec<usize> = px.split(',').filter_map(|v| v.parse().ok()).collect();
                if parts.len() == 2 && gx == parts[0] && gy == parts[1] {
                    eprintln!("cell ({gx},{gy}) world ({:.0},{:.0}): dist_sm {:.2} zbed {:.2} dmin {:.2} ramp {:.3} z {:.3}",
                        p.x, p.y, tfs[0].dist[li], tfs[0].zbed[li], dmin,
                        math::smoothstep(fhw + 40.0, fhw + 320.0, dmin), z);
                }
            }
            height.set(gx as u32, gy as u32, z);
            d_trunk.set(gx as u32, gy as u32, dmin);
        }
    }

    MacroSurface { height, d_trunk }
}

/// Separable 5-tap Gaussian, `passes` iterations. Restores C1 to the
/// projected distance field before profiles are applied to it.
fn gauss(v: &mut [f64], nn: usize, passes: usize) {
    const K: [f64; 5] = [0.0625, 0.25, 0.375, 0.25, 0.0625];
    let mut tmp = vec![0.0f64; v.len()];
    for _ in 0..passes {
        for y in 0..nn {
            for x in 0..nn {
                let mut acc = 0.0;
                for (o, k) in K.iter().enumerate() {
                    let xx = (x as i64 + o as i64 - 2).clamp(0, nn as i64 - 1) as usize;
                    acc += v[y * nn + xx] * k;
                }
                tmp[y * nn + x] = acc;
            }
        }
        for y in 0..nn {
            for x in 0..nn {
                let mut acc = 0.0;
                for (o, k) in K.iter().enumerate() {
                    let yy = (y as i64 + o as i64 - 2).clamp(0, nn as i64 - 1) as usize;
                    acc += tmp[yy * nn + x] * k;
                }
                v[y * nn + x] = acc;
            }
        }
    }
}

fn u_of(dist: f64, fhw: f64) -> f64 {
    (dist - fhw).max(0.0)
}

/// The measured catena: rise = R400 · (u / (400 − fhw))^exp, extrapolated
/// past 400 m on the same power, then SOFT-CAPPED near the relief budget.
fn catena(u: f64, r400: f64, cexp: f64, fhw: f64, cap: f64) -> f64 {
    let denom = (400.0 - fhw).max(60.0);
    let r = r400 * math::pow((u / denom).max(0.0), cexp);
    let k = cap.max(1.0);
    k * (r / k) / (1.0 + r / k) * (1.0 + r / k / (1.0 + r / k))
}
