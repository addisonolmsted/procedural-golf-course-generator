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
    /// z per arc fraction (parallel to spine pts).
    z: Vec<f64>,
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
            // and the surface grows concentric arcs around it (visible as a
            // bullseye radiating from where the trunk leaves the tile).
            // z extends flat at the end values.
            let mut pts = tk.pts.clone();
            let mut zs = tk.z.clone();
            let ext = 1600.0;
            if pts.len() >= 2 {
                let d0 = (pts[0] - pts[1]).normalized();
                pts.insert(0, pts[0] + d0 * ext);
                zs.insert(0, zs[0]);
                let n = pts.len();
                let d1 = (pts[n - 1] - pts[n - 2]).normalized();
                pts.push(pts[n - 1] + d1 * ext);
                zs.push(*zs.last().unwrap());
            }
            let spine = Spine::new(pts);
            let ix = SegIndex::for_spine(&spine);
            TrunkGeom {
                ix,
                z: zs,
                spine,
                terrace_side: if rng.next_f64() < 0.5 { 1.0 } else { -1.0 },
                both_sides: rng.next_f64() > d.terrace_asymmetry,
            }
        })
        .collect();

    // terrace geometry (river valley: steps >= 2; riser from the budget so
    // the flight reads as landform -- 03 §8)
    let steps = d.terrace_steps as f64;
    let tread_w = if steps > 0.0 { rng.range_f64(90.0, 150.0) } else { 0.0 };
    let riser_h = if steps > 0.0 {
        (d.relief_budget_m * 0.55 / steps).clamp(4.0, 20.0)
    } else {
        0.0
    };

    // benching (hill country / plains: resistance_response > ~0.25)
    let bench_period = (d.riser_m * 3.2).max(1.0);
    let bench_k = d.resistance_response;

    let fhw = d.floor_hw_m * d.floor_widen;
    let cap_m = d.relief_budget_m * 0.80;
    let r400 = d.rise_400_m;
    let cexp = d.catena_exp;

    let mut height = Grid::filled(spec, 0.0);
    let mut d_trunk = Grid::filled(spec, f64::MAX);

    for gy in 0..n {
        for gx in 0..n {
            let p = spec.world_of(gx, gy);

            // ---- envelope over trunks: SOFT min, so the crossover between
            // two candidates is a smooth col rather than a crease (hard min
            // printed faint straight seams where candidates switch)
            let mut z_env = f64::MAX;
            let mut soft_acc = 0.0f64;
            let mut soft_n = 0u32;
            let mut dmin = f64::MAX;
            for g in &geoms {
                let hit = g.spine.project_with(&g.ix, p);
                let dist = hit.d;
                if dist < dmin {
                    dmin = dist;
                }
                // bed z at the projection, interpolated along the polyline
                let zi = {
                    let f = hit.u * (g.z.len() - 1) as f64;
                    let i = (f.floor() as usize).min(g.z.len() - 2);
                    g.z[i] + (g.z[i + 1] - g.z[i]) * (f - i as f64)
                };
                let terraced = steps > 0.0
                    && (hit.side * g.terrace_side > 0.0 || g.both_sides);

                let u = u_of(dist, fhw);
                let rise = if dist <= fhw {
                    0.0
                } else if terraced {
                    // the stair: soft risers between flat treads
                    let k = (u / tread_w).floor().min(steps);
                    let frac = ((u / tread_w) - k).clamp(0.0, 1.0);
                    let soft = math::smoothstep(0.72, 1.0, frac);
                    let stair = (k + soft) * riser_h;
                    if k >= steps {
                        // above the flight: resume the catena from its top
                        stair + catena(u - steps * tread_w, r400, cexp, fhw, cap_m)
                    } else {
                        stair
                    }
                } else {
                    catena(u, r400, cexp, fhw, cap_m)
                };
                let cand = zi + rise;
                if cand < z_env {
                    z_env = cand;
                }
                soft_acc += cand;
                soft_n += 1;
            }
            if soft_n > 1 {
                // softmin with a 6 m knee: exact away from crossovers
                const K: f64 = 6.0;
                let mut num = 0.0;
                let mut den = 0.0;
                // recompute weights against the hard min (numerically tame)
                // NOTE: two-pass over trunks is fine — trunk count <= 3
                let _ = soft_acc;
                for g in &geoms {
                    let hit = g.spine.project_with(&g.ix, p);
                    let zi = {
                        let f = hit.u * (g.z.len() - 1) as f64;
                        let i = (f.floor() as usize).min(g.z.len() - 2);
                        g.z[i] + (g.z[i + 1] - g.z[i]) * (f - i as f64)
                    };
                    let u = u_of(hit.d, fhw);
                    let terraced = steps > 0.0 && (hit.side * g.terrace_side > 0.0 || g.both_sides);
                    let rise = if hit.d <= fhw {
                        0.0
                    } else if terraced {
                        let k = (u / tread_w).floor().min(steps);
                        let frac = ((u / tread_w) - k).clamp(0.0, 1.0);
                        let soft = math::smoothstep(0.72, 1.0, frac);
                        let stair = (k + soft) * riser_h;
                        if k >= steps { stair + catena(u - steps * tread_w, r400, cexp, fhw, cap_m) } else { stair }
                    } else {
                        catena(u, r400, cexp, fhw, cap_m)
                    };
                    let cand = zi + rise;
                    let w = math::exp(-(cand - z_env) / K);
                    num += cand * w;
                    den += w;
                }
                z_env = num / den.max(1e-12);
            }
            if !z_env.is_finite() {
                // no trunks (heathland's 55% zero-trunk draw): the macro is
                // the relief field alone, subdued; kettles arrive at T4
                z_env = 0.0;
                dmin = EXTENT_M;
            }

            // ---- interfluve: relief_pred shapes the high ground only; the
            // ramp keeps the valley floor and walls untouched near the trunk
            let ramp = math::smoothstep(fhw + 40.0, fhw + 320.0, dmin);
            let mut z = z_env
                + W_INTERFLUVE
                    * d.relief_budget_m
                    * (t.fields.relief_pred.bilinear(p) * 0.5 + 0.5)
                    * ramp;

            // ---- benching: soft-quantize the slope into treads and risers
            // where the record calls for it. Strength scales with the
            // hardness CONTRAST at this elevation, so the bench pattern is
            // the strata column expressed on real heights (the M1 lesson:
            // the true map pattern needs real elevations).
            if bench_k > 0.05 && dmin > fhw {
                let hard = t.fields.strata.hardness_at(p, z);
                let cell_f = z / bench_period;
                let k = cell_f.floor();
                let frac = cell_f - k;
                // riser confined to the top ~28% of the period, so treads
                // are genuinely flat and risers genuinely steep — at the
                // first render benches read as faint banding, not bluffs
                let soft = math::smoothstep(0.72, 0.97, frac);
                let quant = (k + soft) * bench_period;
                let w = (bench_k * 1.25).min(0.95) * hard * ramp;
                z = z * (1.0 - w) + quant * w;
            }

            height.set(gx, gy, z);
            d_trunk.set(gx, gy, dmin);
        }
    }

    MacroSurface { height, d_trunk }
}

fn u_of(dist: f64, fhw: f64) -> f64 {
    (dist - fhw).max(0.0)
}

/// The measured catena: rise = R400 · (u / (400 − fhw))^exp, extrapolated
/// past 400 m on the same power, then SOFT-CAPPED near the relief budget —
/// unbounded extrapolation ran hill country to 200 m of relief against a
/// 60–110 m budget (measured on the first T1 render).
fn catena(u: f64, r400: f64, cexp: f64, fhw: f64, cap: f64) -> f64 {
    let denom = (400.0 - fhw).max(60.0);
    let r = r400 * math::pow((u / denom).max(0.0), cexp);
    // smooth min against the cap: exact for r << cap, asymptotic to cap
    let k = cap.max(1.0);
    k * (r / k) / (1.0 + r / k)
        * (1.0 + r / k / (1.0 + r / k)) // ~r for small r, -> k for large
}
