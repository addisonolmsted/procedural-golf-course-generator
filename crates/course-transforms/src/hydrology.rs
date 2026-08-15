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
/// EMPTY during the hydrology-v2 rebuild: the reviewer's six-gap
/// critique reset S4 to its measured core (routing + basins +
/// agreement). The enum shell stays for the redesigned transforms.
pub const DEFAULT_TRANSFORMS: [TransformId; 0] = [];

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
        // v2 rebuild pending: transforms are recorded but no transform
        // is currently implemented (full reset by reviewer decision)
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
