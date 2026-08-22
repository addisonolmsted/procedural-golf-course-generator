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

use crate::section::SideProgram;
use course_draw::Descriptors;
use course_network::TrunkPath;
use course_seed::DetRng;
use course_template::Template;
use course_world::grid::{Grid, GridSpec};
use course_world::math::{self, Vec2};
use course_world::noise;
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
                // Mouth-side extension ONLY for trunks whose mouth is a tile
                // edge. A JOINING trunk's mouth is a junction inside the
                // primary's valley — extending it carved a 1.6 km PHANTOM
                // valley straight across the primary (the X on hc seed 58).
                // No bullseye risk there either: the junction sits in the
                // primary's floor, where the envelope is already low.
                if tk.joins.is_none() {
                    let d0 = (pts[0] - pts[1]).normalized();
                    pts.insert(0, pts[0] + d0 * ext);
                    zs.insert(0, zs[0]);
                }
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
            TrunkGeom { ix, z: zs, cum, spine }
        })
        .collect();

    // ---- per-trunk, per-SIDE section programs (the U2 engine). Drawn
    // independently per side: asymmetry everywhere, not just rv terraces.
    let rec = course_draw::records::record(t.archetype);
    let programs: Vec<[SideProgram; 2]> = geoms
        .iter()
        .map(|g| {
            let bed = |arc: f64| -> f64 {
                let i = match g.cum.binary_search_by(|c| c.partial_cmp(&arc).unwrap()) {
                    Ok(i) => i.min(g.z.len() - 2),
                    Err(i) => i.saturating_sub(1).min(g.z.len() - 2),
                };
                let seg = (g.cum[i + 1] - g.cum[i]).max(1e-9);
                g.z[i] + (g.z[i + 1] - g.z[i]) * ((arc - g.cum[i]) / seg).clamp(0.0, 1.0)
            };
            let pos = |arc: f64| g.spine.point_at(arc / g.spine.length().max(1e-9));
            [
                SideProgram::draw(rng, rec.section, d, &t.fields.strata, &bed, &pos, g.spine.length(), rec.width_var),
                SideProgram::draw(rng, rec.section, d, &t.fields.strata, &bed, &pos, g.spine.length(), rec.width_var),
            ]
        })
        .collect();

    let fhw = d.floor_hw_m * d.floor_widen;
    let cap_m = d.relief_budget_m * 0.80;

    // ---- pass 1: per-trunk projection fields, then SMOOTH the distance.
    // Projecting to a polyline leaves the distance field C0 across the
    // medial axis and across segment switches on the outside of bends —
    // its gradient jumps, and every profile applied to it prints a radial
    // CREASE (user report, and the same class as heartland's "boxy cut").
    // A small Gaussian on d (and on the bed reference) restores C1 before
    // any profile touches it.
    let nn = n as usize;
    struct TF {
        /// Index of the TF this trunk joins (a confluence secondary), if any.
        joins: Option<usize>,
        dist: Vec<f64>,
        zbed: Vec<f64>,
        arc: Vec<f64>,
        /// SIGNED lateral offset (side × distance). The blend weight derives
        /// from it at eval: within ±140 m of the trunk the two side programs
        /// cross-fade, so the floor seam is a col; farther out the sign only
        /// flips across the medial axis — which the 500 m trunk-radius floor
        /// pushes into cap-flattened ground where both sides already agree.
        /// (A blurred binary side was tried first: a Gaussian wide enough to
        /// hide a 20 m program difference needs a ~300 m band, and 24 blur
        /// passes buy ~40 m.)
        lat: Vec<f64>,
    }
    let mut tfs: Vec<TF> = Vec::with_capacity(geoms.len());
    for g in &geoms {
        let mut dist = vec![0.0f64; nn * nn];
        let mut zbed = vec![0.0f64; nn * nn];
        let mut arcv = vec![0.0f64; nn * nn];
        let mut latv = vec![0.0f64; nn * nn];
        // Segment table for the SOFT-MIN frame. The exact projection frame
        // is only C0: dist, arc and side all jump across the medial axis,
        // and each jump printed its own artifact (radial creases, the
        // diagonal cliff, the gouged notch — and the user still saw creases
        // on the inside of bends). A log-sum-exp soft-min over segments,
        // with the same weights carried onto arc and side, makes the WHOLE
        // frame C1 in one mechanism instead of three patches.
        const SOFT_K: f64 = 45.0;
        let segs: Vec<(Vec2, Vec2, f64, f64)> = g
            .spine
            .pts
            .windows(2)
            .zip(g.cum.windows(2))
            .map(|(w, c)| (w[0], w[1], c[0], c[1] - c[0]))
            .collect();
        for gy in 0..nn {
            for gx in 0..nn {
                let p = spec.world_of(gx as u32, gy as u32);
                // exact nearest first (cheap reject threshold)
                let hit = g.spine.project_with(&g.ix, p);
                let dmin = hit.d;
                let mut wsum = 0.0;
                let mut dacc = 0.0;
                let mut aacc = 0.0;
                let mut lacc = 0.0;
                for (a, b, arc0, len) in &segs {
                    // coarse reject: segment cannot beat dmin + 5k
                    let mid = Vec2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
                    if p.distance(mid) - len * 0.5 > dmin + 5.0 * SOFT_K {
                        continue;
                    }
                    let v = Vec2::new(b.x - a.x, b.y - a.y);
                    let l2 = v.x * v.x + v.y * v.y;
                    let t = if l2 > 0.0 {
                        (((p.x - a.x) * v.x + (p.y - a.y) * v.y) / l2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    let q = Vec2::new(a.x + v.x * t, a.y + v.y * t);
                    let d = p.distance(q);
                    let side = if v.x * (p.y - q.y) - v.y * (p.x - q.x) >= 0.0 { 1.0 } else { -1.0 };
                    let w = math::exp((dmin - d) / SOFT_K);
                    wsum += w;
                    dacc += w * d;
                    aacc += w * (arc0 + len * t);
                    lacc += w * side * d;
                }
                let li = gy * nn + gx;
                if wsum > 0.0 {
                    dist[li] = dacc / wsum;
                    arcv[li] = aacc / wsum;
                    latv[li] = lacc / wsum;
                } else {
                    dist[li] = dmin;
                    arcv[li] = hit.u * g.spine.length();
                    latv[li] = hit.side * dmin;
                }
                // bed z from the SOFT arc, through the cum table
                let arc = arcv[li];
                let i = match g.cum.binary_search_by(|c| c.partial_cmp(&arc).unwrap()) {
                    Ok(i) => i.min(g.z.len() - 2),
                    Err(i) => i.saturating_sub(1).min(g.z.len() - 2),
                };
                let seg = (g.cum[i + 1] - g.cum[i]).max(1e-9);
                let f = ((arc - g.cum[i]) / seg).clamp(0.0, 1.0);
                zbed[li] = g.z[i] + (g.z[i + 1] - g.z[i]) * f;
            }
        }
        // one light pass for grid smoothing only
        gauss(&mut dist, nn, 1);
        tfs.push(TF { joins: None, dist, zbed, arc: arcv, lat: latv });
    }

    for (ti, tk) in trunks.iter().enumerate() {
        tfs[ti].joins = tk.joins.map(|j| j as usize);
    }

    // ---- pass 2: compose the envelope + interfluve + benches
    let mut height = Grid::filled(spec, 0.0);
    let mut d_trunk = Grid::filled(spec, f64::MAX);
    for gy in 0..nn {
        for gx in 0..nn {
            let p = spec.world_of(gx as u32, gy as u32);
            let li = gy * nn + gx;

            let mut cands: [f64; 4] = [f64::MAX; 4];
            let mut raws: [f64; 4] = [0.0; 4];
            let mut dmin = f64::MAX;
            for (ti, tf) in tfs.iter().enumerate() {
                let dist = tf.dist[li];
                if dist < dmin {
                    dmin = dist;
                }
                let sw = math::smoothstep(-140.0, 140.0, tf.lat[li]);
                let raw = if sw < 0.02 {
                    programs[ti][0].rise(tf.arc[li], dist)
                } else if sw > 0.98 {
                    programs[ti][1].rise(tf.arc[li], dist)
                } else {
                    programs[ti][0].rise(tf.arc[li], dist) * (1.0 - sw)
                        + programs[ti][1].rise(tf.arc[li], dist) * sw
                };
                raws[ti.min(3)] = raw;
            }
            // CONFLUENCE HANDOFF (U5): a joining trunk's section FADES IN
            // with its own arc — at the junction it contributes nothing of
            // its own, and its valley grows out of the primary's over the
            // first ~700 m. This merges the FRAMES; the stamped-overlay
            // look came from blending only the outputs.
            const HANDOFF_M: f64 = 700.0;
            for ti in 0..tfs.len().min(4) {
                if let Some(pj) = tfs[ti].joins {
                    if pj < 4 {
                        let h = math::smoothstep(0.0, HANDOFF_M, tfs[ti].arc[li]);
                        raws[ti] = raws[pj] * (1.0 - h) + raws[ti] * h;
                    }
                }
            }
            for (ti, tf) in tfs.iter().enumerate() {
                let k = cap_m.max(1.0);
                let a = raws[ti.min(3)] / k;
                let rise = k * a / (1.0 + a) * (1.0 + a / (1.0 + a));
                cands[ti.min(3)] = tf.zbed[li] + rise;
            }
            // DISTANCE-partition envelope (replaces the value-softmin).
            // Value-based weights let a far trunk's candidate contribute
            // wherever its HEIGHT was close — so trunk A's scarps printed
            // creases across trunk B's valley walls (user report). Weights
            // now depend only on each trunk's own distance field: a trunk
            // that is not nearby CANNOT contribute structure, whatever its
            // value. Each trunk owns its region; cols between valleys blend
            // where the distances tie, over a band that widens with height
            // up the walls.
            let mut z_env = if tfs.is_empty() {
                0.0
            } else {
                let k_d = 60.0 + 0.18 * dmin.min(900.0);
                let mut num = 0.0;
                let mut den = 0.0;
                for (ti, tf) in tfs.iter().enumerate() {
                    let w = math::exp(-(tf.dist[li] - dmin) / k_d);
                    num += cands[ti.min(3)] * w;
                    den += w;
                }
                num / den.max(1e-12)
            };
            if tfs.is_empty() {
                dmin = EXTENT_M;
            }

            // interfluve: high ground between valleys; on a ZERO-TRUNK tile
            // (heathland's 45% draw) this IS the macro. `ridge_elong` > 1
            // stretches the sampling frame along the grain axis, turning
            // isotropic swells into gentle 600-1500 m RIDGES (U6: heathland
            // is macro-only for now, and ridges are its macro form).
            let ramp = math::smoothstep(fhw + 40.0, fhw + 320.0, dmin);
            let p_rel = if d.ridge_elong > 1.01 {
                let (gc, gs) = (math::cos(t.fields.grain_axis_rad), math::sin(t.fields.grain_axis_rad));
                let al = p.x * gc + p.y * gs;
                let ac = -p.x * gs + p.y * gc;
                let half = EXTENT_M * 0.5;
                let alc = (al - half) / d.ridge_elong + half;
                Vec2::new(alc * gc - ac * gs, alc * gs + ac * gc)
            } else {
                p
            };
            z_env += W_INTERFLUVE
                * d.relief_budget_m
                * (t.fields.relief_pred.bilinear(p_rel) * 0.5 + 0.5)
                * ramp;
            let z = z_env;

            height.set(gx as u32, gy as u32, z);
            d_trunk.set(gx as u32, gy as u32, dmin);
        }
    }

    // CONFLUENCE SPUR ROUNDING: the wedge between two arms upstream of a
    // junction comes to a sharp point where the inner walls meet. A local
    // blur within ~320 m of each junction rounds the tip; the rest of the
    // tile is untouched. (User: acceptable if texture handles it — this is
    // cheaper and the tip is a macro form.)
    {
        let junctions: Vec<Vec2> = trunks
            .iter()
            .filter(|t| t.joins.is_some())
            .map(|t| t.pts[0])
            .collect();
        if !junctions.is_empty() {
            let src: Vec<f64> = height.data.clone();
            for gy in 2..nn - 2 {
                for gx in 2..nn - 2 {
                    let p = spec.world_of(gx as u32, gy as u32);
                    let dj = junctions.iter().fold(f64::MAX, |m, j| m.min(j.distance(p)));
                    if dj > 320.0 {
                        continue;
                    }
                    let w = 1.0 - math::smoothstep(120.0, 320.0, dj);
                    let mut acc = 0.0;
                    for oy in -2i64..=2 {
                        for ox in -2i64..=2 {
                            acc += src[(gy as i64 + oy) as usize * nn + (gx as i64 + ox) as usize];
                        }
                    }
                    let blurred = acc / 25.0;
                    let z0 = src[gy * nn + gx];
                    height.set(gx as u32, gy as u32, z0 * (1.0 - w) + blurred * w);
                }
            }
        }
    }

    MacroSurface { height, d_trunk }
}

/// Sandhills: the TWO FORMS of the previous generator, ported as knowledge
/// (course-primitives/src/generate.rs:165-290). A per-tile continuum scalar
/// spans strongly-oriented dune TRAINS (two beating sinusoids at 1100-1500 m
/// — two to three major ridges per tile — second component ×0.6 at λ×1.18
/// rotated ~5°, sinuous crests) to weakly-oriented MOUND FIELDS (ten
/// isotropic waves, λ log-uniform 1050-1550 m, variance-normalized). The
/// corpus split the old version was fit to: ⅓ strong trains, ~40% mounds,
/// the rest mixed. Relief golf-bounded; a slow cross-wind envelope waxes
/// and wanes the field.
pub fn build_aeolian(
    rng: &mut DetRng,
    t: &Template,
    d: &Descriptors,
) -> MacroSurface {
    let n = (EXTENT_M / RES_M).round() as u32 + 1;
    let spec = GridSpec::new(Vec2::new(0.0, 0.0), RES_M, n, n);
    let tau = core::f64::consts::TAU;
    let wind = t.fields.grain_axis_rad;
    let (wnx, wny) = (math::cos(wind), math::sin(wind));
    let (wcx, wcy) = (-wny, wnx);
    let wind2 = wind + 0.09;
    let (w2x, w2y) = (math::cos(wind2), math::sin(wind2));

    // the continuum: ~1/3 trains, ~40% mounds, rest mixed
    let u_cont = rng.next_f64();
    let w_train = u_cont * u_cont * (3.0 - 2.0 * u_cont);

    let lam = rng.range_f64(1100.0, 1500.0);
    let ph1 = rng.range_f64(0.0, tau);
    let ph2 = rng.range_f64(0.0, tau);
    let ph_swing = rng.range_f64(0.0, tau);
    let ph_env = rng.range_f64(0.0, tau);
    let mut mounds: Vec<(f64, f64, f64, f64)> = Vec::with_capacity(10);
    for _ in 0..10 {
        let ml = 1050.0 * math::exp(rng.next_f64() * math::ln(1550.0 / 1050.0));
        let dir = rng.range_f64(0.0, core::f64::consts::PI);
        let ph = rng.range_f64(0.0, tau);
        let a = 0.6 + 0.8 * rng.next_f64();
        mounds.push((ml, dir, ph, a));
    }
    let mound_var: f64 = mounds.iter().map(|(_, _, _, a)| a * a * 0.5).sum();
    let mound_norm = 1.0 / mound_var.sqrt().max(1e-9);
    let train_norm = 1.0 / math::pow((1.0 + 0.36) * 0.5, 0.5);
    let amp = d.dune_relief_m * 0.62;
    let s_base = rng.next_u32();

    let mut height = Grid::filled(spec, 0.0);
    let d_trunk = Grid::filled(spec, f64::MAX);
    for gy in 0..n {
        for gx in 0..n {
            let p = spec.world_of(gx, gy);
            let u1 = p.x * wnx + p.y * wny;
            let v = p.x * wcx + p.y * wcy;
            let u2 = p.x * w2x + p.y * w2y;
            let swing = 0.45 * math::sin(v / 1400.0 * tau + ph_swing);
            let train = train_norm
                * (math::sin(u1 / lam * tau + ph1 + swing)
                    + 0.6 * math::sin(u2 / (lam * 1.18) * tau + ph2));
            let mut mf = 0.0;
            for (ml, dir, ph, a) in &mounds {
                let m = p.x * math::cos(*dir) + p.y * math::sin(*dir);
                mf += a * math::sin(m / ml * tau + ph);
            }
            mf *= mound_norm;
            // no short envelope at macro — that was the MID-BAND recipe's
            // trick, and here it chopped the trains into segments. The S1
            // macro ran the beat + swing bare.
            let _ = ph_env;
            let field = w_train * train + (1.0 - w_train) * mf;
            let base = 0.10 * d.relief_budget_m * noise::perlin2(p.x / 1100.0, p.y / 1100.0, s_base);
            height.set(gx, gy, amp * field + base);
        }
    }
    MacroSurface { height, d_trunk }
}

/// Separable 5-tap Gaussian, `passes` iterations.
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
