//! A5 — blowouts and deflation pans.
//!
//! **Blowouts are the bunker vocabulary**, so they are PLACED FEATURES with
//! known locations — a list routing can consume — not texture. Each is a
//! steep-walled bowl cut into high ground with a **mass-conserved downwind
//! apron**: the sand that left the bowl is deposited past its downwind rim.
//! That ramp-and-apron context is exactly what the truncated patch-edge
//! gouges lacked, and building it by construction is why the pack filter
//! could afford to reject them.
//!
//! Deflation pans are the negative counterpart on the LOW ground: shallow
//! scoops in the interdune floors where the wind strips loose sand down
//! toward the capillary fringe. They are what makes A6's lakes possible —
//! a pan whose floor reaches the water table is a lake.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};

use crate::draw::Descriptors;

#[derive(Clone, Copy, Debug)]
pub struct Blowout {
    pub center: Vec2,
    /// Across-wind radius, metres. The bowl is elongated ~1.3x along wind.
    pub radius_m: f64,
    pub depth_m: f64,
    /// Boundary irregularity: r(theta) = r * (1 + sum a_k sin(k theta + ph_k)).
    /// A perfect ellipse is a manufactured tell -- real blowout rims are
    /// scalloped where vegetation held and bitten where it failed.
    pub lobes: [(f64, f64); 3],
}

/// Golf bounds on the bowl. 15–60 m across and 2–8 m deep is a playable
/// waste-bunker scale; real blowouts run larger and would swallow a hole.
const R_MIN: f64 = 8.0;
const R_MAX: f64 = 30.0;
const D_MIN: f64 = 2.0;
const D_MAX: f64 = 8.0;
/// Along-wind elongation of the bowl.
const ELONG: f64 = 1.3;
/// Minimum separation between bowls, in units of the larger radius.
const SEP: f64 = 3.0;

/// Place blowouts on the belt tops and carve them, with aprons, into the 2 m
/// surface. `tnorm8` is the megaform position field at 8 m (percentile
/// height), the same axis the hummock and texture gates key on.
pub fn carve(rng: &mut DetRng, height: &mut Grid<f64>, tnorm8: &Grid<f64>,
             d: &Descriptors, avoid: Option<(&[Vec2], f64)>) -> Vec<Blowout> {
    let spec = height.spec;
    let area_km2 = (spec.nx as f64 * spec.cell_size) * (spec.ny as f64 * spec.cell_size) / 1e6;
    let want = (d.blowout_km2 * area_km2).round() as usize;
    let w = Vec2::new(math::cos(d.wind_rad), math::sin(d.wind_rad));

    // --- placement: rejection-sample onto high ground ----------------------
    let mut placed: Vec<Blowout> = Vec::new();
    let mut tries = 0usize;
    while placed.len() < want && tries < want * 60 {
        tries += 1;
        let p = Vec2::new(
            rng.range_f64(120.0, spec.nx as f64 * spec.cell_size - 120.0),
            rng.range_f64(120.0, spec.ny as f64 * spec.cell_size - 120.0),
        );
        // High ground only: a blowout is wind erosion of an exposed crest.
        if tnorm8.bilinear(p) < 0.62 {
            continue;
        }
        // ...and never in a river corridor: review caught seed 5's river
        // running straight over a bowl. Wind deflation does not persist on a
        // watered meadow floor.
        if let Some((line, r_excl)) = avoid {
            let mut near = false;
            for q in line.iter().step_by(4) {
                let dd = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
                if dd < r_excl {
                    near = true;
                    break;
                }
            }
            if near {
                continue;
            }
        }
        let r = rng.range_f64(R_MIN, R_MAX);
        // depth <= 0.30 r: review 2026-08-23, "a bit too sharp" against the
        // real tiles -- these are STABILIZED, grass-held bowls, not active
        // slip-faced pits. 0.45 allowed 24 deg mean walls; 0.30 caps at ~17.
        let depth = rng.range_f64(D_MIN, D_MAX).min(r * 0.30);
        if placed.iter().any(|b| {
            let dd = ((b.center.x - p.x).powi(2) + (b.center.y - p.y).powi(2)).sqrt();
            dd < SEP * b.radius_m.max(r)
        }) {
            continue;
        }
        let lobes = [
            (rng.range_f64(0.06, 0.16), rng.range_f64(0.0, std::f64::consts::TAU)),
            (rng.range_f64(0.05, 0.14), rng.range_f64(0.0, std::f64::consts::TAU)),
            (rng.range_f64(0.03, 0.10), rng.range_f64(0.0, std::f64::consts::TAU)),
        ];
        placed.push(Blowout { center: p, radius_m: r, depth_m: depth, lobes });
    }

    // --- carve: bowl + mass-conserved apron --------------------------------
    for b in &placed {
        let (rx, ry) = (b.radius_m * ELONG, b.radius_m);
        // Bowl volume of the smoothstep section, computed numerically once so
        // the apron can carry exactly what the bowl removed.
        let mut vol = 0.0f64;
        // apron: a Gaussian lobe downwind, sigma tied to the bowl size.
        let ac = Vec2::new(b.center.x + w.x * rx * 1.8, b.center.y + w.y * rx * 1.8);
        let sig = b.radius_m * 1.25;

        let reach = (rx.max(sig * 3.2)) + 4.0;
        let x0 = (((b.center.x - reach).min(ac.x - reach)) / spec.cell_size).floor().max(0.0) as u32;
        let x1 = (((b.center.x + reach).max(ac.x + reach)) / spec.cell_size).ceil()
            .min(spec.nx as f64 - 1.0) as u32;
        let y0 = (((b.center.y - reach).min(ac.y - reach)) / spec.cell_size).floor().max(0.0) as u32;
        let y1 = (((b.center.y + reach).max(ac.y + reach)) / spec.cell_size).ceil()
            .min(spec.ny as f64 - 1.0) as u32;

        // pass 1: cut the bowl, accumulate the removed volume
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = spec.world_of(x, y);
                let dp = Vec2::new(p.x - b.center.x, p.y - b.center.y);
                // rotate into wind frame
                let u = dp.x * w.x + dp.y * w.y;
                let v = -dp.x * w.y + dp.y * w.x;
                let mut rr = ((u / rx).powi(2) + (v / ry).powi(2)).sqrt();
                // scallop the rim: modulate the effective radius by angle
                if rr < 1.35 {
                    let th = math::atan2(v, u);
                    let m = 1.0
                        + b.lobes[0].0 * math::sin(2.0 * th + b.lobes[0].1)
                        + b.lobes[1].0 * math::sin(3.0 * th + b.lobes[1].1)
                        + b.lobes[2].0 * math::sin(5.0 * th + b.lobes[2].1);
                    rr /= m.max(0.55);
                }
                if rr < 1.0 {
                    // The wall transition spans 70% of the radius (was 45%):
                    // same review. A wide smoothstep keeps the flat-ish floor
                    // and the readable rim while dropping the wall grade.
                    let cut = b.depth_m * math::smoothstep(1.0, 0.30, rr);
                    let li = spec.index(x, y);
                    height.data[li] -= cut;
                    vol += cut * spec.cell_size * spec.cell_size;
                }
            }
        }
        // pass 2: deposit the same volume downwind
        let norm = vol / (std::f64::consts::TAU * sig * sig);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = spec.world_of(x, y);
                let d2 = (p.x - ac.x).powi(2) + (p.y - ac.y).powi(2);
                if d2 < (3.2 * sig).powi(2) {
                    let li = spec.index(x, y);
                    height.data[li] += norm * math::exp(-d2 / (2.0 * sig * sig));
                }
            }
        }
    }
    placed
}

/// Deflation pans: gentle scoops in the LOWEST interdune ground, deepening
/// where the megaform position is smallest. Returns the pan depth field so
/// A6 can find lakes. Applied at the macro (8 m) stage — pans are
/// 100–300 m features and belong under the texture, not over it.
pub fn pans(rng: &mut DetRng, height: &mut Grid<f64>, tnorm: &[f64], d: &Descriptors)
    -> Vec<f64>
{
    let spec = height.spec;
    let s = rng.next_u32();
    let mut pan = vec![0.0f64; spec.len()];
    // Pan depth cap: deflation stops at the capillary fringe, a few tens of
    // centimetres ABOVE the table. The first version used 0.6x table depth,
    // which meant a pan could NEVER reach the table by construction and the
    // measured lake fraction pinned at ~0 across every draw. 0.9x, and the
    // fringe is what keeps it physical.
    // ...and deflation chases the SEASONAL LOW table, so at the mean table a
    // deep pan core stands flooded. Without the +0.6 the pan floor sits above
    // the mean table by construction and the lake fraction pins at ~0 — the
    // same never-reaches bug shape, one constant over.
    let cap = (d.water_table_m * 0.9 + 0.6).min(4.0);
    if cap < 0.15 {
        return pan;
    }
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let li = spec.index(x, y);
            let low = 1.0 - math::smoothstep(0.10, 0.34, tnorm[li]);
            if low <= 0.0 {
                continue;
            }
            let p = spec.world_of(x, y);
            let n = 0.5 + 0.5 * course_world::noise::perlin2(p.x / 260.0, p.y / 260.0, s);
            let dd = cap * low * math::smoothstep(0.45, 0.85, n);
            pan[li] = dd;
            height.data[li] -= dd;
        }
    }
    pan
}

// ---------------------------------------------------------------------------
// C6 — Carolina bays
// ---------------------------------------------------------------------------

/// One Carolina bay: a shallow elliptical depression with a sand rim.
pub struct Bay {
    pub center: course_world::math::Vec2,
    /// Semi-major (NW–SE) and semi-minor axes, metres.
    pub a_m: f64,
    pub b_m: f64,
    pub depth_m: f64,
    /// Rim height, metres — carried on the SE side.
    pub rim_m: f64,
    /// Long-axis bearing, radians (NW–SE nominal).
    pub theta: f64,
}

/// Stamp 0–3 Carolina bays onto the finished surface.
///
/// The signature landform of the region and the reason the corpus tiles read
/// as Carolina rather than as generic dissected sand: shallow elliptical
/// basins, long axis NW–SE with striking consistency, a sand rim thrown up
/// on the SE (downwind) side, floors that pond when the table is high.
/// They sit on the INTERFLUVES — a bay that a creek has captured is no
/// longer a bay — so placement is gated on the valley coordinate.
pub fn bays(rng: &mut DetRng, height: &mut Grid<f64>, u_field: &Grid<f64>,
            chan_dist: &Grid<f64>, d: &Descriptors) -> Vec<Bay> {
    let spec = height.spec;
    let mut out: Vec<Bay> = Vec::new();
    let n = d.n_bays;
    if n == 0 {
        return out;
    }
    // NW–SE, with a little spread: measured orientations cluster tightly but
    // are not identical.
    for _ in 0..n * 22 {
        if out.len() >= n as usize {
            break;
        }
        // MEASURED 2026-08-25: the corpus holds only three intact bay-like
        // upland depressions, and they run 173-256 m long, 88-116 m wide,
        // 0.7-2.6 m deep, with soft rims — much smaller and gentler than the
        // 440-506 m basins this was drawing. Sized to what the ground shows.
        // Review 2026-08-25: a bay is a signature feature of this terrain
        // and is worth reading at a glance, so it is pitched between the
        // measured corpus (173-256 m, 0.7-2.6 m) and the earlier oversized
        // draw (440-506 m, 3.3-4.3 m) — deliberately above the measured
        // band, on the user's call, for legibility.
        let a_full = rng.range_f64(130.0, 255.0);       // 260-510 m before fit
        let elong = rng.range_f64(0.42, 0.60);          // measured ~2.2
        let c = course_world::math::Vec2::new(
            rng.range_f64(a_full, course_world::world::EXTENT_M - a_full),
            rng.range_f64(a_full, course_world::world::EXTENT_M - a_full));
        // interfluve only, clear of other bays — and clear of the DRAINAGE:
        // a bay a creek runs through is a bay the creek has captured, and it
        // stops being one (review 2026-08-25). The whole footprint plus its
        // rim has to sit between channels.
        let uu = u_field.bilinear(c);
        if uu < 0.55 {
            continue;
        }
        let theta = (-45.0f64).to_radians() + rng.range_f64(-0.22, 0.22);
        if out.iter().any(|o| o.center.distance(c) < (o.a_m + a_full) * 1.15) {
            continue;
        }
        // The FOOTPRINT — not a bounding circle — has to be free of drainage.
        // A channel that crossed the basin would have captured and drained
        // it, and it would no longer be a bay. Any crossing channel must cut
        // the rim, so sampling the rim (plus an inner ring, for a channel
        // that merely grazes) decides it.
        //
        // At the reviewed size a 470 m bay does not FIT between channels on
        // an interfluve this wide, and outright rejection emptied every
        // tile. So a candidate SHRINKS to its site instead: it is offered at
        // full size and stepped down until it clears, which keeps bays on
        // the ground and as large as the spot honestly allows.
        let (ct, st) = (math::cos(theta), math::sin(theta));
        // Prefer OPEN interfluve: shrink-to-fit alone pulled the median size
        // below where it started, because most candidates land in a cramped
        // spot and get cut down. Seeding the centre where there is room to
        // begin with keeps bays at the reviewed size instead.
        if chan_dist.bilinear(c) < 0.60 * a_full {
            continue;
        }
        let mut a_m = 0.0f64;
        for step in 0..4 {
            let cand = a_full * (1.0 - 0.08 * step as f64);
            let mut hits = false;
            'clear: for &rr in &[1.12f64, 0.62] {
                for k in 0..24 {
                    let ang = std::f64::consts::TAU * k as f64 / 24.0;
                    let (lx, ly) = (cand * rr * math::cos(ang),
                                    cand * elong * rr * math::sin(ang));
                    let q = course_world::math::Vec2::new(
                        c.x + lx * ct - ly * st, c.y + lx * st + ly * ct);
                    if chan_dist.bilinear(q) < 24.0 {
                        hits = true;
                        break 'clear;
                    }
                }
            }
            if !hits {
                a_m = cand;
                break;
            }
        }
        if a_m <= 0.0 {
            continue;
        }
        let b_m = a_m * elong;
        let depth = rng.range_f64(1.5, 3.4);            // deepened for read
        let rim = depth * rng.range_f64(0.22, 0.44);   // rims still soft
        let s_edge = rng.next_u32();
        let reach = a_m * 1.8;
        let (cx, cy) = ((c.x / spec.cell_size).round() as i64,
                        (c.y / spec.cell_size).round() as i64);
        let rp = (reach / spec.cell_size).ceil() as i64 + 1;

        // --- a bay is a CLOSED depression with a LEVEL floor ---------------
        // Subtracting a constant depth from sloping ground does not make a
        // basin, it makes a tilted dish that still drains: measured realised
        // depth ran from -4.08 m (floor ABOVE its surroundings) to +3.24 m,
        // which is why the bays would not read at any size. The floor is now
        // set to an ABSOLUTE level taken from the surrounding ring, so the
        // hole is closed by construction.
        //
        // That only makes sense on ground flat enough to hold it, which is
        // also where bays actually occur — flat interfluve, not hillside —
        // so a candidate on steep ground is rejected rather than bulldozed.
        // A LEVEL floor would be an earthwork on any sloping interfluve, and
        // demanding flat ground emptied nine tiles in ten. So the datum is a
        // plane least-squares-fitted to the surrounding ring with its slope
        // DAMPED to a third: the floor still falls gently the way the ground
        // does, but far less than the ground, so the hollow closes.
        let mut ring: Vec<(f64, f64, f64)> = Vec::new();
        for gy in (cy - rp).max(0)..=(cy + rp).min(spec.ny as i64 - 1) {
            for gx in (cx - rp).max(0)..=(cx + rp).min(spec.nx as i64 - 1) {
                let p = spec.world_of(gx as u32, gy as u32);
                let (dx, dy) = (p.x - c.x, p.y - c.y);
                let ax = dx * ct + dy * st;
                let ay = -dx * st + dy * ct;
                let r = ((ax / a_m).powi(2) + (ay / b_m).powi(2)).sqrt();
                if (1.05..1.45).contains(&r) {
                    ring.push((dx, dy, height.data[spec.index(gx as u32, gy as u32)]));
                }
            }
        }
        if ring.len() < 60 {
            continue;
        }
        let nring = ring.len() as f64;
        let (mut mx, mut my, mut mz) = (0.0, 0.0, 0.0);
        for &(x, y, z) in &ring {
            mx += x / nring;
            my += y / nring;
            mz += z / nring;
        }
        let (mut sxx, mut sxy, mut syy, mut sxz, mut syz) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for &(x, y, z) in &ring {
            let (u, v, w) = (x - mx, y - my, z - mz);
            sxx += u * u;
            sxy += u * v;
            syy += v * v;
            sxz += u * w;
            syz += v * w;
        }
        let det = sxx * syy - sxy * sxy;
        let (gx_s, gy_s) = if det.abs() < 1e-6 {
            (0.0, 0.0)
        } else {
            ((sxz * syy - syz * sxy) / det, (syz * sxx - sxz * sxy) / det)
        };
        let damp = 0.34;
        // the plane must not out-run the basin, or the "hollow" drains
        let tilt = (gx_s.hypot(gy_s)) * damp * a_m;
        if tilt > depth * 0.85 {
            continue;                       // too steep here for a bay to close
        }

        for gy in (cy - rp).max(0)..=(cy + rp).min(spec.ny as i64 - 1) {
            for gx in (cx - rp).max(0)..=(cx + rp).min(spec.nx as i64 - 1) {
                let p = spec.world_of(gx as u32, gy as u32);
                let (dx, dy) = (p.x - c.x, p.y - c.y);
                // into the bay's own frame
                let ax = dx * ct + dy * st;
                let ay = -dx * st + dy * ct;
                // a breathing edge: real bay outlines are smooth but not
                // machined ellipses
                let ang = math::atan2(ay, ax);
                let wob = 1.0 + 0.06 * course_world::noise::perlin1(ang * 2.4, s_edge);
                let r = ((ax / (a_m * wob)).powi(2) + (ay / (b_m * wob)).powi(2)).sqrt();
                let i = spec.index(gx as u32, gy as u32);
                // the basin: LEVEL floor, smooth wall, feathered to the rim
                if r < 1.0 {
                    let datum = mz + damp * (gx_s * (dx - mx) + gy_s * (dy - my));
                    // 0.86, not 1.0: a fully imposed floor is plastic, and
                    // keeping a seventh of the original relief leaves the
                    // basin its own fine texture without reopening a drain.
                    let t = 0.86 * math::smoothstep(1.0, 0.70, r);
                    height.data[i] = height.data[i] * (1.0 - t) + (datum - depth) * t;
                }
                // the rim: outside the rim line, strongest to the SE
                if r >= 0.88 && r < 1.55 {
                    let band = math::exp(-((r - 1.12) / 0.20).powi(2));
                    // SE side = positive along the bay's own long axis
                    let side = math::smoothstep(-0.35, 0.55, ax / a_m.max(1.0));
                    height.data[i] += rim * band * (0.28 + 0.95 * side);
                }
            }
        }
        out.push(Bay { center: c, a_m, b_m, depth_m: depth, rim_m: rim, theta });
    }
    out
}
