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
             d: &Descriptors) -> Vec<Blowout> {
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
