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
/// Floodplain envelope half-width per sqrt(km²) of discharge.
pub const FLOOD_W_PER_SQKM: f64 = 55.0;
/// Height of the flood datum above the local channel bed.
pub const FLOOD_DEPTH_M: f64 = 1.1;
/// The floodplain FLATTENS low ground; it never excavates. Cells more
/// than this far above the flood datum are valley wall, and the cap
/// fades to nothing on them (at 2x this excess) instead of cutting a
/// cliff ring around the channel.
pub const FLOOD_MAX_CUT_M: f64 = 3.0;
/// Channels at/above this drained area are RIVERS: watered whenever the
/// biome carries channel water at all.
pub const RIVER_AREA_M2: f64 = 2.0e6;
/// Channels at/above this drained area are CREEKS: watered when the
/// per-course coin passes.
pub const CREEK_AREA_M2: f64 = 6.0e5;
/// Water depth above the segment's lowest bed, lerped by discharge
/// between creek and river scale.
pub const CREEK_DEPTH_M: f64 = 0.5;
pub const RIVER_DEPTH_M: f64 = 0.9;
/// A channel-water body is built per ~this much centreline arc; each
/// segment is one flat plane (WaterBody carries a single surface), and
/// the sequence steps down monotonically like a real river profile.
pub const CHANNEL_SEG_M: f64 = 120.0;
/// Soft-monotone allowance between consecutive segments: a strict
/// downstream clamp let one deep pocket drag the whole chain below the
/// amplified bed (dashed creeks); a riffle-sized rise per segment lets
/// the surface recover while staying visually monotone.
pub const CHANNEL_RISE_TOL_M: f64 = 0.4;
/// Salt for the per-course channel-water coin (course_scalar).
pub const CHANNEL_WATER_SALT: u64 = 0xC4EE_C5;

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
pub const DEFAULT_TRANSFORMS: [TransformId; 3] =
    [TransformId::Floodplain, TransformId::WaterTable, TransformId::ChannelWater];

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
            TransformId::Floodplain => {
                floodplain_datum(&mut height, sk, &mut water);
            }
            TransformId::WaterTable => {
                water_table_datum(&mut height, spec, &keep8, spec8, &mut water);
            }
            TransformId::ChannelWater => {
                channel_water(&height, spec, sk, identity, spec8, &mut water);
            }
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

/// Flatten the flood envelope toward the trunk, leaving the channel
/// incised: cells within the discharge-scaled envelope are CAPPED at the
/// local channel bed + FLOOD_DEPTH_M; channel cells themselves and
/// anything already lower are untouched, so flow along the trunk stays
/// monotone through the flattened floor (acceptance criterion).
fn floodplain_datum(height: &mut Grid<f64>, sk: &Skeleton, water: &mut Vec<WaterBody>) {
    // big channels only: the trunk and majors carry the floodplain
    let big: Vec<&course_skeleton::kernel::Channel> = sk
        .channels
        .iter()
        .filter(|c| c.area_m2 >= 2.0e6)
        .collect();
    if big.is_empty() {
        return;
    }
    // The whole field is built SMOOTH at 8 m and bilinear-applied at 2 m.
    // The first version stamped (distance, bed) with stride-3 offsets on
    // the 2 m grid — the reviewer immediately caught the consequence: 6 m
    // Voronoi blocks with discontinuous caps ("pixelated pockets") and
    // quantized-distance bands (stripes), with lateral steps up to 60 m
    // where two stamps carrying different beds met.
    let spec8 = sk.flow_distance.spec;
    let n8 = spec8.nx as usize;
    let cell8 = spec8.cell_size;
    let z8 = min_pool8(height, spec8);
    let mut dist = vec![f64::INFINITY; n8 * n8];
    let mut bed = vec![0.0f64; n8 * n8];
    let mut wid = vec![0.0f64; n8 * n8];
    // seed: 8 m cells under the big-channel centrelines; bed from the
    // min-pooled surface (the S3 restore clamp keeps it near the carved
    // bed), width from discharge
    for c in &big {
        let w = FLOOD_W_PER_SQKM * (c.area_m2 / 1.0e6).sqrt();
        for seg in c.pts.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
            let steps = (len / (0.5 * cell8)).ceil() as usize;
            for s in 0..=steps {
                let t = s as f64 / steps.max(1) as f64;
                let x = (((a.x + (b.x - a.x) * t) / cell8) as usize).min(n8 - 1);
                let y = (((a.y + (b.y - a.y) * t) / cell8) as usize).min(n8 - 1);
                let i = y * n8 + x;
                if dist[i] > 0.0 {
                    dist[i] = 0.0;
                    bed[i] = z8.data[i];
                    wid[i] = w;
                } else {
                    // confluence cell: the bigger envelope owns it
                    wid[i] = wid[i].max(w);
                    bed[i] = bed[i].min(z8.data[i]);
                }
            }
        }
    }
    // two-pass chamfer, payload (bed, width) rides with the distance
    let (orth, diag) = (cell8, cell8 * std::f64::consts::SQRT_2);
    let relax = |i: usize, j: usize, cost: f64, dist: &mut [f64], bed: &mut [f64], wid: &mut [f64]| {
        if dist[j] + cost < dist[i] {
            dist[i] = dist[j] + cost;
            bed[i] = bed[j];
            wid[i] = wid[j];
        }
    };
    for y in 0..n8 {
        for x in 0..n8 {
            let i = y * n8 + x;
            if x > 0 {
                relax(i, i - 1, orth, &mut dist, &mut bed, &mut wid);
            }
            if y > 0 {
                relax(i, i - n8, orth, &mut dist, &mut bed, &mut wid);
                if x > 0 {
                    relax(i, i - n8 - 1, diag, &mut dist, &mut bed, &mut wid);
                }
                if x + 1 < n8 {
                    relax(i, i - n8 + 1, diag, &mut dist, &mut bed, &mut wid);
                }
            }
        }
    }
    for y in (0..n8).rev() {
        for x in (0..n8).rev() {
            let i = y * n8 + x;
            if x + 1 < n8 {
                relax(i, i + 1, orth, &mut dist, &mut bed, &mut wid);
            }
            if y + 1 < n8 {
                relax(i, i + n8, orth, &mut dist, &mut bed, &mut wid);
                if x + 1 < n8 {
                    relax(i, i + n8 + 1, diag, &mut dist, &mut bed, &mut wid);
                }
                if x > 0 {
                    relax(i, i + n8 - 1, diag, &mut dist, &mut bed, &mut wid);
                }
            }
        }
    }
    // smooth the propagated payload: kills the seams where envelopes of
    // different reaches meet (the 60 m cliffs), leaves the field local
    for _ in 0..2 {
        bed = course_amplify::synth::box_filter(&bed, n8, 5);
        wid = course_amplify::synth::box_filter(&wid, n8, 5);
    }
    let mk = |data: Vec<f64>| Grid { spec: spec8, data };
    let (distg, bedg, widg) = (mk(dist), mk(bed), mk(wid));
    let n2 = height.spec.nx as usize;
    for y in 0..n2 {
        for x in 0..n2 {
            let p = height.spec.world_of(x as u32, y as u32);
            let d = distg.bilinear(p);
            let w = widg.bilinear(p);
            if d > w || d < 12.0 {
                continue; // outside envelope, or the incised channel itself
            }
            // cap toward the flood datum, feathered at the envelope edge
            let edge = ((w - d) / (0.35 * w)).clamp(0.0, 1.0);
            let cap = bedg.bilinear(p) + FLOOD_DEPTH_M + (1.0 - edge) * 2.0;
            let i = y * n2 + x;
            let excess = height.data[i] - cap;
            if excess > 0.0 {
                let fade =
                    ((2.0 * FLOOD_MAX_CUT_M - excess) / FLOOD_MAX_CUT_M).clamp(0.0, 1.0);
                let t = edge * fade;
                height.data[i] = height.data[i] * (1.0 - t) + cap * t;
            }
        }
    }
    let _ = water; // flood extent bodies arrive with S10 dressing needs
}

/// Pond every intended closed basin up to min(water table, spill).
fn water_table_datum(
    height: &mut Grid<f64>,
    spec: &SiteSpec,
    keep8: &[bool],
    spec8: GridSpec,
    water: &mut Vec<WaterBody>,
) {
    let table_below = spec.descriptors.water_table_m;
    let n2 = height.spec.nx as usize;
    let n8 = spec8.nx as usize;
    // local datum: a broad lowpass of the surface (2 m grid, ~400 m)
    let sigma = course_amplify::conditioning::SIGMA_PER_L * 400.0 / height.spec.cell_size;
    let box_w = ((sigma * 1.153) as usize * 2 + 1).max(3);
    let mut lp = height.data.clone();
    for _ in 0..3 {
        lp = course_amplify::synth::box_filter(&lp, n2, box_w);
    }
    // flood-fill intended pits into components at 8 m; pond each
    let mut seen = vec![false; n8 * n8];
    for start in 0..n8 * n8 {
        if !keep8[start] || seen[start] {
            continue;
        }
        // gather the component
        let mut comp = vec![start];
        let mut qi = 0;
        seen[start] = true;
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
                if keep8[ni] && !seen[ni] {
                    seen[ni] = true;
                    comp.push(ni);
                }
            }
        }
        // water level: local water table, clipped under the basin's rim
        let mut lowest = f64::INFINITY;
        let mut rim = f64::NEG_INFINITY;
        let mut table = 0.0;
        for &c in &comp {
            let p = spec8.world_of((c % n8) as u32, (c / n8) as u32);
            let x2 = ((p.x / height.spec.cell_size) as usize).min(n2 - 1);
            let y2 = ((p.y / height.spec.cell_size) as usize).min(n2 - 1);
            let z = height.data[y2 * n2 + x2];
            lowest = lowest.min(z);
            rim = rim.max(z);
            table += lp[y2 * n2 + x2] - table_below;
        }
        table /= comp.len() as f64;
        let level = table.min(rim - 0.05);
        if level > lowest + 0.05 && comp.len() >= 4 {
            // pond: polygon = traced outline of the ponded component
            let in_comp: std::collections::HashSet<usize> = comp.iter().copied().collect();
            let hull = component_outline(&comp, &|i| in_comp.contains(&i), n8, spec8);
            if hull.len() >= 3 {
                water.push(WaterBody {
                    polygon: hull,
                    surface_m: level,
                    origin: if comp.len() >= 40 {
                        WaterPlaneOrigin::WaterTable
                    } else {
                        WaterPlaneOrigin::ClosedBasin
                    },
                    permanent: true,
                });
            }
        }
    }
}

/// Water IN the channels. Reviewer-driven identity feature: a river
/// valley without a river is not a river valley from a golfing
/// perspective. Rivers (area ≥ RIVER_AREA_M2) are watered whenever the
/// biome carries channel water at all; creeks additionally need the
/// per-course coin, so "occasional creeks" is a property of the COURSE,
/// not of individual reaches.
///
/// Geometry is built at 2 m, not inherited from the 8 m network raster
/// (the first build read as "low-res" blocky ribbons): the carved
/// polyline is densified and smoothed, a deterministic low-flow MEANDER
/// offset wanders it within its floodplain (real channels wander inside
/// their valley floor; the offset is a small sine sum in arc length,
/// position-seeded per channel, amplitude capped well inside the
/// discharge-scaled floodplain), width breathes gently along the arc,
/// and the flat surface planes step every ~50 m instead of 120 m.
/// Heights are NOT touched — the bed stays incised; water is a plane.
fn channel_water(
    height: &Grid<f64>,
    spec: &SiteSpec,
    sk: &Skeleton,
    identity: &RunIdentity,
    spec8: GridSpec,
    water: &mut Vec<WaterBody>,
) {
    let dial = spec
        .dials
        .get("hydrology.channel_water")
        .copied()
        .unwrap_or(0.0);
    if dial <= 0.0 {
        return;
    }
    let coin = identity.course_scalar(CHANNEL_WATER_SALT) < dial;
    let n8 = spec8.nx as usize;
    let cell8 = spec8.cell_size;
    let n2 = height.spec.nx as usize;
    let cell2 = height.spec.cell_size;
    let bed8 = min_pool8(&sk.height, spec8);
    let frame_m = EDGE_FRAME_M;
    let seed = identity.stream_seed();
    for (ci, c) in sk.channels.iter().enumerate() {
        let is_river = c.area_m2 >= RIVER_AREA_M2;
        let is_creek = c.area_m2 >= CREEK_AREA_M2 && !is_river;
        if !(is_river && (coin || dial >= 1.0)) && !(is_creek && coin) {
            continue;
        }
        let km2 = c.area_m2 / 1.0e6;
        let w_m = (5.5 * km2.sqrt()).clamp(6.0, 36.0);
        let t = ((c.area_m2 - CREEK_AREA_M2) / (RIVER_AREA_M2 - CREEK_AREA_M2)).clamp(0.0, 1.0);
        let depth = CREEK_DEPTH_M + (RIVER_DEPTH_M - CREEK_DEPTH_M) * t;
        // upstream-first (traced polylines run mouth→head; detect from bed)
        let bed_at = |p: &Vec2| {
            let x = ((p.x / cell8) as usize).min(n8 - 1);
            let y = ((p.y / cell8) as usize).min(n8 - 1);
            bed8.data[y * n8 + x]
        };
        let raw: Vec<Vec2> = if bed_at(c.pts.first().unwrap()) < bed_at(c.pts.last().unwrap()) {
            c.pts.iter().rev().copied().collect()
        } else {
            c.pts.clone()
        };
        if raw.len() < 2 {
            continue;
        }
        // densify to ~6 m spacing, then two Chaikin rounds — the water
        // path must be smooth at its own width scale
        let mut path: Vec<Vec2> = Vec::new();
        for w2 in raw.windows(2) {
            let (a, b) = (w2[0], w2[1]);
            let len = ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
            let steps = (len / 6.0).ceil().max(1.0) as usize;
            for k in 0..steps {
                let tt = k as f64 / steps as f64;
                path.push(Vec2::new(a.x + (b.x - a.x) * tt, a.y + (b.y - a.y) * tt));
            }
        }
        path.push(*raw.last().unwrap());
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
        // deterministic low-flow meander: sine sum in arc length along
        // the local normal, phases position-seeded per channel
        let hash = |k: u64| -> f64 {
            let mut z = seed ^ (ci as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ k.rotate_left(31);
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            (z >> 11) as f64 / (1u64 << 53) as f64
        };
        let flood_hw = FLOOD_W_PER_SQKM * km2.sqrt();
        // rivers wander up to ~35% of their floodplain half-width;
        // creeks a couple of channel widths
        let amp = if is_river {
            (0.35 * flood_hw).min(3.5 * w_m)
        } else {
            1.5 * w_m
        };
        let lam = (12.0 * w_m).clamp(140.0, 900.0);
        let (ph1, ph2, ph3) = (hash(1) * std::f64::consts::TAU, hash(2) * std::f64::consts::TAU, hash(3) * std::f64::consts::TAU);
        let mut arc = 0.0f64;
        let mut wpath: Vec<(Vec2, f64, f64)> = Vec::with_capacity(path.len()); // (pos, arc, halfw)
        for (i, p) in path.iter().enumerate() {
            if i > 0 {
                let q = path[i - 1];
                arc += ((p.x - q.x).powi(2) + (p.y - q.y).powi(2)).sqrt();
            }
            // local tangent → normal
            let (a, b) = (
                path[i.saturating_sub(2)],
                path[(i + 2).min(path.len() - 1)],
            );
            let (tx, ty) = (b.x - a.x, b.y - a.y);
            let tl = (tx * tx + ty * ty).sqrt().max(1e-9);
            let (nx_, ny_) = (-ty / tl, tx / tl);
            let off = amp
                * (0.55 * libm::sin(std::f64::consts::TAU * arc / lam + ph1)
                    + 0.30 * libm::sin(std::f64::consts::TAU * arc / (lam * 0.47) + ph2)
                    + 0.15 * libm::sin(std::f64::consts::TAU * arc / (lam * 2.3) + ph3));
            // width breathes ±18% at the middle harmonic
            let hw = 0.5
                * w_m
                * (1.0 + 0.18 * libm::sin(std::f64::consts::TAU * arc / (lam * 0.31) + ph2));
            wpath.push((Vec2::new(p.x + nx_ * off, p.y + ny_ * off), arc, hw));
        }
        // segment every ~50 m of arc; per segment: flat plane at local
        // amplified-bed min + depth, soft-monotone downstream
        const SEG_M: f64 = 50.0;
        let z2_at = |p: &Vec2| {
            let x = ((p.x / cell2) as usize).min(n2 - 1);
            let y = ((p.y / cell2) as usize).min(n2 - 1);
            height.data[y * n2 + x]
        };
        let mut prev_surface = f64::INFINITY;
        let mut si = 0usize;
        while si < wpath.len() {
            let arc0 = wpath[si].1;
            let mut sj = si;
            while sj + 1 < wpath.len() && wpath[sj].1 - arc0 < SEG_M {
                sj += 1;
            }
            let seg = &wpath[si..=sj];
            let mut bed_min = f64::INFINITY;
            for (p, _, _) in seg.iter() {
                bed_min = bed_min.min(z2_at(p));
            }
            let surface =
                (bed_min + depth).min(prev_surface + CHANNEL_RISE_TOL_M * SEG_M / 120.0);
            prev_surface = surface;
            // 2 m mask: distance to the segment's sub-path under the
            // local half-width, amplified ground below the surface
            let max_hw = seg.iter().map(|s| s.2).fold(0.0f64, f64::max);
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
            for (p, _, _) in seg.iter() {
                x0 = x0.min(p.x);
                y0 = y0.min(p.y);
                x1 = x1.max(p.x);
                y1 = y1.max(p.y);
            }
            let pad = max_hw + 2.0 * cell2;
            let cx0 = (((x0 - pad).max(frame_m)) / cell2) as usize;
            let cy0 = (((y0 - pad).max(frame_m)) / cell2) as usize;
            let cx1 = ((((x1 + pad).min(cell2 * n2 as f64 - frame_m)) / cell2) as usize).min(n2 - 1);
            let cy1 = ((((y1 + pad).min(cell2 * n2 as f64 - frame_m)) / cell2) as usize).min(n2 - 1);
            let mut mask = std::collections::HashSet::new();
            let mut cells = Vec::new();
            for cy in cy0..=cy1 {
                for cx in cx0..=cx1 {
                    let px = cx as f64 * cell2;
                    let py = cy as f64 * cell2;
                    let mut inside = false;
                    for (p, _, hw) in seg.iter() {
                        let d2 = (px - p.x) * (px - p.x) + (py - p.y) * (py - p.y);
                        if d2 <= hw * hw {
                            inside = true;
                            break;
                        }
                    }
                    if !inside {
                        continue;
                    }
                    let i2 = cy * n2 + cx;
                    if height.data[i2] < surface && mask.insert(i2) {
                        cells.push(i2);
                    }
                }
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
                        permanent: is_river || dial >= 0.4,
                    });
                }
            }
            si = sj + 1;
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
