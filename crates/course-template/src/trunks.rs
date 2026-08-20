//! Step 2 — trunk placement.
//!
//! Trunks are PLACED, not grown. The measurement behind that:
//! `docs/network-first/02-drainage-patterns.md` §4 — no system in a real
//! river-valley tile gathers even 30 % of the tile's own area, so a network
//! grown from the tile's 9 km² alone gives three or four comparable systems in
//! every archetype. **Trunk dominance is discharge, sourced outside the
//! window**, and it therefore has to be declared here.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::world::EXTENT_M;

/// Which tile edge drains. The other three are divides.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Edge {
    South,
    North,
    West,
    East,
}

impl Edge {
    pub fn from_index(i: usize) -> Edge {
        match i % 4 {
            0 => Edge::South,
            1 => Edge::North,
            2 => Edge::West,
            _ => Edge::East,
        }
    }
    /// A point at parameter `t` in [0,1] along the edge.
    pub fn point(self, t: f64) -> Vec2 {
        let u = t * EXTENT_M;
        match self {
            Edge::South => Vec2::new(u, 0.0),
            Edge::North => Vec2::new(u, EXTENT_M),
            Edge::West => Vec2::new(0.0, u),
            Edge::East => Vec2::new(EXTENT_M, u),
        }
    }
    /// Inward normal — the direction a trunk heads from its mouth.
    pub fn inward(self) -> f64 {
        match self {
            Edge::South => core::f64::consts::FRAC_PI_2,
            Edge::North => -core::f64::consts::FRAC_PI_2,
            Edge::West => 0.0,
            Edge::East => core::f64::consts::PI,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Trunk {
    /// Where it leaves the tile.
    pub mouth: Vec2,
    /// Heading INTO the tile at the mouth, radians.
    pub azimuth_rad: f64,
    /// Catchment entering here from outside the window, km². This is what
    /// makes a trunk a trunk.
    pub external_km2: f64,
    /// The far END of the trunk. A point on a far edge when the trunk is a
    /// through-river (it carries external inflow, so it must ENTER the tile
    /// somewhere); a high-ground headwater point inside the tile otherwise.
    pub far: Vec2,
    /// True when `far` sits on an edge.
    pub through: bool,
}

pub struct TrunkParams {
    pub count: u32,
    pub external_inflow_km2: f64,
    /// Fraction of the total inflow the largest trunk takes. 1.0 = a single
    /// dominant river; near 1/count = comparable systems.
    pub dominance: f64,
}

/// Minimum separation between trunk mouths. **Deliberately wide.**
///
/// These are MAIN TRUNKS only. The medium and small systems that fill the
/// space between them are step 4's tributaries, so this spacing is not
/// supposed to match the spacing of all real outlets — a real 3 km border at
/// the 6e4 m² channel threshold carries 2.4–3.9 outlets, but at trunk scale
/// (1e6–2e6 m²) it carries only 0.3–0.55, and most borders carry none
/// (`docs/network-first/outlet_sweep.txt`). **Wide trunk spacing with the gaps
/// filled by tributaries is the intended design**, not a defect to tune out.
///
/// The divide rule below is the correctness test and this is the aesthetic
/// floor on top of it: a divide can be sharp enough to separate two systems
/// while still leaving two trunks close enough to read as one river drawn
/// twice. 750 m over the 2000 m of usable edge (after corner clearance) puts
/// a practical ceiling of three trunks on a tile.
///
/// It was briefly 60 m — a physical minimum — when the divide rule replaced the
/// old 420 m distance floor. That was right about correctness and wrong about
/// how it looks.
pub const MOUTH_SEP_M: f64 = 750.0;

/// Required prominence of the HIGH between two mouths, in `relief_pred` units
/// (the field spans [-1, 1]).
///
/// **This is the real separation rule.** Two mouths are two systems only if a
/// divide stands between them; two mouths in one low share a catchment, and
/// their trunks grow side by side down the same hollow and may cross — which
/// is precisely the near-parallel-channel defect that ended the previous
/// attempt's network round. Calibrated so the resulting spacing distribution
/// matches real major outlets; see `examples/divide_probe.rs`.
pub const MIN_DIVIDE_PROMINENCE: f64 = 0.14;

/// Minimum distance from a mouth to the nearest corner.
///
/// A mouth near a corner has almost no catchment behind it inside the tile,
/// and its trunk runs along the adjacent edge — which is the edge-hugging
/// trunk `heartland` spent a round diagnosing. Measured before this constant
/// existed: 23.6 % of mouths sat within 450 m of a corner, the closest at
/// 260 m. 500 m leaves 2000 m of usable edge, which still holds the four
/// mouths great plains can draw at `MOUTH_SEP_M`.
pub const MOUTH_CORNER_CLEARANCE_M: f64 = 500.0;

pub fn place(rng: &mut DetRng, p: &TrunkParams, relief_pred: &Grid<f64>, edge: Edge) -> Vec<Trunk> {
    if p.count == 0 {
        return Vec::new();
    }
    // Candidate mouths, scored by how LOW the predisposition field is just
    // inboard of the edge: a river leaves through a low, not over a rise.
    // 64 candidates is enough to resolve a 420 m separation on a 3 km edge.
    const CAND: usize = 64;
    let probe_in = 180.0;
    let (ic, is) = (math::cos(edge.inward()), math::sin(edge.inward()));
    let mut scored: Vec<(f64, f64)> = (0..CAND)
        .map(|i| {
            let c = MOUTH_CORNER_CLEARANCE_M / EXTENT_M;
            let t = c + (1.0 - 2.0 * c) * (i as f64 + 0.5) / CAND as f64;
            let m = edge.point(t);
            let probe = Vec2::new(m.x + ic * probe_in, m.y + is * probe_in);
            // low is good -> negate
            let mut s = -relief_pred.bilinear(probe);
            // a little noise so equal-relief edges do not always pick the
            // same t, and so the choice is not a pure argmax of a smooth field
            s += rng.range_f64(-0.15, 0.15);
            (t, s)
        })
        .collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));

    // Ground on the probe line, as a function of the along-edge parameter.
    let ground = |t: f64| {
        let m = edge.point(t);
        relief_pred.bilinear(Vec2::new(m.x + ic * probe_in, m.y + is * probe_in))
    };
    // Prominence of the highest ground between two along-positions, above
    // whichever end is higher. <= 0 means they sit in one low.
    let divide = |a: f64, b: f64| {
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let n = (((hi - lo) * EXTENT_M) / 16.0).ceil().max(2.0) as usize;
        let mut peak = f64::MIN;
        for i in 1..n {
            peak = peak.max(ground(lo + (hi - lo) * i as f64 / n as f64));
        }
        peak - ground(lo).max(ground(hi))
    };

    let mut chosen: Vec<f64> = Vec::new();
    for (t, _) in &scored {
        if chosen.len() >= p.count as usize {
            break;
        }
        let m = edge.point(*t);
        let ok = chosen.iter().all(|&c| {
            edge.point(c).distance(m) >= MOUTH_SEP_M && divide(c, *t) >= MIN_DIVIDE_PROMINENCE
        });
        if ok {
            chosen.push(*t);
        }
    }
    chosen.sort_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));

    // Split the declared inflow: the first trunk takes `dominance`, the rest
    // share what is left.
    let n = chosen.len();
    let c = MOUTH_CORNER_CLEARANCE_M / EXTENT_M;
    let trunks: Vec<Trunk> = chosen
        .iter()
        .enumerate()
        .map(|(i, &t)| {
            let share = if n == 1 {
                1.0
            } else if i == 0 {
                p.dominance
            } else {
                (1.0 - p.dominance) / (n as f64 - 1.0)
            };
            let jitter = rng.range_f64(-0.35, 0.35);
            let external = p.external_inflow_km2 * share;
            let mouth = edge.point(t);
            // Far-end SECTOR: the Voronoi cell of this mouth on the edge
            // parameter, so two trunks' far ends can never swap sides —
            // which is how two authored paths end up crossing mid-tile.
            let lo = if i == 0 { c } else { 0.5 * (chosen[i - 1] + t) };
            let hi = if i + 1 == n { 1.0 - c } else { 0.5 * (t + chosen[i + 1]) };
            let (far, through) = far_end(rng, relief_pred, edge, t, external, lo, hi);
            Trunk {
                mouth,
                azimuth_rad: edge.inward() + jitter,
                external_km2: external,
                far,
                through,
            }
        })
        .collect();
    trunks
}

/// Pick the far END of a trunk.
///
/// A trunk with real external inflow is a THROUGH-river: the catchment it
/// carries lies outside the window, so the river must enter through a far
/// edge. Entry candidates are scored the same way mouths are — through the
/// lows — on the edge OPPOSITE the base edge, offset along-edge from the
/// mouth so the river crosses the tile diagonally more often than dead
/// straight. Below the threshold the trunk is headwater: it rises to a HIGH
/// of the relief field in its own sector of the tile.
pub const THROUGH_KM2: f64 = 8.0;

fn far_end(
    rng: &mut DetRng,
    relief: &Grid<f64>,
    base: Edge,
    mouth_t: f64,
    external_km2: f64,
    sector_lo: f64,
    sector_hi: f64,
) -> (Vec2, bool) {
    if external_km2 >= THROUGH_KM2 {
        let opp = match base {
            Edge::South => Edge::North,
            Edge::North => Edge::South,
            Edge::West => Edge::East,
            Edge::East => Edge::West,
        };
        // score entry candidates through the lows, same probe the mouth used
        let probe_in = 180.0;
        let (ic, is) = (math::cos(opp.inward()), math::sin(opp.inward()));
        let c = MOUTH_CORNER_CLEARANCE_M / EXTENT_M;
        let (lo, hi) = (sector_lo.max(c), sector_hi.min(1.0 - c));
        let mut best = (f64::MAX, opp.point((lo + hi) * 0.5));
        for i in 0..48 {
            let t = lo + (hi - lo) * (i as f64 + 0.5) / 48.0;
            let m = opp.point(t);
            let probe = Vec2::new(m.x + ic * probe_in, m.y + is * probe_in);
            let mut v = relief.bilinear(probe) + rng.range_f64(-0.10, 0.10);
            // gentle preference for an entry offset from the mouth, so the
            // river crosses the tile rather than shooting straight over
            v += 0.25 * (1.0 - (t - mouth_t).abs());
            if v < best.0 {
                best = (v, m);
            }
        }
        (best.1, true)
    } else {
        // headwater: the HIGHEST relief inside the trunk's own sector
        let sector_lo = sector_lo.max(0.08).max(mouth_t - 0.30);
        let sector_hi = sector_hi.min(0.92).min(mouth_t + 0.30);
        let mut best = (f64::MIN, Vec2::new(EXTENT_M * 0.5, EXTENT_M * 0.5));
        for _ in 0..160 {
            let t = rng.range_f64(sector_lo, sector_hi);
            let depth = rng.range_f64(0.45, 0.88) * EXTENT_M;
            let (ic, is) = (math::cos(base.inward()), math::sin(base.inward()));
            let m = base.point(t);
            let p = Vec2::new(m.x + ic * depth, m.y + is * depth);
            if p.x < 150.0 || p.y < 150.0 || p.x > EXTENT_M - 150.0 || p.y > EXTENT_M - 150.0 {
                continue;
            }
            let v = relief.bilinear(p);
            if v > best.0 {
                best = (v, p);
            }
        }
        (best.1, false)
    }
}
