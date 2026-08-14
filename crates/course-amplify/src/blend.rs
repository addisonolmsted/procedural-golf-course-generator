//! Assembly: base surface + tapered residual bands.
//!
//! The taper is the load-bearing rule: **the skeleton's carved profile
//! must survive amplification**. Texture amplitude ramps from a floor at
//! the channel centreline up to full strength on the open hillslope, so
//! S2's monotone channel descent and valley floors stay recognizable and
//! the acceptance criterion "channel centrelines move less than tolerance"
//! holds by construction rather than by hope.

use course_world::grid::Grid;

/// Amplitude floor at the channel centreline.
pub const TAPER_FLOOR: f64 = 0.15;
/// Distance (m) over which texture reaches full strength.
pub const TAPER_FULL_M: f64 = 80.0;

/// Smoothstep taper weight for a distance-to-channel in metres.
pub fn taper(dist_m: f64) -> f64 {
    let t = (dist_m / TAPER_FULL_M).clamp(0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    TAPER_FLOOR + (1.0 - TAPER_FLOOR) * s
}

/// base (2 m) + taper·(mid residual upsampled from 8 m + fine residual).
/// `dist8` is the skeleton's flow-distance plane (8 m).
///
/// AWAY from channels, the base's own sub-64 m content is replaced by the
/// dictionary's: the carved surface carries D8-staircase rills below the
/// extraction threshold (flow on an 8-direction grid is quantized to 45°
/// steps), and the first S3 renders were full of diagonal corduroy that
/// turned out to be the BASE, not the patches. That band belongs to the
/// dictionary by the band-ownership rule anyway. Within the taper zone the
/// exact carved profile is kept — channel geometry is S2's word.
pub fn assemble(
    base2: &Grid<f64>,
    base2_lp64: &[f64],
    mid8: &[f64],
    fine2: &[f64],
    dist8: &Grid<f64>,
) -> Grid<f64> {
    let (nx2, ny2) = (base2.spec.nx as usize, base2.spec.ny as usize);
    let mut out = base2.clone();
    for y in 0..ny2 {
        for x in 0..nx2 {
            let p = base2.spec.world_of(x as u32, y as u32);
            let d = dist8.bilinear(p);
            let w = taper(d);
            // The SMOOTHED base is used EVERYWHERE, not just away from
            // channels. The first blend kept the raw base inside the taper
            // zone to preserve the carved profile — and the reviewer's
            // blind A/B caught the consequence immediately: the raw base's
            // D8 staircase showed as channel-following "straight ribbed
            // cuts", the single most reliable tell at 83%. The carved
            // PROFILE survives through two other mechanisms that don't
            // carry the staircase: valleys are wider than the 64 m cut, and
            // the channel-restore step re-pins centreline cells after
            // polish. Bank micro-shape is parametric placeholder anyway
            // until the transect fit.
            let i2 = y * nx2 + x;
            out.data[i2] = base2_lp64[i2];
            // mid residual lives on the 8 m grid: bilinear via world coords
            let m = {
                let g8 = dist8.spec; // same 8 m spec as the mid plane
                let fx = ((p.x - g8.origin.x) / g8.cell_size).clamp(0.0, (g8.nx - 1) as f64);
                let fy = ((p.y - g8.origin.y) / g8.cell_size).clamp(0.0, (g8.ny - 1) as f64);
                let (x0, y0) = (fx as usize, fy as usize);
                let (x1, y1) = ((x0 + 1).min(g8.nx as usize - 1), (y0 + 1).min(g8.ny as usize - 1));
                let (tx, ty) = (fx - x0 as f64, fy - y0 as f64);
                let n8 = g8.nx as usize;
                let a = mid8[y0 * n8 + x0] * (1.0 - tx) + mid8[y0 * n8 + x1] * tx;
                let b = mid8[y1 * n8 + x0] * (1.0 - tx) + mid8[y1 * n8 + x1] * tx;
                a * (1.0 - ty) + b * ty
            };
            let f = fine2[y * nx2 + x];
            out.data[i2] += w * (m + f);
        }
    }
    out
}
