//! The S4 engine: route the REAL surface, then place its water.
//!
//! Everything here reads the amplified 2 m heightfield — never S2's
//! authored fields. The skeleton appears exactly once, in the agreement
//! DIAGNOSTIC (stage doc: reported for every run, gated on by nothing).

use course_amplify::Amplified;
use course_seed::RunIdentity;
use course_skeleton::kernel::Skeleton;
use course_spec::v2::SiteSpec;
use course_world::flow as wflow;
use course_world::grid::{Grid, GridSpec};
use course_world::math::Vec2;

pub const HYDRO_VERSION: u32 = 1;
/// Derived-channel accumulation threshold: the same cut S2 extracts at,
/// so "agreement" compares like with like.
pub const CHANNEL_AREA_M2: f64 = 1.2e5;
/// A closed basin shallower than this is texture, not hydrology.
pub const BASIN_MIN_DEPTH_M: f64 = 0.25;
/// A BASE depression must be at least this deep to count as intended.
pub const INTENDED_PIT_MIN_DEPTH_M: f64 = 0.45;
/// The carve's boundary rims + drawdown ramp are CONSTRUCTION, not
/// landform; pits and basins inside this frame are excluded (the first
/// build ponded water slivers along the tile edge).
pub const EDGE_FRAME_M: f64 = 48.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformId {
    WaterTable,
    Floodplain,
    /// Water IN the channels (rivers/creeks) — reviewer-driven gameplay
    /// feature: rv always carries its river; other biomes get creeks on
    /// a per-course coin (`hydrology.channel_water` dial).
    ChannelWater,
}

/// The default ordered modifier list. Order is data: it is recorded in
/// `applied` and reordering it demonstrably changes the output (a
/// floodplain flattened before the water-table intersection ponds
/// differently than after).
/// v2: ChannelWater = river/creek placement + its corridor flatten;
/// WaterTable = SPARSE kettle ponds (deepest few only).
pub const DEFAULT_TRANSFORMS: [TransformId; 2] =
    [TransformId::ChannelWater, TransformId::WaterTable];
/// A channel system qualifies as a RIVER at this mouth discharge.
pub const RIVER_AREA_M2: f64 = 2.0e6;
/// Confluence partner waters only if it carries this fraction of the
/// trunk's discharge (reviewer: one river, or the confluence of two).
pub const CONFLUENCE_FRAC: f64 = 0.4;
/// Creek systems (coin-gated biomes) must carry at least this.
pub const CREEK_MIN_AREA_M2: f64 = 1.0e6;
pub const RIVER_DEPTH_M: f64 = 0.9;
pub const CREEK_DEPTH_M: f64 = 0.5;
/// Salt for the per-course channel-water coin (course_scalar).
pub const CHANNEL_WATER_SALT: u64 = 0xC4EE_C5;
/// Salt for the per-course river WIDTH personality (some courses carry
/// a notably wide river — reviewer request).
pub const RIVER_WIDTH_SALT: u64 = 0x81D7;
/// Salt for the rare SWITCHBACK meander personality (rivers only).
pub const SWITCHBACK_SALT: u64 = 0x5B1C;
/// Fraction of rivers that draw the switchback personality: tight
/// hairpin-adjacent bends (Houston CC-style), past the analytic fold
/// bound — the true segment-crossing check guards them.
pub const SWITCHBACK_P: f64 = 0.10;
/// Salt/odds for the BRAIDED strand: a second wound line over the same
/// trunk (a happy accident from a double-watering bug the user asked to
/// keep). Conditional on switchback, so net rarity is ~1/100 tiles.
pub const BRAID_SALT: u64 = 0xB4A1D;
pub const BRAID_P: f64 = 0.10;
/// Braided tiles read as a STREAM complex, not a mega-river: the main
/// strand is capped here (user request), the second strand at 0.62x.
pub const BRAID_CAP_W_M: f64 = 42.0;
/// Converging TRIBUTARY (the "two that converge to one" pattern): half
/// of watered tiles add one tributary that joins the main stem at a
/// real junction. Replaces the dead second-root watering, which never
/// converged (separate root systems) and left floating stubs.
pub const TRIB_SALT: u64 = 0x7B1B;
pub const TRIB_P: f64 = 0.5;
pub const TRIB_MIN_AREA_M2: f64 = 1.0e6;
/// Width personality: lo + span·u^gamma of the course scalar — a long
/// upper tail so p10–p90 tile widths span ~3× (user spec: rv 30–90 yd,
/// hc 25–65, pied/plains 10–35; rare ~100 yd rivers, 5–7 yd creeks).
pub fn width_personality(u: f64) -> f64 {
    0.55 + 1.7 * u.powf(1.6)
}
/// Width clamp (m): floor ~5 yd thread, cap just past 100 yd.
pub const WIDTH_MIN_M: f64 = 4.5;
pub const WIDTH_MAX_M: f64 = 95.0;
/// Salt for the heathland pond-count richness draw.
pub const POND_RICHNESS_SALT: u64 = 0x90D5;
/// Salt/params for BASIN LAKES (user request: a few lakes in the
/// non-heathland biomes). Sourced from the basin inventory's open
/// (fill-raised) depressions: deepest few pond on a per-course draw.
pub const LAKE_SALT: u64 = 0x1A4E;
pub const LAKE_MIN_DEPTH_M: f64 = 0.8;
pub const LAKE_MIN_AREA_M2: f64 = 2500.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterPlaneOrigin {
    WaterTable,
    Floodplain,
    ClosedBasin,
    /// A big channel carrying permanent water (area ≥ RIVER_AREA_M2).
    River,
    /// A smaller watered channel (CREEK_AREA_M2 ≤ area < RIVER_AREA_M2).
    Creek,
}

pub struct WaterBody {
    pub polygon: Vec<Vec2>,
    pub surface_m: f64,
    pub origin: WaterPlaneOrigin,
    pub permanent: bool,
}

pub struct Basin {
    pub polygon: Vec<Vec2>,
    pub lowest_m: f64,
    pub closed: bool,
    pub area_m2: f64,
}

pub struct Hydrology {
    pub hydro_version: u32,
    /// 2 m surface after the datum ops.
    pub height: Grid<f64>,
    /// Direction toward the receiver [0, 2π), 8 m; NaN at sinks/outlets.
    pub flow_dir_rad: Grid<f64>,
    /// Drained area, m², 8 m.
    pub flow_accum: Grid<f64>,
    pub water: Vec<WaterBody>,
    pub basins: Vec<Basin>,
    pub applied: Vec<TransformId>,
    pub skeleton_agreement: f64,
}

/// THE stage-04 entry point.
pub fn generate(
    spec: &SiteSpec,
    sk: &Skeleton,
    amp: &Amplified,
    identity: &RunIdentity,
    transforms: &[TransformId],
) -> Hydrology {
    let mut height = amp.height.clone();
    let n2 = height.spec.nx as usize;
    let spec8 = sk.flow_distance.spec;
    let n8 = spec8.nx as usize;

    // ---- intended pits: embryos + the base's own natural depressions ----
    let base8 = sample8(&sk.height, spec8);
    let keep8 = intended_pits(&base8, &sk.embryos, spec8);

    // ---- ordered modifiers ---------------------------------------------
    let mut water: Vec<WaterBody> = Vec::new();
    let mut applied = Vec::new();
    for t in transforms {
        match t {
            TransformId::ChannelWater => {
                channel_water_v2(&mut height, spec, sk, identity, spec8, &mut water);
            }
            TransformId::WaterTable => {
                kettle_ponds(&height, spec, sk, identity, &mut water);
            }
            TransformId::Floodplain => {} // folded into ChannelWater's corridor
        }
        applied.push(*t);
    }

    // ---- re-derive flow from the surface that now exists ---------------
    // MIN-POOL the 2 m surface into the 8 m routing grid. Point-sampling
    // aliased the fine texture into dams across the 2 m-wide thalweg line
    // (agreement 0.24, median accumulation at S2 channel cells of 4
    // cells); taking each 8 m cell's minimum preserves channel continuity
    // the way hydro-conditioning always has.
    let z8 = min_pool8(&height, spec8);
    let zf = wflow::fill_depressions_masked(&z8, &keep8);
    let (rec, _slope) = wflow::receivers(&zf, spec8.cell_size);
    let acc = wflow::accumulate(&rec);
    let cell_area8 = spec8.cell_size * spec8.cell_size;
    let mut flow_dir = Grid::filled(spec8, f64::NAN);
    let mut flow_accum = Grid::filled(spec8, 0.0f64);
    for lin in 0..n8 * n8 {
        flow_accum.data[lin] = acc[lin] as f64 * cell_area8;
        let r = rec[lin];
        if r >= 0 {
            let (y, x) = ((lin / n8) as f64, (lin % n8) as f64);
            let (ry, rx) = ((r as usize / n8) as f64, (r as usize % n8) as f64);
            flow_dir.data[lin] =
                course_contracts::units::normalize_direction(libm::atan2(ry - y, rx - x));
        }
    }

    // ---- agreement diagnostic ------------------------------------------
    let skeleton_agreement = agreement(sk, &flow_accum, n8);

    // ---- basin inventory -----------------------------------------------
    let basins = inventory_basins(&z8, &zf, &keep8, spec8, spec, identity, &mut water);

    let _ = n2;
    Hydrology {
        hydro_version: HYDRO_VERSION,
        height,
        flow_dir_rad: flow_dir,
        flow_accum,
        water,
        basins,
        applied,
        skeleton_agreement,
    }
}

/// Min-pool a 2 m grid into 8 m cells (each 8 m cell = min of its 4×4).
fn min_pool8(g2: &Grid<f64>, spec8: GridSpec) -> Grid<f64> {
    let n8 = spec8.nx as usize;
    let n2 = g2.spec.nx as usize;
    let mut g = Grid::filled(spec8, f64::INFINITY);
    for y2 in 0..n2 {
        for x2 in 0..n2 {
            let (x8, y8) = ((x2 / 4).min(n8 - 1), (y2 / 4).min(n8 - 1));
            let i8 = y8 * n8 + x8;
            let v = g2.data[y2 * n2 + x2];
            if v < g.data[i8] {
                g.data[i8] = v;
            }
        }
    }
    g
}

/// Sample a 2 m grid at the 8 m node positions (they align exactly).
fn sample8(g2: &Grid<f64>, spec8: GridSpec) -> Grid<f64> {
    let n8 = spec8.nx as usize;
    let mut g = Grid::filled(spec8, 0.0f64);
    for y in 0..n8 {
        for x in 0..n8 {
            let p = spec8.world_of(x as u32, y as u32);
            g.data[y * n8 + x] = g2.bilinear(p);
        }
    }
    g
}

/// Cells allowed to remain sinks: basin embryos plus any depression the
/// SKELETON's base already carried (a deranged biome's natural basins).
fn intended_pits(base8: &Grid<f64>, embryos: &[course_skeleton::kernel::BasinEmbryo], spec8: GridSpec) -> Vec<bool> {
    let n8 = spec8.nx as usize;
    let f = wflow::fill_depressions(base8);
    let mut keep = vec![false; n8 * n8];
    // Only SUBSTANTIAL depressions are intended (kettles, interdune lows —
    // metres deep). The 0.02 m version marked the carve's incidental
    // shallow flats too (~8% of cells on some biomes), every one became a
    // flow-capturing pit, and the derived network fragmented into ≤0.5 km²
    // shards — skeleton agreement read 0.07 with accumulation at S2
    // channel cells of ~3 cells median.
    let frame = (EDGE_FRAME_M / spec8.cell_size).ceil() as usize;
    for i in 0..n8 * n8 {
        let (y, x) = (i / n8, i % n8);
        if x < frame || y < frame || x >= n8 - frame || y >= n8 - frame {
            continue;
        }
        if f.data[i] > base8.data[i] + INTENDED_PIT_MIN_DEPTH_M {
            keep[i] = true;
        }
    }
    let cell = spec8.cell_size;
    for e in embryos {
        let r = e.radius_m + cell;
        let x0 = (((e.center.x - r) / cell).floor().max(0.0)) as usize;
        let x1 = ((((e.center.x + r) / cell).ceil()) as usize).min(n8 - 1);
        let y0 = (((e.center.y - r) / cell).floor().max(0.0)) as usize;
        let y1 = ((((e.center.y + r) / cell).ceil()) as usize).min(n8 - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = spec8.world_of(x as u32, y as u32);
                if (p.x - e.center.x).powi(2) + (p.y - e.center.y).powi(2) <= r * r {
                    keep[y * n8 + x] = true;
                }
            }
        }
    }
    keep
}

/// v2 river/creek placement (reviewer gaps #4, #6) + corridor flatten.
///
/// THE river is the maximum-discharge system's MAIN STEM: root reach,
/// then at every junction the child carrying the most discharge —
/// continuous mouth-to-head by construction. A confluence partner (the
/// second system) waters only at ≥ CONFLUENCE_FRAC of the trunk.
/// Fractional-dial biomes flip one per-course coin and, when wet, water
/// only their top 1–2 systems above CREEK_MIN_AREA_M2 — "few channels
/// with major effects" applies to water too. The corridor gets the
/// smooth chamfer flatten ONLY along watered paths. All masks and
/// outlines live on the 2 m grid; the meander offset is fold-bounded
/// and checked for self-intersection (halve amplitude and rebuild on a
/// hit — deterministic).
fn channel_water_v2(
    height: &mut Grid<f64>,
    spec: &SiteSpec,
    sk: &Skeleton,
    identity: &RunIdentity,
    spec8: GridSpec,
    water: &mut Vec<WaterBody>,
) {
    let dial = spec.dials.get("hydrology.channel_water").copied().unwrap_or(0.0);
    let wscale = spec.dials.get("hydrology.channel_width_scale").copied().unwrap_or(1.0);
    if dial <= 0.0 || sk.channels.is_empty() {
        return;
    }
    let always = dial >= 1.0;
    let coin = identity.course_scalar(CHANNEL_WATER_SALT) < dial;
    if !always && !coin {
        return;
    }
    // ---- rank systems, build main-stem paths ---------------------------
    let chans = &sk.channels;
    let mut root_of = vec![0u32; chans.len()];
    for ci in 0..chans.len() {
        let mut r = ci as u32;
        while let Some(p) = chans[r as usize].parent {
            r = p;
        }
        root_of[ci] = r;
    }
    let mut roots: Vec<u32> = root_of.clone();
    roots.sort_unstable();
    roots.dedup();
    let mut ranked: Vec<(f64, u32)> =
        roots.iter().map(|&r| (chans[r as usize].area_m2, r)).collect();
    ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
    let trunk_area = ranked[0].0;
    let min_area = if always { RIVER_AREA_M2.min(trunk_area) } else { CREEK_MIN_AREA_M2 };
    // ONE stream per tile (user spec: a single, or two that CONVERGE).
    // Separate root systems never converge in-tile, so the old second
    // slot produced independent parallel streams (piedmont/hc) and
    // floating partner stubs (rv seeds 10/15). Convergence is now a
    // TRIBUTARY of the main stem (water_tributary), not a second root.
    let mut systems: Vec<u32> = Vec::new();
    if let Some(&(a, r)) = ranked.first() {
        if a >= min_area {
            systems.push(r);
        }
    }
    let n8 = spec8.nx as usize;
    let cell8 = spec8.cell_size;
    if std::env::var("HYDRO_TRACE").is_ok() {
        eprintln!("systems: {} of {} roots (trunk {:.1} km2, min_area {:.2e})",
            systems.len(), ranked.len(), trunk_area / 1.0e6, min_area);
    }
    let mut inlet_used = false;
    for root in systems {
        // Main stem by walking the RECEIVER FOREST upstream from the
        // mouth cell, taking the biggest donor at every step. Immune to
        // how the trace happened to split reaches (reach-stitching
        // folded into a ball on seed 2: a 300 m root stub + closest-
        // point truncation snapping to earlier meander bends), and
        // self-crossing-free by construction — a functional forest walk
        // can never revisit a cell.
        let area = chans[root as usize].area_m2;
        let is_river = area >= RIVER_AREA_M2;
        // The primary river follows the INFLOW TRUNK when the tile has
        // one: walk the receiver chain from the inlet border cell to
        // the outlet — continuous and edge-to-edge by construction
        // (greedy donor walks got captured by natural tributaries and
        // died mid-tile: reviewer seeds 9/31/48).
        // Only the LARGEST system may claim the trunk inlet: both
        // rivers taking this branch watered the SAME line twice, and
        // with serpentine phases the two copies crossed as an
        // accidental braid (user report). The confluence partner now
        // walks its own mouth like any other system.
        if is_river && !inlet_used {
            if let Some(inlet) = sk.trunk_inlet {
                inlet_used = true;
                let dir_to = |i: usize| -> Option<usize> {
                    let d = sk.flow_dir_rad.data[i];
                    if !d.is_finite() {
                        return None;
                    }
                    let dx = libm::cos(d).round() as i64;
                    let dy = libm::sin(d).round() as i64;
                    let (y, x) = ((i / n8) as i64, (i % n8) as i64);
                    let (nx_, ny_) = (x + dx, y + dy);
                    if nx_ < 0 || ny_ < 0 || nx_ >= n8 as i64 || ny_ >= n8 as i64 {
                        return None;
                    }
                    Some(ny_ as usize * n8 + nx_ as usize)
                };
                let mut cells = vec![inlet];
                let mut cur = inlet;
                let mut guard = 0;
                while let Some(j) = dir_to(cur) {
                    guard += 1;
                    if guard > n8 * 4 {
                        break;
                    }
                    cells.push(j);
                    cur = j;
                }
                let stem: Vec<Vec2> = cells
                    .iter()
                    .map(|&i| spec8.world_of((i % n8) as u32, (i / n8) as u32))
                    .collect();
                let km2 = area / 1.0e6;
                let width_pers = width_personality(identity.course_scalar(RIVER_WIDTH_SALT));
                let mut w_m =
                    (8.5 * km2.sqrt() * width_pers * wscale).clamp(WIDTH_MIN_M, WIDTH_MAX_M);
                // braided tiles read as a stream complex: cap the main
                // strand BEFORE the serpentine build so the neck rule
                // and wavelength follow the capped width
                let braid_coin = identity.course_scalar(SWITCHBACK_SALT) < SWITCHBACK_P
                    && identity.course_scalar(BRAID_SALT) < BRAID_P;
                if braid_coin {
                    w_m = w_m.min(BRAID_CAP_W_M);
                }
                let flood_hw = (55.0 * km2.sqrt()).clamp(40.0, 380.0);
                let tile = cell8 * n8 as f64;
                let line = wound_or_stem(&stem, identity, root, w_m, tile);
                let (mut owned, wound) = match line {
                    Some(l) => (l, true),
                    None => (stem.clone(), false),
                };
                if let Some(t) = edge_terminate(&owned, w_m, tile) {
                    owned = t;
                }
                let line_ref = &owned[..];
                corridor_flatten(
                    height, spec8, line_ref, flood_hw, w_m, RIVER_DEPTH_M,
                    identity.stream_seed(),
                );
                // keep-largest applies here too: healthy trunks are one
                // component anyway (rv 20/20), but edge-hug residual
                // trunks (hc/pied) pinch inside the border mask frame
                // and would strand fragments otherwise
                place_meandering_water(
                    height, identity, root as u64, line_ref, w_m, RIVER_DEPTH_M, flood_hw,
                    true, wound, true, water,
                );
                // DELIBERATE braid (~1/100 tiles): a second, narrower
                // wound strand over the same trunk with its own phase —
                // the accidental double-watering braid, kept on purpose.
                if wound && braid_coin {
                    let phase2 = identity.course_scalar(
                        BRAID_SALT ^ (root as u64).wrapping_mul(0x5DEE_CE2D),
                    ) * std::f64::consts::TAU;
                    let w2 = (0.62 * w_m).max(WIDTH_MIN_M);
                    let lam2 = (5.5 * w_m).clamp(180.0, 600.0);
                    if let Some(mut strand) =
                        serpentine_line(&stem, lam2, 1.9, phase2, tile, w2)
                    {
                        if let Some(t) = edge_terminate(&strand, w2, tile) {
                            strand = t;
                        }
                        corridor_flatten(
                            height,
                            spec8,
                            &strand,
                            flood_hw,
                            w2,
                            RIVER_DEPTH_M,
                            identity.stream_seed() ^ 0xB4A1D,
                        );
                        place_meandering_water(
                            height,
                            identity,
                            root as u64 | 0x8000_0000,
                            &strand,
                            w2,
                            RIVER_DEPTH_M,
                            flood_hw,
                            true,
                            true,
                            true,
                            water,
                        );
                    }
                }
                if !wound {
                    water_tributary(
                        height, spec, sk, identity, spec8, &cells, root, w_m, water,
                    );
                }
                continue;
            }
        }
        let mouth = chans[root as usize]
            .pts
            .iter()
            .map(|p| {
                let x = ((p.x / cell8) as usize).min(n8 - 1);
                let y = ((p.y / cell8) as usize).min(n8 - 1);
                y * n8 + x
            })
            .max_by(|&a, &b| sk.flow_accum.data[a].total_cmp(&sk.flow_accum.data[b]))
            .unwrap();
        let dir_to = |i: usize| -> Option<usize> {
            let d = sk.flow_dir_rad.data[i];
            if !d.is_finite() {
                return None;
            }
            let dx = libm::cos(d).round() as i64;
            let dy = libm::sin(d).round() as i64;
            let (y, x) = ((i / n8) as i64, (i % n8) as i64);
            let (nx_, ny_) = (x + dx, y + dy);
            if nx_ < 0 || ny_ < 0 || nx_ >= n8 as i64 || ny_ >= n8 as i64 {
                return None;
            }
            Some(ny_ as usize * n8 + nx_ as usize)
        };
        let mut cells: Vec<usize> = vec![mouth];
        let mut cur = mouth;
        loop {
            // biggest donor: the neighbor that drains INTO cur
            let (cy, cx) = ((cur / n8) as i64, (cur % n8) as i64);
            let mut best: Option<(f64, usize)> = None;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let (yy, xx) = (cy + dy, cx + dx);
                    if yy < 0 || xx < 0 || yy >= n8 as i64 || xx >= n8 as i64 {
                        continue;
                    }
                    let j = yy as usize * n8 + xx as usize;
                    if dir_to(j) == Some(cur) {
                        let a = sk.flow_accum.data[j];
                        if best.map_or(true, |(ba, _)| a > ba) {
                            best = Some((a, j));
                        }
                    }
                }
            }
            // Rivers run EDGE TO EDGE (reviewer): the skeleton's
            // re-derived accumulation lacks the trunk INFLOW, so the
            // natural-area floor died mid-tile at the inlet side. A
            // river keeps following its biggest donor (the carved trunk
            // valley leads to the far-boundary inlet) until it reaches
            // the border; creeks keep the discharge floor.
            let at_border = {
                let (cy, cx) = (cur / n8, cur % n8);
                cy <= 1 || cx <= 1 || cy >= n8 - 2 || cx >= n8 - 2
            };
            // the MOUTH is at the border by construction — only stop on
            // border contact once the walk is well upstream of it
            if at_border && cells.len() > 20 {
                break;
            }
            let floor = if is_river { 3.2e3 } else { 2.0e5 };
            match best {
                Some((a, j)) if a >= floor => {
                    cells.push(j);
                    cur = j;
                }
                _ => break,
            }
        }
        // cells run mouth→head at 8 m; convert to world points and
        // reverse so the surface chain walks downstream from the head
        let mut stem: Vec<Vec2> = cells
            .iter()
            .map(|&i| spec8.world_of((i % n8) as u32, (i / n8) as u32))
            .collect();
        stem.reverse();
        let km2 = area / 1.0e6;
        // Width varies a LOT (reviewer): stronger discharge scaling, an
        // 80 m ceiling, and a per-course personality scalar so some
        // courses carry a genuinely wide river (22 km² trunk: 30-70 m).
        let width_pers = width_personality(identity.course_scalar(RIVER_WIDTH_SALT));
        let w_m = (8.5 * km2.sqrt() * width_pers * wscale).clamp(WIDTH_MIN_M, WIDTH_MAX_M);
        let depth = if is_river { RIVER_DEPTH_M } else { CREEK_DEPTH_M };
        let flood_hw = (55.0 * km2.sqrt()).clamp(40.0, 380.0);
        // corridor flatten along THIS path only (reviewer: organic,
        // decisive river corridor; not every big channel)
        if std::env::var("HYDRO_TRACE").is_ok() {
            let tile = spec8.cell_size * n8 as f64;
            let bd = |p: &Vec2| p.x.min(p.y).min(tile - p.x).min(tile - p.y);
            eprintln!(
                "root {root}: stem {} pts, w {:.1} m, head bd {:.0} m at ({:.0},{:.0}), mouth bd {:.0} m at ({:.0},{:.0})",
                stem.len(),
                w_m,
                bd(&stem[0]),
                stem[0].x,
                stem[0].y,
                bd(stem.last().unwrap()),
                stem.last().unwrap().x,
                stem.last().unwrap().y
            );
        }
        let tile = cell8 * n8 as f64;
        let line = if is_river {
            wound_or_stem(&stem, identity, root, w_m, tile)
        } else {
            None
        };
        let (mut owned, wound) = match line {
            Some(l) => (l, true),
            None => (stem.clone(), false),
        };
        if let Some(t) = edge_terminate(&owned, w_m, tile) {
            owned = t;
        }
        let line_ref = &owned[..];
        corridor_flatten(
            height, spec8, line_ref, flood_hw, w_m, depth, identity.stream_seed(),
        );
        place_meandering_water(
            height, identity, root as u64, line_ref, w_m, depth, flood_hw, is_river, wound,
            true, water,
        );
        if !wound {
            water_tributary(height, spec, sk, identity, spec8, &cells, root, w_m, water);
        }
    }
}

/// EDGE TERMINATION (user rule): a stream that comes within
/// (width + 10 m) of a tile border mid-run rides the edge — terminate
/// it there instead: truncate and run straight out to the nearest
/// border. The legitimate entry (a path that STARTS in the border
/// zone) is skipped; a hit in the last few points is already the
/// natural terminal. The perpendicular exit lands ON the border, so
/// the notch/run-out machinery cuts the exit like any other terminal.
fn edge_terminate(line: &[Vec2], w_m: f64, tile: f64) -> Option<Vec<Vec2>> {
    if line.len() < 6 {
        return None;
    }
    let clear = w_m + 10.0;
    let bd = |p: &Vec2| p.x.min(p.y).min(tile - p.x).min(tile - p.y);
    let mut i0 = 0;
    while i0 < line.len() && bd(&line[i0]) < clear + 30.0 {
        i0 += 1;
    }
    if i0 == line.len() {
        return None; // never leaves the border zone; nothing sane to do
    }
    let hit = (i0..line.len()).find(|&i| bd(&line[i]) < clear)?;
    if line.len() - hit < 4 {
        return None; // that's the natural terminal approach
    }
    let p = line[hit];
    let mut out: Vec<Vec2> = line[..=hit].to_vec();
    let (d_w, d_e, d_s, d_n) = (p.x, tile - p.x, p.y, tile - p.y);
    let m = d_w.min(d_e).min(d_s).min(d_n);
    let (dx, dy) = if m == d_w {
        (-1.0, 0.0)
    } else if m == d_e {
        (1.0, 0.0)
    } else if m == d_s {
        (0.0, -1.0)
    } else {
        (0.0, 1.0)
    };
    let steps = (m / 8.0).ceil() as usize + 1;
    for k in 1..=steps {
        let t = k as f64 * 8.0;
        out.push(Vec2::new(
            (p.x + dx * t).clamp(0.0, tile),
            (p.y + dy * t).clamp(0.0, tile),
        ));
    }
    Some(out)
}

/// ONE converging tributary (the "two that converge" pattern): find the
/// biggest off-stem donor joining the main stem at a real junction,
/// walk it upstream creek-style, and water it down INTO the junction.
/// Coin-gated so singles stay common; skipped for wound (serpentine)
/// mains, whose water leaves the stem line.
#[allow(clippy::too_many_arguments)]
fn water_tributary(
    height: &mut Grid<f64>,
    spec: &SiteSpec,
    sk: &Skeleton,
    identity: &RunIdentity,
    spec8: GridSpec,
    main_cells: &[usize],
    root: u32,
    main_w_m: f64,
    water: &mut Vec<WaterBody>,
) {
    if identity.course_scalar(TRIB_SALT) >= TRIB_P {
        return;
    }
    let n8 = spec8.nx as usize;
    let cell8 = spec8.cell_size;
    let wscale = spec.dials.get("hydrology.channel_width_scale").copied().unwrap_or(1.0);
    let dir_to = |i: usize| -> Option<usize> {
        let d = sk.flow_dir_rad.data[i];
        if !d.is_finite() {
            return None;
        }
        let dx = libm::cos(d).round() as i64;
        let dy = libm::sin(d).round() as i64;
        let (y, x) = ((i / n8) as i64, (i % n8) as i64);
        let (nx_, ny_) = (x + dx, y + dy);
        if nx_ < 0 || ny_ < 0 || nx_ >= n8 as i64 || ny_ >= n8 as i64 {
            return None;
        }
        Some(ny_ as usize * n8 + nx_ as usize)
    };
    let in_main: std::collections::HashSet<usize> = main_cells.iter().copied().collect();
    // best junction: biggest off-stem donor, at least 350 m of stem
    // away from both terminals (those are border exits)
    let margin = (350.0 / cell8) as usize;
    let mut best: Option<(f64, usize, usize)> = None; // (accum, donor, junction)
    for (si, &c) in main_cells.iter().enumerate() {
        if si < margin || si + margin >= main_cells.len() {
            continue;
        }
        let (cy, cx) = ((c / n8) as i64, (c % n8) as i64);
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (yy, xx) = (cy + dy, cx + dx);
                if yy < 0 || xx < 0 || yy >= n8 as i64 || xx >= n8 as i64 {
                    continue;
                }
                let j = yy as usize * n8 + xx as usize;
                if in_main.contains(&j) || dir_to(j) != Some(c) {
                    continue;
                }
                let a = sk.flow_accum.data[j];
                if a >= TRIB_MIN_AREA_M2 && best.map_or(true, |(ba, _, _)| a > ba) {
                    best = Some((a, j, c));
                }
            }
        }
    }
    let Some((area, start, junction)) = best else {
        return;
    };
    // the junction must sit on the main's actually-WATERED extent: on
    // gp seed 7 the main's kept water was a stub 1.1 km east of the
    // junction and the tributary floated with both ends unconnected
    // (user report). 80 m reach covers the widest main + its banks.
    let jp = spec8.world_of((junction % n8) as u32, (junction / n8) as u32);
    let near_water = water.iter().any(|w| {
        w.polygon
            .iter()
            .any(|q| (q.x - jp.x).powi(2) + (q.y - jp.y).powi(2) < 80.0 * 80.0)
    });
    if !near_water {
        return;
    }
    // upstream walk along biggest donors, creek discharge floor
    let mut cells = vec![start];
    let mut cur = start;
    loop {
        let (cy, cx) = ((cur / n8) as i64, (cur % n8) as i64);
        let mut donor: Option<(f64, usize)> = None;
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (yy, xx) = (cy + dy, cx + dx);
                if yy < 0 || xx < 0 || yy >= n8 as i64 || xx >= n8 as i64 {
                    continue;
                }
                let j = yy as usize * n8 + xx as usize;
                if dir_to(j) == Some(cur) && !in_main.contains(&j) {
                    let a = sk.flow_accum.data[j];
                    if donor.map_or(true, |(ba, _)| a > ba) {
                        donor = Some((a, j));
                    }
                }
            }
        }
        match donor {
            Some((a, j)) if a >= 2.0e5 && cells.len() < n8 * 4 => {
                cells.push(j);
                cur = j;
            }
            _ => break,
        }
    }
    if (cells.len() as f64) * cell8 < 400.0 {
        return; // stubs read as floating fragments, not streams
    }
    // head -> junction, ending ON the main stem so the ribbons merge
    let mut stem: Vec<Vec2> = cells
        .iter()
        .rev()
        .map(|&i| spec8.world_of((i % n8) as u32, (i / n8) as u32))
        .collect();
    stem.push(spec8.world_of((junction % n8) as u32, (junction / n8) as u32));
    let km2 = area / 1.0e6;
    let pers = width_personality(identity.course_scalar(RIVER_WIDTH_SALT));
    let w_m = (8.5 * km2.sqrt() * pers * wscale)
        .clamp(WIDTH_MIN_M, WIDTH_MAX_M)
        .min(0.8 * main_w_m);
    // a tributary must end at its JUNCTION, not a border: if it grazes
    // the edge zone mid-run, drop it rather than re-terminate it
    let tile = cell8 * n8 as f64;
    let clear = w_m + 10.0;
    if stem
        .iter()
        .any(|p| p.x.min(p.y).min(tile - p.x).min(tile - p.y) < clear)
    {
        return;
    }
    let is_river = area >= RIVER_AREA_M2;
    let depth = if is_river { RIVER_DEPTH_M } else { CREEK_DEPTH_M };
    let flood_hw = (55.0 * km2.sqrt()).clamp(40.0, 380.0);
    if std::env::var("HYDRO_TRACE").is_ok() {
        eprintln!(
            "  tributary: {} pts, {:.1} km2, w {:.1} m into stem cell {}",
            stem.len(),
            km2,
            w_m,
            junction
        );
    }
    corridor_flatten(
        height, spec8, &stem, flood_hw, w_m, depth, identity.stream_seed() ^ 0x7B1B,
    );
    place_meandering_water(
        height,
        identity,
        root as u64 | 0x4000_0000,
        &stem,
        w_m,
        depth,
        flood_hw,
        is_river,
        false,
        true,
        water,
    );
}

/// Smooth chamfer flatten along a path (the 776977e floodplain
/// machinery, seeded from the path instead of every big channel).
fn corridor_flatten(
    height: &mut Grid<f64>,
    spec8: GridSpec,
    path: &[Vec2],
    half_w: f64,
    chan_w: f64,
    depth: f64,
    salt: u64,
) {
    let n8 = spec8.nx as usize;
    let cell8 = spec8.cell_size;
    let z8 = min_pool8(height, spec8);
    let mut dist = vec![f64::INFINITY; n8 * n8];
    let mut bed = vec![0.0f64; n8 * n8];
    for w2 in path.windows(2) {
        let (a, b) = (w2[0], w2[1]);
        let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let steps = (len / (0.5 * cell8)).ceil().max(1.0) as usize;
        for k in 0..=steps {
            let t = k as f64 / steps as f64;
            let x = (((a.x + (b.x - a.x) * t) / cell8) as usize).min(n8 - 1);
            let y = (((a.y + (b.y - a.y) * t) / cell8) as usize).min(n8 - 1);
            let i = y * n8 + x;
            if dist[i] > 0.0 {
                dist[i] = 0.0;
                bed[i] = z8.data[i];
            }
        }
    }
    let (orth, diag) = (cell8, cell8 * std::f64::consts::SQRT_2);
    let relax = |i: usize, j: usize, c: f64, d: &mut [f64], b: &mut [f64]| {
        if d[j] + c < d[i] {
            d[i] = d[j] + c;
            b[i] = b[j];
        }
    };
    for y in 0..n8 {
        for x in 0..n8 {
            let i = y * n8 + x;
            if x > 0 { relax(i, i - 1, orth, &mut dist, &mut bed); }
            if y > 0 {
                relax(i, i - n8, orth, &mut dist, &mut bed);
                if x > 0 { relax(i, i - n8 - 1, diag, &mut dist, &mut bed); }
                if x + 1 < n8 { relax(i, i - n8 + 1, diag, &mut dist, &mut bed); }
            }
        }
    }
    for y in (0..n8).rev() {
        for x in (0..n8).rev() {
            let i = y * n8 + x;
            if x + 1 < n8 { relax(i, i + 1, orth, &mut dist, &mut bed); }
            if y + 1 < n8 {
                relax(i, i + n8, orth, &mut dist, &mut bed);
                if x + 1 < n8 { relax(i, i + n8 + 1, diag, &mut dist, &mut bed); }
                if x > 0 { relax(i, i + n8 - 1, diag, &mut dist, &mut bed); }
            }
        }
    }
    for _ in 0..2 {
        bed = course_amplify::synth::box_filter(&bed, n8, 5);
    }
    // Rim-notch through-cut (seed 31 west edge): where the path ENDS at
    // a tile border, the carve's boundary machinery leaves a SILL just
    // inside the edge (rim rows + the drawdown-ramp shoulder — ~2.5 m
    // high for ~250 m on seed 31) standing above the chained water
    // surface: a dry gap right at the exit notch. Within 350 m of such
    // an end, the corridor CORE is cut down to the RUNNING MIN of the
    // path bed in the downstream direction — a monotone run-out that
    // meets the natural bed exactly at the window edge (no step to
    // chase) and carves through any sill down to the notch bed.
    const NOTCH_SENTINEL: f64 = 1.0e9;
    const NOTCH_WINDOW_M: f64 = 500.0;
    let tile = cell8 * n8 as f64;
    let border_dist = |p: Vec2| p.x.min(p.y).min(tile - p.x).min(tile - p.y);
    let notch_hw = 0.5 * chan_w + 14.0;
    let mut tgt = vec![NOTCH_SENTINEL; n8 * n8];
    if path.len() >= 2 {
        let mut arcs = vec![0.0f64; path.len()];
        for i in 1..path.len() {
            let (a, b) = (path[i - 1], path[i]);
            arcs[i] = arcs[i - 1] + ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        }
        let total = *arcs.last().unwrap();
        let pbed: Vec<f64> = path
            .iter()
            .map(|p| {
                let x = ((p.x / cell8) as usize).min(n8 - 1);
                let y = ((p.y / cell8) as usize).min(n8 - 1);
                z8.data[y * n8 + x]
            })
            .collect();
        // FULL-PATH running-min envelope. The border windows fixed the
        // exit notches but left every sill beyond 500 m of arc standing
        // (seed 5's mid-tile constriction, the edge-gutter bumps on
        // seeds 2/4/12): any bump the chained surface can't climb is a
        // dry gap. Walking downstream, the corridor core is cut to the
        // lowest bed seen so far, plus an allowance: 0.2 m in the
        // terminal windows (decisive notch cut), 0.7·depth mid-path so
        // small rises stay as PONDED water (the reviewer's
        // pond-over-bumps) instead of a shaved trough.
        let inlet_bordered = border_dist(path[0]) <= 24.0;
        let outlet_bordered = border_dist(*path.last().unwrap()) <= 24.0;
        // seed from the min bed over the first 40 m when the path STARTS
        // at a border — the border cell itself sits on the rim rows
        let mut t = if inlet_bordered {
            let mut m = f64::INFINITY;
            for i in 0..path.len() {
                if arcs[i] > 40.0 {
                    break;
                }
                m = m.min(pbed[i]);
            }
            m
        } else {
            f64::INFINITY
        };
        let mut tpath = vec![NOTCH_SENTINEL; path.len()];
        for i in 0..path.len() {
            t = t.min(pbed[i]);
            let near_end = (inlet_bordered && arcs[i] <= NOTCH_WINDOW_M)
                || (outlet_bordered && total - arcs[i] <= NOTCH_WINDOW_M);
            // mid-path allowance sets the wetting margin: the chained
            // surface can sit as low as slot-bed + depth while the cut
            // floor sits at run-min + allow, so the floor only wets by
            // ~(depth − allow). 0.7·depth left creeks 0.15 m of margin
            // and 0.5·depth still only 0.05 (depth 0.5) — the DASHED
            // piedmont creeks. Cap at depth − 0.15 so every class keeps
            // a real margin; rivers (depth 0.9) are unaffected.
            let allow = if near_end {
                0.2
            } else {
                (0.2 + 0.5 * depth).min(depth - 0.15)
            };
            tpath[i] = t + allow;
        }
        for (i, p) in path.iter().enumerate() {
            if tpath[i] >= NOTCH_SENTINEL {
                continue;
            }
            let x = ((p.x / cell8) as usize).min(n8 - 1);
            let y = ((p.y / cell8) as usize).min(n8 - 1);
            let j = y * n8 + x;
            tgt[j] = tgt[j].min(tpath[i]);
        }
    }
    // propagate the notch target with the same chamfer (nearest-seed);
    // dist_t gates the cut to cells NEAR a window path cell — the
    // target must not leak to mid-tile stretches of the same path
    let mut dist_t = vec![f64::INFINITY; n8 * n8];
    {
        for i in 0..n8 * n8 {
            if tgt[i] < NOTCH_SENTINEL {
                dist_t[i] = 0.0;
            }
        }
        let relax = |i: usize, j: usize, c: f64, d: &mut [f64], b: &mut [f64]| {
            if d[j] + c < d[i] {
                d[i] = d[j] + c;
                b[i] = b[j];
            }
        };
        for y in 0..n8 {
            for x in 0..n8 {
                let i = y * n8 + x;
                if x > 0 { relax(i, i - 1, orth, &mut dist_t, &mut tgt); }
                if y > 0 {
                    relax(i, i - n8, orth, &mut dist_t, &mut tgt);
                    if x > 0 { relax(i, i - n8 - 1, diag, &mut dist_t, &mut tgt); }
                    if x + 1 < n8 { relax(i, i - n8 + 1, diag, &mut dist_t, &mut tgt); }
                }
            }
        }
        for y in (0..n8).rev() {
            for x in (0..n8).rev() {
                let i = y * n8 + x;
                if x + 1 < n8 { relax(i, i + 1, orth, &mut dist_t, &mut tgt); }
                if y + 1 < n8 {
                    relax(i, i + n8, orth, &mut dist_t, &mut tgt);
                    if x + 1 < n8 { relax(i, i + n8 + 1, diag, &mut dist_t, &mut tgt); }
                    if x > 0 { relax(i, i + n8 - 1, diag, &mut dist_t, &mut tgt); }
                }
            }
        }
    }
    let mk = |d: Vec<f64>| Grid { spec: spec8, data: d };
    let (dg, bg, tg, dtg) = (mk(dist), mk(bed), mk(tgt), mk(dist_t));
    let n2 = height.spec.nx as usize;
    for y in 0..n2 {
        for x in 0..n2 {
            let p = height.spec.world_of(x as u32, y as u32);
            let d = dg.bilinear(p);
            let dt = dtg.bilinear(p);
            let tv = tg.bilinear(p);
            // DEPTH-SCALED BANKS (user round: hc gorges read as raw
            // slots, and their narrow floors pinched the water to
            // slivers on ridge-spur crossings). The old fixed 12 m
            // ease made bank steepness grow with cut depth; now the
            // floor is FLAT across the corridor core and banks rise at
            // a NOISY grade beyond it, daylighting into natural ground
            // wherever it is already below the bank plane — continuous
            // by construction. BANK_GRADE is the MAX slope; correlated
            // value-noise (two octaves, ~30/95 m) only FLATTENS it
            // (user spec: constant-grade banks read as a machined
            // cutout; noise must never steepen past the validated cap).
            const BANK_GRADE: f64 = 0.45;
            const MAX_BANK_M: f64 = 95.0;
            if dt <= notch_hw + MAX_BANK_M && tv < 1.0e8 {
                let vn = |px: f64, py: f64, scale: f64| -> f64 {
                    let h = |ix: i64, iy: i64| -> f64 {
                        let mut z = salt
                            ^ (ix as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15)
                            ^ (iy as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
                        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
                        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
                        (z >> 11) as f64 / (1u64 << 53) as f64
                    };
                    let (gx, gy) = (px / scale, py / scale);
                    let (x0, y0) = (gx.floor(), gy.floor());
                    let (fx, fy) = (gx - x0, gy - y0);
                    let sm = |t: f64| t * t * (3.0 - 2.0 * t);
                    let (sx, sy) = (sm(fx), sm(fy));
                    let (x0, y0) = (x0 as i64, y0 as i64);
                    let top = h(x0, y0) * (1.0 - sx) + h(x0 + 1, y0) * sx;
                    let bot = h(x0, y0 + 1) * (1.0 - sx) + h(x0 + 1, y0 + 1) * sx;
                    top * (1.0 - sy) + bot * sy
                };
                let n = 0.6 * vn(p.x, p.y, 95.0) + 0.4 * vn(p.x, p.y, 31.0);
                let grade = BANK_GRADE * (1.0 - 0.55 * n);
                let target = tv + grade * (dt - notch_hw).max(0.0);
                let i = y * n2 + x;
                if height.data[i] > target {
                    height.data[i] = target;
                    continue;
                }
                if dt <= notch_hw {
                    continue;
                }
            }
            if d > half_w || d < 10.0 {
                continue;
            }
            let edge = ((half_w - d) / (0.35 * half_w)).clamp(0.0, 1.0);
            let cap = bg.bilinear(p) + depth + 0.2 + (1.0 - edge) * 2.0;
            let i = y * n2 + x;
            let excess = height.data[i] - cap;
            if excess > 0.0 {
                let fade = ((6.0 - excess) / 3.0).clamp(0.0, 1.0);
                let t = edge * fade;
                height.data[i] += (cap - height.data[i]) * t;
            }
        }
    }
}

/// SWITCHBACK rivers: serpentine centerline via a sine-generated curve
/// (Langbein–Leopold). The user's reference (Houston CC / Buffalo
/// Bayou) has meander AMPLITUDE comparable to WAVELENGTH over long
/// runs — unreachable by a lateral-offset model, whose offset curve
/// folds long before amp ≈ λ. Here the HEADING oscillates instead:
/// θ = homing(toward a stem lookahead) + ω·sin(2π s/λ), integrated at
/// 6 m steps, ω ≈ 109°. Tapers kill the wind near path ends and tile
/// borders; a lateral governor bounds the belt; the true segment-
/// crossing check guards, backing ω off ×0.82 per failed attempt.
fn serpentine_line(
    stem: &[Vec2],
    lam: f64,
    omega0: f64,
    phase: f64,
    tile: f64,
    w_m: f64,
) -> Option<Vec<Vec2>> {
    if stem.len() < 2 {
        return None;
    }
    // dense smoothed base with arc positions
    let mut base: Vec<Vec2> = Vec::new();
    for w2 in stem.windows(2) {
        let (a, b) = (w2[0], w2[1]);
        let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let steps = (len / 6.0).ceil().max(1.0) as usize;
        for k in 0..steps {
            let t = k as f64 / steps as f64;
            base.push(Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    base.push(*stem.last().unwrap());
    for _ in 0..2 {
        let mut sm = Vec::with_capacity(base.len() * 2);
        sm.push(base[0]);
        for w2 in base.windows(2) {
            let (a, b) = (w2[0], w2[1]);
            sm.push(Vec2::new(0.75 * a.x + 0.25 * b.x, 0.75 * a.y + 0.25 * b.y));
            sm.push(Vec2::new(0.25 * a.x + 0.75 * b.x, 0.25 * a.y + 0.75 * b.y));
        }
        sm.push(*base.last().unwrap());
        base = sm;
    }
    let mut arcs = vec![0.0f64; base.len()];
    for i in 1..base.len() {
        let (a, b) = (base[i - 1], base[i]);
        arcs[i] = arcs[i - 1] + ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    }
    let total = *arcs.last().unwrap();
    if total < 3.0 * lam {
        if std::env::var("HYDRO_TRACE").is_ok() {
            eprintln!("  serpentine: too short ({total:.0} m < 3x{lam:.0})");
        }
        return None; // too short to wind
    }
    let at = |a: f64| -> Vec2 {
        let a = a.clamp(0.0, total);
        let i = arcs.partition_point(|&v| v < a).min(base.len() - 1);
        base[i]
    };
    let bd = |p: Vec2| p.x.min(p.y).min(tile - p.x).min(tile - p.y);
    let step = 6.0;
    // 2-D backoff: soften the wind a little AND open the wavelength a
    // lot — pure omega backoff flattened wide rivers to sinuosity ~1.1
    // while still failing the neck rule; larger loops keep the wind and
    // buy their clearance geometrically.
    for attempt in 0..6 {
        let omega = omega0 * 0.90f64.powi(attempt);
        let lam = lam * (1.0 + 0.35 * attempt as f64);
        if total < 3.0 * lam {
            continue;
        }
        let mut p = base[0];
        let mut out = vec![p];
        // Valley progress rate = J0(omega_eff), the Langbein–Leopold
        // coupling: path speed and valley speed must be CONSISTENT or
        // the surplus path length ends in crossings (a fixed rate
        // pinned sinuosity at 2.18 for every omega and every attempt
        // crossed). Where the tapers suppress the wind, the rate rises
        // back toward 1 so the path tracks the base line straight.
        let j0 = |x: f64| {
            let x2 = x * x;
            1.0 - x2 / 4.0 + x2 * x2 / 64.0 - x2 * x2 * x2 / 2304.0
                + x2 * x2 * x2 * x2 / 147_456.0
        };
        let mut proj = 0.0f64;
        let max_iter = (total / step * 8.0) as usize + 400;
        for _ in 0..max_iter {
            if proj >= total - 0.55 * lam {
                break;
            }
            // heading is referenced to the valley DIRECTION (base
            // tangent), never a lookahead POINT: point-homing at a
            // ±109° swing makes the path ORBIT the slowly-advancing
            // target — the plotted lines were chains of little curls.
            // A weak cross-track term keeps the belt centered instead.
            let t_a = at((proj - 15.0).max(0.0));
            let t_b = at((proj + 15.0).min(total));
            let th_tan = libm::atan2(t_b.y - t_a.y, t_b.x - t_a.x);
            let anchor = at(proj);
            let e = Vec2::new(p.x - anchor.x, p.y - anchor.y);
            let e_lat = -e.x * libm::sin(th_tan) + e.y * libm::cos(th_tan);
            let corr = (-0.9 * e_lat / lam).clamp(-0.5, 0.5);
            let t0 = (proj / (0.9 * lam)).clamp(0.0, 1.0);
            let t1 = ((total - proj) / (1.4 * lam)).clamp(0.0, 1.0);
            let tb = ((bd(p) - 140.0) / 220.0).clamp(0.0, 1.0);
            let g = (1.0 - (e_lat.abs() / (1.1 * lam)).powi(4)).clamp(0.0, 1.0);
            // damp the wind where the VALLEY itself turns fast — loops
            // pile into a kinked base line and collide (the tangles all
            // sat on one sharp base turn)
            let t_c = at((proj + 0.35 * lam).min(total));
            let th_ahead = libm::atan2(t_c.y - t_b.y, t_c.x - t_b.x);
            let mut turn = th_ahead - th_tan;
            while turn > std::f64::consts::PI {
                turn -= std::f64::consts::TAU;
            }
            while turn < -std::f64::consts::PI {
                turn += std::f64::consts::TAU;
            }
            let tk = (1.0 - turn.abs() / 1.2).clamp(0.0, 1.0);
            let omega_eff = omega * t0 * t1 * tb * g * tk;
            proj += step * j0(omega_eff);
            // phase rides the VALLEY arc (proj): a path-arc phase
            // stalls inside loops and bunches them into collision
            let swing = omega_eff * libm::sin(std::f64::consts::TAU * proj / lam + phase);
            let th = th_tan + swing + corr;
            p = Vec2::new(p.x + step * libm::cos(th), p.y + step * libm::sin(th));
            out.push(p);
        }
        // stitch the tapered tail onto the base line out to the border
        let mut a = proj;
        while a < total {
            a += step;
            out.push(at(a));
        }
        // true segment-crossing check on the wound line
        let mut wt: Vec<(Vec2, f64, f64)> = Vec::with_capacity(out.len());
        let mut acc = 0.0;
        for (i, q) in out.iter().enumerate() {
            if i > 0 {
                acc += ((q.x - out[i - 1].x).powi(2) + (q.y - out[i - 1].y).powi(2)).sqrt();
            }
            wt.push((*q, acc, 0.0));
        }
        // NECK CLEARANCE: the centerline never crossing is not enough —
        // two loop limbs passing closer than the RIBBON is wide merge
        // into a pool (user report, seed 2 at 92 m). Any two points far
        // apart along the river must keep >= 1.55 w of open space; failing
        // attempts back omega off, so wide rivers wind gently and
        // narrow ones keep tight loops — meander varies with thickness.
        let clearance = 1.55 * w_m;
        let neck_ok = {
            let pts: Vec<&(Vec2, f64, f64)> = wt.iter().step_by(4).collect();
            let c2 = clearance * clearance;
            let arc_far = (3.0 * clearance).max(0.6 * lam);
            let mut ok = true;
            'outer: for i in 0..pts.len() {
                for j in (i + 1)..pts.len() {
                    let (a, aa, _) = pts[i];
                    let (b, ba, _) = pts[j];
                    if ba - aa < arc_far {
                        continue;
                    }
                    if (a.x - b.x).powi(2) + (a.y - b.y).powi(2) < c2 {
                        ok = false;
                        break 'outer;
                    }
                }
            }
            ok
        };
        if std::env::var("HYDRO_TRACE").is_ok() {
            eprintln!(
                "  serpentine try {attempt}: omega {omega:.2}, {} pts, sinuosity {:.2}, crosses {}, neck_ok {neck_ok}",
                out.len(),
                acc / total,
                self_intersects(&wt)
            );
            if let Ok(dir) = std::env::var("SERP_DUMP") {
                let csv: String = out
                    .iter()
                    .map(|q| format!("{:.1},{:.1}\n", q.x, q.y))
                    .collect();
                let _ = std::fs::write(format!("{dir}/serp_try{attempt}.csv"), csv);
            }
        }
        if neck_ok && !self_intersects(&wt) {
            if std::env::var("HYDRO_TRACE").is_ok() {
                eprintln!(
                    "  serpentine: attempt {attempt}, omega {omega:.2}, sinuosity {:.2}",
                    acc / total
                );
            }
            return Some(out);
        }
    }
    None
}

/// The switchback coin + serpentine build (None = keep the plain stem).
fn wound_or_stem(
    stem: &[Vec2],
    identity: &RunIdentity,
    root: u32,
    w_m: f64,
    tile: f64,
) -> Option<Vec<Vec2>> {
    if identity.course_scalar(SWITCHBACK_SALT) >= SWITCHBACK_P {
        return None;
    }
    let phase = identity
        .course_scalar(SWITCHBACK_SALT ^ (root as u64).wrapping_mul(0xA5A5_9E37))
        * std::f64::consts::TAU;
    let lam = (5.5 * w_m).clamp(180.0, 600.0);
    serpentine_line(stem, lam, 1.9, phase, tile, w_m)
}

/// Meandering water along a downstream path; amplitude fold-bounded and
/// self-intersection-checked (reviewer gap #6).
#[allow(clippy::too_many_arguments)]
#[allow(clippy::too_many_arguments)]
fn place_meandering_water(
    height: &Grid<f64>,
    identity: &RunIdentity,
    path_id: u64,
    stem: &[Vec2],
    w_m: f64,
    depth: f64,
    flood_hw: f64,
    is_river: bool,
    pre_wound: bool,
    keep_longest: bool,
    water: &mut Vec<WaterBody>,
) {
    if stem.len() < 2 {
        return;
    }
    let n2 = height.spec.nx as usize;
    let cell2 = height.spec.cell_size;
    let seed = identity.stream_seed();
    let hash = |k: u64| -> f64 {
        let mut z = seed ^ path_id.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ k.rotate_left(31);
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        (z >> 11) as f64 / (1u64 << 53) as f64
    };
    // densify + smooth
    let mut path: Vec<Vec2> = Vec::new();
    for w2 in stem.windows(2) {
        let (a, b) = (w2[0], w2[1]);
        let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
        let steps = (len / 6.0).ceil().max(1.0) as usize;
        for k in 0..steps {
            let t = k as f64 / steps as f64;
            path.push(Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
        }
    }
    path.push(*stem.last().unwrap());
    for _ in 0..2 {
        let mut sm = Vec::with_capacity(path.len() * 2);
        sm.push(path[0]);
        for w2 in path.windows(2) {
            let (a, b) = (w2[0], w2[1]);
            sm.push(Vec2::new(0.75 * a.x + 0.25 * b.x, 0.75 * a.y + 0.25 * b.y));
            sm.push(Vec2::new(0.25 * a.x + 0.75 * b.x, 0.25 * a.y + 0.75 * b.y));
        }
        sm.push(*path.last().unwrap());
        path = sm;
    }
    // wavelength rides WIDTH across the whole range (user spec): a 5 yd
    // creek wiggles at ~80 m while an 80 yd river sweeps ~1 km bends.
    // The old [140, 900] clamp flattened exactly those extremes.
    let lam = (12.0 * w_m).clamp(80.0, 1400.0);
    // fold bound: |d(off)/ds| < 0.5 needs amp·2π/λ·Σweights < 0.5 —
    // with harmonic weights 0.55+0.30/0.47+0.15/2.3 ≈ 1.26 effective at
    // the shortest λ·0.47 ⇒ amp ≤ 0.08·λ. Also stay inside the
    // floodplain. Pre-wound (serpentine) paths carry their meander in
    // the LINE itself — no offset on top.
    let mut amp = if pre_wound { 0.0 } else { (0.35 * flood_hw).min(0.08 * lam) };
    if !is_river {
        // keep the ribbon INSIDE the corridor cut core (0.5·w+14 m from
        // the stem): a 1.8·w wander pushed narrow creeks onto uncut
        // rough ground and dashed the ribbon (piedmont seed 11). Rivers
        // don't need this — their valleys are broadly carved.
        amp = amp
            .min(1.8 * w_m)
            .min((0.5 * w_m + 14.0) - 0.66 * w_m - 2.0)
            .max(0.0);
    }
    let (ph1, ph2, ph3) = (
        hash(1) * std::f64::consts::TAU,
        hash(2) * std::f64::consts::TAU,
        hash(3) * std::f64::consts::TAU,
    );
    // cumulative arc + total, for the end tapers
    let mut arcs = vec![0.0f64; path.len()];
    for i in 1..path.len() {
        let (a, b) = (path[i - 1], path[i]);
        arcs[i] = arcs[i - 1] + ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    }
    let arc_total = *arcs.last().unwrap();
    for try_i in 0..4 {
        if try_i == 3 {
            // final fallback: zero offset — the traced flow path never
            // crosses itself, so water ALWAYS places
            amp = 0.0;
        }
        let mut wpath: Vec<(Vec2, f64, f64)> = Vec::with_capacity(path.len());
        for (i, p) in path.iter().enumerate() {
            let arc = arcs[i];
            // taper the wander to ZERO near both ends: end offsets
            // pushed the path onto the boundary drawdown walls and
            // broke the ribbon near the high edge (reviewer, seed 31)
            let t0 = (arc / 300.0).clamp(0.0, 1.0);
            let t1 = ((arc_total - arc) / 300.0).clamp(0.0, 1.0);
            let end_taper = t0 * t0 * (3.0 - 2.0 * t0) * t1 * t1 * (3.0 - 2.0 * t1);
            let (a, b) = (path[i.saturating_sub(2)], path[(i + 2).min(path.len() - 1)]);
            let (tx, ty) = (b.x - a.x, b.y - a.y);
            let tl = (tx * tx + ty * ty).sqrt().max(1e-9);
            let (nx_, ny_) = (-ty / tl, tx / tl);
            let off = amp * end_taper
                * (0.55 * libm::sin(std::f64::consts::TAU * arc / lam + ph1)
                    + 0.30 * libm::sin(std::f64::consts::TAU * arc / (lam * 0.47) + ph2)
                    + 0.15 * libm::sin(std::f64::consts::TAU * arc / (lam * 2.3) + ph3));
            // breathing scales DOWN with width: +-28% reads organic on a
            // creek but bulge-and-waist on a 90 m river (a P2 tell)
            let breath = 0.28 * (35.0 / w_m).clamp(0.4, 1.0);
            let hw = 0.5 * w_m
                * (1.0 + breath * libm::sin(std::f64::consts::TAU * arc / (lam * 0.31) + ph2));
            wpath.push((Vec2::new(p.x + nx_ * off, p.y + ny_ * off), arc, hw));
        }
        if self_intersects(&wpath) {
            if std::env::var("HYDRO_TRACE").is_ok() {
                eprintln!("  try {try_i}: self-intersects at amp {amp:.1}");
            }
            amp *= 0.5;
            continue;
        }
        // segment into planes, soft-monotone. Length is SLOPE-ADAPTIVE:
        // a fixed 50 m plane on a steadily-falling creek only wets its
        // downstream portion (the bed drops a full creek depth per
        // segment) — the periodic dashed creeks on the piedmont
        // gallery. End a segment once the bed has fallen 0.35·depth so
        // steep reaches get short, tightly-tiling steps.
        let pbed2: Vec<f64> = wpath
            .iter()
            .map(|(p, _, _)| {
                let x = ((p.x / cell2) as usize).min(n2 - 1);
                let y = ((p.y / cell2) as usize).min(n2 - 1);
                height.data[y * n2 + x]
            })
            .collect();
        let mut prev_surface = f64::INFINITY;
        let mut si = 0usize;
        let mut seg_ord = 0usize;
        let mut seg_planes: Vec<(usize, WaterBody)> = Vec::new();
        while si < wpath.len() {
            let arc0 = wpath[si].1;
            let (mut bmin, mut bmax) = (pbed2[si], pbed2[si]);
            let mut sj = si;
            // 14 m planes (was 50): the constant-height steps printed
            // visible terrace arcs inside wide ribbons (a P2 tell) —
            // at 14 m a 1% grade steps 14 cm instead of half a metre
            while sj + 1 < wpath.len() && wpath[sj].1 - arc0 < 14.0 {
                let nb = pbed2[sj + 1];
                if wpath[sj].1 - arc0 >= 8.0
                    && (bmax.max(nb) - bmin.min(nb)) > 0.35 * depth
                {
                    break;
                }
                bmin = bmin.min(nb);
                bmax = bmax.max(nb);
                sj += 1;
            }
            let seg = &wpath[si..=sj];
            // ROBUST bed reference (q40), not the min: one deep pocket
            // in the corridor dragged the whole chain down, and with
            // the +0.15/segment rise cap the river then ran DRY for
            // hundreds of metres climbing back out (seeds 5/11/13/15
            // mid-tile gaps). A percentile submerges pockets instead of
            // steering by them.
            let mut beds: Vec<f64> = seg
                .iter()
                .map(|(p, _, _)| {
                    let x = ((p.x / cell2) as usize).min(n2 - 1);
                    let y = ((p.y / cell2) as usize).min(n2 - 1);
                    height.data[y * n2 + x]
                })
                .collect();
            beds.sort_by(|a, b| a.total_cmp(b));
            let bed_ref = beds[beds.len() * 2 / 5];
            // never dive under the bed: where the amplified bed rises
            // downstream (wobble), real water PONDS — hold the level
            // instead of skipping the segment (the skipped segments
            // were the reviewer's discontinuous river)
            let mut surface = (bed_ref + depth).min(prev_surface + 0.15);
            if surface < bed_ref + 0.15 * depth {
                surface = (bed_ref + 0.15 * depth).min(prev_surface.max(bed_ref + 0.1));
            }
            prev_surface = surface;
            let max_hw = seg.iter().map(|s| s.2).fold(0.0f64, f64::max);
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for (p, _, _) in seg.iter() {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            let pad = max_hw + 2.0 * cell2;
            // rivers reach the tile edge; only the outermost cells stay
            let fr = 2.0 * cell2;
            let lim = cell2 * n2 as f64 - fr;
            let cx0 = (((x0 - pad).max(fr)) / cell2) as usize;
            let cy0 = (((y0 - pad).max(fr)) / cell2) as usize;
            let cx1 = ((((x1 + pad).min(lim)) / cell2) as usize).min(n2 - 1);
            let cy1 = ((((y1 + pad).min(lim)) / cell2) as usize).min(n2 - 1);
            let mut mask = std::collections::HashSet::new();
            let mut cells = Vec::new();
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    let px = cx as f64 * cell2;
                    let py = cy as f64 * cell2;
                    let inside = seg.iter().any(|(p, _, hw)| {
                        (px - p.x) * (px - p.x) + (py - p.y) * (py - p.y) <= hw * hw
                    });
                    if !inside {
                        continue;
                    }
                    let i2 = cy * n2 + cx;
                    if height.data[i2] < surface && mask.insert(i2) {
                        cells.push(i2);
                    }
                }
            }
            if std::env::var("HYDRO_TRACE").is_ok() && cells.len() < 8 {
                eprintln!("  seg at arc {:.0}: only {} cells (surface {:.2})", arc0, cells.len(), surface);
            }
            // creeks are 3-6 px wide at 2 m — an 8-cell floor dropped
            // legitimate narrow segments and dashed the ribbon
            let min_cells = if is_river { 4 } else { 3 };
            if cells.len() >= min_cells {
                let poly = component_outline(&cells, &|i| mask.contains(&i), n2, height.spec);
                if poly.len() >= 3 {
                    seg_planes.push((
                        seg_ord,
                        WaterBody {
                            polygon: poly,
                            surface_m: surface,
                            origin: if is_river {
                                WaterPlaneOrigin::River
                            } else {
                                WaterPlaneOrigin::Creek
                            },
                            permanent: is_river,
                        },
                    ));
                }
            }
            seg_ord += 1;
            si = sj + 1;
        }
        // LONGEST-RUN selection (generalizes the creek head trim): any
        // stream that is not the border-to-border trunk keeps only its
        // longest consecutive run of placed planes. Headwater flicker,
        // border-frame stubs and mid-run breaks all read as FLOATING
        // FRAGMENTS otherwise (user report: pieces disconnected from
        // the trunk near edges) — a shorter clean stream beats a chain
        // of satellites.
        if keep_longest && !seg_planes.is_empty() {
            // SPATIAL largest component, not ord-runs: a border-clipped
            // pinch can break the ribbon while every segment still
            // places (consecutive ords), so only actual polygon
            // adjacency (40 m dilated bboxes, union-find) tells truth.
            let n = seg_planes.len();
            let boxes: Vec<(f64, f64, f64, f64)> = seg_planes
                .iter()
                .map(|(_, w)| {
                    let (mut x0, mut x1, mut y0, mut y1) =
                        (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                    for p in &w.polygon {
                        x0 = x0.min(p.x);
                        x1 = x1.max(p.x);
                        y0 = y0.min(p.y);
                        y1 = y1.max(p.y);
                    }
                    (x0, x1, y0, y1)
                })
                .collect();
            let mut parent: Vec<usize> = (0..n).collect();
            fn find(parent: &mut [usize], i: usize) -> usize {
                let mut r = i;
                while parent[r] != r {
                    r = parent[r];
                }
                let mut c = i;
                while parent[c] != r {
                    let nx = parent[c];
                    parent[c] = r;
                    c = nx;
                }
                r
            }
            let dil = 40.0;
            for i in 0..n {
                for j in (i + 1)..n {
                    let (a, b) = (boxes[i], boxes[j]);
                    if a.0 - dil <= b.1 && b.0 - dil <= a.1 && a.2 - dil <= b.3 && b.2 - dil <= a.3
                    {
                        let (ri, rj) = (find(&mut parent, i), find(&mut parent, j));
                        parent[ri] = rj;
                    }
                }
            }
            let mut count: std::collections::HashMap<usize, usize> =
                std::collections::HashMap::new();
            for i in 0..n {
                *count.entry(find(&mut parent, i)).or_insert(0) += 1;
            }
            // deterministic winner: max count, ties by smallest root idx
            let best_root = count
                .iter()
                .max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(a.0)))
                .map(|(r, _)| *r)
                .unwrap();
            let mut keep = vec![false; n];
            for i in 0..n {
                keep[i] = find(&mut parent, i) == best_root;
            }
            for (i, (_, w)) in seg_planes.into_iter().enumerate() {
                if keep[i] {
                    water.push(w);
                }
            }
        } else {
            water.extend(seg_planes.into_iter().map(|(_, w)| w));
        }
        return;
    }
}

/// TRUE self-intersection test: segment-crossing sweep on a decimated
/// polyline. Proximity was the wrong test — the S2 trunk already
/// meanders, and opposite banks of a legitimate bend are close in
/// space while far in arc; only an actual CROSSING is a defect
/// (reviewer gap #6: swiggly but never intersecting itself).
fn self_intersects(wpath: &[(Vec2, f64, f64)]) -> bool {
    let pts: Vec<&(Vec2, f64, f64)> = wpath.iter().step_by(4).collect();
    let cross = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    for i in 0..pts.len().saturating_sub(1) {
        for j in (i + 3)..pts.len().saturating_sub(1) {
            let (a1, aa, _) = pts[i];
            let (a2, _, _) = pts[i + 1];
            let (b1, ba, _) = pts[j];
            let (b2, _, _) = pts[j + 1];
            if (ba - aa).abs() < 30.0 {
                continue;
            }
            let d1 = cross(*a1, *a2, *b1);
            let d2 = cross(*a1, *a2, *b2);
            let d3 = cross(*b1, *b2, *a1);
            let d4 = cross(*b1, *b2, *a2);
            if ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0)) {
                return true;
            }
        }
    }
    false
}

/// SPARSE kettle ponds (reviewer gap #5): only the deepest few kettles
/// pond, well under their rims; everything else stays dry.
fn kettle_ponds(
    height: &Grid<f64>,
    spec: &SiteSpec,
    sk: &Skeleton,
    identity: &RunIdentity,
    water: &mut Vec<WaterBody>,
) {
    if sk.embryos.is_empty() {
        return;
    }
    // a kettle ponds only where the water table can actually reach it:
    // heathland sits at 0.15-0.4 m; sandhills' 25 m table must stay dry
    if spec.descriptors.water_table_m > 2.0 {
        return;
    }
    let richness = identity.course_scalar(POND_RICHNESS_SALT);
    let want = (2.0 + 4.0 * richness).round() as usize;
    let mut ranked: Vec<&course_skeleton::kernel::BasinEmbryo> = sk.embryos.iter().collect();
    ranked.sort_by(|a, b| b.depth_m.total_cmp(&a.depth_m));
    let n2 = height.spec.nx as usize;
    let cell2 = height.spec.cell_size;
    for e in ranked.into_iter().take(want) {
        let cx = ((e.center.x / cell2) as usize).min(n2 - 1);
        let cy = ((e.center.y / cell2) as usize).min(n2 - 1);
        // local minimum within the embryo radius
        let r = (e.radius_m / cell2) as i64;
        let mut low = f64::INFINITY;
        for dy in -r..=r {
            for dx in -r..=r {
                let x = (cx as i64 + dx).clamp(0, n2 as i64 - 1) as usize;
                let y = (cy as i64 + dy).clamp(0, n2 as i64 - 1) as usize;
                low = low.min(height.data[y * n2 + x]);
            }
        }
        // pond level well under the rim: a third of the stamped depth
        let level = low + (0.33 * e.depth_m).clamp(0.25, 1.2);
        // 2 m level-set flood from the low point, capped
        let start = {
            let mut best = (f64::INFINITY, cy * n2 + cx);
            for dy in -r..=r {
                for dx in -r..=r {
                    let x = (cx as i64 + dx).clamp(0, n2 as i64 - 1) as usize;
                    let y = (cy as i64 + dy).clamp(0, n2 as i64 - 1) as usize;
                    let i = y * n2 + x;
                    if height.data[i] < best.0 {
                        best = (height.data[i], i);
                    }
                }
            }
            best.1
        };
        let cap = ((std::f64::consts::PI * e.radius_m * e.radius_m) / (cell2 * cell2) * 3.0)
            as usize;
        let mut seen = std::collections::HashSet::new();
        let mut queue = vec![start];
        seen.insert(start);
        let mut cells = vec![start];
        let mut runaway = false;
        while let Some(i) = queue.pop() {
            if seen.len() > cap {
                runaway = true;
                break;
            }
            let (y, x) = (i / n2, i % n2);
            for (dy, dx) in [(0i64, 1i64), (0, -1), (1, 0), (-1, 0)] {
                let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                if yy < 0 || xx < 0 || yy >= n2 as i64 || xx >= n2 as i64 {
                    continue;
                }
                let j = yy as usize * n2 + xx as usize;
                if !seen.contains(&j) && height.data[j] < level {
                    seen.insert(j);
                    queue.push(j);
                    cells.push(j);
                }
            }
        }
        if runaway || cells.len() < 12 {
            continue;
        }
        let poly = component_outline(&cells, &|i| seen.contains(&i), n2, height.spec);
        if poly.len() >= 3 {
            water.push(WaterBody {
                polygon: poly,
                surface_m: level,
                origin: WaterPlaneOrigin::WaterTable,
                permanent: true,
            });
        }
    }
}

/// Basin inventory from the routed surface.
#[allow(clippy::too_many_arguments)]
fn inventory_basins(
    z8: &Grid<f64>,
    zf: &Grid<f64>,
    keep8: &[bool],
    spec8: GridSpec,
    spec: &SiteSpec,
    identity: &RunIdentity,
    water: &mut Vec<WaterBody>,
) -> Vec<Basin> {
    let n8 = spec8.nx as usize;
    let cell_area = spec8.cell_size * spec8.cell_size;
    let frame = (EDGE_FRAME_M / spec8.cell_size).ceil() as usize;
    let in_frame =
        |i: usize| -> bool {
            let (y, x) = (i / n8, i % n8);
            x < frame || y < frame || x >= n8 - frame || y >= n8 - frame
        };
    // basin cells: raised by fill (open, would flood) or kept pits (closed)
    let mut label = vec![0u32; n8 * n8];
    let mut basins = Vec::new();
    let mut lake_cands: Vec<(f64, Vec<usize>, f64, f64)> = Vec::new();
    let mut next = 1u32;
    for start in 0..n8 * n8 {
        let deep = (zf.data[start] > z8.data[start] + BASIN_MIN_DEPTH_M || keep8[start])
            && !in_frame(start);
        if !deep || label[start] != 0 {
            continue;
        }
        let mut comp = vec![start];
        let mut qi = 0;
        label[start] = next;
        while qi < comp.len() {
            let cur = comp[qi];
            qi += 1;
            let (y, x) = (cur / n8, cur % n8);
            for (dy, dx) in [(0i64, 1i64), (0, -1), (1, 0), (-1, 0)] {
                let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                if yy < 0 || xx < 0 || yy >= n8 as i64 || xx >= n8 as i64 {
                    continue;
                }
                let ni = yy as usize * n8 + xx as usize;
                let ndeep = (zf.data[ni] > z8.data[ni] + BASIN_MIN_DEPTH_M || keep8[ni])
                    && !in_frame(ni);
                if ndeep && label[ni] == 0 {
                    label[ni] = next;
                    comp.push(ni);
                }
            }
        }
        if comp.len() < 4 {
            continue;
        }
        let closed = comp.iter().any(|&c| keep8[c]);
        let lowest = comp
            .iter()
            .map(|&c| z8.data[c])
            .fold(f64::INFINITY, f64::min);
        let hull = component_outline(&comp, &|i| label[i] == next, n8, spec8);
        if hull.len() >= 3 {
            basins.push(Basin {
                polygon: hull,
                lowest_m: lowest,
                closed,
                area_m2: comp.len() as f64 * cell_area,
            });
        }
        // lake candidate: OPEN basins carry a well-defined spill (the
        // fill level); kept pits are embryo machinery (kettles own them)
        if !closed {
            let spill = comp
                .iter()
                .map(|&c| zf.data[c])
                .fold(f64::NEG_INFINITY, f64::max);
            let depth = spill - lowest;
            if depth >= LAKE_MIN_DEPTH_M && comp.len() as f64 * cell_area >= LAKE_MIN_AREA_M2 {
                lake_cands.push((depth, comp.clone(), lowest, spill));
            }
        }
        next += 1;
    }
    // ---- BASIN LAKES (user request: a few, not a ton) ------------------
    // Per-course draw, scaled by the water table: shallow-table biomes
    // (rv/piedmont) see 0 lakes on ~45% of tiles, 1 on 35%, 2 on 15%,
    // 3 on 5%; a deep table thins that toward ~85% dry (hill-country
    // tanks and playa lakes PERCH on impermeable floors, so a hard
    // table cut-off was wrong — it zeroed hc and plains entirely).
    // Deepest qualifying basins pond at 55% of their depth; anything
    // overlapping existing water (rivers, creeks, kettles) is skipped
    // so floodplains don't double-fill.
    {
        let g = ((spec.descriptors.water_table_m - 2.0) / 8.0).clamp(0.0, 1.0);
        let u = identity.course_scalar(LAKE_SALT);
        let want = if u < 0.45 + 0.40 * g {
            0
        } else if u < 0.85 + 0.12 * g {
            1
        } else if u < 0.96 + 0.03 * g {
            2
        } else {
            3
        };
        if want > 0 {
            let wet_boxes: Vec<(f64, f64, f64, f64)> = water
                .iter()
                .map(|w| {
                    let (mut x0, mut x1, mut y0, mut y1) =
                        (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                    for p in &w.polygon {
                        x0 = x0.min(p.x);
                        x1 = x1.max(p.x);
                        y0 = y0.min(p.y);
                        y1 = y1.max(p.y);
                    }
                    (x0, x1, y0, y1)
                })
                .collect();
            lake_cands.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1[0].cmp(&b.1[0])));
            let mut made = 0usize;
            for (_, comp, lowest, spill) in &lake_cands {
                if made >= want {
                    break;
                }
                let surface = lowest + 0.55 * (spill - lowest);
                let cells: Vec<usize> = comp
                    .iter()
                    .copied()
                    .filter(|&c| z8.data[c] < surface)
                    .collect();
                if (cells.len() as f64) * cell_area < LAKE_MIN_AREA_M2 {
                    continue;
                }
                let (mut x0, mut x1, mut y0, mut y1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
                for &c in &cells {
                    let p = spec8.world_of((c % n8) as u32, (c / n8) as u32);
                    x0 = x0.min(p.x);
                    x1 = x1.max(p.x);
                    y0 = y0.min(p.y);
                    y1 = y1.max(p.y);
                }
                let m = 30.0;
                if wet_boxes.iter().any(|b| {
                    x0 - m <= b.1 && b.0 - m <= x1 && y0 - m <= b.3 && b.2 - m <= y1
                }) {
                    continue;
                }
                let in_cells: std::collections::HashSet<usize> = cells.iter().copied().collect();
                let poly = component_outline(&cells, &|i| in_cells.contains(&i), n8, spec8);
                if poly.len() >= 3 {
                    water.push(WaterBody {
                        polygon: poly,
                        surface_m: surface,
                        origin: WaterPlaneOrigin::ClosedBasin,
                        permanent: true,
                    });
                    made += 1;
                }
            }
        }
    }
    basins
}

/// Agreement: the fraction of S2 channel cells with a derived channel
/// within AGREE_RADIUS_CELLS, where "derived channel" uses a halved
/// accumulation cut.
///
/// Why the tolerances (visual probe, piedmont seed 11): the derived
/// network IS the same drainage system — the accumulation image traces
/// the skeleton — but S2's lower network sits AT the extraction
/// threshold by construction, so a 2-cell path displacement or a small
/// catchment split drops the derived value just under the same cut, and
/// a cell-exact same-threshold metric read 0.20 against visibly
/// agreeing networks. The diagnostic's job is to catch S3 DESTROYING
/// drainage (a weak taper), and these tolerances keep it sensitive to
/// that while ignoring sub-30 m jitter.
fn agreement(sk: &Skeleton, flow_accum: &Grid<f64>, n8: usize) -> f64 {
    const AGREE_RADIUS_CELLS: i64 = 3;
    let cut = 0.5 * CHANNEL_AREA_M2;
    let mut hits = 0usize;
    let mut total = 0usize;
    for y in 0..n8 {
        for x in 0..n8 {
            if sk.flow_distance.data[y * n8 + x] > 0.0 {
                continue;
            }
            total += 1;
            let mut ok = false;
            'scan: for dy in -AGREE_RADIUS_CELLS..=AGREE_RADIUS_CELLS {
                for dx in -AGREE_RADIUS_CELLS..=AGREE_RADIUS_CELLS {
                    let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                    if yy < 0 || xx < 0 || yy >= n8 as i64 || xx >= n8 as i64 {
                        continue;
                    }
                    if flow_accum.data[yy as usize * n8 + xx as usize] >= cut {
                        ok = true;
                        break 'scan;
                    }
                }
            }
            if ok {
                hits += 1;
            }
        }
    }
    if total == 0 {
        1.0
    } else {
        hits as f64 / total as f64
    }
}

/// Trace a connected component's outer boundary (Moore neighbor tracing
/// over the 8 m cell centres). The polygon FOLLOWS the landform: the
/// first build published convex hulls, whose rings covered 25–40%
/// non-basin terrain on the sprawling flats (component/hull area 0.58 to
/// 0.76) and read as uncorrelated with the topography in review.
///
/// `is_in` must answer for THIS component only — tracing walks
/// 8-neighbors while components are built 4-connected, so a whole-mask
/// test could leak the walk onto a diagonally-adjacent component.
fn component_outline(
    comp: &[usize],
    is_in: &dyn Fn(usize) -> bool,
    n8: usize,
    spec8: GridSpec,
) -> Vec<Vec2> {
    const DIRS: [(i64, i64); 8] = [
        (-1, 0),
        (-1, -1),
        (0, -1),
        (1, -1),
        (1, 0),
        (1, 1),
        (0, 1),
        (-1, 1),
    ];
    let inside = |c: (i64, i64)| {
        c.0 >= 0
            && c.1 >= 0
            && (c.0 as usize) < n8
            && (c.1 as usize) < n8
            && is_in(c.1 as usize * n8 + c.0 as usize)
    };
    // topmost-leftmost cell: a boundary cell whose W and N neighbors are
    // guaranteed outside, so "entered from the west" is a valid start
    let &start = comp
        .iter()
        .min_by_key(|&&c| (c / n8, c % n8))
        .expect("outline of empty component");
    let s = ((start % n8) as i64, (start / n8) as i64);
    let mut p = s;
    let mut back = (s.0 - 1, s.1); // the empty cell we "entered" from
    let mut cells = vec![s];
    for _ in 0..comp.len() * 8 + 16 {
        let bi = DIRS
            .iter()
            .position(|&(dx, dy)| (p.0 + dx, p.1 + dy) == back)
            .expect("backtrack is always a neighbor");
        let mut next = None;
        let mut last_empty = back;
        for k in 1..=8 {
            let (dx, dy) = DIRS[(bi + k) % 8];
            let cand = (p.0 + dx, p.1 + dy);
            if inside(cand) {
                next = Some(cand);
                break;
            }
            last_empty = cand;
        }
        let Some(c) = next else { break }; // isolated single cell
        if c == s {
            break;
        }
        cells.push(c);
        back = last_empty;
        p = c;
    }
    cells.dedup();
    let pts: Vec<Vec2> = cells
        .iter()
        .map(|&(x, y)| spec8.world_of(x as u32, y as u32))
        .collect();
    if pts.len() >= 3 {
        pts
    } else {
        // degenerate (a line of cells): fall back to the hull of the comp
        convex_hull(
            &comp
                .iter()
                .map(|&c| spec8.world_of((c % n8) as u32, (c / n8) as u32))
                .collect::<Vec<_>>(),
        )
    }
}

/// Andrew's monotone chain. Deterministic (stable sort on (x, y) bits).
fn convex_hull(pts: &[Vec2]) -> Vec<Vec2> {
    let mut p: Vec<Vec2> = pts.to_vec();
    p.sort_by(|a, b| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y)));
    p.dedup_by(|a, b| a.x == b.x && a.y == b.y);
    if p.len() < 3 {
        return p;
    }
    let cross = |o: Vec2, a: Vec2, b: Vec2| (a.x - o.x) * (b.y - o.y) - (a.y - o.y) * (b.x - o.x);
    let mut lower: Vec<Vec2> = Vec::new();
    for &pt in &p {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], pt) <= 0.0 {
            lower.pop();
        }
        lower.push(pt);
    }
    let mut upper: Vec<Vec2> = Vec::new();
    for &pt in p.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], pt) <= 0.0 {
            upper.pop();
        }
        upper.push(pt);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}
