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
        tfs.push(TF { dist, zbed, arc: arcv, lat: latv });
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
                let sw = math::smoothstep(-140.0, 140.0, tf.lat[li]);
                let raw = if sw < 0.02 {
                    programs[ti][0].rise(tf.arc[li], dist)
                } else if sw > 0.98 {
                    programs[ti][1].rise(tf.arc[li], dist)
                } else {
                    programs[ti][0].rise(tf.arc[li], dist) * (1.0 - sw)
                        + programs[ti][1].rise(tf.arc[li], dist) * sw
                };
                // soft budget cap, as before
                let k = cap_m.max(1.0);
                let a = raw / k;
                let rise = k * a / (1.0 + a) * (1.0 + a / (1.0 + a));
                cands[ti.min(3)] = tf.zbed[li] + rise;
            }
            // Softmin envelope with an ADAPTIVE knee: 6 m read as two
            // valleys stamped on top of each other at junctions (user
            // report) — the col between candidates blended over only ~6 m.
            // The knee grows with distance-to-trunk, so junction floors
            // merge broadly while distinct far walls still cross cleanly.
            let hard = cands.iter().cloned().fold(f64::MAX, f64::min);
            let mut z_env = if hard == f64::MAX { 0.0 } else {
                let k = 4.0 + 0.13 * dmin.min(700.0);
                let mut num = 0.0;
                let mut den = 0.0;
                for c in cands.iter().take(tfs.len()) {
                    let w = math::exp(-(c - hard) / k);
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
            let z = z_env;

            height.set(gx as u32, gy as u32, z);
            d_trunk.set(gx as u32, gy as u32, dmin);
        }
    }

    // ---- heathland kettles: closed depressions ARE the identity landform
    // (literature 40-400 m diameter, 2-12 m deep), plus a hummock band so
    // the ground between them reads kame-and-kettle rather than noise.
    if d.kettle_count > 0 {
        let s_hum = rng.next_u32();
        for gy in 0..nn {
            for gx in 0..nn {
                let p = spec.world_of(gx as u32, gy as u32);
                let hum = 0.14
                    * d.relief_budget_m
                    * (0.6 * noise::perlin2(p.x / 260.0, p.y / 260.0, s_hum)
                        + 0.4 * noise::perlin2(p.x / 140.0, p.y / 140.0, s_hum.wrapping_add(9)));
                let z = height.get(gx as u32, gy as u32) + hum;
                height.set(gx as u32, gy as u32, z);
            }
        }
        for _ in 0..d.kettle_count {
            let cx = rng.range_f64(200.0, EXTENT_M - 200.0);
            let cy = rng.range_f64(200.0, EXTENT_M - 200.0);
            let c = Vec2::new(cx, cy);
            // keep kettles off the trunk floor
            if d_trunk.bilinear(c) < d.floor_hw_m + 60.0 {
                continue;
            }
            let r = rng.range_f64(40.0, 190.0);
            let depth = rng.range_f64(d.kettle_depth_m * 0.4, d.kettle_depth_m).max(1.0);
            let (glo, ghi) = (
                (((cx - r - 16.0) / RES_M).floor().max(0.0) as u32, ((cy - r - 16.0) / RES_M).floor().max(0.0) as u32),
                (((cx + r + 16.0) / RES_M).ceil().min(nn as f64 - 1.0) as u32, ((cy + r + 16.0) / RES_M).ceil().min(nn as f64 - 1.0) as u32),
            );
            for gy in glo.1..=ghi.1 {
                for gx in glo.0..=ghi.0 {
                    let p = spec.world_of(gx, gy);
                    let t = (p.distance(c) / r).min(1.0);
                    // smooth bowl: deepest at centre, C1 rim
                    let bowl = depth * (1.0 - math::smoothstep(0.0, 1.0, t));
                    let z = height.get(gx, gy) - bowl;
                    height.set(gx, gy, z);
                }
            }
        }
    }

    MacroSurface { height, d_trunk }
}

/// Sandhills: large parallel dune trains. Crests run along the template's
/// grain axis, wobbling at kilometre scale; amplitude modulates ALONG the
/// crest so ridges segment into barchanoid hummocks where it dips; the
/// profile is asymmetric (gentle stoss, steeper lee) and interdune ground
/// is broad and flat. Literature scale, golf-bounded relief
/// (04-landform-literature).
pub fn build_aeolian(
    rng: &mut DetRng,
    t: &Template,
    d: &Descriptors,
) -> MacroSurface {
    let n = (EXTENT_M / RES_M).round() as u32 + 1;
    let spec = GridSpec::new(Vec2::new(0.0, 0.0), RES_M, n, n);
    let axis = t.fields.grain_axis_rad;
    let (tx, ty) = (math::cos(axis), math::sin(axis));
    let (nx_, ny_) = (-ty, tx);
    let lam = d.dune_lam_m.max(400.0);
    let amp = d.dune_relief_m * 0.5;
    let s_wob = rng.next_u32();
    let s_amp = rng.next_u32();
    let s_base = rng.next_u32();
    let phase0 = rng.range_f64(0.0, core::f64::consts::TAU);

    let mut height = Grid::filled(spec, 0.0);
    let d_trunk = Grid::filled(spec, f64::MAX);
    for gy in 0..n {
        for gx in 0..n {
            let p = spec.world_of(gx, gy);
            let along = p.x * tx + p.y * ty;
            let across = p.x * nx_ + p.y * ny_;
            // crest wobble at km scale keeps ridges parallel but alive
            let wob = 0.22 * lam * noise::perlin2(along / 1900.0, across / 2600.0, s_wob);
            let ph = (across + wob) / lam * core::f64::consts::TAU + phase0;
            // asymmetric profile: skewed sine (gentle stoss, steep lee)
            let sk = math::sin(ph + 0.62 * math::cos(ph));
            // dune body above broad flat interdune ground
            let body = math::pow(sk.max(0.0), 1.9);
            // amplitude modulates along-crest: ridges break into hummocks
            let am: f64 = 0.55
                + 0.45 * noise::perlin2(along / 1500.0, across / 3000.0, s_amp);
            let base = 0.18
                * d.relief_budget_m
                * noise::perlin2(p.x / 900.0, p.y / 900.0, s_base);
            height.set(gx, gy, amp * 2.0 * body * am.max(0.15) + base
                + 0.15 * d.relief_budget_m * (t.fields.relief_pred.bilinear(p) * 0.5 + 0.5));
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

