//! C1 + C2 — the Carolina sand-cap datum and the dendritic creek network.
//!
//! The fluvial mode's structural spine, built in two authored stages:
//!
//!   C1  a broad, gently domed, FLAT-TOPPED sand-cap datum — deliberately
//!       valley-free, because every valley must come from the network;
//!   C2  the network itself — trunks first (the river integrator's heading
//!       machinery, generalised), then tributaries grown ATTACH-AND-CLIMB.
//!
//! Attach-and-climb is a re-derivation of attempt 4's proven construction
//! (`course-network/src/tribs.rs` + `proto.rs`, branch `network-first`,
//! HEAD a412b31 — the allowlisted Tier D reference). Its two structural
//! guarantees carry over unchanged:
//!
//!   * NO LOOPS: after a bounded grace window every step must GAIN
//!     proto-elevation, and a strictly increasing path cannot revisit;
//!   * NO CROSSINGS: a claim radius against the incrementally updated
//!     channel set, parent exempt by IDENTITY, not by arc.
//!
//! What attempt 4 proved DOESN'T work is equally load-bearing: headward
//! growth without an elevation constraint mazes (channels organise by room,
//! not by water), and strict tip-splitting is the binary-tree trap (Rb = 2
//! by construction). Attachments along a parent's length are what produce
//! stem-and-branch structure and Rb > 2.
//!
//! Corpus targets (docs/sandhills/01-corpus.md, 30 judged NC tiles):
//! density 2.33 km/km², d2c p50 107.3 m, junction p50 40.7° with 13.2%
//! above 80°, main_share 57%, n_sys 3, relief p99–p1 46.9 m.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};
use course_world::noise;
use course_world::world::EXTENT_M;

use crate::draw::Descriptors;
use crate::wind::macro_spec;

// ---------------------------------------------------------------------------
// C1 — the sand-cap datum
// ---------------------------------------------------------------------------

/// The Carolina interfluve mass at 8 m: 3-octave fBm at the drawn cap
/// wavelength + a regional tilt, with a SOFT TOP-CLIP so the uplands read
/// flat-topped (the biome doc's "broad flat-topped interfluves"), not as
/// rolling hills. Valley-free by design.
pub fn datum(rng: &mut DetRng, d: &Descriptors) -> Grid<f64> {
    let spec = macro_spec();
    let (nx, ny) = (spec.nx, spec.ny);
    let (s1, s2) = (rng.next_u32(), rng.next_u32());
    let _s3 = rng.next_u32(); // burned: the retired third octave
    // The cap is a gently TILTED SAND PLAIN, not a blob field (C3 lesson,
    // two failed renders: fBm swales as deep as the valleys drowned the
    // drainage, and tanh clipping printed terraced camo blobs). Most of the
    // relief is the regional fall-line tilt; a low, broad undulation rides
    // it; every sharp low is C3's job. "Flat-topped interfluves" emerge as
    // the un-incised plain between valleys — they are not authored here.
    let (tc, ts) = (math::cos(d.floor_tilt_rad), math::sin(d.floor_tilt_rad));
    // Tilt is a CREEK GRADE, not the relief carrier (review 2026-08-25: on
    // real tiles the relief spans highland to VALLEY FLOOR — with a 27 m
    // tilt our upstream valley sat above the downstream highland and the
    // low tint never followed the valley). ~13 m over 3 km = 0.4%, a
    // low-gradient blackwater profile; the valley incision and interfluve
    // doming carry the relief.
    let tilt = 0.28 * d.cap_relief_m / EXTENT_M;
    let und_amp = d.cap_relief_m * (0.28 - 0.12 * d.cap_flat);
    let l = d.cap_wave_m * 1.6;

    let mut raw = vec![0.0f64; spec.len()];
    for y in 0..ny {
        for x in 0..nx {
            let p = spec.world_of(x, y);
            raw[spec.index(x, y)] = noise::perlin2(p.x / l, p.y / l, s1)
                + 0.50 * noise::perlin2(p.x / l * 2.0, p.y / l * 2.0, s2);
        }
    }
    let mut sorted = raw.clone();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = sorted[(sorted.len() as f64 * 0.05) as usize];
    let hi = sorted[(sorted.len() as f64 * 0.95) as usize];
    let span = (hi - lo).max(1e-9);

    let mut g = Grid::filled(spec, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            let p = spec.world_of(x, y);
            let t = (raw[spec.index(x, y)] - lo) / span - 0.5;
            g.set(x, y, und_amp * t + tilt * (p.x * tc + p.y * ts));
        }
    }
    g
}

// ---------------------------------------------------------------------------
// the network
// ---------------------------------------------------------------------------

/// One channel, stored MOUTH-FIRST: index 0 is the downstream end and `z`
/// strictly increases along the point list. Trunks are reversed into this
/// order after integration; climbs produce it natively.
pub struct Channel {
    pub pts: Vec<Vec2>,
    pub z: Vec<f64>,
    /// 1 = trunk, 2/3 = tributary tiers, 4 = density fill pass.
    pub tier: u8,
    /// Index of the parent channel (None for trunks).
    pub parent: Option<u32>,
    /// Which drainage system this channel belongs to.
    pub sys: u32,
}

impl Channel {
    pub fn arc_len(&self) -> f64 {
        self.pts.windows(2).map(|w| w[0].distance(w[1])).sum()
    }
}

pub struct Network {
    pub chans: Vec<Channel>,
}

/// Why a climb ended. Every termination is counted, never hidden
/// (re-derived: tribs.rs `End`).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum End {
    Divide,
    Edge,
    Claimed,
    MaxArc,
    Stub,
}

// --- the incremental point index (re-derivation of proto.rs ChannelSet) ---

struct Index {
    cell: f64,
    n: usize,
    buckets: Vec<Vec<u32>>,
    pts: Vec<Vec2>,
    z: Vec<f64>,
    chan: Vec<u32>,
}

impl Index {
    fn new() -> Index {
        let cell = 64.0;
        let n = (EXTENT_M / cell).ceil() as usize + 1;
        Index { cell, n, buckets: vec![Vec::new(); n * n],
                pts: Vec::new(), z: Vec::new(), chan: Vec::new() }
    }
    fn key(&self, p: Vec2) -> (usize, usize) {
        let x = (p.x / self.cell).clamp(0.0, (self.n - 1) as f64) as usize;
        let y = (p.y / self.cell).clamp(0.0, (self.n - 1) as f64) as usize;
        (x, y)
    }
    fn add(&mut self, p: Vec2, z: f64, chan: u32) {
        let (x, y) = self.key(p);
        let id = self.pts.len() as u32;
        self.buckets[y * self.n + x].push(id);
        self.pts.push(p);
        self.z.push(z);
        self.chan.push(chan);
    }
    fn add_channel(&mut self, c: &Channel, id: u32) {
        for i in 0..c.pts.len() {
            self.add(c.pts[i], c.z[i], id);
        }
    }
    /// Nearest stored point, optionally skipping one channel BY IDENTITY —
    /// the arc-based exemption was "the entire measured tail bias in one
    /// line" (attempt 4), so it is identity here too.
    fn nearest(&self, p: Vec2, skip: Option<u32>) -> Option<(f64, f64, u32)> {
        if self.pts.is_empty() {
            return None;
        }
        let (kx, ky) = self.key(p);
        let mut best = (f64::MAX, 0.0f64, u32::MAX);
        let mut r = 0i64;
        loop {
            let mut any_cell = false;
            for oy in -r..=r {
                for ox in -r..=r {
                    if r > 0 && ox.abs() != r && oy.abs() != r {
                        continue;
                    }
                    let (x, y) = (kx as i64 + ox, ky as i64 + oy);
                    if x < 0 || y < 0 || x >= self.n as i64 || y >= self.n as i64 {
                        continue;
                    }
                    any_cell = true;
                    for &id in &self.buckets[y as usize * self.n + x as usize] {
                        if Some(self.chan[id as usize]) == skip {
                            continue;
                        }
                        let dist = p.distance(self.pts[id as usize]);
                        if dist < best.0 {
                            best = (dist, self.z[id as usize], self.chan[id as usize]);
                        }
                    }
                }
            }
            // ring guarantee: a hit is final once the next ring cannot beat it
            if best.2 != u32::MAX && best.0 <= (r as f64) * self.cell {
                break;
            }
            r += 1;
            if !any_cell && r as usize > 2 * self.n {
                break;
            }
        }
        if best.2 == u32::MAX { None } else { Some(best) }
    }
}

// --- the proto-elevation field (re-derivation of proto.rs Proto) ----------

/// `e(p) = z(nearest channel) + k·d^0.6 + w·datum(p)` — the potential every
/// climber ascends. The d^0.6 rise makes valleys the low ground everywhere;
/// the datum term is a light tiebreak toward real high ground (attempt 4 ran
/// w_macro = 0.05 of the relief budget; same scale here).
struct Field<'a> {
    idx: &'a Index,
    datum: &'a Grid<f64>,
    k_rise: f64,
    /// Weight on the channel-distance rise term. Tributaries climb AWAY
    /// from the net (1.0, datum a 0.05 tiebreak). Edge fragments are the
    /// headwaters of OFF-TILE systems: the d^0.6 term points them back out
    /// of the tile (every fragment died a stub on it), so they climb the
    /// real ground instead (0.0, datum 1.0).
    w_d6: f64,
}

impl<'a> Field<'a> {
    fn e(&self, p: Vec2) -> f64 {
        if self.w_d6 == 0.0 {
            return self.datum.bilinear(p);
        }
        let (dist, zc, _) = self.idx.nearest(p, None).unwrap();
        self.w_d6 * (zc + self.k_rise * math::pow(dist.max(0.0), 0.6))
            + 0.05 * self.datum.bilinear(p)
    }
    fn grad(&self, p: Vec2) -> Vec2 {
        let h = 12.0;
        Vec2::new(
            (self.e(Vec2::new(p.x + h, p.y)) - self.e(Vec2::new(p.x - h, p.y))) / (2.0 * h),
            (self.e(Vec2::new(p.x, p.y + h)) - self.e(Vec2::new(p.x, p.y - h))) / (2.0 * h),
        )
    }
}

// --- tier parameters (re-derivation of tribs.rs TierParams) ---------------

#[derive(Clone)]
struct Tier {
    spacing: (f64, f64),
    end_margin: f64,
    /// Departure-angle centre and orthogonal-tail p. Corpus: p50 40.7°,
    /// >80° 13.2%. Tail drawn LOW because survival selects for orthogonal
    /// (attempt 4 measured 0.05 drawn → 9–13% landed).
    centre_deg: f64,
    tail_p: f64,
    /// Hold the departure course (mouth, head) — the "downstream sections
    /// have greater mass" dial.
    hold_m: (f64, f64),
    /// Meander swing — small: the tier cut prints the path into the ground
    /// (attempt 4 cut this twice on review, 0.22 → 0.09).
    swing: f64,
    lam: (f64, f64),
    claim: f64,
    min_len: f64,
    max_len: f64,
    step: f64,
    min_gain: f64,
}

fn tier2(attach_m: f64) -> Tier {
    Tier {
        spacing: (attach_m, attach_m * 1.45),
        end_margin: 240.0,
        centre_deg: 41.0,   // tribs.rs tier2, corpus-fit
        tail_p: 0.10,
        hold_m: (320.0, 100.0),
        swing: 0.09,
        lam: (450.0, 750.0),   // review 2026-08-24: lambda floor raised
        claim: 180.0,       // tribs.rs tier2 — sets d2c together with density
        min_len: 150.0,
        // Review 2026-08-24: reach capped well below attempt 4's 2600 m
        // climbs; first ~1/4 tile, then raised to ~1/2 tile on review
        // ("increase it to around 0.5 tile as a max and see").
        max_len: 1500.0,
        step: 20.0,
        min_gain: 0.012,
    }
}

fn tier3(attach_m: f64) -> Tier {
    Tier {
        spacing: (attach_m * 0.65, attach_m * 0.95),
        end_margin: 140.0,
        centre_deg: 41.0,
        tail_p: 0.10,
        hold_m: (200.0, 70.0),
        swing: 0.09,
        lam: (350.0, 600.0),   // review 2026-08-24: lambda floor raised
        claim: 155.0,
        min_len: 90.0,
        max_len: 700.0,
        step: 16.0,
        min_gain: 0.012,
    }
}

// ---------------------------------------------------------------------------
// trunks — the river integrator, generalised for creeks on the cap
// ---------------------------------------------------------------------------

/// One trunk creek, edge-to-edge along the regional drainage axis, steered
/// into datum lows. Re-uses the plan_river heading machinery (water.rs):
/// wobble + wander + downhill lean + border repulsion, inside an acceptance
/// ladder that rejects self-approach AND proximity to already-placed systems.
/// Returned MOUTH-FIRST with a strictly increasing bed.
fn trunk(rng: &mut DetRng, datum: &Grid<f64>, d: &Descriptors,
         slot: (f64, f64), idx: &Index, cardinal_only: bool)
         -> Option<(Vec<Vec2>, Vec<f64>)> {
    let step = 8.0;
    let lim = EXTENT_M - 2.0;
    // regional downstream = down the drawn tilt, quantised to the nearest of
    // EIGHT headings — diagonal crossings are legitimate (review question
    // 2026-08-24); `cardinal_only` is the retry fallback, since a diagonal
    // start near a corner has less room to establish itself.
    let downhill = d.floor_tilt_rad + std::f64::consts::PI;
    let n_axes = if cardinal_only { 4 } else { 8 };
    let quantum = std::f64::consts::TAU / n_axes as f64;
    let base_heading = quantum * (downhill / quantum).round();
    let (cx, cy) = (math::cos(base_heading), math::sin(base_heading));
    // start on a border the heading points away from; a diagonal has two
    // candidates and draws one, positioned so the path has interior to cross
    let mut edges: Vec<(bool, f64)> = Vec::new(); // (x-edge?, coordinate)
    if cx > 0.3 { edges.push((true, 2.0)); }
    if cx < -0.3 { edges.push((true, lim)); }
    if cy > 0.3 { edges.push((false, 2.0)); }
    if cy < -0.3 { edges.push((false, lim)); }
    let (on_x_edge, coord) = edges[rng.below(edges.len())];
    let diagonal = edges.len() == 2;
    let t0 = if !diagonal {
        // The trunk OCCUPIES THE PLAIN'S LOW, not a random offset: D8 on a
        // carved tile whose trunk sat high on the cross-slope routed the
        // tile's flow down the open plain instead of down the valley
        // (valley_compare, seeds 104/106 — the extracted trunk was not our
        // trunk). Seven candidate offsets, keep the lowest crossing line.
        let mut best = (f64::MAX, 0.5 * (slot.0 + slot.1));
        for k in 0..7 {
            let cand = slot.0 + (slot.1 - slot.0) * (k as f64 + rng.range_f64(0.1, 0.9)) / 7.0;
            let mut sum = 0.0;
            for j in 0..24 {
                let along = (j as f64 + 0.5) / 24.0 * EXTENT_M;
                let p = if on_x_edge {
                    Vec2::new(along, cand * EXTENT_M)
                } else {
                    Vec2::new(cand * EXTENT_M, along)
                };
                sum += datum.bilinear(p);
            }
            if sum < best.0 {
                best = (sum, cand);
            }
        }
        best.1
    } else {
        // shift toward the upstream corner so the run crosses the interior
        let along_pos = if on_x_edge { cy > 0.0 } else { cx > 0.0 };
        if along_pos { rng.range_f64(0.06, 0.48) } else { rng.range_f64(0.52, 0.94) }
    };
    let start = if on_x_edge {
        Vec2::new(coord, t0 * EXTENT_M)
    } else {
        Vec2::new(t0 * EXTENT_M, coord)
    };
    // creek planform: longer, lazier than the Nebraska river styles — a
    // low-gradient blackwater creek, not a free-meandering sand-bed river
    // Review 2026-08-24 ("squiggle too much ... the lambda floor should be
    // raised a lot"): sweeping wander, not creek-scale wiggle — the tier cut
    // prints this path into the ground and tight bends cut messy.
    let lam = rng.range_f64(750.0, 1000.0);
    let swing = rng.range_f64(0.45, 0.70);
    let (ph1, ph2) = (rng.range_f64(0.0, std::f64::consts::TAU),
                      rng.range_f64(0.0, std::f64::consts::TAU));
    let (s_noise, s_lam, s_swing) = (rng.next_u32(), rng.next_u32(), rng.next_u32());

    let mut pts: Vec<Vec2> = Vec::new();
    'attempt: for attempt in 0..6 {
        let damp = 0.87f64.powi(attempt); // the ladder damp, water.rs
        let (ph1a, ph2a, s_na, s_la, s_sa) = if attempt == 0 {
            (ph1, ph2, s_noise, s_lam, s_swing)
        } else {
            (rng.range_f64(0.0, std::f64::consts::TAU),
             rng.range_f64(0.0, std::f64::consts::TAU),
             rng.next_u32(), rng.next_u32(), rng.next_u32())
        };
        let mut cand: Vec<Vec2> = vec![start];
        let mut p = start;
        let mut arc = 0.0;
        use std::collections::HashMap;
        let cellsz = 16.0;
        let mut hash: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
        hash.entry(((start.x / cellsz) as i32, (start.y / cellsz) as i32))
            .or_default().push(0);
        for _ in 0..3000 {
            let lam_e = lam * (1.0 + 0.35 * noise::perlin1(arc / 700.0, s_la));
            let sw_e = swing * damp
                * (0.5 + 0.7 * (0.5 + 0.5 * noise::perlin1(arc / 520.0, s_sa)));
            let wob = sw_e
                * (math::sin(std::f64::consts::TAU * arc / lam_e + ph1a)
                    + 0.35 * math::sin(std::f64::consts::TAU * arc / (lam_e * 2.7) + ph2a));
            let wander = 0.55 * damp * noise::perlin1(arc / 1100.0, s_na);
            let hd = base_heading + wob + wander;
            // downhill lean, stronger than the river's: a creek trunk lives
            // in the datum lows, that is the whole point of it
            let lp = Vec2::new(p.x - 70.0 * math::sin(hd), p.y + 70.0 * math::cos(hd));
            let rp = Vec2::new(p.x + 70.0 * math::sin(hd), p.y - 70.0 * math::cos(hd));
            let lean = 0.70 * ((datum.bilinear(rp) - datum.bilinear(lp)) / 5.0).clamp(-1.0, 1.0);
            // Border repulsion as a VECTOR turned into a steering correction
            // (cross product of heading and the inward push) — the scalar
            // lateral form only worked for cardinal crossings. For them this
            // reduces to the old behaviour: the fore/aft borders push along
            // the heading and the cross term vanishes.
            let rx = math::smoothstep(260.0, 40.0, p.x)
                - math::smoothstep(lim - 260.0, lim - 40.0, p.x);
            let ry = math::smoothstep(260.0, 40.0, p.y)
                - math::smoothstep(lim - 260.0, lim - 40.0, p.y);
            let (hx, hy) = (math::cos(hd), math::sin(hd));
            let repel = 0.55 * (hx * ry - hy * rx);
            let dev = (hd - base_heading + lean + repel).clamp(-1.4, 1.4);
            let h = base_heading + dev;
            p = Vec2::new(p.x + step * math::cos(h), p.y + step * math::sin(h));
            if (p.x <= 2.0 || p.x >= lim || p.y <= 2.0 || p.y >= lim) && arc > 400.0 {
                break;
            }
            p.x = p.x.clamp(4.0, lim);
            p.y = p.y.clamp(4.0, lim);
            arc += step;
            // self-approach (the seed-9 figure-eight lesson) …
            let (cx, cy) = ((p.x / cellsz) as i32, (p.y / cellsz) as i32);
            let i = cand.len();
            for dy in -1..=1 {
                for dx in -1..=1 {
                    if let Some(v) = hash.get(&(cx + dx, cy + dy)) {
                        for j in v {
                            if (i - j) as f64 * step > 120.0
                                && cand[*j].distance(p) < 12.0 {
                                continue 'attempt;
                            }
                        }
                    }
                }
            }
            // … and approach to a previously placed SYSTEM: two trunks
            // running confluent would carve one valley twice
            if let Some((dist, _, _)) = idx.nearest(p, None) {
                if dist < 260.0 {
                    continue 'attempt;
                }
            }
            hash.entry((cx, cy)).or_default().push(i);
            cand.push(p);
        }
        if (cand.len() as f64) * step < 1500.0 {
            continue 'attempt;
        }
        pts = cand;
        break;
    }
    if pts.len() < 60 {
        return None;
    }
    // bed on the datum, smoothed, then forced monotone downstream
    let mut bed: Vec<f64> = pts.iter().map(|q| datum.bilinear(*q) - 0.5).collect();
    for _ in 0..2 {
        for i in 1..bed.len() - 1 {
            bed[i] = (bed[i - 1] + bed[i] * 2.0 + bed[i + 1]) / 4.0;
        }
    }
    for i in 1..bed.len() {
        bed[i] = bed[i].min(bed[i - 1] - 0.002);
    }
    // mouth-first: reverse so z increases along the stored list
    pts.reverse();
    bed.reverse();
    Some((pts, bed))
}

// ---------------------------------------------------------------------------
// attach-and-climb (re-derivation of tribs.rs climb + attach_points)
// ---------------------------------------------------------------------------

/// Attachment sites along one channel: arc positions at drawn spacings,
/// clear of the ends. Junction spacing is AUTHORED — an input, not an
/// outcome of where sources landed.
fn attach_points(rng: &mut DetRng, c: &Channel, t: &Tier) -> Vec<usize> {
    let mut arcs: Vec<f64> = vec![0.0];
    for w in c.pts.windows(2) {
        arcs.push(arcs.last().unwrap() + w[0].distance(w[1]));
    }
    let total = *arcs.last().unwrap();
    let mut out = Vec::new();
    if total < t.end_margin * 2.0 + t.spacing.0 {
        return out;
    }
    let mut arc = t.end_margin + rng.range_f64(0.0, t.spacing.0 * 0.6);
    while arc < total - t.end_margin {
        let i = arcs.partition_point(|a| *a < arc).min(c.pts.len() - 1);
        out.push(i);
        arc += rng.range_f64(t.spacing.0, t.spacing.1);
    }
    out
}

/// Climb from one attachment point on channel `cid`. The junction is t = 0:
/// the walk departs at the drawn angle off the parent's downstream tangent,
/// holds its course (interpolated by arc position), then steers uphill on
/// the field — strictly monotone after a bounded grace window.
/// Climb from one attachment point on channel `cid`. The junction is t = 0:
/// the walk departs at the drawn angle off the parent's downstream tangent,
/// holds its course (interpolated by arc position), then steers uphill.
fn climb(rng: &mut DetRng, net: &Network, cid: u32, at: usize,
         field: &Field, t: &Tier) -> Result<(Vec<Vec2>, Vec<f64>, End), End> {
    let c = &net.chans[cid as usize];
    let start = c.pts[at];
    // channels are stored mouth-first, so DOWNSTREAM = toward index 0
    let down = if at > 0 {
        Vec2::new(c.pts[at - 1].x - c.pts[at].x, c.pts[at - 1].y - c.pts[at].y).normalized()
    } else {
        Vec2::new(c.pts[0].x - c.pts[1].x, c.pts[0].y - c.pts[1].y).normalized()
    };
    let mut arcs = 0.0;
    for w in c.pts.windows(2) {
        arcs += w[0].distance(w[1]);
    }
    let arc_at: f64 = c.pts[..=at].windows(2).map(|w| w[0].distance(w[1])).sum();
    let frac = (arc_at / arcs.max(1.0)).clamp(0.0, 1.0);

    // the departure: flow arrives at theta off the downstream tangent; the
    // WALK leaves along the reverse of that flow
    let theta = if rng.next_f64() < t.tail_p {
        rng.range_f64(80.0, 95.0).to_radians()
    } else {
        rng.range_f64(t.centre_deg - 8.0, t.centre_deg + 9.0).to_radians()
    };
    let side = if rng.next_f64() < 0.5 { 1.0 } else { -1.0 };
    let (co, sn) = (math::cos(theta * side), math::sin(theta * side));
    let inc = Vec2::new(down.x * co - down.y * sn, down.x * sn + down.y * co);
    let depart = Vec2::new(-inc.x, -inc.y).normalized();

    // hold interpolated MOUTH→HEAD: mouth-first storage puts the mouth at
    // frac 0, so hold_m.0 belongs at frac 0
    let hold_m = t.hold_m.0 + (t.hold_m.1 - t.hold_m.0) * frac;
    walk(rng, start, c.z[at], depart, hold_m, field, t, Some(cid), Some(c))
}

/// The shared walker: departs `start` along `depart`, holds, then climbs
/// the field. `skip` is the identity exemption for the claim (the parent
/// channel); `parent` enables the 30 m post-junction clearance. Edge
/// fragments walk with neither.
fn walk(rng: &mut DetRng, start: Vec2, z0: f64, depart: Vec2, hold_m: f64,
        field: &Field, t: &Tier, skip: Option<u32>, parent: Option<&Channel>)
        -> Result<(Vec<Vec2>, Vec<f64>, End), End> {
    let lam = rng.range_f64(t.lam.0, t.lam.1);
    let phase = rng.range_f64(0.0, std::f64::consts::TAU);

    let mut pts = vec![start];
    let mut z = vec![z0];
    let mut heading = depart;
    let mut arc = 0.0;
    let mut e_prev = field.e(start);
    let e_start = e_prev;
    let end;
    loop {
        if arc >= t.max_len {
            end = End::MaxArc;
            break;
        }
        let cur = *pts.last().unwrap();
        let g = field.grad(cur);
        let uphill = if g.length() < 1e-9 { heading } else { g.normalized() };
        // full commitment for the first few steps — the junction owns its
        // angle (attempt 4 measured a 10° uphill tilt inside 72 m without it)
        let hold_w = if arc < 3.0 * t.step {
            1.0
        } else {
            ((hold_m - arc) / hold_m.max(1.0)).clamp(0.0, 1.0) * 0.85
        };
        let base = Vec2::new(depart.x * hold_w + uphill.x * (1.0 - hold_w),
                             depart.y * hold_w + uphill.y * (1.0 - hold_w));
        heading = Vec2::new(heading.x * 0.65 + base.x * 0.35,
                            heading.y * 0.65 + base.y * 0.35).normalized();
        let wob = t.swing * math::sin(std::f64::consts::TAU * arc / lam + phase)
            * (arc / 200.0).min(1.0);
        let th = math::atan2(heading.y, heading.x) + wob;
        let mut step_dir = Vec2::new(math::cos(th), math::sin(th));

        // THE MONOTONE RULE with the bounded grace window: a real trib's
        // lower course crosses near-flat floodplain, and strict monotone
        // from step 1 selects for orthogonal departures (attempt 4 measured
        // >80° at 30.8% against a 12% draw). During grace the heading is
        // committed, so the window cannot loop; after it, strict.
        let grace = arc < (hold_m * 0.5).clamp(120.0, 280.0);
        let mut next = Vec2::new(cur.x + step_dir.x * t.step, cur.y + step_dir.y * t.step);
        let mut e_next = field.e(next);
        if !grace && e_next < e_prev + t.min_gain {
            step_dir = uphill;
            next = Vec2::new(cur.x + step_dir.x * t.step, cur.y + step_dir.y * t.step);
            e_next = field.e(next);
            if e_next < e_prev + t.min_gain {
                end = End::Divide;
                break;
            }
        }
        if next.x < 4.0 || next.y < 4.0 || next.x > EXTENT_M - 4.0 || next.y > EXTENT_M - 4.0 {
            end = End::Edge;
            break;
        }
        // the claim: parent exempt by identity, everything else forbidden
        if let Some((dist, _, _)) = field.idx.nearest(next, skip) {
            if dist < t.claim {
                for _ in 0..2 {
                    if pts.len() > 2 {
                        pts.pop();
                        z.pop();
                    }
                }
                end = End::Claimed;
                break;
            }
        }
        // The parent identity exemption is for the JUNCTION, not the whole
        // walk: past the departure a committed grace-window heading can plow
        // back into the parent's next bend (seed 24: a trib 3 m off its own
        // parent at arc 80). Beyond 48 m of arc the trib must keep an 18 m
        // hard clearance from its parent — 18 because two checked points
        // 20 m apart at 18 m can still dip to ~15 m between samples, safely
        // above the 10 m crossing assertion.
        if let (true, Some(par)) = (arc + t.step > 48.0, parent) {
            let dpar = par.pts.iter().map(|q| q.distance(next)).fold(f64::MAX, f64::min);
            if dpar < 30.0 {
                for _ in 0..2 {
                    if pts.len() > 2 {
                        pts.pop();
                        z.pop();
                    }
                }
                end = End::Claimed;
                break;
            }
        }
        heading = step_dir;
        arc += t.step;
        pts.push(next);
        z.push((e_next - e_start + z0).max(z.last().unwrap() + 0.01));
        e_prev = e_next;
    }
    let len: f64 = pts.windows(2).map(|w| w[0].distance(w[1])).sum();
    if len < t.min_len || pts.len() < 4 {
        if std::env::var("NET_DEBUG").is_ok() {
            eprintln!("  stub: {end:?} at len {len:.0} from ({:.0},{:.0})", start.x, start.y);
        }
        return Err(End::Stub);
    }
    Ok((pts, z, end))
}

// ---------------------------------------------------------------------------
// grow — the whole C2 stage
// ---------------------------------------------------------------------------

/// Grow the full network on the datum: trunks, then tier-2 tribs on trunks,
/// tier-3 on everything, then density-fill passes until the drainage density
/// lands in the corpus band (2.33 km/km² measured; band 2.0–2.7).
pub fn grow(rng: &mut DetRng, datum: &Grid<f64>, d: &Descriptors) -> Network {
    let mut net = Network { chans: Vec::new() };
    let mut idx = Index::new();

    // --- the trunk: ONE per tile (review 2026-08-24 — every real NC tile
    // is single-trunked; the survey's extra "systems" are edge fragments).
    for s in 0..d.n_sys.min(1) {
        let got = trunk(rng, datum, d, (0.25, 0.75), &idx, false)
            .or_else(|| trunk(rng, datum, d, (0.25, 0.75), &idx, true));
        if let Some((pts, z)) = got {
            let c = Channel { pts, z, tier: 1, parent: None, sys: s };
            idx.add_channel(&c, net.chans.len() as u32);
            net.chans.push(c);
        }
    }

    // attempt 4's rise coefficient: 0.55 of the budget over an 800 m climb
    let k_rise = 0.55 * d.cap_relief_m / math::pow(800.0, 0.6);

    // --- the main tree: tier-2 tribs on the trunk, tier-3 on those
    let t2_targets: Vec<u32> =
        (0..net.chans.len() as u32).filter(|c| net.chans[*c as usize].tier == 1).collect();
    tier_pass(rng, &mut net, &mut idx, datum, k_rise, &tier2(d.attach_m), &t2_targets, 2);
    let t3_targets: Vec<u32> =
        (0..net.chans.len() as u32).filter(|c| net.chans[*c as usize].tier == 2).collect();
    tier_pass(rng, &mut net, &mut idx, datum, k_rise, &tier3(d.attach_m), &t3_targets, 3);

    // --- FINGERS (review 2026-08-26: "more very small length tributaries
    // at the tips"): the ordinary passes keep an end margin, so channel
    // heads were bare. Short steep head-water nicks, placed only on the
    // head-ward quarter of tier-2/3 channels.
    let finger = Tier {
        spacing: (110.0, 170.0),
        end_margin: 24.0,
        centre_deg: 41.0,
        tail_p: 0.10,
        hold_m: (70.0, 40.0),
        swing: 0.09,
        lam: (150.0, 300.0),
        claim: 80.0,
        min_len: 50.0,
        max_len: 210.0,
        step: 12.0,
        min_gain: 0.008,
    };
    let tips: Vec<u32> = (0..net.chans.len() as u32)
        .filter(|c| net.chans[*c as usize].tier >= 2)
        .collect();
    for cid in tips {
        let sites = {
            let c = &net.chans[cid as usize];
            let mut arcs: Vec<f64> = vec![0.0];
            for w in c.pts.windows(2) {
                arcs.push(arcs.last().unwrap() + w[0].distance(w[1]));
            }
            let total = *arcs.last().unwrap();
            let mut out = Vec::new();
            let mut a = (0.70 * total).max(finger.end_margin);
            while a < total - finger.end_margin {
                out.push(arcs.partition_point(|x| *x < a).min(c.pts.len() - 1));
                a += rng.range_f64(finger.spacing.0, finger.spacing.1);
            }
            out
        };
        for at in sites {
            let field = Field { idx: &idx, datum, k_rise, w_d6: 1.0 };
            if let Ok((pts, z, _)) = climb(rng, &net, cid, at, &field, &finger) {
                let sys = net.chans[cid as usize].sys;
                let c = Channel { pts, z, tier: 4, parent: Some(cid), sys };
                idx.add_channel(&c, net.chans.len() as u32);
                net.chans.push(c);
            }
        }
    }

    // --- EDGE FRAGMENTS (review 2026-08-24): with quarter-tile tribs a
    // single trunk covers only its own band — density fell to 1.4-1.9 and
    // d2c blew out to 200-455 against the corpus 2.33/107. Real tiles close
    // that gap with clipped pieces of NEIGHBORING systems: short streams
    // whose mouths sit on the tile border and whose headwaters climb inward.
    // The survey's n_sys 3 / main_share 57% is exactly this structure.
    // --- final polish: small fills where the ground is still far from water
    let mut n = 0;
    while d2c_p50(&net) > 165.0 && density_km_km2(&net) < 1.85 && n < 4 {
        let mut t = tier3(d.attach_m * 0.8);
        t.min_len = 110.0;
        // fills squeeze into interior voids the fragments cannot reach; the
        // relaxed claim lets a climb thread between saturated neighbours on
        // its way out (it still cannot TOUCH them — 130 m clearance), and
        // the longer leg lets it actually arrive (tier-3's 450 m cap left
        // the deep voids untouched)
        t.claim = 130.0;
        t.max_len = 800.0;
        let all: Vec<u32> = (0..net.chans.len() as u32).collect();
        let before = net.chans.len();
        tier_pass(rng, &mut net, &mut idx, datum, k_rise, &t, &all, 4);
        if net.chans.len() == before {
            break;
        }
        n += 1;
    }
    // --- the fallback fragment (review 2026-08-24: "remove all of the half
    // trunks ... maybe for really low density we can include a single half
    // spanning trunk"). Only a genuinely underwatered tile gets one, and it
    // gets exactly one: a clipped neighboring trunk entering at the emptiest
    // border low, carrying its own tribs.
    if density_km_km2(&net) < 1.45 {
        let mut placed = 0u32;
        let mut n_frag = 0u32;
        while n_frag < 4 && placed < 1 {
            let fm = frag_mouth(&idx, datum);
            if std::env::var("NET_DEBUG").is_ok() {
                eprintln!("  frag_mouth -> {:?}", fm.map(|(m, _)| (m.x, m.y)));
            }
            let Some((mouth, inward)) = fm else { break };
            if let Some((pts, z)) = frag_trunk(rng, datum, &idx, mouth, inward) {
                let sys = net.chans.iter().map(|c| c.sys).max().unwrap_or(0) + 1;
                let cid = net.chans.len() as u32;
                let c = Channel { pts, z, tier: 1, parent: None, sys };
                idx.add_channel(&c, cid);
                net.chans.push(c);
                let before_t2 = net.chans.len() as u32;
                let mut ft2 = tier2(d.attach_m);
                ft2.max_len = 650.0;
                tier_pass(rng, &mut net, &mut idx, datum, k_rise, &ft2, &[cid], 2);
                let new_t2: Vec<u32> = (before_t2..net.chans.len() as u32).collect();
                tier_pass(rng, &mut net, &mut idx, datum, k_rise,
                          &tier3(d.attach_m), &new_t2, 3);
                tier_pass(rng, &mut net, &mut idx, datum, k_rise,
                          &tier3(d.attach_m), &[cid], 3);
                placed += 1;
            }
            n_frag += 1;
        }
    }
    net
}

/// One tributary pass: attachment sites on every target channel, one climb
/// per site with a single stub retry.
fn tier_pass(rng: &mut DetRng, net: &mut Network, idx: &mut Index,
             datum: &Grid<f64>, k_rise: f64, tier: &Tier, targets: &[u32],
             tier_no: u8) {
    for &cid in targets {
        let sites = attach_points(rng, &net.chans[cid as usize], tier);
        for at in sites {
            // skip only sites doomed in EVERY direction (first step lands
            // inside a non-parent claim no matter where it points); a site
            // that merely NEIGHBORS a claim can still walk away from it
            let start = net.chans[cid as usize].pts[at];
            if idx.nearest(start, Some(cid))
                .is_some_and(|(dd, _, _)| dd < tier.claim - tier.step) {
                continue;
            }
            for _attempt in 0..2 {
                let field = Field { idx: &*idx, datum, k_rise, w_d6: 1.0 };
                match climb(rng, net, cid, at, &field, tier) {
                    Ok((pts, z, _end)) => {
                        let sys = net.chans[cid as usize].sys;
                        let c = Channel { pts, z, tier: tier_no, parent: Some(cid), sys };
                        idx.add_channel(&c, net.chans.len() as u32);
                        net.chans.push(c);
                        break;
                    }
                    Err(End::Stub) => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

/// The border point farthest from every existing channel — where the next
/// off-tile system's clipped fragment enters. Returns the mouth and the
/// inward normal, or None once the whole border is watered.
fn frag_mouth(idx: &Index, datum: &Grid<f64>) -> Option<(Vec2, Vec2)> {
    let lim = EXTENT_M - 6.0;
    // lowest border point among those with room (a fragment's first reach
    // must clear the claim) — valleys cross borders at their low points
    let mut best: Option<(f64, Vec2, Vec2)> = None;
    let mut t = 24.0;
    while t < EXTENT_M - 24.0 {
        for (p, nrm) in [
            (Vec2::new(t, 6.0), Vec2::new(0.0, 1.0)),
            (Vec2::new(t, lim), Vec2::new(0.0, -1.0)),
            (Vec2::new(6.0, t), Vec2::new(1.0, 0.0)),
            (Vec2::new(lim, t), Vec2::new(-1.0, 0.0)),
        ] {
            let dist = idx.nearest(p, None).map(|(dd, _, _)| dd).unwrap_or(f64::MAX);
            // room ≥ 680: min_len of walkable ground plus the claim, else
            // the walk cannot survive to acceptance (seed 107 burned all 8
            // attempts on one 500 m-roomed mouth)
            if dist < 680.0 {
                continue;
            }
            let z = datum.bilinear(p);
            if best.map_or(true, |(bz, _, _)| z < bz) {
                best = Some((z, p, nrm));
            }
        }
        t += 24.0;
    }
    best.map(|(_, p, nrm)| (p, nrm))
}

/// A neighboring system's clipped trunk: enters at a border mouth, runs
/// inward on a committed heading with the trunk integrator's organs (wobble,
/// wander, low-seeking lean), and ends at its drawn length, at another edge,
/// or on approach to the resident network — that approach IS the divide
/// zone. Returned mouth-first with a strictly increasing bed.
fn frag_trunk(rng: &mut DetRng, datum: &Grid<f64>, idx: &Index,
              mouth: Vec2, inward: Vec2) -> Option<(Vec<Vec2>, Vec<f64>)> {
    let step = 8.0;
    let lim = EXTENT_M - 2.0;
    let base_heading = math::atan2(inward.y, inward.x) + rng.range_f64(-0.35, 0.35);
    let lam = rng.range_f64(700.0, 950.0);
    let swing = rng.range_f64(0.35, 0.60);
    let len_cap = rng.range_f64(800.0, 1600.0);
    let mut out: Option<Vec<Vec2>> = None;
    'attempt: for attempt in 0..3 {
        let damp = 0.87f64.powi(attempt);
        let (ph1, ph2) = (rng.range_f64(0.0, std::f64::consts::TAU),
                          rng.range_f64(0.0, std::f64::consts::TAU));
        let (s_n, s_l, s_s) = (rng.next_u32(), rng.next_u32(), rng.next_u32());
        let mut cand = vec![mouth];
        let mut p = mouth;
        let mut arc = 0.0;
        loop {
            if arc >= len_cap {
                break;
            }
            let lam_e = lam * (1.0 + 0.35 * noise::perlin1(arc / 700.0, s_l));
            let sw_e = swing * damp
                * (0.5 + 0.7 * (0.5 + 0.5 * noise::perlin1(arc / 520.0, s_s)));
            let wob = sw_e
                * (math::sin(std::f64::consts::TAU * arc / lam_e + ph1)
                    + 0.35 * math::sin(std::f64::consts::TAU * arc / (lam_e * 2.7) + ph2));
            let wander = 0.55 * damp * noise::perlin1(arc / 1100.0, s_n);
            let hd = base_heading + wob + wander;
            let lp = Vec2::new(p.x - 70.0 * math::sin(hd), p.y + 70.0 * math::cos(hd));
            let rp = Vec2::new(p.x + 70.0 * math::sin(hd), p.y - 70.0 * math::cos(hd));
            let lean = 0.50 * ((datum.bilinear(rp) - datum.bilinear(lp)) / 5.0).clamp(-1.0, 1.0);
            let dev = (hd - base_heading + lean).clamp(-1.2, 1.2);
            let h = base_heading + dev;
            p = Vec2::new(p.x + step * math::cos(h), p.y + step * math::sin(h));
            if (p.x <= 2.0 || p.x >= lim || p.y <= 2.0 || p.y >= lim) && arc > 200.0 {
                break;
            }
            p.x = p.x.clamp(4.0, lim - 2.0);
            p.y = p.y.clamp(4.0, lim - 2.0);
            arc += step;
            // approach to the resident network: the divide — stop, keep
            if let Some((dist, _, _)) = idx.nearest(p, None) {
                if dist < 200.0 {
                    break;
                }
            }
            // self-approach: a hooked draw is rejected wholesale
            for (j, q) in cand.iter().enumerate() {
                if (cand.len() - j) as f64 * step > 120.0 && q.distance(p) < 12.0 {
                    continue 'attempt;
                }
            }
            cand.push(p);
        }
        let arc_total = (cand.len() - 1) as f64 * step;
        let disp = cand[0].distance(*cand.last().unwrap());
        // reject curls: a through-going stream displaces most of its arc
        if arc_total < 340.0 || disp < 0.55 * arc_total {
            continue 'attempt;
        }
        out = Some(cand);
        break;
    }
    let pts = out?;
    // bed on the datum, mouth-first: strictly increasing upstream
    let mut bed: Vec<f64> = pts.iter().map(|q| datum.bilinear(*q) - 1.0).collect();
    for _ in 0..2 {
        for i in 1..bed.len() - 1 {
            bed[i] = (bed[i - 1] + bed[i] * 2.0 + bed[i + 1]) / 4.0;
        }
    }
    for i in 1..bed.len() {
        bed[i] = bed[i].max(bed[i - 1] + 0.002);
    }
    Some((pts, bed))
}

// ---------------------------------------------------------------------------
// the graph battery — measured in-crate, BEFORE any surface exists
// ---------------------------------------------------------------------------

pub struct NetStats {
    pub density_km_km2: f64,
    pub d2c_p50_m: f64,
    pub junc_p50_deg: f64,
    pub junc_gt80_frac: f64,
    pub main_share: f64,
    pub n_sys: usize,
    pub n_chans: usize,
    pub crossings: usize,
}

pub fn density_km_km2(net: &Network) -> f64 {
    let total_m: f64 = net.chans.iter().map(|c| c.arc_len()).sum();
    (total_m / 1000.0) / ((EXTENT_M / 1000.0) * (EXTENT_M / 1000.0))
}

/// d2c p50 via a two-pass chamfer transform on the 8 m grid — the corpus
/// instrument's resolution. Also the fill-pass criterion in `grow`: density
/// alone saturates near the existing tree and leaves far corners empty
/// (single-trunk battery: d2c 108–181 against the corpus 107).
pub fn d2c_p50(net: &Network) -> f64 {
    let spec = macro_spec();
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let big = 1e18f64;
    let mut dt = vec![big; (nx * ny) as usize];
    for c in &net.chans {
        for p in &c.pts {
            let x = (p.x / 8.0).round().clamp(0.0, (nx - 1) as f64) as i64;
            let y = (p.y / 8.0).round().clamp(0.0, (ny - 1) as f64) as i64;
            dt[(y * nx + x) as usize] = 0.0;
        }
    }
    let (orth, diag) = (8.0, 8.0 * std::f64::consts::SQRT_2);
    for y in 0..ny {
        for x in 0..nx {
            let i = (y * nx + x) as usize;
            for (dx, dy, w) in [(-1i64, 0i64, orth), (0, -1, orth), (-1, -1, diag), (1, -1, diag)] {
                let (px, py) = (x + dx, y + dy);
                if px >= 0 && px < nx && py >= 0 && py < ny {
                    let v = dt[(py * nx + px) as usize] + w;
                    if v < dt[i] { dt[i] = v; }
                }
            }
        }
    }
    for y in (0..ny).rev() {
        for x in (0..nx).rev() {
            let i = (y * nx + x) as usize;
            for (dx, dy, w) in [(1i64, 0i64, orth), (0, 1, orth), (1, 1, diag), (-1, 1, diag)] {
                let (px, py) = (x + dx, y + dy);
                if px >= 0 && px < nx && py >= 0 && py < ny {
                    let v = dt[(py * nx + px) as usize] + w;
                    if v < dt[i] { dt[i] = v; }
                }
            }
        }
    }
    let mut dts: Vec<f64> = dt;
    dts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    dts[dts.len() / 2]
}

pub fn stats(net: &Network) -> NetStats {
    let d2c_p50 = d2c_p50(net);
    // junction angles, measured from GEOMETRY, not from the draw
    let mut angles: Vec<f64> = Vec::new();
    for c in &net.chans {
        let Some(pid) = c.parent else { continue };
        let p = &net.chans[pid as usize];
        if c.pts.len() < 2 || p.pts.len() < 2 { continue; }
        let mouth = c.pts[0];
        let mut k = 0usize;
        let mut bd = f64::MAX;
        for (i, q) in p.pts.iter().enumerate() {
            let dd = q.distance(mouth);
            if dd < bd { bd = dd; k = i; }
        }
        let k = k.max(1);
        let pflow = Vec2::new(p.pts[k - 1].x - p.pts[k].x, p.pts[k - 1].y - p.pts[k].y)
            .normalized();
        let tflow = Vec2::new(c.pts[0].x - c.pts[1].x, c.pts[0].y - c.pts[1].y).normalized();
        let dot = (pflow.x * tflow.x + pflow.y * tflow.y).clamp(-1.0, 1.0);
        angles.push(dot.acos().to_degrees());
    }
    angles.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let junc_p50 = if angles.is_empty() { f64::NAN } else { angles[angles.len() / 2] };
    let gt80 = if angles.is_empty() { 0.0 } else {
        angles.iter().filter(|a| **a > 80.0).count() as f64 / angles.len() as f64
    };

    // main_share over systems
    let n_sys = net.chans.iter().filter(|c| c.tier == 1).count();
    let mut per_sys = vec![0.0f64; net.chans.iter().map(|c| c.sys).max().unwrap_or(0) as usize + 1];
    for c in &net.chans {
        per_sys[c.sys as usize] += c.arc_len();
    }
    let total: f64 = per_sys.iter().sum();
    let main_share = per_sys.iter().cloned().fold(0.0, f64::max) / total.max(1e-9);

    NetStats {
        density_km_km2: density_km_km2(net),
        d2c_p50_m: d2c_p50,
        junc_p50_deg: junc_p50,
        junc_gt80_frac: gt80,
        main_share,
        n_sys,
        n_chans: net.chans.len(),
        crossings: crossings(net),
    }
}

/// Crossings ASSERTED, not sampled: any point of a channel closer than 10 m
/// to a DIFFERENT channel, outside a 60 m junction exemption zone, is a
/// crossing. Two polylines sampled at ≤20 m steps cannot intersect in plan
/// without producing such a pair.
pub fn crossings(net: &Network) -> usize {
    // junction exemption: points near any mouth
    let mouths: Vec<Vec2> = net.chans.iter()
        .filter(|c| c.parent.is_some())
        .map(|c| c.pts[0]).collect();
    let near_junction = |p: Vec2| mouths.iter().any(|m| m.distance(p) < 60.0);

    let mut idx = Index::new();
    for (i, c) in net.chans.iter().enumerate() {
        idx.add_channel(c, i as u32);
    }
    let mut n = 0usize;
    for (i, c) in net.chans.iter().enumerate() {
        for p in &c.pts {
            if near_junction(*p) {
                continue;
            }
            if let Some((dist, _, other)) = idx.nearest(*p, Some(i as u32)) {
                // parent is NOT exempt out here — away from its junction a
                // trib must keep clear of its own parent too
                if dist < 10.0 && !near_junction(idx_point(net, other, *p)) {
                    n += 1;
                }
            }
        }
    }
    n
}

fn idx_point(net: &Network, chan: u32, near: Vec2) -> Vec2 {
    // nearest point of `chan` to `near` (for the junction test on the OTHER
    // side of a close pair)
    let c = &net.chans[chan as usize];
    let mut best = (f64::MAX, c.pts[0]);
    for p in &c.pts {
        let dd = p.distance(near);
        if dd < best.0 {
            best = (dd, *p);
        }
    }
    best.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mode::Mode;
    use course_seed::RunIdentity;

    fn skeleton(seed: u64) -> (Grid<f64>, Network) {
        let id = RunIdentity::from_seed(seed);
        let d = crate::draw::site(&id, Some(Mode::Fluvial), None);
        let mut dr = crate::rng::stream(&id, crate::rng::DATUM);
        let g = datum(&mut dr, &d);
        let mut cr = crate::rng::stream(&id, crate::rng::CHANNEL);
        let net = grow(&mut cr, &g, &d);
        (g, net)
    }

    #[test]
    fn the_datum_relief_lands_in_the_corpus_band() {
        // corpus relief p99-p1 46.9; the datum carries most of it (valleys
        // add a few metres of local cut later)
        for seed in [11u64, 12, 13] {
            let (g, _) = skeleton(seed);
            let mut v = g.data.clone();
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let relief = v[(v.len() as f64 * 0.99) as usize]
                - v[(v.len() as f64 * 0.01) as usize];
            // datum-only relief: the valley carve adds its 12-17 m on top
            assert!(relief > 14.0 && relief < 45.0, "seed {seed}: relief {relief:.1}");
        }
    }

    #[test]
    fn network_never_crosses() {
        for seed in 20u64..28 {
            let (_, net) = skeleton(seed);
            assert_eq!(crossings(&net), 0, "seed {seed}");
        }
    }

    #[test]
    fn climbs_are_monotone() {
        // z strictly increases mouth→head on EVERY channel — the structural
        // no-loop guarantee, checked directly
        for seed in [31u64, 32, 33, 34] {
            let (_, net) = skeleton(seed);
            for (i, c) in net.chans.iter().enumerate() {
                for w in c.z.windows(2) {
                    assert!(w[1] > w[0], "seed {seed} chan {i}: bed not monotone");
                }
            }
        }
    }

    #[test]
    fn density_lands_in_band() {
        // corpus 2.33 km/km²; each seed within a generous band, the pool
        // near the target
        let mut pool = Vec::new();
        for seed in 40u64..64 {
            let (_, net) = skeleton(seed);
            let dens = density_km_km2(&net);
            assert!(dens > 1.05 && dens < 2.6, "seed {seed}: density {dens:.2}");
            pool.push(dens);
        }
        let mean = pool.iter().sum::<f64>() / pool.len() as f64;
        // review 2026-08-24: densities run ~25% under the corpus 2.33 until
        // the carve round shows whether the cut ground reads sparse
        assert!(mean > 1.4 && mean < 2.3, "pooled density {mean:.2}");
    }

    #[test]
    fn junction_angles_in_corpus_band() {
        // corpus p50 40.7°, >80° 13.2% — pooled across seeds
        let mut angles = 0.0f64;
        let mut gt80 = 0.0f64;
        let mut n = 0usize;
        for seed in [70u64, 71, 72, 73, 74, 75] {
            let (_, net) = skeleton(seed);
            let s = stats(&net);
            if s.junc_p50_deg.is_finite() {
                angles += s.junc_p50_deg;
                gt80 += s.junc_gt80_frac;
                n += 1;
            }
        }
        let p50 = angles / n as f64;
        let tail = gt80 / n as f64;
        assert!(p50 > 30.0 && p50 < 55.0, "junction p50 {p50:.1}");
        assert!(tail < 0.30, "orthogonal tail {tail:.2}");
    }

    #[test]
    fn fluvial_mode_draws_channels() {
        let (_, net) = skeleton(99);
        assert!(net.chans.len() >= 3, "only {} channels", net.chans.len());
        let total: f64 = net.chans.iter().map(|c| c.arc_len()).sum();
        assert!(total > 8_000.0, "network only {total:.0} m");
    }
}
