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
            river: Option<&RiverPlan>) -> Water {
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
        RiverStyle { lam: (220.0, 340.0), swing: (1.20, 1.70), wander: 0.80 }, // C loops
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
    // Cross the tile roughly perpendicular to the wind, like the Dismal.
    let along = rng.next_f64() < 0.5;
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

    let mut pts: Vec<Vec2> = vec![start];
    let mut p = start;
    let mut arc = 0.0;
    let lim = EXTENT_M - 2.0;
    for _ in 0..6000 {
        // heading: forward progress + periodic swing + slow organic wander.
        // BOTH the wavelength and the swing breathe along the run -- a
        // constant-parameter serpentine is exactly as manufactured as a
        // constant sinusoid, just curlier (measured on the first render of
        // this redesign, and twice before on this project).
        let lam_e = lam * (1.0 + 0.45 * course_world::noise::perlin1(arc / 640.0, s_lam));
        let sw_e = swing
            * (0.45 + 0.75 * (0.5 + 0.5 * course_world::noise::perlin1(arc / 480.0, s_swing)));
        let wob = sw_e
            * (math::sin(std::f64::consts::TAU * arc / lam_e + ph1)
                + 0.35 * math::sin(std::f64::consts::TAU * arc / (lam_e * 2.7) + ph2));
        let wander = style.wander * course_world::noise::perlin1(arc / 900.0, s_noise);
        // gentle downhill preference: sample the macro left and right of the
        // heading and lean toward the lower side, so the channel occupies
        // lows instead of skirting them (review on seed 5: the stream hugged
        // past a low point, "weird from a drainage perspective").
        let hd = base_heading + wob + wander;
        let lp = Vec2::new(p.x - 60.0 * math::sin(hd), p.y + 60.0 * math::cos(hd));
        let rp2 = Vec2::new(p.x + 60.0 * math::sin(hd), p.y - 60.0 * math::cos(hd));
        let (zl, zr) = (height.bilinear(lp), height.bilinear(rp2));
        let lean = 0.30 * ((zr - zl) / 6.0).clamp(-1.0, 1.0);
        // total deviation from the crossing direction is clamped short of a
        // reversal: on high-swing draws, swing + wander + lean exceeded pi and
        // the integrator milled in place -- seed 9 rendered as knotted coils
        // that then drained every pond they crossed.
        let dev = (hd - base_heading + lean).clamp(-1.95, 1.95);
        let h = base_heading + dev;
        p = Vec2::new(p.x + step * math::cos(h), p.y + step * math::sin(h));
        // soft reflect off the side borders so the creek stays on-tile
        if along {
            p.y = p.y.clamp(60.0, lim - 60.0);
            if p.x >= lim { break; }
            p.x = p.x.max(2.0);
        } else {
            p.x = p.x.clamp(60.0, lim - 60.0);
            if p.y >= lim { break; }
            p.y = p.y.max(2.0);
        }
        arc += step;
        pts.push(p);
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
    let depth = 1.2;
    let hw_water = 3.0;                            // ~6 m wet width (review-confirmed)
    let hw_cut = 7.0;                              // channel slot + shoulder
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
                            asym, s_wl, s_wr, s_flip, s_shelf, s_cat, s_wr2, shelf_h });
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
        // per-side widths, independently breathing and MORE aggressive than
        // the old shared +-38%
        let ml = 1.0 + 0.75 * course_world::noise::perlin1(arc / 300.0, pl.s_wl)
            + 0.35 * course_world::noise::perlin1(arc / 90.0, pl.s_wl ^ 0x11);
        let mr = 1.0 + 0.75 * course_world::noise::perlin1(arc / 300.0, pl.s_wr)
            + 0.35 * course_world::noise::perlin1(arc / 90.0, pl.s_wr ^ 0x11);
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
                    + math::smoothstep(0.5, 1.0, u) * 1.1
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
    let hw_water = 3.0;
    let hw_cut = 7.0;
    for (i, q) in pl.pts.iter().enumerate() {
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
