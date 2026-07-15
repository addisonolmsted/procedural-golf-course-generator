//! Simulated-annealing polish over a constructed layout.
//!
//! Fixed iteration count and a single DetRng stream keep the pass
//! deterministic. Hard constraints are enforced by move rejection (an
//! accepted state is always fully valid); the objective trades site quality,
//! length adherence, curved walk lengths, loop adherence, and fairway grade.
//! Walks are cached and recomputed only when a move touches their endpoints
//! or its new geometry crosses their cached polyline.

use golf_core::det::DetRng;
use golf_core::math::{self, Vec2};

use crate::construct::{Ctx, Placed};
use crate::geometry::{corridor_mean_slope, crosses_creek, drive_drop, hole_dz, segments};
use crate::walk::{corridor_mask, find_walk, walk_crosses_play, WalkGrid};

/// Annealing iterations (fixed — determinism and predictable cost).
const ITERS: usize = 1600;
const T0: f64 = 0.60;
const T1: f64 = 0.02;

/// A complete valid layout: 9 holes + their walks (walk i leaves green i;
/// hole 9's walk returns to the clubhouse).
#[derive(Clone, Debug)]
pub struct Solution {
    pub holes: Vec<Placed>,
    pub walks: Vec<(Vec<Vec2>, f64)>,
}

pub(crate) fn objective(ctx: &Ctx, holes: &[Placed], walks: &[(Vec<Vec2>, f64)]) -> f64 {
    let mut obj = 0.0;
    for (i, h) in holes.iter().enumerate() {
        obj += ctx.fields.greens[h.gsite].score;
        obj += 0.5 * ctx.fields.tees[h.tsite].score;
        if crosses_creek(ctx.ct, h.pts[0], h.pts[1], 30.0, 200.0) {
            obj += 0.4;
        }
        obj -= 6.0 * (h.len - h.target).abs() / h.target;
        let green = *h.pts.last().unwrap();
        obj -= ctx.relax.loop_w * 0.002 * green.distance(ctx.lp.target(ctx.fr[i]));
        let mut ms = 0.0;
        for (a, b) in segments(&h.pts) {
            ms += corridor_mean_slope(ctx.ct, a, b);
        }
        obj -= 1.5 * ms / (h.pts.len() - 1) as f64;
        // Elevation shape, fit to the atlas: holes play near-level (median
        // Δz ≈ 0), tees prefer to sit a little above the drive landing zone
        // (real median +1 m, p90 +7 m), and blind uphill drives cost.
        let dz = hole_dz(ctx.ct, &h.pts);
        obj -= 0.6 * ((dz - 8.0).max(0.0) / 10.0);
        obj -= 0.3 * ((-dz - 20.0).max(0.0) / 10.0);
        let drop = drive_drop(ctx.ct, &h.pts);
        obj += 0.45 * (drop / 7.0).clamp(0.0, 1.0);
        obj -= 0.30 * ((-drop - 5.0) / 8.0).clamp(0.0, 1.0);
        // Sustained steepness past the opening stretch (soft ~7% cap) and
        // blind opening shots cost; par 3s that connect adjacent high
        // ground with a carry dip between earn a premium.
        obj -= 0.12 * crate::geometry::steep_grade_pen(ctx.ct, &h.pts);
        let landing = {
            let mut acc = 0.0;
            let mut p = *h.pts.last().unwrap();
            'walk: for w in h.pts.windows(2) {
                let seg = w[0].distance(w[1]);
                if acc + seg >= 137.0 && seg > 0.0 {
                    p = w[0].lerp(w[1], (137.0 - acc) / seg);
                    break 'walk;
                }
                acc += seg;
            }
            p
        };
        if crate::geometry::blind_tee(ctx.ct, h.pts[0], landing) {
            obj -= 0.5;
        }
        if h.par == 3 {
            let pt = ctx.fields.prominence
                [crate::fields::cell_of(&ctx.fields.spec, h.pts[0])];
            let pg = ctx.fields.prominence
                [crate::fields::cell_of(&ctx.fields.spec, *h.pts.last().unwrap())];
            let hi2hi = (pt / 5.0).clamp(0.0, 1.0) * (pg / 5.0).clamp(0.0, 1.0);
            obj += 0.6 * hi2hi;
        }
    }
    obj -= 0.010 * walks.iter().map(|w| w.1).sum::<f64>();
    obj
}

fn all_segs(holes: &[Placed]) -> Vec<(Vec2, Vec2)> {
    holes.iter().flat_map(|h| segments(&h.pts)).collect()
}

/// Recompute the walks in `idxs` for the candidate holes; `None` if any walk
/// becomes infeasible.
fn recompute_walks(
    ctx: &Ctx,
    wgrid: &WalkGrid,
    holes: &[Placed],
    walks: &mut [(Vec<Vec2>, f64)],
    idxs: &[usize],
) -> bool {
    if idxs.is_empty() {
        return true;
    }
    let segs = all_segs(holes);
    let mask = corridor_mask(&wgrid.spec, &segs);
    for &k in idxs {
        let from = *holes[k].pts.last().unwrap();
        let to = if k < 8 {
            holes[k + 1].pts[0]
        } else {
            ctx.clubhouse
        };
        match find_walk(wgrid, &mask, &segs, from, to) {
            Some(w) => walks[k] = w,
            None => return false,
        }
    }
    true
}

/// Walk indices invalidated by hole `i` taking new geometry: endpoint walks
/// plus any cached walk the new line now crosses.
fn affected_walks(
    holes: &[Placed],
    walks: &[(Vec<Vec2>, f64)],
    i: usize,
    tee_moved: bool,
    green_moved: bool,
) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    if green_moved {
        out.push(i);
    }
    if tee_moved && i > 0 {
        out.push(i - 1);
    }
    let segs: Vec<(Vec2, Vec2)> = segments(&holes[i].pts).collect();
    for (k, w) in walks.iter().enumerate() {
        if !out.contains(&k) && walk_crosses_play(&w.0, &segs) {
            out.push(k);
        }
    }
    out.sort_unstable();
    out
}

/// Weighted index into a ranked pool: quadratic bias toward the top.
fn ranked_pick(rng: &mut DetRng, len: usize) -> usize {
    let n = len.min(20);
    let u = rng.next_f64();
    ((u * u * n as f64) as usize).min(n - 1)
}

pub(crate) fn anneal(ctx: &Ctx, rng: &mut DetRng, wgrid: &WalkGrid, sol: &mut Solution) {
    let mut cur_obj = objective(ctx, &sol.holes, &sol.walks);
    let mut best = sol.clone();
    let mut best_obj = cur_obj;

    for it in 0..ITERS {
        let t = T0 * math::pow(T1 / T0, it as f64 / ITERS as f64);
        let u = rng.next_f64();

        // Pick the move: (kind, hole index).
        let (kind, i) = if u < 0.35 {
            (0u8, rng.below(9)) // green re-pick
        } else if u < 0.60 {
            // dogleg re-draw on a par-4/5 hole
            let idxs: Vec<usize> = (0..9).filter(|&k| sol.holes[k].par > 3).collect();
            (1, idxs[rng.below(idxs.len())])
        } else if u < 0.85 {
            (2, rng.below(9)) // tee re-pick
        } else {
            // target the longest walk: shorten it by moving its destination
            let (wi, _) = sol
                .walks
                .iter()
                .enumerate()
                .max_by(|a, b| a.1 .1.partial_cmp(&b.1 .1).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap();
            if wi == 8 {
                (0, 8) // re-pick green 9 (the clubhouse end is fixed)
            } else {
                (2, wi + 1)
            }
        };

        let mut cand = sol.holes.clone();
        let others: Vec<&Placed> = cand
            .iter()
            .enumerate()
            .filter_map(|(k, h)| (k != i).then_some(h))
            .collect();

        let (tee_moved, green_moved) = match kind {
            0 => {
                // Green re-pick from the ranked pool, keeping the next-tee band.
                let tee = cand[i].pts[0];
                let pool = ctx.green_pool(i, tee);
                if pool.is_empty() {
                    continue;
                }
                let (gi, _) = pool[ranked_pick(rng, pool.len())];
                let gpos = ctx.fields.greens[gi].pos;
                if i < 8 {
                    let d = gpos.distance(cand[i + 1].pts[0]);
                    if !(crate::construct::WALK_MIN..=ctx.relax.walk_max).contains(&d) {
                        continue;
                    }
                }
                let Some(mut h) = ctx.realize(rng, i, tee, gpos) else {
                    continue;
                };
                h.gsite = gi;
                h.tsite = cand[i].tsite;
                if !ctx.hard_ok(&h.pts, &others) {
                    continue;
                }
                cand[i] = h;
                (false, true)
            }
            1 => {
                let (tee, green) = (cand[i].pts[0], *cand[i].pts.last().unwrap());
                let Some(mut h) = ctx.realize(rng, i, tee, green) else {
                    continue;
                };
                h.gsite = cand[i].gsite;
                h.tsite = cand[i].tsite;
                if !ctx.hard_ok(&h.pts, &others) {
                    continue;
                }
                cand[i] = h;
                (false, false)
            }
            _ => {
                // Tee re-pick: near the clubhouse for hole 1, else in the walk
                // band of the previous green.
                let green = *cand[i].pts.last().unwrap();
                let pool: Vec<usize> = if i == 0 {
                    let mut p: Vec<(usize, f64)> = ctx
                        .fields
                        .tees
                        .iter()
                        .enumerate()
                        .filter_map(|(ti, s)| {
                            let d = s.pos.distance(ctx.clubhouse);
                            (55.0..=200.0).contains(&d).then_some((ti, s.score - 0.008 * d))
                        })
                        .collect();
                    p.sort_by(|a, b| {
                        b.1.partial_cmp(&a.1)
                            .unwrap_or(std::cmp::Ordering::Equal)
                            .then(ctx.fields.tees[a.0].cell.cmp(&ctx.fields.tees[b.0].cell))
                    });
                    p.into_iter().map(|(ti, _)| ti).collect()
                } else {
                    let prev_green = *cand[i - 1].pts.last().unwrap();
                    ctx.tee_pool(i, prev_green, &others)
                };
                if pool.is_empty() {
                    continue;
                }
                let ti = pool[ranked_pick(rng, pool.len())];
                let tpos = ctx.fields.tees[ti].pos;
                let Some(mut h) = ctx.realize(rng, i, tpos, green) else {
                    continue;
                };
                h.gsite = cand[i].gsite;
                h.tsite = ti;
                if !ctx.hard_ok(&h.pts, &others) {
                    continue;
                }
                cand[i] = h;
                (true, false)
            }
        };

        let idxs = affected_walks(&cand, &sol.walks, i, tee_moved, green_moved);
        let mut cand_walks = sol.walks.clone();
        if !recompute_walks(ctx, wgrid, &cand, &mut cand_walks, &idxs) {
            continue;
        }

        let new_obj = objective(ctx, &cand, &cand_walks);
        let d = new_obj - cur_obj;
        if d >= 0.0 || rng.next_f64() < math::exp(d / t) {
            sol.holes = cand;
            sol.walks = cand_walks;
            cur_obj = new_obj;
            if cur_obj > best_obj {
                best_obj = cur_obj;
                best = sol.clone();
            }
        }
    }

    if best_obj > cur_obj {
        *sol = best;
    }
}
