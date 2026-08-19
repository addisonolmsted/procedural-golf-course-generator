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

/// Smoothing length for the macro's structure tensor. Long enough that
/// the answer is the LANDFORM's grain and not a single wave crest.
pub const GRAIN_SIGMA_M: f64 = 300.0;
/// A donor has to carry at least this share of the biggest donor's
/// discharge to be considered at all. Without it the walk can follow a
/// well-aligned trickle off the main stem; with it the grain only ever
/// decides between credible continuations.
pub const GRAIN_ACC_SHARE: f64 = 0.35;
/// Weight of alignment against normalized discharge in the donor score.
/// At 1.0 a perfectly aligned, fully coherent donor can beat a rival
/// carrying up to ~65 % more water — enough to follow a strike valley,
/// not enough to leave the drainage.
pub const GRAIN_W: f64 = 1.0;

/// The macro's grain: `(along_axis_rad, coherence)` per cell.
///
/// Structure tensor of the lowpass gradient, smoothed. The MINOR
/// eigenvector is the direction the surface varies least — the ridge or
/// valley axis, which is the boundary between high and low ground a real
/// trunk tends to run along. Coherence is `(λ1−λ2)/(λ1+λ2)`: 0 where the
/// macro has no grain at all, and the caller must gate on it, because an
/// axis angle taken from an isotropic patch is noise.
pub fn grain(spec: &GridSpec, macro_z: &Grid<f64>) -> (Vec<f64>, Vec<f64>) {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let n = nx * ny;
    let (mut gx, mut gy) = (vec![0.0f64; n], vec![0.0f64; n]);
    for y in 0..ny {
        for x in 0..nx {
            let xm = x.saturating_sub(1);
            let xp = (x + 1).min(nx - 1);
            let ym = y.saturating_sub(1);
            let yp = (y + 1).min(ny - 1);
            gx[y * nx + x] = (macro_z.data[y * nx + xp] - macro_z.data[y * nx + xm])
                / ((xp - xm) as f64 * cell).max(1e-9);
            gy[y * nx + x] = (macro_z.data[yp * nx + x] - macro_z.data[ym * nx + x])
                / ((yp - ym) as f64 * cell).max(1e-9);
        }
    }
    let sig = (GRAIN_SIGMA_M / cell).max(1.0);
    let jxx = blur(&gx.iter().zip(&gx).map(|(a, b)| a * b).collect::<Vec<_>>(), nx, ny, sig);
    let jyy = blur(&gy.iter().zip(&gy).map(|(a, b)| a * b).collect::<Vec<_>>(), nx, ny, sig);
    let jxy = blur(&gx.iter().zip(&gy).map(|(a, b)| a * b).collect::<Vec<_>>(), nx, ny, sig);
    let mut along = vec![0.0f64; n];
    let mut coh = vec![0.0f64; n];
    for i in 0..n {
        let (a, b, c) = (jxx[i], jyy[i], jxy[i]);
        let tr = a + b;
        let disc = ((a - b) * (a - b) + 4.0 * c * c).max(0.0).sqrt();
        coh[i] = if tr > 1e-15 { (disc / tr).clamp(0.0, 1.0) } else { 0.0 };
        // major-gradient direction, then a quarter turn onto the axis
        along[i] = 0.5 * libm::atan2(2.0 * c, a - b) + std::f64::consts::FRAC_PI_2;
    }
    (along, coh)
}

/// Separable Gaussian, σ in cells, edge-clamped — a local copy so this
/// module does not reach into `carve`'s private helpers.
fn blur(src: &[f64], nx: usize, ny: usize, sigma: f64) -> Vec<f64> {
    let r = (sigma * 3.0).ceil().max(1.0) as i64;
    let k: Vec<f64> = (-r..=r)
        .map(|d| {
            let t = d as f64 / sigma;
            libm::exp(-0.5 * t * t)
        })
        .collect();
    let s: f64 = k.iter().sum();
    let k: Vec<f64> = k.iter().map(|v| v / s).collect();
    let mut tmp = vec![0.0f64; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let mut acc = 0.0;
            for (i, w) in k.iter().enumerate() {
                let xx = (x as i64 + i as i64 - r).clamp(0, nx as i64 - 1) as usize;
                acc += src[y * nx + xx] * w;
            }
            tmp[y * nx + x] = acc;
        }
    }
    let mut out = vec![0.0f64; nx * ny];
    for x in 0..nx {
        for y in 0..ny {
            let mut acc = 0.0;
            for (i, w) in k.iter().enumerate() {
                let yy = (y as i64 + i as i64 - r).clamp(0, ny as i64 - 1) as usize;
                acc += tmp[yy * nx + x] * w;
            }
            out[y * nx + x] = acc;
        }
    }
    out
}

/// Pick up to `count` trunks and walk each one upstream along maximum
/// accumulation.
///
/// Mouths are the highest-accumulation cells on the base edge, kept
/// `TRUNK_MOUTH_SEP_M` apart so two mouths of the same river do not both
/// qualify. The walk climbs the donor with the most drained area, which is
/// the main stem by definition, and stops when the stem stops being a
/// trunk.
///
/// The walk is GRAIN-AWARE. Maximum accumulation alone sends the trunk
/// down the macro's steepest overall descent, which on a wave-sum macro
/// crosses the crests: measured, our hill-country trunks ran at 44° to the
/// macro's grain against a corpus 16°. Real trunks exploit structure —
/// strike valleys, range-front streams — and run along the boundary
/// between high and low ground. So among donors that carry a credible
/// share of the stem's discharge, the walk prefers the one heading along
/// the local grain, weighted by how coherent that grain is. Where the
/// macro is isotropic every candidate scores the same and discharge
/// decides, which is the old behaviour.
///
/// Following the grain and descending are in tension — a boundary that
/// runs level cannot carry a river — so this can only prefer along-grain
/// AMONG descending options. `grain_cross_share` reports how often it had
/// to cross anyway.
pub fn trunks(
    spec: &GridSpec,
    ff: &flow::FlowField,
    base_edge: Edge,
    count: usize,
    grain: Option<&(Vec<f64>, Vec<f64>)>,
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
        // upstream walk: discharge sets the field of candidates, the
        // macro's grain picks among them
        let mut path = vec![mouth];
        let mut cur = mouth;
        loop {
            if donors[cur].is_empty() {
                break;
            }
            let best_acc = donors[cur].iter().map(|&d| ff.acc[d as usize]).max().unwrap_or(0);
            if (best_acc as f64) * cell_area < TRUNK_STOP_M2 {
                break;
            }
            // direction is taken over the last few cells, not one step: a
            // single D8 hop only ever points in eight directions, which is
            // far too coarse to compare against a continuous axis.
            let back = path[path.len().saturating_sub(4)];
            let (bx, by) = ((back % nx) as f64, (back / nx) as f64);
            let mut best: Option<(f64, usize)> = None;
            for &d in &donors[cur] {
                let d = d as usize;
                let a = ff.acc[d] as f64;
                if a < GRAIN_ACC_SHARE * best_acc as f64 || a * cell_area < TRUNK_STOP_M2 {
                    continue;
                }
                let mut score = a / best_acc as f64;
                if let Some((along, coh)) = grain {
                    let (dx, dy) = ((d % nx) as f64 - bx, (d / nx) as f64 - by);
                    let len = (dx * dx + dy * dy).sqrt();
                    if len > 1e-9 {
                        let th = libm::atan2(dy, dx);
                        let align = libm::cos(th - along[cur]).abs();
                        score += GRAIN_W * coh[cur] * align;
                    }
                }
                // deterministic tie-break on the linear index
                if best.is_none_or(|(s, i)| score > s || (score == s && d < i)) {
                    best = Some((score, d));
                }
            }
            let Some((_, next)) = best else { break };
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

/// Cost weights for the constructed trunk line. Descent is penalised
/// because the walk runs UPSTREAM; height above the local surroundings is
/// penalised so the line stays in the low corridor rather than climbing a
/// flank; misalignment with the grain is penalised so it prefers the
/// boundary between high and low ground.
pub const TRUNK_W_DESCENT: f64 = 2.0;
pub const TRUNK_W_HIGH: f64 = 1.6;
pub const TRUNK_W_GRAIN: f64 = 0.6;
/// Cost of TURNING. Without it the grain term steers the line sideways at
/// every step and the result is a scribble: measured sinuosity 1.34-2.41
/// against a corpus 1.06-1.10. This is the same lesson the meander taught
/// — a directional prior has to bound the perturbation, or the
/// perturbation becomes the path.
pub const TRUNK_W_TURN: f64 = 1.5;
/// Neighbourhood the "height above local surroundings" is measured over.
pub const TRUNK_RELH_M: f64 = 400.0;

/// A trunk CONSTRUCTED as a least-cost line, not read off the flow field.
///
/// Why this exists. The first cut chose among donors on the macro's D8
/// forest and weighted them by grain alignment — and it changed nothing
/// (measured: 46°→48°, 29°→29°, 17°→17° against the grain). The reason is
/// structural: on a receiver forest most cells have exactly ONE donor
/// carrying a credible share of the discharge, because a place with two is
/// by definition a confluence and those are rare. A selection rule cannot
/// steer a path when there is nothing to select between. The trunk's route
/// is fixed by the macro's flow field, and the only way to move it is to
/// stop deriving it from that field.
///
/// So this walks the macro directly: from the mouth, step to the
/// 8-neighbour that minimises descent, height above the local
/// surroundings, and misalignment with the grain, subject to strictly
/// increasing distance from the mouth (which guarantees termination and
/// forward progress). The result is a line that follows the low corridor
/// and the toe of a ridge — and it is only a proposal until the corridor
/// carve makes the flow field agree with it, which is the step that keeps
/// the derivation invariant intact.
pub fn trunk_line(
    spec: &GridSpec,
    macro_z: &Grid<f64>,
    grain: &(Vec<f64>, Vec<f64>),
    mouth: usize,
    max_len_m: f64,
) -> Vec<Vec2> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let n = nx * ny;
    let (along, coh) = grain;
    // height above the local surroundings, and a scale to normalise it
    let lp = blur(&macro_z.data, nx, ny, (TRUNK_RELH_M / cell).max(1.0));
    let relh: Vec<f64> = (0..n).map(|i| macro_z.data[i] - lp[i]).collect();
    let mut sorted: Vec<f64> = relh.iter().map(|v| v.abs()).collect();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let scale = sorted[sorted.len() * 9 / 10].max(0.5);

    let (mx, my) = ((mouth % nx) as f64, (mouth / nx) as f64);
    let mut visited = vec![false; n];
    let mut cur = mouth;
    let mut out = vec![spec.world_of((mouth % nx) as u32, (mouth / nx) as u32)];
    visited[cur] = true;
    let mut len = 0.0f64;
    let mut path = vec![mouth];
    while len < max_len_m {
        let (cx, cy) = ((cur % nx) as i64, (cur / nx) as i64);
        // heading over the last few cells, for the turn cost; a single
        // 8 m hop only points in eight directions and is far too coarse
        let heading = if path.len() >= 5 {
            let b = path[path.len() - 5];
            let (bx, by) = ((b % nx) as f64, (b / nx) as f64);
            let (dx, dy) = (cx as f64 - bx, cy as f64 - by);
            if dx.hypot(dy) > 1e-9 { Some(libm::atan2(dy, dx)) } else { None }
        } else {
            None
        };
        let d_cur = (((cx as f64 - mx).powi(2)) + ((cy as f64 - my).powi(2))).sqrt();
        let mut best: Option<(f64, usize)> = None;
        for dy in -1i64..=1 {
            for dx in -1i64..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let (xx, yy) = (cx + dx, cy + dy);
                if xx <= 0 || yy <= 0 || xx >= nx as i64 - 1 || yy >= ny as i64 - 1 {
                    continue;
                }
                let nb = yy as usize * nx + xx as usize;
                if visited[nb] {
                    continue;
                }
                let d_nb = (((xx as f64 - mx).powi(2)) + ((yy as f64 - my).powi(2))).sqrt();
                if d_nb <= d_cur {
                    continue; // forward progress, and it is what terminates the walk
                }
                let dz = macro_z.data[nb] - macro_z.data[cur];
                let th = libm::atan2(dy as f64, dx as f64);
                let align = libm::cos(th - along[cur]).abs();
                let turn = match heading {
                    Some(h) => 1.0 - libm::cos(th - h),
                    None => 0.0,
                };
                let c = TRUNK_W_DESCENT * (-dz).max(0.0) / scale
                    + TRUNK_W_HIGH * relh[nb].max(0.0) / scale
                    + TRUNK_W_GRAIN * coh[cur] * (1.0 - align)
                    + TRUNK_W_TURN * turn;
                if best.is_none_or(|(s, i)| c < s || (c == s && nb < i)) {
                    best = Some((c, nb));
                }
            }
        }
        let Some((_, next)) = best else { break };
        let step = ((next % nx) as f64 - (cur % nx) as f64)
            .hypot((next / nx) as f64 - (cur / nx) as f64)
            * cell;
        visited[next] = true;
        path.push(next);
        out.push(spec.world_of((next % nx) as u32, (next / nx) as u32));
        len += step;
        cur = next;
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

/// Corridor half-width of the pre-carved trunk, metres. Wide enough that
/// D8 cannot step out of it, narrow enough that the erosion loop still
/// owns the valley's final width.
pub const CORRIDOR_HW_M: f64 = 44.0;
/// Minimum rise per sample walking UPSTREAM along the bed. The bed is laid
/// by running maximum from the mouth so it descends monotonically to base
/// level; without that a corridor can dam itself and the fill turns it
/// into a lake, which is the failure the river builder hit twice.
pub const BED_RISE_M: f64 = 0.012;

/// Build the trunks and cut their corridors into `z`, returning the lines.
///
/// This is the whole of step one: the trunk is CONSTRUCTED — placed from
/// the macro's own low ground and ridge toes, meandered to the archetype's
/// corpus sinuosity — and then cut in as the lowest ground on the tile
/// before the erosion loop runs. The loop then routes down it and fifteen
/// iterations of stream power reinforce it, so the network that reaches
/// extraction is still read off the built surface.
///
/// The corridor is deliberately modest. It exists to say WHERE the trunk
/// goes, not to be the finished valley — the catena and the erosion own
/// its section, and those are calibrated.
#[allow(clippy::too_many_arguments)]
pub fn stamp_trunks(
    spec: &GridSpec,
    z: &mut Grid<f64>,
    base_edge: Edge,
    count: usize,
    cut_m: f64,
    target_sinuosity: f64,
    phases: &[f64],
) -> Vec<Vec<Vec2>> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let ff = flow::route(z);
    let gr = grain(spec, z);
    let seeds = trunks(spec, &ff, base_edge, count, Some(&gr));
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    for (k, t) in seeds.iter().enumerate() {
        // The FLOW path, not `trunk_line`. Measured: `trunk_line` only
        // guarantees increasing distance from the mouth, not descent, so
        // wherever the ground falls away from the mouth the monotone bed
        // rises above the surface and nothing gets cut — the corridor came
        // out discontinuous and the erosion loop ignored it (median gap
        // 73-161 m between the proposal and the network it produced). The
        // flow path descends by construction; the meander then moves it
        // laterally, which the bed can still follow.
        let raw = t.pts.clone();
        if raw.len() < 8 {
            continue;
        }
        let (lo, hi) = MEANDER_LAM_M;
        let u = phases.get(2 * k).copied().unwrap_or(0.5);
        let lam = lo + (hi - lo) * u;
        let phase = phases.get(2 * k + 1).copied().unwrap_or(0.0) * std::f64::consts::TAU;
        let line = meander_to(&raw, lam, target_sinuosity, phase);

        // Resample the line at half-cell steps and lay a monotone bed:
        // walking upstream from the mouth the bed may only rise, so the
        // corridor drains to base level by construction.
        let mut pts: Vec<Vec2> = Vec::new();
        for w in line.windows(2) {
            let (a, b) = (w[0], w[1]);
            let len = (b.x - a.x).hypot(b.y - a.y).max(1e-9);
            let steps = ((len / (cell * 0.5)).ceil() as usize).max(1);
            for s in 0..steps {
                let t = s as f64 / steps as f64;
                pts.push(Vec2::new(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
            }
        }
        pts.push(*line.last().unwrap());
        let at = |p: Vec2| -> Option<usize> {
            let (x, y) = ((p.x / cell).round(), (p.y / cell).round());
            if x < 0.0 || y < 0.0 || x >= nx as f64 || y >= ny as f64 {
                return None;
            }
            Some(y as usize * nx + x as usize)
        };
        let mut bed: Vec<f64> = Vec::with_capacity(pts.len());
        for (i, p) in pts.iter().enumerate() {
            let Some(lin) = at(*p) else {
                bed.push(bed.last().copied().unwrap_or(0.0));
                continue;
            };
            // depth tapers from the mouth upstream: a trunk is deepest
            // where it carries the most water
            let f = 1.0 - (i as f64 / pts.len() as f64);
            let want = z.data[lin] - cut_m * (0.25 + 0.75 * f);
            bed.push(match bed.last() {
                Some(&prev) => want.max(prev + BED_RISE_M),
                None => want,
            });
        }
        // Stamp: every cell within the corridor is pulled down to the bed,
        // with a cosine shoulder so the edge is not a wall.
        let r = (CORRIDOR_HW_M / cell).ceil() as i64;
        for (i, p) in pts.iter().enumerate() {
            let Some(lin) = at(*p) else { continue };
            let (cx, cy) = ((lin % nx) as i64, (lin / nx) as i64);
            for dy in -r..=r {
                for dx in -r..=r {
                    let (xx, yy) = (cx + dx, cy + dy);
                    if xx <= 0 || yy <= 0 || xx >= nx as i64 - 1 || yy >= ny as i64 - 1 {
                        continue;
                    }
                    let d = ((dx * dx + dy * dy) as f64).sqrt() * cell;
                    if d > CORRIDOR_HW_M {
                        continue;
                    }
                    let w = 0.5 * (1.0 + libm::cos(std::f64::consts::PI * d / CORRIDOR_HW_M));
                    let j = yy as usize * nx + xx as usize;
                    let target = bed[i] + (1.0 - w) * cut_m;
                    if target < z.data[j] {
                        z.data[j] = target;
                    }
                }
            }
        }
        out.push(line);
    }
    out
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
