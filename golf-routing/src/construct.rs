//! Greedy returning-nine construction with backtracking.
//!
//! Holes are placed in play order around a soft loop schedule anchored at the
//! clubhouse: cumulative-length fractions map to target bearings around a
//! loop center, which is what brings hole 9 home. All hard constraints
//! (45 m buffers, water rules, box containment) are enforced at placement;
//! a shared backtrack budget bounds the search deterministically.

use golf_core::det::DetRng;
use golf_core::math::{self, Vec2};
use golf_terrain::CourseTerrain;

use crate::fields::Fields;
use crate::geometry::{
    self, corridor_grade_ok, corridor_water_ok, in_box, point_seg_dist, seg_seg_dist,
    seg_seg_intersect, solve_dogleg, solve_par5, BUFFER_M,
};

/// Segments of play may not pass closer than this to the clubhouse.
const CLUB_CLEAR: f64 = 50.0;
/// Next tee distance band from the previous green (lower bound keeps the
/// 45 m inter-hole buffer satisfiable at the connection).
pub(crate) const WALK_MIN: f64 = 46.0;
/// Box margin for all play geometry (the corridor has width).
const GEO_MARGIN: f64 = 20.0;

/// Relaxation ladder stage.
#[derive(Clone, Copy, Debug)]
pub struct Relax {
    pub stage: u8,
    /// Par-3 length tolerance (fraction of target).
    pub len_tol: f64,
    pub walk_max: f64,
    pub loop_w: f64,
    /// Hole-9 green anchor radius around the clubhouse.
    pub anchor9: f64,
    /// Corridor-grade backstop: max fraction of samples above fairway grade.
    pub steep_frac: f64,
    /// Hole elevation-change caps (m): max tee→green climb / drop. Atlas
    /// extremes are +26/−37 (uphill >20 m on only 1.5% of real holes).
    pub up_max: f64,
    pub down_max: f64,
}

/// Six attempts: tighten → loosen, rotating to alternate clubhouse sites in
/// the middle stages (some sites are simply unroutable; a different anchor is
/// often better than looser constraints).
pub const LADDER: [Relax; 6] = [
    Relax { stage: 0, len_tol: 0.05, walk_max: 160.0, loop_w: 1.0, anchor9: 130.0, steep_frac: 0.45, up_max: 22.0, down_max: 38.0 },
    Relax { stage: 1, len_tol: 0.08, walk_max: 200.0, loop_w: 0.7, anchor9: 170.0, steep_frac: 0.45, up_max: 25.0, down_max: 40.0 },
    Relax { stage: 2, len_tol: 0.08, walk_max: 200.0, loop_w: 0.7, anchor9: 170.0, steep_frac: 0.50, up_max: 25.0, down_max: 40.0 },
    Relax { stage: 3, len_tol: 0.12, walk_max: 250.0, loop_w: 0.4, anchor9: 220.0, steep_frac: 0.58, up_max: 29.0, down_max: 44.0 },
    Relax { stage: 4, len_tol: 0.16, walk_max: 300.0, loop_w: 0.25, anchor9: 280.0, steep_frac: 0.66, up_max: 33.0, down_max: 48.0 },
    Relax { stage: 5, len_tol: 0.20, walk_max: 340.0, loop_w: 0.15, anchor9: 320.0, steep_frac: 0.75, up_max: 37.0, down_max: 54.0 },
];

/// A placed hole (before walks are computed).
#[derive(Clone, Debug)]
pub struct Placed {
    pub par: u8,
    /// tee, dogleg verts, green.
    pub pts: Vec<Vec2>,
    pub len: f64,
    pub target: f64,
    /// Candidate-list indices of the chosen green/tee sites (scoring + anneal
    /// re-picks). Set by the caller after `realize`.
    pub gsite: usize,
    pub tsite: usize,
}

/// The loop schedule: soft target positions for each green. Two shapes:
/// a closed loop around a center (parkland), or a linear out-and-back along
/// an axis (the mountain-valley pattern) — both start and end at the
/// clubhouse.
#[derive(Clone, Copy, Debug)]
pub struct LoopPlan {
    pub center: Vec2,
    pub theta0: f64,
    pub dir: f64,
    pub r_club: f64,
    pub r_mid: f64,
    /// Out-and-back mode: `center` is the clubhouse, `theta0` the axis
    /// bearing, `r_mid` the half-length, `r_club` the lateral separation of
    /// the two legs.
    pub linear: bool,
}

impl LoopPlan {
    /// Target position at loop fraction `f` ∈ [0, 1].
    pub fn target(&self, f: f64) -> Vec2 {
        if self.linear {
            let axis = Vec2::new(math::cos(self.theta0), math::sin(self.theta0));
            let along = 1.0 - (1.0 - 2.0 * f).abs(); // 0 → 1 → 0
            let side = if f < 0.5 { 0.5 } else { -0.5 };
            return self.center + axis * (self.r_mid * along)
                + axis.perp() * (side * self.dir * self.r_club);
        }
        let r = self.r_club + (self.r_mid - self.r_club) * math::sin(std::f64::consts::PI * f);
        let th = self.theta0 + self.dir * std::f64::consts::TAU * f;
        self.center + Vec2::new(math::cos(th), math::sin(th)) * r
    }
}

/// Out-and-back plan: axis drawn, legs separated laterally, half-length fit
/// to the course length and clamped into the box.
pub fn plan_linear(rng: &mut DetRng, clubhouse: Vec2, total_len: f64) -> LoopPlan {
    let theta = rng.range_f64(0.0, std::f64::consts::TAU);
    let axis = Vec2::new(math::cos(theta), math::sin(theta));
    // Distance to the box edge along the axis.
    let mut half = 0.42 * total_len;
    for t in 1..=40 {
        let p = clubhouse + axis * (half * t as f64 / 40.0);
        if !in_box(p, 120.0) {
            half = half * (t - 1) as f64 / 40.0;
            break;
        }
    }
    LoopPlan {
        center: clubhouse,
        theta0: theta,
        dir: if rng.next_f64() < 0.5 { 1.0 } else { -1.0 },
        r_club: rng.range_f64(190.0, 260.0),
        r_mid: half.max(500.0),
        linear: true,
    }
}

/// Standard normal via Irwin–Hall (12 uniforms) — matches the sampler's
/// convention and stays platform-exact.
pub fn normal(rng: &mut DetRng) -> f64 {
    let mut s = 0.0;
    for _ in 0..12 {
        s += rng.next_f64();
    }
    s - 6.0
}

/// First call (`tried` empty): score²-weighted draw among the best sites.
/// Retry calls: the candidate farthest from every already-tried clubhouse —
/// when a neighborhood proves unroutable, move, don't shave the score list.
pub fn pick_clubhouse(rng: &mut DetRng, fields: &Fields, tried: &[Vec2]) -> Option<Vec2> {
    if fields.clubs.is_empty() {
        return None;
    }
    if tried.is_empty() {
        let top: Vec<_> = fields.clubs.iter().take(6).collect();
        let weights: Vec<f64> = top.iter().map(|s| (s.score.max(0.1)).powi(2)).collect();
        let total: f64 = weights.iter().sum();
        let mut u = rng.next_f64() * total;
        let mut pick = 0;
        for (i, w) in weights.iter().enumerate() {
            if u < *w {
                pick = i;
                break;
            }
            u -= w;
        }
        return Some(top[pick].pos);
    }
    fields
        .clubs
        .iter()
        .map(|s| {
            let dmin = tried
                .iter()
                .map(|t| t.distance(s.pos))
                .fold(f64::INFINITY, f64::min);
            (s, dmin)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(s, _)| s.pos)
}

/// Draw a loop schedule. `compact` (steep terrain) pulls the loop tighter —
/// mountain nines are shorter-radius, more linear affairs than parkland loops.
pub fn plan_loop(rng: &mut DetRng, clubhouse: Vec2, total_len: f64, compact: bool) -> LoopPlan {
    let box_center = Vec2::new(1000.0, 1000.0);
    let to_center = box_center - clubhouse;
    let base_ang = if to_center.length() < 1.0 {
        rng.range_f64(0.0, std::f64::consts::TAU)
    } else {
        math::atan2(to_center.y, to_center.x)
    };
    let theta_c = base_ang + rng.range_f64(-0.9, 0.9);
    let r_c = if compact {
        rng.range_f64(260.0, 390.0)
    } else {
        rng.range_f64(340.0, 470.0)
    };
    let mut center = clubhouse + Vec2::new(math::cos(theta_c), math::sin(theta_c)) * r_c;
    center.x = center.x.clamp(crate::BOX_MIN + 220.0, crate::BOX_MAX - 220.0);
    center.y = center.y.clamp(crate::BOX_MIN + 220.0, crate::BOX_MAX - 220.0);
    let r_club = center.distance(clubhouse).max(150.0);
    // Aim the loop circumference at ~0.9× the walked course length.
    let avg_r = 0.143 * total_len;
    let r_mid = (r_club + (avg_r - r_club) * std::f64::consts::FRAC_PI_2)
        .clamp(r_club * 0.8, 620.0);
    let dir = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
    let d = clubhouse - center;
    LoopPlan {
        center,
        theta0: math::atan2(d.y, d.x),
        dir,
        r_club,
        r_mid,
        linear: false,
    }
}

/// Cumulative loop fraction of each green (walk gaps included).
pub fn green_fractions(targets: &[f64; 9]) -> [f64; 9] {
    const GAP: f64 = 95.0;
    let total: f64 = targets.iter().sum::<f64>() + 8.0 * GAP;
    let mut f = [0.0f64; 9];
    let mut acc = 0.0;
    for i in 0..9 {
        acc += targets[i];
        f[i] = acc / total;
        acc += GAP;
    }
    f
}

pub(crate) struct Ctx<'a> {
    pub(crate) ct: &'a CourseTerrain,
    pub(crate) fields: &'a Fields,
    pub(crate) relax: Relax,
    pub(crate) clubhouse: Vec2,
    pub(crate) lp: LoopPlan,
    pub(crate) pars: [u8; 9],
    pub(crate) targets: [f64; 9],
    pub(crate) fr: [f64; 9],
    budget: i32,
}

impl<'a> Ctx<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        ct: &'a CourseTerrain,
        fields: &'a Fields,
        relax: Relax,
        clubhouse: Vec2,
        lp: LoopPlan,
        pars: [u8; 9],
        targets: [f64; 9],
    ) -> Self {
        Ctx {
            ct,
            fields,
            relax,
            clubhouse,
            lp,
            pars,
            targets,
            fr: green_fractions(&targets),
            budget: 0,
        }
    }

    pub(crate) fn hard_ok(&self, pts: &[Vec2], placed: &[&Placed]) -> bool {
        // Geometry containment.
        if !pts.iter().all(|&p| in_box(p, GEO_MARGIN)) {
            return false;
        }
        // Hole elevation-change caps (severe climbs are the playability sin;
        // big drops read as dramatic but stay bounded too).
        let dz = geometry::hole_dz(self.ct, pts);
        if dz > self.relax.up_max || -dz > self.relax.down_max {
            return false;
        }
        let segs: Vec<(Vec2, Vec2)> = geometry::segments(pts).collect();
        // Own non-adjacent segments must not intersect (tight double doglegs).
        if segs.len() == 3 && seg_seg_intersect(segs[0].0, segs[0].1, segs[2].0, segs[2].1) {
            return false;
        }
        for &(a, b) in &segs {
            if point_seg_dist(self.clubhouse, a, b) < CLUB_CLEAR {
                return false;
            }
            if !corridor_water_ok(self.ct, a, b)
                || !corridor_grade_ok(self.ct, a, b, self.relax.steep_frac)
            {
                return false;
            }
            // 45 m buffer against every other hole's line of play.
            for p in placed {
                for (q1, q2) in geometry::segments(&p.pts) {
                    if seg_seg_dist(a, b, q1, q2) < BUFFER_M {
                        return false;
                    }
                }
            }
        }
        true
    }

    /// Composite ordering score for a green candidate of hole `i` from `tee`.
    pub(crate) fn green_rank(&self, i: usize, tee: Vec2, pos: Vec2, score: f64) -> f64 {
        let d = tee.distance(pos);
        let target = self.targets[i];
        let len_pen = if self.pars[i] == 3 {
            6.0 * (d - target).abs() / target
        } else {
            // Straight-line shortfall the doglegs must absorb; mild preference
            // for gentle doglegs (d close to but under target).
            2.0 * ((target * 0.985 - d).abs() / target)
        };
        // The return leg must keep aiming home even when relaxation loosens
        // the loop elsewhere — closure failures are the dominant dead end.
        let lw = if i >= 6 { self.relax.loop_w.max(0.8) } else { self.relax.loop_w };
        let loop_pen = lw * 0.004 * pos.distance(self.lp.target(self.fr[i]));
        // Steer away from steep climbs early (the hard cap only backstops).
        let dz = self.ct.macro_heights.bilinear(pos) - self.ct.macro_heights.bilinear(tee);
        let mut elev_pen = 0.6 * ((dz - 8.0).max(0.0) / 10.0) + 0.3 * ((-dz - 20.0).max(0.0) / 10.0);
        // Sustained steepness past the opening stretch costs (soft ~7% cap);
        // a blind opening shot costs too.
        elev_pen += 0.12 * geometry::steep_grade_pen(self.ct, &[tee, pos]);
        let land = tee + (pos - tee) * (137.0_f64.min(0.6 * d) / d.max(1e-9));
        if geometry::blind_tee(self.ct, tee, land) {
            elev_pen += 0.45;
        }
        // Par 3s love connecting adjacent high ground with a carry between.
        let mut bonus = 0.0;
        if self.pars[i] == 3 {
            let pt = self.fields.prominence[crate::fields::cell_of(&self.fields.spec, tee)];
            let pg = self.fields.prominence[crate::fields::cell_of(&self.fields.spec, pos)];
            let hi2hi = (pt / 5.0).clamp(0.0, 1.0) * (pg / 5.0).clamp(0.0, 1.0);
            let zm = self.ct.macro_heights.bilinear(tee.lerp(pos, 0.5));
            let z_ends = self
                .ct
                .macro_heights
                .bilinear(tee)
                .min(self.ct.macro_heights.bilinear(pos));
            let dip = ((z_ends - zm) / 4.0).clamp(0.0, 1.0);
            bonus += 0.6 * hi2hi + 0.35 * hi2hi * dip;
        }
        score + bonus - len_pen - loop_pen - elev_pen
    }

    /// Try to realize hole `i` from `tee` to `green` (exact-length doglegs).
    pub(crate) fn realize(&self, rng: &mut DetRng, i: usize, tee: Vec2, green: Vec2) -> Option<Placed> {
        let par = self.pars[i];
        let target = self.targets[i];
        let d = tee.distance(green);
        match par {
            3 => {
                if (d - target).abs() / target > self.relax.len_tol {
                    return None;
                }
                Some(Placed { par, pts: vec![tee, green], len: d, target, gsite: 0, tsite: 0 })
            }
            4 => {
                if d > target {
                    // Straight two-segment par 4 (vertex on the line).
                    if (d - target) / target > self.relax.len_tol {
                        return None;
                    }
                    let v = tee.lerp(green, 0.58);
                    return Some(Placed {
                        par,
                        pts: vec![tee, v, green],
                        len: d,
                        target,
                        gsite: 0,
                        tsite: 0,
                    });
                }
                for _ in 0..3 {
                    let afrac = (0.58 + 0.05 * normal(rng)).clamp(0.48, 0.66);
                    let side = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
                    for s in [side, -side] {
                        if let Some((v, _turn)) = solve_dogleg(tee, green, target, afrac, s) {
                            return Some(Placed {
                                par,
                                pts: vec![tee, v, green],
                                len: target,
                                target,
                                gsite: 0,
                                tsite: 0,
                            });
                        }
                    }
                }
                None
            }
            _ => {
                if d > target {
                    if (d - target) / target > self.relax.len_tol {
                        return None;
                    }
                    let v1 = tee.lerp(green, 0.37);
                    let v2 = tee.lerp(green, 0.72);
                    return Some(Placed {
                        par,
                        pts: vec![tee, v1, v2, green],
                        len: d,
                        target,
                        gsite: 0,
                        tsite: 0,
                    });
                }
                for _ in 0..4 {
                    let f1 = (0.37 + 0.04 * normal(rng)).clamp(0.30, 0.44);
                    let f2 = (0.72 + 0.04 * normal(rng)).clamp(f1 + 0.18, 0.82);
                    let delta1 = 0.21 * normal(rng);
                    let side2 = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
                    if let Some(([v1, v2], _t)) = solve_par5(tee, green, target, f1, f2, delta1, side2)
                    {
                        return Some(Placed {
                            par,
                            pts: vec![tee, v1, v2, green],
                            len: target,
                            target,
                            gsite: 0,
                            tsite: 0,
                        });
                    }
                }
                None
            }
        }
    }

    /// Candidate greens for hole `i` from `tee`, best first.
    pub(crate) fn green_pool(&self, i: usize, tee: Vec2) -> Vec<(usize, f64)> {
        let target = self.targets[i];
        let (dlo, dhi) = match self.pars[i] {
            3 => (target * (1.0 - self.relax.len_tol), target * (1.0 + self.relax.len_tol)),
            4 => (target * 0.88, target * (1.0 + self.relax.len_tol)),
            _ => (target * 0.82, target * (1.0 + self.relax.len_tol)),
        };
        let mut pool: Vec<(usize, f64)> = Vec::new();
        for (gi, s) in self.fields.greens.iter().enumerate() {
            let d = tee.distance(s.pos);
            if d < dlo || d > dhi {
                continue;
            }
            if i == 8 {
                let dc = s.pos.distance(self.clubhouse);
                if dc < 55.0 || dc > self.relax.anchor9 {
                    continue;
                }
            }
            if i == 7 {
                // Lookahead: hole 8's green must leave hole 9 (tee ring +
                // target + clubhouse anchor) geometrically satisfiable.
                let dc = s.pos.distance(self.clubhouse);
                let l9 = self.targets[8];
                let hi = l9 * (1.0 + self.relax.len_tol) + self.relax.anchor9 + self.relax.walk_max;
                let lo = (l9 * (1.0 - self.relax.len_tol) - self.relax.anchor9 - self.relax.walk_max)
                    .max(0.0);
                if dc < lo || dc > hi {
                    continue;
                }
            }
            pool.push((gi, self.green_rank(i, tee, s.pos, s.score)));
        }
        pool.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(self.fields.greens[a.0].cell.cmp(&self.fields.greens[b.0].cell))
        });
        pool.truncate(56);
        pool
    }

    /// Candidate tees near `green` for the next hole, best first.
    pub(crate) fn tee_pool(&self, next_i: usize, green: Vec2, placed: &[&Placed]) -> Vec<usize> {
        let mut pool: Vec<(usize, f64)> = Vec::new();
        'cand: for (ti, s) in self.fields.tees.iter().enumerate() {
            let d = green.distance(s.pos);
            if d < WALK_MIN || d > self.relax.walk_max {
                continue;
            }
            if s.pos.distance(self.clubhouse) < 55.0 {
                continue;
            }
            // A tee is an endpoint of the next line of play: keep it clear of
            // every placed line now to avoid guaranteed buffer failures later.
            for p in placed {
                for (a, b) in geometry::segments(&p.pts) {
                    if point_seg_dist(s.pos, a, b) < BUFFER_M {
                        continue 'cand;
                    }
                }
            }
            let loop_pen = self.relax.loop_w
                * 0.003
                * s.pos.distance(self.lp.target(self.fr[next_i.saturating_sub(1)]));
            // Directed elevated-tee preference: the hole will head roughly at
            // this hole's loop target — reward tees overlooking that line.
            let aim = self.lp.target(self.fr[next_i]);
            let dir = (aim - s.pos).normalized();
            let drop_est = self.ct.macro_heights.bilinear(s.pos)
                - self.ct.macro_heights.bilinear(s.pos + dir * 137.0);
            let drop_term = 0.15 * (drop_est / 7.0).clamp(-1.0, 1.0);
            pool.push((ti, s.score - 0.012 * d - loop_pen + drop_term));
        }
        pool.sort_by(|a, b| {
            b.1.partial_cmp(&a.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(self.fields.tees[a.0].cell.cmp(&self.fields.tees[b.0].cell))
        });
        pool.truncate(24);
        pool.into_iter().map(|(ti, _)| ti).collect()
    }

    /// Place holes i..9 recursively. `tee`/`tsite` are hole i's tee. Consumes
    /// budget on every alternative tried beyond the first at each decision
    /// point.
    fn place(
        &mut self,
        rng: &mut DetRng,
        i: usize,
        tee: Vec2,
        tsite: usize,
        placed: &mut Vec<Placed>,
    ) -> bool {
        let pool = self.green_pool(i, tee);
        for (rank, &(gi, _)) in pool.iter().enumerate() {
            if rank > 0 {
                self.budget -= 1;
                if self.budget < 0 {
                    return false;
                }
            }
            let green = self.fields.greens[gi].pos;
            let Some(mut hole) = self.realize(rng, i, tee, green) else {
                continue;
            };
            hole.gsite = gi;
            hole.tsite = tsite;
            {
                let refs: Vec<&Placed> = placed.iter().collect();
                if !self.hard_ok(&hole.pts, &refs) {
                    continue;
                }
            }
            placed.push(hole);
            if i == 8 {
                return true;
            }
            // Choose the next tee and recurse.
            let tees = {
                let refs: Vec<&Placed> = placed.iter().collect();
                self.tee_pool(i + 1, green, &refs)
            };
            let mut aborted = false;
            for (trank, &ti) in tees.iter().take(8).enumerate() {
                if trank > 0 {
                    self.budget -= 1;
                    if self.budget < 0 {
                        aborted = true;
                        break;
                    }
                }
                let tpos = self.fields.tees[ti].pos;
                if self.place(rng, i + 1, tpos, ti, placed) {
                    return true;
                }
            }
            placed.pop();
            if aborted {
                return false;
            }
        }
        false
    }
}

/// One full construction attempt at a relaxation stage: several loop plans,
/// several first tees each. Returns the placed holes and the loop plan, or
/// `None` (search budget exhausted / no feasible layout at this stage).
pub fn construct(
    ct: &CourseTerrain,
    fields: &Fields,
    rng: &mut DetRng,
    pars: [u8; 9],
    targets: [f64; 9],
    relax: Relax,
    clubhouse: Vec2,
) -> Option<(Vec<Placed>, LoopPlan)> {
    let total: f64 = targets.iter().sum();
    // Steep worlds get compact, more linear loops.
    let box_slope = {
        let spec = ct.macro_slope.spec;
        let mut s = 0.0;
        let mut n = 0usize;
        for y in 0..spec.ny {
            for x in 0..spec.nx {
                let p = spec.world_of(x, y);
                if in_box(p, 0.0) {
                    s += ct.macro_slope.data[spec.index(x, y)];
                    n += 1;
                }
            }
        }
        s / n.max(1) as f64
    };
    let compact = box_slope > 0.14;

    // Hole 1 tee: best tee candidates near the clubhouse.
    let mut t1: Vec<(usize, f64)> = fields
        .tees
        .iter()
        .enumerate()
        .filter_map(|(ti, s)| {
            let d = s.pos.distance(clubhouse);
            let dmax = (150.0 + 40.0 * relax.stage as f64).min(300.0);
            if (55.0..=dmax).contains(&d) {
                Some((ti, s.score - 0.008 * d))
            } else {
                None
            }
        })
        .collect();
    t1.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(fields.tees[a.0].cell.cmp(&fields.tees[b.0].cell))
    });

    // A loop plan that aims the course into a ridge or across a marsh kills
    // every descendant attempt the same way — global direction needs its own
    // retries, cheapest first: three circular loops, then two out-and-back
    // axes (the shape extreme mountain and water-maze seeds actually need).
    for attempt in 0..5 {
        let lp = if attempt < 3 {
            plan_loop(rng, clubhouse, total, compact)
        } else {
            plan_linear(rng, clubhouse, total)
        };
        let mut ctx = Ctx::new(ct, fields, relax, clubhouse, lp, pars, targets);
        ctx.budget = 160 + 120 * relax.stage as i32;
        for &(ti, _) in t1.iter().take(6) {
            let mut placed: Vec<Placed> = Vec::with_capacity(9);
            if ctx.place(rng, 0, fields.tees[ti].pos, ti, &mut placed) {
                return Some((placed, lp));
            }
            ctx.budget = ctx.budget.max(0) + 80; // fresh start from another first tee
        }
    }
    None
}
