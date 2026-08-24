//! C3 — the valley carve: the network becomes negative relief.
//!
//! A generalisation of the review-passed river-corridor machinery
//! (`water.rs` `carve_corridor`, closed 12/12): the same two-line idea —
//! a bed under the channel, a catena hung off it, min-composed into the
//! ground — but Hack-scaled from 1 m creek heads to trunk valleys, and run
//! over every channel of the network, junction-first.
//!
//! NOTHING here is constant (review 2026-08-25: "as organic as possible,
//! avoid constant widths slopes profiles"):
//!   * depth, floor width and wall run all ride the Hack proxy AND breathe
//!     along the arc on their own noise octaves;
//!   * the two sides of a valley are shaped independently (drawn asymmetry,
//!     slowly flipping sides — the river's cut-bank/slip-off idiom);
//!   * the wall profile exponent itself wanders along the arc;
//!   * the inside of every bend widens with local curvature;
//!   * a two-octave catena noise roughens the walls (zero at the water
//!     line, full at the shoulder) and a short-wave rim jag breaks the
//!     shoulder line.
//!
//! The catena is ONE zone — floor and concave sand slope, no benches, no
//! scarps (not sand country) — and past the wall it keeps rising at a
//! drawn outer grade until it MEETS the upland, so min-composition seals
//! the hand-off with no cliff by construction (the water.rs lesson: a hard
//! reach cutoff against high ground printed an 8 m staircase).

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::noise;

use crate::channel::Network;
use crate::draw::Descriptors;

/// Upstream-length credit for channels that continue off-tile: a trunk's
/// upstream edge is not a creek head, and a fragment keeps draining beyond
/// its border mouth. Drawn per tile. `guess`, bounded by the biome's creek
/// lengths (a few km).
const TRUNK_CREDIT_M: (f64, f64) = (1200.0, 2600.0);
const FRAG_CREDIT_M: (f64, f64) = (800.0, 1800.0);

/// The Hack proxy: incision scales with upstream stem length as
/// `(up_len / 1500)^0.7` — attempt 4's `trib_cut` scaling, re-derived
/// (course-network, branch network-first @ a412b31).
fn hack(up_len_m: f64) -> f64 {
    math::pow((up_len_m / 1500.0).max(0.0), 0.7)
}

/// Per-channel carve program: the bed and the arc table, plus the drawn
/// organ seeds. Beds are built junction-first so a tributary floor MEETS
/// its parent floor exactly.
struct Program {
    pts: Vec<Vec2>,
    bed: Vec<f64>,
    /// arc position of each point, from the mouth
    arc: Vec<f64>,
    /// Hack factor at each point (drives every size)
    hk: Vec<f64>,
    /// signed turn (heading change per metre, smoothed) for bend widening
    turn: Vec<f64>,
    asym: f64,
    s_wl: u32,
    s_wr: u32,
    s_flip: u32,
    s_depth_used: u32,
    s_p: u32,
    s_jag: u32,
}

pub fn carve(rng: &mut DetRng, height: &mut Grid<f64>, net: &Network,
             datum: &Grid<f64>, d: &Descriptors) {
    let spec = height.spec;
    // ---- per-tile draws -------------------------------------------------
    let depth_unit = d.valley_depth_m;
    let floor_unit = d.valley_floor_m;
    let wall_unit = d.valley_wall_m;
    let trunk_credit = rng.range_f64(TRUNK_CREDIT_M.0, TRUNK_CREDIT_M.1);
    let frag_credit = rng.range_f64(FRAG_CREDIT_M.0, FRAG_CREDIT_M.1);
    let out_grade = rng.range_f64(0.012, 0.020);
    let p_base = rng.range_f64(1.5, 2.1);
    let s_cat1 = rng.next_u32();
    let s_cat2 = rng.next_u32();
    let s_floor = rng.next_u32();

    // ---- junction-first beds -------------------------------------------
    let mut progs: Vec<Program> = Vec::with_capacity(net.chans.len());
    for (cid, c) in net.chans.iter().enumerate() {
        // resample to ~6 m so the stamp distance is to the LINE, not to
        // sparse points (a 20 m point spacing scallops a 3 m creek floor)
        let (pts, _) = resample(&c.pts, 6.0);
        let n = pts.len();
        let mut arc = vec![0.0f64; n];
        for i in 1..n {
            arc[i] = arc[i - 1] + pts[i - 1].distance(pts[i]);
        }
        let total = arc[n - 1];
        let credit = if c.parent.is_none() {
            if c.sys == 0 { trunk_credit } else { frag_credit }
        } else {
            0.0
        };
        let s_depth = rng.next_u32();
        let mut hk: Vec<f64> = (0..n)
            .map(|i| hack(total - arc[i] + credit))
            .collect();
        // a fragment's MOUTH also carries downstream credit (it keeps
        // flowing off-tile); fold it in as a floor on the mouth half
        if c.parent.is_none() && c.sys > 0 {
            for i in 0..n {
                let fade = (1.0 - arc[i] / (total * 0.5)).max(0.0);
                hk[i] = hk[i].max(hack(frag_credit) * fade);
            }
        }

        // bed target: the LOWPASS of the datum along the path, minus the
        // breathing Hack depth
        let mut ground: Vec<f64> = pts.iter().map(|q| datum.bilinear(*q)).collect();
        for _ in 0..3 {
            for i in 1..n - 1 {
                ground[i] = (ground[i - 1] + ground[i] * 2.0 + ground[i + 1]) / 4.0;
            }
        }
        let mut bed: Vec<f64> = (0..n)
            .map(|i| {
                let lam = 430.0 + 320.0 * hk[i];
                let breathe = 1.0 + 0.22 * noise::perlin1(arc[i] / lam, s_depth);
                let depth = (depth_unit * hk[i] * breathe).max(0.9);
                ground[i] - depth
            })
            .collect();
        // junction continuity: the mouth floor IS the parent floor there,
        // and the difference decays over the first ~150 m
        if let Some(pid) = c.parent {
            let par = &progs[pid as usize];
            let mut best = (f64::MAX, 0usize);
            for (j, q) in par.pts.iter().enumerate() {
                let dd = q.distance(pts[0]);
                if dd < best.0 {
                    best = (dd, j);
                }
            }
            let offset = par.bed[best.1] - bed[0];
            for i in 0..n {
                bed[i] += offset * math::exp(-arc[i] / 150.0);
            }
        }
        // monotone upstream — water cannot flow uphill along its own floor
        for i in 1..n {
            let ds = arc[i] - arc[i - 1];
            bed[i] = bed[i].max(bed[i - 1] + 0.0015 * ds);
        }

        // signed turn for the inside-of-bend widening (river idiom)
        let mut turn = vec![0.0f64; n];
        let w = 8usize; // ~48 m window
        for i in 0..n {
            let a = i.saturating_sub(w);
            let b = (i + w).min(n - 1);
            if b > a + 1 {
                let h0 = math::atan2(pts[a + 1].y - pts[a].y, pts[a + 1].x - pts[a].x);
                let h1 = math::atan2(pts[b].y - pts[b - 1].y, pts[b].x - pts[b - 1].x);
                let mut dh = h1 - h0;
                while dh > std::f64::consts::PI { dh -= std::f64::consts::TAU; }
                while dh < -std::f64::consts::PI { dh += std::f64::consts::TAU; }
                turn[i] = dh / (arc[b] - arc[a]).max(1.0);
            }
        }

        progs.push(Program {
            pts, bed, arc, hk, turn,
            asym: rng.range_f64(0.35, 0.65),
            s_wl: rng.next_u32(),
            s_wr: rng.next_u32(),
            s_flip: rng.next_u32(),
            s_depth_used: s_depth,
            s_p: rng.next_u32(),
            s_jag: rng.next_u32(),
        });
        let _ = cid;
    }

    // ---- the stamp ------------------------------------------------------
    let cell = 8.0;
    for pr in &progs {
        let n = pr.pts.len();
        for i in 0..n {
            let q = pr.pts[i];
            let a = pr.arc[i];
            let hk_i = pr.hk[i];
            // local tangent for the side sign
            let t0 = pr.pts[i.saturating_sub(1)];
            let t1 = pr.pts[(i + 1).min(n - 1)];
            let tang = Vec2::new(t1.x - t0.x, t1.y - t0.y).normalized();

            // ---- the local program: nothing constant --------------------
            // floor half-width, per side below; wall run; rise; exponent
            let f_base = (floor_unit * hk_i).max(2.5);
            let wall_base = (wall_unit * hk_i).max(5.0);
            let depth_loc = {
                let lam = 430.0 + 320.0 * hk_i;
                let breathe = 1.0 + 0.22 * noise::perlin1(a / lam, pr.s_depth_used);
                (depth_unit * hk_i * breathe).max(0.9)
            };
            let rise = depth_loc * (1.02 + 0.10 * noise::perlin1(a / 300.0, pr.s_p));
            let p_exp = (p_base + 0.25 * noise::perlin1(a / 520.0, pr.s_p)).clamp(1.35, 2.35);
            // which side is the cut bank flips slowly along the arc
            let flip = if noise::perlin1(a / 700.0, pr.s_flip) > 0.0 { 1.0 } else { -1.0 };

            // Bounded: the runout beyond the wall gets a fixed window and
            // then YIELDS to the upland — an unbounded rise/out_grade reach
            // let the stamp carve unrelated swales 600+ m away (seed 205).
            let reach = f_base * 1.9 + wall_base * 2.2 + 380.0;
            let r_px = (reach / cell).ceil() as i64 + 1;
            let (cx, cy) = ((q.x / cell).round() as i64, (q.y / cell).round() as i64);
            for gy in (cy - r_px).max(0)..=(cy + r_px).min(spec.ny as i64 - 1) {
                for gx in (cx - r_px).max(0)..=(cx + r_px).min(spec.nx as i64 - 1) {
                    let p = spec.world_of(gx as u32, gy as u32);
                    let dx = p.x - q.x;
                    let dy = p.y - q.y;
                    let dist = (dx * dx + dy * dy).sqrt();
                    if dist > reach {
                        continue;
                    }
                    let side = if tang.x * dy - tang.y * dx >= 0.0 { 1.0 } else { -1.0 };
                    // per-side breathing (river idiom: independent seeds per
                    // side) + drawn asymmetry on the flipping cut bank
                    let s_side = if side > 0.0 { pr.s_wl } else { pr.s_wr };
                    // Breathing wavelength SCALES with the valley (Hack):
                    // a 210 m octave on a 300 m-wide trunk printed the
                    // valley as a caterpillar of discs (seed 109 render) —
                    // big streams vary their width over longer distances,
                    // and amplitude eases as wavelength grows.
                    let lam1 = 210.0 + 240.0 * hk_i;
                    let lam2 = 96.0 + 110.0 * hk_i;
                    let amp1 = 0.55 / (1.0 + 0.55 * hk_i);
                    let breathe = 1.0 + amp1 * noise::perlin1(a / lam1, s_side)
                        + 0.15 * noise::perlin1(a / lam2, s_side.wrapping_add(9));
                    let asf = if side * flip > 0.0 {
                        2.0 * (1.0 - pr.asym)
                    } else {
                        2.0 * pr.asym
                    };
                    // inside of the bend widens with curvature
                    let inside = if side * pr.turn[i] > 0.0 {
                        1.0 + (2.6 * pr.turn[i].abs() * 100.0).min(0.30)
                    } else {
                        1.0
                    };
                    let f_loc = (f_base * breathe * inside).max(2.0);
                    let wall_loc = (wall_base * breathe.sqrt() * asf).max(4.0);

                    // rim jag: the shoulder line wobbles at short wavelength
                    // — amplitude well under the floor width, or the rim
                    // beads into a string of pearls (render, seed 109)
                    let jag = (2.0 + 3.5 * hk_i)
                        * noise::perlin2(p.x / 64.0, p.y / 64.0, pr.s_jag);
                    let de = (dist + jag - f_loc).max(0.0);
                    let u = de / wall_loc;
                    let z = if u <= 0.0 || dist <= f_loc {
                        // the floor: near-flat, faint cross-grain
                        pr.bed[i]
                            + 0.12 * hk_i * noise::perlin2(p.x / 30.0, p.y / 30.0, s_floor)
                    } else if u < 1.0 {
                        pr.bed[i] + rise * math::pow(u, p_exp)
                    } else {
                        pr.bed[i] + rise + out_grade * (de - wall_loc)
                    };
                    // catena noise: zero at the water line, full at the
                    // shoulder — organic walls, clean floor
                    let fade = u.clamp(0.0, 1.0);
                    let noise_amp = 0.30 * (0.3 + 0.7 * hk_i.min(1.0)) * fade;
                    let mut cand = z
                        + noise_amp
                            * (noise::perlin2(p.x / 55.0, p.y / 55.0, s_cat1)
                                + 0.5 * noise::perlin2(p.x / 23.0, p.y / 23.0, s_cat2));
                    let idx = spec.index(gx as u32, gy as u32);
                    // locality: past the runout window the catena hands the
                    // ground back to the upland, smoothly
                    let over = (de - wall_loc - 200.0) / 160.0;
                    if over > 0.0 {
                        let t = math::smoothstep(0.0, 1.0, over.min(1.0));
                        cand = cand * (1.0 - t) + datum.data[idx] * t;
                    }
                    if cand < height.data[idx] {
                        height.data[idx] = cand;
                    }
                }
            }
        }
    }
}

fn resample(pts: &[Vec2], step: f64) -> (Vec<Vec2>, Vec<usize>) {
    if pts.len() < 2 {
        return (pts.to_vec(), vec![0; pts.len()]);
    }
    let mut out = vec![pts[0]];
    let mut src = vec![0usize];
    let mut carry = 0.0;
    for i in 1..pts.len() {
        let seg = pts[i - 1].distance(pts[i]);
        if seg < 1e-9 {
            continue;
        }
        let dirx = (pts[i].x - pts[i - 1].x) / seg;
        let diry = (pts[i].y - pts[i - 1].y) / seg;
        let mut t = step - carry;
        while t <= seg {
            out.push(Vec2::new(pts[i - 1].x + dirx * t, pts[i - 1].y + diry * t));
            src.push(i);
            t += step;
        }
        carry = seg - (t - step);
    }
    if out.last().unwrap().distance(*pts.last().unwrap()) > step * 0.4 {
        out.push(*pts.last().unwrap());
        src.push(pts.len() - 1);
    }
    (out, src)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use course_seed::RunIdentity;

    fn carved(seed: u64) -> (Grid<f64>, Grid<f64>, Network) {
        let id = RunIdentity::from_seed(seed);
        let d = crate::draw::site(&id, Some(Mode::Fluvial), None);
        let mut dr = crate::rng::stream(&id, crate::rng::DATUM);
        let datum = crate::channel::datum(&mut dr, &d);
        let mut cr = crate::rng::stream(&id, crate::rng::CHANNEL);
        let net = crate::channel::grow(&mut cr, &datum, &d);
        let mut h = datum.clone();
        carve(&mut cr, &mut h, &net, &datum, &d);
        (datum, h, net)
    }

    #[test]
    fn the_carve_only_cuts() {
        // valleys are EROSIONAL: min-composition may lower ground, never
        // raise it — the mode's deposits() == false, checked on the field
        for seed in [201u64, 202, 203] {
            let (datum, h, _) = carved(seed);
            for i in 0..h.data.len() {
                assert!(h.data[i] <= datum.data[i] + 1e-9, "seed {seed}: raised ground");
            }
        }
    }

    #[test]
    fn the_carve_reaches_every_trunk_and_stays_local() {
        for seed in [204u64, 205] {
            let (datum, h, net) = carved(seed);
            let spec = h.spec;
            // somewhere along the trunk the cut is substantial…
            let trunk = &net.chans[0];
            let mut max_cut = 0.0f64;
            for p in &trunk.pts {
                let x = (p.x / 8.0).round().clamp(0.0, (spec.nx - 1) as f64) as u32;
                let y = (p.y / 8.0).round().clamp(0.0, (spec.ny - 1) as f64) as u32;
                max_cut = max_cut.max(datum.get(x, y) - h.get(x, y));
            }
            assert!(max_cut > 5.0, "seed {seed}: trunk cut only {max_cut:.2} m");
            // …and ground far from every channel is untouched
            let far = crate::channel::d2c_p50(&net); // sanity anchor only
            let _ = far;
            let mut idx_far = None;
            'search: for y in (0..spec.ny).step_by(7) {
                for x in (0..spec.nx).step_by(7) {
                    let p = spec.world_of(x, y);
                    let near = net.chans.iter().flat_map(|c| c.pts.iter())
                        .any(|q| q.distance(p) < 650.0);
                    if !near {
                        idx_far = Some((x, y));
                        break 'search;
                    }
                }
            }
            if let Some((x, y)) = idx_far {
                assert!((datum.get(x, y) - h.get(x, y)).abs() < 1e-9,
                        "seed {seed}: cut far from the network");
            }
        }
    }

    #[test]
    fn valley_widths_are_not_constant() {
        // the review rule, asserted: measure the half-width of the cut at
        // many stations along the trunk; the spread must be real
        let (datum, h, net) = carved(206);
        let spec = h.spec;
        let trunk = &net.chans[0];
        let mut widths = Vec::new();
        let n = trunk.pts.len();
        for i in (6..n - 6).step_by(8) {
            let q = trunk.pts[i];
            let t0 = trunk.pts[i - 1];
            let t1 = trunk.pts[i + 1];
            let tang = Vec2::new(t1.x - t0.x, t1.y - t0.y).normalized();
            let nrm = Vec2::new(-tang.y, tang.x);
            // walk outward until the cut fades below 0.3 m
            let mut wsum = 0.0;
            for sgn in [1.0, -1.0] {
                let mut wm = 0.0;
                for k in 1..80 {
                    let dd = k as f64 * 8.0;
                    let p = Vec2::new(q.x + nrm.x * dd * sgn, q.y + nrm.y * dd * sgn);
                    if p.x < 0.0 || p.y < 0.0 || p.x > 2999.0 || p.y > 2999.0 {
                        break;
                    }
                    let x = (p.x / 8.0).round() as u32;
                    let y = (p.y / 8.0).round() as u32;
                    // the VALLEY proper (>1.5 m of cut), not the faint
                    // runout apron — the apron walk saturated the 640 m
                    // cap once depths were recalibrated, and a capped
                    // measure reads as constant width
                    if datum.get(x.min(spec.nx - 1), y.min(spec.ny - 1))
                        - h.get(x.min(spec.nx - 1), y.min(spec.ny - 1)) < 1.5 {
                        break;
                    }
                    wm = dd;
                }
                wsum += wm;
            }
            if wsum > 0.0 {
                widths.push(wsum);
            }
        }
        assert!(widths.len() > 10, "too few stations measured");
        let mean = widths.iter().sum::<f64>() / widths.len() as f64;
        let var = widths.iter().map(|w| (w - mean).powi(2)).sum::<f64>() / widths.len() as f64;
        let cv = var.sqrt() / mean;
        assert!(cv > 0.14, "valley width nearly constant: cv {cv:.3} at mean {mean:.0} m");
    }
}
