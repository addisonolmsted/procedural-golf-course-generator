//! `golf-routing`: 9-hole, par-36 returning-nine routing over a generated
//! `CourseTerrain`.
//!
//! Consumes the terrain read-only. Pipeline: site fields + candidates → par
//! sequence + length targets → greedy loop construction (45 m buffers, water
//! rules, box containment enforced at placement) → walking paths (A*, curved
//! length) → simulated-annealing polish → strict verification. Deterministic
//! in `(terrain, seed)`: one DetRng stream (`b"routing-v1"`), fixed iteration
//! counts, no map-iteration or wall-clock dependence anywhere.

pub mod anneal;
pub mod construct;
pub mod fields;
pub mod geometry;
pub mod sequence;
pub mod walk;

use golf_core::det::DetRng;
use golf_core::math::Vec2;
use golf_terrain::CourseTerrain;

pub const ROUTING_VERSION: u32 = 3;

/// Playable box: everything (tees, greens, lines of play, walks) stays inside
/// the central 1.5 km × 1.5 km of the 2 km world, leaving a 500 m
/// visualization buffer.
pub const BOX_MIN: f64 = 250.0;
pub const BOX_MAX: f64 = 1750.0;

/// One routed hole.
#[derive(Clone, Debug)]
pub struct Hole {
    pub par: u8,
    /// Line of play: tee, dogleg vertices, green — par−2 interior points,
    /// i.e. par 3: 2 pts / par 4: 3 / par 5: 4.
    pub pts: Vec<Vec2>,
    /// Sum of segment lengths (the hole's yardage), meters.
    pub len: f64,
    /// The sampled length target this hole was built for.
    pub target_len: f64,
    /// Walking path from this green to the next tee (hole 9: back to the
    /// clubhouse). Curved around lines of play; never crosses one.
    pub walk_to_next: Vec<Vec2>,
    pub walk_len: f64,
}

impl Hole {
    pub fn tee(&self) -> Vec2 {
        self.pts[0]
    }
    pub fn green(&self) -> Vec2 {
        *self.pts.last().unwrap()
    }
}

/// A routed course: returning nine anchored at the clubhouse.
#[derive(Clone, Debug)]
pub struct Routing {
    pub clubhouse: Vec2,
    pub holes: Vec<Hole>,
    pub par_seq: [u8; 9],
    pub total_len: f64,
    pub total_walk: f64,
    /// Relaxation-ladder stage that produced the layout (0 = none needed).
    pub relax_stage: u8,
}

impl Routing {
    /// All line-of-play segments across the course.
    pub fn all_segments(&self) -> Vec<(Vec2, Vec2)> {
        self.holes
            .iter()
            .flat_map(|h| geometry::segments(&h.pts).collect::<Vec<_>>())
            .collect()
    }
}

/// Routing failure: the terrain couldn't host a valid nine within the
/// relaxation ladder (expected rare; the game re-rolls the seed).
#[derive(Clone, Debug)]
pub struct RouteFail {
    pub reason: &'static str,
}

impl std::fmt::Display for RouteFail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "routing failed: {}", self.reason)
    }
}
impl std::error::Error for RouteFail {}

/// Route a nine on `ct`. Deterministic in `(ct, seed)`.
pub fn route(ct: &CourseTerrain, seed: u64) -> Result<Routing, RouteFail> {
    let f = fields::build(ct);
    if f.clubs.is_empty() || f.greens.len() < 12 || f.tees.len() < 16 {
        return Err(RouteFail { reason: "too few candidate sites" });
    }
    let mut rng = DetRng::new(seed, b"routing-v1");
    let pars = sequence::draw_sequence(&mut rng);
    let targets = sequence::draw_lengths(&mut rng, &pars);
    let wgrid = walk::WalkGrid::new(ct);

    // The drawn clubhouse is kept through the first two stages; each later
    // stage moves to the candidate farthest from everything already tried
    // (an unroutable neighborhood is escaped, not re-relaxed in place).
    let drawn_club = construct::pick_clubhouse(&mut rng, &f, &[]);
    let mut tried_clubs: Vec<Vec2> = Vec::new();

    for relax in construct::LADDER.iter() {
        let clubhouse = match relax.stage {
            0 | 1 => drawn_club,
            _ => construct::pick_clubhouse(&mut rng, &f, &tried_clubs),
        };
        let Some(clubhouse) = clubhouse else { continue };
        if !tried_clubs.iter().any(|c| c.distance(clubhouse) < 1.0) {
            tried_clubs.push(clubhouse);
        }

        let Some((holes, lp)) =
            construct::construct(ct, &f, &mut rng, pars, targets, *relax, clubhouse)
        else {
            continue;
        };

        // Initial walks.
        let segs: Vec<(Vec2, Vec2)> = holes
            .iter()
            .flat_map(|h| geometry::segments(&h.pts).collect::<Vec<_>>())
            .collect();
        let mask = walk::corridor_mask(&wgrid.spec, &segs);
        let mut walks: Vec<(Vec<Vec2>, f64)> = Vec::with_capacity(9);
        let mut ok = true;
        for i in 0..9 {
            let from = *holes[i].pts.last().unwrap();
            let to = if i < 8 { holes[i + 1].pts[0] } else { clubhouse };
            match walk::find_walk(&wgrid, &mask, &segs, from, to) {
                Some(w) => walks.push(w),
                None => {
                    ok = false;
                    break;
                }
            }
        }
        if !ok {
            continue;
        }

        let ctx = construct::Ctx::new(ct, &f, *relax, clubhouse, lp, pars, targets);
        let mut sol = anneal::Solution { holes, walks };
        anneal::anneal(&ctx, &mut rng, &wgrid, &mut sol);

        let routing = assemble(clubhouse, pars, relax.stage, sol);
        if verify(ct, &routing).is_empty() {
            return Ok(routing);
        }
    }
    Err(RouteFail { reason: "relaxation ladder exhausted" })
}

fn assemble(clubhouse: Vec2, pars: [u8; 9], stage: u8, sol: anneal::Solution) -> Routing {
    let mut holes: Vec<Hole> = Vec::with_capacity(9);
    for (h, w) in sol.holes.into_iter().zip(sol.walks) {
        holes.push(Hole {
            par: h.par,
            pts: h.pts,
            len: h.len,
            target_len: h.target,
            walk_to_next: w.0,
            walk_len: w.1,
        });
    }
    let total_len = holes.iter().map(|h| h.len).sum();
    let total_walk = holes.iter().map(|h| h.walk_len).sum();
    Routing {
        clubhouse,
        holes,
        par_seq: pars,
        total_len,
        total_walk,
        relax_stage: stage,
    }
}

/// Strict validation of every routing invariant. Empty = valid. Used by the
/// final gate in `route`, the test suite, and `xtask routing-report`.
pub fn verify(ct: &CourseTerrain, r: &Routing) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    let mut fail = |s: String| v.push(s);

    if r.holes.len() != 9 {
        fail(format!("{} holes", r.holes.len()));
        return v;
    }
    let par_sum: u32 = r.par_seq.iter().map(|&p| p as u32).sum();
    if par_sum != 36 {
        fail(format!("par sum {par_sum}"));
    }

    let max_tol = construct::LADDER[construct::LADDER.len() - 1].len_tol + 1e-9;
    for (i, h) in r.holes.iter().enumerate() {
        if h.par != r.par_seq[i] {
            fail(format!("hole {} par mismatch", i + 1));
        }
        if h.pts.len() != h.par as usize - 1 {
            fail(format!("hole {} has {} pts for par {}", i + 1, h.pts.len(), h.par));
        }
        if (geometry::poly_len(&h.pts) - h.len).abs() > 1e-6 {
            fail(format!("hole {} len mismatch", i + 1));
        }
        if (h.len - h.target_len).abs() / h.target_len > max_tol {
            fail(format!(
                "hole {} len {:.0} vs target {:.0}",
                i + 1,
                h.len,
                h.target_len
            ));
        }
        for &p in &h.pts {
            if !geometry::in_box(p, 0.0) {
                fail(format!("hole {} point outside box", i + 1));
            }
        }
        for &p in &h.walk_to_next {
            if !geometry::in_box(p, -1e-9) {
                fail(format!("hole {} walk outside box", i + 1));
            }
        }
        for (a, b) in geometry::segments(&h.pts) {
            if !geometry::corridor_water_ok(ct, a, b) {
                fail(format!("hole {} crosses blocking water", i + 1));
            }
            if geometry::point_seg_dist(r.clubhouse, a, b) < 50.0 - 1e-9 {
                fail(format!("hole {} too close to clubhouse", i + 1));
            }
        }
        // Own-hole non-adjacent segments must not cross.
        let segs: Vec<(Vec2, Vec2)> = geometry::segments(&h.pts).collect();
        if segs.len() == 3
            && geometry::seg_seg_intersect(segs[0].0, segs[0].1, segs[2].0, segs[2].1)
        {
            fail(format!("hole {} self-intersects", i + 1));
        }
        if (geometry::poly_len(&h.walk_to_next) - h.walk_len).abs() > 1e-6 {
            fail(format!("hole {} walk len mismatch", i + 1));
        }
        if h.walk_len > walk::WALK_CURVED_MAX + 1e-9 {
            fail(format!("hole {} walk {:.0} m too long", i + 1, h.walk_len));
        }
    }

    // Inter-hole buffers (subsumes non-intersection).
    for i in 0..9 {
        for j in i + 1..9 {
            for (a, b) in geometry::segments(&r.holes[i].pts) {
                for (p, q) in geometry::segments(&r.holes[j].pts) {
                    let d = geometry::seg_seg_dist(a, b, p, q);
                    if d < geometry::BUFFER_M - 1e-9 {
                        fail(format!(
                            "holes {} and {} only {:.1} m apart",
                            i + 1,
                            j + 1,
                            d
                        ));
                    }
                }
            }
        }
    }

    // Walks: connectivity, no play-line crossings, no blocking water.
    let all: Vec<(Vec2, Vec2)> = r.all_segments();
    for i in 0..9 {
        let h = &r.holes[i];
        let w = &h.walk_to_next;
        if w.len() < 2 {
            fail(format!("hole {} walk empty", i + 1));
            continue;
        }
        if w[0].distance(h.green()) > 1e-6 {
            fail(format!("hole {} walk doesn't start at green", i + 1));
        }
        let expected_end = if i < 8 {
            r.holes[i + 1].tee()
        } else {
            r.clubhouse
        };
        if w.last().unwrap().distance(expected_end) > 1e-6 {
            fail(format!("hole {} walk doesn't reach its target", i + 1));
        }
        if walk::walk_crosses_play(w, &all) {
            fail(format!("hole {} walk crosses a line of play", i + 1));
        }
        for seg in w.windows(2) {
            let steps = (seg[0].distance(seg[1]) / 5.0).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let p = seg[0].lerp(seg[1], k as f64 / steps as f64);
                let cell = fields::cell_of(&ct.water.class.spec, p);
                if ct.water.is_blocking(cell) {
                    fail(format!("hole {} walk enters blocking water", i + 1));
                }
            }
        }
    }

    // Returning nine: hole 1 tee and hole 9 green near the clubhouse.
    let anchor_max = construct::LADDER[construct::LADDER.len() - 1].anchor9 + 1e-9;
    if r.holes[8].green().distance(r.clubhouse) > anchor_max {
        fail("hole 9 green not anchored at clubhouse".to_string());
    }
    if r.holes[0].tee().distance(r.clubhouse) > 360.0 {
        fail("hole 1 tee too far from clubhouse".to_string());
    }

    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use golf_terrain::{generate_course, macro_spec};

    fn routed(seed: u64) -> Option<(CourseTerrain, Routing)> {
        let (ct, _) = generate_course(&macro_spec(), seed);
        route(&ct, seed).ok().map(|r| (ct, r))
    }

    #[test]
    fn route_is_deterministic() {
        let (ct, _) = generate_course(&macro_spec(), 2024);
        let a = route(&ct, 2024);
        let b = route(&ct, 2024);
        match (a, b) {
            (Ok(ra), Ok(rb)) => {
                assert_eq!(ra.par_seq, rb.par_seq);
                assert_eq!(ra.total_len.to_bits(), rb.total_len.to_bits());
                for (x, y) in ra.holes.iter().zip(&rb.holes) {
                    assert_eq!(x.pts.len(), y.pts.len());
                    for (p, q) in x.pts.iter().zip(&y.pts) {
                        assert_eq!(p.x.to_bits(), q.x.to_bits());
                        assert_eq!(p.y.to_bits(), q.y.to_bits());
                    }
                    assert_eq!(x.walk_len.to_bits(), y.walk_len.to_bits());
                }
            }
            (Err(ea), Err(eb)) => assert_eq!(ea.reason, eb.reason),
            _ => panic!("determinism: one attempt routed, the other failed"),
        }
    }

    #[test]
    fn routed_seeds_pass_verification() {
        let mut routed_n = 0;
        for seed in 1..=12u64 {
            let (ct, _) = generate_course(&macro_spec(), seed);
            match route(&ct, seed) {
                Ok(r) => {
                    routed_n += 1;
                    let viol = verify(&ct, &r);
                    assert!(viol.is_empty(), "seed {seed}: {viol:?}");
                    assert_eq!(r.par_seq.iter().map(|&p| p as u32).sum::<u32>(), 36);
                }
                Err(_) => {}
            }
        }
        assert!(routed_n >= 8, "only {routed_n}/12 seeds routed");
    }

    #[test]
    fn walks_use_curved_length() {
        if let Some((_ct, r)) = routed(7) {
            for h in &r.holes {
                assert!(h.walk_len >= 0.999 * geometry::poly_len(&h.walk_to_next));
            }
        }
    }
}
