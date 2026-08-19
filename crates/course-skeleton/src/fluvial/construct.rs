//! CONSTRUCTIVE channel network — the tree is grown, then the erosion loop
//! is left to make it true.
//!
//! The reviewer's spec: one or two (maybe three) primary trunks correlated
//! with the macro terrain, dendritic tributaries climbing toward high
//! ground, everything draining to the trunks, no straight segments.
//!
//! WHAT THIS MODULE MAY NOT DO. `divides.rs` opens with the rule that
//! retired the previous generator: an authored divide that disagrees with
//! the flow field is the exact failure mode. So nothing here is ever
//! handed downstream as "the network". The construction happens BEFORE the
//! erosion — the tree's corridors are cut into C1's macro, and then the
//! ordinary carve runs on top. Corridors are the lowest ground when the
//! loop starts, so flow routes down them and fifteen iterations of stream
//! power reinforce them; the network that reaches S3 is still the one the
//! extraction reads off the built surface. The acceptance test is that the
//! two agree (`carried_vs_claimed` near 1), and it is measured, not
//! assumed.
//!
//! M1, in this file so far: the trunks.

use course_contracts::metadata::Edge;
use course_world::flow;
use course_world::grid::{Grid, GridSpec};
use course_world::math::Vec2;

/// Trunk polyline plus what it drains, MOUTH FIRST — the same convention
/// `Channel::pts` uses, so the two can be compared without reversing.
#[derive(Clone, Debug)]
pub struct Trunk {
    pub pts: Vec<Vec2>,
    /// Linear index of the mouth cell on the base edge.
    pub mouth: usize,
    /// Drained area at the mouth on the MACRO, m² — the ranking key.
    pub area_m2: f64,
}

/// Below this drained area a trunk has stopped being a trunk; the walk
/// ends and tributary growth takes over from there. 4e5 m² is where the
/// discharge classes put "large", and it is ~3× the extraction threshold.
pub const TRUNK_STOP_M2: f64 = 4.0e5;
/// Minimum separation between two trunk mouths. Without it the top two
/// accumulation cells on the base edge are neighbours on the same river
/// and the tile gets one trunk drawn twice.
pub const TRUNK_MOUTH_SEP_M: f64 = 420.0;
/// Meander wavelength band, as a multiple of... nothing: these are metres,
/// chosen an order of magnitude above the ~120 m routing wander, which is
/// too short to swing a trunk (measured: it raised mean sinuosity without
/// touching the straight-run tail).
pub const MEANDER_LAM_M: (f64, f64) = (400.0, 900.0);
/// Heading swing, radians. Sinuosity rises with this; the corpus wants
/// 1.06–1.10 at a 600 m window, which lands near 0.45 rad.
pub const MEANDER_OMEGA: f64 = 0.45;
/// Integration step for the sine-generated curve, metres.
const MEANDER_STEP_M: f64 = 12.0;

/// Which border cells water is allowed to leave through. Mirrors
/// `carve::outlet_mask` — one cell, for the reason documented there.
fn on_outlet_edge(spec: &GridSpec, edge: Edge, x: usize, y: usize) -> bool {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    match edge {
        Edge::S => y == 0,
        Edge::N => y == ny - 1,
        Edge::W => x == 0,
        Edge::E => x == nx - 1,
        Edge::CornerSw => y == 0 || x == 0,
        Edge::CornerSe => y == 0 || x == nx - 1,
        Edge::CornerNw => y == ny - 1 || x == 0,
        Edge::CornerNe => y == ny - 1 || x == nx - 1,
    }
}

/// Flow on the MACRO alone — no roughness, no wander, no erosion. This is
/// the field that answers "where would water go on the landform S1 built",
/// which is the question the trunk placement has to agree with.
///
/// Borders are rimmed exactly as the carve rims them, so the macro drains
/// to the same base edge the rest of the stage uses. Without it the macro
/// leaks out of whichever border happens to sit lowest and the trunks come
/// out pointing the wrong way.
pub fn macro_flow(spec: &GridSpec, macro_z: &Grid<f64>, base_edge: Edge, rim_m: f64) -> flow::FlowField {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut z = macro_z.clone();
    for y in 0..ny {
        for x in 0..nx {
            let on_border = y == 0 || y == ny - 1 || x == 0 || x == nx - 1;
            if on_border && !on_outlet_edge(spec, base_edge, x, y) {
                z.data[y * nx + x] += rim_m;
            }
        }
    }
    flow::route(&z)
}

/// Pick up to `count` trunks and walk each one upstream along maximum
/// accumulation.
///
/// Mouths are the highest-accumulation cells on the base edge, kept
/// `TRUNK_MOUTH_SEP_M` apart so two mouths of the same river do not both
/// qualify. The walk climbs the donor with the most drained area, which is
/// the main stem by definition, and stops when the stem stops being a
/// trunk.
pub fn trunks(
    spec: &GridSpec,
    ff: &flow::FlowField,
    base_edge: Edge,
    count: usize,
) -> Vec<Trunk> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let cell_area = cell * cell;
    let n = nx * ny;

    // donors per cell, so the upstream walk is O(1) per step
    let mut donors: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 0..n {
        let r = ff.rec[i];
        if r >= 0 {
            donors[r as usize].push(i as u32);
        }
    }

    let mut cands: Vec<(f64, usize)> = (0..n)
        .filter(|&i| on_outlet_edge(spec, base_edge, i % nx, i / nx))
        .map(|i| (ff.acc[i] as f64 * cell_area, i))
        .collect();
    cands.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));

    let mut out: Vec<Trunk> = Vec::new();
    for (area, mouth) in cands {
        if out.len() >= count.max(1) {
            break;
        }
        if area < TRUNK_STOP_M2 {
            break;
        }
        let (mx, my) = ((mouth % nx) as f64, (mouth / nx) as f64);
        if out.iter().any(|t| {
            let (px, py) = ((t.mouth % nx) as f64, (t.mouth / nx) as f64);
            ((mx - px).powi(2) + (my - py).powi(2)).sqrt() * cell < TRUNK_MOUTH_SEP_M
        }) {
            continue;
        }
        // upstream walk along the biggest donor
        let mut path = vec![mouth];
        let mut cur = mouth;
        loop {
            let Some(&next) = donors[cur]
                .iter()
                .max_by(|&&a, &&b| {
                    ff.acc[a as usize]
                        .cmp(&ff.acc[b as usize])
                        .then((b as usize).cmp(&(a as usize)))
                })
            else {
                break;
            };
            let next = next as usize;
            if (ff.acc[next] as f64) * cell_area < TRUNK_STOP_M2 {
                break;
            }
            path.push(next);
            cur = next;
            if path.len() > n {
                break; // defensive; the forest is acyclic by construction
            }
        }
        if path.len() < 4 {
            continue;
        }
        out.push(Trunk {
            pts: path
                .iter()
                .map(|&l| spec.world_of((l % nx) as u32, (l / nx) as u32))
                .collect(),
            mouth,
            area_m2: area,
        });
    }
    out
}

/// Sine-generated (Langbein–Leopold) meander applied to a polyline.
///
/// Three invariants, each of which cost a debugging round in the river
/// builder and are restated here because this is an independent port:
///
/// - the heading is referenced to the base line's TANGENT, never to a
///   lookahead POINT — point-homing at a large swing makes the path orbit
///   its slowly-advancing target and the result is a chain of curls;
/// - the valley progress rate must be `J0(ω)`, the Langbein–Leopold
///   coupling. A fixed rate leaves surplus path length that has to go
///   somewhere, and it goes into self-crossings;
/// - a weak cross-track correction keeps the belt centred, and tapers kill
///   the wind at both ends so the mouth still arrives where it should.
pub fn meander(base: &[Vec2], lam: f64, omega: f64, phase: f64) -> Vec<Vec2> {
    if base.len() < 3 || lam <= 0.0 || omega <= 0.0 {
        return base.to_vec();
    }
    let mut arcs = vec![0.0f64; base.len()];
    for i in 1..base.len() {
        let (a, b) = (base[i - 1], base[i]);
        arcs[i] = arcs[i - 1] + ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    }
    let total = arcs[base.len() - 1];
    if total < 2.5 * lam {
        return base.to_vec(); // too short to carry a full wave
    }
    let at = |s: f64| -> Vec2 {
        let s = s.clamp(0.0, total);
        let i = arcs.partition_point(|&v| v < s).min(base.len() - 1);
        base[i]
    };
    // J0 by its series — the argument never exceeds ~1 rad here.
    let j0 = |x: f64| {
        let x2 = x * x;
        1.0 - x2 / 4.0 + x2 * x2 / 64.0 - x2 * x2 * x2 / 2304.0
    };
    let mut p = base[0];
    let mut out = vec![p];
    let mut proj = 0.0f64;
    let max_iter = (total / MEANDER_STEP_M * 6.0) as usize + 200;
    for _ in 0..max_iter {
        if proj >= total - 0.4 * lam {
            break;
        }
        let a = at((proj - 12.0).max(0.0));
        let b = at((proj + 12.0).min(total));
        let th_tan = libm::atan2(b.y - a.y, b.x - a.x);
        // taper the wind in and out so the ends stay put
        let t_in = (proj / (0.8 * lam)).clamp(0.0, 1.0);
        let t_out = ((total - proj) / (0.8 * lam)).clamp(0.0, 1.0);
        let om = omega * t_in * t_out;
        // cross-track correction against the base line
        let anchor = at(proj);
        let e = Vec2::new(p.x - anchor.x, p.y - anchor.y);
        let e_lat = -e.x * libm::sin(th_tan) + e.y * libm::cos(th_tan);
        let corr = (-0.9 * e_lat / lam).clamp(-0.4, 0.4);
        let th = th_tan + om * libm::sin(proj / lam * std::f64::consts::TAU + phase) + corr;
        p = Vec2::new(
            p.x + MEANDER_STEP_M * libm::cos(th),
            p.y + MEANDER_STEP_M * libm::sin(th),
        );
        out.push(p);
        proj += MEANDER_STEP_M * j0(om).max(0.25);
    }
    out.push(*base.last().unwrap());
    out
}

/// Meander only as far as the target sinuosity, by bisecting the swing.
///
/// A FIXED swing is the wrong control here and the measurement says so:
/// trunks read straight off the macro already carry 1.03–1.13 at a 600 m
/// window against a corpus 1.06–1.10, so a blanket ω overshoots the tiles
/// that were already winding (measured 1.203 and 1.216) while barely
/// helping the ones that were not. Seek the target instead: the straight
/// trunks get the whole swing, the sinuous ones get almost none, and the
/// archetype's own corpus figure is the setpoint.
///
/// Eight bisection steps on a monotone response, so it is deterministic
/// and costs eight cheap integrations of a polyline.
pub fn meander_to(base: &[Vec2], lam: f64, target: f64, phase: f64) -> Vec<Vec2> {
    let raw = window_sinuosity(base, 600.0).unwrap_or(1.0);
    if raw >= target {
        return base.to_vec(); // already at or past the corpus figure
    }
    let (mut lo, mut hi) = (0.0f64, MEANDER_OMEGA);
    let mut best = base.to_vec();
    for _ in 0..8 {
        let mid = 0.5 * (lo + hi);
        let cand = meander(base, lam, mid, phase);
        let s = window_sinuosity(&cand, 600.0).unwrap_or(raw);
        if s > target {
            hi = mid;
        } else {
            lo = mid;
            best = cand;
        }
    }
    best
}

/// Sinuosity over a sliding window, the same statistic `planform.rs`
/// reports — arc over chord, so 1.0 is dead straight.
pub fn window_sinuosity(pts: &[Vec2], window_m: f64) -> Option<f64> {
    if pts.len() < 3 {
        return None;
    }
    let mut arcs = vec![0.0f64; pts.len()];
    for i in 1..pts.len() {
        let (a, b) = (pts[i - 1], pts[i]);
        arcs[i] = arcs[i - 1] + ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
    }
    let total = arcs[pts.len() - 1];
    if total < window_m {
        return None;
    }
    let mut v: Vec<f64> = Vec::new();
    let mut i = 0usize;
    for j in 0..pts.len() {
        while arcs[j] - arcs[i] > window_m {
            i += 1;
        }
        if arcs[j] - arcs[i] >= window_m * 0.98 {
            let chord = ((pts[j].x - pts[i].x).powi(2) + (pts[j].y - pts[i].y).powi(2)).sqrt();
            if chord > 1e-6 {
                v.push((arcs[j] - arcs[i]) / chord);
            }
        }
    }
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    Some(v[v.len() / 2])
}
