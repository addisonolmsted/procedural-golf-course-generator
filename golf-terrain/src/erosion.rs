//! Deterministic geomorphic sculpting of the canonical macro grid.
//!
//! One `erode` call applies a finite amount of fluvial incision (implicit
//! stream power, Braun & Willett 2013), sediment deposition (explicit
//! erode-then-route, headroom-limited), a talus guard, and hillslope
//! diffusion — structured as routing *epochs*, because depression routing is
//! the cost center, not the field updates.
//!
//! Everything here is pure grid arithmetic: `+ − × ÷`, comparisons, and IEEE
//! `sqrt`. No RNG, no transcendentals, fixed traversal orders and tie-breaks
//! — the result is bit-identical for identical inputs on any platform.
//!
//! The initial condition is the noise terrain; there is no uplift and no
//! steady state. Erosion dissects the noise, it does not erase it.

use golf_core::Grid;
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::collections::{BinaryHeap, VecDeque};

/// Canonical macro grid edge (12.5 m cells over the 2 km world).
pub const MACRO_N: u32 = 160;
/// Routing epochs per `erode` call.
pub const EPOCHS: usize = 8;
/// Diffusion substeps per epoch.
pub const DIFF_SUBSTEPS: usize = 2;
/// Talus guard threshold (rise/run ≈ 35°) — a guard rail, not a style knob.
pub const TALUS_SLOPE: f64 = 0.7;
/// Target slope of freshly deposited (alluvial) surfaces: sediment aggrades a
/// channel bed up toward `z_receiver + S_DEP·d` — this is what builds flat
/// valley floors, the atlas's measured 0–2% band. (A donor-headroom rule was
/// tried first and strangles aggradation: in a channel the upstream donor is
/// only the channel step higher, so floors could never build.)
const S_DEP: f64 = 0.006;
/// Per-epoch cap on the implicit factor F = k·√A/d. Uncapped, F ≫ 1 at
/// trunk-channel scale and the update saturates to the receiver height —
/// erasing any erodibility contrast and over-deepening instantly. Capping
/// keeps incision graded so knickpoints persist and hard strata read as
/// treads.
const F_MAX: f64 = 1.5;
/// Per-epoch incision depth cap as a fraction of initial relief (guards
/// against single-cell canyons).
const INCISION_CAP_FRACTION: f64 = 0.35;
/// ε used by the priority flood to tilt filled flats toward their spill.
const FLOOD_EPS: f64 = 1e-6;
/// Bump when erosion semantics change — folded into search fingerprints.
/// v2: the p(θ) re-fit draws `hardness_contrast > 0` (v8 fixed it at 0), so the
/// differential-erodibility path is now live in shipped courses.
pub const EROSION_VERSION: u32 = 2;

/// The three seed-sampled erosion axes (fixed structural constants like
/// EPOCHS and TALUS_SLOPE are deliberately not parameters).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ErosionParams {
    /// Total dimensionless fluvial intensity Θ = Σ_epochs K·dt at headwater
    /// scale. ~0.15 lowland, ~0.35 rolling, ~0.8 mountain.
    pub intensity: f64,
    /// Deposition strength g in Δz = g·Q_in/A (dimensionless).
    pub deposition: f64,
    /// Total hillslope diffusion α = κ·T/h², split across substeps. Must stay
    /// ≤ 3.0 so each substep is within the explicit stability limit.
    pub diffusion: f64,
    /// Differential-erodibility contrast c: local fluvial K is scaled by
    /// `1 − c·hardness` (hardness ∈ [−1, 1], +1 = resistant rock).
    pub hardness_contrast: f64,
}

impl Default for ErosionParams {
    fn default() -> Self {
        ErosionParams {
            intensity: 0.35,
            deposition: 1.0,
            diffusion: 1.2,
            hardness_contrast: 0.0,
        }
    }
}

/// Volume ledger in m³. `eroded == deposited + exported` exactly (unit test).
#[derive(Clone, Copy, Debug, Default)]
pub struct MassBalance {
    pub eroded: f64,
    pub deposited: f64,
    pub exported: f64,
}

/// The final routing pass, exported verbatim: the canonical D8 flow tree of
/// the finished surface. The water pass and (later) hole routing consume this
/// instead of re-deriving it.
#[derive(Clone, Debug)]
pub struct RoutingSnapshot {
    /// D8 receiver per cell (self = base level at the boundary).
    pub rcv: Vec<u32>,
    /// Flow-path length to the receiver, meters.
    pub dist: Vec<f64>,
    /// Braun–Willett topological order (receivers before donors).
    pub order: Vec<u32>,
    /// Depression-filled surface used for routing.
    pub zfill: Vec<f64>,
}

/// Byproducts of the final routing pass on the finished surface — hooks for
/// the water system and the viewer's Flow view.
#[derive(Clone, Debug)]
pub struct ErosionOutputs {
    /// Drainage area per cell, m² (final surface, D8 routing).
    pub flow_area: Grid<f64>,
    /// Depression fill depth `zfill − z`, meters (≥ 0; where ponds would sit).
    pub lake_depth: Grid<f64>,
    /// Cumulative net deposition, meters.
    pub sediment: Grid<f64>,
    pub mass: MassBalance,
    pub routing: RoutingSnapshot,
}

/// Fixed D8 neighbor scan order (documented tie-break: first wins on ties).
const NBR: [(i32, i32); 8] = [
    (0, -1),
    (-1, 0),
    (1, 0),
    (0, 1),
    (-1, -1),
    (1, -1),
    (-1, 1),
    (1, 1),
];
const SQRT_2: f64 = std::f64::consts::SQRT_2;

/// Monotone total order on finite f64 (for deterministic heap keys).
#[inline]
fn fkey(v: f64) -> u64 {
    let b = v.to_bits();
    if b >> 63 == 1 {
        !b
    } else {
        b | (1 << 63)
    }
}

/// Erode `heights` in place. `hardness`, if given, must be the same length as
/// the grid with values in [−1, 1] (+1 = resistant); it modulates fluvial K
/// by `1 − hardness_contrast · h`.
pub fn erode(heights: &mut Grid<f64>, hardness: Option<&[f64]>, p: &ErosionParams) -> ErosionOutputs {
    erode_epochs(heights, hardness, p, EPOCHS)
}

/// `erode` with an explicit epoch count — benching/experiments only; the
/// shipped pipeline always uses [`EPOCHS`].
pub fn erode_epochs(
    heights: &mut Grid<f64>,
    hardness: Option<&[f64]>,
    p: &ErosionParams,
    epochs: usize,
) -> ErosionOutputs {
    let spec = heights.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    assert!(nx >= 4 && ny >= 4, "erosion grid too small: {nx}x{ny}");
    if let Some(h) = hardness {
        assert_eq!(h.len(), nx * ny, "hardness length mismatch");
    }
    let cell = spec.cell_size;
    let a0 = cell * cell;
    let n = nx * ny;

    // Initial relief bounds the per-epoch incision depth.
    let (z0_min, z0_max) = min_max(&heights.data);
    let incision_cap = INCISION_CAP_FRACTION * (z0_max - z0_min).max(1e-9) / epochs as f64;

    let mut ws = Workspace::new(nx, ny);
    let mut mass = MassBalance::default();
    let mut sediment = vec![0.0f64; n];

    let k_f = p.intensity / epochs as f64;
    let alpha_step = (p.diffusion / (epochs * DIFF_SUBSTEPS) as f64).min(0.24);

    let mut flooded = false;
    for epoch in 0..epochs {
        // ---- routing --------------------------------------------------------
        // Recompute the flow tree every OTHER epoch: the surface moves little
        // per epoch, stale receivers stay a valid tree (incision guards on
        // true heights), and the next routed epoch re-adapts the channels.
        // Halves the dominant cost (flood + receivers + ordering).
        if epoch % 2 == 0 {
            flooded = route(&heights.data, nx, ny, cell, &mut ws);
        }

        // ---- fluvial incision (implicit, forward topological order) -------
        let z = &mut heights.data;
        for oi in 0..n {
            let i = ws.order[oi] as usize;
            let r = ws.rcv[i] as usize;
            ws.eroded[i] = 0.0;
            if r == i {
                continue; // base level (boundary)
            }
            // Lakes don't incise: only cut if truly above the (already
            // updated) receiver on the real surface.
            if z[i] <= z[r] {
                continue;
            }
            let hard_mult = match hardness {
                Some(h) => 1.0 - p.hardness_contrast * h[i],
                None => 1.0,
            };
            let f = (k_f * hard_mult.max(0.0) * ws.area[i].sqrt() / ws.dist[i]).min(F_MAX);
            let mut znew = (z[i] + f * z[r]) / (1.0 + f);
            // Guard against single-cell canyons: cap the per-epoch cut.
            if z[i] - znew > incision_cap {
                znew = z[i] - incision_cap;
            }
            ws.eroded[i] = (z[i] - znew) * a0;
            mass.eroded += ws.eroded[i];
            z[i] = znew;
        }

        // ---- graded alluvial target (forward: receivers first) -------------
        // T[i] = the height the bed would aggrade to with unlimited sediment:
        // the receiver's target plus the alluvial grade, propagated the whole
        // profile in one sweep (and through lakes via their outlet chain).
        // Capped at the initial ceiling so fills never exceed original relief.
        let _ = flooded;
        for oi in 0..n {
            let i = ws.order[oi] as usize;
            let r = ws.rcv[i] as usize;
            ws.target[i] = if r == i {
                z[i]
            } else {
                z[i].max((ws.target[r] + S_DEP * ws.dist[i]).min(z0_max))
            };
        }

        // ---- sediment routing (reverse order: donors before receivers) ----
        for q in ws.qin.iter_mut() {
            *q = 0.0;
        }
        for oi in (0..n).rev() {
            let i = ws.order[oi] as usize;
            let r = ws.rcv[i] as usize;
            let qin = ws.qin[i];
            if r == i {
                // Boundary: everything arriving here leaves the world.
                mass.exported += qin;
                continue;
            }
            let mut dz = 0.0;
            if qin > 0.0 {
                // Fill toward the graded target, capped by the flux that
                // actually arrived — g > 1 must not deposit volume that
                // never existed.
                let cap_h = (ws.target[i] - z[i]).max(0.0);
                dz = (p.deposition * qin / ws.area[i]).min(cap_h).min(qin / a0);
                if dz > 0.0 {
                    z[i] += dz;
                    sediment[i] += dz;
                    mass.deposited += dz * a0;
                }
            }
            ws.qin[r] += qin + ws.eroded[i] - dz * a0;
        }

        // ---- talus guard (gather deltas, then apply) -----------------------
        talus_pass(z, nx, ny, cell, &mut ws.delta);

        // ---- hillslope diffusion (Jacobi, double-buffered) -----------------
        for _ in 0..DIFF_SUBSTEPS {
            diffuse(z, &mut ws.scratch, nx, ny, alpha_step);
            std::mem::swap(z, &mut ws.scratch);
        }
    }

    // ---- final routing pass on the finished surface for the outputs -------
    flood_into(&heights.data, nx, ny, &mut ws.zfill, &mut ws.visited, &mut ws.heap, &mut ws.fifo);
    receivers(&heights.data, &ws.zfill, nx, ny, cell, &mut ws.rcv, &mut ws.dist);
    topo_order_inner(nx, ny, &mut ws);
    accumulate_area(&ws.order, &ws.rcv, a0, &mut ws.area);

    let lake: Vec<f64> = ws
        .zfill
        .iter()
        .zip(heights.data.iter())
        .map(|(zf, z)| (zf - z).max(0.0))
        .collect();
    ErosionOutputs {
        flow_area: Grid::from_data(spec, ws.area.clone()),
        lake_depth: Grid::from_data(spec, lake),
        sediment: Grid::from_data(spec, sediment),
        mass,
        routing: RoutingSnapshot {
            rcv: ws.rcv.clone(),
            dist: ws.dist.clone(),
            order: ws.order.clone(),
            zfill: ws.zfill.clone(),
        },
    }
}

// ---------------------------------------------------------------------------
// Routing machinery
// ---------------------------------------------------------------------------

struct Workspace {
    zfill: Vec<f64>,
    visited: Vec<bool>,
    heap: BinaryHeap<Reverse<(u64, u32)>>,
    fifo: VecDeque<u32>,
    rcv: Vec<u32>,
    dist: Vec<f64>,
    ndon: Vec<u32>,
    don_off: Vec<usize>,
    donors: Vec<u32>,
    order: Vec<u32>,
    area: Vec<f64>,
    eroded: Vec<f64>,
    qin: Vec<f64>,
    target: Vec<f64>,
    delta: Vec<f64>,
    scratch: Vec<f64>,
}

impl Workspace {
    fn new(nx: usize, ny: usize) -> Self {
        let n = nx * ny;
        Workspace {
            zfill: vec![0.0; n],
            visited: vec![false; n],
            heap: BinaryHeap::with_capacity(n / 4),
            fifo: VecDeque::with_capacity(n / 4),
            rcv: vec![0; n],
            dist: vec![0.0; n],
            ndon: vec![0; n],
            don_off: vec![0; n + 1],
            donors: vec![0; n],
            order: Vec::with_capacity(n),
            area: vec![0.0; n],
            eroded: vec![0.0; n],
            qin: vec![0.0; n],
            target: vec![0.0; n],
            delta: vec![0.0; n],
            scratch: vec![0.0; n],
        }
    }
}

/// Compute receivers/order/area for the current surface. Skips the priority
/// flood when the surface has no interior pits or flats (pit detection is
/// O(N), the flood is not). Returns whether the flood ran (i.e., `ws.zfill`
/// is valid lake data for this epoch).
fn route(z: &[f64], nx: usize, ny: usize, cell: f64, ws: &mut Workspace) -> bool {
    let a0 = cell * cell;
    let flooded = has_pits(z, nx, ny);
    if flooded {
        flood_into(z, nx, ny, &mut ws.zfill, &mut ws.visited, &mut ws.heap, &mut ws.fifo);
        // Receivers on the filled surface (epsilon tilt guarantees descent).
        receivers(z, &ws.zfill, nx, ny, cell, &mut ws.rcv, &mut ws.dist);
    } else {
        receivers(z, z, nx, ny, cell, &mut ws.rcv, &mut ws.dist);
    }
    topo_order_inner(nx, ny, ws);
    accumulate_area(&ws.order, &ws.rcv, a0, &mut ws.area);
    flooded
}

#[inline]
fn is_boundary(x: usize, y: usize, nx: usize, ny: usize) -> bool {
    x == 0 || y == 0 || x == nx - 1 || y == ny - 1
}

/// Does any interior cell lack a strictly lower D8 neighbor?
fn has_pits(z: &[f64], nx: usize, ny: usize) -> bool {
    for y in 1..ny - 1 {
        for x in 1..nx - 1 {
            let i = y * nx + x;
            let zi = z[i];
            let mut drains = false;
            for (dx, dy) in NBR {
                let j = (y as i32 + dy) as usize * nx + (x as i32 + dx) as usize;
                if z[j] < zi {
                    drains = true;
                    break;
                }
            }
            if !drains {
                return true;
            }
        }
    }
    false
}

/// Priority flood with ε (Barnes 2014, FIFO-optimized): fills every interior
/// depression so all cells drain to the boundary. Deterministic: heap ordered
/// by (height key, cell index); FIFO preserves insertion order.
fn flood_into(
    z: &[f64],
    nx: usize,
    ny: usize,
    zfill: &mut [f64],
    visited: &mut [bool],
    heap: &mut BinaryHeap<Reverse<(u64, u32)>>,
    fifo: &mut VecDeque<u32>,
) {
    for v in visited.iter_mut() {
        *v = false;
    }
    heap.clear();
    fifo.clear();
    for y in 0..ny {
        for x in 0..nx {
            if is_boundary(x, y, nx, ny) {
                let i = y * nx + x;
                zfill[i] = z[i];
                visited[i] = true;
                heap.push(Reverse((fkey(z[i]), i as u32)));
            }
        }
    }
    loop {
        let i = if let Some(&f) = fifo.front() {
            fifo.pop_front();
            f as usize
        } else if let Some(Reverse((_, h))) = heap.pop() {
            h as usize
        } else {
            break;
        };
        let (x, y) = (i % nx, i / nx);
        for (dx, dy) in NBR {
            let cx = x as i32 + dx;
            let cy = y as i32 + dy;
            if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                continue;
            }
            let c = cy as usize * nx + cx as usize;
            if visited[c] {
                continue;
            }
            visited[c] = true;
            if z[c] <= zfill[i] {
                // In the depression / flat: raise onto the spill surface and
                // take the O(1) queue.
                zfill[c] = zfill[i] + FLOOD_EPS;
                fifo.push_back(c as u32);
            } else {
                zfill[c] = z[c];
                heap.push(Reverse((fkey(z[c]), c as u32)));
            }
        }
    }
}

/// D8 steepest-descent receivers computed on `zref` (the filled surface when
/// flooding ran, the raw surface otherwise). `dist` is the flow-path length.
fn receivers(
    _z: &[f64],
    zref: &[f64],
    nx: usize,
    ny: usize,
    cell: f64,
    rcv: &mut [u32],
    dist: &mut [f64],
) {
    let diag = cell * SQRT_2;
    for y in 0..ny {
        for x in 0..nx {
            let i = y * nx + x;
            if is_boundary(x, y, nx, ny) {
                rcv[i] = i as u32;
                dist[i] = cell;
                continue;
            }
            let zi = zref[i];
            let mut best = i;
            let mut best_grad = 0.0;
            let mut best_d = cell;
            for (k, (dx, dy)) in NBR.iter().enumerate() {
                let j = (y as i32 + dy) as usize * nx + (x as i32 + dx) as usize;
                let d = if k < 4 { cell } else { diag };
                let grad = (zi - zref[j]) / d;
                if grad > best_grad {
                    best_grad = grad;
                    best = j;
                    best_d = d;
                }
            }
            rcv[i] = best as u32;
            dist[i] = best_d;
        }
    }
}

/// Build the Braun–Willett topological order: every cell appears after its
/// receiver. Donors are CSR-packed by counting sort (ascending cell index),
/// base cells seed an explicit DFS in ascending index order.
fn topo_order_inner(nx: usize, ny: usize, ws: &mut Workspace) {
    let n = nx * ny;
    for c in ws.ndon.iter_mut() {
        *c = 0;
    }
    for i in 0..n {
        let r = ws.rcv[i] as usize;
        if r != i {
            ws.ndon[r] += 1;
        }
    }
    ws.don_off[0] = 0;
    for i in 0..n {
        ws.don_off[i + 1] = ws.don_off[i] + ws.ndon[i] as usize;
    }
    let mut fill = ws.don_off.clone();
    for i in 0..n {
        let r = ws.rcv[i] as usize;
        if r != i {
            ws.donors[fill[r]] = i as u32;
            fill[r] += 1;
        }
    }
    ws.order.clear();
    // DFS from each base cell (rcv == self), ascending index; donors pushed
    // in ascending order (pop order is then descending — fixed either way).
    let mut stack: Vec<u32> = Vec::with_capacity(64);
    for i in 0..n {
        if ws.rcv[i] as usize != i {
            continue;
        }
        stack.push(i as u32);
        while let Some(c) = stack.pop() {
            ws.order.push(c);
            let ci = c as usize;
            for k in ws.don_off[ci]..ws.don_off[ci + 1] {
                stack.push(ws.donors[k]);
            }
        }
    }
    debug_assert_eq!(ws.order.len(), n, "topological order must cover the grid");
}

/// Drainage area: init a0 everywhere, fold donors into receivers by walking
/// the order in reverse.
fn accumulate_area(order: &[u32], rcv: &[u32], a0: f64, area: &mut [f64]) {
    for a in area.iter_mut() {
        *a = a0;
    }
    for oi in (0..order.len()).rev() {
        let i = order[oi] as usize;
        let r = rcv[i] as usize;
        if r != i {
            area[r] += area[i];
        }
    }
}

/// Talus guard: transfer material along the steepest true-surface descent
/// wherever it exceeds TALUS_SLOPE. Gather into a delta buffer, then apply
/// (order-independent).
fn talus_pass(z: &mut [f64], nx: usize, ny: usize, cell: f64, delta: &mut [f64]) {
    let diag = cell * SQRT_2;
    for d in delta.iter_mut() {
        *d = 0.0;
    }
    for y in 0..ny {
        for x in 0..nx {
            let i = y * nx + x;
            let zi = z[i];
            let mut best = i;
            let mut best_excess = 0.0;
            for (k, (dx, dy)) in NBR.iter().enumerate() {
                let cx = x as i32 + dx;
                let cy = y as i32 + dy;
                if cx < 0 || cy < 0 || cx >= nx as i32 || cy >= ny as i32 {
                    continue;
                }
                let j = cy as usize * nx + cx as usize;
                let d = if k < 4 { cell } else { diag };
                let excess = zi - z[j] - TALUS_SLOPE * d;
                if excess > best_excess {
                    best_excess = excess;
                    best = j;
                }
            }
            if best != i {
                let m = 0.125 * best_excess;
                delta[i] -= m;
                delta[best] += m;
            }
        }
    }
    for i in 0..z.len() {
        z[i] += delta[i];
    }
}

/// One explicit diffusion substep into `out` (Jacobi, Neumann boundaries via
/// existing-neighbor sums). Mass-conserving.
fn diffuse(z: &[f64], out: &mut [f64], nx: usize, ny: usize, alpha: f64) {
    for y in 0..ny {
        for x in 0..nx {
            let i = y * nx + x;
            let zi = z[i];
            let mut acc = 0.0;
            if x > 0 {
                acc += z[i - 1] - zi;
            }
            if x < nx - 1 {
                acc += z[i + 1] - zi;
            }
            if y > 0 {
                acc += z[i - nx] - zi;
            }
            if y < ny - 1 {
                acc += z[i + nx] - zi;
            }
            out[i] = zi + alpha * acc;
        }
    }
}

fn min_max(vals: &[f64]) -> (f64, f64) {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for &v in vals {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    (lo, hi)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{generate, world_spec, TerrainParams};
    use golf_core::{GridSpec, Vec2};

    fn test_grid(seed: u64, redistribution: f64) -> Grid<f64> {
        let params = TerrainParams {
            amplitude: 60.0,
            base_period: 900.0,
            redistribution,
            ..TerrainParams::default()
        };
        // 80² keeps tests fast; erode() is size-agnostic.
        let spec = GridSpec::new(Vec2::new(12.5, 12.5), 25.0, 80, 80);
        generate(&spec, seed, &params).heights
    }

    #[test]
    fn erosion_is_deterministic() {
        let base = test_grid(7, 1.0);
        let p = ErosionParams::default();
        let mut a = base.clone();
        let mut b = base.clone();
        let oa = erode(&mut a, None, &p);
        let ob = erode(&mut b, None, &p);
        assert_eq!(a.data, b.data);
        assert_eq!(oa.flow_area.data, ob.flow_area.data);
        assert_eq!(oa.lake_depth.data, ob.lake_depth.data);
    }

    #[test]
    fn every_cell_drains_to_boundary() {
        for (seed, redist) in [(1u64, 1.0), (3, 8.0), (9, 0.125)] {
            let mut g = test_grid(seed, redist);
            let outs = erode(&mut g, None, &ErosionParams::default());
            // Re-derive routing on the final surface exactly as erode() did.
            let (nx, ny) = (g.spec.nx as usize, g.spec.ny as usize);
            let mut ws = Workspace::new(nx, ny);
            flood_into(&g.data, nx, ny, &mut ws.zfill, &mut ws.visited, &mut ws.heap, &mut ws.fifo);
            receivers(&g.data, &ws.zfill, nx, ny, g.spec.cell_size, &mut ws.rcv, &mut ws.dist);
            for start in 0..nx * ny {
                let mut i = start;
                let mut steps = 0;
                while ws.rcv[i] as usize != i {
                    i = ws.rcv[i] as usize;
                    steps += 1;
                    assert!(steps <= nx * ny, "cycle from cell {start}");
                }
                let (x, y) = (i % nx, i / nx);
                assert!(is_boundary(x, y, nx, ny), "cell {start} drains to interior {i}");
            }
            assert!(outs.flow_area.data.iter().all(|a| a.is_finite()));
        }
    }

    #[test]
    fn mass_ledger_balances() {
        let mut g = test_grid(5, 1.0);
        let sum_before: f64 = g.data.iter().sum();
        let a0 = g.spec.cell_size * g.spec.cell_size;
        let outs = erode(&mut g, None, &ErosionParams { intensity: 0.8, ..Default::default() });
        let m = outs.mass;
        assert!(m.eroded > 0.0, "erosion should move material");
        let ledger = (m.eroded - m.deposited - m.exported).abs();
        assert!(ledger <= 1e-9 * m.eroded.max(1.0), "ledger off by {ledger}");
        // Height-sum drift equals the exported volume (talus + diffusion
        // conserve exactly; incision − deposition = export).
        let sum_after: f64 = g.data.iter().sum();
        let drift = (sum_before - sum_after) * a0 - m.exported;
        assert!(
            drift.abs() <= 1e-6 * m.exported.max(1.0),
            "sum drift {drift} vs exported {}",
            m.exported
        );
    }

    #[test]
    fn heights_stay_bounded_and_finite() {
        for redist in [1.0, 6.0] {
            let mut g = test_grid(11, redist);
            let (lo, hi) = min_max(&g.data);
            erode(&mut g, None, &ErosionParams { intensity: 1.5, ..Default::default() });
            assert!(g.data.iter().all(|z| z.is_finite()));
            let (lo2, hi2) = min_max(&g.data);
            assert!(lo2 >= lo - 1e-9, "min dropped: {lo2} < {lo}");
            assert!(hi2 <= hi + 1e-9, "max rose: {hi2} > {hi}");
        }
    }

    #[test]
    fn flat_input_is_a_no_op_modulo_epsilon() {
        let spec = GridSpec::new(Vec2::ZERO, 25.0, 40, 40);
        let mut g = Grid::filled(spec, 5.0f64);
        let outs = erode(&mut g, None, &ErosionParams::default());
        assert!(g.data.iter().all(|z| (z - 5.0).abs() < 1e-3));
        assert!(outs.mass.eroded < 1e-3);
    }

    #[test]
    fn params_have_monotone_effects() {
        let base = test_grid(13, 1.0);
        let run = |p: ErosionParams| {
            let mut g = base.clone();
            let o = erode(&mut g, None, &p);
            (g, o)
        };
        // More intensity → more export.
        let (_, weak) = run(ErosionParams { intensity: 0.15, ..Default::default() });
        let (_, strong) = run(ErosionParams { intensity: 1.2, ..Default::default() });
        assert!(strong.mass.exported > weak.mass.exported);
        // More deposition strength → more sediment captured before the edge.
        // (Flat-band area is an emergent tuning target measured by the sweep,
        // not a per-seed invariant — the headroom cap can trade flats for
        // steps on any one field.)
        let (_, dep_lo) = run(ErosionParams { deposition: 0.4, intensity: 0.8, ..Default::default() });
        let (_, dep_hi) = run(ErosionParams { deposition: 2.5, intensity: 0.8, ..Default::default() });
        assert!(dep_hi.mass.deposited > dep_lo.mass.deposited);
        // More diffusion → less steep (>25%) area.
        let steep_frac = |g: &Grid<f64>| {
            let s = crate::derive_slope(g);
            s.data.iter().filter(|&&v| v > 0.25).count() as f64 / s.data.len() as f64
        };
        let (g_soft, _) = run(ErosionParams { diffusion: 3.0, ..Default::default() });
        let (g_hard, _) = run(ErosionParams { diffusion: 0.3, ..Default::default() });
        assert!(steep_frac(&g_soft) <= steep_frac(&g_hard));
    }

    #[test]
    fn hardness_contrast_changes_the_result() {
        let base = test_grid(17, 1.0);
        let n = base.data.len();
        // Synthetic hardness: hard in the west half, soft in the east.
        let hard: Vec<f64> = (0..n)
            .map(|i| if i % 80 < 40 { 1.0 } else { -1.0 })
            .collect();
        let p = ErosionParams { intensity: 0.8, hardness_contrast: 0.7, ..Default::default() };
        let mut with = base.clone();
        let mut without = base.clone();
        erode(&mut with, Some(&hard), &p);
        erode(&mut without, None, &p);
        assert_ne!(with.data, without.data);
        let mut zeroc = base.clone();
        let p0 = ErosionParams { intensity: 0.8, hardness_contrast: 0.0, ..Default::default() };
        erode(&mut zeroc, Some(&hard), &p0);
        assert_eq!(zeroc.data, without.data, "contrast 0 must ignore hardness");
    }

    #[test]
    fn uses_world_spec_smoke() {
        // The canonical use: erode a macro-sized world grid without panics.
        let mut t = generate(&world_spec(MACRO_N), 2024, &TerrainParams::default()).heights;
        let outs = erode(&mut t, None, &ErosionParams::default());
        assert!(outs.mass.eroded > 0.0);
    }
}
