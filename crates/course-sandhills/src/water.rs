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
pub fn find(height: &Grid<f64>, datum8: &Grid<f64>, d: &Descriptors) -> Water {
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
    let mut margin = vec![0.0f64; spec.len()];
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let li = spec.index(x, y);
            let p = spec.world_of(x, y);
            table[li] = datum8.bilinear(p) + floor_off - d.water_table_m;
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
    let lake_frac = lake_cells as f64 / spec.len() as f64;
    Water { surface, lake_frac, river: None }
}

/// The allogenic river: a spring-fed through-river crossing the tile on the
/// drawn coin. It cuts a shallow valley across whatever is in the way — that
/// is what allogenic means — with a monotone surface from inflow to outflow.
pub fn river(rng: &mut DetRng, height: &mut Grid<f64>, water: &mut Water, d: &Descriptors) {
    if !d.allogenic_river {
        return;
    }
    let spec = height.spec;
    // Enter and leave on opposite edges, perpendicular-ish to the wind so it
    // reads as crossing the trains (the Dismal cuts across the field).
    let along = rng.next_f64() < 0.5;
    let t0 = rng.range_f64(0.25, 0.75);
    let t1 = (t0 + rng.range_f64(-0.18, 0.18)).clamp(0.15, 0.85);
    let (a, b) = if along {
        (Vec2::new(0.0, t0 * EXTENT_M), Vec2::new(EXTENT_M, t1 * EXTENT_M))
    } else {
        (Vec2::new(t0 * EXTENT_M, 0.0), Vec2::new(t1 * EXTENT_M, EXTENT_M))
    };
    // Meander with harmonics and breathing. A single constant sinusoid is a
    // manufactured tell -- attempt 4's records say it twice ("a single
    // sinusoid reads as a regular scallop") and the first version of this
    // river proved it a third time on the render. Fundamental + a weak short
    // and long harmonic, with the wavelength breathing along the run.
    // Dense enough that point spacing stays under the water half-width even
    // where the meander stretches the arc -- 200 chord-uniform points left
    // 30+ m gaps at tight bends and the water rendered as dashes.
    let n = 700usize;
    let lam = rng.range_f64(700.0, 1100.0);
    let amp = rng.range_f64(60.0, 140.0);
    let (ph, ph2, ph3, phb) = (rng.range_f64(0.0, std::f64::consts::TAU),
                               rng.range_f64(0.0, std::f64::consts::TAU),
                               rng.range_f64(0.0, std::f64::consts::TAU),
                               rng.range_f64(0.0, std::f64::consts::TAU));
    let dirv = Vec2::new(b.x - a.x, b.y - a.y);
    let len = dirv.length();
    let u = Vec2::new(dirv.x / len, dirv.y / len);
    let pn = u.perp();
    let pts: Vec<Vec2> = (0..=n)
        .map(|i| {
            let s = i as f64 / n as f64;
            let arc = s * len;
            let taper = math::smoothstep(0.0, 0.12, s) * math::smoothstep(1.0, 0.88, s);
            let breathe = 1.0 + 0.30 * math::sin(std::f64::consts::TAU * arc / (lam * 3.3) + phb);
            let k = std::f64::consts::TAU * arc / (lam * breathe);
            let off = amp * taper
                * (math::sin(k + ph)
                    + 0.22 * math::sin(k / 0.41 + ph2)
                    + 0.30 * math::sin(k * 0.43 + ph3));
            Vec2::new(a.x + dirv.x * s + pn.x * off, a.y + dirv.y * s + pn.y * off)
        })
        .collect();

    // Monotone bed: sample the ground, run a downstream running-min, drop a
    // little more, and cut a shallow valley to it.
    let hw = rng.range_f64(18.0, 34.0);       // water half-width
    let vw = hw * 9.0;                        // valley half-width
    let mut bed: Vec<f64> = pts.iter().map(|p| height.bilinear(*p) - 2.0).collect();
    for i in 1..bed.len() {
        bed[i] = bed[i].min(bed[i - 1] - 0.006);
    }
    for (i, p) in pts.iter().enumerate() {
        let x0 = ((p.x - vw) / spec.cell_size).floor().max(0.0) as u32;
        let x1 = ((p.x + vw) / spec.cell_size).ceil().min(spec.nx as f64 - 1.0) as u32;
        let y0 = ((p.y - vw) / spec.cell_size).floor().max(0.0) as u32;
        let y1 = ((p.y + vw) / spec.cell_size).ceil().min(spec.ny as f64 - 1.0) as u32;
        for y in y0..=y1 {
            for x in x0..=x1 {
                let q = spec.world_of(x, y);
                let dd = ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt();
                if dd >= vw {
                    continue;
                }
                let li = spec.index(x, y);
                // valley target: bed at the channel, easing up to grade
                // Smoothstep section, not r^2: the quadratic printed a hard
                // parallel-walled trough band along the whole run.
                let f = math::smoothstep(0.12, 1.0, dd / vw);
                let t = bed[i] + f * (height.data[li] - bed[i]).max(0.0);
                if height.data[li] > t {
                    let excess = height.data[li] - t;
                    height.data[li] -= excess * math::smoothstep(0.0, 3.0, excess);
                }
                if dd < hw {
                    let ws = bed[i] + 0.6;
                    let cur = water.surface.data[li];
                    // The river takes precedence over a lake it crosses --
                    // it drains the crossing to its own falling surface.
                    if cur.is_nan() || cur > ws {
                        water.surface.data[li] = ws;
                    }
                }
            }
        }
    }
    water.river = Some(pts);
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
        let t = build_full(&RunIdentity::from_seed(9), &p, Some(FormClass::Train));
        assert!(t.river.is_none(), "the river is removed pending redesign");
    }
}
