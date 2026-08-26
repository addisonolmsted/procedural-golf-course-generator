//! Water erosion and deposition on the assembled macro surface.
//!
//! The `z = B + H(u,W)` representation builds a valley by *construction*: it
//! states the height above the drainage rather than deriving it. That gives
//! correct depths and widths but leaves two known marks — valley edges that
//! are as smooth as the profile that drew them, and interfluves carrying the
//! branching ridge network of the drainage dual. A short landscape-evolution
//! pass addresses both from the physics rather than from another dial:
//!
//! * **stream-power incision** sharpens the valley edge, because incision
//!   rate scales with drainage area and so cuts hardest exactly along the
//!   lines the network already occupies;
//! * **hillslope diffusion** rounds the interfluves, because a diffusive
//!   operator decays curvature fastest where curvature is highest — which is
//!   the ridge crests;
//! * **deposition** puts the eroded material back where transport slackens,
//!   building the flat valley floors the corpus shows.
//!
//! The scheme is Braun & Willett's implicit stream-power solver (O(n),
//! unconditionally stable for n = 1) with the Davy–Lague deposition term
//! applied by operator splitting, and explicit sub-stepped linear diffusion.
//! On the 8 m macro grid (376², ~141k cells) a 60-step run costs a few tens
//! of milliseconds, which is why this can sit in the build at all.
//!
//! It is deliberately a REFINEMENT, not a simulation: the run is short and
//! the coefficients small, so the authored network stays the network. Flow
//! routing follows the channels because they are already incised, so the
//! pass deepens what is there instead of inventing its own drainage.

use std::cmp::Ordering;
use std::collections::BinaryHeap;

use course_world::grid::Grid;

/// Coefficients for one erosion run. All lengths in metres.
#[derive(Clone, Copy, Debug)]
pub struct Erosion {
    /// Stream-power coefficient (per step, per m^(1-2m) of area).
    pub k: f64,
    /// Drainage-area exponent. 0.5 is the standard m/n = 0.5 for n = 1.
    pub m: f64,
    /// Davy–Lague deposition coefficient, dimensionless.
    pub g: f64,
    /// Hillslope diffusivity, m²/step.
    pub kd: f64,
    /// Critical hillslope gradient for NONLINEAR (Roering) transport. Zero
    /// selects plain linear diffusion.
    ///
    /// This is the term that does what a review asks for when it wants
    /// "sharper edges and smoother ridges" at the same time. Linear
    /// diffusion can only smooth: it decays every curvature, so it rounds
    /// the ridge AND blunts the valley shoulder. Roering flux
    /// `q = -Kd S / (1 - (S/Sc)^2)` instead makes transport run away as the
    /// slope approaches Sc, so gentle ground (ridge crests, interfluves)
    /// diffuses normally while steep ground evacuates almost freely. The
    /// result is the real signature of a threshold landscape: convex, smooth
    /// hilltops meeting near-planar sideslopes at a SHARP break — which is
    /// the valley edge.
    pub sc: f64,
    /// Step size (dimensionless — folded into k and kd).
    pub dt: f64,
    pub steps: usize,
}

impl Default for Erosion {
    fn default() -> Self {
        // Tuned on the ten review seeds so the median |dz| lands under a
        // metre: this is meant to refine the assembled surface, not to
        // relandscape it.
        Erosion { k: 4.5e-5, m: 0.5, g: 0.55, kd: 1.4, sc: 0.0, dt: 1.0, steps: 60 }
    }
}

/// What a run actually did, so a review can see it rather than trust it.
#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub mean_abs_dz: f64,
    pub p99_abs_dz: f64,
    pub max_dz: f64,
    pub min_dz: f64,
    pub eroded_m3: f64,
    pub deposited_m3: f64,
}

/// Min-heap entry for the priority flood.
struct Node(f64, usize);
impl PartialEq for Node {
    fn eq(&self, o: &Self) -> bool {
        self.0 == o.0
    }
}
impl Eq for Node {}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        // reversed: BinaryHeap is a max-heap, we want the lowest cell first
        o.0.partial_cmp(&self.0).unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}

/// Priority-flood depression fill (Barnes et al.). Used for ROUTING ONLY —
/// the eroded surface keeps its own hollows, which is what lets ponds and
/// bays survive the pass.
fn fill(spec: &course_world::grid::GridSpec, z: &[f64]) -> Vec<f64> {
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let mut zf = vec![f64::INFINITY; z.len()];
    let mut seen = vec![false; z.len()];
    let mut heap: BinaryHeap<Node> = BinaryHeap::new();
    for y in 0..ny {
        for x in 0..nx {
            if x == 0 || y == 0 || x == nx - 1 || y == ny - 1 {
                let i = spec.index(x as u32, y as u32);
                zf[i] = z[i];
                seen[i] = true;
                heap.push(Node(z[i], i));
            }
        }
    }
    const EPS: f64 = 1e-4;
    while let Some(Node(zc, i)) = heap.pop() {
        let (x, y) = ((i % nx as usize) as i64, (i / nx as usize) as i64);
        for (dx, dy) in NB {
            let (a, b) = (x + dx, y + dy);
            if a < 0 || b < 0 || a >= nx || b >= ny {
                continue;
            }
            let j = spec.index(a as u32, b as u32);
            if seen[j] {
                continue;
            }
            seen[j] = true;
            zf[j] = z[j].max(zc + EPS);
            heap.push(Node(zf[j], j));
        }
    }
    zf
}

const NB: [(i64, i64); 8] = [
    (-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, -1), (-1, 1), (1, 1),
];

/// Erode `z` in place. Returns what changed.
pub fn run(z: &mut Grid<f64>, p: &Erosion) -> Stats {
    let spec = z.spec;
    let (nx, ny) = (spec.nx as i64, spec.ny as i64);
    let n = spec.len();
    let dx = spec.cell_size;
    let cell_area = dx * dx;
    let z0: Vec<f64> = z.data.clone();

    let mut recv = vec![0usize; n];
    let mut rdist = vec![dx; n];
    let mut area = vec![0.0f64; n];
    let mut stack: Vec<usize> = Vec::with_capacity(n);
    let mut ero = vec![0.0f64; n];
    let mut qs = vec![0.0f64; n];
    let (mut eroded, mut deposited) = (0.0f64, 0.0f64);

    for _ in 0..p.steps {
        let zf = fill(&spec, &z.data);

        // --- D8 receivers on the FILLED surface ---------------------------
        for y in 0..ny {
            for x in 0..nx {
                let i = spec.index(x as u32, y as u32);
                recv[i] = i;
                rdist[i] = dx;
                if x == 0 || y == 0 || x == nx - 1 || y == ny - 1 {
                    continue; // border is base level: it drains off-tile
                }
                let mut best = 0.0;
                for (ddx, ddy) in NB {
                    let j = spec.index((x + ddx) as u32, (y + ddy) as u32);
                    let d = if ddx != 0 && ddy != 0 { dx * std::f64::consts::SQRT_2 } else { dx };
                    let slope = (zf[i] - zf[j]) / d;
                    if slope > best {
                        best = slope;
                        recv[i] = j;
                        rdist[i] = d;
                    }
                }
            }
        }

        // --- Braun & Willett stack: every node after its receiver ---------
        // Counting sort into donor lists, then an iterative DFS from each
        // base-level node. O(n), no recursion.
        let mut ndon = vec![0u32; n];
        for i in 0..n {
            if recv[i] != i {
                ndon[recv[i]] += 1;
            }
        }
        let mut off = vec![0usize; n + 1];
        for i in 0..n {
            off[i + 1] = off[i] + ndon[i] as usize;
        }
        let mut fill_at = off.clone();
        let mut donors = vec![0usize; off[n]];
        for i in 0..n {
            if recv[i] != i {
                donors[fill_at[recv[i]]] = i;
                fill_at[recv[i]] += 1;
            }
        }
        stack.clear();
        for i in 0..n {
            if recv[i] == i {
                stack.push(i);
                let mut head = stack.len() - 1;
                while head < stack.len() {
                    let c = stack[head];
                    head += 1;
                    for k in off[c]..off[c + 1] {
                        stack.push(donors[k]);
                    }
                }
            }
        }

        // --- drainage area: upstream-to-downstream accumulation -----------
        for a in area.iter_mut() {
            *a = cell_area;
        }
        for &i in stack.iter().rev() {
            if recv[i] != i {
                area[recv[i]] += area[i];
            }
        }

        // --- implicit stream power, n = 1 ---------------------------------
        // z_i = (z_i + f z_r) / (1 + f),  f = k dt A^m / dx.  Solved in
        // stack order so z_r is already final. Unconditionally stable, which
        // is the whole reason for the Braun-Willett ordering.
        for &i in stack.iter() {
            let r = recv[i];
            if r == i {
                ero[i] = 0.0;
                continue;
            }
            let before = z.data[i];
            let f = p.k * p.dt * area[i].powf(p.m) / rdist[i];
            let mut zn = (z.data[i] + f * z.data[r]) / (1.0 + f);
            if zn < z.data[r] {
                zn = z.data[r]; // never invert the flow path
            }
            z.data[i] = zn;
            ero[i] = (before - zn).max(0.0);
        }

        // --- deposition: Davy-Lague G Qs / A, by operator splitting -------
        // Sediment is routed upstream-to-downstream; each node deposits a
        // share of what passes through it and hands on the rest. Where the
        // network slackens — big area, low slope, the valley floors — the
        // deposition term dominates and builds the flat floor the corpus
        // shows, without any explicit "floodplain" rule.
        for q in qs.iter_mut() {
            *q = 0.0;
        }
        for &i in stack.iter().rev() {
            qs[i] += ero[i] * cell_area;
            eroded += ero[i] * cell_area;
            let mut dep = p.g * qs[i] / area[i];
            let avail = qs[i] / cell_area;
            if dep > avail {
                dep = avail;
            }
            z.data[i] += dep;
            deposited += dep * cell_area;
            qs[i] -= dep * cell_area;
            let r = recv[i];
            if r != i {
                qs[r] += qs[i];
            }
        }

        // --- hillslope diffusion ------------------------------------------
        // Explicit, sub-stepped to keep kd*sdt/dx^2 <= 0.2. This is the term
        // that rounds the interfluves: curvature decays fastest where it is
        // largest, and on this surface that is the ridge crests.
        // Effective diffusivity is capped at MAXAMP x Kd so the runaway near
        // S = Sc stays integrable; the cap is what sets the sub-step count.
        const MAXAMP: f64 = 14.0;
        let amp = if p.sc > 0.0 { MAXAMP } else { 1.0 };
        let sub = ((p.kd * amp * p.dt) / (0.2 * dx * dx)).ceil().max(1.0) as usize;
        let sdt = p.dt / sub as f64;
        let mut tmp = z.data.clone();
        // face flux in x then y, so the scheme is conservative
        let mut fx = vec![0.0f64; n];
        let mut fy = vec![0.0f64; n];
        for _ in 0..sub {
            tmp.copy_from_slice(&z.data);
            let kfac = |g: f64| -> f64 {
                if p.sc <= 0.0 {
                    p.kd
                } else {
                    let r = (g / p.sc).abs().min(0.985);
                    (p.kd / (1.0 - r * r)).min(p.kd * MAXAMP)
                }
            };
            for y in 0..ny {
                for x in 0..nx - 1 {
                    let i = spec.index(x as u32, y as u32);
                    let j = spec.index((x + 1) as u32, y as u32);
                    let g = (tmp[j] - tmp[i]) / dx;
                    fx[i] = -kfac(g) * g;
                }
                fx[spec.index((nx - 1) as u32, y as u32)] = 0.0;
            }
            for y in 0..ny - 1 {
                for x in 0..nx {
                    let i = spec.index(x as u32, y as u32);
                    let j = spec.index(x as u32, (y + 1) as u32);
                    let g = (tmp[j] - tmp[i]) / dx;
                    fy[i] = -kfac(g) * g;
                }
            }
            for x in 0..nx {
                fy[spec.index(x as u32, (ny - 1) as u32)] = 0.0;
            }
            for y in 1..ny - 1 {
                for x in 1..nx - 1 {
                    let i = spec.index(x as u32, y as u32);
                    let w = spec.index((x - 1) as u32, y as u32);
                    let s_ = spec.index(x as u32, (y - 1) as u32);
                    let div = (fx[i] - fx[w]) / dx + (fy[i] - fy[s_]) / dx;
                    z.data[i] = tmp[i] - sdt * div;
                }
            }
        }
    }

    let mut ad: Vec<f64> = (0..n).map(|i| (z.data[i] - z0[i]).abs()).collect();
    let mean = ad.iter().sum::<f64>() / n as f64;
    ad.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let dz: Vec<f64> = (0..n).map(|i| z.data[i] - z0[i]).collect();
    Stats {
        mean_abs_dz: mean,
        p99_abs_dz: ad[(n as f64 * 0.99) as usize],
        max_dz: dz.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        min_dz: dz.iter().cloned().fold(f64::INFINITY, f64::min),
        eroded_m3: eroded,
        deposited_m3: deposited,
    }
}
