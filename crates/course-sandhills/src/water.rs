//! A6 — water. Lakes where deflation pans meet the table; the allogenic river.
//!
//! **No drainage network is ever grown.** The Nebraska Sandhills has no
//! surface drainage — rain infiltrates. Its water is (a) the Ogallala
//! intersecting interdune floors, which is why the real corpus carries a lake
//! fraction of 0.258, and (b) a handful of spring-fed THROUGH rivers (the
//! Dismal, the Middle Loup) that cross the dune field without draining it.
//! Sand Hills GC sits on one. The river is a coin (p 0.30), not a consequence
//! of the terrain.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::world::EXTENT_M;

use crate::draw::Descriptors;

pub struct Water {
    /// Water surface at 2 m; NaN = dry. Lakes are flat planes; the river
    /// surface falls monotonically along its path.
    pub surface: Grid<f64>,
    pub lake_frac: f64,
    pub river: Option<Vec<Vec2>>,
    /// The creek's own water level at each node of `river`: the profile the
    /// carve guarantees descends to the mouth. The raster `surface` can sit
    /// above it where a pond floods the channel.
    pub river_z: Option<Vec<f64>>,
}

/// Lakes: connected pockets where the ground sits below the local water
/// table. The table rides the regional datum (the Ogallala is unconfined
/// here), so each lake takes ONE flat level — the 25th percentile of the
/// table across its own extent — and cells above that level go dry again.
/// What the table drawdown needs: a centre-line, its bed, and how far the
/// influence reaches. Generalised from `&RiverPlan` so the gorge can supply
/// it too — a 50 m canyon floor sits far below the regional table, and
/// without the drawdown the whole valley floods.
pub struct Drawdown<'a> {
    pub line: &'a [Vec2],
    pub bed: &'a [f64],
    pub inner: f64,
    pub reach: f64,
}

impl<'a> Drawdown<'a> {
    pub fn from_plan(pl: &'a RiverPlan) -> Self {
        Drawdown {
            line: &pl.corr,
            bed: &pl.bed,
            inner: pl.floor_hw + pl.wall_m,
            reach: (pl.floor_hw + pl.wall_m) * 1.4 + 220.0,
        }
    }
}

/// Water finds its own outline.
///
/// A body's level and its outline came from two different rules -- a tilted
/// table decided which cells were wet, one flat level was then painted on
/// them; a pond was clipped by an opening or a keep-out zone and kept its
/// level -- so beside the water the ground could stand BELOW the water
/// (measured on the 200-seed mixed run, 2026-09-06: the largest pond's shore
/// had dry cells below the level on 44/45 fluvial tiles, by up to 2.7 m).
///
/// The rule here is the textbook one. `fill_levels` is a priority flood from
/// the sinks (the tile edge, and where told so the creek) inward: every cell
/// gets its SPILL elevation, the lowest level water standing on it can reach
/// a sink at; walls (a dam) are never entered. `settle` then takes a body
/// and the level it wants and lowers the level to the lowest spill elevation
/// the body would reach, so the water can hold it, and wets every connected
/// cell below it. Nothing beside the water is lower than the water, and
/// nothing is flooded that would drain. Ablation: `WATER_SETTLE=off`.
fn settle_on() -> bool {
    std::env::var("WATER_SETTLE").map(|v| v != "off").unwrap_or(true)
}

fn fill_levels(spec: &course_world::grid::GridSpec, ground: &[f64], sink: &[bool],
               wall: &[bool]) -> Vec<f64> {
    use std::cmp::Reverse;
    use std::collections::BinaryHeap;
    let key = |z: f64| -> i64 { (z * 1000.0).round() as i64 };
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut fill = vec![f64::INFINITY; spec.len()];
    let mut done = vec![false; spec.len()];
    let mut heap: BinaryHeap<Reverse<(i64, usize)>> = BinaryHeap::new();
    for y in 0..ny {
        for x in 0..nx {
            let i = spec.index(x as u32, y as u32);
            let edge = x == 0 || y == 0 || x == nx - 1 || y == ny - 1;
            if (edge || sink[i]) && !wall[i] {
                fill[i] = ground[i];
                done[i] = true;
                heap.push(Reverse((key(ground[i]), i)));
            }
        }
    }
    while let Some(Reverse((_, i))) = heap.pop() {
        let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (a, b) = (x + dx, y + dy);
            if a < 0 || b < 0 || a >= nx || b >= ny {
                continue;
            }
            let j = spec.index(a as u32, b as u32);
            if done[j] || wall[j] {
                continue;
            }
            done[j] = true;
            fill[j] = ground[j].max(fill[i]);
            heap.push(Reverse((key(fill[j]), j)));
        }
    }
    fill
}

fn settle(spec: &course_world::grid::GridSpec, ground: &[f64], fill: &[f64],
          seeds: &[usize], level_max: f64, wall: &[bool]) -> (f64, Vec<usize>) {
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut level = level_max;
    let mut cells: Vec<usize> = Vec::new();
    // fixed rounds: each lowers the level to the lowest spill met and
    // re-floods; a closed basin needs one, a leaking one two
    for _ in 0..4 {
        let mut seen = vec![false; spec.len()];
        let mut stack: Vec<usize> = Vec::new();
        cells.clear();
        for &i in seeds {
            if !seen[i] && ground[i] < level && !wall[i] {
                seen[i] = true;
                stack.push(i);
            }
        }
        let mut spill = f64::INFINITY;
        while let Some(i) = stack.pop() {
            cells.push(i);
            spill = spill.min(fill[i]);
            let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let (a, b) = (x + dx, y + dy);
                if a < 0 || b < 0 || a >= nx || b >= ny {
                    continue;
                }
                let j = spec.index(a as u32, b as u32);
                if !seen[j] && !wall[j] && ground[j] < level {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if spill >= level - 1e-6 {
            break;                       // the basin holds this level
        }
        level = spill;
    }
    (level, cells)
}

/// Dam it. Most Carolina table ponds sit on ground that is not a closed
/// basin at 2 m -- settled, five of six on the mixed run drained to nothing,
/// which is not what the corpus shows (1.7 % water inside a course, ponds
/// dug and dammed). So a body that cannot hold its level as found gets a
/// RIM: the dry ring beside the water is raised to the level plus a lip,
/// and the raise tapers outward at 50 % so it reads as an earth bank, not a
/// wall. Fixed four rounds; the pond then holds by construction.
/// `keep`: cells never raised -- the creek's ribbon and banks, so a bank
/// beside running water is never dammed across it.
fn berm(spec: &course_world::grid::GridSpec, ground: &mut [f64], wet: &[bool], level: f64,
        keep: &[bool]) {
    const LIP: f64 = 0.15;
    const FALL: f64 = 0.6;           // per 2 m cell: a 30 % bank
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let nb8 = [(1i64, 0i64), (-1, 0), (0, 1), (0, -1), (1, 1), (1, -1), (-1, 1), (-1, -1)];
    let mut mark = vec![false; spec.len()];
    let mut front: Vec<usize> = Vec::new();
    for i in 0..spec.len() {
        if !wet[i] {
            continue;
        }
        let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
        for (dx, dy) in nb8 {
            let (a, b) = (x + dx, y + dy);
            if a < 0 || b < 0 || a >= nx || b >= ny {
                continue;
            }
            let j = spec.index(a as u32, b as u32);
            if !wet[j] && !mark[j] && !keep[j] && ground[j] < level + LIP {
                ground[j] = level + LIP;
                mark[j] = true;
                front.push(j);
            }
        }
    }
    for _ in 0..4 {
        let mut next: Vec<usize> = Vec::new();
        for &i in &front {
            let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
            for (dx, dy) in nb8 {
                let (a, b) = (x + dx, y + dy);
                if a < 0 || b < 0 || a >= nx || b >= ny {
                    continue;
                }
                let j = spec.index(a as u32, b as u32);
                let want = ground[i] - FALL;
                if !wet[j] && !mark[j] && !keep[j] && ground[j] < want {
                    ground[j] = want;
                    mark[j] = true;
                    next.push(j);
                }
            }
        }
        front = next;
    }
}

/// The creek's ribbon grown by three cells: its banks, which no berm may raise.
fn creek_banks(spec: &course_world::grid::GridSpec, creek: &[bool]) -> Vec<bool> {
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut out = creek.to_vec();
    for _ in 0..3 {
        let src = out.clone();
        for y in 0..ny {
            for x in 0..nx {
                let i = spec.index(x as u32, y as u32);
                if src[i] {
                    continue;
                }
                for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = (x + dx, y + dy);
                    if a >= 0 && b >= 0 && a < nx && b < ny && src[spec.index(a as u32, b as u32)] {
                        out[i] = true;
                        break;
                    }
                }
            }
        }
    }
    out
}

pub fn find(height: &mut Grid<f64>, datum8: &Grid<f64>, d: &Descriptors,
            river: Option<&Drawdown>, blowouts: &[crate::blowout::Blowout]) -> Water {
    let spec = height.spec;
    let mut surface = Grid::filled(spec, f64::NAN);
    let mut wet = vec![false; spec.len()];
    let mut table = vec![0.0f64; spec.len()];
    // The record defines water_table_m as depth below the INTERDUNE FLOOR,
    // and the floors sit above the datum -- referencing the datum quietly
    // deepened every table by the floor offset. Measure the offset once, as
    // the p8 of height-above-datum (the floors are the low tail).
    // Shoreline smoothing: threshold a SMOOTHED margin, not the raw one.
    // At 2 m the raw height-vs-table comparison follows every texture grain
    // and the shoreline comes out sawtoothed; smoothing the margin (not the
    // ground) rounds the shore without moving any terrain.
    let mut above: Vec<f64> = (0..spec.len())
        .step_by(7)
        .map(|i| {
            let (x, y) = ((i % spec.nx as usize) as u32, (i / spec.nx as usize) as u32);
            height.data[i] - datum8.bilinear(spec.world_of(x, y))
        })
        .collect();
    above.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let floor_off = above[above.len() / 12];

    // --- THE TABLE FOLLOWS THE LAND, IT IS NOT ONE PLANE (2026-08-29) ------
    // `floor_off` above is a SINGLE GLOBAL SCALAR and `datum8` is a plane, so
    // the mapped table was one tilted plane across 3 km. The interdune floors
    // do NOT all sit at the same height above that plane -- the megaform has
    // structure at its own 0.9-1.2 km wavelength -- so a single plane
    // intersects only whichever floor happens to be globally lowest. With
    // ~3 valleys per tile that is winner-take-all, and the winner is rarely
    // in the middle: MEASURED, only 2.2% of aeolian pond cells fell in the
    // 1.5 km core, which is 25% of the tile area -- an 11x deficit, and the
    // reviewer saw it as "ponds always at the edge, out of routing's reach".
    //
    // A real water table is not a plane over 3 km; it is a SUBDUED REPLICA of
    // the land surface, following regional topography with damped amplitude.
    // So the floor offset becomes a broad local field: the same low-percentile
    // statistic, measured in a ~700 m neighbourhood, then damped back toward
    // the global value so the table stays flatter than the ground (a table
    // that copied the surface exactly would flood every valley equally, which
    // is the opposite error).
    const FLOOR_R_M: f64 = 700.0;
    const FLOOR_DAMP: f64 = 1.00;   // fully local; the area fix below sets the level
    let dspec0 = datum8.spec;
    let mut above8 = vec![0.0f64; dspec0.len()];
    for y in 0..dspec0.ny {
        for x in 0..dspec0.nx {
            let p = dspec0.world_of(x, y);
            let hx = ((p.x / spec.cell_size).round() as i64)
                .clamp(0, spec.nx as i64 - 1) as u32;
            let hy = ((p.y / spec.cell_size).round() as i64)
                .clamp(0, spec.ny as i64 - 1) as u32;
            above8[dspec0.index(x, y)] =
                *height.get(hx, hy) - *datum8.get(x, y);
        }
    }
    let rr = (FLOOR_R_M / dspec0.cell_size).round() as i64;
    let mut floor_f = Grid::filled(dspec0, floor_off);
    let mut buf: Vec<f64> = Vec::new();
    for y in 0..dspec0.ny {
        for x in 0..dspec0.nx {
            buf.clear();
            let (cx, cy) = (x as i64, y as i64);
            // MIRROR at the boundary, never clip. A clipped neighbourhood is
            // a smaller sample, so the local floor estimate is noisier at the
            // tile edge and |ground - floor| reaches its extremes there --
            // which would rebuild, in the fix, exactly the edge bias the fix
            // exists to remove. Reflection gives every node a full-size
            // window. (The same trap bit the analysis that found this bug:
            // a mode="nearest" smooth put 97% of minima at the edge.)
            let refl = |v: i64, n: i64| -> u32 {
                let mut t = v;
                while t < 0 || t >= n {
                    if t < 0 { t = -t; }
                    if t >= n { t = 2 * (n - 1) - t; }
                }
                t as u32
            };
            let mut gy = cy - rr;
            while gy <= cy + rr {
                let mut gx = cx - rr;
                while gx <= cx + rr {
                    let dd = (((gx - cx).pow(2) + (gy - cy).pow(2)) as f64).sqrt()
                        * dspec0.cell_size;
                    if dd <= FLOOR_R_M {
                        let ry = refl(gy, dspec0.ny as i64);
                        let rx = refl(gx, dspec0.nx as i64);
                        buf.push(above8[dspec0.index(rx, ry)]);
                    }
                    gx += 3;
                }
                gy += 3;
            }
            if buf.len() < 8 {
                continue;
            }
            buf.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let local = buf[buf.len() / 12];
            floor_f.set(x, y, floor_off + FLOOR_DAMP * (local - floor_off));
        }
    }
    // RIVER DRAWDOWN. A river drains its corridor: the local water table is
    // depressed toward the river's own bed, easing back to the regional
    // table over a few hundred metres. Without this, shallow-table seeds
    // flooded the corridor floor with lakes sitting directly on the river
    // (review, 2026-08-23). Computed at the datum grid's 8 m and sampled
    // bilinearly, like the table itself.
    let dspec = datum8.spec;
    let mut ddown = Grid::filled(dspec, 0.0f64);
    if let Some(pl) = river {
        let reach = pl.reach;
        for (i, q) in pl.line.iter().enumerate().step_by(2) {
            let bed_t = pl.bed[i] + 0.3;
            let x0 = ((q.x - reach) / dspec.cell_size).floor().max(0.0) as u32;
            let x1 = ((q.x + reach) / dspec.cell_size).ceil().min(dspec.nx as f64 - 1.0) as u32;
            let y0 = ((q.y - reach) / dspec.cell_size).floor().max(0.0) as u32;
            let y1 = ((q.y + reach) / dspec.cell_size).ceil().min(dspec.ny as f64 - 1.0) as u32;
            for yy in y0..=y1 {
                for xx in x0..=x1 {
                    let w = dspec.world_of(xx, yy);
                    let dd = ((w.x - q.x).powi(2) + (w.y - q.y).powi(2)).sqrt();
                    if dd >= reach {
                        continue;
                    }
                    let p2 = dspec.world_of(xx, yy);
                    let tab = datum8.bilinear(p2) + floor_f.bilinear(p2)
                        - d.water_table_m;
                    let full = (tab - bed_t).max(0.0);
                    let f = 1.0 - math::smoothstep(pl.inner, reach, dd);
                    let li = dspec.index(xx, yy);
                    if full * f > ddown.data[li] {
                        ddown.data[li] = full * f;
                    }
                }
            }
        }
    }

    let mut margin = vec![0.0f64; spec.len()];
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let li = spec.index(x, y);
            let p = spec.world_of(x, y);
            table[li] = datum8.bilinear(p) + floor_f.bilinear(p)
                - d.water_table_m - ddown.bilinear(p);
            margin[li] = height.data[li] - table[li];
        }
    }
    // AREA-PRESERVING LEVEL. Referencing the table to the LOCAL floor also
    // raises the bar everywhere -- the ground must now dip below its own
    // neighbourhood rather than below the whole tile -- so the same
    // water_table_m draw wets less ground (measured: tiles with any water
    // 16/30 -> 12/30). The draw is a calibrated dial and its meaning must not
    // shift under a change that was only ever about WHERE the water sits, so
    // the table is offset by one global constant chosen to reproduce the wet
    // AREA the single-plane rule would have produced. Location redistributes;
    // amount is held fixed.
    // NOT on river tiles: there the drawdown already sets the corridor level,
    // and re-levelling to match a global area target raises the table into the
    // creek -- caught by aeolian_mode_still_grows_no_channels, which measured
    // the wet width blowing out to 44 m against a ~6 m creek spec.
    if river.is_none() {
        let mut old_margin: Vec<f64> = Vec::with_capacity(spec.len());
        for y in 0..spec.ny {
            for x in 0..spec.nx {
                let li = spec.index(x, y);
                let p = spec.world_of(x, y);
                old_margin.push(height.data[li]
                    - (datum8.bilinear(p) + floor_off - d.water_table_m
                       - ddown.bilinear(p)));
            }
        }
        let want = old_margin.iter().filter(|m| **m < 0.0).count();
        if want > 0 {
            let mut sorted = margin.clone();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let delta = sorted[(want - 1).min(sorted.len() - 1)];
            if delta > 0.0 {
                for m in margin.iter_mut() {
                    *m -= delta;
                }
                for t in table.iter_mut() {
                    *t += delta;
                }
            }
        }
    }
    // three 1-2-1 passes ~ a 6 m Gaussian on the margin field
    let mut tmp = margin.clone();
    for _ in 0..3 {
        for y in 1..spec.ny - 1 {
            for x in 1..spec.nx - 1 {
                let i = spec.index(x, y);
                tmp[i] = (margin[i] * 4.0
                    + margin[spec.index(x + 1, y)] + margin[spec.index(x - 1, y)]
                    + margin[spec.index(x, y + 1)] + margin[spec.index(x, y - 1)]) / 8.0;
            }
        }
        std::mem::swap(&mut margin, &mut tmp);
    }
    for i in 0..spec.len() {
        wet[i] = margin[i] < 0.0;
    }
    // Blowouts are exempt from filling (review): a deflation bowl is dry by
    // construction -- the wind cut it, and deflation stops at the capillary
    // fringe, so its floor sits above saturation even where the cut dips
    // below the mapped table.
    // blowout bowls are walls to standing water (dry by construction)
    let mut wall = vec![false; spec.len()];
    for b in blowouts {
        let r = b.radius_m * 1.45 + 6.0;
        let x0 = ((b.center.x - r) / spec.cell_size).floor().max(0.0) as u32;
        let x1 = ((b.center.x + r) / spec.cell_size).ceil().min(spec.nx as f64 - 1.0) as u32;
        let y0 = ((b.center.y - r) / spec.cell_size).floor().max(0.0) as u32;
        let y1 = ((b.center.y + r) / spec.cell_size).ceil().min(spec.ny as f64 - 1.0) as u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let p = spec.world_of(x, y);
                if ((p.x - b.center.x).powi(2) + (p.y - b.center.y).powi(2)).sqrt() < r {
                    wet[spec.index(x, y)] = false;
                    wall[spec.index(x, y)] = true;
                }
            }
        }
    }
    let fill = if settle_on() {
        fill_levels(&spec, &height.data, &vec![false; spec.len()], &wall)
    } else {
        Vec::new()
    };
    // connected components, 4-neighbour flood fill
    let mut comp = vec![0u32; spec.len()];
    let mut next = 0u32;
    let mut stack = Vec::new();
    let mut lake_cells = 0usize;
    for start in 0..spec.len() {
        if !wet[start] || comp[start] != 0 {
            continue;
        }
        next += 1;
        stack.push(start);
        comp[start] = next;
        let mut cells = vec![start];
        while let Some(i) = stack.pop() {
            let (x, y) = ((i % spec.nx as usize) as u32, (i / spec.nx as usize) as u32);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                if nx < 0 || ny < 0 || nx >= spec.nx as i64 || ny >= spec.ny as i64 {
                    continue;
                }
                let j = spec.index(nx as u32, ny as u32);
                if wet[j] && comp[j] == 0 {
                    comp[j] = next;
                    stack.push(j);
                    cells.push(j);
                }
            }
        }
        // MEASURED CULL (2026-08-23): bodies under 500 m2 were 29% of all
        // water bodies and 0.3% of the water area, at a median 0.18 m deep --
        // ankle-deep specks that flicker at render distance and complicate
        // every routing mask, worth nothing as hazards. The 500-2500 m2 class
        // stays: 0.62 m deep, the pond-by-the-green vocabulary. (Real ground
        // does carry sub-500 m2 dead-flat speckle, but that is WET MEADOW,
        // which is a texture question, not open water.)
        if cells.len() < 125 {
            continue; // < 500 m2 at 2 m
        }
        // one FLAT level per lake
        let mut lv: Vec<f64> = cells.iter().map(|i| table[*i]).collect();
        lv.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mut level = lv[lv.len() / 4];
        // depth gate: a pond whose DEEPEST point is under 0.3 m is a sheen,
        // not a water body.
        let deepest = cells.iter()
            .map(|i| level - height.data[*i])
            .fold(f64::MIN, f64::max);
        if deepest < 0.3 {
            continue;
        }
        // Gate on the FINAL footprint: setting one flat level per lake
        // shrinks the component (cells above the level go dry again), and the
        // first version gated before the shrink -- 28 m2 fragments survived.
        let final_cells: Vec<usize> = if settle_on() {
            // the level the basin holds, and every cell below it (see
            // `settle`); a basin that would lose half the body is dammed
            // at the level instead (see `berm`)
            let seeds: Vec<usize> =
                cells.iter().copied().filter(|i| height.data[*i] < level).collect();
            let (lv2, c2) = settle(&spec, &height.data, &fill, &seeds, level, &wall);
            if c2.len() * 2 >= seeds.len() {
                level = lv2;
                c2
            } else {
                let mut w = vec![false; spec.len()];
                for &i in &seeds {
                    w[i] = true;
                }
                berm(&spec, &mut height.data, &w, level, &vec![false; spec.len()]);
                seeds
            }
        } else {
            cells.into_iter().filter(|i| height.data[*i] < level).collect()
        };
        if final_cells.len() < 125 {
            continue;
        }
        for i in final_cells {
            surface.data[i] = level;
            lake_cells += 1;
        }
    }
    // FINAL sweep: the level cut can SPLIT a component -- cells above the
    // level in its middle go dry and disconnect it -- so gating per source
    // component still let 28 m2 slivers through. Relabel the finished wet
    // grid and drop small pieces wholesale.
    let mut final_lab = vec![0u32; spec.len()];
    let mut nid = 0u32;
    let mut counts: Vec<usize> = vec![0];
    for start in 0..spec.len() {
        if !surface.data[start].is_finite() || final_lab[start] != 0 {
            continue;
        }
        nid += 1;
        counts.push(0);
        stack.push(start);
        final_lab[start] = nid;
        while let Some(i) = stack.pop() {
            counts[nid as usize] += 1;
            let (x, y) = ((i % spec.nx as usize) as u32, (i / spec.nx as usize) as u32);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let (nx2, ny2) = (x as i64 + dx, y as i64 + dy);
                if nx2 < 0 || ny2 < 0 || nx2 >= spec.nx as i64 || ny2 >= spec.ny as i64 {
                    continue;
                }
                let j = spec.index(nx2 as u32, ny2 as u32);
                if surface.data[j].is_finite() && final_lab[j] == 0 {
                    final_lab[j] = nid;
                    stack.push(j);
                }
            }
        }
    }
    for i in 0..spec.len() {
        if final_lab[i] != 0 && counts[final_lab[i] as usize] < 125 {
            surface.data[i] = f64::NAN;
            lake_cells -= 1;
        }
    }
    // At most this many lake bodies survive on a tile, and never two whose
    // banks come within MIN_LAKE_GAP_M. Largest are kept -- a tile reads by
    // its biggest water. Raised 3 -> 8 with the spacing rule (review
    // 2026-08-28): a flat count culled well-separated lakes that were doing
    // no harm, while the thing worth removing was always clustered speckle.
    const MAX_LAKES: usize = 8;
    const MIN_LAKE_GAP_M: f64 = 50.0;

    // SMALL-POND CAP (review, 2026-08-23): small ponds (< 2500 m2) may be at
    // most 2/3 of the body count. Measured tiles already sit at 9-24%, so
    // this is an invariant against a bad draw rather than a correction --
    // a tile whose water reads as pond-speckle rather than as lakes with
    // pond accents would be wrong even if every pond passed its own gates.
    // Cull order is smallest first: the least feature-worthy go first.
    let small_limit = (2500.0 / (spec.cell_size * spec.cell_size)) as usize;
    let mut small: Vec<(usize, u32)> = (1..=nid as usize)
        .filter(|k| counts[*k] >= 125 && counts[*k] < small_limit)
        .map(|k| (counts[k], k as u32))
        .collect();
    let total = (1..=nid as usize).filter(|k| counts[*k] >= 125).count();
    // Culling small ponds also shrinks `total`, so capping them at 2/3 of the
    // count BEFORE the cull never actually reaches 2/3 after it — the ratio
    // chases itself down. Solve it directly instead: we want
    // small <= 2/3 (small + big), which is small <= 2 * big. Latent since the
    // cap was written; doubling the dune relief exposed it by multiplying the
    // small hollows (seed 9 measured 271 of 274 bodies small).
    let big = total.saturating_sub(small.len());
    let keep_max = 2 * big;
    if small.len() > keep_max {
        small.sort(); // smallest first
        let drop: std::collections::HashSet<u32> =
            small[..small.len() - keep_max].iter().map(|(_, k)| *k).collect();
        for i in 0..spec.len() {
            if final_lab[i] != 0 && drop.contains(&final_lab[i]) {
                surface.data[i] = f64::NAN;
                lake_cells -= 1;
            }
        }
    }
    // HARD COUNT CAP (review, 2026-08-28): at most MAX_LAKES bodies per tile,
    // and the ones kept are the LARGEST. The small-pond ratio above bounds the
    // pond/lake MIX but not the absolute number, so a wet draw could still
    // speckle a tile with a dozen bodies that each passed their own gates.
    // This runs after that cull so the ratio rule decides which ponds survive
    // to compete on size, and it is a no-op on the great majority of tiles.
    let mut live_counts = vec![0usize; nid as usize + 1];
    for i in 0..spec.len() {
        if surface.data[i].is_finite() && final_lab[i] != 0 {
            live_counts[final_lab[i] as usize] += 1;
        }
    }
    let mut live: Vec<(usize, u32)> = (1..=nid as usize)
        .filter(|k| live_counts[*k] > 0)
        .map(|k| (live_counts[k], k as u32))
        .collect();
    {
        // Only cells still wet carry a live label into the selection.
        let mut live_lab = vec![0u32; spec.len()];
        for i in 0..spec.len() {
            if surface.data[i].is_finite() {
                live_lab[i] = final_lab[i];
            }
        }
        let drop = pick_spaced(&spec, &live_lab, &mut live, MAX_LAKES, MIN_LAKE_GAP_M);
        for i in 0..spec.len() {
            if live_lab[i] != 0 && drop.contains(&live_lab[i]) {
                surface.data[i] = f64::NAN;
                lake_cells -= 1;
            }
        }
    }

    let lake_frac = lake_cells as f64 / spec.len() as f64;
    Water { surface, lake_frac, river: None, river_z: None }
}

/// The allogenic river, redesigned to review spec (2026-08-23): "quite
/// narrow, maybe a max of 20 feet across, and wander/switchback but with a
/// finer and more organic cut."
///
/// Three consequences. (1) The water is ~6 m wide — a 3-cell ribbon at 2 m —
/// so the cut is a CREEK SLOT, not a valley: ~1.2 m deep, shoulders easing
/// over a few metres, no broad regrade. (2) The planform is built by a
/// HEADING INTEGRATOR, not offsets from a chord: heading = progress direction
/// + swing·sin(2π·arc/λ) with drawn harmonics, which is what makes true
/// switchbacks possible (an offset line cannot fold past its chord — attempt
/// 4 learned this on its serpentine round). (3) Organic wander comes from
/// low-frequency heading noise, so no two bends repeat.
/// Planform style — pulled out so review variants are the SAME code at
/// different numbers, and the chosen one wires in by changing defaults.
#[derive(Clone, Copy, Debug)]
pub struct RiverStyle {
    pub lam: (f64, f64),
    pub swing: (f64, f64),
    /// low-frequency heading wander amplitude, radians
    pub wander: f64,
}

impl RiverStyle {
    /// The five review-passed planforms (2026-08-23, 10/10 pass). One is
    /// drawn per river, equal weights — variety across the passed set rather
    /// than one blended middle. Reviewer bound: λ never below C's 220 m
    /// ("C for seed 5 should be the tightest bound for lambda").
    pub const PASSED: [RiverStyle; 5] = [
        RiverStyle { lam: (420.0, 560.0), swing: (0.70, 0.95), wander: 0.55 }, // A lazy
        RiverStyle { lam: (300.0, 420.0), swing: (0.95, 1.20), wander: 0.65 }, // B classic
        // C's floor is 283, measured: that is the lambda the reviewer's
        // approved C-on-seed-5 actually ran at, and a fresh seed drawing 254
        // from the old (220,340) failed review for exactly "lambda too
        // small". The bound is the approved draw, not the old range edge.
        RiverStyle { lam: (283.0, 340.0), swing: (1.20, 1.70), wander: 0.80 }, // C loops
        RiverStyle { lam: (260.0, 480.0), swing: (0.75, 1.45), wander: 0.90 }, // D mixed
        RiverStyle { lam: (500.0, 700.0), swing: (0.40, 0.60), wander: 1.10 }, // E wander
    ];
}

impl Default for RiverStyle {
    fn default() -> Self {
        RiverStyle::PASSED[2]
    }
}

/// Everything the two carve passes need, planned once so the corridor can be
/// cut into the 8 m macro BEFORE texture (it is a landform and belongs under
/// the quilt — review 2026-08-23: the corridor floor was "much too smooth")
/// while the 6 m channel slot stays a sharp post-texture feature.
pub struct RiverPlan {
    pub pts: Vec<Vec2>,
    pub corr: Vec<Vec2>,
    pub bed: Vec<f64>,
    pub floor_hw: f64,
    pub wall_m: f64,
    pub wall_rise: f64,
    /// Per-side asymmetry: the cut-bank side is steeper and narrower. Drawn
    /// once per river; which side is "cut" flips slowly along the arc.
    pub asym: f64,
    pub s_wl: u32,
    pub s_wr: u32,
    pub s_flip: u32,
    pub s_shelf: u32,
    /// catena-noise and wall-rise-variation seeds — the level lines of the
    /// valley transitions read as artificial without them (review).
    pub s_cat: u32,
    pub s_wr2: u32,
    /// Shelf (terrace) height above the floor, metres; present in stretches.
    pub shelf_h: f64,
    /// The INCISED variant (~12% of rivers): a deep valley sharply cut into
    /// the dunelands — the Sand Hills CC setting, where the Middle Loup runs
    /// well below the dune surface. Same machinery, gorge numbers.
    pub incised: bool,
}

pub fn river(rng: &mut DetRng, height: &mut Grid<f64>, water: &mut Water, d: &Descriptors) {
    let pick = RiverStyle::PASSED[rng.below(RiverStyle::PASSED.len())];
    river_styled(rng, height, water, d, pick, false)
}

pub fn river_styled(rng: &mut DetRng, height: &mut Grid<f64>, water: &mut Water,
                    d: &Descriptors, style: RiverStyle, force: bool) {
    if !d.allogenic_river && !force {
        return;
    }
    if let Some(plan) = plan_river(rng, height, d, style) {
        carve_corridor(height, &plan);
        cut_channel(height, water, &plan);
    }
}

pub fn plan_river(rng: &mut DetRng, height: &Grid<f64>, d: &Descriptors, style: RiverStyle)
    -> Option<RiverPlan> {
    let spec = height.spec;
    let step = 4.0;
    // The river runs WITH the dune trains, not across them (review: seeds 76
    // and 86 crossed the belts perpendicular). Physically, water follows the
    // interdune corridors, which run along the crest axis = wind + 90 deg.
    // The old coin is still flipped so every later draw keeps its value.
    let _legacy_coin = rng.next_f64();
    let crest_axis = d.wind_rad + std::f64::consts::FRAC_PI_2;
    let along = math::cos(crest_axis).abs() >= math::sin(crest_axis).abs();
    let t0 = rng.range_f64(0.25, 0.75);
    let (start, base_heading) = if along {
        (Vec2::new(2.0, t0 * EXTENT_M), 0.0)
    } else {
        (Vec2::new(t0 * EXTENT_M, 2.0), std::f64::consts::FRAC_PI_2)
    };
    // Meander scale, corrected against the confirmed corridor tiles: the
    // heading integrator's lateral amplitude is ~swing*lam/2pi, and at
    // lam 90-180 the channel swung only ~20 m inside a 100-300 m corridor --
    // a timid wiggle down the middle of a smooth band, where the real Middle
    // Loup loops across most of its meadow.
    let lam = rng.range_f64(style.lam.0, style.lam.1);
    let swing = rng.range_f64(style.swing.0, style.swing.1);
    let (ph1, ph2) = (rng.range_f64(0.0, std::f64::consts::TAU),
                      rng.range_f64(0.0, std::f64::consts::TAU));
    let s_noise = rng.next_u32();
    let s_lam = rng.next_u32();
    let s_swing = rng.next_u32();
    let lim = EXTENT_M - 2.0;

    // Build the path inside an ACCEPTANCE LADDER: a candidate that crosses
    // or kisses itself is rejected and rebuilt with the swing and wander
    // damped 13% per attempt. Review failures that forced this: seed 9 drew
    // figure-eight crossovers (the deviation clamp bounds the HEADING, but a
    // +110 deg lobe into a -110 deg lobe can still cross in PLAN), and seed
    // 18 bent in a square against the border because position was CLAMPED to
    // a line -- heading wobble against a hard wall integrates into square
    // slides. Borders now repel the heading smoothly instead.
    //
    // Attempt 1 draws exactly what the unladdered version drew, so every
    // seed that passed review is byte-identical; retries draw fresh phases.
    let mut pts: Vec<Vec2> = Vec::new();
    'attempt: for attempt in 0..6 {
        let damp = 0.87f64.powi(attempt);
        let (ph1a, ph2a, s_na, s_la, s_sa) = if attempt == 0 {
            (ph1, ph2, s_noise, s_lam, s_swing)
        } else {
            (rng.range_f64(0.0, std::f64::consts::TAU),
             rng.range_f64(0.0, std::f64::consts::TAU),
             rng.next_u32(), rng.next_u32(), rng.next_u32())
        };
        let mut cand: Vec<Vec2> = vec![start];
        let mut p = start;
        let mut arc = 0.0;
        use std::collections::HashMap;
        let cellsz = 16.0;
        let mut hash: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        hash.entry(((start.x / cellsz) as i32, (start.y / cellsz) as i32))
            .or_default().push(0);
        for _ in 0..6000 {
            let lam_e = lam * (1.0 + 0.45 * course_world::noise::perlin1(arc / 640.0, s_la));
            let sw_e = swing * damp
                * (0.45 + 0.75 * (0.5 + 0.5 * course_world::noise::perlin1(arc / 480.0, s_sa)));
            let wob = sw_e
                * (math::sin(std::f64::consts::TAU * arc / lam_e + ph1a)
                    + 0.35 * math::sin(std::f64::consts::TAU * arc / (lam_e * 2.7) + ph2a));
            let wander = style.wander * damp
                * course_world::noise::perlin1(arc / 900.0, s_na);
            let hd = base_heading + wob + wander;
            let lp = Vec2::new(p.x - 60.0 * math::sin(hd), p.y + 60.0 * math::cos(hd));
            let rp2 = Vec2::new(p.x + 60.0 * math::sin(hd), p.y - 60.0 * math::cos(hd));
            let (zl, zr) = (height.bilinear(lp), height.bilinear(rp2));
            let lean = 0.30 * ((zr - zl) / 6.0).clamp(-1.0, 1.0);
            let cross = if along { p.y } else { p.x };
            let repel = 0.55
                * (math::smoothstep(260.0, 40.0, cross)
                    - math::smoothstep(lim - 260.0, lim - 40.0, cross));
            let dev = (hd - base_heading + lean + repel).clamp(-1.95, 1.95);
            let h = base_heading + dev;
            p = Vec2::new(p.x + step * math::cos(h), p.y + step * math::sin(h));
            if along {
                p.y = p.y.clamp(8.0, lim - 8.0);
                if p.x >= lim { break; }
                p.x = p.x.max(2.0);
            } else {
                p.x = p.x.clamp(8.0, lim - 8.0);
                if p.y >= lim { break; }
                p.y = p.y.max(2.0);
            }
            arc += step;
            let (cx, cy) = ((p.x / cellsz) as i32, (p.y / cellsz) as i32);
            let idx = cand.len();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(v) = hash.get(&(cx + dx, cy + dy)) {
                        for j in v {
                            if (idx - j) as f64 * step > 120.0 {
                                let q = cand[*j];
                                if ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt() < 12.0 {
                                    continue 'attempt;
                                }
                            }
                        }
                    }
                }
            }
            hash.entry((cx, cy)).or_default().push(idx);
            cand.push(p);
        }
        pts = cand;
        break;
    }
    if pts.len() < 40 {
        return None;
    }

    // THE CATENA, from literature and geomorphic principles (review
    // 2026-08-23: the measured-tile version dissected too much terrain; keep
    // the effect narrow). Structure keeps the two-line design — channel on
    // the switchback path, corridor on its lowpass — with sizes from first
    // principles rather than from the messy corridor tiles:
    //
    //   * meander-belt width ≈ 10–14 channel widths (Leopold & Wolman) →
    //     the flat floor spans the belt: 36–76 m full at a 6 m channel;
    //   * a spring-fed sand-bed stream runs at near-constant discharge, so
    //     one channel size and a shallow ~1.2 m section;
    //   * valley sides in loose sand are repose-limited: the wall climbs its
    //     2–3.5 m over 60–110 m (3–5% grades), then the ~1.5% outer grade
    //     hands off to the dunes.
    //
    // Full visible corridor ≈ 160–300 m against the old 300–640.
    let hw_water = 3.0;                            // ~6 m wet width (review-confirmed)
    let hw_cut = 7.0;                              // channel slot + shoulder
    let depth = 1.2;
    let floor_hw = rng.range_f64(10.0, 22.0);      // review: full effect ~200 m
    let wall_m = rng.range_f64(35.0, 75.0);        // review: full effect ~200 m
    // MEASURED 2026-08-26: our river valleys sit 17-20 m below their rim
    // with 5-8% walls, where the Dismal River is 54 m / 16.7% and Sand Hills
    // GC 50 m / 10.4%. Raising these was TRIED and changed almost nothing
    // (13-21 m, unchanged) for a reason worth recording: our tiles carry
    // 28-34 m of total relief against 71-105 m for real Sandhills course
    // sites, and a valley cannot be deeper than its landscape is tall. The
    // corridor is not the binding constraint -- macro dune relief is. Left
    // at the reviewed values until that is fixed upstream.
    let wall_rise = rng.range_f64(2.0, 3.5);       // lit: low dune-country relief
    let mut bed: Vec<f64> = pts.iter().map(|q| height.bilinear(*q) - depth).collect();
    // two smoothing passes so the bed does not chase every dune it crosses
    for _ in 0..2 {
        for i in 1..bed.len() - 1 {
            bed[i] = (bed[i - 1] + bed[i] * 2.0 + bed[i + 1]) / 4.0;
        }
    }
    for i in 1..bed.len() {
        bed[i] = bed[i].min(bed[i - 1] - 0.004);
    }
    let s_wl = rng.next_u32();
    let s_wr = rng.next_u32();
    let s_flip = rng.next_u32();
    let s_shelf = rng.next_u32();
    let s_cat = rng.next_u32();
    let s_wr2 = rng.next_u32();
    let s_grade = rng.next_u32();
    let asym = rng.range_f64(0.35, 0.65);
    let shelf_h = rng.range_f64(0.6, 1.4);         // review: steps close to the stream
    // Incised coin LAST among the shared draws, so non-incised seeds keep
    // byte-identical rivers; only a gorge seed consumes the override draws.
    let incised = rng.next_f64() < 0.12;
    let (inc_depth, floor_hw, wall_m, wall_rise) = if incised {
        // the gorge: bed well below the dune surface, walls climbing hard --
        // deep, narrow, sharp. The Sand Hills CC setting.
        (rng.range_f64(4.5, 8.0), rng.range_f64(7.0, 14.0),
         rng.range_f64(30.0, 55.0), rng.range_f64(6.0, 11.0))
    } else {
        (depth, floor_hw, wall_m, wall_rise)
    };
    // BUG, found 2026-08-26: the gorge branch recomputed `depth`, but `bed`
    // was already built above from the 1.2 m default and nothing re-read it,
    // so the override was dead — the compiler had been reporting `depth`
    // unused all along. Every "incised" seed therefore carried a 1.2 m bed
    // with gorge WALLS, which is why measured valley depth came out at
    // 17-20 m against 50-54 m for the real Dismal River and Sand Hills, and
    // why the rim draws had nothing to cut into. The bed is lowered here,
    // after grading, so the long profile it just built is preserved.
    if incised {
        let drop = inc_depth - depth;
        for b in bed.iter_mut() {
            *b -= drop;
        }
    }

    // In a GORGE the corridor is narrow enough that ordinary meander
    // amplitude swings the channel outside its own walls (review: incised
    // seed 94, "river meanders outside of the valley"). Incised channels are
    // soft-pulled toward the corridor line, capped near the floor width --
    // a smooth tanh clamp, so bends compress rather than kink. Applied ONLY
    // to incised plans; regular rivers are untouched.
    if incised {
        let win0 = 40usize;
        let corr0: Vec<Vec2> = (0..pts.len())
            .map(|i| {
                let a = i.saturating_sub(win0);
                let b = (i + win0).min(pts.len() - 1);
                let mut sx = 0.0;
                let mut sy = 0.0;
                for j in a..=b {
                    sx += pts[j].x;
                    sy += pts[j].y;
                }
                let n = (b - a + 1) as f64;
                Vec2::new(sx / n, sy / n)
            })
            .collect();
        let maxoff = floor_hw + 5.0;
        for i in 0..pts.len() {
            let ox = pts[i].x - corr0[i].x;
            let oy = pts[i].y - corr0[i].y;
            let dist = (ox * ox + oy * oy).sqrt();
            if dist > 1e-6 {
                let e2 = math::exp(2.0 * dist / maxoff);
                let tanh = (e2 - 1.0) / (e2 + 1.0);
                let sc = maxoff * tanh / dist;
                pts[i] = Vec2::new(corr0[i].x + ox * sc, corr0[i].y + oy * sc);
            }
        }
    }

    // --- the meadow corridor, on the LOWPASS line --------------------------
    let win = 40usize; // ~160 m of path
    let corr: Vec<Vec2> = (0..pts.len())
        .map(|i| {
            let a = i.saturating_sub(win);
            let b = (i + win).min(pts.len() - 1);
            let mut sx = 0.0;
            let mut sy = 0.0;
            for j in a..=b {
                sx += pts[j].x;
                sy += pts[j].y;
            }
            let n = (b - a + 1) as f64;
            Vec2::new(sx / n, sy / n)
        })
        .collect();
    // Past the wall the profile keeps rising at the measured outer grade
    // (3.2 m @ 200 -> 4.2 m @ 290 = ~1.1%) until it MEETS the dune ground.
    // The first version capped the target instead, and the cap printed a
    // sheer scarp wherever the corridor crossed a tall belt.
    const OUT_GRADE: f64 = 0.015;
    let reach = floor_hw + wall_m + 180.0;
    return Some(RiverPlan { pts, corr, bed, floor_hw, wall_m, wall_rise,
                            asym, s_wl, s_wr, s_flip, s_shelf, s_cat, s_wr2, shelf_h,
                            incised });
}

/// The corridor: carved into the MACRO (8 m), before texture. Left and right
/// are shaped independently — the cut-bank side is steeper and narrower, the
/// slip-off side wider and gentler, the roles flipping slowly along the arc —
/// and an intermittent shelf (a young terrace) rides the slip-off side.
/// Short draws cutting into the river-corridor RIM.
///
/// Review 2026-08-26: our Sandhills river valleys read as painted-on. Against
/// the real Dismal River the reason is plain — a real Sandhills river runs in
/// a sharply DISSECTED canyon, its rim crenulated by dozens of short steep
/// draws, while the dune field a few hundred metres away carries no drainage
/// at all. Ours was a smooth trough with a sinuous line in it.
///
/// The asymmetry is real geology rather than an oversight to correct
/// everywhere: Sandhills rivers are allogenic, fed by groundwater through
/// sand too permeable to shed surface flow, so runoff channels exist ONLY
/// where the water table is cut — along the trunk. That is why this is a rim
/// treatment and not a drainage network: very low density overall, short
/// tributaries, exactly as the review asked.
///
/// Each draw grades UP from its mouth at a fixed slope and stops where the
/// ground falls below that grade — which is the rim by construction, since
/// the wall climbs faster than the draw and the plateau does not. Nothing
/// decides where the rim is; the draw finds it.
pub fn carve_rim_draws(rng: &mut DetRng, height: &mut Grid<f64>, pl: &RiverPlan) -> usize {
    let spec = height.spec;
    let cell = spec.cell_size;
    let step = 4.0;
    let spacing = rng.range_f64(150.0, 260.0);
    let n = pl.corr.len();
    if n < 40 {
        return 0;
    }
    let mut cut = 0usize;
    let mut arc_next = rng.range_f64(60.0, spacing);
    for i in 3..n - 3 {
        let arc = i as f64 * step;
        if arc < arc_next {
            continue;
        }
        arc_next = arc + spacing * rng.range_f64(0.62, 1.45);
        let a = pl.corr[i - 3];
        let b = pl.corr[i + 3];
        let tang = Vec2::new(b.x - a.x, b.y - a.y);
        let tl = tang.length().max(1e-9);
        let perp = Vec2::new(-tang.y / tl, tang.x / tl);
        let side = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
        // start just outside the wet floor and head away from the river,
        // with a little obliquity so draws are not a comb of normals
        let obl = rng.range_f64(-0.55, 0.55);
        let dir = Vec2::new(
            perp.x * side * math::cos(obl) - perp.y * side * math::sin(obl),
            perp.x * side * math::sin(obl) + perp.y * side * math::cos(obl),
        );
        let mouth = Vec2::new(pl.corr[i].x + dir.x * (pl.floor_hw + 6.0),
                              pl.corr[i].y + dir.y * (pl.floor_hw + 6.0));
        // The floor grades up from the CORRIDOR FLOOR, not from the ground at
        // the mouth. Taking it from the mouth put the draw's floor level with
        // the wall it was supposed to cut, so `floor_z >= g` held on the first
        // step and every draw died at t = 0 — the pass ran and changed
        // nothing. A tributary starts at the elevation of the water it joins.
        let z_mouth = height.bilinear(pl.corr[i]);
        let grade = rng.range_f64(0.030, 0.075);
        let len = rng.range_f64(90.0, 300.0);
        let hw0 = rng.range_f64(9.0, 22.0);
        let s_w = rng.next_u32();
        let mut t = 0.0f64;
        let mut alive = false;
        while t < len {
            // a draw wanders as it climbs; straight ones read as scratches
            let wob = 0.30 * course_world::noise::perlin1(t / 90.0, s_w);
            let dx = dir.x * math::cos(wob) - dir.y * math::sin(wob);
            let dy = dir.x * math::sin(wob) + dir.y * math::cos(wob);
            let p = Vec2::new(mouth.x + dx * t, mouth.y + dy * t);
            let g = height.bilinear(p);
            let floor_z = z_mouth + grade * t;
            if floor_z >= g - 0.25 {
                break;                  // the grade has caught the ground: this is the rim
            }
            alive = true;
            // width tapers to a point at the head
            let f = 1.0 - t / len;
            let hw = (hw0 * f.max(0.0).sqrt()).max(cell);
            let r = (hw / cell).ceil() as i64 + 1;
            let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
            for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
                for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                    let q = spec.world_of(gx as u32, gy as u32);
                    let dd = q.distance(p);
                    if dd > hw {
                        continue;
                    }
                    let idx = spec.index(gx as u32, gy as u32);
                    let here = height.data[idx];
                    if here <= floor_z {
                        continue;
                    }
                    // V section: floor at the centre, untouched at the lip
                    let u = (dd / hw).clamp(0.0, 1.0);
                    let target = floor_z + (here - floor_z) * u.powf(1.45);
                    if target < here {
                        height.data[idx] = target;
                    }
                }
            }
            t += step;
        }
        if alive {
            cut += 1;
        }
    }
    cut
}

pub fn carve_corridor(height: &mut Grid<f64>, pl: &RiverPlan) {
    let spec = height.spec;
    const OUT_GRADE: f64 = 0.015;
    let step = 4.0;
    // Reach must be far enough that the repose hand-off below ALWAYS meets
    // the ground before it — a hard reach cutoff against tall belts printed
    // an 8 m staircase cliff (review render, seed 5).
    let reach_max = (pl.floor_hw + pl.wall_m) * 1.6 + 320.0;
    for (i, q) in pl.corr.iter().enumerate() {
        let arc = i as f64 * step;
        // local tangent for the side sign
        let a = &pl.corr[i.saturating_sub(3)];
        let b = &pl.corr[(i + 3).min(pl.corr.len() - 1)];
        let tang = Vec2::new(b.x - a.x, b.y - a.y);
        let tl = tang.length().max(1e-9);
        let (tx, ty) = (tang.x / tl, tang.y / tl);
        // which side is the cut bank right now (flips over ~700 m)
        let flip = course_world::noise::perlin1(arc / 700.0, pl.s_flip) > 0.0;
        // per-side widths: aggressive breathing, plus INSIDE-OF-BEND
        // widening -- a migrating channel leaves its slip-off side broad, so
        // the valley opens on the inside of every bend and the corridor stops
        // shadowing the channel line exactly (review).
        let v1 = Vec2::new(pl.corr[i].x - a.x, pl.corr[i].y - a.y);
        let v2 = Vec2::new(b.x - pl.corr[i].x, b.y - pl.corr[i].y);
        let cz = v1.x * v2.y - v1.y * v2.x;
        let turn = (cz / (v1.length() * v2.length()).max(1e-9)).clamp(-1.0, 1.0);
        let bend = 2.6 * turn.abs().min(0.30);
        let (bl2, br2) = if turn > 0.0 { (bend, 0.0) } else { (0.0, bend) };
        let ml = 1.0 + bl2 + 0.95 * course_world::noise::perlin1(arc / 300.0, pl.s_wl)
            + 0.50 * course_world::noise::perlin1(arc / 90.0, pl.s_wl ^ 0x11);
        let mr = 1.0 + br2 + 0.95 * course_world::noise::perlin1(arc / 300.0, pl.s_wr)
            + 0.50 * course_world::noise::perlin1(arc / 90.0, pl.s_wr ^ 0x11);
        // shelf gate: present in stretches on the slip-off side
        let shelf_on = course_world::noise::perlin1(arc / 520.0, pl.s_shelf) > 0.15;
        let fl = pl.bed[i] + 0.4;
        let x0 = ((q.x - reach_max) / spec.cell_size).floor().max(0.0) as u32;
        let x1 = ((q.x + reach_max) / spec.cell_size).ceil().min(spec.nx as f64 - 1.0) as u32;
        let y0 = ((q.y - reach_max) / spec.cell_size).floor().max(0.0) as u32;
        let y1 = ((q.y + reach_max) / spec.cell_size).ceil().min(spec.ny as f64 - 1.0) as u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let w = spec.world_of(x, y);
                let (dx, dy) = (w.x - q.x, w.y - q.y);
                let dd = (dx * dx + dy * dy).sqrt();
                if dd >= reach_max {
                    continue;
                }
                let left = dx * ty - dy * tx > 0.0;
                let cut_side = left == flip;
                let (m, wallf) = if cut_side {
                    // cut bank: narrower and steeper
                    (if left { ml } else { mr } * (1.0 - pl.asym * 0.45), 1.0 - pl.asym * 0.5)
                } else {
                    (if left { ml } else { mr } * (1.0 + pl.asym * 0.45), 1.0 + pl.asym * 0.4)
                };
                let fhw = (pl.floor_hw * m).max(8.0);
                // the WALL breathes too -- with only the floor breathing, the
                // visible corridor width read as near-constant (review)
                let wm = (pl.wall_m * wallf * m).max(28.0);
                let u = ((dd - fhw) / wm).clamp(0.0, 1.0);
                let beyond = (dd - fhw - wm).max(0.0);
                // wall rise breathes along the arc, and the whole catena
                // carries 2-D noise that grows away from the water -- level
                // transition lines read as artificial without it (review)
                let wr = pl.wall_rise
                    * (1.0 + 0.35 * course_world::noise::perlin1(arc / 900.0, pl.s_wr2));
                let cat = (0.15 + 1.05 * u)
                    * course_world::noise::perlin2(w.x / 90.0, w.y / 90.0, pl.s_cat)
                    + 0.35 * u
                        * course_world::noise::perlin2(w.x / 34.0, w.y / 34.0,
                                                       pl.s_cat ^ 0x5A5A)
                    // rim band: the daylight line was too rounded (review) --
                    // extra short noise concentrated where wall meets dunes
                    + math::smoothstep(0.5, 1.0, u) * 1.5
                        * course_world::noise::perlin2(w.x / 42.0, w.y / 42.0,
                                                       pl.s_cat ^ 0xC3);
                // hand-off: ~1.5% for the first 40 m, then loose-sand repose
                // (~10%) until the profile MEETS the dunes. The flat-grade
                // version could not daylight on a 15 m belt inside the reach
                // and cliffed at the boundary.
                let hand = if beyond <= 40.0 {
                    OUT_GRADE * beyond
                } else {
                    OUT_GRADE * 40.0 + 0.10 * (beyond - 40.0)
                };
                let mut tgt = fl + wr * u * u * (3.0 - 2.0 * u) + hand + cat;
                // the shelf: flatten a band of the slip-off wall onto a tread
                if shelf_on && !cut_side {
                    // steps sit CLOSE to the stream at this scale (review)
                    let sh = fl + pl.shelf_h;
                    if tgt > sh && u < 0.45 {
                        let ease = math::smoothstep(0.45, 0.28, u);
                        tgt = tgt + (sh - tgt) * ease;
                    }
                }
                let li = spec.index(x, y);
                if height.data[li] > tgt {
                    let ex = height.data[li] - tgt;
                    height.data[li] -= ex * math::smoothstep(0.0, 4.0, ex);
                }
            }
        }
    }
}

/// The channel slot + water: cut at 2 m, AFTER texture — the one sharp piece.
/// Cut the wet creek slot on a gorge floor and write its water surface.
///
/// The gorge's own line, rather than a `RiverPlan` — same job as
/// `cut_channel`, but the gorge carries a plain centre-line and bed. Kept
/// narrow (a Sandhills river is a few metres of water in a very large
/// canyon) and cut into the ground so the wet ribbon is continuous by
/// construction, which is the lesson from the Carolina meander creek.
/// Cut the channel and fill it. `width_m` is the water surface width: a creek
/// runs 3-5 m, a river 9-15 m (`Descriptors::river_w_m`).
///
/// The section is FLAT-BOTTOMED with a steep rise to the water surface, which
/// is what the reviewer asked for and what the real channels do. It used to be
/// a `smoothstep` from the centre-line out to a 4 m bank, which is a rounded
/// gutter -- the water sat in a dish with no bank line anywhere.
pub fn cut_creek(height: &mut Grid<f64>, water: &mut Water,
                 line: &[Vec2], bed: &[f64], width_m: f64,
                 _sections: Option<&CreekSections>, salt: u32) {
    if line.len() < 3 {
        return;
    }
    // Ablations: `none` cuts and wets nothing (for difference maps); `slot`
    // is the flat-bed stamp this function cut before 2026-09-02; `incise`
    // is the corpus-section carve of 4573265 that read as blobs. The
    // Nebraska section pack is measurement for the review tooling; the
    // carve no longer consumes it.
    let mode = std::env::var("CREEK_CARVE").unwrap_or_default();
    if mode == "none" {
        return;
    }
    if mode == "slot" {
        cut_creek_slot_legacy(height, water, line, bed, width_m);
        return;
    }
    let spec = height.spec;
    let (arcs, perps, bends) = node_frames(line);
    let h0: Vec<f64> = height.data.clone();
    let mut mask = vec![false; spec.len()];
    let mut wet = 0usize;
    let bed_at: Vec<f64> = (0..line.len()).map(|i| bed[i.min(bed.len() - 1)]).collect();
    let hw = (width_m * 0.5).max(1.2);
    if mode == "incise" {
        let builtin = CreekSections::builtin();
        let sec = _sections.unwrap_or(&builtin);
        let cfg = Incision { sections: sec, bank_steep: BANK_STEEPNESS, hw,
                             hw_clamp: (hw * 0.62, hw * 1.45),
                             cut_base: 0.30 + 0.045 * width_m, salt };
        incise(height, &h0, &mut water.surface, &mut mask, &mut wet,
               line, &bed_at, &arcs, &perps, &bends, &cfg);
    } else if mode == "lowered" {
        // the slot, lowered not replaced (ablation)
        let cfg = SlotLowered { hw, hw_clamp: (hw * 0.62, hw * 1.45),
                                depth: 0.30 + 0.045 * width_m, rise_m: 2.4, free_m: 0.45, salt };
        slot_lowered(height, &h0, &mut water.surface, &mut mask, &mut wet,
                     line, &bed_at, &arcs, &perps, &bends, &cfg);
    } else if mode == "delta" {
        let builtin = CreekSections::builtin();
        let sec = _sections.unwrap_or(&builtin);
        let cfg = DeltaCarve { sections: sec, hw, hw_clamp: (hw * 0.62, hw * 1.45),
                               wet_d: 0.30 + 0.045 * width_m, salt,
                               mouth_first: bed[0] <= bed[bed.len() - 1] };
        let level = delta_carve(height, &h0, &mut water.surface, &mut mask, &mut wet,
                                line, &arcs, &perps, &bends, &cfg);
        water.river = Some(line.to_vec());
        water.river_z = Some(level);
    } else {
        // the crease, with the river's own width and depth law
        let cfg = CreekCrease { hw, hw_clamp: (hw * 0.6, hw * 1.5),
                                depth: 0.32 + 0.04 * width_m, bank_m: 3.0 + 0.15 * width_m, salt,
                                mouth_first: bed[0] <= bed[bed.len() - 1], cut_cap: 6.0 };
        let (level, pooled) = creek_crease(height, &h0, &mut water.surface, &mut mask, &mut wet,
                                           line, &arcs, &perps, &bends, &cfg);
        // a pool behind a rise stands over the floor beside the ribbon too
        pool_beside(&spec, &height.data, &mut water.surface, &mut wet, line, &level, &pooled, 150.0);
        water.river = Some(line.to_vec());
        water.river_z = Some(level);
    }
    water.lake_frac += wet as f64 / spec.len() as f64;
}

/// Centred running mean of `v` over +-`half_m` of arc length. O(n).
fn run_mean(v: &[f64], arcs: &[f64], half_m: f64) -> Vec<f64> {
    let n = v.len();
    let mut out = vec![0.0; n];
    let (mut lo, mut hi, mut acc) = (0usize, 0usize, 0.0);
    for i in 0..n {
        while hi < n && arcs[hi] <= arcs[i] + half_m { acc += v[hi]; hi += 1; }
        while lo < hi && arcs[lo] < arcs[i] - half_m { acc -= v[lo]; lo += 1; }
        out[i] = acc / (hi - lo).max(1) as f64;
    }
    out
}

/// The pack's measured transects averaged into depth-ordered rungs of
/// NORMALISED bank shape. Indexing the raw rows along the creek -- even from
/// a smooth depth -- swaps one real transect for an unrelated one every
/// metre or so, and that printed teeth on the banks; averaging keeps the
/// corpus shape while making it a smooth function of depth.
struct ShapeLadder {
    depth: Vec<f64>,
    p_steep: Vec<[f64; 25]>,
    p_gentle: Vec<[f64; 25]>,
}

const LADDER: usize = 8;

impl ShapeLadder {
    fn build(sec: &CreekSections) -> ShapeLadder {
        let n = sec.len().max(1);
        let norm = |s: &[f64; 25], w: f64| -> [f64; 25] {
            let w = w.clamp(1.0, 24.0);
            let at = |d: f64| -> f64 {
                let d = d.clamp(0.0, 24.0);
                let j = (d as usize).min(23);
                s[j] + (s[j + 1] - s[j]) * (d - j as f64)
            };
            let top = at(w).max(1e-6);
            let mut p = [0.0; 25];
            for (i, v) in p.iter_mut().enumerate() {
                *v = (at(i as f64 / 24.0 * w) / top).clamp(0.0, 1.0);
            }
            p[0] = 0.0;
            p[24] = 1.0;
            p
        };
        let mut out = ShapeLadder { depth: Vec::new(), p_steep: Vec::new(), p_gentle: Vec::new() };
        for k in 0..LADDER {
            let lo = k * n / LADDER;
            let hi = ((k + 1) * n / LADDER).max(lo + 1).min(n);
            let m = (hi - lo) as f64;
            let mut d = 0.0;
            let mut ps = [0.0; 25];
            let mut pg = [0.0; 25];
            for i in lo..hi {
                d += sec.depth[i];
                let a = norm(&sec.s_steep[i], sec.w_steep[i]);
                let b = norm(&sec.s_gentle[i], sec.w_gentle[i]);
                for j in 0..25 { ps[j] += a[j]; pg[j] += b[j]; }
            }
            for j in 0..25 { ps[j] /= m; pg[j] /= m; }
            ps[0] = 0.0; pg[0] = 0.0; ps[24] = 1.0; pg[24] = 1.0;
            out.depth.push(d / m);
            out.p_steep.push(ps);
            out.p_gentle.push(pg);
        }
        out
    }

    fn rung_for_depth(&self, d: f64) -> f64 {
        let n = self.depth.len();
        let j = self.depth.partition_point(|&x| x < d);
        if j == 0 { return 0.0; }
        if j >= n { return (n - 1) as f64; }
        let (a, b) = (self.depth[j - 1], self.depth[j]);
        (j - 1) as f64 + if b - a > 1e-9 { ((d - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// Normalised bank height at rung `k`, side, `x = distance / width`.
    fn bank(&self, k: f64, steep: bool, x: f64) -> f64 {
        if x <= 0.0 { return 0.0; }
        if x >= 1.0 { return 1.0; }
        let tab = if steep { &self.p_steep } else { &self.p_gentle };
        let n = tab.len();
        let k = k.clamp(0.0, (n - 1) as f64 - 1e-9);
        let i0 = (k as usize).min(n - 2);
        let f = k - i0 as f64;
        let xi = x * 24.0;
        let j = (xi as usize).min(23);
        let g = xi - j as f64;
        let a = tab[i0][j] + (tab[i0][j + 1] - tab[i0][j]) * g;
        let b = tab[i0 + 1][j] + (tab[i0 + 1][j + 1] - tab[i0 + 1][j]) * g;
        (a + (b - a) * f).clamp(0.0, 1.0)
    }
}

/// What the creek crease needs per mode.
pub struct CreekCrease {
    /// nominal wet half-width, metres, and the band its scalloping stays in
    pub hw: f64,
    pub hw_clamp: (f64, f64),
    /// nominal incision depth below the local ground, metres
    pub depth: f64,
    /// nominal bank width beyond the wet edge, metres
    pub bank_m: f64,
    pub salt: u32,
    pub mouth_first: bool,
    /// The most the crease may cut below the local ground, metres. Where a
    /// rise on the line demands more, the water POOLS behind it instead
    /// (2026-09-07): the descending level used to be a running min of the
    /// ground, so one rise near the mouth dug a 6 m trench through the floor
    /// (17/45 fluvial tiles; ground 60 m either side 6-10 m above the water).
    /// Below REGRADE_MAX (6 m) the descent is paid by regrading the floor
    /// toward the creek at 5 %; the cap is that same 6 m, so only a taller
    /// rise pools.
    pub cut_cap: f64,
}

/// The creek as a THIN, IRREGULAR CREASE in a floor that already slopes to it.
///
/// What the 2 m corpus actually shows (256 resolved Carolina transects): the
/// ground rises from the channel in a straight line at ~5 % -- 0.12 m at 4 m,
/// 0.46 at 10, 0.82 at 16, 1.33 at 24 -- with no bank and no rim. The dark
/// line in a real hillshade is the bottom of that broad V, and the V is the
/// VALLEY FLOOR converging on the creek. Every earlier carve sank a profile
/// into a flat floor and so drew a trough with a rim parallel to the creek;
/// the rim was the artefact, whatever rode on it. So the convergence moved
/// to the valley stage (`assemble`, the measured 5 % over the inner floor)
/// and the creek itself is only what remains: a narrow incision, its width
/// and depth scalloping along the arc at 8-90 m so the margin is never a
/// ruled line, a bank of a metre or two with a rounded lip, and NOTHING
/// beyond. Level derived from the ground with one descending pass; every
/// cell is `ground - D` (texture by subtraction); one evaluation per cell
/// against the segments. Never a fill, zero draws.
#[allow(clippy::too_many_arguments)]
fn creek_crease(height: &mut Grid<f64>, h0: &[f64], surface: &mut Grid<f64>,
                creek: &mut [bool], wet_cells: &mut usize, qs: &[Vec2],
                arcs: &[f64], perps: &[Vec2], bends: &[f64], cfg: &CreekCrease) -> (Vec<f64>, Vec<bool>) {
    const BLUR_R: i64 = 3;
    const BANK_LIP: f64 = 0.05;
    const LONG_SMOOTH_M: f64 = 80.0;
    /// the margin scallops: two short scales on the wet half-width
    const HW_VAR: [(f64, f64); 2] = [(0.40, 8.5), (0.30, 23.0)];
    /// the depth breathes at pool/riffle scales
    const DEPTH_VAR: [(f64, f64); 2] = [(0.40, 31.0), (0.30, 90.0)];
    const DEPTH_MIN: f64 = 0.15;
    /// bank width scallops with the margin
    const BANK_VAR: (f64, f64) = (0.45, 14.0);
    const BANK_CLAMP: (f64, f64) = (1.2, 8.0);
    /// outer/inner asymmetry as a continuous bend weight
    const ASYM_TURN: f64 = 2.5;
    const ASYM_HW: f64 = 0.12;
    const ASYM_BANK: f64 = 0.25;
    /// the lip: C1 into the ground over this much elevation
    const K_LIP: f64 = 0.12;
    let spec = height.spec;
    let cell = spec.cell_size;
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let n = qs.len();
    if n < 3 {
        return (Vec::new(), Vec::new());
    }
    let (s_h1, s_h2) = (cfg.salt ^ 0xC2B2_AE35, cfg.salt ^ 0x7F4A_7C15);
    let (s_d1, s_d2) = (cfg.salt ^ 0x85EB_CA6B, cfg.salt ^ 0x3C6E_F372);
    let s_b = cfg.salt ^ 0x9E37_79B9;

    // --- the ground under the creek: SAT once, 13 m box at the node cells --
    let w1 = nx as usize + 1;
    let mut sat = vec![0.0f64; (nx as usize + 1) * (ny as usize + 1)];
    for y in 0..ny as usize {
        let mut row = 0.0;
        for x in 0..nx as usize {
            row += h0[y * nx as usize + x];
            sat[(y + 1) * w1 + x + 1] = sat[y * w1 + x + 1] + row;
        }
    }
    let box13 = |x: i64, y: i64| -> f64 {
        let (x0, x1) = ((x - BLUR_R).max(0) as usize, ((x + BLUR_R).min(nx - 1) + 1) as usize);
        let (y0, y1) = ((y - BLUR_R).max(0) as usize, ((y + BLUR_R).min(ny - 1) + 1) as usize);
        (sat[y1 * w1 + x1] - sat[y0 * w1 + x1] - sat[y1 * w1 + x0] + sat[y0 * w1 + x0])
            / ((x1 - x0) * (y1 - y0)) as f64
    };
    let g_raw: Vec<f64> = qs.iter().map(|p| {
        box13(((p.x / cell).round() as i64).clamp(0, nx - 1), ((p.y / cell).round() as i64).clamp(0, ny - 1))
    }).collect();
    let g = run_mean(&g_raw, arcs, LONG_SMOOTH_M);

    // --- per node: width, depth, bank, asymmetry --------------------------
    let pn = |a: f64, l: f64, s: u32| course_world::noise::perlin1(a / l, s);
    let hw_n: Vec<f64> = arcs.iter().map(|&a| {
        (cfg.hw * (1.0 + HW_VAR[0].0 * pn(a, HW_VAR[0].1, s_h1) + HW_VAR[1].0 * pn(a, HW_VAR[1].1, s_h2)))
            .clamp(cfg.hw_clamp.0, cfg.hw_clamp.1)
    }).collect();
    let dep_n: Vec<f64> = arcs.iter().map(|&a| {
        (cfg.depth * (1.0 + DEPTH_VAR[0].0 * pn(a, DEPTH_VAR[0].1, s_d1) + DEPTH_VAR[1].0 * pn(a, DEPTH_VAR[1].1, s_d2)))
            .max(DEPTH_MIN)
    }).collect();
    let bank_n: Vec<f64> = arcs.iter().map(|&a| {
        (cfg.bank_m * (1.0 + BANK_VAR.0 * pn(a, BANK_VAR.1, s_b))).clamp(BANK_CLAMP.0, BANK_CLAMP.1)
    }).collect();
    let asym_raw: Vec<f64> = bends.iter().map(|&b| (b / ASYM_TURN).clamp(-1.0, 1.0)).collect();
    let asym = run_mean(&asym_raw, arcs, 15.0);

    // --- the level: ground minus depth, descending to the mouth -----------
    let mut level: Vec<f64> = (0..n).map(|i| g[i] - dep_n[i]).collect();
    if cfg.mouth_first {
        for i in (0..n - 1).rev() { level[i] = level[i].min(level[i + 1]); }
    } else {
        for i in 1..n { level[i] = level[i].min(level[i - 1]); }
    }
    // Cap the cut and pool behind what is left (see `cut_cap`): the level
    // may not sit deeper than the cap below the ground, and a dam backs
    // water up -- walking upstream from the mouth the level never falls.
    // Both together keep the level descending to the mouth.
    let level_min = level.clone();
    if std::env::var("CREEK_POOL").map(|v| v != "off").unwrap_or(true) {
        for i in 0..n {
            let cap = g[i] - cfg.cut_cap;
            if level[i] < cap { level[i] = cap; }
        }
        if cfg.mouth_first {
            for i in 1..n { level[i] = level[i].max(level[i - 1]); }
        } else {
            for i in (0..n - 1).rev() { level[i] = level[i].max(level[i + 1]); }
        }
    }
    // where the water stands above the descending level: a pool behind a
    // dam, the only place standing water may spread beside the ribbon
    let pooled: Vec<bool> = (0..n).map(|i| level[i] > level_min[i] + 0.05).collect();
    for i in 1..n {
        let (lo, hi) = if cfg.mouth_first { (level[i - 1], level[i]) } else { (level[i], level[i - 1]) };
        debug_assert!(hi >= lo - 1e-9, "creek level rises toward the mouth at {i}");
        assert!(hi >= lo - 1e-6, "creek level rises toward the mouth at {i}: {lo} -> {hi}");
    }
    // --- the floor converges on the CREEK (2026-09-09) --------------------
    // The valley stage slopes the floor at 5 % toward the TRUNK AXIS, and
    // the creek wanders across that floor: 40 m off the axis the ground
    // stands 2 m higher, so every bend read as a rise. The descent cost
    // (ground minus level, beyond the crease's own depth) used to be paid
    // as a narrow cut down the bank -- the "deep cut" family -- or, capped,
    // as a pool that dammed the valley behind it (700234: the whole floor
    // under water for 700 m). What the corpus section shows is the floor
    // converging on the creek. So the excursion is regraded at that same
    // 5 %: subtracted from the ground over a width of `ex / FLOOR_CONV_S`,
    // texture kept, up to REGRADE_MAX; only a rise beyond that pools.
    // Ablation: `CREEK_REGRADE=off`.
    const REGRADE_MAX: f64 = 6.0;
    let regrade_on = std::env::var("CREEK_REGRADE").map(|v| v != "off").unwrap_or(true);
    let ex: Vec<f64> = (0..n).map(|i| if regrade_on {
        (g[i] - level[i] - dep_n[i]).max(0.0).min(REGRADE_MAX)
    } else { 0.0 }).collect();
    // the cut at the wet edge: the crease depth plus whatever descent the
    // regrade did not take
    let d_edge: Vec<f64> = (0..n).map(|i| (g[i] - level[i] - ex[i]).max(0.05)).collect();
    let mut rg = vec![0.0f64; spec.len()];
    if regrade_on {
        let conv_s = crate::assemble::FLOOR_CONV_S;
        let mut rg_d = vec![f64::INFINITY; spec.len()];
        let mut rg_i = vec![u32::MAX; spec.len()];
        for i in 0..n {
            if ex[i] < 0.05 { continue; }
            let r = (ex[i] / conv_s).clamp(20.0, 150.0);
            let rc = (r / cell).ceil() as i64;
            let (cx, cy) = ((qs[i].x / cell).round() as i64, (qs[i].y / cell).round() as i64);
            for gy in (cy - rc).max(0)..=(cy + rc).min(ny - 1) {
                for gx in (cx - rc).max(0)..=(cx + rc).min(nx - 1) {
                    let idx = spec.index(gx as u32, gy as u32);
                    let dd = spec.world_of(gx as u32, gy as u32).distance(qs[i]);
                    if dd < rg_d[idx] { rg_d[idx] = dd; rg_i[idx] = i as u32; }
                }
            }
        }
        let conv_w = crate::assemble::FLOOR_CONV_W;
        for idx in 0..spec.len() {
            if rg_i[idx] == u32::MAX { continue; }
            let i = rg_i[idx] as usize;
            let r = (ex[i] / conv_s).clamp(20.0, 150.0);
            let dd = rg_d[idx];
            // never below the converging floor's own target: a cell already
            // at or under it is texture, and pushing it further put hollows
            // under the water line beside the ribbon
            let conv = if dd < conv_w { conv_s * dd - conv_s * dd * dd / (2.0 * conv_w) } else { 0.5 * conv_s * conv_w };
            let target = level[i] + dep_n[i] + conv;
            let want = ex[i] * (1.0 - math::smoothstep(0.0, 1.0, dd / r));
            rg[idx] = want.min((h0[idx] - target).max(0.0));
        }
    }
    let h0r: Vec<f64> = (0..spec.len()).map(|k| h0[k] - rg[k]).collect();
    for idx in 0..spec.len() {
        if rg[idx] > 0.0 && h0r[idx] < height.data[idx] {
            height.data[idx] = h0r[idx];
        }
    }
    let h0 = &h0r[..];

    // --- window and nearest station (segments) ----------------------------
    let reach_m = cfg.hw_clamp.1 * (1.0 + ASYM_HW) + BANK_CLAMP.1 * (1.0 + ASYM_BANK) + 2.0 * cell;
    let r_max = (reach_m / cell).ceil() as i64 + 1;
    let mut near = vec![u32::MAX; spec.len()];
    let mut ndist = vec![f64::INFINITY; spec.len()];
    for i in 0..n - 1 {
        let (a, b) = (qs[i], qs[i + 1]);
        let ab = Vec2::new(b.x - a.x, b.y - a.y);
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let (cx, cy) = (((a.x + b.x) * 0.5 / cell).round() as i64,
                        ((a.y + b.y) * 0.5 / cell).round() as i64);
        for gy in (cy - r_max).max(0)..=(cy + r_max).min(ny - 1) {
            for gx in (cx - r_max).max(0)..=(cx + r_max).min(nx - 1) {
                let idx = spec.index(gx as u32, gy as u32);
                let q = spec.world_of(gx as u32, gy as u32);
                let t = if len2 > 1e-12 {
                    (((q.x - a.x) * ab.x + (q.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
                } else { 0.0 };
                let d = q.distance(Vec2::new(a.x + ab.x * t, a.y + ab.y * t));
                if d < ndist[idx] {
                    ndist[idx] = d;
                    near[idx] = (if t > 0.5 { i + 1 } else { i }) as u32;
                }
            }
        }
    }

    // --- one evaluation per cell ------------------------------------------
    for idx in 0..spec.len() {
        if near[idx] == u32::MAX { continue; }
        let i = near[idx] as usize;
        let dd = ndist[idx];
        let q = spec.world_of((idx % nx as usize) as u32, (idx / nx as usize) as u32);
        let side = (q.x - qs[i].x) * perps[i].x + (q.y - qs[i].y) * perps[i].y;
        let outer = if side >= 0.0 { asym[i] } else { -asym[i] };
        let hw_c = hw_n[i] * (1.0 + ASYM_HW * outer);
        let bank_c = bank_n[i] * (1.0 - ASYM_BANK * outer);   // cut bank shorter and steeper
        let floor = level[i] - 0.5 * dep_n[i].min(0.6);        // wet depth: half the crease, capped
        if dd <= hw_c + cell * 0.32 {
            creek[idx] = true;
            if surface.data[idx].is_nan() {
                *wet_cells += 1;
                surface.data[idx] = level[i];
            } else {
                surface.data[idx] = surface.data[idx].min(level[i]);
            }
        }
        let cand = if dd <= hw_c {
            floor.min(h0[idx])
        } else {
            let x = (dd - hw_c) / bank_c;
            if x >= 1.0 {
                f64::INFINITY                   // beyond the bank: not lowered
            } else {
                // the crease: the wet-edge cut falling to zero over the bank
                // with a rounded lip, subtracted from the ground as it is
                let dcut = d_edge[i] * (1.0 - math::smoothstep(0.0, 1.0, x));
                let t = (h0[idx] - dcut).max(floor);
                course_world::ease::smin(t, h0[idx], K_LIP)
            }
        };
        if cand < height.data[idx] {
            height.data[idx] = cand;
            creek[idx] = true;
        }
        // The bank stands at or above the water line. A texture hollow in
        // the bank zone below the level was either left dry beside the water
        // (the screen's "perched" edge, 37/45 fluvial tiles once the pool
        // spread was confined to standing water) or wetted into a side pond
        // at every bend. Neither is a creek bank: the hollow is filled to
        // the water line plus a lip. Bounded by the texture, ~0.5 m. Over
        // the whole window, not just the (scalloped) bank width: the hollows
        // sat just beyond it.
        if dd > hw_c + cell * 0.32 && height.data[idx] < level[i] + BANK_LIP {
            height.data[idx] = level[i] + BANK_LIP;
            creek[idx] = true;
        }
    }
    (level, pooled)
}

/// Water standing beside the creek where the creek's level stands above the
/// floor (a pool behind a rise): from the ribbon outward, every cell whose
/// ground lies below its nearest station's level, within `reach_m` of the
/// line. The fluvial build does this in its backwater pass; the aeolian
/// build has no such pass, so it is done here.
fn pool_beside(spec: &course_world::grid::GridSpec, ground: &[f64], surface: &mut Grid<f64>,
               wet_cells: &mut usize, line: &[Vec2], level: &[f64], pooled: &[bool], reach_m: f64) {
    let cell = spec.cell_size;
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut st = vec![u32::MAX; spec.len()];
    let mut depth = vec![0u16; spec.len()];
    let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
    for (k, p) in line.iter().enumerate() {
        let (x, y) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
        if x < 0 || y < 0 || x >= nx || y >= ny { continue; }
        let i = spec.index(x as u32, y as u32);
        if st[i] == u32::MAX { st[i] = k as u32; q.push_back(i); }
    }
    let max_d = (reach_m / cell) as u16;
    while let Some(i) = q.pop_front() {
        if depth[i] >= max_d { continue; }
        let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (a, b) = (x + dx, y + dy);
            if a < 0 || b < 0 || a >= nx || b >= ny { continue; }
            let j = spec.index(a as u32, b as u32);
            if st[j] == u32::MAX { st[j] = st[i]; depth[j] = depth[i] + 1; q.push_back(j); }
        }
    }
    // from the wet ribbon outward over ground below the local level, only
    // where the level is a pool's (the ribbon's own level over a hollow
    // beside it is not standing water: that printed a side pond at every
    // bend on the mixed run)
    let is_pool = |j: usize| pooled[(st[j] as usize).min(pooled.len() - 1)];
    let mut q2: std::collections::VecDeque<usize> =
        (0..spec.len()).filter(|i| st[*i] != u32::MAX && surface.data[*i].is_finite() && is_pool(*i)).collect();
    while let Some(i) = q2.pop_front() {
        let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
        for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
            let (a, b) = (x + dx, y + dy);
            if a < 0 || b < 0 || a >= nx || b >= ny { continue; }
            let j = spec.index(a as u32, b as u32);
            if st[j] == u32::MAX || !is_pool(j) { continue; }
            let want = level[(st[j] as usize).min(level.len() - 1)];
            if ground[j] >= want - 0.02 { continue; }
            if surface.data[j].is_nan() {
                *wet_cells += 1;
                surface.data[j] = want;
                q2.push_back(j);
            } else if surface.data[j] < want - 1e-9 {
                surface.data[j] = want;
                q2.push_back(j);
            }
        }
    }
}

/// What the delta carve needs per mode.
pub struct DeltaCarve<'a> {
    /// the corpus pack: channel depth and bank shape, smooth along the arc
    pub sections: &'a CreekSections,
    /// wet half-width and the band its breathing stays in, metres
    pub hw: f64,
    pub hw_clamp: (f64, f64),
    /// water depth over the flat floor, metres
    pub wet_d: f64,
    pub salt: u32,
    /// index 0 is the mouth (the low end)
    pub mouth_first: bool,
}

/// The creek as a DELTA from the textured ground.
///
/// Owner, after three carves (2026-09-02): the cuts read artificial because
/// they were untextured and discontinuous; the version with the bowls looked
/// better because its banks kept the texture. So this carve is built from
/// exactly two principles and nothing else.
///
/// TEXTURE BY CONSTRUCTION. Every bank cell is `ground - D`, where `D` is a
/// smooth depth profile: the channel depth at the wet edge falling to zero
/// at the bank width. Subtracting a smooth function keeps every ripple of
/// the ground on the bank; nothing is replaced and no "detail return" has to
/// be tuned. The only flat cells are the wet floor, under water.
///
/// CONTINUITY BY CONSTRUCTION. The channel depth and the bank width are
/// properties of the CREEK -- real corpus values walked smoothly along the
/// arc -- never the gap to the valley's graded bed, so the channel exists
/// everywhere at one character. The water level is derived from the ground
/// under the creek (13 m box, then an 80 m running mean along the arc) with
/// ONE monotone pass that descends toward the mouth and never raises water
/// into a dip; measured, that costs a median of no extra cut and a 90th
/// percentile of ~1.3 m where the ground rises downstream. The bank
/// asymmetry is a continuous weight, not a switch. One evaluation per cell
/// against the nearest point on the creek's segments. No notches, no
/// distance warp, no depth cap, no window fade, no min-composition.
///
/// Never a fill. Zero draws. Fixed passes. The graded bed is used only to
/// say which end is the mouth.
#[allow(clippy::too_many_arguments)]
fn delta_carve(height: &mut Grid<f64>, h0: &[f64], surface: &mut Grid<f64>,
               creek: &mut [bool], wet_cells: &mut usize, qs: &[Vec2],
               arcs: &[f64], perps: &[Vec2], bends: &[f64], cfg: &DeltaCarve) -> Vec<f64> {
    /// smoothed-ground box half-width, cells (3 -> 13 m at 2 m)
    const BLUR_R: i64 = 3;
    /// longitudinal smoothing of the ground under the creek, metres
    const LONG_SMOOTH_M: f64 = 80.0;
    /// longitudinal smoothing of the channel depth, metres
    const DEPTH_SMOOTH_M: f64 = 120.0;
    const DEPTH_CLAMP: (f64, f64) = (0.35, 3.0);
    /// bank width variation along the arc: two non-harmonic scales
    const W_VAR: [(f64, f64); 2] = [(0.18, 60.0), (0.12, 170.0)];
    const W_CLAMP: (f64, f64) = (4.0, 26.0);
    /// how much the outer (cut) bank narrows and the inner (bar) widens
    const ASYM_W: f64 = 0.15;
    const ASYM_HW: f64 = 0.125;
    /// the bend weight saturates at this turn over +-10 m
    const ASYM_TURN: f64 = 2.5;
    /// C1 rim, metres of elevation
    const K_RIM: f64 = 0.10;
    let spec = height.spec;
    let cell = spec.cell_size;
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let n = qs.len();
    if n < 3 {
        return Vec::new();
    }
    let sec = cfg.sections;
    let s_h = cfg.salt ^ 0xC2B2_AE35;
    let s_sec = cfg.salt ^ 0x1656_67B1;
    let (s_w1, s_w2) = (cfg.salt ^ 0xA54F_F53A, cfg.salt ^ 0x2545_F491);

    // --- the ground under the creek: SAT once, 13 m box at the node cells --
    let w1 = nx as usize + 1;
    let mut sat = vec![0.0f64; (nx as usize + 1) * (ny as usize + 1)];
    for y in 0..ny as usize {
        let mut row = 0.0;
        for x in 0..nx as usize {
            row += h0[y * nx as usize + x];
            sat[(y + 1) * w1 + x + 1] = sat[y * w1 + x + 1] + row;
        }
    }
    let box13 = |x: i64, y: i64| -> f64 {
        let (x0, x1) = ((x - BLUR_R).max(0) as usize, ((x + BLUR_R).min(nx - 1) + 1) as usize);
        let (y0, y1) = ((y - BLUR_R).max(0) as usize, ((y + BLUR_R).min(ny - 1) + 1) as usize);
        (sat[y1 * w1 + x1] - sat[y0 * w1 + x1] - sat[y1 * w1 + x0] + sat[y0 * w1 + x0])
            / ((x1 - x0) * (y1 - y0)) as f64
    };
    let g_raw: Vec<f64> = qs.iter().map(|p| {
        box13(((p.x / cell).round() as i64).clamp(0, nx - 1), ((p.y / cell).round() as i64).clamp(0, ny - 1))
    }).collect();
    let g = run_mean(&g_raw, arcs, LONG_SMOOTH_M);

    // --- the creek's own depth, width and shape, smooth along the arc -----
    let n_sec = sec.len().max(2);
    let walk_m = sec.along_corr_m.max(10.0);
    let depth_raw: Vec<f64> = arcs.iter().map(|&a| {
        let k = (0.5 + 0.7 * course_world::noise::perlin1(a / walk_m, s_sec)).clamp(0.0, 0.999)
            * (n_sec - 1) as f64;
        sec.depth_at(k).clamp(DEPTH_CLAMP.0, DEPTH_CLAMP.1)
    }).collect();
    let depth = run_mean(&depth_raw, arcs, DEPTH_SMOOTH_M);
    let (w_typ, d_typ) = (sec.width_p50(), sec.depth_p50());
    let hw_n: Vec<f64> = arcs.iter().map(|&a| {
        (cfg.hw * (1.0 + 0.28 * course_world::noise::perlin1(a / 140.0, s_h)))
            .clamp(cfg.hw_clamp.0, cfg.hw_clamp.1)
    }).collect();
    // bend weight in [-1, 1], continuous: positive where the left side is
    // the outer bank
    let asym_raw: Vec<f64> = bends.iter().map(|&b| (b / ASYM_TURN).clamp(-1.0, 1.0)).collect();
    let asym = run_mean(&asym_raw, arcs, 15.0);

    // --- the water level: ground minus depth, descending to the mouth ----
    let mut level: Vec<f64> = (0..n).map(|i| g[i] - depth[i]).collect();
    if cfg.mouth_first {
        for i in (0..n - 1).rev() { level[i] = level[i].min(level[i + 1]); }
    } else {
        for i in 1..n { level[i] = level[i].min(level[i - 1]); }
    }
    for i in 1..n {
        let (lo, hi) = if cfg.mouth_first { (level[i - 1], level[i]) } else { (level[i], level[i - 1]) };
        debug_assert!(hi >= lo - 1e-9, "creek level rises toward the mouth at {i}");
        assert!(hi >= lo - 1e-6, "creek level rises toward the mouth at {i}: {lo} -> {hi}");
    }
    // the cut at the wet edge: the corpus depth plus whatever the descent cost
    let d_edge: Vec<f64> = (0..n).map(|i| (g[i] - level[i]).max(0.05)).collect();
    let w_n: Vec<f64> = (0..n).map(|i| {
        let a = arcs[i];
        (w_typ * math::pow(d_edge[i] / d_typ, 0.25)
            * (1.0 + W_VAR[0].0 * course_world::noise::perlin1(a / W_VAR[0].1, s_w1)
                   + W_VAR[1].0 * course_world::noise::perlin1(a / W_VAR[1].1, s_w2)))
            .clamp(W_CLAMP.0, W_CLAMP.1)
    }).collect();
    let lad = ShapeLadder::build(sec);
    let kfs: Vec<f64> = d_edge.iter().map(|&d| lad.rung_for_depth(d)).collect();

    // --- window: the widest bank plus the widest wet width; D is zero at the
    // bank width by construction, so nothing can reach it
    let reach_m = cfg.hw_clamp.1 * (1.0 + ASYM_HW) + W_CLAMP.1 * (1.0 + ASYM_W) + 2.0 * cell;
    let r_max = (reach_m / cell).ceil() as i64 + 1;

    // --- one station per cell: the nearest point on the SEGMENTS ----------
    let mut near = vec![u32::MAX; spec.len()];
    let mut ndist = vec![f64::INFINITY; spec.len()];
    for i in 0..n - 1 {
        let (a, b) = (qs[i], qs[i + 1]);
        let ab = Vec2::new(b.x - a.x, b.y - a.y);
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let (cx, cy) = (((a.x + b.x) * 0.5 / cell).round() as i64,
                        ((a.y + b.y) * 0.5 / cell).round() as i64);
        for gy in (cy - r_max).max(0)..=(cy + r_max).min(ny - 1) {
            for gx in (cx - r_max).max(0)..=(cx + r_max).min(nx - 1) {
                let idx = spec.index(gx as u32, gy as u32);
                let q = spec.world_of(gx as u32, gy as u32);
                let t = if len2 > 1e-12 {
                    (((q.x - a.x) * ab.x + (q.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
                } else { 0.0 };
                let d = q.distance(Vec2::new(a.x + ab.x * t, a.y + ab.y * t));
                if d < ndist[idx] {
                    ndist[idx] = d;
                    near[idx] = (if t > 0.5 { i + 1 } else { i }) as u32;
                }
            }
        }
    }

    // --- one evaluation per cell --------------------------------------------
    for idx in 0..spec.len() {
        if near[idx] == u32::MAX { continue; }
        let i = near[idx] as usize;
        let dd = ndist[idx];
        let q = spec.world_of((idx % nx as usize) as u32, (idx / nx as usize) as u32);
        let side = (q.x - qs[i].x) * perps[i].x + (q.y - qs[i].y) * perps[i].y;
        // outer-ness in [-1, 1]: +1 on the cut bank, -1 on the bar
        let outer = if side >= 0.0 { asym[i] } else { -asym[i] };
        let hw_c = hw_n[i] * (1.0 + ASYM_HW * outer);
        let w_c = w_n[i] * (1.0 - ASYM_W * outer);
        let floor = level[i] - cfg.wet_d;
        if dd <= hw_c + cell * 0.32 {
            creek[idx] = true;
            if surface.data[idx].is_nan() {
                *wet_cells += 1;
                surface.data[idx] = level[i];
            } else {
                surface.data[idx] = surface.data[idx].min(level[i]);
            }
        }
        let cand = if dd <= hw_c {
            floor
        } else {
            let x = (dd - hw_c) / w_c;
            if x >= 1.0 {
                continue;                       // beyond the bank: untouched
            }
            // the depth profile: the wet-edge cut, falling to zero along the
            // corpus bank shape; subtracted from the ground AS IT IS
            let dcut = d_edge[i] * (1.0 - lad.bank(kfs[i], outer >= 0.0, x));
            let t = (h0[idx] - dcut).max(floor);
            course_world::ease::smin(t, h0[idx], K_RIM)
        };
        if cand < height.data[idx] {
            height.data[idx] = cand;
            creek[idx] = true;
        }
    }
    level
}

/// The slot carve's numbers, per mode. Carolina: a 4-5 m blackwater creek
/// (`hw` 2.0-2.5, cut 0.30 m); Nebraska: the allogenic river (`hw` up to
/// 6.9 m, `0.30 + 0.045 * width_m` deep). Rise and freeboard are the slot's.
pub struct SlotLowered {
    pub hw: f64,
    pub hw_clamp: (f64, f64),
    pub depth: f64,
    pub rise_m: f64,
    pub free_m: f64,
    /// salt for the along-arc noise; never an RNG draw
    pub salt: u32,
}

/// The slot, LOWERED not replaced.
///
/// The profile is the one `cut_creek` cut through the Nebraska river rounds
/// and passed 12/12: a flat wet bed `depth` below the water, then one
/// straight bank at the slot's grade. What changes is how it meets the
/// ground. It used to be stamped over the textured surface as an absolute
/// elevation (a clean extrusion, "a constant cutout without texture"), and
/// the corpus incision that replaced it composed a radial target per node
/// and kept the lowest (bowls). Here:
///
///  * the bank climbs at its grade until it meets the SMOOTHED local ground
///    (13 m box), so it closes wherever the ground is, and the 2 m texture
///    residual returns across the outer bank -- at the rim the surface is
///    the untouched ground, through a C1 blend, never a crease or a step;
///  * every cell belongs to ONE station, the nearest point on the creek's
///    segments, and is evaluated once -- nothing is min-composed, so no
///    node can print a disc, and no window prints its square;
///  * the only variation along the arc is the wet half-width, the depth and
///    the bank grade, each a Perlin at 140-210 m, plus the outer/inner bank
///    asymmetry from the bend sign. No notches, no distance warp, no depth
///    cap, no re-derived level: the water sits at the bed each mode already
///    grades (`creek_z` / the fluvial `zs`), which the gorge and `fluvial`
///    keep monotone.
///
/// Never a fill. Zero draws. Fixed passes.
#[allow(clippy::too_many_arguments)]
fn slot_lowered(height: &mut Grid<f64>, h0: &[f64], surface: &mut Grid<f64>,
                creek: &mut [bool], wet_cells: &mut usize, qs: &[Vec2], zs: &[f64],
                arcs: &[f64], perps: &[Vec2], bends: &[f64], cfg: &SlotLowered) {
    // along-arc variation: the same scales the review-passed slot breathed at
    const HW_VAR: f64 = 0.28;
    const HW_LEN_M: f64 = 140.0;
    const DEPTH_VAR: f64 = 0.20;
    const DEPTH_LEN_M: f64 = 165.0;
    const DEPTH_MIN_M: f64 = 0.25;
    const GRADE_VAR: f64 = 0.25;
    const GRADE_LEN_M: f64 = 210.0;
    /// 17-42 degrees
    const GRADE_CLAMP: (f64, f64) = (0.30, 0.90);
    const OUTER_GRADE: f64 = 1.20;
    const INNER_GRADE: f64 = 0.80;
    /// smoothed-ground box half-width, cells (3 -> 13 m at 2 m)
    const BLUR_R: i64 = 3;
    /// horizontal extent of the C1 rim blend, metres
    const RIM_M: f64 = 1.5;
    /// where across the bank the texture residual comes back (fractions of
    /// the bank's closing distance)
    const DETAIL_IN: (f64, f64) = (0.25, 0.90);
    let spec = height.spec;
    let cell = spec.cell_size;
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let n = qs.len();
    if n < 2 {
        return;
    }
    let s_h = cfg.salt ^ 0xC2B2_AE35;
    let s_d = cfg.salt ^ 0x85EB_CA6B;
    let s_g = cfg.salt ^ 0x9E37_79B9;
    let grade0 = ((cfg.depth + cfg.free_m) / cfg.rise_m).clamp(GRADE_CLAMP.0, GRADE_CLAMP.1);

    // --- per node -------------------------------------------------------------
    let mut hw_n = Vec::with_capacity(n);
    let mut dep_n = Vec::with_capacity(n);
    let mut gr_n = Vec::with_capacity(n);
    let mut floor_n = Vec::with_capacity(n);
    for i in 0..n {
        let a = arcs[i];
        hw_n.push((cfg.hw * (1.0 + HW_VAR * course_world::noise::perlin1(a / HW_LEN_M, s_h)))
            .clamp(cfg.hw_clamp.0, cfg.hw_clamp.1));
        let d = (cfg.depth * (1.0 + DEPTH_VAR * course_world::noise::perlin1(a / DEPTH_LEN_M, s_d)))
            .max(DEPTH_MIN_M);
        dep_n.push(d);
        gr_n.push((grade0 * (1.0 + GRADE_VAR * course_world::noise::perlin1(a / GRADE_LEN_M, s_g)))
            .clamp(GRADE_CLAMP.0, GRADE_CLAMP.1));
        floor_n.push(zs[i] - d);
    }
    let node_cell = |i: usize| -> usize {
        let (cx, cy) = ((qs[i].x / cell).round() as i64, (qs[i].y / cell).round() as i64);
        spec.index(cx.clamp(0, nx - 1) as u32, cy.clamp(0, ny - 1) as u32)
    };

    // --- the window, derived from the deepest demand at the gentlest grade
    // (with room for the ground beside the creek standing higher than at
    // the node), so nothing can clip; the rim blend is added on top.
    let mut d_max = 0.0f64;
    for i in 0..n {
        d_max = d_max.max(h0[node_cell(i)] - floor_n[i]);
    }
    let reach_m = cfg.hw_clamp.1 * OUTER_K
        + (d_max * 1.6 + 1.0) / (GRADE_CLAMP.0 * INNER_GRADE) + 2.0 * RIM_M + 2.0 * cell;
    let r_max = (reach_m / cell).ceil() as i64 + 1;

    // --- corridor, smoothed ground, texture residual --------------------------
    let mut corr = vec![false; spec.len()];
    for p in qs {
        let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
        for gy in (cy - r_max).max(0)..=(cy + r_max).min(ny - 1) {
            for gx in (cx - r_max).max(0)..=(cx + r_max).min(nx - 1) {
                corr[spec.index(gx as u32, gy as u32)] = true;
            }
        }
    }
    let w1 = nx as usize + 1;
    let mut sat = vec![0.0f64; (nx as usize + 1) * (ny as usize + 1)];
    for y in 0..ny as usize {
        let mut row = 0.0;
        for x in 0..nx as usize {
            row += h0[y * nx as usize + x];
            sat[(y + 1) * w1 + x + 1] = sat[y * w1 + x + 1] + row;
        }
    }
    let mut h0s = vec![0.0f64; spec.len()];
    let mut detail = vec![0.0f64; spec.len()];
    for idx in 0..spec.len() {
        if !corr[idx] { continue; }
        let (x, y) = ((idx % nx as usize) as i64, (idx / nx as usize) as i64);
        let (x0, x1) = ((x - BLUR_R).max(0) as usize, ((x + BLUR_R).min(nx - 1) + 1) as usize);
        let (y0, y1) = ((y - BLUR_R).max(0) as usize, ((y + BLUR_R).min(ny - 1) + 1) as usize);
        let sum = sat[y1 * w1 + x1] - sat[y0 * w1 + x1] - sat[y1 * w1 + x0] + sat[y0 * w1 + x0];
        h0s[idx] = sum / ((x1 - x0) * (y1 - y0)) as f64;
        detail[idx] = h0[idx] - h0s[idx];
    }

    // --- one station per cell: the nearest point on the SEGMENTS -------------
    // (distance to the 1 m nodes beats against the 2 m grid and combs the bank)
    let mut near = vec![u32::MAX; spec.len()];
    let mut ndist = vec![f64::INFINITY; spec.len()];
    for i in 0..n - 1 {
        let (a, b) = (qs[i], qs[i + 1]);
        let ab = Vec2::new(b.x - a.x, b.y - a.y);
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let (cx, cy) = (((a.x + b.x) * 0.5 / cell).round() as i64,
                        ((a.y + b.y) * 0.5 / cell).round() as i64);
        for gy in (cy - r_max).max(0)..=(cy + r_max).min(ny - 1) {
            for gx in (cx - r_max).max(0)..=(cx + r_max).min(nx - 1) {
                let idx = spec.index(gx as u32, gy as u32);
                let q = spec.world_of(gx as u32, gy as u32);
                let t = if len2 > 1e-12 {
                    (((q.x - a.x) * ab.x + (q.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
                } else { 0.0 };
                let d = q.distance(Vec2::new(a.x + ab.x * t, a.y + ab.y * t));
                if d < ndist[idx] {
                    ndist[idx] = d;
                    near[idx] = (if t > 0.5 { i + 1 } else { i }) as u32;
                }
            }
        }
    }

    // --- one evaluation per cell ----------------------------------------------
    let dw_lim = r_max as f64 * cell - cfg.hw_clamp.1 * OUTER_K - 2.0 * RIM_M;
    let mut demand: Vec<f64> = Vec::new();
    let mut closes: Vec<f64> = Vec::new();
    for idx in 0..spec.len() {
        if near[idx] == u32::MAX { continue; }
        let i = near[idx] as usize;
        let dd = ndist[idx];
        let q = spec.world_of((idx % nx as usize) as u32, (idx / nx as usize) as u32);
        let side = (q.x - qs[i].x) * perps[i].x + (q.y - qs[i].y) * perps[i].y;
        let outer = side * bends[i] >= 0.0;
        let hw_c = hw_n[i] * if outer { OUTER_K } else { INNER_K };
        let gr_c = (gr_n[i] * if outer { OUTER_GRADE } else { INNER_GRADE })
            .clamp(GRADE_CLAMP.0, GRADE_CLAMP.1);
        let floor = floor_n[i];
        // wetting, decided before any cut test (a floor already below the
        // water is still under water); the tolerance is the slot's
        if dd <= hw_c + cell * 0.32 {
            creek[idx] = true;
            if surface.data[idx].is_nan() {
                *wet_cells += 1;
                surface.data[idx] = zs[i];
            } else {
                surface.data[idx] = surface.data[idx].min(zs[i]);
            }
        }
        let cand = if dd <= hw_c {
            floor
        } else {
            let dw = dd - hw_c;
            // where this bank meets the smoothed ground HERE: constant grade,
            // closing further out where the ground beside the creek is higher
            let dw_close = ((h0s[idx] - floor) / gr_c).max(0.0);
            if dw > dw_close + 2.0 * RIM_M {
                continue;                       // past the rim: untouched
            }
            if std::env::var_os("NET_DEBUG").is_some() && dd < hw_c + cell {
                demand.push(h0s[idx] - floor);
                closes.push(dw_close);
            }
            let back = math::smoothstep(DETAIL_IN.0, DETAIL_IN.1, dw / dw_close.max(1e-6));
            let mut t = floor + gr_c * dw + detail[idx] * back;
            // the window can only be reached on a side slope steeper than the
            // bank; fade to the ground over the last rim rather than step
            if dw > dw_lim - 2.0 * RIM_M {
                t += (h0[idx] - t) * math::smoothstep(dw_lim - 2.0 * RIM_M, dw_lim, dw);
            }
            course_world::ease::smin(t, h0[idx], gr_c * RIM_M)
        };
        if cand < height.data[idx] {
            height.data[idx] = cand;
            creek[idx] = true;
        }
    }
    if !demand.is_empty() {
        let pct = |v: &mut Vec<f64>, p: f64| { v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v[((v.len() - 1) as f64 * p) as usize] };
        eprintln!("  slot_lowered: demand p10/50/90 {:.2}/{:.2}/{:.2} m  close p10/50/90 {:.1}/{:.1}/{:.1} m  window {:.0} m",
                  pct(&mut demand, 0.1), pct(&mut demand, 0.5), pct(&mut demand, 0.9),
                  pct(&mut closes, 0.1), pct(&mut closes, 0.5), pct(&mut closes, 0.9), r_max as f64 * cell);
    }
}

/// Arc length, left normal over a +-5 node window, and the bend sign, for a
/// creek polyline. `side * bend >= 0` is the OUTER (cut) bank, the same
/// convention the fluvial planform hands to the incision.
fn node_frames(p: &[Vec2]) -> (Vec<f64>, Vec<Vec2>, Vec<f64>) {
    let n = p.len();
    let mut arcs = Vec::with_capacity(n);
    let mut a = 0.0;
    arcs.push(0.0);
    for i in 1..n {
        a += p[i - 1].distance(p[i]);
        arcs.push(a);
    }
    let mut perps = Vec::with_capacity(n);
    let mut bends = Vec::with_capacity(n);
    for i in 0..n {
        let (lo, hi) = (i.saturating_sub(5), (i + 5).min(n - 1));
        let tv = p[hi] - p[lo];
        let l = tv.length().max(1e-9);
        perps.push(Vec2::new(-tv.y / l, tv.x / l));
        let (lo2, hi2) = (i.saturating_sub(10), (i + 10).min(n - 1));
        let v1 = p[i] - p[lo2];
        let v2 = p[hi2] - p[i];
        bends.push(-(v1.x * v2.y - v1.y * v2.x));
    }
    (arcs, perps, bends)
}

/// ABLATION ONLY (CREEK_CARVE=slot): the flat-bottomed slot this function cut
/// until 2026-09-02 -- a 5 m footprint stamped over the textured ground.
/// Correct in section and far better than the 32 m skirt it replaced, but it
/// still REPLACES the ground rather than lowering it, so the creek came out
/// as a clean extrusion. Deleted once the incision is signed off.
fn cut_creek_slot_legacy(height: &mut Grid<f64>, water: &mut Water,
                         line: &[Vec2], bed: &[f64], width_m: f64) {
    let spec = height.spec;
    let cell = spec.cell_size;
    let hw = (width_m * 0.5).max(1.2);
    // Deeper for a wider channel, but shallow in absolute terms either way.
    let depth = 0.30 + 0.045 * width_m;
    // The rise from the flat bed to the surface, horizontally. Short, so the
    // bank is steep; a hair over one 2 m cell so it still rasterises.
    let rise = 2.4;
    // Freeboard: the bank keeps climbing past the water surface, so the
    // water's edge is a line on a slope rather than the lip of a pan.
    let free = 0.45;
    let bank = rise + 2.0;
    for (i, p) in line.iter().enumerate() {
        let z = bed[i.min(bed.len() - 1)];
        let r = ((hw + bank) / cell).ceil() as i64 + 1;
        let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
        for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
            for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                let idx = spec.index(gx as u32, gy as u32);
                let dd = spec.world_of(gx as u32, gy as u32).distance(*p);
                if dd > hw + bank {
                    continue;
                }
                // Flat bed out to `hw`, then a steep straight rise through
                // the water surface to the bank. Never a fill.
                let cand = if dd <= hw {
                    z - depth
                } else {
                    let u = ((dd - hw) / rise).min(1.0);
                    z - depth + (depth + free) * u
                };
                if cand < height.data[idx] {
                    height.data[idx] = cand;
                }
                // A tolerance on the WETTING test only; the cut geometry
                // above still uses the true half-width. A 3 m creek gives
                // hw = 1.5 m against a 2 m cell, and testing cell centres
                // against that radius alone leaves gaps -- seed 41423 came out
                // as two bodies of 8948 and 2592 m2, which is one creek in two
                // pieces, not a creek and a lake. 0.32 of a cell is the
                // smallest that held every seed together; half a cell also
                // worked but pushed the measured width a full cell over spec.
                if dd <= hw + cell * 0.32 {
                    if water.surface.data[idx].is_nan() {
                        water.lake_frac += 1.0 / spec.len() as f64;
                        water.surface.data[idx] = z;
                    } else {
                        water.surface.data[idx] = water.surface.data[idx].min(z);
                    }
                }
            }
        }
    }
}

pub fn cut_channel(height: &mut Grid<f64>, water: &mut Water, pl: &RiverPlan) {
    let spec = height.spec;
    // Width cheats slightly with the local planform (review): wider through
    // straight, long-wavelength runs, narrower in tight bends. VERY slight —
    // wet width spans ~5.2–7.0 m across the whole range.
    let curv_at = |i: usize| -> f64 {
        let a = pl.pts[i.saturating_sub(10)];
        let b = pl.pts[i];
        let c = pl.pts[(i + 10).min(pl.pts.len() - 1)];
        let v1 = Vec2::new(b.x - a.x, b.y - a.y);
        let v2 = Vec2::new(c.x - b.x, c.y - b.y);
        let dot = (v1.x * v2.x + v1.y * v2.y)
            / (v1.length().max(1e-9) * v2.length().max(1e-9));
        dot.clamp(-1.0, 1.0).acos()
    };
    for (i, q) in pl.pts.iter().enumerate() {
        let straightness = 1.0 - math::smoothstep(0.05, 0.45, curv_at(i));
        let hw_water = 2.6 + 0.9 * straightness;
        let hw_cut = 6.4 + 1.6 * straightness;
        let x0 = ((q.x - hw_cut) / spec.cell_size).floor().max(0.0) as u32;
        let x1 = ((q.x + hw_cut) / spec.cell_size).ceil().min(spec.nx as f64 - 1.0) as u32;
        let y0 = ((q.y - hw_cut) / spec.cell_size).floor().max(0.0) as u32;
        let y1 = ((q.y + hw_cut) / spec.cell_size).ceil().min(spec.ny as f64 - 1.0) as u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let w = spec.world_of(x, y);
                let dd = ((w.x - q.x).powi(2) + (w.y - q.y).powi(2)).sqrt();
                if dd >= hw_cut {
                    continue;
                }
                let li = spec.index(x, y);
                let f = math::smoothstep(0.35, 1.0, dd / hw_cut);
                let t = pl.bed[i] + f * (height.data[li] - pl.bed[i]).max(0.0);
                if height.data[li] > t {
                    let ex = height.data[li] - t;
                    height.data[li] -= ex * math::smoothstep(0.0, 1.2, ex);
                }
                if dd < hw_water {
                    let ws = pl.bed[i] + 0.4;
                    let cur = water.surface.data[li];
                    if cur.is_nan() || cur > ws {
                        water.surface.data[li] = ws;
                    }
                }
            }
        }
    }
    water.river = Some(pl.pts.clone());
}

#[cfg(test)]
mod tests {
    use crate::{build_full, record::FormClass, texture};
    use course_seed::RunIdentity;

    fn pack() -> Option<texture::PatchPack> {
        texture::PatchPack::load(std::path::Path::new(
            "../../assets/sandhills_patches_aeolian.bin")).ok()
        .or_else(|| texture::PatchPack::load(std::path::Path::new(
            "assets/sandhills_patches_aeolian.bin")).ok())
    }

    #[test]
    fn blowouts_sit_on_high_ground_and_conserve_mass() {
        let Some(p) = pack() else { return };
        let t = build_full(&RunIdentity::from_seed(3), &p, Some(FormClass::Train));
        assert!(!t.blowouts.is_empty(), "no blowouts placed");
        // mass conservation: the bowl+apron pass must not change tile volume
        // by more than a rounding sliver. (The river is a cut and the pans
        // are deflation -- both legitimately remove; blowouts must not.)
        for b in &t.blowouts {
            assert!(b.depth_m >= 2.0 - 1e-9 && b.depth_m <= 8.0 + 1e-9);
            assert!(b.radius_m >= 8.0 - 1e-9 && b.radius_m <= 30.0 + 1e-9);
        }
    }

    #[test]
    fn small_ponds_never_dominate_the_count() {
        // Seeds re-picked 2026-08-27. The water-table ramp went flat (W4), so
        // seed 9 now draws a deep table and carries 4 water bodies in total —
        // a 2-of-3 ratio over 4 bodies measures nothing. These three are
        // pond-rich under the new draw (lake_frac 0.066 / 0.032 / 0.067).
        let Some(p) = pack() else { return };
        for seed in [3u64, 11, 14] {
            let t = build_full(&RunIdentity::from_seed(seed), &p, Some(FormClass::Train));
            let spec = t.water.spec;
            // count bodies by flood fill
            let mut lab = vec![false; spec.len()];
            let mut small = 0usize;
            let mut total = 0usize;
            let mut stack = Vec::new();
            for st in 0..spec.len() {
                if !t.water.data[st].is_finite() || lab[st] {
                    continue;
                }
                let mut n = 0usize;
                stack.push(st);
                lab[st] = true;
                while let Some(i) = stack.pop() {
                    n += 1;
                    let (x, y) = ((i % spec.nx as usize) as u32, (i / spec.nx as usize) as u32);
                    for (dx, dy) in [(1i64, 0), (-1, 0), (0, 1), (0, -1)] {
                        let (a, b) = (x as i64 + dx, y as i64 + dy);
                        if a < 0 || b < 0 || a >= spec.nx as i64 || b >= spec.ny as i64 {
                            continue;
                        }
                        let j = spec.index(a as u32, b as u32);
                        if t.water.data[j].is_finite() && !lab[j] {
                            lab[j] = true;
                            stack.push(j);
                        }
                    }
                }
                total += 1;
                if (n as f64) * spec.cell_size * spec.cell_size < 2500.0 {
                    small += 1;
                }
            }
            if total > 0 {
                assert!(small * 3 <= total * 2,
                        "seed {seed}: {small}/{total} bodies are small ponds");
            }
        }
    }

    #[test]
    fn shallow_tables_make_lakes_and_deep_tables_stay_dry() {
        // The shallow example moved from seed 9 to seed 3 when the table ramp
        // went flat (W4): seed 9 now draws a deep table, which is the rule
        // working, not breaking. Seed 23 still draws a deep one.
        let Some(p) = pack() else { return };
        let wet = build_full(&RunIdentity::from_seed(3), &p, Some(FormClass::Train));
        assert!(wet.lake_frac > 0.005, "shallow-table seed made no lakes");
        let dry = build_full(&RunIdentity::from_seed(23), &p, Some(FormClass::Train));
        assert!(dry.lake_frac < 0.01, "deep-table seed made lakes");
    }

    #[test]
    fn aeolian_mode_still_grows_no_channels() {
        // The river is a COIN, not a consequence of terrain. A seed without
        // the coin must have zero flowing water -- lakes only.
        let Some(p) = pack() else { return };
        let no = build_full(&RunIdentity::from_seed(3), &p, Some(FormClass::Train));
        assert!(no.river.is_none(), "seed 3 does not draw the coin");
        // Seed 5 draws the coin with a 10.5 m table -- no lakes, so the wet
        // set is the river alone and the width proxy is honest. (The first
        // version used lake-country seed 9 and measured the lakes.)
        let yes = build_full(&RunIdentity::from_seed(5), &p, Some(FormClass::Train));
        let r = yes.river.expect("seed 5 draws the coin");
        let spec = yes.water.spec;
        let wet = (0..spec.len()).filter(|i| yes.water.data[*i].is_finite()).count();
        // Measure the polyline, do not assume its spacing. This read
        // `r.len() * 4.0` for plan_river's 4 m step; the gorge creek comes off
        // carve::beds at 6 m, which inflated the proxy by 1.5x and failed a
        // creek that was in fact 6.3 m wide.
        let len_m: f64 = r.windows(2).map(|w| w[0].distance(w[1])).sum();
        let mean_width = wet as f64 * spec.cell_size * spec.cell_size / len_m;
        assert!(mean_width < 9.0,
                "river wet width proxy {mean_width:.1} m -- creek spec is ~6 m");
    }
}

// ---------------------------------------------------------------------------
// X4 — the Carolina water: wet creeks and rare ponds
// ---------------------------------------------------------------------------

/// Water for the fluvial mode.
///
/// Carolina is far drier than Nebraska: the measured lake fraction is 0.047
/// against the Sandhills' 0.26, and the water that IS there is mostly the
/// creeks themselves — blackwater streams running in their own floors —
/// rather than interdune ponds. So this is the opposite construction to
/// `find`: the channels are wet by definition, and standing water is the
/// exception, found only where the table grazes a broad valley floor.
///
/// No drawdown field is needed here (the aeolian mode needs one because its
/// river is an intruder on a dune field). Here the creeks ARE the drainage:
/// ground near them is low because they drained it.
pub fn fluvial(rng: &mut DetRng, height: &mut Grid<f64>,
               beds: &[(Vec<Vec2>, Vec<f64>)], tiers: &[u8],
               u_field: &Grid<f64>, w_field: &Grid<f64>, d: &Descriptors,
               sections: Option<&CreekSections>) -> Water {
    let spec = height.spec;
    let mut surface = Grid::filled(spec, f64::NAN);
    let cell = spec.cell_size;

    // --- the creeks -------------------------------------------------------
    // Wet width scales with the tier: a trunk carries a few metres of water,
    // a headwater gully carries none at all (dry sand most of the year, and
    // below the extraction threshold besides).
    // THE MEANDERING-CREEK VARIANT (review 2026-08-25). In about one tile in
    // ten the trunk valley carries a creek that meanders across its own
    // floor instead of a pond sitting in the deepest part. The planform is
    // the one the Nebraska river rounds closed at 12/12: a wavelength and
    // swing drawn from RiverStyle::PASSED, applied here as an offset from
    // the trunk centre-line rather than as a free path, so the creek stays
    // in its valley. Max 4-5 m across, per the review.
    let meander = rng.next_f64() < d.p_valley_creek;
    if std::env::var("NET_DEBUG").is_ok() && meander {
        eprintln!("  MEANDER creek tile");
    }
    let mstyle = RiverStyle::PASSED[rng.below(RiverStyle::PASSED.len())];
    let m_lam = rng.range_f64(mstyle.lam.0, mstyle.lam.1);
    let m_swing = rng.range_f64(mstyle.swing.0, mstyle.swing.1);
    let m_phase = rng.range_f64(0.0, std::f64::consts::TAU);
    let m_seed = rng.next_u32();

    let mut wet_cells = 0usize;
    // Cells the FLATTENING must not touch. A meander creek is offset from
    // the trunk centre-line by up to a swing width, so a mask built from
    // that centre-line missed it entirely and the flattening pulled the
    // channel floor up toward a 26 m blur — leaving a ridge along the
    // creek's own axis flanked by the carve, which read as a beaded dark
    // line down the middle of the swale (review). The carve marks its own
    // cells instead.
    let mut creek = vec![false; spec.len()];
    // The creek's own centre-line, for the review overlay (`Water::river`).
    let mut creek_line: Option<Vec<Vec2>> = None;
    let mut creek_level: Option<Vec<f64>> = None;
    let mut creek_pooled: Option<Vec<bool>> = None;
    for (ci, (pts, bed)) in beds.iter().enumerate() {
        let tier = tiers.get(ci).copied().unwrap_or(1);
        // Only the TRUNK carries visible water. The tributaries were drawn
        // as 2-4 m ribbons and read as hairlines across the tile (review);
        // a creek that thin is a line on a map, not water on the ground.
        if tier >= 2 {
            continue;
        }
        // Review 2026-08-25: the full trunk must not read as a ribbon of
        // water. A trunk carries wet REACHES — pools and runs separated by
        // dry or braided sand — so its wetting is gated along the arc, and
        // the width stays modest but never hairline. Tributaries are wet
        // continuously; they are small enough that it reads as a creek line
        // rather than a lake.
        let hw = if meander {
            rng.range_f64(2.0, 2.5)         // 4-5 m across, per review
        } else {
            // Review: trunk water may be short in LENGTH, but it must not be
            // hairline in WIDTH. 1.8-3.0 gave a 4 m thread; a wet reach is a
            // pool, so it is widened and made correspondingly rarer.
            rng.range_f64(3.2, 5.0)         // 6.5-10 m across
        };
        let s_reach = rng.next_u32();
        // A meandering creek runs continuously; a braided trunk carries only
        // OCCASIONAL wet reaches. The old floor (-0.05..0.20) passed about
        // half the arc and printed a continuous ribbon; this passes roughly
        // a fifth, in a couple of separated runs.
        let reach_lo = if meander { -1.0 } else { rng.range_f64(0.30, 0.58) };

        // --- a trunk wet reach is a POOL, not a thread --------------------
        // Drawing the reach as a fixed-half-width ribbon gave a 5 m line: a
        // hairline, which is exactly what the review rejected. A pool is
        // instead FLOODED to a level and lets the valley floor decide its
        // own width, so it comes out short and broad — "small length and
        // width, but not hairline" — with a shoreline that belongs to the
        // ground rather than to the centre-line.
        if !meander {
            let mut a0 = 0.0f64;
            let mut run: Vec<(Vec2, f64)> = Vec::new();
            let mut flush = |run: &mut Vec<(Vec2, f64)>, wet: &mut usize| {
                let len: f64 = run.windows(2).map(|w| w[0].0.distance(w[1].0)).sum();
                if run.len() < 2 || len < 150.0 {
                    run.clear();
                    return;
                }
                // "Small in length" (review): a wet reach is a pool, not a
                // reservoir. A long run is trimmed to its DOWNSTREAM end,
                // which is where the water would actually stand.
                let mut back = 0.0f64;
                let mut cut = 0usize;
                for k in (1..run.len()).rev() {
                    back += run[k - 1].0.distance(run[k].0);
                    if back > 420.0 {
                        cut = k - 1;
                        break;
                    }
                }
                if cut > 0 {
                    run.drain(..cut);
                }
                // level set by the DOWNSTREAM lip: a pool is impounded from
                // below, so it can never be deeper than its own outlet.
                // Lowered 2026-08-27 with the wider floors. A pool is
                // impounded to a LEVEL, so the area it covers is set by the
                // floor it sits on: the same +0.75 m that made a tidy pool on
                // a 192 m floor spreads across a 274 m one. The level and the
                // reach both come down so the pool stays a pool.
                let lip = run.last().unwrap().1;
                let lvl = lip + 0.42;
                for (p, _) in run.iter() {
                    let r = (68.0 / cell).ceil() as i64;
                    let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
                    for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
                        for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                            let q = spec.world_of(gx as u32, gy as u32);
                            if q.distance(*p) > 68.0 {
                                continue;
                            }
                            let i = spec.index(gx as u32, gy as u32);
                            if height.data[i] >= lvl {
                                continue;
                            }
                            if surface.data[i].is_nan() {
                                *wet += 1;
                                surface.data[i] = lvl;
                            } else {
                                surface.data[i] = surface.data[i].max(lvl);
                            }
                        }
                    }
                }
                run.clear();
            };
            for k in 0..pts.len() {
                if k > 0 {
                    a0 += pts[k - 1].distance(pts[k]);
                }
                if course_world::noise::perlin1(a0 / 420.0, s_reach) < reach_lo {
                    flush(&mut run, &mut wet_cells);
                } else {
                    run.push((pts[k], bed[k]));
                }
            }
            flush(&mut run, &mut wet_cells);
            continue;
        }

        // ---- MEANDER CREEK: a bend-train planform, carved as a CHANNEL ----
        // 2026-09-01. Two things changed here at once, each behind an
        // ablation toggle so a bad render can be attributed (README §4
        // rule 3); both legacy branches go once the render is signed off.
        //
        //  * PLANFORM. The two-sine offset (`sine_offset_legacy`, under
        //    CREEK_PLANFORM=sine) is replaced by `planform::bend_train`: a
        //    sequence of individually drawn bends and straight runs with no
        //    carrier, each bend confined to the floor ONCE by the physical
        //    room `(0.34 - u) * w` instead of a per-sample clamp. Review,
        //    twice: "too sinusoidal". Record: docs/calibration/creek-planform.md.
        //  * CARVE. The old cut graded a soft skirt out to 32 m either side,
        //    so the creek sculpted its own valley -- the bug that sank the
        //    previous round (under CREEK_CARVE=skirt). The valley belongs to
        //    the HAND profile; the creek gets the section `cut_creek` has
        //    always cut on the aeolian side: flat bed, a steep bank that
        //    climbs until it meets the ground, and NOTHING beyond.
        //
        // The five planform draws above are consumed exactly as before
        // (`mstyle` is drawn and unused now); `bend_train` takes no draws.
        let legacy_planform = std::env::var("CREEK_PLANFORM").map(|v| v == "sine").unwrap_or(false);
        let carve_mode = std::env::var("CREEK_CARVE").unwrap_or_default();
        let legacy_carve = carve_mode == "skirt";
        // CREEK_CARVE=none: the planform is built and reported but nothing
        // is cut or wetted -- the review wants the ground WITHOUT the creek,
        // with the creek's line drawn over it, to judge placement before
        // the section (owner, 2026-09-01).
        let no_carve = carve_mode == "none";
        let _ = mstyle;
        let (qs, mut zs, arcs, perps, bends) = if legacy_planform {
            sine_offset_legacy(pts, bed, cell, m_lam, m_swing, m_phase, m_seed, u_field)
        } else {
            let room = |q: Vec2| (0.34 - u_field.bilinear(q)).max(0.0) * w_field.bilinear(q);
            let prm = crate::planform::Params::from_draws(m_lam, m_swing, m_phase, 2.0 * hw);
            let pf = crate::planform::bend_train(pts, crate::planform::Flow::MouthFirst,
                                                 &prm, m_seed, &room);
            // the bed follows the STATION the node came from, so it rises
            // with the index exactly as the trunk bed does
            let zs = crate::planform::rekey(bed, pts, &pf.t);
            let n = pf.p.len();
            let mut arcs = Vec::with_capacity(n);
            let mut a = 0.0;
            arcs.push(0.0);
            for i in 1..n {
                a += pf.p[i - 1].distance(pf.p[i]);
                arcs.push(a);
            }
            let mut perps = Vec::with_capacity(n);
            let mut bends = Vec::with_capacity(n);
            for i in 0..n {
                let lo = i.saturating_sub(5);
                let hi = (i + 5).min(n - 1);
                let tv = pf.p[hi] - pf.p[lo];
                let l = tv.length().max(1e-9);
                perps.push(Vec2::new(-tv.y / l, tv.x / l));
                // bend direction from the signed turn over +-10 m: a left
                // turn has its centre on the left, so the OUTER (cut) bank is
                // on the right. `bends` carries the sign such that
                // `side * bends >= 0` is the outer bank, as `off` did.
                let lo2 = i.saturating_sub(10);
                let hi2 = (i + 10).min(n - 1);
                let v1 = pf.p[i] - pf.p[lo2];
                let v2 = pf.p[hi2] - pf.p[i];
                bends.push(-(v1.x * v2.y - v1.y * v2.x));
            }
            (pf.p, zs, arcs, perps, bends)
        };
        // Ground-following bed with a CUT CAP, and the bed RISES with the
        // index because index 0 is the mouth (`carve::beds` builds every
        // channel mouth-first; `zs` is that bed interpolated along the path).
        //
        // This constraint used to be `.min(zs[i - 1])`, which forced the bed
        // to FALL going upstream -- backwards. It dragged the whole profile
        // down to the mouth elevation, and the note that used to sit here
        // blamed monotonicity for the resulting trench ("strict monotonicity
        // ... every later point had to cut metres ... 510% slopes"). It was
        // the direction, not the monotonicity: measured on seed 500030,
        // descending-upstream needs a median cut of 8.25 m and up to 16.6,
        // while rising-upstream needs 1.08 m and 7.2. The CUT_CAP override
        // then had to fight it, and what came out was a ground-following
        // ditch -- 46.5% of Carolina creek cells sat impounded behind a ridge
        // in their own water surface, with up to 5.93 m of climb.
        //
        // Order matters: the monotone floor is applied first, the cut cap
        // second. The cap only ever RAISES, so it cannot reintroduce a fall.
        const CUT_CAP: f64 = 2.2;
        let mut gr: Vec<f64> = Vec::with_capacity(qs.len());
        for i in 0..qs.len() {
            gr.push(height.bilinear(qs[i]));
        }
        zs[0] = zs[0].min(gr[0] - 0.35);
        for i in 1..zs.len() {
            let want = zs[i].min(gr[i] - 0.35).max(zs[i - 1]);
            zs[i] = want.max(gr[i] - CUT_CAP);
        }
        // The invariant, asserted where it is established. This is a few
        // thousand comparisons against a 1.6 s tile build, and the bug it
        // guards against shipped: an inverted comparison here is invisible in
        // every render and only shows up as water that cannot leave.
        for i in 1..zs.len() {
            debug_assert!(zs[i] >= zs[i - 1] - 1e-9,
                "creek bed falls upstream at {i}: {} -> {}", zs[i - 1], zs[i]);
            assert!(zs[i] >= zs[i - 1] - 1e-6,
                "creek bed falls upstream at {i}: {} -> {}", zs[i - 1], zs[i]);
        }
        // The cut is taken against a SNAPSHOT of the ground and combined by
        // MIN, not applied in sequence: path points sit ~1 m apart, so each
        // cell is visited by several of them, and a running subtraction
        // compounded those visits into a black slot down the centre-line
        // (review). Reading h0 makes the result order-independent.
        creek_line = Some(qs.clone());
        if no_carve {
            continue;
        }
        let h0: Vec<f64> = height.data.clone();
        let s_d = m_seed ^ 0x85EB_CA6B;
        let s_h = m_seed ^ 0xC2B2_AE35;
        // --- the incision: LOWER the ground, do not replace it -------------
        // Section round 2 (2026-09-02). Round 1 stamped an analytic profile
        // over the textured ground (`height = min(h0, profile)`), and inside
        // its footprint the 2 m texture was gone: "a constant cutout without
        // texture where the creek is" (owner). The gorge learned the same
        // lesson at 8 m (`gorge.rs:379-383`): lower the surface, do not
        // replace it, and add the detail back across the bank.
        //
        // Measured (tools/aeolian/creek_sections.py, 13,762 resolved real
        // transects over 35 sandhills_nc tiles): a real creek section at 2 m
        // is a BROAD RAGGED V -- rim 1.0 m (p10/90 0.36/2.05) at 16-24 m,
        // closure width p50 16.5 m, symmetric, depth decorrelating along the
        // stream in ~40 m -- with no flat bed and no steep bank the DTM can
        // see. So the bank SHAPE comes from that measurement
        // (`CreekSections`, `assets/sandhills_creek_sections.txt`), walked
        // along the creek at the measured correlation length, and the ground
        // keeps its texture: the section scales the gap between the SMOOTHED
        // ground and the bed, and the texture residual returns over the
        // first part of the bank. Only the wet floor is level (slightly U),
        // and the monotone bed above is untouched: the down-elevation path
        // is exactly what it was.
        //
        // Ablations: CREEK_CARVE=section keeps the round-1 stamp; CREEK_BANK
        // (0..1) pulls the bank width from the corpus closure width toward
        // the round-1 ramp width. Zero draws in any branch.
        let bank_steep = std::env::var("CREEK_BANK").ok()
            .and_then(|v| v.parse::<f64>().ok()).unwrap_or(BANK_STEEPNESS).clamp(0.0, 1.0);
        if legacy_carve {
            for i in 0..qs.len() {
                let (p, z, a) = (qs[i], zs[i], arcs[i]);
                let cut = (0.30 + 0.20 * course_world::noise::perlin1(a / 165.0, s_d)).max(0.12);
                let hwi = (hw * (1.0 + 0.28 * course_world::noise::perlin1(a / 140.0, s_h)))
                    .clamp(1.4, 3.4);
                skirt_carve_legacy(height, &h0, &mut surface, &mut creek, &mut wet_cells,
                                   p, z, a, perps[i], bends[i], cut, hwi, m_seed);
            }
            continue;
        }
        if carve_mode == "section" {
            section_carve_round1(height, &h0, &mut surface, &mut creek, &mut wet_cells,
                                 &qs, &zs, &arcs, &perps, &bends, hw, m_seed);
            continue;
        }
        if carve_mode == "incise" {
            // the round-2 corpus incision (approved on the 20-tile gallery,
            // then found to stamp per-node bowls): kept as the ablation
            let builtin;
            let sec: &CreekSections = match sections {
                Some(s) => s,
                None => { builtin = CreekSections::builtin(); &builtin }
            };
            incise(height, &h0, &mut surface, &mut creek, &mut wet_cells,
                   &qs, &zs, &arcs, &perps, &bends,
                   &Incision { sections: sec, bank_steep, hw, hw_clamp: (1.4, 3.4),
                               cut_base: 0.30, salt: m_seed });
            continue;
        }
        if carve_mode == "lowered" {
            slot_lowered(height, &h0, &mut surface, &mut creek, &mut wet_cells,
                         &qs, &zs, &arcs, &perps, &bends,
                         &SlotLowered { hw, hw_clamp: (1.4, 3.4), depth: 0.30,
                                        rise_m: 2.4, free_m: 0.45, salt: m_seed });
            continue;
        }
        if carve_mode == "delta" {
            let builtin;
            let sec: &CreekSections = match sections {
                Some(s) => s,
                None => { builtin = CreekSections::builtin(); &builtin }
            };
            creek_level = Some(delta_carve(height, &h0, &mut surface, &mut creek, &mut wet_cells,
                        &qs, &arcs, &perps, &bends,
                        &DeltaCarve { sections: sec, hw, hw_clamp: (1.4, 3.4), wet_d: 0.30,
                                      salt: m_seed, mouth_first: zs.first() <= zs.last() }));
            continue;
        }
        // The crease: a narrow irregular incision at the bottom of a floor
        // that the valley stage already slopes toward it (owner, 2026-09-02).
        let (lv_c, pooled_c) = creek_crease(height, &h0, &mut surface, &mut creek, &mut wet_cells,
                    &qs, &arcs, &perps, &bends,
                    // Measured against the corpus section through the same
                    // pipeline: 0.55 m / 2.2 m rose 0.58 m by 4 m where the
                    // real creek rises 0.12; shallower and wider-banked.
                    &CreekCrease { hw, hw_clamp: (1.2, 3.6), depth: 0.35, bank_m: 4.5,
                                   salt: m_seed, mouth_first: zs.first() <= zs.last(),
                                   cut_cap: 6.0 });
        creek_level = Some(lv_c);
        creek_pooled = Some(pooled_c);
    }

    // --- the water TABLE is retired ---------------------------------------
    // It flooded every floor cell under a tile-wide 6th-percentile level, so
    // the whole lower trunk went wet and grew a finger up each tributary
    // mouth — the ragged full-trunk ribbon of the 2026-08-25 review, and the
    // real source of the "hairlines connected to bigger lakes". A flat level
    // against a graded floor cannot help but do this: the floor falls along
    // the valley, so ONE level always drowns the downstream half. Standing
    // water now comes only from closed depressions (below), which is where
    // it comes from on the real tiles.

    // --- PONDS IN CLOSED DEPRESSIONS ---------------------------------------
    // Real Carolina interfluves DO hold standing water away from the trunk:
    // measured over 8 kept tiles, closed depressions cover 0.24% of a tile
    // on the interfluves alone (up to 0.68%), reaching 2.3 ha and 2.7 m
    // deep. That is bay country — a bay floor is exactly such a pit — so
    // rather than placing ponds, we FIND them: fill the surface, and
    // whatever the fill had to raise is a basin that would hold water.
    //
    // Done at 8 m: a pond smaller than a few cells is not a pond, and the
    // fill is far cheaper there.
    let step = (8.0 / cell).round().max(1.0) as usize;
    let (nx8, ny8) = ((spec.nx as usize).div_ceil(step), (spec.ny as usize).div_ceil(step));
    let mut z8 = vec![0.0f64; nx8 * ny8];
    for y in 0..ny8 {
        for x in 0..nx8 {
            let sx = (x * step).min(spec.nx as usize - 1) as u32;
            let sy = (y * step).min(spec.ny as usize - 1) as u32;
            z8[y * nx8 + x] = height.data[spec.index(sx, sy)];
        }
    }
    // Planchon-Darboux: start high everywhere but the border, then let the
    // surface drain down to its neighbours until nothing moves.
    let hi = z8.iter().cloned().fold(f64::MIN, f64::max) + 10.0;
    let mut f: Vec<f64> = (0..nx8 * ny8)
        .map(|i| {
            let (x, y) = (i % nx8, i / nx8);
            if x == 0 || y == 0 || x == nx8 - 1 || y == ny8 - 1 { z8[i] } else { hi }
        })
        .collect();
    let eps = 1e-4;
    for _ in 0..200 {
        let mut moved = false;
        for pass in 0..2 {
            let order: Vec<usize> = if pass == 0 {
                (0..nx8 * ny8).collect()
            } else {
                (0..nx8 * ny8).rev().collect()
            };
            for i in order {
                if f[i] <= z8[i] {
                    continue;
                }
                let (x, y) = ((i % nx8) as i64, (i / nx8) as i64);
                let mut lowest = f64::MAX;
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (a, b) = (x + dx, y + dy);
                    if a >= 0 && b >= 0 && a < nx8 as i64 && b < ny8 as i64 {
                        lowest = lowest.min(f[b as usize * nx8 + a as usize]);
                    }
                }
                let cand = (lowest + eps).max(z8[i]);
                if cand < f[i] - 1e-9 {
                    f[i] = cand;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
    // A pit is only a pond if it is both DEEP enough and BIG enough. Without
    // the area test the fill returns every texture-scale hollow and the tile
    // floods: measured, 7-11% of the tile against real tiles' 0.5-2.5% of
    // closed depression at the same depth threshold.
    // Raised 2026-08-27 with the wider valley floors. Widening the floor to
    // the corpus 274 m left far more flat low ground, and a fill-based pond
    // test finds every shallow dip in it: measured wet fraction went to
    // 1.4-5.0% against a corpus 0.5-2.5%. The gate is a DEPTH, so it is the
    // right thing to raise — a 1 m dip on a 270 m floodplain is damp ground,
    // not a lake.
    let pond_min = rng.range_f64(1.55, 2.20);
    let mut pit = vec![false; nx8 * ny8];
    for i in 0..nx8 * ny8 {
        pit[i] = f[i] - z8[i] >= pond_min;
    }
    // connected-component area filter, 4-neighbour flood
    let min_cells = 190usize;                    // ≈ 1.2 ha at 8 m
    let mut keep = vec![false; nx8 * ny8];
    let mut seen = vec![false; nx8 * ny8];
    for start in 0..nx8 * ny8 {
        if !pit[start] || seen[start] {
            continue;
        }
        let mut stack = vec![start];
        let mut comp = Vec::new();
        seen[start] = true;
        while let Some(k) = stack.pop() {
            comp.push(k);
            let (x, y) = ((k % nx8) as i64, (k / nx8) as i64);
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (a, b) = (x + dx, y + dy);
                if a < 0 || b < 0 || a >= nx8 as i64 || b >= ny8 as i64 {
                    continue;
                }
                let m = b as usize * nx8 + a as usize;
                if pit[m] && !seen[m] {
                    seen[m] = true;
                    stack.push(m);
                }
            }
        }
        // Where a pit sits matters as much as its size. Real interfluve
        // depressions are RARE — 0.05-0.68% of a tile against 0.48-2.46%
        // for the tile as a whole — so upland pits have to clear a much
        // higher bar than valley ones, which is also what a bay floor
        // does: large and properly basined, not a hollow in the fabric.
        if comp.len() < min_cells {
            continue;
        }
        let mut u_sum = 0.0;
        let mut deepest = 0.0f64;
        for &k in &comp {
            let (bx, by) = (k % nx8, k / nx8);
            let wx = bx as f64 * cell * step as f64;
            let wy = by as f64 * cell * step as f64;
            u_sum += u_field.bilinear(Vec2::new(wx, wy));
            deepest = deepest.max(f[k] - z8[k]);
        }
        let u_mean = u_sum / comp.len() as f64;
        let ok = if u_mean < 0.55 {
            true                                   // valley floor: ordinary
        } else {
            comp.len() >= 230 && deepest >= 1.3    // upland: ≈1.5 ha and deep
        };
        if ok {
            for k in comp {
                keep[k] = true;
            }
        }
    }
    // The pond LEVEL is interpolated back to 2 m, not sampled per 8 m cell:
    // nearest-cell sampling gave the shorelines an 8 m staircase.
    let lvl_at = |wx: f64, wy: f64| -> f64 {
        let gx = (wx / (cell * step as f64)).clamp(0.0, (nx8 - 1) as f64);
        let gy = (wy / (cell * step as f64)).clamp(0.0, (ny8 - 1) as f64);
        let (x0, y0) = (gx.floor() as usize, gy.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(nx8 - 1), (y0 + 1).min(ny8 - 1));
        let (tx, ty) = (gx - x0 as f64, gy - y0 as f64);
        let a = f[y0 * nx8 + x0] * (1.0 - tx) + f[y0 * nx8 + x1] * tx;
        let b = f[y1 * nx8 + x0] * (1.0 - tx) + f[y1 * nx8 + x1] * tx;
        a * (1.0 - ty) + b * ty
    };
    // Ponds belong OFF the trunk. Filling depressions along the main valley
    // floor is what turned the trunk into one long lake (review); the trunk
    // gets its wet reaches and nothing more.
    let mut trunkzone = vec![false; spec.len()];
    for (ci, (pts, _)) in beds.iter().enumerate() {
        if tiers.get(ci).copied().unwrap_or(1) != 1 {
            continue;
        }
        for p in pts {
            let r = (95.0 / cell).ceil() as i64;
            let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
            for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
                for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                    let q = spec.world_of(gx as u32, gy as u32);
                    if q.distance(*p) <= if meander { 150.0 } else { 95.0 } {
                        trunkzone[spec.index(gx as u32, gy as u32)] = true;
                    }
                }
            }
        }
    }
    let mut pond = vec![false; spec.len()];
    let mut pond_lvl = vec![0.0f64; spec.len()];
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let i = spec.index(x, y);
            if trunkzone[i] {
                continue;
            }
            let (bx, by) = ((x as usize / step).min(nx8 - 1), (y as usize / step).min(ny8 - 1));
            let j = by * nx8 + bx;
            if !keep[j] {
                continue;
            }
            let wp = spec.world_of(x, y);
            let lvl = lvl_at(wp.x, wp.y);
            if height.data[i] < lvl {
                pond[i] = true;
                pond_lvl[i] = lvl;
            }
        }
    }

    // --- OPEN the pond mask ------------------------------------------------
    // Filled depressions grow hairline fingers up every little draw, and
    // some of them hang off otherwise good lakes (review). A morphological
    // OPENING — erode, then dilate by the same radius — deletes anything
    // narrower than the kernel and leaves everything wider untouched, which
    // is exactly the distinction wanted, and it costs two passes rather than
    // a boundary analysis. The CREEK is not opened: it is legitimately
    // narrower than this and is added afterwards.
    let orad = 4i64;                             // removes < ~16 m of width
    let mut eroded = vec![false; spec.len()];
    for y in 0..spec.ny as i64 {
        for x in 0..spec.nx as i64 {
            let mut all = true;
            'e: for dy in -orad..=orad {
                for dx in -orad..=orad {
                    if dx * dx + dy * dy > orad * orad {
                        continue;
                    }
                    let (a, b) = (x + dx, y + dy);
                    if a < 0 || b < 0 || a >= spec.nx as i64 || b >= spec.ny as i64
                        || !pond[spec.index(a as u32, b as u32)]
                    {
                        all = false;
                        break 'e;
                    }
                }
            }
            eroded[spec.index(x as u32, y as u32)] = all;
        }
    }
    let mut opened = vec![false; spec.len()];
    for y in 0..spec.ny as i64 {
        for x in 0..spec.nx as i64 {
            if !eroded[spec.index(x as u32, y as u32)] {
                continue;
            }
            for dy in -orad..=orad {
                for dx in -orad..=orad {
                    if dx * dx + dy * dy > orad * orad {
                        continue;
                    }
                    let (a, b) = (x + dx, y + dy);
                    if a >= 0 && b >= 0 && a < spec.nx as i64 && b < spec.ny as i64 {
                        opened[spec.index(a as u32, b as u32)] = true;
                    }
                }
            }
        }
    }

    // --- COMPACTNESS: a pond is a basin, not a contour worm ----------------
    // Opening removes what is thin everywhere, but a 25 m x 350 m ribbon
    // wrapped around a hillside survives it and still is not a pond: it is a
    // level line grazing a slope. The shape test is area against the biggest
    // disk that fits inside — pi for a circle, ~4pi for a 4:1 ellipse, 30+
    // for the worms on seed 105 — which separates basins from slope grazes
    // without touching a compact lake of any size.
    {
        let big = 1e9f64;
        let mut dt: Vec<f64> = opened.iter().map(|&w| if w { big } else { 0.0 }).collect();
        let (dx1, dx2) = (cell, cell * std::f64::consts::SQRT_2);
        for y in 0..spec.ny as i64 {
            for x in 0..spec.nx as i64 {
                let i = spec.index(x as u32, y as u32);
                if dt[i] == 0.0 {
                    continue;
                }
                let mut m = dt[i];
                for (ox, oy, w) in [(-1i64, 0i64, dx1), (0, -1, dx1), (-1, -1, dx2), (1, -1, dx2)] {
                    let (a, b) = (x + ox, y + oy);
                    if a >= 0 && b >= 0 && a < spec.nx as i64 && b < spec.ny as i64 {
                        m = m.min(dt[spec.index(a as u32, b as u32)] + w);
                    }
                }
                dt[i] = m;
            }
        }
        for y in (0..spec.ny as i64).rev() {
            for x in (0..spec.nx as i64).rev() {
                let i = spec.index(x as u32, y as u32);
                if dt[i] == 0.0 {
                    continue;
                }
                let mut m = dt[i];
                for (ox, oy, w) in [(1i64, 0i64, dx1), (0, 1, dx1), (1, 1, dx2), (-1, 1, dx2)] {
                    let (a, b) = (x + ox, y + oy);
                    if a >= 0 && b >= 0 && a < spec.nx as i64 && b < spec.ny as i64 {
                        m = m.min(dt[spec.index(a as u32, b as u32)] + w);
                    }
                }
                dt[i] = m;
            }
        }
        let mut seen = vec![false; spec.len()];
        for start in 0..spec.len() {
            if !opened[start] || seen[start] {
                continue;
            }
            let mut stack = vec![start];
            let mut comp = Vec::new();
            seen[start] = true;
            let mut rmax = 0.0f64;
            while let Some(k) = stack.pop() {
                comp.push(k);
                rmax = rmax.max(dt[k]);
                let (x, y) = ((k % spec.nx as usize) as i64, (k / spec.nx as usize) as i64);
                for (ox, oy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (a, b) = (x + ox, y + oy);
                    if a < 0 || b < 0 || a >= spec.nx as i64 || b >= spec.ny as i64 {
                        continue;
                    }
                    let m = spec.index(a as u32, b as u32);
                    if opened[m] && !seen[m] {
                        seen[m] = true;
                        stack.push(m);
                    }
                }
            }
            // rmax is ALSO the size floor: a pond that cannot hold a 36 m
            // disk is a puddle, and a worm chopped into compact fragments by
            // the opening becomes exactly such a string of beads — which is
            // what seed 105 printed across its north-east interfluve.
            let area = comp.len() as f64 * cell * cell;
            if rmax < 18.0 || area < 6000.0 || area > 13.0 * rmax * rmax {
                for k in comp {
                    opened[k] = false;
                }
            }
        }
    }

    // --- a meander valley keeps AT MOST ONE lake on its creek --------------
    // Review: where the creek runs, one lake on it is a feature and three are
    // a chain of ponds. Components touching the creek are found, the largest
    // is kept, the rest are dropped; lakes away from the creek are untouched.
    if meander {
        let mut lab = vec![0u32; spec.len()];
        let mut next = 1u32;
        let mut touch: Vec<(u32, usize)> = Vec::new();
        for start in 0..spec.len() {
            if !opened[start] || lab[start] != 0 {
                continue;
            }
            let mut stack = vec![start];
            lab[start] = next;
            let (mut size, mut hits) = (0usize, false);
            while let Some(k) = stack.pop() {
                size += 1;
                if !surface.data[k].is_nan() {
                    hits = true;             // creek water already here
                }
                let (x, y) = ((k % spec.nx as usize) as i64, (k / spec.nx as usize) as i64);
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (a, b) = (x + dx, y + dy);
                    if a < 0 || b < 0 || a >= spec.nx as i64 || b >= spec.ny as i64 {
                        continue;
                    }
                    let m = spec.index(a as u32, b as u32);
                    if opened[m] && lab[m] == 0 {
                        lab[m] = next;
                        stack.push(m);
                    }
                }
            }
            if hits {
                touch.push((next, size));
            }
            next += 1;
        }
        if touch.len() > 1 {
            touch.sort_by_key(|(_, sz)| std::cmp::Reverse(*sz));
            for (id, _) in touch.into_iter().skip(1) {
                for i in 0..spec.len() {
                    if lab[i] == id {
                        opened[i] = false;
                    }
                }
            }
        }
    }

    if settle_on() {
        // One flat level per body, settled to what its basin holds (see
        // `settle`): the opening and the trunk keep-out clipped the outline
        // and left ground below the level dry beside it. The creek and the
        // keep-out stay off limits, so a pond that would drain into the
        // creek drops to the ground where it would.
        // the creek drains: a pond whose basin opens onto it drops to the
        // ground where it would spill in
        let no_wall = vec![false; spec.len()];
        let fill = fill_levels(&spec, &height.data, &creek, &no_wall);
        let keep = creek_banks(&spec, &creek);
        let mut lab = vec![false; spec.len()];
        for start in 0..spec.len() {
            if !opened[start] || lab[start] {
                continue;
            }
            let mut stack = vec![start];
            lab[start] = true;
            let mut comp: Vec<usize> = Vec::new();
            while let Some(k) = stack.pop() {
                comp.push(k);
                let (x, y) = ((k % spec.nx as usize) as i64, (k / spec.nx as usize) as i64);
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (a, b) = (x + dx, y + dy);
                    if a < 0 || b < 0 || a >= spec.nx as i64 || b >= spec.ny as i64 {
                        continue;
                    }
                    let m = spec.index(a as u32, b as u32);
                    if opened[m] && !lab[m] {
                        lab[m] = true;
                        stack.push(m);
                    }
                }
            }
            let mut lv: Vec<f64> = comp.iter().map(|k| pond_lvl[*k]).collect();
            lv.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let want = lv[lv.len() / 2];
            let (mut level, mut cells) = settle(&spec, &height.data, &fill, &comp, want, &no_wall);
            let body: Vec<usize> = comp.iter().copied().filter(|k| height.data[*k] < want).collect();
            let dammed = cells.len() * 2 < body.len();
            if dammed {
                let mut w = vec![false; spec.len()];
                for &k in &body {
                    w[k] = true;
                }
                berm(&spec, &mut height.data, &w, want, &keep);
                level = want;
                cells = body;
            }
            if std::env::var("NET_DEBUG").is_ok() {
                eprintln!("  pond settle: {} -> {} cells, level {:.2} -> {:.2}{}", comp.len(), cells.len(), want, level,
                          if dammed { " (dammed)" } else { "" });
            }
            for i in cells {
                if surface.data[i].is_nan() {
                    wet_cells += 1;
                    surface.data[i] = level;
                } else {
                    surface.data[i] = surface.data[i].max(level);
                }
            }
        }
    } else {
        for i in 0..spec.len() {
            if !opened[i] {
                continue;
            }
            if surface.data[i].is_nan() {
                wet_cells += 1;
                surface.data[i] = pond_lvl[i];
            } else {
                surface.data[i] = surface.data[i].max(pond_lvl[i]);
            }
        }
    }

    // LAKE COUNT CAP (review, 2026-08-28): at most MAX_LAKES standing bodies
    // per tile, largest kept.
    //
    // Applied to the FINISHED surface, not to any one producer. Carolina fills
    // standing water from two independent places -- valley pools impounded
    // behind a downstream lip, and ponds found in closed depressions -- and a
    // cap on either alone misses the other: capping `opened` measured "1 body
    // before cap" on seed 500063 while the render showed nine pools strung
    // along the valleys. Creek cells are exempt by the `creek` mask, so a
    // wet channel is never counted as a lake nor culled as one.
    {
        const MAX_LAKES: usize = 8;
        const MIN_LAKE_GAP_M: f64 = 50.0;
        let mut lab = vec![0u32; spec.len()];
        let mut sizes: Vec<(usize, u32)> = Vec::new();
        let mut nid = 0u32;
        let mut stack: Vec<usize> = Vec::new();
        for start in 0..spec.len() {
            if surface.data[start].is_nan() || creek[start] || lab[start] != 0 {
                continue;
            }
            nid += 1;
            lab[start] = nid;
            stack.clear();
            stack.push(start);
            let mut count = 0usize;
            while let Some(i) = stack.pop() {
                count += 1;
                let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
                for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                    let (jx, jy) = (x + dx, y + dy);
                    if jx < 0 || jy < 0 || jx >= spec.nx as i64 || jy >= spec.ny as i64 {
                        continue;
                    }
                    let j = spec.index(jx as u32, jy as u32);
                    if !surface.data[j].is_nan() && !creek[j] && lab[j] == 0 {
                        lab[j] = nid;
                        stack.push(j);
                    }
                }
            }
            sizes.push((count, nid));
        }
        let drop = pick_spaced(&spec, &lab, &mut sizes, MAX_LAKES, MIN_LAKE_GAP_M);
        for i in 0..spec.len() {
            if lab[i] != 0 && drop.contains(&lab[i]) {
                surface.data[i] = f64::NAN;
                wet_cells -= 1;
            }
        }
    }

    // --- FLATTEN THE GROUND UNDER STANDING WATER ---------------------------
    // A pond floor is a depositional surface: fines settle out and level it.
    // Sloping ground beneath a flat water plane reads wrong, and it is wrong
    // — so anything under standing water is levelled to a shallow, near-flat
    // bed. The CREEKS are exempt: a creek bed keeps its gradient, which is
    // what makes it a creek.
    for (ci, (pts, _)) in beds.iter().enumerate() {
        let tier = tiers.get(ci).copied().unwrap_or(1);
        if tier >= 4 {
            continue;
        }
        for p in pts {
            let r = 4;
            let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
            for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
                for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                    creek[spec.index(gx as u32, gy as u32)] = true;
                }
            }
        }
    }
    // Level the ROUGHNESS, not the basin. Pulling the floor toward a fixed
    // offset below the water plane filled the bays in — a bay is a basin,
    // and its depth is the point of it. Blending toward a heavily smoothed
    // copy of the ground removes the fabric a pond floor would not carry
    // while keeping the hollow it sits in.
    let mut flat = height.data.clone();
    {
        let r = (26.0 / cell).round() as i64;      // ~26 m
        for _ in 0..2 {
            for axis in 0..2 {
                let src = flat.clone();
                for y in 0..spec.ny as i64 {
                    for x in 0..spec.nx as i64 {
                        let (mut acc, mut n) = (0.0, 0.0);
                        for k in -r..=r {
                            let (px, py) = if axis == 0 {
                                ((x + k).clamp(0, spec.nx as i64 - 1), y)
                            } else {
                                (x, (y + k).clamp(0, spec.ny as i64 - 1))
                            };
                            acc += src[spec.index(px as u32, py as u32)];
                            n += 1.0;
                        }
                        flat[spec.index(x as u32, y as u32)] = acc / n;
                    }
                }
            }
        }
    }
    for i in 0..spec.len() {
        if surface.data[i].is_nan() || creek[i] {
            continue;
        }
        // deeper water = quieter floor
        // Feathered by depth: at the shoreline the levelling is weak and it
        // strengthens with submergence, so no flat-edged strip prints inside
        // the water (review: a thinner flat section showed inside the trunk).
        let sub = (surface.data[i] - height.data[i]).max(0.0);
        let k = 0.9 * math::smoothstep(0.05, 1.1, sub);
        height.data[i] += (flat[i] - height.data[i]) * k;
        // and never leave ground standing above its own water surface
        if height.data[i] > surface.data[i] - 0.05 {
            height.data[i] = surface.data[i] - 0.05;
        }
    }

    // --- IMPOUNDMENTS ON THE TRUNK (measured, 2026-08-29) ------------------
    // The corpus says our Carolina water was ~4x too dry: real Sandhills-NC
    // courses carry a median 1.7% water INSIDE the course polygon (p75 3.5%),
    // 93% of them have at least one body, typically 2 bodies of 0.2-4 ha
    // (Asheboro 12.9%, Pinehurst No.7 7.5%). Ours ran 0.4% with a single
    // body. The missing feature is not scattered ponds: it is the DAMMED
    // CREEK REACH -- a flat pond in the valley floor with the drainage
    // running through it, plus irrigation ponds cut off the same line.
    //
    // This is the sanctioned exception to `clear_lakes_near` (2026-08-27,
    // "no standing lakes on the valley floor beside running water"). That
    // rule stays for OFF-CHANNEL bodies, which is what it was written for;
    // an ON-CHANNEL impoundment is the region's commonest water feature and
    // outlawing it was a measurement error, corrected here with the owner's
    // sign-off. Ponds are built from the creek's own bed, which cut_creek
    // has already forced monotone, so a pond can never run uphill.
    //
    // Draws are appended at the TAIL of the WATER stream per the transcript
    // discipline: every earlier draw keeps its position and its value.
    let mut dam_block = vec![false; spec.len()];
    if let Some((pts, bed)) = beds.iter().zip(tiers.iter())
        .find(|(_, t)| **t == 1).map(|(b, _)| b)
    {
        let n_pond = {
            let u = rng.next_f64();
            if u < 0.18 { 0 } else if u < 0.62 { 1 } else if u < 0.92 { 2 } else { 3 }
        };
        // Every draw first, in the order the transcript has always had them,
        // so the dams can all stand before any pond settles.
        // The pond stands `rise` above the WATER at the dam. It was defined
        // over the graded bed, which the creek's level no longer follows
        // (the bed lies metres below the floor the crease cuts into), so a
        // pond could sit 12 m above the creek that fed it and the backwater
        // then drowned the valley. With a creek line and its level the dam
        // is placed on the line; without them the bed is what there is.
        let mut plans: Vec<(usize, usize, f64, f64)> = Vec::new();   // (up, di, lvl, cap)
        let on_line = match (creek_line.as_ref(), creek_level.as_ref()) {
            (Some(l), Some(lv)) if l.len() > 2 && lv.len() == l.len() && settle_on() => Some((l, lv)),
            _ => None,
        };
        // Which way is upstream along the trunk stations? The bed rises
        // that way. The old walk assumed lower index = upstream and on a
        // mouth-first trunk flooded the whole reach DOWN to the mouth
        // (seed sets of a million cells on the mixed run).
        let pts_up: i64 = if bed[0] > bed[bed.len() - 1] { -1 } else { 1 };
        for k in 0..n_pond {
            // dam station: spread along the trunk, never in the last eighth
            let frac = rng.range_f64(0.12 + 0.24 * k as f64, 0.32 + 0.24 * k as f64);
            let di = ((pts.len() - 1) as f64 * frac.min(0.88)) as usize;
            // pond depth over the water at the dam: 0.8-2.6 m -> 0.2-4 ha
            let rise = rng.range_f64(0.8, 2.6);
            let (up, lvl) = match on_line {
                Some((l, lv)) => {
                    let pd = pts[di.min(pts.len() - 1)];
                    let kd = (0..l.len()).min_by(|a, b| l[*a].distance(pd).partial_cmp(&l[*b].distance(pd)).unwrap()).unwrap();
                    let lvl = lv[kd] + rise;
                    // flood UPSTREAM along the line while its water stays below level
                    let upstream: i64 = if lv.first() <= lv.last() { 1 } else { -1 };
                    let mut ku = kd as i64;
                    while ku + upstream >= 0 && ku + upstream < l.len() as i64
                        && lv[(ku + upstream) as usize] < lvl {
                        ku += upstream;
                    }
                    // the nearest trunk station on the upstream side closes the reach
                    let pu = l[ku as usize];
                    let side: Vec<usize> = if pts_up < 0 { (0..=di).collect() } else { (di..pts.len()).collect() };
                    let up = side.into_iter().min_by(|a, b| pts[*a].distance(pu).partial_cmp(&pts[*b].distance(pu)).unwrap()).unwrap();
                    (up, lvl)
                }
                None => {
                    let lvl = bed[di.min(bed.len() - 1)] + rise;
                    let mut up = di as i64;
                    while up + pts_up >= 0 && up + pts_up < bed.len() as i64
                        && bed[(up + pts_up) as usize] < lvl {
                        up += pts_up;
                    }
                    (up as usize, lvl)
                }
            };
            if std::env::var("NET_DEBUG").is_ok() {
                eprintln!("  impoundment plan: di {di} up {up} lvl {lvl:.2} rise {rise:.2} on_line {}", on_line.is_some());
            }
            if (di as i64 - up as i64).unsigned_abs() < 6 {
                continue;             // too short to read as a pond
            }
            // half-width from the valley: widen until the ground rises above
            // the level, capped so a pond never becomes a lake
            let cap = rng.range_f64(38.0, 105.0);
            plans.push((up, di, lvl, cap));
        }
        if settle_on() {
            // The dam is virtual -- nothing in the ground holds the pond --
            // so it is a WALL to the flood. It stands on the CREEK, not on
            // the trunk bed line the stations index (the creek wanders up
            // to ~100 m off it, and a wall on the bed line let the pond
            // leak down the creek's own trench): the creek-line point
            // nearest the dam station, and a band of the line just
            // downstream of it, walled to `cap * 1.8` either side.
            let (line, down): (Vec<Vec2>, i64) = match (creek_line.as_ref(), creek_level.as_ref()) {
                (Some(l), Some(lv)) if l.len() > 2 => {
                    (l.clone(), if lv.first() <= lv.last() { -1 } else { 1 })
                }
                _ => (pts.clone(), if bed.last() <= bed.first() { 1 } else { -1 }),
            };
            for &(_, di, _, cap) in &plans {
                let pd = pts[di.min(pts.len() - 1)];
                let kd = (0..line.len()).min_by(|a, b| line[*a].distance(pd).partial_cmp(&line[*b].distance(pd)).unwrap()).unwrap() as i64;
                // ~110 m of line beyond the dam (the line runs at ~1 m per point when it is the creek)
                let step = (line.len() as f64 / (3.0 * EXTENT_M)).max(1.0 / 8.0);   // points per metre, roughly
                let w = ((110.0 * step).ceil() as i64).max(2);
                let (k0, k1) = if down > 0 { (kd + 1, (kd + w).min(line.len() as i64 - 1)) }
                               else { ((kd - w).max(0), kd - 1) };
                if k0 > k1 {
                    continue;
                }
                let r = (cap * 1.8 / cell).ceil() as i64;
                for k in k0..=k1 {
                    let p = line[k as usize];
                    let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
                    for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
                        for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                            let q = spec.world_of(gx as u32, gy as u32);
                            if q.distance(p) > cap * 1.8 {
                                continue;
                            }
                            // beyond the dam: nearer to the walled band than to the dam point itself
                            if q.distance(line[kd as usize]) > q.distance(p) - 1e-9 {
                                dam_block[spec.index(gx as u32, gy as u32)] = true;
                            }
                        }
                    }
                }
            }
        }
        let fill = if settle_on() {
            fill_levels(&spec, &height.data, &vec![false; spec.len()], &dam_block)
        } else {
            Vec::new()
        };
        let mut made = 0usize;
        for &(up, di, lvl, cap) in &plans {
            let mut seeds: Vec<usize> = Vec::new();
            for si in up.min(di)..=up.max(di) {
                let p = pts[si.min(pts.len() - 1)];
                let r = (cap / cell).ceil() as i64;
                let (cx, cy) = ((p.x / cell).round() as i64,
                                (p.y / cell).round() as i64);
                for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
                    for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                        let d = (((gx - cx).pow(2) + (gy - cy).pow(2)) as f64)
                            .sqrt() * cell;
                        if d > cap {
                            continue;
                        }
                        let i = gy as usize * spec.nx as usize + gx as usize;
                        if height.data[i] >= lvl {
                            continue;      // above the pond surface: dry bank
                        }
                        seeds.push(i);
                    }
                }
            }
            seeds.sort_unstable();
            seeds.dedup();
            if settle_on() {
                let (mut level, mut cells) = settle(&spec, &height.data, &fill, &seeds, lvl, &dam_block);
                // A pond, not a lake: on a flat floor the settled pond ran
                // to 24 ha (seed 700124) against the corpus's 0.2-4 ha. The
                // dam is lowered until the pond holds POND_CAP_HA: six
                // rounds of bisection between the water at the dam and the
                // drawn level.
                const POND_CAP_HA: f64 = 4.0;
                let cap_cells = (POND_CAP_HA * 1e4 / (cell * cell)) as usize;
                if cells.len() > cap_cells {
                    let floor_lvl = seeds.iter().map(|i| height.data[*i]).fold(f64::MAX, f64::min);
                    let (mut lo, mut hi) = (floor_lvl, lvl);
                    for _ in 0..6 {
                        let mid = 0.5 * (lo + hi);
                        let (l2, c2) = settle(&spec, &height.data, &fill, &seeds, mid, &dam_block);
                        if c2.len() > cap_cells {
                            hi = mid;
                        } else {
                            lo = mid;
                            level = l2;
                            cells = c2;
                        }
                    }
                    if cells.len() > cap_cells {
                        let (l2, c2) = settle(&spec, &height.data, &fill, &seeds, lo, &dam_block);
                        level = l2;
                        cells = c2;
                    }
                }
                let dammed = cells.len() * 2 < seeds.len() && cells.len() <= cap_cells / 2;
                if dammed {
                    let mut w = vec![false; spec.len()];
                    for &i in &seeds {
                        w[i] = true;
                    }
                    berm(&spec, &mut height.data, &w, lvl, &creek_banks(&spec, &creek));
                    level = lvl;
                    cells = seeds.clone();
                }
                if std::env::var("NET_DEBUG").is_ok() {
                    eprintln!("  impoundment settle: {} -> {} cells, level {:.2} -> {:.2}{}", seeds.len(), cells.len(), lvl, level,
                              if dammed { " (dammed)" } else { "" });
                }
                for i in cells {
                    if surface.data[i].is_nan() {
                        wet_cells += 1;
                        surface.data[i] = level;
                    } else {
                        surface.data[i] = surface.data[i].max(level);
                    }
                }
            } else {
                for i in seeds {
                    if surface.data[i].is_nan() {
                        wet_cells += 1;
                        surface.data[i] = lvl;
                    } else {
                        surface.data[i] = surface.data[i].max(lvl);
                    }
                }
            }
            made += 1;
        }
        if std::env::var("NET_DEBUG").is_ok() {
            eprintln!("  impoundments: {made}/{n_pond}");
        }
    }

    // --- BACKWATER (mixed-run screen, 2026-09-06) ---------------------------
    // An impoundment raises the water on the reach it floods, and the creek
    // upstream of it kept its own, lower level: walking toward the mouth the
    // water surface stepped UP by 1.4-2.9 m on 33/45 fluvial tiles. A dam
    // backs water up: walking upstream from the mouth the level never falls.
    // The creek's level is pulled up to any pond it runs through, the raise
    // held upstream until the creek's own level exceeds it, the raised cells
    // spread sideways over ground below them (`spread`), and the level the
    // routing sees (`river_z`) is the backwatered one.
    if settle_on() {
        if let (Some(line), Some(lv)) = (creek_line.as_ref(), creek_level.as_mut()) {
            let (nx, ny) = (spec.nx as i64, spec.ny as i64);
            // nearest station per creek cell: multi-source BFS from the
            // station cells over the creek ribbon
            let mut st = vec![u32::MAX; spec.len()];
            let mut q: std::collections::VecDeque<usize> = std::collections::VecDeque::new();
            for (k, p) in line.iter().enumerate() {
                let (x, y) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
                if x < 0 || y < 0 || x >= nx || y >= ny {
                    continue;
                }
                let i = spec.index(x as u32, y as u32);
                if st[i] == u32::MAX {
                    st[i] = k as u32;
                    q.push_back(i);
                }
            }
            // The creek mask also carries channel cells far from the line
            // (a cell 500 m off was found this way), so the walk is capped
            // at the ribbon's own width: RIB steps from a station cell.
            const RIB: u8 = 5;
            let mut depth = vec![0u8; spec.len()];
            while let Some(i) = q.pop_front() {
                if depth[i] >= RIB {
                    continue;
                }
                let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
                for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = (x + dx, y + dy);
                    if a < 0 || b < 0 || a >= nx || b >= ny {
                        continue;
                    }
                    let j = spec.index(a as u32, b as u32);
                    if creek[j] && st[j] == u32::MAX {
                        st[j] = st[i];
                        depth[j] = depth[i] + 1;
                        q.push_back(j);
                    }
                }
            }
            // the water actually standing at each station
            let n = lv.len();
            let mut at: Vec<f64> = lv.clone();
            for i in 0..spec.len() {
                if st[i] != u32::MAX && surface.data[i].is_finite() {
                    let k = (st[i] as usize).min(n - 1);
                    at[k] = at[k].max(surface.data[i]);
                }
            }
            let mouth_first = lv.first() <= lv.last();
            if mouth_first {
                for k in 1..n {
                    at[k] = at[k].max(at[k - 1]);
                }
            } else {
                for k in (0..n.saturating_sub(1)).rev() {
                    at[k] = at[k].max(at[k + 1]);
                }
            }
            // Every valley-floor cell takes the level of its nearest station
            // (a BFS from the station cells over the trunk zone), so the
            // backwater is bounded by the stations' own levels: a spread
            // that carried one level sideways crept down the banks to the
            // mouth, since downstream ground is always lower.
            let mut stv = st.clone();
            let mut q2: std::collections::VecDeque<usize> =
                (0..spec.len()).filter(|i| stv[*i] != u32::MAX).collect();
            while let Some(i) = q2.pop_front() {
                let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
                for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = (x + dx, y + dy);
                    if a < 0 || b < 0 || a >= nx || b >= ny {
                        continue;
                    }
                    let j = spec.index(a as u32, b as u32);
                    if trunkzone[j] && stv[j] == u32::MAX {
                        stv[j] = stv[i];
                        q2.push_back(j);
                    }
                }
            }
            // the ribbon first, then outward from it over ground below the
            // local backwater level
            let mut raised: Vec<usize> = Vec::new();
            for i in 0..spec.len() {
                if st[i] == u32::MAX {
                    continue;
                }
                let want = at[(st[i] as usize).min(n - 1)];
                // only ground below the water line: the creek mask carries
                // the carved BANK cells too, and setting a surface on a
                // bank standing above the water made "wet" cells above
                // their own water with dry hollows beside them
                if height.data[i] >= want {
                    continue;
                }
                if surface.data[i].is_nan() {
                    wet_cells += 1;
                    surface.data[i] = want;
                    raised.push(i);
                } else if surface.data[i] < want - 1e-9 {
                    surface.data[i] = want;
                    raised.push(i);
                }
            }
            // beside the ribbon only where the water is STANDING: behind an
            // impoundment (the station raised above the crease's level) or
            // behind a rise the crease pooled. The ribbon's own level over a
            // hollow beside it is running water, and spreading it printed a
            // side pond at every bend on the mixed run (2026-09-09).
            let standing: Vec<bool> = (0..n).map(|k| {
                at[k] > lv[k] + 0.05 || creek_pooled.as_ref().map_or(false, |p| p[k.min(p.len() - 1)])
            }).collect();
            let w0 = wet_cells;
            let mut q3: std::collections::VecDeque<usize> = raised.iter().copied()
                .filter(|i| standing[(st[*i] as usize).min(n - 1)]).collect();
            while let Some(i) = q3.pop_front() {
                let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
                for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                    let (a, b) = (x + dx, y + dy);
                    if a < 0 || b < 0 || a >= nx || b >= ny {
                        continue;
                    }
                    let j = spec.index(a as u32, b as u32);
                    if stv[j] == u32::MAX || dam_block[j] || !standing[(stv[j] as usize).min(n - 1)] {
                        continue;
                    }
                    let want = at[(stv[j] as usize).min(n - 1)];
                    if height.data[j] >= want {
                        continue;
                    }
                    if surface.data[j].is_nan() {
                        wet_cells += 1;
                        surface.data[j] = want;
                        q3.push_back(j);
                    } else if surface.data[j] < want - 1e-9 {
                        surface.data[j] = want;
                        q3.push_back(j);
                    }
                }
            }
            if std::env::var("NET_DEBUG").is_ok() {
                let dz: Vec<f64> = (0..n).map(|k| at[k] - lv[k]).collect();
                let mx = dz.iter().cloned().fold(f64::MIN, f64::max);
                let n_up = dz.iter().filter(|v| **v > 0.01).count();
                eprintln!("  backwater: {} ribbon cells raised, stations raised {}/{} by up to {:.2} m, +{} cells beside",
                          raised.len(), n_up, n, mx, wet_cells - w0);
            }
            *lv = at;
        }
    }

    Water { surface, lake_frac: wet_cells as f64 / spec.len() as f64,
            river: creek_line, river_z: creek_level }
}

/// Drop every lake body that sits on the valley floor beside running water.
///
/// Review 2026-08-27: a river tile must not have standing lakes on the valley
/// floor. The table drawdown already pulls the level down near the creek, but
/// it cannot help a closed depression whose own floor dips below the drawn-down
/// table, and those are exactly the ones that land on the floor.
///
/// The test is HEIGHT ABOVE THE CHANNEL, not distance from it. A plain radius
/// was tried first and is the wrong criterion in both directions: at 300 m it
/// left a 110 m-wide lake lying just outside the ring on seed 41418, and any
/// radius wide enough to catch that one also deletes legitimate ponds sitting
/// up on the apron, which the dune field is supposed to have.
///
/// Removal is by CONNECTED BODY. Clearing only the qualifying cells would
/// leave the outer crescent of a lake behind as a ring, which is worse.
pub fn clear_lakes_near(water: &mut Water, line: &[Vec2], bed: &[f64]) {
    /// Inside this radius a lake is in the trench, whatever its level.
    const R_TRENCH: f64 = 260.0;
    /// Outside this one the river is not what is keeping the ground wet.
    const R_REACH: f64 = 1100.0;
    /// A body whose surface is within this of the channel bed is on the floor.
    const H_FLOOR: f64 = 12.0;

    let spec = water.surface.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let wet: Vec<bool> = water.surface.data.iter().map(|v| v.is_finite()).collect();
    if line.is_empty() || !wet.iter().any(|&w| w) {
        return;
    }

    // Distance to the channel and the bed elevation at the nearest point.
    let mut dl = vec![f64::INFINITY; nx * ny];
    let mut bl = vec![0.0f64; nx * ny];
    let rc = (R_REACH / spec.cell_size).ceil() as i64;
    for (k, p) in line.iter().enumerate().step_by(4) {
        let bz = bed[k.min(bed.len() - 1)];
        let (cx, cy) = ((p.x / spec.cell_size).round() as i64,
                        (p.y / spec.cell_size).round() as i64);
        for gy in (cy - rc).max(0)..=(cy + rc).min(ny as i64 - 1) {
            for gx in (cx - rc).max(0)..=(cx + rc).min(nx as i64 - 1) {
                let i = gy as usize * nx + gx as usize;
                let d = (((gx - cx).pow(2) + (gy - cy).pow(2)) as f64).sqrt()
                    * spec.cell_size;
                if d < dl[i] {
                    dl[i] = d;
                    bl[i] = bz;
                }
            }
        }
    }

    // Flood fill each wet body; drop it whole if any cell qualifies.
    let mut seen = vec![false; nx * ny];
    let mut stack: Vec<usize> = Vec::new();
    let mut body: Vec<usize> = Vec::new();
    let mut dropped = 0usize;
    for s in 0..nx * ny {
        if !wet[s] || seen[s] {
            continue;
        }
        stack.clear();
        body.clear();
        stack.push(s);
        seen[s] = true;
        let mut hit = false;
        while let Some(i) = stack.pop() {
            body.push(i);
            hit |= dl[i] < R_TRENCH
                || (dl[i] < R_REACH && water.surface.data[i] <= bl[i] + H_FLOOR);
            let (x, y) = (i % nx, i / nx);
            for (dx, dy) in [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)] {
                let (jx, jy) = (x as i64 + dx, y as i64 + dy);
                if jx < 0 || jy < 0 || jx >= nx as i64 || jy >= ny as i64 {
                    continue;
                }
                let j = jy as usize * nx + jx as usize;
                if wet[j] && !seen[j] {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if hit {
            for &i in &body {
                water.surface.data[i] = f64::NAN;
            }
            dropped += body.len();
        }
    }
    water.lake_frac = (water.lake_frac - dropped as f64 / spec.len() as f64).max(0.0);
}

/// Choose which lake bodies survive: largest first, at most `max_n`, and never
/// two whose banks come within `min_gap_m` of each other.
///
/// Returns the labels to DROP. Both archetypes call this -- Nebraska on the
/// label field it already built, Carolina on the finished surface -- so the
/// rule lives in one place rather than being written twice with two subtly
/// different tie-breaks.
///
/// Spacing is measured bank to bank on BOUNDARY cells: the minimum distance
/// between two disjoint sets is the minimum distance between their boundaries,
/// and a lake's boundary is a small fraction of its area, so the pairwise test
/// stays cheap while staying exact.
///
/// Do NOT subsample the boundary in raster order. That was the first version
/// and it is silently wrong: a lake has exactly two boundary cells on most
/// rows, its left and its right, so taking every other one in scan order keeps
/// one whole side of the lake and discards the other. Measured on seed 500025,
/// two lakes 36 m apart were reported 113.7 m apart and both kept. Bounding
/// boxes do the cheap rejection instead, which costs nothing and cannot lie.
fn pick_spaced(spec: &course_world::grid::GridSpec, lab: &[u32],
               sizes: &mut Vec<(usize, u32)>, max_n: usize, min_gap_m: f64)
    -> std::collections::HashSet<u32> {
    let mut drop: std::collections::HashSet<u32> = std::collections::HashSet::new();
    if sizes.is_empty() {
        return drop;
    }
    let mut bank: std::collections::HashMap<u32, Vec<(f64, f64)>> =
        std::collections::HashMap::new();
    // (min_x, min_y, max_x, max_y) per body, for the cheap rejection.
    let mut bbox: std::collections::HashMap<u32, (f64, f64, f64, f64)> =
        std::collections::HashMap::new();
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    for y in 0..ny {
        for x in 0..nx {
            let i = spec.index(x as u32, y as u32);
            let k = lab[i];
            if k == 0 {
                continue;
            }
            let edge = [(-1i64, 0i64), (1, 0), (0, -1), (0, 1)].iter().any(|(dx, dy)| {
                let (jx, jy) = (x + dx, y + dy);
                jx < 0 || jy < 0 || jx >= nx || jy >= ny
                    || lab[spec.index(jx as u32, jy as u32)] != k
            });
            if edge {
                let p = spec.world_of(x as u32, y as u32);
                bank.entry(k).or_default().push((p.x, p.y));
                let b = bbox.entry(k).or_insert((p.x, p.y, p.x, p.y));
                b.0 = b.0.min(p.x);
                b.1 = b.1.min(p.y);
                b.2 = b.2.max(p.x);
                b.3 = b.3.max(p.y);
            }
        }
    }

    sizes.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));   // largest first, stable
    let gap2 = min_gap_m * min_gap_m;
    let mut kept: Vec<u32> = Vec::new();
    for &(_, k) in sizes.iter() {
        if kept.len() >= max_n {
            drop.insert(k);
            continue;
        }
        let (mine, mb) = match (bank.get(&k), bbox.get(&k)) {
            (Some(v), Some(b)) if !v.is_empty() => (v, *b),
            _ => {
                drop.insert(k);
                continue;
            }
        };
        let too_close = kept.iter().any(|o| {
            let ob = match bbox.get(o) {
                Some(b) => *b,
                None => return false,
            };
            // Box-to-box gap first: if the boxes are already far enough
            // apart the bodies certainly are, and this skips almost every
            // pair on a tile whose lakes are spread out.
            let dx = (ob.0 - mb.2).max(mb.0 - ob.2).max(0.0);
            let dy = (ob.1 - mb.3).max(mb.1 - ob.3).max(0.0);
            if dx * dx + dy * dy >= gap2 {
                return false;
            }
            bank.get(o).map_or(false, |theirs| {
                mine.iter().any(|(ax, ay)| {
                    theirs.iter().any(|(bx, by)| {
                        let (ddx, ddy) = (ax - bx, ay - by);
                        ddx * ddx + ddy * ddy < gap2
                    })
                })
            })
        });
        if too_close {
            drop.insert(k);
        } else {
            kept.push(k);
        }
    }
    drop
}



/// Default position of the bank-width dial: 0 = the corpus closure width and
/// shape (owner's choice, 2026-09-02: "follow the corpus"); 1 = the round-1
/// ramp width with the corpus shape. TASTE DIAL, env `CREEK_BANK` for sheets.
pub const BANK_STEEPNESS: f64 = 0.0;

/// Measured real creek cross-sections (`CSEC1`,
/// `assets/sandhills_creek_sections.txt`, built by
/// `tools/aeolian/creek_sections.py`). Each row is one real transect: rim
/// depth, closure width per side, and the section at 1 m from -24 to +24 m
/// relative to the local floor trend, zero at the channel. Sections are kept
/// in ABSOLUTE metres and rows are sorted by rim depth, so the carve picks a
/// real section whose depth matches the depth its own bed demands: a 1 m
/// bed gets a real 1 m section (a broad, gentle V), a deep bed a real deep
/// one. Normalising the shape and scaling it to our depth was tried first
/// and turned a real 1 m floodplain step into a 3 m flat-floored trough.
#[derive(Clone, Debug)]
pub struct CreekSections {
    /// e-folding distance of rim depth along a real creek, metres.
    pub along_corr_m: f64,
    /// rim depth per row, ascending.
    pub depth: Vec<f64>,
    /// Closure width of the STEEPER (narrower) and the gentler side, metres.
    w_steep: Vec<f64>,
    w_gentle: Vec<f64>,
    /// S(d) in metres, d = 0..24, per side; steeper side first.
    s_steep: Vec<[f64; 25]>,
    s_gentle: Vec<[f64; 25]>,
}

impl CreekSections {
    /// Parse a CSEC1 file. Rows tagged `s` (individual transects) by default;
    /// `CREEK_SECTIONS=median` takes the `m` rows (reach medians) instead.
    pub fn load(path: &std::path::Path) -> std::io::Result<CreekSections> {
        let text = std::fs::read_to_string(path)?;
        let want = if std::env::var("CREEK_SECTIONS").map(|v| v == "median").unwrap_or(false) { "m" } else { "s" };
        let mut rows: Vec<(f64, f64, f64, [f64; 25], [f64; 25])> = Vec::new();
        let mut along = 40.0;
        for line in text.lines() {
            let t = line.trim();
            if t.is_empty() || t.starts_with('#') || t == "CSEC1" { continue; }
            let mut it = t.split_whitespace();
            let key = it.next().unwrap_or("");
            let vals: Vec<f64> = it.filter_map(|x| x.parse().ok()).collect();
            match key {
                "along_corr_m" => along = vals.first().copied().unwrap_or(40.0),
                k if k == want => {
                    if vals.len() < 3 + 49 { continue; }
                    let (depth, w_l, w_r) = (vals[0], vals[1], vals[2]);
                    if !(depth > 0.05) { continue; }
                    let v = &vals[3..3 + 49];
                    let mut s_r = [0.0; 25];
                    let mut s_l = [0.0; 25];
                    for d in 0..=24 {
                        s_r[d] = v[24 + d].max(0.0);
                        s_l[d] = v[24 - d].max(0.0);
                    }
                    if w_l <= w_r { rows.push((depth, w_l, w_r, s_l, s_r)); } else { rows.push((depth, w_r, w_l, s_r, s_l)); }
                }
                _ => {}
            }
        }
        if rows.is_empty() {
            return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "CSEC1: no usable rows"));
        }
        rows.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        Ok(CreekSections {
            along_corr_m: along,
            depth: rows.iter().map(|r| r.0).collect(),
            w_steep: rows.iter().map(|r| r.1.clamp(1.0, 24.0)).collect(),
            w_gentle: rows.iter().map(|r| r.2.clamp(1.0, 24.0)).collect(),
            s_steep: rows.iter().map(|r| r.3).collect(),
            s_gentle: rows.iter().map(|r| r.4).collect(),
        })
    }

    /// The fallback when the asset is absent: eight straight ramps (the
    /// round-1 bank) at depths 0.3..2.4 m, so the carve still runs and a
    /// missing asset is visible as a straight bank on the render.
    pub fn builtin() -> CreekSections {
        let mut out = CreekSections { along_corr_m: 40.0, depth: Vec::new(), w_steep: Vec::new(),
                                      w_gentle: Vec::new(), s_steep: Vec::new(), s_gentle: Vec::new() };
        for k in 0..8 {
            let depth = 0.3 + 2.1 * k as f64 / 7.0;
            let w = 6.0 + 10.0 * k as f64 / 7.0;
            let mut a = [0.0; 25];
            let mut b = [0.0; 25];
            for d in 0..=24 {
                a[d] = depth * (d as f64 / (0.85 * w)).min(1.0);
                b[d] = depth * (d as f64 / (1.15 * w)).min(1.0);
            }
            out.depth.push(depth);
            out.w_steep.push(0.85 * w);
            out.w_gentle.push(1.15 * w);
            out.s_steep.push(a);
            out.s_gentle.push(b);
        }
        out
    }

    /// Median closure width and rim depth of the pack.
    pub fn width_p50(&self) -> f64 {
        let mut v: Vec<f64> = (0..self.len())
            .map(|i| 0.5 * (self.w_steep[i] + self.w_gentle[i])).collect();
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2].clamp(4.0, 24.0)
    }
    pub fn depth_p50(&self) -> f64 { self.depth[self.len() / 2].clamp(0.2, 3.0) }

    pub fn len(&self) -> usize { self.depth.len() }
    pub fn is_empty(&self) -> bool { self.depth.is_empty() }

    /// Fractional row index whose rim depth is nearest `d`.
    pub fn row_for_depth(&self, d: f64) -> f64 {
        let n = self.len();
        let j = self.depth.partition_point(|&x| x < d);
        if j == 0 { return 0.0; }
        if j >= n { return (n - 1) as f64; }
        let (a, b) = (self.depth[j - 1], self.depth[j]);
        (j - 1) as f64 + if b - a > 1e-9 { ((d - a) / (b - a)).clamp(0.0, 1.0) } else { 0.0 }
    }

    /// Section relief, metres, at distance `d_m` from the wet edge; beyond
    /// 24 m it continues at the rim grade.
    pub fn relief(&self, k: f64, steep: bool, d_m: f64) -> f64 {
        let tab = if steep { &self.s_steep } else { &self.s_gentle };
        let (i0, f) = self.row(k);
        let at = |row: &[f64; 25], d: f64| -> f64 {
            if d >= 24.0 {
                let g = (row[24] - row[18]) / 6.0;
                return row[24] + g.max(0.0) * (d - 24.0);
            }
            let j = (d as usize).min(23);
            row[j] + (row[j + 1] - row[j]) * (d - j as f64)
        };
        let d = d_m.max(0.0);
        let a = at(&tab[i0], d);
        let b = at(&tab[(i0 + 1).min(self.len() - 1)], d);
        a + (b - a) * f
    }

    pub fn depth_at(&self, k: f64) -> f64 {
        let (i0, f) = self.row(k);
        let b = self.depth[(i0 + 1).min(self.len() - 1)];
        self.depth[i0] + (b - self.depth[i0]) * f
    }

    /// Closure width, metres, at fractional row `k`.
    pub fn width(&self, k: f64, steep: bool) -> f64 {
        let tab = if steep { &self.w_steep } else { &self.w_gentle };
        let (i0, f) = self.row(k);
        let b = tab[(i0 + 1).min(self.len() - 1)];
        tab[i0] + (b - tab[i0]) * f
    }

    fn row(&self, k: f64) -> (usize, f64) {
        let n = self.len();
        if n < 2 { return (0, 0.0); }
        let k = k.clamp(0.0, (n - 1) as f64 - 1e-9);
        let i0 = (k as usize).min(n - 2);
        (i0, k - i0 as f64)
    }
}

/// How wavy the reach is at each node: |sin(turn over +-10 m)| smoothed over
/// +-20 m, normalised so a 40 m radius bend reads 1. Shared by the round-1
/// section and the incision.
fn waviness(qs: &[Vec2]) -> Vec<f64> {
    let n = qs.len();
    let raw: Vec<f64> = (0..n).map(|i| {
        let lo = i.saturating_sub(10);
        let hi = (i + 10).min(n - 1);
        let v1 = qs[i] - qs[lo];
        let v2 = qs[hi] - qs[i];
        let den = v1.length() * v2.length();
        if den < 1e-9 { 0.0 } else { (v1.x * v2.y - v1.y * v2.x).abs() / den }
    }).collect();
    (0..n).map(|i| {
        let lo = i.saturating_sub(20);
        let hi = (i + 21).min(n);
        let m = raw[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
        (m / 0.30).clamp(0.0, 1.0)
    }).collect()
}

const U_K: f64 = 0.25;
const GRADE_BASE: f64 = 0.55;
const GRADE_MIN: f64 = 0.30;
const GRADE_MAX: f64 = 1.00;
const OUTER_GRADE_K: f64 = 1.35;
const INNER_GRADE_K: f64 = 0.75;
const CURVE_GRADE_K: f64 = 0.35;
const CURVE_SHOULDER_K: f64 = 0.8;
const OUTER_K: f64 = 1.15;
const INNER_K: f64 = 0.90;

/// Everything the incision needs that differs between the two modes: the
/// Carolina creek is a 4-5 m blackwater channel on a valley floor, the
/// Nebraska one a 4-14 m river on a canyon floor, and each is measured
/// against its own corpus pack.
pub struct Incision<'a> {
    pub sections: &'a CreekSections,
    /// 0 = the corpus section, 1 = the round-1 analytic ramp.
    pub bank_steep: f64,
    /// wet half-width, metres, and the band its along-arc breathing stays in
    pub hw: f64,
    pub hw_clamp: (f64, f64),
    /// depth of the wet floor below the bed level, metres, before modulation
    pub cut_base: f64,
    /// salt for every noise field; never an RNG draw
    pub salt: u32,
}

/// The corpus-shaped incision. LOWERS the ground; never replaces it.
///
/// Per node the bed demands a depth `D = h0s - bed_edge`; a real section of
/// that depth is chosen from the pack (with a slow along-arc jitter so
/// neighbours differ the way real stations do), and the target is
/// `bed_edge + S_real(d) + detail * g(d)` against the SMOOTHED ground `h0s`
/// (13 m box), where `detail = h0 - h0s` returns over the first
/// `DETAIL_KNEE_M` of the bank. The distance coordinate is warped by
/// world-space noise (plan-view raggedness) and by notches every ~40-80 m.
/// The `bank_steep` dial lerps the relief toward the round-1 ramp. The cut
/// ends where the target meets the ground. Min-composed into a target field
/// and applied once.
#[allow(clippy::too_many_arguments)]
fn incise(height: &mut Grid<f64>, h0: &[f64], surface: &mut Grid<f64>, creek: &mut [bool],
          wet_cells: &mut usize, qs: &[Vec2], zs: &[f64], arcs: &[f64], perps: &[Vec2],
          bends: &[f64], cfg: &Incision) {
    let (hw, m_seed, sec, bank_steep) = (cfg.hw, cfg.salt, cfg.sections, cfg.bank_steep);
    // --- constants: CORPUS-derived are read from `sec`; these are TASTE ----
    /// how far the section walk strays from the depth-matched row, as a
    /// fraction of the pack
    const JITTER: f64 = 0.12;
    /// rescale allowed when no row matches the bed's depth
    const SCALE_CLAMP: (f64, f64) = (0.7, 1.4);
    /// widest cut considered, metres from the wet edge
    const REACH_M: f64 = 30.0;
    /// smoothed-ground box half-width, cells (3 -> 13 m at 2 m)
    const BLUR_R: i64 = 3;
    /// the texture residual is fully back this far up the bank, metres
    const DETAIL_KNEE_M: f64 = 5.0;
    /// plan-view raggedness: relative warp of the distance coordinate
    const RAG: f64 = 0.18;
    /// smin blend at the rim, metres
    const K_RIM: f64 = 0.12;
    const NOTCH_PERIOD_M: f64 = 40.0;
    const NOTCH_T: f64 = 0.40;
    const NOTCH_K: f64 = 0.9;
    let spec = height.spec;
    let cell = spec.cell_size;
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let s_d = m_seed ^ 0x85EB_CA6B;
    let s_h = m_seed ^ 0xC2B2_AE35;
    let s_g = m_seed ^ 0x9E37_79B9;
    let s_sec = m_seed ^ 0x1656_67B1;
    let s_n = m_seed ^ 0xD3A2_646C;
    let (s_r1, s_r2, s_r3) = (m_seed ^ 0x7F4A_7C15, m_seed ^ 0x3C6E_F372, m_seed ^ 0xA54F_F53A);
    let curv_n = waviness(qs);
    let r_max = ((cfg.hw_clamp.1 * OUTER_K + REACH_M) / cell).ceil() as i64 + 1;

    // corridor
    let mut corr = vec![false; spec.len()];
    for p in qs {
        let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
        for gy in (cy - r_max).max(0)..=(cy + r_max).min(ny - 1) {
            for gx in (cx - r_max).max(0)..=(cx + r_max).min(nx - 1) {
                corr[spec.index(gx as u32, gy as u32)] = true;
            }
        }
    }
    // summed-area table of h0, then the smoothed ground on corridor cells
    let w1 = nx as usize + 1;
    let mut sat = vec![0.0f64; (nx as usize + 1) * (ny as usize + 1)];
    for y in 0..ny as usize {
        let mut row = 0.0;
        for x in 0..nx as usize {
            row += h0[y * nx as usize + x];
            sat[(y + 1) * w1 + x + 1] = sat[y * w1 + x + 1] + row;
        }
    }
    let mut h0s = vec![0.0f64; spec.len()];
    let mut detail = vec![0.0f64; spec.len()];
    let mut rag = vec![0.0f64; spec.len()];
    for idx in 0..spec.len() {
        if !corr[idx] { continue; }
        let (x, y) = ((idx % nx as usize) as i64, (idx / nx as usize) as i64);
        let (x0, x1) = ((x - BLUR_R).max(0) as usize, ((x + BLUR_R).min(nx - 1) + 1) as usize);
        let (y0, y1) = ((y - BLUR_R).max(0) as usize, ((y + BLUR_R).min(ny - 1) + 1) as usize);
        let sum = sat[y1 * w1 + x1] - sat[y0 * w1 + x1] - sat[y1 * w1 + x0] + sat[y0 * w1 + x0];
        h0s[idx] = sum / ((x1 - x0) * (y1 - y0)) as f64;
        detail[idx] = h0[idx] - h0s[idx];
        let q = spec.world_of(x as u32, y as u32);
        rag[idx] = course_world::noise::perlin2(q.x / 27.0, q.y / 27.0, s_r1)
            + 0.6 * course_world::noise::perlin2(q.x / 13.0, q.y / 13.0, s_r2)
            + 0.25 * course_world::noise::perlin2(q.x / 6.5, q.y / 6.5, s_r3);
    }

    let mut target: Vec<f64> = h0.to_vec();
    let n_sec = sec.len().max(2);
    let walk_m = sec.along_corr_m.max(10.0);
    for i in 0..qs.len() {
        let (p, z, a) = (qs[i], zs[i], arcs[i]);
        let perp = perps[i];
        let bsg = bends[i];
        let cn = curv_n[i];
        // depth: pools and shallows along the run; wet width breathes too
        let cut = (cfg.cut_base + 0.20 * course_world::noise::perlin1(a / 165.0, s_d)).max(0.12);
        let hwi = (hw * (1.0 + 0.28 * course_world::noise::perlin1(a / 140.0, s_h)))
            .clamp(cfg.hw_clamp.0, cfg.hw_clamp.1);
        let g_var = 1.0 + 0.25 * course_world::noise::perlin1(a / 210.0, s_g);
        let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
        let idx_node = spec.index(cx.clamp(0, nx - 1) as u32, cy.clamp(0, ny - 1) as u32);
        let bed_edge = z - cut * (1.0 - U_K);
        let d_inc = (h0s[idx_node] - bed_edge).max(0.2);
        // a real section of THIS depth, with a slow walk about it
        let k0 = sec.row_for_depth(d_inc);
        let kf = (k0 + JITTER * n_sec as f64 * course_world::noise::perlin1(a / walk_m, s_sec))
            .clamp(0.0, (n_sec - 1) as f64 - 1e-9);
        let scale = (d_inc / sec.depth_at(kf).max(0.05)).clamp(SCALE_CLAMP.0, SCALE_CLAMP.1);
        // notches: bank-line indents on one side, every ~40-80 m
        let nz = course_world::noise::perlin1(a / NOTCH_PERIOD_M, s_n);
        let notch = 1.0 + NOTCH_K * math::smoothstep(NOTCH_T, NOTCH_T + 0.2, nz);
        let notch_outer = course_world::noise::perlin1(a / 300.0, s_n ^ 0x11) >= 0.0;
        for gy in (cy - r_max).max(0)..=(cy + r_max).min(ny - 1) {
            for gx in (cx - r_max).max(0)..=(cx + r_max).min(nx - 1) {
                let idx = spec.index(gx as u32, gy as u32);
                let q = spec.world_of(gx as u32, gy as u32);
                let dd = q.distance(p);
                let side = (q.x - p.x) * perp.x + (q.y - p.y) * perp.y;
                let outer = side * bsg >= 0.0;
                let hw_i = if outer { hwi * OUTER_K } else { hwi * INNER_K };
                // wetting exactly as before: decided before any cut test
                if dd <= hw_i + cell * 0.32 {
                    creek[idx] = true;
                    if surface.data[idx].is_nan() {
                        *wet_cells += 1;
                        surface.data[idx] = z;
                    } else {
                        surface.data[idx] = surface.data[idx].min(z);
                    }
                }
                let cand = if dd <= hw_i {
                    // the level, slightly U, wet floor
                    z - cut * (1.0 - U_K * (dd / hw_i).powi(2))
                } else {
                    // distance up the bank, ragged and notched
                    let dw = (dd - hw_i) * (1.0 + RAG * rag[idx])
                        / if outer == notch_outer { notch } else { 1.0 };
                    if dw > REACH_M { continue; }
                    let s_real = sec.relief(kf, outer, dw) * scale;
                    let grade = (GRADE_BASE * g_var * (1.0 - CURVE_GRADE_K * cn)
                                 * if outer { OUTER_GRADE_K } else { INNER_GRADE_K })
                        .clamp(GRADE_MIN, GRADE_MAX);
                    let s_ramp = grade * dw;
                    let rel = s_real + (s_ramp - s_real) * bank_steep;
                    let g = (dw / DETAIL_KNEE_M).clamp(0.0, 1.0);
                    // A real section is relief RELATIVE TO THE LOCAL GROUND
                    // TREND (detrended at 25-40 m), so it rides the smoothed
                    // ground's cross-slope: on a valley side the target
                    // climbs with the side. Applied as absolute rise from the
                    // bed it ran out as a flat shelf and stopped at the reach
                    // limit with a wall (first render).
                    let trend = h0s[idx] - h0s[idx_node];
                    let mut c0 = bed_edge + rel + trend + detail[idx] * g;
                    // guaranteed close-out: whatever the section does, the
                    // target rejoins the ground by REACH_M, never with a wall
                    let close = math::smoothstep(0.65 * REACH_M, REACH_M, dw);
                    c0 += (h0[idx] - c0) * close;
                    if c0 >= h0[idx] + K_RIM {
                        continue;           // the section has met the ground
                    }
                    course_world::ease::smin(c0, h0[idx], K_RIM)
                };
                if cand < target[idx] {
                    target[idx] = cand;
                    creek[idx] = true;
                }
            }
        }
    }
    for idx in 0..spec.len() {
        if corr[idx] && target[idx] < height.data[idx] {
            height.data[idx] = target[idx];
        }
    }
}

/// ABLATION ONLY (CREEK_CARVE=section): the round-1 analytic section (U bed,
/// per-node curve-aware bank grade and shoulder, smin shoulder) stamped over
/// the textured ground, exactly as committed at ffbb218. Deleted once the
/// incision is signed off.
#[allow(clippy::too_many_arguments)]
fn section_carve_round1(height: &mut Grid<f64>, h0: &[f64], surface: &mut Grid<f64>,
                        creek: &mut [bool], wet_cells: &mut usize, qs: &[Vec2], zs: &[f64],
                        arcs: &[f64], perps: &[Vec2], bends: &[f64], hw: f64, m_seed: u32) {
    const CUT_CAP: f64 = 2.2;
    const SHOULDER_BASE_M: f64 = 3.0;
    const SHOULDER_MIN_M: f64 = 1.5;
    const SHOULDER_MAX_M: f64 = 8.0;
    const INNER_SHOULDER_K: f64 = 1.4;
    let spec = height.spec;
    let cell = spec.cell_size;
    let s_d = m_seed ^ 0x85EB_CA6B;
    let s_h = m_seed ^ 0xC2B2_AE35;
    let s_g = m_seed ^ 0x9E37_79B9;
    let s_sh = m_seed ^ 0x27D4_EB2F;
    let curv_n = waviness(qs);
    let r_max = ((3.4 * OUTER_K + (CUT_CAP + 0.5) / GRADE_MIN + SHOULDER_MAX_M) / cell)
        .ceil() as i64 + 1;
    for i in 0..qs.len() {
        let (p, z, a) = (qs[i], zs[i], arcs[i]);
        let perp = perps[i];
        let bsg = bends[i];
        let cut = (0.30 + 0.20 * course_world::noise::perlin1(a / 165.0, s_d)).max(0.12);
        let hwi = (hw * (1.0 + 0.28 * course_world::noise::perlin1(a / 140.0, s_h))).clamp(1.4, 3.4);
        let g_var = 1.0 + 0.25 * course_world::noise::perlin1(a / 210.0, s_g);
        let sh_var = 1.0 + 0.30 * course_world::noise::perlin1(a / 130.0, s_sh);
        let cn = curv_n[i];
        let g_curve = 1.0 - CURVE_GRADE_K * cn;
        let sh_curve = 1.0 + CURVE_SHOULDER_K * cn;
        let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
        for gy in (cy - r_max).max(0)..=(cy + r_max).min(spec.ny as i64 - 1) {
            for gx in (cx - r_max).max(0)..=(cx + r_max).min(spec.nx as i64 - 1) {
                let idx = spec.index(gx as u32, gy as u32);
                let q = spec.world_of(gx as u32, gy as u32);
                let dd = q.distance(p);
                let side = (q.x - p.x) * perp.x + (q.y - p.y) * perp.y;
                let outer = side * bsg >= 0.0;
                let hw_i = if outer { hwi * OUTER_K } else { hwi * INNER_K };
                let grade = (GRADE_BASE * g_var * g_curve
                             * if outer { OUTER_GRADE_K } else { INNER_GRADE_K })
                    .clamp(GRADE_MIN, GRADE_MAX);
                let shoulder = (SHOULDER_BASE_M * sh_var * sh_curve
                                * if outer { 1.0 } else { INNER_SHOULDER_K })
                    .clamp(SHOULDER_MIN_M, SHOULDER_MAX_M);
                let bed_edge = z - cut * (1.0 - U_K);
                let cand = if dd <= hw_i {
                    z - cut * (1.0 - U_K * (dd / hw_i).powi(2))
                } else {
                    bed_edge + grade * (dd - hw_i)
                };
                if dd <= hw_i + cell * 0.32 {
                    creek[idx] = true;
                    if surface.data[idx].is_nan() {
                        *wet_cells += 1;
                        surface.data[idx] = z;
                    } else {
                        surface.data[idx] = surface.data[idx].min(z);
                    }
                }
                let cand = course_world::ease::smin(cand, h0[idx], grade * shoulder);
                if cand >= h0[idx] - 1e-9 {
                    continue;
                }
                if cand < height.data[idx] {
                    height.data[idx] = cand;
                }
                creek[idx] = true;
            }
        }
    }
}

/// ABLATION ONLY (CREEK_PLANFORM=sine): the two-sine offset creek exactly as
/// shipped at 3839dcb, water.rs:1418-1497. Returns per-node (position, bed,
/// arc, left normal, bend sign). Deleted once the bend-train render is
/// signed off.
#[allow(clippy::too_many_arguments)]
fn sine_offset_legacy(pts: &[Vec2], bed: &[f64], cell: f64, m_lam: f64, m_swing: f64,
                      m_phase: f64, m_seed: u32, u_field: &Grid<f64>)
    -> (Vec<Vec2>, Vec<f64>, Vec<f64>, Vec<Vec2>, Vec<f64>) {
    let mut pos: Vec<(Vec2, Vec2, f64)> = Vec::new();   // centre, perp, swing
    let mut zs: Vec<f64> = Vec::new();
    let mut fac: Vec<f64> = Vec::new();
    let mut arcs: Vec<f64> = Vec::new();
    let mut arc = 0.0;
    for k in 0..pts.len().saturating_sub(1) {
        let (a, b) = (pts[k], pts[k + 1]);
        let seg = a.distance(b);
        if seg <= 1e-6 {
            continue;
        }
        let tang = Vec2::new(b.x - a.x, b.y - a.y).normalized();
        let perp = Vec2::new(-tang.y, tang.x);
        let n = (seg / (cell * 0.5)).ceil().max(1.0) as usize;
        for j in 0..n {
            let t = j as f64 / n as f64;
            let p = Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t);
            let s_here = arc + seg * t;
            let lam_e = m_lam
                * (1.0 + 0.35 * course_world::noise::perlin1(s_here / 640.0, m_seed));
            let off = m_swing * 26.0
                * (math::sin(std::f64::consts::TAU * s_here / lam_e + m_phase)
                    + 0.35 * math::sin(std::f64::consts::TAU * s_here
                                       / (lam_e * 2.7) + m_phase * 1.7));
            let mut f = 0.0;
            for step in 0..=8 {
                let cand = 1.0 - step as f64 / 8.0;
                let q = Vec2::new(p.x + perp.x * off * cand, p.y + perp.y * off * cand);
                if u_field.bilinear(q) < 0.34 {
                    f = cand;
                    break;
                }
            }
            pos.push((p, perp, off));
            zs.push(bed[k] + (bed[k + 1] - bed[k]) * t);
            fac.push(f);
            arcs.push(s_here);
        }
        arc += seg;
    }
    let src = fac.clone();
    for i in 0..fac.len() {
        let lo = i.saturating_sub(12);
        let hi = (i + 13).min(src.len());
        fac[i] = src[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
    }
    let mut qs: Vec<Vec2> = Vec::with_capacity(pos.len());
    let mut perps = Vec::with_capacity(pos.len());
    let mut offs = Vec::with_capacity(pos.len());
    for i in 0..pos.len() {
        let (p0, perp, off) = pos[i];
        let o = off * fac[i];
        qs.push(Vec2::new(p0.x + perp.x * o, p0.y + perp.y * o));
        perps.push(perp);
        offs.push(off);
    }
    (qs, zs, arcs, perps, offs)
}

/// ABLATION ONLY (CREEK_CARVE=skirt): the soft-skirt carve exactly as shipped
/// at 3839dcb, water.rs:1543-1595 -- the one that graded a swale out to 32 m
/// and let the creek sculpt its own valley. Deleted once the channel carve
/// is signed off.
#[allow(clippy::too_many_arguments)]
fn skirt_carve_legacy(height: &mut Grid<f64>, h0: &[f64], surface: &mut Grid<f64>,
                      creek: &mut [bool], wet_cells: &mut usize, p: Vec2, z: f64, a: f64,
                      perp: Vec2, off: f64, cut: f64, hwi: f64, m_seed: u32) {
    let spec = height.spec;
    let cell = spec.cell_size;
    let s_w = m_seed ^ 0x9E37_79B9;
    let base_skirt = 13.0f64;
    let wmul = 1.0
        + 0.40 * course_world::noise::perlin1(a / 270.0, s_w)
        + 0.20 * course_world::noise::perlin1(a / 88.0, s_w ^ 0x11);
    let sk_c = (base_skirt * wmul).clamp(6.5, 23.0);
    let r = ((sk_c * 1.45) / cell).ceil() as i64 + 1;
    let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
    for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
        for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
            let idx = spec.index(gx as u32, gy as u32);
            let q = spec.world_of(gx as u32, gy as u32);
            let dd = q.distance(p);
            let side = (q.x - p.x) * perp.x + (q.y - p.y) * perp.y;
            let outer = side * off >= 0.0;
            let sk = if outer { sk_c * 1.40 } else { sk_c * 0.68 };
            if dd > sk {
                continue;
            }
            let t = math::smoothstep(sk, hwi, dd);
            let target = z - cut;
            let over = (h0[idx] - target).max(0.0);
            let cand = h0[idx] - t * over;
            if cand < height.data[idx] {
                height.data[idx] = cand;
            }
            if dd <= hwi + 2.0 {
                creek[idx] = true;
            }
            if dd <= hwi {
                if surface.data[idx].is_nan() {
                    *wet_cells += 1;
                    surface.data[idx] = z;
                } else {
                    surface.data[idx] = surface.data[idx].min(z);
                }
            }
        }
    }
}

#[cfg(test)]
fn fluvial_guarded(rng: &mut DetRng, height: &mut Grid<f64>, beds: &[(Vec<Vec2>, Vec<f64>)],
                   tiers: &[u8], u_field: &Grid<f64>, d: &Descriptors) -> Water {
    let w = Grid::filled(height.spec, 150.0f64);
    fluvial(rng, height, beds, tiers, u_field, &w, d, None)
}

#[cfg(test)]
mod transcript_guard {
    //! The WATER stream must not shift. The pond loop draws from the same
    //! stream AFTER the creek, so a changed draw count in the creek block
    //! would move every Carolina pond on every fluvial seed. This pins the
    //! next draw after `fluvial` returns on a small synthetic tile; the
    //! golden was recorded against the two-sine creek (3839dcb) and must
    //! survive every planform and carve change.
    use super::*;
    use course_world::grid::GridSpec;

    pub(super) fn run() -> u32 {
        let id = course_seed::RunIdentity::from_seed(600034); // a meander seed
        let d = crate::draw::site(&id, Some(crate::mode::Mode::Fluvial), None);
        let spec = GridSpec::new(Vec2::new(0.0, 0.0), 2.0, 300, 300);
        let mut height = Grid::filled(spec, 0.0f64);
        let mut u = Grid::filled(spec, 0.0f64);
        for y in 0..300u32 {
            for x in 0..300u32 {
                let p = spec.world_of(x, y);
                let dx = (p.x - 300.0).abs();
                height.set(x, y, 0.02 * p.y + 0.03 * dx);
                u.set(x, y, (dx / 150.0).min(1.0));
            }
        }
        let pts: Vec<Vec2> = (0..=100).map(|i| Vec2::new(300.0, i as f64 * 6.0)).collect();
        let bed: Vec<f64> = pts.iter().map(|p| 0.02 * p.y - 0.4).collect();
        let mut rng = crate::rng::stream(&id, crate::rng::WATER);
        let _w = fluvial_guarded(&mut rng, &mut height, &[(pts, bed)], &[1u8], &u, &d);
        rng.next_u32()
    }

    #[test]
    fn water_stream_position_after_fluvial_is_pinned() {
        let got = run();
        eprintln!("WATER stream next_u32 after fluvial: {got}");
        assert_eq!(got, GOLDEN, "the WATER transcript shifted");
    }
    /// Recorded 2026-09-01 against the two-sine creek at 3839dcb.
    const GOLDEN: u32 = 1554183941;
}

#[cfg(test)]
mod incision {
    //! The incision LOWERS the ground, keeps its texture, and leaves the
    //! water level monotone. Synthetic 300x300 tile at 2 m with a two-octave
    //! perlin texture and a straight trunk down the middle (index 0 = mouth).
    use super::*;
    use course_world::grid::GridSpec;

    fn tile() -> (Grid<f64>, Grid<f64>, Vec<Vec2>, Vec<f64>, Descriptors, course_seed::RunIdentity) {
        let id = course_seed::RunIdentity::from_seed(600034);
        let d = crate::draw::site(&id, Some(crate::mode::Mode::Fluvial), None);
        let spec = GridSpec::new(Vec2::new(0.0, 0.0), 2.0, 300, 300);
        let mut height = Grid::filled(spec, 0.0f64);
        let mut u = Grid::filled(spec, 0.0f64);
        for y in 0..300u32 {
            for x in 0..300u32 {
                let p = spec.world_of(x, y);
                let dx = (p.x - 300.0).abs();
                let tex = 0.25 * course_world::noise::perlin2(p.x / 9.0, p.y / 9.0, 7)
                    + 0.12 * course_world::noise::perlin2(p.x / 4.5, p.y / 4.5, 9);
                height.set(x, y, 0.02 * p.y + 0.03 * dx + tex);
                u.set(x, y, (dx / 150.0).min(1.0));
            }
        }
        let pts: Vec<Vec2> = (0..=100).map(|i| Vec2::new(300.0, i as f64 * 6.0)).collect();
        let bed: Vec<f64> = pts.iter().map(|p| 0.02 * p.y - 0.4).collect();
        (height, u, pts, bed, d, id)
    }

    fn run() -> (Grid<f64>, Grid<f64>, Water) {
        let (mut height, u, pts, bed, d, id) = tile();
        let h0 = height.clone();
        let mut rng = crate::rng::stream(&id, crate::rng::WATER);
        let w = fluvial_guarded(&mut rng, &mut height, &[(pts, bed)], &[1u8], &u, &d);
        (h0, height, w)
    }

    #[test]
    fn never_fills() {
        // the carve never fills; the raises allowed are a pond's berm (see
        // `berm`) and the creek's bank lip (see `creek_crease`), both within
        // 12 cells of water (the crease's window reaches ~12 m past the
        // ribbon, and at the tile edge past the line's end)
        let (h0, h, w) = run();
        let spec = h.spec;
        let near_water = |i: usize| -> bool {
            let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
            for dy in -12i64..=12 { for dx in -12i64..=12 {
                let (a, b) = (x + dx, y + dy);
                if a >= 0 && b >= 0 && a < spec.nx as i64 && b < spec.ny as i64
                    && w.surface.data[spec.index(a as u32, b as u32)].is_finite() { return true; }
            } }
            false
        };
        let nearest_water = |i: usize| -> f64 {
            let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
            let mut best = f64::MAX;
            for j in 0..spec.len() {
                if w.surface.data[j].is_finite() {
                    let (a, b) = ((j % spec.nx as usize) as i64, (j / spec.nx as usize) as i64);
                    best = best.min((((a - x).pow(2) + (b - y).pow(2)) as f64).sqrt());
                }
            }
            best
        };
        for i in 0..h0.data.len() {
            if h.data[i] > h0.data[i] + 1e-9 {
                assert!(near_water(i), "fill away from water at {i} ({},{}): {} -> {}, nearest water {} cells",
                        i % spec.nx as usize, i / spec.nx as usize, h0.data[i], h.data[i], nearest_water(i));
            }
        }
    }

    #[test]
    fn water_level_monotone_and_above_bed() {
        let (_, h, w) = run();
        let line = w.river.as_ref().expect("meander seed carries a creek line");
        let z = w.river_z.as_ref().expect("and its water profile");
        assert_eq!(line.len(), z.len());
        // THE invariant: the creek's own level descends to its mouth (index 0)
        let mut worst = 0.0f64;
        for i in 1..z.len() { worst = worst.max(z[i - 1] - z[i]); }
        assert!(worst < 1e-6, "creek level rises toward the mouth by {worst:.4} m");
        assert!(z[z.len() - 1] - z[0] > 1.0, "the creek does not fall: {:.2} m", z[z.len() - 1] - z[0]);
        // and the ground never stands above the water in the channel
        let spec = h.spec;
        let mut n = 0;
        for p in line.iter() {
            let (x, y) = ((p.x / 2.0).round() as u32, (p.y / 2.0).round() as u32);
            if x >= spec.nx || y >= spec.ny { continue; }
            let idx = spec.index(x, y);
            let s = w.surface.data[idx];
            if s.is_nan() { continue; }
            assert!(h.data[idx] <= s + 1e-9, "ground above water at {p:?}");
            n += 1;
        }
        assert!(n > 200, "only {n} wet nodes along the line");
    }

    #[test]
    fn texture_survives_the_bank() {
        let (_, h, w) = run();
        let line = w.river.expect("creek line");
        let spec = h.spec;
        // residual against a 13 m box
        let box13 = |x: i64, y: i64| -> f64 {
            let mut acc = 0.0; let mut n = 0.0;
            for dy in -3..=3 { for dx in -3..=3 {
                let (a, b) = (x + dx, y + dy);
                if a >= 0 && b >= 0 && a < spec.nx as i64 && b < spec.ny as i64 {
                    acc += h.data[spec.index(a as u32, b as u32)]; n += 1.0;
                }
            } }
            acc / n
        };
        let dist = |p: Vec2| line.iter().map(|q| q.distance(p)).fold(f64::INFINITY, f64::min);
        let (mut bank, mut far) = (Vec::new(), Vec::new());
        for y in (30..270).step_by(3) {
            for x in (60..240).step_by(3) {
                let p = spec.world_of(x, y);
                let dd = dist(p);
                let r = h.data[spec.index(x, y)] - box13(x as i64, y as i64);
                if dd > 4.0 && dd < 14.0 { bank.push(r * r); }
                if dd > 40.0 && dd < 80.0 { far.push(r * r); }
            }
        }
        let rms = |v: &Vec<f64>| (v.iter().sum::<f64>() / v.len() as f64).sqrt();
        let (rb, rf) = (rms(&bank), rms(&far));
        eprintln!("texture rms on the bank {rb:.3}  away {rf:.3}");
        assert!(rb >= 0.6 * rf, "the bank lost its texture: {rb:.3} vs {rf:.3}");
    }

    #[test]
    fn deterministic() {
        let (_, a, _) = run();
        let (_, b, _) = run();
        assert!(a.data.iter().zip(b.data.iter()).all(|(x, y)| x == y));
    }

    #[test]
    fn csec_loader_and_builtin() {
        let text = "CSEC1\n# c\nhalf_m 24\nstep_m 1\nalong_corr_m 37.5\ns 1.0 8 12 ".to_string()
            + &(0..49).map(|i| format!("{:.3}", ((i as f64 - 24.0).abs() / 12.0).min(1.5))).collect::<Vec<_>>().join(" ")
            + "\ns 0.5 6 6 " + &(0..49).map(|i| format!("{:.3}", ((i as f64 - 24.0).abs() / 6.0 * 0.5).min(0.6))).collect::<Vec<_>>().join(" ") + "\n";
        let tmp = std::env::temp_dir().join("csec_test.txt");
        std::fs::write(&tmp, text).unwrap();
        let s = CreekSections::load(&tmp).unwrap();
        assert_eq!(s.len(), 2);
        assert!((s.along_corr_m - 37.5).abs() < 1e-9);
        assert!(s.depth[0] <= s.depth[1], "rows sorted by depth");
        for k in [0.0, 0.5, 1.0] {
            for steep in [true, false] {
                assert_eq!(s.relief(k, steep, 0.0), 0.0);
                assert!(s.relief(k, steep, 24.0) > 0.0);
                assert!(s.relief(k, steep, 30.0) >= s.relief(k, steep, 24.0));
                assert!(s.width(k, steep) >= 1.0);
            }
        }
        assert!(s.width(0.0, true) <= s.width(0.0, false));
        assert!((s.row_for_depth(0.0) - 0.0).abs() < 1e-9 && (s.row_for_depth(5.0) - 1.0).abs() < 1e-9);
        assert!((s.row_for_depth(0.75) - 0.5).abs() < 1e-9);
        let b = CreekSections::builtin();
        assert!(b.len() >= 2 && b.relief(3.0, true, 1.0) > 0.0);
        assert!(CreekSections::load(std::path::Path::new("/nonexistent/csec.txt")).is_err());
        // the shipped asset, if present from the crate dir
        let asset = std::path::Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets/sandhills_creek_sections.txt"));
        if asset.exists() {
            let s = CreekSections::load(asset).unwrap();
            assert!(s.len() > 100, "asset rows {}", s.len());
            assert!(s.along_corr_m > 10.0);
            assert!(s.depth.windows(2).all(|w| w[1] >= w[0]));
        }
    }
}

#[cfg(test)]
mod slot_lowered_tests {
    //! The slot, lowered not replaced, on a synthetic aeolian-style call of
    //! `cut_creek`: textured and textureless 300x300 tiles at 2 m, a straight
    //! line at 1 m nodes, a 9 m river and a 2.4 m creek.
    use super::*;
    use course_world::grid::GridSpec;

    fn tile(textured: bool) -> (Grid<f64>, Vec<Vec2>, Vec<f64>) {
        let spec = GridSpec::new(Vec2::new(0.0, 0.0), 2.0, 300, 300);
        let mut h = Grid::filled(spec, 0.0f64);
        for y in 0..300u32 {
            for x in 0..300u32 {
                let p = spec.world_of(x, y);
                let tex = if textured {
                    0.25 * course_world::noise::perlin2(p.x / 9.0, p.y / 9.0, 7)
                        + 0.12 * course_world::noise::perlin2(p.x / 4.5, p.y / 4.5, 9)
                } else { 0.0 };
                h.set(x, y, 0.02 * p.y + 0.03 * (p.x - 300.0).abs() + tex);
            }
        }
        let line: Vec<Vec2> = (0..=600).map(|i| Vec2::new(300.0, i as f64)).collect();
        let bed: Vec<f64> = line.iter().map(|p| 0.02 * p.y - 0.4).collect();
        (h, line, bed)
    }

    fn run(textured: bool, width: f64) -> (Grid<f64>, Grid<f64>, Water, Vec<Vec2>, Vec<f64>) {
        let (mut h, line, bed) = tile(textured);
        let h0 = h.clone();
        let mut w = Water { surface: Grid::filled(h.spec, f64::NAN), lake_frac: 0.0, river: None, river_z: None };
        cut_creek(&mut h, &mut w, &line, &bed, width, None, 7);
        (h0, h, w, line, bed)
    }

    #[test]
    fn never_fills() {
        // the carve never fills; the one raise allowed is the bank lip, a
        // hollow beside the ribbon brought to the water line, within 8
        // cells of water (see `creek_crease`)
        for width in [9.0, 2.4] {
            let (h0, h, w, _, _) = run(true, width);
            let spec = h.spec;
            for i in 0..h0.data.len() {
                if h.data[i] <= h0.data[i] + 1e-9 { continue; }
                let (x, y) = ((i % spec.nx as usize) as i64, (i / spec.nx as usize) as i64);
                let mut near = false;
                for dy in -8i64..=8 { for dx in -8i64..=8 {
                    let (a, b) = (x + dx, y + dy);
                    if a >= 0 && b >= 0 && a < spec.nx as i64 && b < spec.ny as i64
                        && w.surface.data[spec.index(a as u32, b as u32)].is_finite() { near = true; }
                } }
                assert!(near, "fill away from water at {i}: {} -> {}", h0.data[i], h.data[i]);
                assert!(h.data[i] - h0.data[i] < 1.5, "fill of {} m at {i}", h.data[i] - h0.data[i]);
            }
        }
    }

    #[test]
    fn deterministic() {
        let (_, a, _, _, _) = run(true, 9.0);
        let (_, b, _, _, _) = run(true, 9.0);
        assert!(a.data.iter().zip(b.data.iter()).all(|(x, y)| x == y));
    }

    #[test]
    fn single_wet_body_at_bed_level() {
        for width in [9.0, 2.4] {
            let (_, _, w, line, bed) = run(true, width);
            let spec = w.surface.spec;
            // the creek's own level descends to the mouth (index 0 is the
            // low end of the synthetic bed) and every node is wet
            let z = w.river_z.as_ref().expect("level profile");
            for i in 1..z.len() { assert!(z[i] >= z[i - 1] - 1e-6, "level falls toward the head at {i}"); }
            let _ = bed;
            for p in line.iter() {
                let (x, y) = ((p.x / 2.0) as u32, (p.y / 2.0) as u32);
                if x >= spec.nx || y >= spec.ny { continue; }
                assert!(!w.surface.data[spec.index(x, y)].is_nan(), "dry at {p:?}");
            }
            // one connected body
            let n = spec.len();
            let mut seen = vec![false; n];
            let mut bodies = 0;
            for start in 0..n {
                if w.surface.data[start].is_nan() || seen[start] { continue; }
                bodies += 1;
                let mut stack = vec![start];
                seen[start] = true;
                while let Some(i) = stack.pop() {
                    let (x, y) = ((i % 300) as i64, (i / 300) as i64);
                    for dy in -1..=1i64 { for dx in -1..=1i64 {
                        let (a, b) = (x + dx, y + dy);
                        if a < 0 || b < 0 || a >= 300 || b >= 300 { continue; }
                        let j = (b * 300 + a) as usize;
                        if !w.surface.data[j].is_nan() && !seen[j] { seen[j] = true; stack.push(j); }
                    } }
                }
            }
            assert_eq!(bodies, 1, "width {width}: {bodies} wet bodies");
        }
    }

    #[test]
    fn profile_is_one_ramp_with_no_step() {
        // textureless: from the wet edge outward the surface rises at the
        // bank grade and then IS the ground; no cell-to-cell jump beyond the
        // steepest grade, and nothing changed past the rim
        let (h0, h, _, _, _) = run(false, 9.0);
        let spec = h.spec;
        let max_rise = 0.90 * 1.2 * 2.0 * 1.05 + 0.05;
        let mut touched_far = 0;
        for y in (40..260).step_by(20) {
            let row: Vec<f64> = (0..300).map(|x| h.data[spec.index(x, y)]).collect();
            let c = 150usize;
            for side in [1i64, -1i64] {
                let mut x = c as i64 + side * 3;       // start past the wet edge (hw 4.5 m + tol)
                let mut last = row[x as usize];
                while (0..300).contains(&(x + side)) {
                    x += side;
                    let v = row[x as usize];
                    assert!(v >= last - 1e-6, "profile falls at row {y} x {x}");
                    assert!(v - last <= max_rise, "step of {:.2} m at row {y} x {x}", v - last);
                    last = v;
                    if (x - c as i64).abs() as f64 * 2.0 > 40.0 { break; }
                }
            }
            for x in 0..300u32 {
                let idx = spec.index(x, y);
                if ((x as f64 - 150.0) * 2.0).abs() > 60.0 && (h.data[idx] - h0.data[idx]).abs() > 1e-9 {
                    touched_far += 1;
                }
            }
        }
        assert_eq!(touched_far, 0, "ground changed beyond the rim");
    }

    #[test]
    fn texture_survives_the_bank() {
        let (_, h, _, line, _) = run(true, 9.0);
        let spec = h.spec;
        let box13 = |x: i64, y: i64| -> f64 {
            let (mut acc, mut n) = (0.0, 0.0);
            for dy in -3..=3 { for dx in -3..=3 {
                let (a, b) = (x + dx, y + dy);
                if a >= 0 && b >= 0 && a < 300 && b < 300 { acc += h.data[spec.index(a as u32, b as u32)]; n += 1.0; }
            } }
            acc / n
        };
        let (mut bank, mut far) = (Vec::new(), Vec::new());
        for y in (30..270).step_by(3) {
            for x in (60..240).step_by(3) {
                let p = spec.world_of(x, y);
                let dd = line.iter().map(|q| q.distance(p)).fold(f64::INFINITY, f64::min);
                let r = h.data[spec.index(x, y)] - box13(x as i64, y as i64);
                if dd > 7.0 && dd < 15.0 { bank.push(r * r); }
                if dd > 40.0 && dd < 80.0 { far.push(r * r); }
            }
        }
        let rms = |v: &Vec<f64>| (v.iter().sum::<f64>() / v.len() as f64).sqrt();
        let (rb, rf) = (rms(&bank), rms(&far));
        eprintln!("slot_lowered texture rms on the bank {rb:.3}  away {rf:.3}");
        assert!(rb >= 0.6 * rf, "the bank lost its texture: {rb:.3} vs {rf:.3}");
    }
}
