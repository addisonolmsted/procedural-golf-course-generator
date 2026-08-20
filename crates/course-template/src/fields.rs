//! Step 3 — the macro steering FIELDS.
//!
//! These are fields, not terrain. No heightfield exists until step 6. Their
//! job is to steer network growth (step 4) and to place risers in the
//! hillslope profile (step 6) — one field with two consequences, which is why
//! a bench and the bend that runs along it cannot disagree.

use course_seed::DetRng;
use course_world::grid::{Grid, GridSpec};
use course_world::math::{self, Vec2};
use course_world::noise;

/// Field resolution. These carry wavelengths >= 400 m, so 16 m is ample and
/// keeps step 3 far inside its budget.
pub const FIELD_RES_M: f64 = 16.0;

/// Band-limited value noise: a fixed sum of octaves with the shortest
/// wavelength declared, so nothing sub-band leaks into a steering field.
fn band_noise(p: Vec2, min_wave_m: f64, octaves: u32, seed: u32) -> f64 {
    let mut v = 0.0;
    let mut amp = 1.0;
    let mut norm = 0.0;
    let mut wave = min_wave_m * (1u32 << (octaves - 1)) as f64;
    for o in 0..octaves {
        v += amp * noise::perlin2(p.x / wave, p.y / wave, seed.wrapping_add(o * 7919));
        norm += amp;
        amp *= 0.55;
        wave *= 0.5;
    }
    v / norm
}

/// One bed in the rock column.
#[derive(Clone, Copy, Debug)]
pub struct Layer {
    pub thickness_m: f64,
    /// Resistance to erosion, [0,1].
    pub hardness: f64,
}

/// The rock column and its attitude.
///
/// **Resistance is a property of the COLUMN, not of the map plane.** A bed
/// outcrops where the land surface crosses it, so on a map its trace follows a
/// contour — which is exactly why a dissected plateau reads as a staircase and
/// why hill country's benches run along slopes rather than across them.
/// Generating resistance directly as an (x, y) field produces a barcode: the
/// first render of this stage did, and that is what the visualiser caught.
///
/// The consequence for a network-first pipeline is that the true map pattern
/// cannot exist until step 6, when elevations do. Step 3 emits the column;
/// step 4 steers on a PROVISIONAL evaluation (see `resistance_hint`).
#[derive(Clone, Debug)]
pub struct Strata {
    /// Bottom-up, repeated cyclically above and below.
    pub layers: Vec<Layer>,
    /// Azimuth the beds dip toward, radians.
    pub dip_rad: f64,
    /// Dip as rise/run. Small: these are near-horizontal beds.
    pub dip_grade: f64,
    /// Stratigraphic datum at the tile centre, metres.
    pub datum_m: f64,
}

impl Strata {
    pub fn total_thickness_m(&self) -> f64 {
        self.layers.iter().map(|l| l.thickness_m).sum()
    }

    /// Height above the local stratigraphic datum for a point at elevation `z`.
    pub fn strat_height(&self, w: Vec2, z: f64) -> f64 {
        let half = course_world::world::EXTENT_M * 0.5;
        let (dc, ds) = (math::cos(self.dip_rad), math::sin(self.dip_rad));
        z - (self.datum_m + self.dip_grade * ((w.x - half) * dc + (w.y - half) * ds))
    }

    /// Hardness of the bed outcropping at `(w, z)`.
    pub fn hardness_at(&self, w: Vec2, z: f64) -> f64 {
        let total = self.total_thickness_m();
        if self.layers.is_empty() || total <= 0.0 {
            return 0.5;
        }
        let mut u = self.strat_height(w, z) % total;
        if u < 0.0 {
            u += total;
        }
        let mut acc = 0.0;
        for l in &self.layers {
            acc += l.thickness_m;
            if u < acc {
                return l.hardness;
            }
        }
        self.layers[self.layers.len() - 1].hardness
    }
}

/// The steering fields step 4 and step 6 both read.
pub struct MacroFields {
    /// Local structural axis, radians. AXIAL — theta and theta+pi mean the
    /// same thing; consumers must treat it that way.
    pub grain_rad: Grid<f64>,
    /// The rock column. The real source of resistance.
    pub strata: Strata,
    /// PROVISIONAL map-plane resistance, [0,1]: `strata` evaluated at a
    /// proxy elevation derived from `relief_pred`. Step 4 steers on this
    /// because it has nothing better; step 6 recomputes from real elevations
    /// and THAT is what places risers. Do not treat this as truth.
    pub resistance_hint: Grid<f64>,
    /// Where the tile tends high or low, [-1,1]. Dimensionless: it steers,
    /// it does not set elevation.
    pub relief_pred: Grid<f64>,
    /// Mean grain azimuth.
    pub grain_axis_rad: f64,
}

pub struct FieldParams {
    /// From the draw: how strongly the grain biases things.
    pub anisotropy: f64,
    /// From the draw: how strongly resistance bands act.
    pub resistance_response: f64,
    /// Riser height at a contact, metres — sets bed thickness.
    pub riser_m: f64,
    /// Relief the tile will be built to, for the provisional elevation proxy.
    pub relief_budget_m: f64,
}

pub fn build(rng: &mut DetRng, p: &FieldParams) -> MacroFields {
    let n = (course_world::world::EXTENT_M / FIELD_RES_M).round() as u32 + 1;
    let spec = GridSpec::new(Vec2::new(0.0, 0.0), FIELD_RES_M, n, n);

    let grain_axis_rad = rng.range_f64(0.0, core::f64::consts::PI);
    let s_grain = rng.next_u32();
    let s_rel = rng.next_u32();

    // --- the rock column. Bed thickness comes from the riser: a tall riser is
    // a thick resistant bed, and a thick stack steps less often. A tile of
    // relief R crossing beds of thickness T shows about R/T contacts, so the
    // thickness is what sets how many benches a slope has.
    let dip_rad = rng.range_f64(0.0, core::f64::consts::TAU);
    // Near-horizontal: 0.3-2.0 %. Steeper than this and the beds stop
    // outcropping as contours and start behaving like a tilted block.
    let dip_grade = rng.range_f64(0.003, 0.020);
    let layers = if p.riser_m <= 0.01 {
        Vec::new()
    } else {
        let n_beds = 3 + rng.below(3);
        (0..n_beds)
            .map(|i| {
                let hard = i % 2 == 0;
                Layer {
                    // resistant beds are the thin ones that hold the treads
                    thickness_m: if hard {
                        p.riser_m * rng.range_f64(0.9, 1.6)
                    } else {
                        p.riser_m * rng.range_f64(2.2, 4.5)
                    },
                    hardness: if hard {
                        (0.55 + 0.45 * p.resistance_response).clamp(0.0, 1.0)
                    } else {
                        (0.45 - 0.35 * p.resistance_response).clamp(0.0, 1.0)
                    },
                }
            })
            .collect()
    };
    let datum_m = rng.range_f64(0.0, p.relief_budget_m.max(1.0));
    let strata = Strata { layers, dip_rad, dip_grade, datum_m };

    let mut grain = Grid::filled(spec, 0.0);
    let mut relief = Grid::filled(spec, 0.0);
    let mut hint = Grid::filled(spec, 0.5);

    for y in 0..n {
        for x in 0..n {
            let w = spec.world_of(x, y);

            // --- grain: a mean axis that wanders. Anisotropy sets how much
            // the wander is suppressed, so a high-anisotropy tile has a
            // coherent axis and a low one is effectively unstructured.
            let wob = band_noise(w, 900.0, 3, s_grain);
            let spread = (1.0 - p.anisotropy).clamp(0.0, 1.0) * core::f64::consts::FRAC_PI_2;
            grain.set(x, y, grain_axis_rad + wob * spread);

            // --- relief predisposition: >= 400 m only. Noise used where
            // noise is honest -- the long-wavelength lottery of where a site
            // happens to sit high or low.
            relief.set(x, y, band_noise(w, 420.0, 3, s_rel));
        }
    }

    // A summed-octave field occupies only a fraction of [-1,1] -- measured
    // rms 0.21, range -0.52..0.62 before this. A field DOCUMENTED as [-1,1]
    // that uses a fifth of it silently mis-scales every consumer, so rescale
    // to the declared range. Symmetric about zero: "tends high" and "tends
    // low" must stay balanced, which a min/max stretch would not preserve.
    {
        let peak = relief.data.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if peak > 1e-9 {
            let k = 1.0 / peak;
            for v in relief.data.iter_mut() {
                *v = (*v * k).clamp(-1.0, 1.0);
            }
        }
    }

    // --- provisional map-plane resistance, from a proxy elevation. This is
    // the ONLY honest thing step 3 can say about where beds outcrop, and it
    // is already enough for the contacts to follow the relief contours rather
    // than stripe the map.
    if !strata.layers.is_empty() {
        for y in 0..n {
            for x in 0..n {
                let w = spec.world_of(x, y);
                let z = *relief.get(x, y) * p.relief_budget_m * 0.5;
                hint.set(x, y, strata.hardness_at(w, z));
            }
        }
    }

    MacroFields { grain_rad: grain, strata, resistance_hint: hint, relief_pred: relief, grain_axis_rad }
}

/// PROVISIONAL escarpment traces: the contacts as they outcrop on the proxy
/// elevation, so step 4 has lines to deflect along. The REAL traces emerge at
/// step 6 from real elevations — a bed's outcrop follows a contour, and step 3
/// has no contours. Do not render these as if they were the final scarps.
///
/// Marching squares on the 0.5 iso-level, then chained by endpoint matching.
/// An earlier version scanned ALONG the strike direction looking for sign
/// changes, which is where the field is constant by construction -- it found
/// almost nothing. Marching squares is indifferent to how the bands are
/// oriented or warped, which is the property worth having.
pub fn scarp_traces(f: &MacroFields, min_len_m: f64) -> Vec<Vec<Vec2>> {
    let spec = f.resistance_hint.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mid = 0.5 * (f.strata.layers.iter().map(|l| l.hardness).fold(0.0f64, f64::max)
        + f.strata.layers.iter().map(|l| l.hardness).fold(1.0f64, f64::min));
    let at = |x: usize, y: usize| *f.resistance_hint.get(x as u32, y as u32) - mid;

    // One segment per cell (the ambiguous saddle case is split arbitrarily;
    // for a steering line that is immaterial).
    let mut segs: Vec<(Vec2, Vec2)> = Vec::new();
    for y in 0..ny.saturating_sub(1) {
        for x in 0..nx.saturating_sub(1) {
            let v = [at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1)];
            let c = [spec.world_of(x as u32, y as u32),
                     spec.world_of(x as u32 + 1, y as u32),
                     spec.world_of(x as u32 + 1, y as u32 + 1),
                     spec.world_of(x as u32, y as u32 + 1)];
            let mut hits: Vec<Vec2> = Vec::new();
            for e in 0..4 {
                let (a, b) = (e, (e + 1) % 4);
                if v[a].signum() != v[b].signum() {
                    let t = v[a] / (v[a] - v[b]);
                    hits.push(c[a].lerp(c[b], t.clamp(0.0, 1.0)));
                }
            }
            if hits.len() >= 2 {
                segs.push((hits[0], hits[1]));
                if hits.len() == 4 {
                    segs.push((hits[2], hits[3]));
                }
            }
        }
    }

    // Chain: greedily extend a polyline by any unused segment whose endpoint
    // is within half a cell of the current tail.
    //
    // Bucketed by endpoint cell. The obvious all-pairs scan is O(n^2), and it
    // was measured at 555 ms/tile on piedmont -- whose thin beds make the MOST
    // contacts -- against 14 ms on hill country. A stage that gets slower the
    // less structure it has is a bug, not a budget problem.
    let tol = FIELD_RES_M * 0.51;
    let bs = FIELD_RES_M.max(tol * 2.0);
    let key = |v: Vec2| ((v.x / bs).floor() as i64, (v.y / bs).floor() as i64);
    let mut buckets: std::collections::HashMap<(i64, i64), Vec<usize>> =
        std::collections::HashMap::new();
    for (i, sg) in segs.iter().enumerate() {
        buckets.entry(key(sg.0)).or_default().push(i);
        buckets.entry(key(sg.1)).or_default().push(i);
    }

    let mut used = vec![false; segs.len()];
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    for i in 0..segs.len() {
        if used[i] { continue; }
        used[i] = true;
        let mut line = vec![segs[i].0, segs[i].1];
        loop {
            let tail = *line.last().unwrap();
            let (kx, ky) = key(tail);
            let mut best: Option<(usize, bool, f64)> = None;
            for oy in -1..=1 {
                for ox in -1..=1 {
                    let Some(ids) = buckets.get(&(kx + ox, ky + oy)) else { continue };
                    for &j in ids {
                        if used[j] { continue; }
                        let (d0, d1) = (tail.distance(segs[j].0), tail.distance(segs[j].1));
                        let (d, flip) = if d0 <= d1 { (d0, false) } else { (d1, true) };
                        if d <= tol && best.is_none_or(|(_, _, bd)| d < bd) {
                            best = Some((j, flip, d));
                        }
                    }
                }
            }
            match best {
                Some((j, flip, _)) => {
                    used[j] = true;
                    line.push(if flip { segs[j].0 } else { segs[j].1 });
                }
                None => break,
            }
        }
        if arc_len(&line) >= min_len_m {
            out.push(line);
        }
    }
    out
}

fn arc_len(p: &[Vec2]) -> f64 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}
