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
/// Floodplain envelope half-width per sqrt(km²) of discharge.
pub const FLOOD_W_PER_SQKM: f64 = 55.0;
/// Height of the flood datum above the local channel bed.
pub const FLOOD_DEPTH_M: f64 = 1.1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TransformId {
    WaterTable,
    Floodplain,
}

/// The default ordered modifier list. Order is data: it is recorded in
/// `applied` and reordering it demonstrably changes the output (a
/// floodplain flattened before the water-table intersection ponds
/// differently than after).
pub const DEFAULT_TRANSFORMS: [TransformId; 2] =
    [TransformId::Floodplain, TransformId::WaterTable];

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaterPlaneOrigin {
    WaterTable,
    Floodplain,
    ClosedBasin,
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
    _identity: &RunIdentity,
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
    for i in 0..n8 * n8 {
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
    let n2 = height.spec.nx as usize;
    let cell2 = height.spec.cell_size;
    // rasterize big-channel bed elevation + envelope width to a coarse
    // helper grid (8 m) then apply at 2 m
    let mut nearest: Vec<(f64, f64, f64)> = vec![(f64::INFINITY, 0.0, 0.0); n2 * n2]; // (d2, bed_z, width)
    for c in &big {
        let w = FLOOD_W_PER_SQKM * (c.area_m2 / 1.0e6).sqrt();
        for seg in c.pts.windows(2) {
            let (a, b) = (seg[0], seg[1]);
            let steps = ((((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt()) / cell2).ceil() as usize;
            for s in 0..=steps {
                let t = s as f64 / steps.max(1) as f64;
                let px = a.x + (b.x - a.x) * t;
                let py = a.y + (b.y - a.y) * t;
                let cx = (px / cell2) as i64;
                let cy = (py / cell2) as i64;
                let bed = {
                    let xi = (cx.clamp(0, n2 as i64 - 1)) as usize;
                    let yi = (cy.clamp(0, n2 as i64 - 1)) as usize;
                    height.data[yi * n2 + xi]
                };
                let rr = (w / cell2).ceil() as i64;
                // coarse stamping stride keeps this O(len·w) not O(len·w²)
                for dy in (-rr..=rr).step_by(3) {
                    for dx in (-rr..=rr).step_by(3) {
                        let (xx, yy) = (cx + dx, cy + dy);
                        if xx < 0 || yy < 0 || xx >= n2 as i64 || yy >= n2 as i64 {
                            continue;
                        }
                        let d2 = ((dx * dx + dy * dy) as f64) * cell2 * cell2;
                        let i = yy as usize * n2 + xx as usize;
                        if d2 < nearest[i].0 {
                            nearest[i] = (d2, bed, w);
                        }
                    }
                }
            }
        }
    }
    for i in 0..n2 * n2 {
        let (d2, bed, w) = nearest[i];
        if !d2.is_finite() {
            continue;
        }
        let d = d2.sqrt();
        if d > w || d < 12.0 {
            continue; // outside envelope, or the incised channel itself
        }
        // cap toward the flood datum, feathered at the envelope edge
        let edge = ((w - d) / (0.35 * w)).clamp(0.0, 1.0);
        let cap = bed + FLOOD_DEPTH_M + (1.0 - edge) * 2.0;
        if height.data[i] > cap {
            height.data[i] = height.data[i] * (1.0 - edge) + cap * edge;
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
            // pond: polygon = convex hull of ponded cells (closed,
            // non-self-intersecting, positive area by construction;
            // marching-squares outline is a later refinement)
            let pts: Vec<Vec2> = comp
                .iter()
                .map(|&c| spec8.world_of((c % n8) as u32, (c / n8) as u32))
                .collect();
            let hull = convex_hull(&pts);
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
    // basin cells: raised by fill (open, would flood) or kept pits (closed)
    let mut label = vec![0u32; n8 * n8];
    let mut basins = Vec::new();
    let mut next = 1u32;
    for start in 0..n8 * n8 {
        let deep = zf.data[start] > z8.data[start] + BASIN_MIN_DEPTH_M || keep8[start];
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
                let ndeep = zf.data[ni] > z8.data[ni] + BASIN_MIN_DEPTH_M || keep8[ni];
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
        let pts: Vec<Vec2> = comp
            .iter()
            .map(|&c| spec8.world_of((c % n8) as u32, (c / n8) as u32))
            .collect();
        let hull = convex_hull(&pts);
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
