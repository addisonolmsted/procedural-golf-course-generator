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
/// Salt for the heathland pond-count richness draw.
pub const POND_RICHNESS_SALT: u64 = 0x90D5;

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
    let basins = inventory_basins(&z8, &zf, &keep8, spec8, &mut water);

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
    let mut systems: Vec<u32> = Vec::new();
    for (k, &(a, r)) in ranked.iter().enumerate() {
        let ok = match k {
            0 => a >= min_area,
            1 => a >= min_area && (always && a >= CONFLUENCE_FRAC * trunk_area || !always),
            _ => false, // one river or the confluence of two — never more
        };
        if ok {
            systems.push(r);
        }
    }
    let n8 = spec8.nx as usize;
    let cell8 = spec8.cell_size;
    if std::env::var("HYDRO_TRACE").is_ok() {
        eprintln!("systems: {} of {} roots (trunk {:.1} km2, min_area {:.2e})",
            systems.len(), ranked.len(), trunk_area / 1.0e6, min_area);
    }
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
        let width_pers = 0.75 + identity.course_scalar(RIVER_WIDTH_SALT);
        let w_m = (8.5 * km2.sqrt() * width_pers).clamp(6.0, 80.0);
        let depth = if is_river { RIVER_DEPTH_M } else { CREEK_DEPTH_M };
        let flood_hw = (55.0 * km2.sqrt()).clamp(40.0, 380.0);
        // corridor flatten along THIS path only (reviewer: organic,
        // decisive river corridor; not every big channel)
        if std::env::var("HYDRO_TRACE").is_ok() {
            eprintln!("root {root}: stem {} pts, w {:.1} m", stem.len(), w_m);
        }
        corridor_flatten(height, spec8, &stem, flood_hw, depth);
        place_meandering_water(
            height, identity, root as u64, &stem, w_m, depth, flood_hw, is_river, water,
        );
    }
}

/// Smooth chamfer flatten along a path (the 776977e floodplain
/// machinery, seeded from the path instead of every big channel).
fn corridor_flatten(
    height: &mut Grid<f64>,
    spec8: GridSpec,
    path: &[Vec2],
    half_w: f64,
    depth: f64,
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
    let mk = |d: Vec<f64>| Grid { spec: spec8, data: d };
    let (dg, bg) = (mk(dist), mk(bed));
    let n2 = height.spec.nx as usize;
    for y in 0..n2 {
        for x in 0..n2 {
            let p = height.spec.world_of(x as u32, y as u32);
            let d = dg.bilinear(p);
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

/// Meandering water along a downstream path; amplitude fold-bounded and
/// self-intersection-checked (reviewer gap #6).
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
    let lam = (12.0 * w_m).clamp(140.0, 900.0);
    // fold bound: |d(off)/ds| < 0.5 needs amp·2π/λ·Σweights < 0.5 —
    // with harmonic weights 0.55+0.30/0.47+0.15/2.3 ≈ 1.26 effective at
    // the shortest λ·0.47 ⇒ amp ≤ 0.08·λ. Also stay inside the
    // floodplain.
    let mut amp = (0.35 * flood_hw).min(0.08 * lam);
    if !is_river {
        amp = amp.min(1.8 * w_m);
    }
    let (ph1, ph2, ph3) = (
        hash(1) * std::f64::consts::TAU,
        hash(2) * std::f64::consts::TAU,
        hash(3) * std::f64::consts::TAU,
    );
    for try_i in 0..4 {
        if try_i == 3 {
            // final fallback: zero offset — the traced flow path never
            // crosses itself, so water ALWAYS places
            amp = 0.0;
        }
        let mut arc = 0.0f64;
        let mut wpath: Vec<(Vec2, f64, f64)> = Vec::with_capacity(path.len());
        for (i, p) in path.iter().enumerate() {
            if i > 0 {
                let q = path[i - 1];
                arc += ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt();
            }
            let (a, b) = (path[i.saturating_sub(2)], path[(i + 2).min(path.len() - 1)]);
            let (tx, ty) = (b.x - a.x, b.y - a.y);
            let tl = (tx * tx + ty * ty).sqrt().max(1e-9);
            let (nx_, ny_) = (-ty / tl, tx / tl);
            let off = amp
                * (0.55 * libm::sin(std::f64::consts::TAU * arc / lam + ph1)
                    + 0.30 * libm::sin(std::f64::consts::TAU * arc / (lam * 0.47) + ph2)
                    + 0.15 * libm::sin(std::f64::consts::TAU * arc / (lam * 2.3) + ph3));
            let hw = 0.5 * w_m
                * (1.0 + 0.28 * libm::sin(std::f64::consts::TAU * arc / (lam * 0.31) + ph2));
            wpath.push((Vec2::new(p.x + nx_ * off, p.y + ny_ * off), arc, hw));
        }
        if self_intersects(&wpath) {
            if std::env::var("HYDRO_TRACE").is_ok() {
                eprintln!("  try {try_i}: self-intersects at amp {amp:.1}");
            }
            amp *= 0.5;
            continue;
        }
        // segment into ~50 m planes, soft-monotone
        let mut prev_surface = f64::INFINITY;
        let mut si = 0usize;
        while si < wpath.len() {
            let arc0 = wpath[si].1;
            let mut sj = si;
            while sj + 1 < wpath.len() && wpath[sj].1 - arc0 < 50.0 {
                sj += 1;
            }
            let seg = &wpath[si..=sj];
            let mut bed_min = f64::INFINITY;
            for (p, _, _) in seg.iter() {
                let x = ((p.x / cell2) as usize).min(n2 - 1);
                let y = ((p.y / cell2) as usize).min(n2 - 1);
                bed_min = bed_min.min(height.data[y * n2 + x]);
            }
            let surface = (bed_min + depth).min(prev_surface + 0.15);
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
            if cells.len() >= 8 {
                let poly = component_outline(&cells, &|i| mask.contains(&i), n2, height.spec);
                if poly.len() >= 3 {
                    water.push(WaterBody {
                        polygon: poly,
                        surface_m: surface,
                        origin: if is_river {
                            WaterPlaneOrigin::River
                        } else {
                            WaterPlaneOrigin::Creek
                        },
                        permanent: is_river,
                    });
                }
            }
            si = sj + 1;
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
fn inventory_basins(
    z8: &Grid<f64>,
    zf: &Grid<f64>,
    keep8: &[bool],
    spec8: GridSpec,
    _water: &mut Vec<WaterBody>,
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
        next += 1;
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
