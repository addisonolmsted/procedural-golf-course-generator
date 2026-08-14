//! Consistency polish: remove the pits the TEXTURE dug, keep the pits the
//! TERRAIN means.
//!
//! Quilted residual can locally out-dig the drainage (a deep patch across
//! a shallow swale leaves a puddle basin the skeleton never intended).
//! One masked priority-flood raises exactly those cells to their spill
//! level. Intended depressions survive through the mask:
//! - basin embryos (S2's recorded kettles/blowouts), and
//! - any depression the BASE surface already had (a deranged biome's
//!   natural basins), detected on the cheap 8 m base fill and upsampled.
//!
//! This never restructures: it only raises texture-made pits, bounded by
//! the texture's own amplitude.

use course_world::flow;
use course_world::grid::Grid;
use course_world::math::Vec2;

use course_skeleton::kernel::BasinEmbryo;

pub fn polish(
    amplified2: &mut Grid<f64>,
    base8: &Grid<f64>,
    embryos: &[BasinEmbryo],
) {
    // Which ground is ALLOWED to stay a pit, at 8 m then upsampled.
    let f8 = flow::fill_depressions(base8);
    let (nx8, ny8) = (base8.spec.nx as usize, base8.spec.ny as usize);
    let mut keep8 = vec![false; nx8 * ny8];
    for i in 0..nx8 * ny8 {
        if f8.data[i] > base8.data[i] + 0.02 {
            keep8[i] = true;
        }
    }
    let (nx2, ny2) = (amplified2.spec.nx as usize, amplified2.spec.ny as usize);
    let mut keep2 = vec![false; nx2 * ny2];
    let cell2 = amplified2.spec.cell_size;
    let cell8 = base8.spec.cell_size;
    let scale = (cell8 / cell2).round() as usize;
    for y in 0..ny2 {
        for x in 0..nx2 {
            let (x8, y8) = ((x / scale).min(nx8 - 1), (y / scale).min(ny8 - 1));
            keep2[y * nx2 + x] = keep8[y8 * nx8 + x8];
        }
    }
    for e in embryos {
        let r = e.radius_m + 2.0 * cell2;
        let (cx, cy) = (e.center.x, e.center.y);
        let x0 = (((cx - r) / cell2).floor().max(0.0)) as usize;
        let x1 = (((cx + r) / cell2).ceil() as usize).min(nx2 - 1);
        let y0 = (((cy - r) / cell2).floor().max(0.0)) as usize;
        let y1 = (((cy + r) / cell2).ceil() as usize).min(ny2 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = Vec2::new(x as f64 * cell2, y as f64 * cell2);
                if (p.x - cx).powi(2) + (p.y - cy).powi(2) <= r * r {
                    keep2[y * nx2 + x] = true;
                }
            }
        }
    }
    // Flood at 4 m: the priority-flood is the cost here (312 ms at 2 m),
    // and a pit that matters to drainage is far wider than one 2 m cell —
    // PIT_CAP_M itself is 0.35 m. Downsample (min-pool, so no pit is
    // missed), flood, then bilinearly read the spill level back at 2 m.
    let n4 = nx2 / 2;
    let spec4 = course_world::grid::GridSpec::new(
        amplified2.spec.origin,
        amplified2.spec.cell_size * 2.0,
        n4 as u32,
        n4 as u32,
    );
    let mut z4 = Grid { spec: spec4, data: vec![f64::INFINITY; n4 * n4] };
    let mut keep4 = vec![false; n4 * n4];
    for y in 0..ny2 {
        for x in 0..nx2 {
            let (x4, y4) = ((x / 2).min(n4 - 1), (y / 2).min(n4 - 1));
            let i4 = y4 * n4 + x4;
            let v = amplified2.data[y * nx2 + x];
            if v < z4.data[i4] {
                z4.data[i4] = v;
            }
            keep4[i4] |= keep2[y * nx2 + x];
        }
    }
    let filled4 = flow::fill_depressions_masked(&z4, &keep4);
    let mut filled = amplified2.clone();
    for y in 0..ny2 {
        for x in 0..nx2 {
            let p = amplified2.spec.world_of(x as u32, y as u32);
            filled.data[y * nx2 + x] = filled4.bilinear(p).max(amplified2.data[y * nx2 + x]);
        }
    }
    // Cap, don't flatten: filling to spill turns every texture pit into a
    // dead-flat pool, and the first-light render was full of smooth grey
    // blobs. Instead, raise each unintended pit only enough that at most
    // PIT_CAP_M of depth remains — small hollows stay (S4's water logic
    // owns whether they pond), drainage-blocking holes are gone.
    //
    // The raise is applied SMOOTHED: the keep mask lives on 8 m blocks
    // and the spill on a 4 m grid, so a hard per-cell raise builds a
    // cliff along the blocky keep boundary — the reviewer's basins with
    // axis-aligned "staircase" edges. Diffusing the raise over ~20 m
    // turns those cliffs into shore ramps; pit interiors still rise to
    // the cap (their raise field is locally constant, and a box filter
    // preserves constants).
    let n = amplified2.data.len();
    let mut raise: Vec<f64> = (0..n)
        .map(|i| (filled.data[i] - PIT_CAP_M - amplified2.data[i]).max(0.0))
        .collect();
    for _ in 0..2 {
        raise = crate::synth::box_filter(&raise, nx2, 11);
    }
    for i in 0..n {
        amplified2.data[i] += raise[i];
    }
}

/// Residual pit depth allowed to remain after polish.
pub const PIT_CAP_M: f64 = 0.35;
