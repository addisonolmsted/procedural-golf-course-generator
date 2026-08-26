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
}

/// Lakes: connected pockets where the ground sits below the local water
/// table. The table rides the regional datum (the Ogallala is unconfined
/// here), so each lake takes ONE flat level — the 25th percentile of the
/// table across its own extent — and cells above that level go dry again.
pub fn find(height: &Grid<f64>, datum8: &Grid<f64>, d: &Descriptors,
            river: Option<&RiverPlan>, blowouts: &[crate::blowout::Blowout]) -> Water {
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
    // RIVER DRAWDOWN. A river drains its corridor: the local water table is
    // depressed toward the river's own bed, easing back to the regional
    // table over a few hundred metres. Without this, shallow-table seeds
    // flooded the corridor floor with lakes sitting directly on the river
    // (review, 2026-08-23). Computed at the datum grid's 8 m and sampled
    // bilinearly, like the table itself.
    let dspec = datum8.spec;
    let mut ddown = Grid::filled(dspec, 0.0f64);
    if let Some(pl) = river {
        let reach = (pl.floor_hw + pl.wall_m) * 1.4 + 220.0;
        for (i, q) in pl.corr.iter().enumerate().step_by(2) {
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
                    let tab = datum8.bilinear(p2) + floor_off - d.water_table_m;
                    let full = (tab - bed_t).max(0.0);
                    let f = 1.0 - math::smoothstep(pl.floor_hw + pl.wall_m, reach, dd);
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
            table[li] = datum8.bilinear(p) + floor_off - d.water_table_m
                - ddown.bilinear(p);
            margin[li] = height.data[li] - table[li];
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
                }
            }
        }
    }
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
        let level = lv[lv.len() / 4];
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
        let final_cells: Vec<usize> =
            cells.into_iter().filter(|i| height.data[*i] < level).collect();
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
    let keep_max = total * 2 / 3;
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
    let lake_frac = lake_cells as f64 / spec.len() as f64;
    Water { surface, lake_frac, river: None }
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
    let (depth, floor_hw, wall_m, wall_rise) = if incised {
        // the gorge: bed well below the dune surface, walls climbing hard --
        // deep, narrow, sharp. The Sand Hills CC setting.
        (rng.range_f64(4.5, 8.0), rng.range_f64(7.0, 14.0),
         rng.range_f64(30.0, 55.0), rng.range_f64(6.0, 11.0))
    } else {
        (depth, floor_hw, wall_m, wall_rise)
    };

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
        let Some(p) = pack() else { return };
        for seed in [3u64, 9, 18] {
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
        let Some(p) = pack() else { return };
        let wet = build_full(&RunIdentity::from_seed(9), &p, Some(FormClass::Train));
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
        let len_m = r.len() as f64 * 4.0;
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
               u_field: &Grid<f64>, d: &Descriptors) -> Water {
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

        // ---- MEANDER CREEK: one path, carved so it cannot break ----------
        // Two faults broke the drawn creek into dashes (review 2026-08-25).
        // The swing was accepted or rejected per SAMPLE, so a rejected
        // sample teleported the creek back to the centre-line and left a gap
        // up to a swing-width across; and every cell was gated on
        // `s <= ground + 0.6`, which punched holes wherever the bank rose.
        // The path is now built once with a SMOOTHED clamp — it can bend
        // away from the valley wall but never jump — and the creek is
        // CARVED, so the water is continuous by construction.
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
                // the largest FRACTION of the swing that stays on the floor
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
        // smooth the clamp: the creek leans off the wall over ~50 m instead
        // of snapping to the centre-line between one sample and the next
        let src = fac.clone();
        for i in 0..fac.len() {
            let lo = i.saturating_sub(12);
            let hi = (i + 13).min(src.len());
            fac[i] = src[lo..hi].iter().sum::<f64>() / (hi - lo) as f64;
        }
        // The carve must not become a TRENCH. Writing a hard floor inside
        // hw and a hard shelf outside it put a wall at each boundary — 825%
        // slope on seed 121, the "sharp ditch" of the review. Two changes:
        // the bed FOLLOWS THE GROUND (the swing can ride onto higher ground,
        // and a bed taken from the trunk centre-line there had to cut metres
        // to reach it), and the cut is a one-sided SOFT skirt that reaches
        // zero 26 m out, so there is no edge to catch the light.
        // --- the carved swale: narrower, and never the same twice ---------
        // Review: a fixed 26 m skirt with a fixed 0.35 m cut is literally an
        // extrusion — constant width, constant cross-section, constant depth
        // for three kilometres — and it reads as one. Four things now vary
        // along the axis, all from the creek's own seed so the rest of the
        // tile is untouched:
        //
        //   * half the base width (13 m, was 26), modulated by two octaves;
        //   * the cut depth, so the swale has pools and shallows;
        //   * the wet width itself;
        //   * and an ASYMMETRY tied to the bend — the outer bank of a
        //     meander is the cut bank and carries the wider, deeper edge,
        //     the inner bank is the point bar and barely registers. That is
        //     what stops the cross-section being a symmetric trough.
        let s_w = m_seed ^ 0x9E37_79B9;
        let s_d = m_seed ^ 0x85EB_CA6B;
        let s_h = m_seed ^ 0xC2B2_AE35;
        let base_skirt = 13.0f64;
        let mut qs: Vec<Vec2> = Vec::with_capacity(pos.len());
        for i in 0..pos.len() {
            let (p0, perp, off) = pos[i];
            let o = off * fac[i];
            qs.push(Vec2::new(p0.x + perp.x * o, p0.y + perp.y * o));
        }
        // Ground-following bed with a CUT CAP. Strict monotonicity plus a
        // single low ground sample locked the bed down for the rest of the
        // path, so every later point had to cut metres to reach it — the
        // skirt then spread a 50 m cut over 26 m and gave 510% slopes 20 m
        // from the water. The bed still prefers to fall, but it may climb
        // rather than cut deeper than CUT_CAP, which no eye can read on a
        // creek and which bounds the carve by construction.
        const CUT_CAP: f64 = 2.2;
        let mut gr: Vec<f64> = Vec::with_capacity(qs.len());
        for i in 0..qs.len() {
            gr.push(height.bilinear(qs[i]));
        }
        zs[0] = zs[0].min(gr[0] - 0.35);
        for i in 1..zs.len() {
            let want = zs[i].min(gr[i] - 0.35).min(zs[i - 1]);
            zs[i] = want.max(gr[i] - CUT_CAP).min(gr[i] - 0.35);
        }
        // The cut is taken against a SNAPSHOT of the ground and combined by
        // MIN, not applied in sequence. Path points sit ~1 m apart and the
        // skirt reaches 26 m, so each cell is visited by ~50 of them; a
        // running subtraction compounded those visits and gouged a hard
        // black slot down the creek's own centre-line (review). Reading h0
        // makes the result order-independent and idempotent.
        let h0: Vec<f64> = height.data.clone();
        for i in 0..qs.len() {
            let (p, z) = (qs[i], zs[i]);
            let a = arcs[i];
            let perp = pos[i].1;
            let off = pos[i].2;
            // width: two octaves, so it swells and pinches on more than one
            // scale instead of breathing regularly
            let wmul = 1.0
                + 0.40 * course_world::noise::perlin1(a / 270.0, s_w)
                + 0.20 * course_world::noise::perlin1(a / 88.0, s_w ^ 0x11);
            let sk_c = (base_skirt * wmul).clamp(6.5, 23.0);
            // depth: pools and shallows along the run
            let cut = (0.30 + 0.20 * course_world::noise::perlin1(a / 165.0, s_d)).max(0.12);
            // wet width breathes a little too
            let hwi = (hw * (1.0 + 0.28 * course_world::noise::perlin1(a / 140.0, s_h)))
                .clamp(1.4, 3.4);
            let r = ((sk_c * 1.45) / cell).ceil() as i64 + 1;
            let (cx, cy) = ((p.x / cell).round() as i64, (p.y / cell).round() as i64);
            for gy in (cy - r).max(0)..=(cy + r).min(spec.ny as i64 - 1) {
                for gx in (cx - r).max(0)..=(cx + r).min(spec.nx as i64 - 1) {
                    let idx = spec.index(gx as u32, gy as u32);
                    let q = spec.world_of(gx as u32, gy as u32);
                    let dd = q.distance(p);
                    // which bank? the outer one is the cut bank
                    let side = (q.x - p.x) * perp.x + (q.y - p.y) * perp.y;
                    let outer = side * off >= 0.0;
                    let sk = if outer { sk_c * 1.40 } else { sk_c * 0.68 };
                    if dd > sk {
                        continue;
                    }
                    // one-sided soft cut: full at the centre-line, nothing at
                    // the skirt, and never a fill
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
                            wet_cells += 1;
                            surface.data[idx] = z;
                        } else {
                            surface.data[idx] = surface.data[idx].min(z);
                        }
                    }
                }
            }
        }
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

    Water { surface, lake_frac: wet_cells as f64 / spec.len() as f64, river: None }
}
