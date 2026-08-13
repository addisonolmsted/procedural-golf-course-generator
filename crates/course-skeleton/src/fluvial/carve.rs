//! The derived channel network: erode, then extract.
//!
//! S2 used to AUTHOR channel polylines — grow a trunk, sprout tributaries,
//! then defend the result with crossing guards, separation neighbourhoods,
//! mouth-angle enforcement, curl penalties and a final trim sweep. Ten
//! structural patches later the review still found loops and long parallel
//! pairs (seed 48). The paradigm was the bug: geometry authored in path
//! space has to be *argued* into consistency with the terrain it sits on.
//!
//! Here the terrain decides. A light stream-power erosion runs on C1's
//! implied surface (plus a sub-macro roughness seed that gives drainage
//! something to compete over), and the network is EXTRACTED from the
//! resulting flow field. What the review asked for becomes structural:
//!
//! - **No loops.** A D8 receiver graph is a forest; a loop is unreachable.
//! - **No crossings.** Two flow paths that meet share every cell after the
//!   meeting point — they merge, they cannot cross.
//! - **Sensible junctions.** Confluences sit where terrain converges, and
//!   the merge angle is the angle at which the two valleys arrive.
//! - **Parallel only where earned.** Neighbouring channels stay separate
//!   exactly when a divide separates them.
//!
//! Density stops being a growth budget and becomes one extraction
//! threshold: rills below it stay in the surface as texture for S3.
//!
//! Everything is fixed-count and position-seeded: N erosion iterations, a
//! fixed wave transcript, no data-dependent loops (stage-02's budget rule).

use course_contracts::metadata::Edge;
use course_seed::DetRng;
use course_world::flow;
use course_world::grid::{Grid, GridSpec};
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

use crate::kernel::Channel;

/// Sub-macro roughness waves: the convergence seed. C1's field is
/// band-limited to >= 400 m, and flow over a surface that smooth runs in
/// near-parallel sheets — the roughness is what makes valleys compete and
/// capture. Wavelengths sit in S2's own band (>= 64 m), so this is not S3
/// texture leaking upstream; it is the spur-and-hollow degree of freedom
/// the pipeline revision identified as missing.
pub const N_WAVES: usize = 32;
pub const WAVE_BAND_M: (f64, f64) = (140.0, 520.0);
/// Draws consumed: 4 per wave + 1 threshold jitter.
pub const DRAWS: usize = N_WAVES * 4 + 1;

/// Default erosion iterations (fixed — never "until converged").
pub const ITERS: usize = 15;
/// Per-iteration vertical clamp (m): keeps a single step from cutting a
/// gorge where accumulation is huge.
pub const STEP_CLAMP_M: f64 = 0.45;
/// Minimum roughness amplitude (m) regardless of relief — the flat-biome
/// symmetry breaker (see `carve`).
pub const ROUGH_FLOOR_M: f64 = 0.55;

/// The carve's own dials, derived from the biome dials by the caller.
pub struct CarveParams {
    /// Roughness amplitude as a fraction of the C1 relief amplitude.
    pub roughness_frac: f64,
    /// Stream-power coefficient (metres per unit sqrt(area)*slope).
    pub k: f64,
    /// Channel-extraction threshold in drained AREA (m²).
    pub area_threshold_m2: f64,
    /// Scales total incision (negative integration shrinks it).
    pub incision_scale: f64,
    /// Base-level drawdown (m) applied as a ramp into the base edge —
    /// the tile drains to something, and that something is lower.
    pub base_drop_m: f64,
    /// Upstream drainage area (m²) entering at the trunk inlet — the
    /// catchment that lies OUTSIDE the tile. Zero disables the inlet.
    pub inflow_area_m2: f64,
    /// Rim the three non-base borders so all outflow leaves via the base
    /// edge (S2's contract). See `carve`.
    pub close_borders: bool,
    /// Erosion iterations.
    pub iters: usize,
    /// Per-iteration vertical clamp (m).
    pub step_clamp_m: f64,
}

/// What the carve produces: the eroded surface plus the extracted network.
pub struct Carved {
    /// The carved base surface (8 m) — valleys are cut into it already.
    pub z: Grid<f64>,
    /// Per-cell channel membership: `channel_of[lin]` = Some(index).
    pub channel_of: Vec<Option<u32>>,
    /// Strahler order per channel cell (0 where not a channel).
    pub order_at: Vec<u8>,
    /// D8 receiver index per cell on the carved surface (-1 = outlet).
    pub rec: Vec<i64>,
    /// Drained area, m².
    pub area: Vec<f64>,
    /// The traced vector network (for QA instruments and the viewer).
    pub channels: Vec<Channel>,
}

/// Amplitude and band of the flat-breaking perturbation (see `carve`).
pub const DEFLAT_AMP_M: f64 = 0.10;
/// Width of the perturbed skirt inside the rimmed borders (m).
pub const RIM_SKIRT_M: f64 = 260.0;

/// Length of the base-level drawdown ramp (m).
pub const BASE_RAMP_M: f64 = 900.0;
/// External catchment (m²) entering at the trunk inlet, per unit of the
/// biome's `trunk_river` dial. A river valley (dial 1.0) gets 20 km² —
/// two orders above the tile's own 9 km², which is what makes its trunk a
/// through-going river rather than one more tributary; at 2.5 km² (the
/// first attempt) the external river was merely comparable to the tile's
/// own catchments and failed to dominate on half the seeds. Biomes with
/// the dial at zero (heathland kettle country, sandhills) get no external
/// river at all, which is correct for them.
pub const INFLOW_PER_TRUNK_DIAL_M2: f64 = 2.0e7;

/// Default extraction threshold (m² of drained area). The corpus cut is
/// 6e4 (extract_v2's CHANNEL_AREA_M2), but that number is not directly
/// transferable: our tile is rimmed (all water leaves by one edge) and its
/// routing surface is perturbed to avoid grid artifacts, both of which
/// change how accumulation concentrates. The dial is therefore fitted to
/// the corpus's measured DENSITY band (2.3–2.7 km/km²) rather than copied,
/// and sits on the low side of it by review request — sub-threshold rills
/// stay in the surface as texture for S3's dictionary. Final calibration
/// happens against the D5 battery once the engine is integrated.
pub const AREA_THRESHOLD_M2: f64 = 2.5e4;
fn base_ramp_m() -> f64 { std::env::var("RAMP").ok().and_then(|v| v.parse().ok()).unwrap_or(BASE_RAMP_M) }
pub const DEFLAT_BAND_M: (f64, f64) = (48.0, 160.0);

/// Deterministic roughness field: a fixed-count wave sum, mean-zero.
fn roughness(spec: &GridSpec, rng: &mut DetRng, amp: f64) -> Vec<f64> {
    roughness_at(spec, rng, amp, WAVE_BAND_M, N_WAVES)
}

/// Wave-sum field with an explicit band and count.
fn roughness_at(
    spec: &GridSpec,
    rng: &mut DetRng,
    amp: f64,
    band: (f64, f64),
    count: usize,
) -> Vec<f64> {
    let (lo, hi) = band;
    let log_ratio = libm::log(hi / lo);
    let mut waves = Vec::with_capacity(count);
    for _ in 0..count {
        let (u_lam, u_dir, u_phi, u_amp) =
            (rng.next_f64(), rng.next_f64(), rng.next_f64(), rng.next_f64());
        let lam = lo * libm::exp(u_lam * log_ratio);
        let dir = u_dir * std::f64::consts::PI;
        let phase = u_phi * std::f64::consts::TAU;
        // mild red tilt: longer waves carry more amplitude
        let a = libm::pow(lam / hi, 0.5) * (0.6 + 0.8 * u_amp);
        waves.push((lam, libm::cos(dir), libm::sin(dir), phase, a));
    }
    let var: f64 = waves.iter().map(|w| w.4 * w.4 * 0.5).sum();
    let norm = amp / var.sqrt().max(1e-9);
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut out = vec![0.0; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let p = spec.world_of(x as u32, y as u32);
            let mut v = 0.0;
            for (lam, ca, sa, phase, a) in &waves {
                let u = p.x * ca + p.y * sa;
                v += a * libm::sin(u / lam * std::f64::consts::TAU + phase);
            }
            out[y * nx + x] = v * norm;
        }
    }
    out
}

/// Erode `implied` into a carved surface whose flow field IS the network.
///
/// `keep_pit` marks cells that must stay sinks (basin embryos): they are
/// excluded from depression filling, so they capture their catchment and
/// deranged drainage emerges instead of being special-cased.
/// `erodibility` is a per-cell multiplier (C1 hardness): resistant ground
/// deflects channels physically, which is what the old geometric
/// discontinuity rules were imitating.
pub fn carve(
    spec: &GridSpec,
    implied: &Grid<f64>,
    erodibility: &[f64],
    keep_pit: &[bool],
    base_edge: Edge,
    rng: &mut DetRng,
    p: &CarveParams,
    relief_amp_m: f64,
) -> Carved {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    let cell = spec.cell_size;
    let cell_area = cell * cell;

    // ---- seed surface: implied + roughness -----------------------------
    // ABSOLUTE FLOOR on the roughness amplitude. Scaling it purely by
    // relief starves the flat biomes (river_valley's relief_amp is ~1.4 m,
    // giving 7 cm of seed) and D8 then routes their flats along grid rows —
    // the first carve renders showed exactly that: kilometre-long dead
    // straight horizontal channels on bottomland. The floor is what breaks
    // the symmetry that flats otherwise resolve axis-aligned.
    let amp = (p.roughness_frac * relief_amp_m).max(ROUGH_FLOOR_M);
    let rough = roughness(spec, rng, amp);
    let mut z = implied.clone();
    for i in 0..n {
        z.data[i] += rough[i];
    }

    // BASE-LEVEL DRAWDOWN. All four borders are open outlets to the flow
    // router, so without this the tile drains radially like an island and
    // the drawn base edge carries whatever share it happens to win (12–82%
    // measured across seeds). Real tiles are not single catchments either
    // (corpus max-edge share is 46–78%), but the base edge must be the
    // DOMINANT one — it is where S1 says base level sits. A ramp lowering
    // the last ~900 m into that edge states it physically, without
    // touching interior relief.
    {
        let (nx_, ny_) = (spec.nx as usize, spec.ny as usize);
        for y in 0..ny_ {
            for x in 0..nx_ {
                let d = edge_distance_m(spec, base_edge, x, y);
                let t = (1.0 - (d / base_ramp_m()).min(1.0)).clamp(0.0, 1.0);
                z.data[y * nx_ + x] -= p.base_drop_m * t * t * (3.0 - 2.0 * t);
            }
        }
    }

    // CLOSED BOUNDARIES. The flow router treats every border cell as an
    // open outlet, so a tile drains radially like an island: measured base-
    // edge outflow was 12–82% and a regional gradient up to 1.5x relief did
    // not fix it (water leaves by whichever border it reaches first).
    // S2's contract is explicit that the trunk runs from the interior out
    // through the base-level band, so the three non-base borders are RIMMED
    // — raised out of reach — and every drop of water has to find the base
    // edge. This is the standard landscape-evolution boundary condition:
    // one open boundary, the rest no-flux.
    if p.close_borders {
        let rim = 40.0 + 4.0 * relief_amp_m;
        for y in 0..ny {
            for x in 0..nx {
                let on_border = y == 0 || y == ny - 1 || x == 0 || x == nx - 1;
                if on_border && !outlet_side(base_edge, spec, x, y) {
                    z.data[y * nx + x] += rim;
                }
            }
        }
    }

    // Outlet band: the base-level edge must stay the lowest ground so the
    // network drains THERE rather than off an arbitrary side.
    let outlet = outlet_mask(spec, base_edge);

    // TRUNK INLET: the lowest border cell that is NOT on the base edge —
    // where the through-going river enters. A tile is a window in a larger
    // landscape, and a bottomland tile's main river carries a catchment
    // that lies mostly outside it; without external inflow the biggest
    // channel only ever drains the tile's own 9 km², which is why river
    // valley seeds read as random dendritic drainage instead of "a river
    // crossing the tile" (review observation 3).
    // The inlet sits on the FAR half of the tile from base level: a main
    // river should cross the tile edge to edge, and picking the globally
    // lowest non-base border cell often put the inlet right beside the
    // outlet, so the trunk clipped a corner instead of spanning the site
    // (review: "a single meandering channel from one edge to another").
    let inlet = if p.inflow_area_m2 > 0.0 {
        let mut best = (f64::INFINITY, usize::MAX);
        for y in 0..ny {
            for x in 0..nx {
                let on_border = y == 0 || y == ny - 1 || x == 0 || x == nx - 1;
                if !on_border || outlet_side(base_edge, spec, x, y) {
                    continue;
                }
                // Require the far half AND the middle 60% of that edge:
                // the lowest far cell is often a corner, and a corner inlet
                // makes the trunk hug the rimmed border instead of crossing
                // the site (the old engine placed its outlet in the middle
                // 60% for the same reason).
                if edge_distance_m(spec, base_edge, x, y) < 0.5 * EXTENT_M {
                    continue;
                }
                let (fx, fy) = (x as f64 / (nx - 1) as f64, y as f64 / (ny - 1) as f64);
                let along = if matches!(base_edge, Edge::E | Edge::W) { fy } else { fx };
                if !(0.2..=0.8).contains(&along) {
                    continue;
                }
                let lin = y * nx + x;
                if z.data[lin] < best.0 {
                    best = (z.data[lin], lin);
                }
            }
        }
        best.1
    } else {
        usize::MAX
    };

    // ---- erosion loop (fixed iterations) -------------------------------
    let mut rec;
    let mut area = vec![cell_area; n];
    {
        // Start from a depression-free surface: the roughness sum and C1's
        // own wave interference both create closed lows, and a pooled cell
        // has zero slope so the carve can never drain it. Pre-filling makes
        // every lake a flat at spill level whose OUTLET carries the full
        // upstream area — the outlet then incises and the lake drains, the
        // way drainage integration actually works.
        z = flow::fill_depressions_masked(&z, keep_pit);
        // De-flatten — ON THE ROUTING SURFACE ONLY. A priority-flood fill
        // grades its lakes by epsilon (8e-4 m), and an eps-flat under a
        // PLANAR macro tilt gives D8 an exactly axis-parallel descent (the
        // probe found a 712 m dead-straight due-east channel, invariant to
        // every erosion dial). A short-wave perturbation gives the flow
        // real terrain to follow across former lake floors. Applying it to
        // the TERRAIN instead dug fresh pits that the next fill turned back
        // into lakes — 47% of channel cells ended up pooled, and the
        // straight runs returned. The routing surface is the only place it
        // is needed, and the carved terrain stays depression-free.
        let deflat = roughness_at(spec, &mut rng.clone(), DEFLAT_AMP_M, DEFLAT_BAND_M, 8);
        // The rim is a wall, and water pinned against a wall runs dead
        // straight along it: the probe's last 696 m run sat in row y = 1,
        // one cell inside the rimmed border. Real tiles have no walls, so
        // the wall's inner face gets the same perturbation the lake floors
        // get, and border-parallel flow wanders like everything else.
        let near_rim: Vec<bool> = (0..n)
            .map(|i| {
                let (y, x) = (i / nx, i % nx);
                let d = (y.min(ny - 1 - y).min(x).min(nx - 1 - x)) as f64 * cell;
                d < RIM_SKIRT_M && !outlet_side(base_edge, spec, x, y)
            })
            .collect();
        let route_surface = |z: &Grid<f64>, keep_pit: &[bool]| -> Grid<f64> {
            let mut zf = flow::fill_depressions_masked(z, keep_pit);
            for i in 0..zf.data.len() {
                if zf.data[i] > z.data[i] + 1e-6 || near_rim[i] {
                    zf.data[i] += deflat[i];
                }
            }
            zf
        };
        for _ in 0..(if p.k > 0.0 { p.iters } else { 0 }) {
            let zf = route_surface(&z, keep_pit);
            let (r, slope) = flow::receivers(&zf, cell);
            let acc = flow::accumulate(&r);
            for i in 0..n {
                area[i] = acc[i] as f64 * cell_area;
            }
            add_inflow(&mut area, &r, inlet, p.inflow_area_m2);
            rec = r;
            // Carve DOWNSTREAM-FIRST, capping each cell's cut at half its
            // drop to the already-carved receiver. Two artifacts died here:
            // unconstrained carving digs pits (fill turns them into flat
            // pools, and D8 crosses a pool along grid rows), while
            // repairing pits afterwards flattens whole reaches onto a
            // uniform minimum-drop ramp — which reads as a dead-straight
            // channel, exactly the 696 m horizontal run the probe found.
            // Capping by the local drop keeps the surface strictly
            // drainable with the terrain's own gradients intact.
            carve_downstream(
                &mut z, &rec, &slope, &area, erodibility, &outlet, cell_area, p,
            );
        }
        // Final route on the carved surface: this is the field the network
        // is extracted from, so it must match the surface exactly. It runs
        // for EVERY biome, including the ones that do not erode (k = 0):
        // flow still concentrates on their surface, and those lines are the
        // dry drainage the corpus measures at d2c ~103–118 m everywhere.
        let zf = route_surface(&z, keep_pit);
        let (r, _) = flow::receivers(&zf, cell);
        let acc = flow::accumulate(&r);
        rec = r;
        for i in 0..n {
            area[i] = acc[i] as f64 * cell_area;
        }
        add_inflow(&mut area, &rec, inlet, p.inflow_area_m2);
    }

    // ---- extraction ----------------------------------------------------
    // Extraction is INDEPENDENT of how hard the biome erodes. The corpus
    // measures d2c 103–118 m and density 2.3–2.7 km/km² in EVERY biome
    // including heathland and sandhills — flow concentrates on any real
    // surface whether or not a perennial stream runs there, and those are
    // the "dried channel-like features" the review saw on heathland tiles.
    // What differs per biome is incision depth (k) and integration, not
    // whether the lines exist. Gating extraction on k produced zero
    // channels for two biomes against a measured invariant.
    let is_channel: Vec<bool> =
        (0..n).map(|i| area[i] >= p.area_threshold_m2).collect();
    let order_at = strahler(&rec, &is_channel, n);
    let (channels, channel_of) = trace(spec, &rec, &is_channel, &order_at, &area, &z);

    Carved { z, channel_of, order_at, rec, area, channels }
}

/// One carving pass in downstream-to-upstream order: a cell is cut only
/// after its receiver, and never by more than half its remaining drop to
/// that receiver. Strict monotonicity holds by construction (no pits, so
/// no flat pools) without imposing any artificial uniform gradient.
/// O(n), no recursion, deterministic.
#[allow(clippy::too_many_arguments)]
fn carve_downstream(
    z: &mut Grid<f64>,
    rec: &[i64],
    slope: &[f64],
    area: &[f64],
    erodibility: &[f64],
    outlet: &[bool],
    cell_area: f64,
    p: &CarveParams,
) {
    let n = rec.len();
    let mut donors: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..n {
        let r = rec[i];
        if r >= 0 {
            donors[r as usize].push(i as u32);
        } else {
            stack.push(i);
        }
    }
    while let Some(i) = stack.pop() {
        for &d in &donors[i] {
            let d = d as usize;
            if !outlet[d] {
                let a_term = (area[d] / cell_area).sqrt();
                let want = p.k * a_term * slope[d] * erodibility[d] * p.incision_scale;
                // never take more than half the drop to the receiver, and
                // never more than the per-iteration clamp
                let head = (z.data[d] - z.data[i]).max(0.0) * 0.5;
                z.data[d] -= want.min(p.step_clamp_m).min(head);
            }
            stack.push(d);
        }
    }
}

/// Distance (m) from a cell to the base-level edge.
fn edge_distance_m(spec: &GridSpec, edge: Edge, x: usize, y: usize) -> f64 {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let (dl, dr) = (x as f64 * cell, (nx - 1 - x) as f64 * cell);
    let (db, dt) = (y as f64 * cell, (ny - 1 - y) as f64 * cell);
    match edge {
        Edge::S => db,
        Edge::N => dt,
        Edge::W => dl,
        Edge::E => dr,
        Edge::CornerSw => db.min(dl),
        Edge::CornerSe => db.min(dr),
        Edge::CornerNw => dt.min(dl),
        Edge::CornerNe => dt.min(dr),
    }
}

/// Is this border cell on the base-level side?
fn outlet_side(edge: Edge, spec: &GridSpec, x: usize, y: usize) -> bool {
    edge_distance_m(spec, edge, x, y) < 1.0
}

/// Add an external upstream catchment entering at `inlet`, propagated
/// downstream along the receiver chain.
fn add_inflow(area: &mut [f64], rec: &[i64], inlet: usize, extra: f64) {
    if inlet == usize::MAX || extra <= 0.0 {
        return;
    }
    let mut cur = inlet;
    let mut guard = 0usize;
    loop {
        area[cur] += extra;
        let r = rec[cur];
        if r < 0 {
            break;
        }
        cur = r as usize;
        guard += 1;
        if guard > rec.len() {
            break; // defensive: the flow graph is a forest, this cannot loop
        }
    }
}

/// Cells on (or just inside) the base-level edge: protected from carving so
/// the outlet band stays the drain.
fn outlet_mask(spec: &GridSpec, edge: Edge) -> Vec<bool> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    // ONE cell. A wide protected band is uncarved ground that arriving
    // flow has to cross with no channel to follow, so the eps-graded fill
    // routes it along grid rows — the long straight horizontal runs at the
    // base edge in the first carve renders. Pinning only the border row
    // keeps base level fixed while letting channels cut right up to it.
    let band = 1usize;
    let mut m = vec![false; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let on = match edge {
                Edge::S => y < band,
                Edge::N => y + band >= ny,
                Edge::W => x < band,
                Edge::E => x + band >= nx,
                Edge::CornerSw => y < band || x < band,
                Edge::CornerSe => y < band || x + band >= nx,
                Edge::CornerNw => y + band >= ny || x < band,
                Edge::CornerNe => y + band >= ny || x + band >= nx,
            };
            m[y * nx + x] = on;
        }
    }
    m
}

/// Strahler order over the flow forest, restricted to channel cells.
/// Computed bottom-up in the same in-degree topological order `accumulate`
/// uses, so it is O(n) and deterministic.
fn strahler(rec: &[i64], is_channel: &[bool], n: usize) -> Vec<u8> {
    let mut order = vec![0u8; n];
    let mut indeg = vec![0u32; n];
    for i in 0..n {
        if !is_channel[i] {
            continue;
        }
        let r = rec[i];
        if r >= 0 && is_channel[r as usize] {
            indeg[r as usize] += 1;
        }
    }
    // heads first
    let mut stack: Vec<usize> = (0..n)
        .filter(|&i| is_channel[i] && indeg[i] == 0)
        .collect();
    // per-cell running state: (max child order, count of children at max)
    let mut best = vec![0u8; n];
    let mut best_count = vec![0u32; n];
    let mut pending = indeg.clone();
    while let Some(i) = stack.pop() {
        let o = if best_count[i] >= 2 { best[i] + 1 } else { best[i].max(1) };
        order[i] = o;
        let r = rec[i];
        if r >= 0 && is_channel[r as usize] {
            let r = r as usize;
            match o.cmp(&best[r]) {
                std::cmp::Ordering::Greater => {
                    best[r] = o;
                    best_count[r] = 1;
                }
                std::cmp::Ordering::Equal => best_count[r] += 1,
                std::cmp::Ordering::Less => {}
            }
            pending[r] -= 1;
            if pending[r] == 0 {
                stack.push(r);
            }
        }
    }
    order
}

/// Trace the raster network into polylines: one `Channel` per maximal
/// constant-order reach, walking downstream through `rec` exactly the way
/// `tools/macro_campaign/real_planform.py` traces real lidar — so the QA
/// instruments compare like with like.
#[allow(clippy::too_many_arguments)]
fn trace(
    spec: &GridSpec,
    rec: &[i64],
    is_channel: &[bool],
    order_at: &[u8],
    area: &[f64],
    z: &Grid<f64>,
) -> (Vec<Channel>, Vec<Option<u32>>) {
    let (nx, _ny) = (spec.nx as usize, spec.ny as usize);
    let n = rec.len();
    let center = |lin: usize| -> Vec2 {
        spec.world_of((lin % nx) as u32, (lin / nx) as u32)
    };
    // A reach STARTS at a channel cell that is either a head (no channel
    // donor) or a confluence (>=2 channel donors), and runs downstream
    // until the next confluence or the network's end.
    let mut donors = vec![0u32; n];
    for i in 0..n {
        if !is_channel[i] {
            continue;
        }
        let r = rec[i];
        if r >= 0 && is_channel[r as usize] {
            donors[r as usize] += 1;
        }
    }
    let mut channel_of: Vec<Option<u32>> = vec![None; n];
    let mut starts: Vec<usize> = (0..n)
        .filter(|&i| is_channel[i] && donors[i] != 1)
        .collect();
    // Deterministic order: by linear index (already ascending).
    starts.sort_unstable();

    let mut channels: Vec<Channel> = Vec::new();
    // reach id per START cell, so children can look up their parent later
    let mut reach_of_cell: Vec<Option<u32>> = vec![None; n];
    let mut raw: Vec<(Vec<usize>, u8)> = Vec::new();
    for &s in &starts {
        let mut path = vec![s];
        let mut cur = s;
        loop {
            let r = rec[cur];
            if r < 0 || !is_channel[r as usize] {
                break;
            }
            let r = r as usize;
            path.push(r);
            // stop when we reach a confluence (it starts its own reach)
            if donors[r] >= 2 {
                break;
            }
            cur = r;
        }
        if path.len() < 2 {
            continue;
        }
        let o = order_at[s].max(1);
        let id = raw.len() as u32;
        for &c in &path[..path.len() - 1] {
            channel_of[c] = Some(id);
            reach_of_cell[c] = Some(id);
        }
        raw.push((path, o));
    }

    // Parent linkage: a reach's parent is the reach owning its downstream
    // terminal cell. Arc position = distance along that parent to the cell.
    for (id, (path, o)) in raw.iter().enumerate() {
        let pts: Vec<Vec2> = path.iter().map(|&l| center(l)).collect();
        let end = *path.last().unwrap();
        let parent = channel_of[end].filter(|&p| p != id as u32);
        let junction_arc_m = match parent {
            Some(p) => {
                let ppath = &raw[p as usize].0;
                let mut arc = 0.0;
                let mut acc = 0.0;
                for w in ppath.windows(2) {
                    let (a, b) = (center(w[0]), center(w[1]));
                    acc += ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                    if w[1] == end {
                        arc = acc;
                        break;
                    }
                }
                arc
            }
            None => f64::NAN,
        };
        // pts[0] is the UPSTREAM end here; the rest of the crate expects
        // pts[0] = downstream (the old grower emitted mouth-first), so
        // reverse and translate the arc accordingly.
        let mut pts_rev = pts.clone();
        pts_rev.reverse();
        channels.push(Channel {
            pts: pts_rev,
            order: *o,
            parent,
            junction_arc_m,
            area_m2: area[end],
        });
    }
    let _ = z;
    (channels, channel_of)
}
