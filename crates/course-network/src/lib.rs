//! Step 4 — the network, built in phases: trunks, then tributary tiers.
//!
//! Attempt 4, pipeline step **4** (with step 5's long profiles folded in —
//! elevations exist DURING construction, not after). See
//! `docs/network-first/README.md` and the N-phase plan.
//!
//! **Owns:** the 3-D network graph. Trunks are authored end-to-end paths with
//! long-wave meander; tributaries descend a proto-elevation field from high
//! ground and converge onto the trunks.
//!
//! **Expected failures** (discipline rule 6): every SURFACE metric — there is
//! still no surface. Network planform metrics are checkable HERE.
//!
//! DEPENDENCY RULE: this crate may not depend on `course-skeleton`,
//! `course-primitives`, `course-amplify` or `course-transforms`. Enforced by
//! `tools/no_old_deps.sh`.

pub mod account;
pub mod proto;
pub mod tribs;
pub mod trunk_path;

pub use account::{Node, Reach, ACCOUNT_RES_M};
pub use tribs::Trib;
pub use trunk_path::{TrunkPath, TrunkPathParams};

use course_draw::rng::stream;
use course_seed::RunIdentity;
use course_template::Template;
use course_world::math::Vec2;

/// Phase N1's stream. Registered in `course_draw::rng`.
pub const NETWORK_TRUNKPATH: &str = "n4/network/trunkpath/v1";

/// The channel-extraction threshold — the corpus policy
/// (`extract_v2.CHANNEL_AREA_M2`), so `d2c`, density and Horton ratios mean
/// the same thing as the bands in `01-measurement-policy.md`.
pub const CHANNEL_AREA_M2: f64 = 6.0e4;

/// The network as of phase N2: trunks + tier-2 tributaries.
pub struct Network {
    pub trunks: Vec<TrunkPath>,
    pub tribs: Vec<Trib>,
    /// Terminations, by cause — reported, never hidden. Stubs are the only
    /// failures; edge and claimed are legitimate head placements.
    pub n_stub: u32,
    pub n_edge: u32,
    pub n_claimed: u32,
}

impl Network {
    /// Total trunk length, metres.
    pub fn trunk_len_m(&self) -> f64 {
        self.trunks
            .iter()
            .map(|t| t.pts.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>())
            .sum()
    }
}

/// Build the network a template implies. Phase N1: trunks only.
///
/// Every trunk is a THROUGH-river; a secondary may CONVERGE onto the primary
/// (a confluence inside the tile). Build order: primaries first, then joining
/// trunks, so the junction can be placed on an existing path.
pub fn build(id: &RunIdentity, t: &Template) -> Network {
    let mut rng = stream(id, NETWORK_TRUNKPATH);
    let relief_budget = t.relief_budget_m;
    let mut trunks: Vec<TrunkPath> = Vec::new();
    // template index -> built index, so joins survive drops
    let mut built_of: Vec<Option<usize>> = vec![None; t.trunks.len()];

    let order: Vec<usize> = (0..t.trunks.len())
        .filter(|i| t.trunks[*i].joins.is_none())
        .chain((0..t.trunks.len()).filter(|i| t.trunks[*i].joins.is_some()))
        .collect();

    for ti in order {
        let k = &t.trunks[ti];
        // All trunks are mature rivers crossing the tile: gentle drop.
        let head_frac = rng.range_f64(0.04, 0.12);

        // For a joining trunk: junction on the primary, acute approach.
        let join = k.joins.and_then(|pi| {
            let built = built_of[pi as usize]?;
            let primary = &trunks[built];
            let sp = course_world::spline::Spine::new(primary.pts.clone());
            // CHOOSE the junction where the geometry works: on a meandering
            // primary the local tangent mid-bend can point away from the
            // secondary's entry, and an acute approach built there faces the
            // wrong way and hairpins back (measured: 2 m curve radius). Scan
            // candidate positions and take the one whose upstream tangent
            // agrees best with the direction to the entry.
            let mut u = 0.45;
            let mut best = f64::MIN;
            for i in 0..16 {
                let cand = 0.28 + 0.44 * (i as f64 + rng.next_f64() * 0.5) / 16.0;
                let pt = sp.point_at(cand);
                let toward = Vec2::new(k.far.x - pt.x, k.far.y - pt.y).normalized();
                let score = sp.tangent_at(cand).dot(toward);
                if score > best {
                    best = score;
                    u = cand;
                }
            }
            let jp = sp.point_at(u);
            let up_t = sp.tangent_at(u);
            let down = Vec2::new(-up_t.x, -up_t.y);
            let to_entry = Vec2::new(k.far.x - jp.x, k.far.y - jp.y).normalized();
            let th = rng.range_f64(35.0, 50.0).to_radians();
            // Both rotation sides give an acute junction; take the one whose
            // approach actually faces the entry.
            let rot = |sgn: f64| {
                let (c, sn) = (course_world::math::cos(th * sgn), course_world::math::sin(th * sgn));
                let inc = Vec2::new(down.x * c - down.y * sn, down.x * sn + down.y * c);
                Vec2::new(-inc.x, -inc.y).normalized()
            };
            let (a, b) = (rot(1.0), rot(-1.0));
            let up_dir = if a.dot(to_entry) >= b.dot(to_entry) { a } else { b };
            // junction elevation from the primary's profile
            let arc = u * sp.length();
            let mut acc = 0.0;
            let mut jz = 0.0;
            for i in 1..primary.pts.len() {
                acc += primary.pts[i].distance(primary.pts[i - 1]);
                if acc >= arc {
                    jz = primary.z[i];
                    break;
                }
            }
            if std::env::var("N1_DEBUG").is_ok() {
                eprintln!("join: u={u:.2} jp=({:.0},{:.0}) up_dir=({:.2},{:.2}) agree={best:.2}",
                          jp.x, jp.y, up_dir.x, up_dir.y);
            }
            Some((jp, up_dir, jz))
        });
        let mouth = join.map(|(jp, _, _)| jp).unwrap_or(k.mouth);
        let join_arg = join.map(|(_, d, z)| (d, z));

        // Deterministic ladder: narrow the swing until the candidate neither
        // crosses nor crowds what is already placed. A trunk that cannot be
        // placed cleanly is DROPPED and counted, never overlapped.
        let mut placed = None;
        for attempt in 0..4 {
            let shrink = 0.72_f64.powi(attempt);
            let base = TrunkPathParams::default();
            let cand = trunk_path::build_trunk(
                &mut rng,
                mouth,
                k.far,
                k.external_km2,
                &t.fields.relief_pred,
                relief_budget,
                &TrunkPathParams {
                    swing_rad: (base.swing_rad.0 * shrink, base.swing_rad.1 * shrink),
                    lam_m: (base.lam_m.0 / shrink.max(0.5), base.lam_m.1 / shrink.max(0.5)),
                    head_relief_frac: head_frac,
                    ..base
                },
                join_arg,
            );
            let clean = if join.is_some() {
                // allowed to touch its primary AT the junction
                !crosses_any_except_near(&cand, &trunks, mouth, 150.0)
            } else {
                !crosses_any(&cand, &trunks) && min_corridor_ok(&cand, &trunks, 400.0)
            };
            if clean {
                placed = Some(cand);
                break;
            }
        }
        if let Some(mut tp) = placed {
            tp.joins = k.joins.and_then(|pi| built_of[pi as usize].map(|b| b as u32));
            built_of[ti] = Some(trunks.len());
            trunks.push(tp);
        }
    }

    // ---- phase N2: tier-2 tributaries descend onto the trunks.
    let g = grow_tribs(id, t, &trunks);
    Network {
        trunks,
        tribs: g.0,
        n_stub: g.1,
        n_edge: g.2,
        n_claimed: g.3,
    }
}



/// Grow the tier-2 tributaries: attach-and-climb (the reversal — user
/// design, N2 rework). Junction sites are AUTHORED along the channels;
/// each climber departs at the drawn angle and must gain proto-elevation
/// every step, so loops are structurally impossible.
///
/// Public, with `hold_override` exposed, so the rung ladder sweeps the
/// hold-distance dial through the SAME code path the pipeline uses.
pub fn grow_tribs_with(
    id: &RunIdentity,
    t: &Template,
    trunks: &[TrunkPath],
    hold_override: Option<(f64, f64)>,
) -> (Vec<Trib>, u32, u32, u32) {
    let relief_budget = t.relief_budget_m;
    let mut tribs: Vec<Trib> = Vec::new();
    // ends by cause: (kept is implicit) stubs, divide/claimed/edge/max are
    // normal terminations — only stubs are "failures", the rest are heads
    let (mut n_stub, mut n_edge, mut n_claimed) = (0u32, 0u32, 0u32);
    if !trunks.is_empty() {
        let mut chans = proto::ChannelSet::new();
        for tk in trunks {
            chans.add_polyline(&tk.pts, &tk.z);
        }
        let mut trng = stream(id, course_draw::rng::NETWORK_TRIBS);
        let mut tier = tribs::tier2();
        if let Some(h) = hold_override {
            tier.hold_m = h;
        }
        let k_rise = 0.55 * relief_budget / math_pow(800.0, 0.6);
        let sites = tribs::attach_points(&mut trng, &chans, &tier);
        for site in sites {
            let proto_f = proto::Proto {
                chans: &chans,
                relief: &t.fields.relief_pred,
                k_rise,
                w_macro: 0.05,
                budget_m: relief_budget,
            };
            // One retry on a stub: a site inside a meander bend often cannot
            // support the first drawn departure but takes a different one.
            let mut placed = false;
            for _attempt in 0..2 {
                match tribs::climb(&mut trng, site, &proto_f, &tier) {
                    Ok((tb, end)) => {
                        match end {
                            tribs::End::Edge => n_edge += 1,
                            tribs::End::Claimed => n_claimed += 1,
                            _ => {}
                        }
                        chans.add_polyline(&tb.pts, &tb.z);
                        tribs.push(tb);
                        placed = true;
                        break;
                    }
                    Err(_) => {}
                }
            }
            if !placed {
                n_stub += 1;
            }
        }
    }
    (tribs, n_stub, n_edge, n_claimed)
}

pub fn grow_tribs(
    id: &RunIdentity,
    t: &Template,
    trunks: &[TrunkPath],
) -> (Vec<Trib>, u32, u32, u32) {
    grow_tribs_with(id, t, trunks, None)
}

fn math_pow(x: f64, y: f64) -> f64 {
    course_world::math::pow(x, y)
}

/// Closest approach between a candidate and every placed trunk, sampled
/// coarsely. Two through-corridors closer than this read as one river drawn
/// twice — the review finding that set the 750 m mouth floor, applied to the
/// whole path.
fn min_corridor_ok(cand: &TrunkPath, placed: &[TrunkPath], min_m: f64) -> bool {
    for other in placed {
        for a in cand.pts.iter().step_by(3) {
            for b in other.pts.iter().step_by(3) {
                if a.distance(*b) < min_m {
                    return false;
                }
            }
        }
    }
    true
}

fn crosses_any_except_near(cand: &TrunkPath, placed: &[TrunkPath], junction: Vec2, r: f64) -> bool {
    for other in placed {
        for wa in cand.pts.windows(2) {
            if wa[0].distance(junction) < r {
                continue;
            }
            for wb in other.pts.windows(2) {
                if wb[0].distance(junction) < r {
                    continue;
                }
                if segs_cross(wa[0], wa[1], wb[0], wb[1]) {
                    return true;
                }
            }
        }
    }
    false
}

fn segs_cross(a1: course_world::math::Vec2, a2: course_world::math::Vec2,
              b1: course_world::math::Vec2, b2: course_world::math::Vec2) -> bool {
    let d = |p: course_world::math::Vec2, q: course_world::math::Vec2,
             r: course_world::math::Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let (d1, d2) = (d(b1, b2, a1), d(b1, b2, a2));
    let (d3, d4) = (d(a1, a2, b1), d(a1, a2, b2));
    ((d1 > 0.0) != (d2 > 0.0)) && ((d3 > 0.0) != (d4 > 0.0))
}

fn crosses_any(cand: &TrunkPath, placed: &[TrunkPath]) -> bool {
    for other in placed {
        for wa in cand.pts.windows(2) {
            for wb in other.pts.windows(2) {
                if segs_cross(wa[0], wa[1], wb[0], wb[1]) {
                    return true;
                }
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_draw::{generate, Archetype};
    use course_world::spline::Spine;

    fn net(seed: u64, a: Archetype) -> Network {
        let id = RunIdentity::from_seed(seed);
        let t = course_template::build(&id, &generate(&id, Some(a)));
        build(&id, &t)
    }

    #[test]
    fn deterministic() {
        let a = net(4, Archetype::Piedmont);
        let b = net(4, Archetype::Piedmont);
        assert_eq!(a.trunks.len(), b.trunks.len());
        for (x, y) in a.trunks.iter().zip(&b.trunks) {
            assert_eq!(x.pts.len(), y.pts.len());
            for (p, q) in x.pts.iter().zip(&y.pts) {
                assert_eq!(p.x, q.x);
                assert_eq!(p.y, q.y);
            }
        }
    }

    #[test]
    fn profile_is_monotone_and_zero_at_mouth() {
        for seed in 0..30u64 {
            for a in [Archetype::Piedmont, Archetype::RiverValley, Archetype::HillCountry] {
                for t in net(seed, a).trunks {
                    if t.joins.is_none() {
                        assert_eq!(t.z[0], 0.0, "{a} seed {seed}: mouth not at base level");
                    } else {
                        assert!(t.z[0] > 0.0, "{a} seed {seed}: junction at base level");
                    }
                    for w in t.z.windows(2) {
                        assert!(w[1] >= w[0], "{a} seed {seed}: profile not monotone");
                    }
                }
            }
        }
    }

    #[test]
    fn curves_meet_the_radius_floor() {
        // Smoothness is asserted, not eyeballed. The floor is the params
        // value; the assert allows a small margin for the border clamp.
        for seed in 0..30u64 {
            for a in [Archetype::Piedmont, Archetype::RiverValley] {
                for t in net(seed, a).trunks {
                    let r = Spine::new(t.pts.clone()).min_curvature_radius();
                    assert!(r >= 60.0, "{a} seed {seed}: min radius {r:.0} m");
                }
            }
        }
    }

    #[test]
    fn river_valley_trunk_is_through_and_end_to_end() {
        use course_world::world::EXTENT_M;
        for seed in 0..30u64 {
            let n = net(seed, Archetype::RiverValley);
            assert_eq!(n.trunks.len(), 1, "seed {seed}");
            let t = &n.trunks[0];
            let on_edge = |p: course_world::math::Vec2| {
                p.x <= 1.0 || p.y <= 1.0 || p.x >= EXTENT_M - 1.0 || p.y >= EXTENT_M - 1.0
            };
            assert!(on_edge(t.pts[0]), "seed {seed}: mouth off edge");
            assert!(on_edge(*t.pts.last().unwrap()), "seed {seed}: head off edge");
        }
    }

    #[test]
    fn trunks_do_not_cross_and_every_trunk_ends_at_an_edge_or_junction() {
        use course_world::world::EXTENT_M;
        for seed in 0..30u64 {
            for a in [Archetype::Piedmont, Archetype::GreatPlains, Archetype::HillCountry] {
                let n = net(seed, a);
                for (i, tk) in n.trunks.iter().enumerate() {
                    // every ENTRY is on an edge (all trunks are through)
                    let head = *tk.pts.last().unwrap();
                    let on_edge = head.x <= 1.0 || head.y <= 1.0
                        || head.x >= EXTENT_M - 1.0 || head.y >= EXTENT_M - 1.0;
                    assert!(on_edge, "{a} seed {seed}: trunk {i} entry off edge");
                    // and the downstream end is an edge mouth or a junction
                    if tk.joins.is_none() {
                        let m = tk.pts[0];
                        let m_edge = m.x <= 1.0 || m.y <= 1.0
                            || m.x >= EXTENT_M - 1.0 || m.y >= EXTENT_M - 1.0;
                        assert!(m_edge, "{a} seed {seed}: trunk {i} mouth off edge");
                    }
                    // no proper crossings against other trunks, away from a
                    // shared junction
                    for (j, other) in n.trunks.iter().enumerate() {
                        if j <= i { continue; }
                        let junction = if tk.joins == Some(j as u32) {
                            Some(tk.pts[0])
                        } else if other.joins == Some(i as u32) {
                            Some(other.pts[0])
                        } else {
                            None
                        };
                        for wa in tk.pts.windows(2) {
                            if let Some(jp) = junction {
                                if wa[0].distance(jp) < 160.0 { continue; }
                            }
                            for wb in other.pts.windows(2) {
                                if let Some(jp) = junction {
                                    if wb[0].distance(jp) < 160.0 { continue; }
                                }
                                assert!(
                                    !super::segs_cross(wa[0], wa[1], wb[0], wb[1]),
                                    "{a} seed {seed}: trunks {i} and {j} cross"
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn convergent_junctions_exist_and_sit_on_the_primary() {
        let mut joins = 0;
        for seed in 0..120u64 {
            for a in [Archetype::Piedmont, Archetype::GreatPlains, Archetype::HillCountry] {
                let n = net(seed, a);
                for tk in &n.trunks {
                    if let Some(pi) = tk.joins {
                        joins += 1;
                        let primary = &n.trunks[pi as usize];
                        let jp = tk.pts[0];
                        let d = primary.pts.iter().fold(f64::MAX, |m, q| m.min(q.distance(jp)));
                        assert!(d < 25.0, "{a} seed {seed}: junction {d:.0} m off the primary");
                        assert!(tk.z[0] > 0.0, "{a} seed {seed}: junction at base level");
                    }
                }
            }
        }
        assert!(joins > 10, "converge coin never fired ({joins})");
    }

    #[test]
    fn tribs_are_monotone_and_attach_to_earlier_channels() {
        for seed in 0..25u64 {
            for a in [Archetype::Piedmont, Archetype::RiverValley, Archetype::HillCountry] {
                let n = net(seed, a);
                let n_polys = n.trunks.len() as u32;
                for (ti, tb) in n.tribs.iter().enumerate() {
                    // junction-first, profile climbs from the junction z
                    for w in tb.z.windows(2) {
                        assert!(w[1] >= w[0], "{a} seed {seed}: trib profile not monotone");
                    }
                    // attaches only to a channel placed BEFORE it, so every
                    // chain terminates on a trunk — acyclic by construction
                    assert!(
                        tb.parent_poly < n_polys + ti as u32,
                        "{a} seed {seed}: trib attached to a later channel"
                    );
                }
            }
        }
    }

    #[test]
    fn tribs_cannot_loop() {
        // The structural guarantee, verified anyway: proto-elevation (and so
        // z) strictly increases along every path, and a strictly increasing
        // path cannot revisit any point. Belt: no two non-adjacent points
        // closer than half a step.
        for seed in 0..20u64 {
            for a in [Archetype::Piedmont, Archetype::RiverValley, Archetype::HillCountry] {
                let n = net(seed, a);
                for tb in &n.tribs {
                    for w in tb.z.windows(2) {
                        assert!(w[1] > w[0], "{a} seed {seed}: z not strictly increasing");
                    }
                    for i in 0..tb.pts.len() {
                        for j in (i + 3)..tb.pts.len() {
                            let d = tb.pts[i].distance(tb.pts[j]);
                            assert!(
                                d > 12.0,
                                "{a} seed {seed}: trib self-approaches ({d:.1} m, pts {i}/{j})"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn departure_angles_land_in_the_corpus_band() {
        use course_world::math::Vec2;
        let mut angs: Vec<f64> = Vec::new();
        for seed in 0..20u64 {
            let n = net(seed, Archetype::Piedmont);
            let mut chans = proto::ChannelSet::new();
            for tk in &n.trunks {
                chans.add_polyline(&tk.pts, &tk.z);
            }
            for tb in &n.tribs {
                if (tb.attach_pt as usize) >= chans.pts.len() {
                    continue; // attached to another trib
                }
                let k = 3.min(tb.pts.len() - 1);
                let inc = Vec2::new(tb.pts[0].x - tb.pts[k].x, tb.pts[0].y - tb.pts[k].y);
                if inc.length() < 1e-6 {
                    continue;
                }
                let down = chans.down[tb.attach_pt as usize];
                let c = inc.normalized().dot(down).clamp(-1.0, 1.0);
                angs.push(course_world::math::acos(c).to_degrees());
            }
        }
        angs.sort_by(|x, y| x.partial_cmp(y).unwrap());
        let p50 = angs[angs.len() / 2];
        assert!((30.0..=55.0).contains(&p50), "departure p50 {p50:.1}");
    }

    #[test]
    fn sandhills_has_no_trunks() {
        for seed in 0..10u64 {
            let n = net(seed, Archetype::Sandhills);
            assert!(n.trunks.is_empty());
            assert!(n.tribs.is_empty());
        }
    }
}
