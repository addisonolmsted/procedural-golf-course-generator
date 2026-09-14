//! A3 + A4 — the interdune datum, and the dune bodies hung off the phase.
//!
//! The surface is the MEAN of one dune profile per wave, rescaled to the drawn
//! relief:
//!
//! ```text
//! raw(p) = mean over waves[ profile(cycle_i(p)) ]
//! z(p)   = datum(p) + relief * (raw - raw_p5) / (raw_p95 - raw_p5)
//! ```
//!
//! Still a SUM, never a min/max envelope. `heartland` and attempt 4 both had
//! to compose overlapping valley frames with a soft-min and spent rounds
//! fighting the seams; a sum of smooth periodic fields has no seam to fight.
//! The rescale is what keeps amplitude honest: superposing K profiles shrinks
//! the variance (they are not in phase), so a fixed gain would make a mound
//! field silently flatter than a train. Normalising on the measured p95-p5
//! makes the drawn relief mean what it says for any concentration.
//!
//! THE PROFILE IS ASYMMETRIC, and that asymmetry IS the dune: a gentle
//! windward ramp rising to the crest, then a short steep lee face. A
//! symmetric profile is a wave, not a dune field.
//!
//! ON THE MACRO ANGLES. Golf bounds playable dune relief to 10-35 m
//! (04-landform-literature.md) while the crest spacing is ~1300 m, so the
//! MACRO profile is inherently gentle -- a few degrees, not the 30-plus of a
//! live slip face. That is not a defect and must not be "fixed" by steepening
//! the macro: real Nebraska barchanoid ridges reach those angles because they
//! are 90-150 m tall, which is four times the golf ceiling. The steep sand a
//! player actually sees belongs to the blowouts (A5) and the texture stage,
//! at their own much shorter scale. `derived_angles` reports the macro
//! numbers so a drift is visible.

use course_seed::DetRng;
use course_world::grid::Grid;
use course_world::math::{self, Vec2};

use crate::draw::Descriptors;
use crate::wind::{macro_spec, WindField};

pub struct Surface {
    pub height: Grid<f64>,
    /// The megaform alone, before the hummock tier. Kept so an ablation is one
    /// line rather than a rebuild — discipline rule 3.
    pub belts: Grid<f64>,
    /// Where hummocks were allowed, in `[0, 1]`. Zero on the interdune floors.
    pub hummock_gate: Grid<f64>,
    /// Interdune datum, kept for A5/A6: blowouts deflate toward it and the
    /// water table is measured from it.
    pub datum: Grid<f64>,
}

/// The dune cross-section, as a function of phase within one cycle.
///
/// `t` is in `[0, 1)` with the CREST at 0. Downwind of the crest comes the
/// lee face (a short steep drop), then the stoss ramp (a long gentle rise)
/// back to the next crest. Returns `[0, 1]`, 1 at the crest.
pub fn profile(t: f64, stoss_share: f64) -> f64 {
    let lee = (1.0 - stoss_share).max(1e-6);
    let raw = if t < lee {
        // Lee face. Near-planar -- a slip face is a plane, not a cosine --
        // with a small ease at the toe so the join to the stoss ramp is C1
        // and does not print a crease along every trough.
        let u = t / lee;
        (1.0 - u) * (1.0 - math::smoothstep(0.82, 1.0, u))
    } else {
        // Stoss ramp. Mildly convex -- flat at the toe, steepening toward the
        // crest -- but with a BOUNDED peak slope.
        //
        // The exponent is load-bearing and was found by a failing test. The
        // first version was `u^2(3-2u) * u^0.35`, whose peak slope is ~1.2x
        // its mean; at the low end of the mound share range that made the
        // STOSS steeper than the lee, which is backwards -- a slip face is
        // the steep side of a dune by definition. `u^1.15` caps the peak at
        // 1.15x, so lee/stoss stays above 1 across the whole share range.
        // The exponent is evaluated on a SHIFTED variable. Raw `u^1.15` has
        // second derivative `0.17 * u^-0.85`, which is INFINITE at the toe, so
        // every wave printed a curvature singularity along its own trough line
        // and eight of them wove a visible crosshatch. Rounding the crest did
        // not touch it -- measured, max|d2| stayed at 3082 before and after,
        // which is what identified the toe as the real source. The shift makes
        // the curvature finite while leaving the shape and the bounded peak
        // slope intact (h(1) = 1, max slope 1.08).
        const TOE: f64 = 0.06;
        let u = (t - lee) / stoss_share.max(1e-6);
        let n = (1.0 + TOE).powf(1.15) - TOE.powf(1.15);
        ((u + TOE).powf(1.15) - TOE.powf(1.15)) / n
    };
    // ---- the crest is ROUNDED, and that is a scale argument -------------
    //
    // Raw, the two branches meet at the crest with a slope JUMP of
    // `1.15/share + 1/(1-share)` -- 4.4 to 5.8 across the shipped share
    // range. Each wave then prints that kink along its own crest lines, and
    // eight waves at spread orientations weave them into a visible
    // crosshatch. MEASURED on seed 19 (the worst draw: share 0.77, spread
    // 0.605 rad): 11.3 effective orientation families, creases over 5 cm
    // across 25.6% of the tile, against 2.4-6.8% for a typical draw.
    //
    // A real dune crest IS a slope break -- the brink where the stoss meets
    // the slip face -- but that sharpness belongs to an INDIVIDUAL dune at
    // tens of metres, not to a 1300 m ridge belt, which is a belt OF dunes
    // and has no single brink. So the megaform crest is rounded here and the
    // sharp brinks arrive with the hummock tier, at the scale where they are
    // physically right.
    let d = t.min(1.0 - t);                 // wrapped distance to the crest
    if d >= CREST_ROUND {
        return raw;
    }
    let w = math::smoothstep(0.0, 1.0, d / CREST_ROUND);
    let cap = 1.0 - (d / CREST_ROUND).powi(2) * (1.0 - PEAK_EDGE);
    cap * (1.0 - w) + raw * w
}

/// Shear the surface downwind by an amount proportional to its own height.
///
/// THE ASYMMETRY IS APPLIED ONCE, TO THE COMPOSITE. Giving every wave its own
/// stoss/lee profile gave eight crossing families of slip faces, and they wove
/// a visible crosshatch across the tile. ABLATION (2026-08-22): rebuilding the
/// same seeds with a symmetric profile erased the weave completely and dropped
/// |lap| mean from 0.00337 to 0.00199 -- which is what identified it, after a
/// C1 crest fix and a toe-singularity fix had each been measured and each left
/// the weave untouched.
///
/// A real dune field has ONE wind, so it has ONE family of slip faces. This is
/// what wind actually does: it carries sand up the windward slope and drops it
/// over the brink, which displaces the crest downwind in proportion to how
/// high it stands. Sampling the symmetric field at `p - offset(p) * w_hat`
/// steepens every lee face and lengthens every stoss ramp, at one orientation,
/// for free.
fn advect(z: &Grid<f64>, tnorm: &[f64], wind_rad: f64, offset_m: f64) -> Grid<f64> {
    let spec = z.spec;
    let w = Vec2::new(math::cos(wind_rad), math::sin(wind_rad));
    let mut out = Grid::filled(spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let p = spec.world_of(x, y);
            let t = tnorm[spec.index(x, y)].clamp(0.0, 1.0);
            let src = Vec2::new(p.x - w.x * offset_m * t, p.y - w.y * offset_m * t);
            out.set(x, y, z.bilinear(src));
        }
    }
    out
}

/// Monotone shaping of the normalised megaform (mound round, 2026-08-28).
///
/// The reviewer put our mounds next to the real reference tiles
/// (t04184_10033..t04187_10037): real mound rows are chains of SLENDER
/// bodies -- 300-800 m long, 50-150 m wide -- separated by broad flat
/// meadows, while our rows were continuous sinuous belts ~lambda/2 wide with
/// lowlands as busy as the highs. No wander or kappa dial reaches that: the
/// per-wave profile spans the whole cycle, so body width is locked to the
/// wavelength by construction. Raising the field to a power unlocks it --
/// the high ground narrows laterally, the mid-level contour bands that read
/// as connected "worms" drop into the floor, and the lowlands flatten, which
/// is the same defect the reviewer called out separately.
///
/// Piecewise C1: `t^p` on [0,1] (fixed points 0 and 1, so the p5/p95
/// calibration of `dune_relief_m` survives); linear continuation with slope
/// `p` above 1 (the unclamped crest tails keep climbing, no plateau); slope
/// 0.15 below 0 (sub-datum tails stay slightly expressed rather than
/// printing a hard flat at exactly the datum).
fn shape_body(t: f64, p: f64) -> f64 {
    if p <= 1.0 {
        return t;
    }
    if t < 0.0 {
        0.15 * t
    } else if t <= 1.0 {
        t.powf(p)
    } else {
        // The crest tail SATURATES (2026-09-07). Linear continuation at slope
        // p let the top 5 % of the field run away: on the mixed run a mound
        // stood 65 m over a 25 m drawn relief, 100 m wide, with 200 % sides
        // -- (max - p95)/(p95 - p5) of 1.6 against 0.43 on the real tiles.
        // Same slope p at t = 1 (C1), asymptote 1 + TAIL.
        1.0 + TAIL * (1.0 - math::exp(-p * (t - 1.0) / TAIL))
    }
}

/// How far above the p95 level a crest may climb, as a fraction of the
/// p5-p95 span. Real Nebraska tiles: (max - p95)/(p95 - p5) median 0.43,
/// p90 ~0.8.
const TAIL: f64 = 0.6;

/// Angle of repose (2026-09-07). Sand does not stand steeper than ~32
/// degrees, and the real 8 m macro never exceeds 69 % (p99 39 %) on 24
/// Nebraska tiles, while ours reached 538 %. A fixed number of relaxation
/// passes moves material from the higher cell to the lower wherever a
/// neighbour pair exceeds TALUS, mass-conserving, so a cliff becomes a slip
/// face and the calibrated relief (p5-p95) is barely touched.
const TALUS: f64 = 0.50;
const TALUS_PASSES: usize = 40;

fn talus(g: &mut Grid<f64>) {
    talus_at(g, TALUS);
}

/// The repose pass at a given grade, for other stages (the gorge runs it
/// at 0.60 over its cut: the real canyon wall's 2 m max is 80 %).
pub fn talus_at(g: &mut Grid<f64>, grade: f64) {
    let spec = g.spec;
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let lim = grade * spec.cell_size;
    let lim_d = grade * spec.cell_size * std::f64::consts::SQRT_2;
    for _ in 0..TALUS_PASSES {
        let mut moved = false;
        for y in 0..ny {
            for x in 0..nx {
                let i = y * nx + x;
                for (dx, dy, l) in [(1usize, 0usize, lim), (0, 1, lim), (1, 1, lim_d)] {
                    let (a, b) = (x + dx, y + dy);
                    if a >= nx || b >= ny {
                        continue;
                    }
                    let j = b * nx + a;
                    let d = g.data[i] - g.data[j];
                    if d > l {
                        let e = 0.5 * (d - l);
                        g.data[i] -= e;
                        g.data[j] += e;
                        moved = true;
                    } else if d < -l {
                        let e = 0.5 * (-d - l);
                        g.data[i] += e;
                        g.data[j] -= e;
                        moved = true;
                    }
                }
            }
        }
        if !moved {
            break;
        }
    }
}

/// Half-width of the crest rounding, as a fraction of the dune cycle.
/// Slope jump at the crest falls from ~5.2 to ~0.02 at this value.
const CREST_ROUND: f64 = 0.10;
/// Profile height at the edge of the rounding band, used to blend the cap in
/// without moving the peak.
const PEAK_EDGE: f64 = 0.86;

/// Macro stoss and lee angles in degrees, implied by relief, wavelength and
/// the share split. Reported, not drawn -- see the module doc.
pub fn derived_angles(d: &Descriptors) -> (f64, f64) {
    let lee_run = (1.0 - d.stoss_share) * d.wavelength_m;
    let stoss_run = d.stoss_share * d.wavelength_m;
    (
        math::atan2(d.dune_relief_m, stoss_run).to_degrees(),
        math::atan2(d.dune_relief_m, lee_run).to_degrees(),
    )
}

/// Coherent sand-supply field in `[0, 1]`. Where it is low the belt fades to
/// the interdune floor, which is what makes belts terminate and fray.
fn supply(rng: &mut DetRng, spec: course_world::grid::GridSpec,
          patchiness: f64, corr_m: f64) -> Vec<f64> {
    let s = rng.next_u32();
    let mut v = vec![0.0f64; spec.len()];
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let p = spec.world_of(x, y);
            // Two octaves: the long one carves whole pods, the short one
            // roughens the edge of each.
            let n = 0.72 * course_world::noise::perlin2(p.x / corr_m, p.y / corr_m, s)
                + 0.28 * course_world::noise::perlin2(p.x / (corr_m * 0.38),
                                                      p.y / (corr_m * 0.38), s ^ 0x9E37);
            // Threshold with a soft knee. `patchiness` moves the threshold up,
            // so more of the tile falls below it and more belt is erased.
            // The knee's WIDTH has a floor (2026-09-14): with the upper edge
            // pinned at 0.28 it shrank from 0.45 to 0.11 as patchiness rose
            // 0.24 -> 0.85, so a patchy field also got margins four times
            // sharper -- coverage and abruptness through one expression.
            // That, with `body_p`, made the isolated-mound style: measured
            // against the 39 real mound-like tiles, twice the bodies per
            // km^2, each smaller, on lowlands with half the relief. The
            // floor binds only above patchiness ~0.53, so every Train tile
            // (0.24-0.52) is byte-identical.
            const KNEE_MIN: f64 = 0.29;
            let lo = -0.30 + 0.55 * patchiness;
            let t = math::smoothstep(lo, 0.28f64.max(lo + KNEE_MIN), n);
            v[spec.index(x, y)] = 1.0 - patchiness + patchiness * t;
        }
    }
    v
}

pub fn build(rng: &mut DetRng, w: &WindField, hw: &WindField, d: &Descriptors) -> Surface {
    let spec = macro_spec();
    let (nx, ny) = (spec.nx, spec.ny);

    // --- A3: the interdune datum -------------------------------------------
    // The flat floor the dunes sit on, and the ground a router actually uses.
    // A plane at the drawn regional tilt; deliberately featureless, because
    // every bit of interest in this archetype is supposed to come from the
    // dunes and the blowouts.
    let tilt = d.floor_tilt_m_km / 1000.0;
    let (tc, ts) = (math::cos(d.floor_tilt_rad), math::sin(d.floor_tilt_rad));
    let mut datum = Grid::filled(spec, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            let p = spec.world_of(x, y);
            datum.set(x, y, tilt * (p.x * tc + p.y * ts));
        }
    }

    // --- A4: the dune bodies ------------------------------------------------
    let ww = wave_weights(&w.ks);
    let mut raw = vec![0.0f64; spec.len()];
    for y in 0..ny {
        for x in 0..nx {
            // SYMMETRIC per wave -- the asymmetry is applied once, below.
            let mut a = 0.0;
            for i in 0..w.n_waves() {
                a += ww[i] * profile(w.cycle(i, x, y), 0.5);
            }
            raw[spec.index(x, y)] = a;
        }
    }
    // --- sand supply: belts do not exist everywhere ------------------------
    let sup = supply(rng, spec, d.belt_patchiness, d.belt_patch_m);
    let base = pct(&raw, 0.02, 0.98).0;
    for i in 0..raw.len() {
        raw[i] = base + (raw[i] - base) * sup[i];
    }

    let (lo, hi) = pct(&raw, 0.05, 0.95);
    let span = (hi - lo).max(1e-9);

    let mut belts = Grid::filled(spec, 0.0f64);
    let mut tnorm = vec![0.0f64; spec.len()];
    for y in 0..ny {
        for x in 0..nx {
            // NOT clamped. Normalising on p5/p95 and then clamping flattens
            // the top and bottom 5% of the field into plateaus with hard
            // polygonal edges -- visible as straight-sided flats in the
            // render. The scale still makes p95-p5 equal the drawn relief;
            // the tails simply run past it, which is what a real crest does.
            let t = shape_body((raw[spec.index(x, y)] - lo) / span, d.body_p);
            tnorm[spec.index(x, y)] = t;
            belts.set(x, y, datum.get(x, y) + d.dune_relief_m * t);
        }
    }

    // --- A4b: the hummock tier ---------------------------------------------
    // The same phase machinery at dune scale, GATED to the belt tops. The gate
    // is the whole point: sand piles on the ridges, and the flat interdune
    // floors between them are the ground a course is routed on. Ungated, this
    // tier would fill the valleys and take the archetype's routable ground
    // with it -- the corpus passes the golf proxy on those floors.
    let hww = wave_weights(&hw.ks);
    let mut hraw = vec![0.0f64; spec.len()];
    for y in 0..ny {
        for x in 0..nx {
            let mut a = 0.0;
            for i in 0..hw.n_waves() {
                a += hww[i] * profile(hw.cycle(i, x, y), 0.5);
            }
            hraw[spec.index(x, y)] = a;
        }
    }
    let (hlo, hhi) = pct(&hraw, 0.05, 0.95);
    let hspan = (hhi - hlo).max(1e-9);

    let mut height = Grid::filled(spec, 0.0f64);
    let mut gate = Grid::filled(spec, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            let li = spec.index(x, y);
            // The gate never reaches zero. Real interdune floors carry
            // roughly half the belt's texture (measured 0.48x/0.53x/0.75x
            // across the 30-64, 64-150 and 150-400 m bands over 24 tiles);
            // a hard gate made ours 0.44/0.40/0.53 and left the floors at
            // 0.895 calm against a real 0.680.
            let g = d.hummock_floor
                + (1.0 - d.hummock_floor)
                    * math::smoothstep(d.hummock_gate, d.hummock_gate + 0.30, tnorm[li]);
            let h = (hraw[li] - hlo) / hspan - 0.5;
            gate.set(x, y, g);
            height.set(x, y, belts.get(x, y) + d.hummock_relief_m * g * h);
        }
    }

    // --- the single asymmetry pass ----------------------------------------
    // Offset is what the share asks for: a crest displaced downwind by
    // (share - 0.5) of its own wavelength has that share's stoss/lee split.
    // The belts and the hummocks are advected by their OWN wavelengths, since
    // a hummock rides on the belt and is shaped by the same wind at its own
    // scale.
    let belt_off = (d.stoss_share - 0.5) * d.wavelength_m;
    let hum_off = (d.stoss_share - 0.5) * d.hummock_lambda_m;
    let belts_a = advect(&belts, &tnorm, d.wind_rad, belt_off);
    let mut hum = Grid::filled(spec, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            hum.set(x, y, height.get(x, y) - belts.get(x, y));
        }
    }
    let hnorm: Vec<f64> = (0..spec.len()).map(|i| hraw[i]).collect();
    let hum_a = advect(&hum, &hnorm, d.wind_rad, hum_off);
    let mut height2 = Grid::filled(spec, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            height2.set(x, y, belts_a.get(x, y) + hum_a.get(x, y));
        }
    }

    // The gate is advected with the belts it keys on -- otherwise the exported
    // gate describes the PRE-shear surface and the two disagree by the offset.
    let gate_a = advect(&gate, &tnorm, d.wind_rad, belt_off);
    talus(&mut height2);

    Surface { height: height2, datum, belts: belts_a, hummock_gate: gate_a }
}

/// Per-wave amplitude weights, proportional to wavelength.
///
/// BIGGER DUNES ARE TALLER. With equal amplitudes the short waves dominate
/// visually, because a hillshade reads SLOPE and slope goes as amplitude times
/// wavenumber: at a 0.35x wavelength a wave carries 2.9x the slope for the same
/// height. Once `lambda_spread` gave the waves a 4.7x range of sizes, the
/// shortest ones printed a regular corduroy over everything. Weighting by
/// wavelength makes every wave contribute comparable SLOPE instead, which is
/// both what the render needs and what dune geometry says.
fn wave_weights(ks: &[f64]) -> Vec<f64> {
    let w: Vec<f64> = ks.iter().map(|k| 1.0 / k.max(1e-9)).collect();
    let s: f64 = w.iter().sum();
    w.iter().map(|v| v / s.max(1e-9)).collect()
}

fn pct(v: &[f64], a: f64, b: f64) -> (f64, f64) {
    let mut s: Vec<f64> = v.iter().copied().filter(|z| z.is_finite()).collect();
    s.sort_by(|p, q| p.partial_cmp(q).unwrap());
    let q = |f: f64| s[((s.len() - 1) as f64 * f).round() as usize];
    (q(a), q(b))
}

/// Peak-to-trough relief of the built surface, p95 - p5.
/// The hummock phase field alone, normalised to ±0.5 — the C4 relict
/// mantle reuses the aeolian hummock machinery on the fluvial interfluves
/// (the physical cohesion between the two modes). No advection: a relict,
/// degraded mantle reads symmetric.
pub fn hummock_field(hw: &WindField) -> Grid<f64> {
    let spec = macro_spec();
    let ww = wave_weights(&hw.ks);
    let mut raw = vec![0.0f64; spec.len()];
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let mut a = 0.0;
            for i in 0..hw.n_waves() {
                a += ww[i] * profile(hw.cycle(i, x, y), 0.5);
            }
            raw[spec.index(x, y)] = a;
        }
    }
    let (lo, hi) = pct(&raw, 0.05, 0.95);
    let span = (hi - lo).max(1e-9);
    let mut g = Grid::filled(spec, 0.0f64);
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            g.set(x, y, (raw[spec.index(x, y)] - lo) / span - 0.5);
        }
    }
    g
}

pub fn relief(g: &Grid<f64>) -> f64 {
    let mut v: Vec<f64> = g.data.iter().copied().filter(|z| z.is_finite()).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    q(0.95) - q(0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wind::RES_M;
    use crate::{draw, rng, wind, Mode};
    use course_seed::RunIdentity;

    fn built(seed: u64, form: Option<crate::FormClass>) -> (Descriptors, Surface) {
        let id = RunIdentity::from_seed(seed);
        let d = draw::site(&id, Some(Mode::Aeolian), form);
        let mut r = rng::stream(&id, rng::WIND);
        let w = wind::build(&mut r, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                            d.wind_wander_m, d.kappa, 0.0);
        let mut hr = rng::stream(&id, rng::HUMMOCK);
        let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m,
                             d.wind_wander_rad, d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
        let mut br = rng::stream(&id, rng::PATCHY);
        let s = build(&mut br, &w, &hw, &d);
        (d, s)
    }

    #[test]
    fn profile_spans_the_unit_interval_and_peaks_at_the_crest() {
        for share in [0.58, 0.66, 0.70, 0.80] {
            let mut lo = f64::MAX;
            let mut hi = f64::MIN;
            for i in 0..2000 {
                let v = profile(i as f64 / 2000.0, share);
                assert!(v.is_finite());
                lo = lo.min(v);
                hi = hi.max(v);
            }
            assert!(lo >= -1e-9 && hi <= 1.0 + 1e-9, "profile left [0,1]: {lo}..{hi}");
            assert!(profile(0.0, share) > 0.99, "crest is not at phase 0");
        }
    }

    #[test]
    fn the_profile_is_asymmetric_and_the_lee_is_the_steep_side() {
        // The test that would fail if someone replaced the profile with a
        // cosine. A symmetric dune is a wave.
        for share in [0.58, 0.66, 0.70, 0.80] {
            let n = 4000;
            let d: Vec<f64> = (0..n).map(|i| profile((i + 1) as f64 / n as f64, share)
                                          - profile(i as f64 / n as f64, share)).collect();
            let steepest_fall = d.iter().cloned().fold(f64::MAX, f64::min).abs();
            let steepest_rise = d.iter().cloned().fold(f64::MIN, f64::max);
            assert!(steepest_fall > steepest_rise * 1.15,
                    "share {share}: lee {steepest_fall:.5} is not clearly steeper \
                     than stoss {steepest_rise:.5}");
        }
    }

    #[test]
    fn the_crest_is_c1() {
        // The kink at the crest wove a visible crosshatch across the tile:
        // each of the eight waves printed its own crest line, and on the worst
        // draw (seed 19, share 0.77, spread 0.605 rad) creases over 5 cm
        // covered 25.6% of the tile against 2.4-6.8% typical. A megaform ridge
        // belt is a belt OF dunes and has no single brink; the sharp break
        // belongs to the hummock tier.
        for share in [0.58, 0.66, 0.72, 0.80] {
            let n = 20000;
            let h = 1.0 / n as f64;
            let at = |t: f64| profile(t.rem_euclid(1.0), share);
            let slope = |t: f64| (at(t + h) - at(t - h)) / (2.0 * h);
            let jump = (slope(1.0 - 3.0 * h) - slope(3.0 * h)).abs();
            assert!(jump < 0.6,
                    "share {share}: slope jump at the crest is {jump:.3} -- raw is ~5");
            assert!(at(0.0) > 0.985, "share {share}: rounding moved the peak");
        }
    }

    #[test]
    fn hummocks_stay_off_the_interdune_floors() {
        // THE golf-critical invariant. The archetype's routable ground is the
        // flat floor between ridge belts -- real Nebraska passes the proxy on
        // those floors and nowhere else. A hummock tier that filled them would
        // take the playable ground with it.
        for s in 0..12 {
            let (_, sf) = built(s, None);
            let spec = sf.height.spec;
            let mut lowest = Vec::new();
            for y in 0..spec.ny {
                for x in 0..spec.nx {
                    // DATUM-FREE dune height. Ranking on `belts` alone would
                    // rank the tile's downhill CORNER as floor, because that
                    // grid carries the regional tilt -- and the gate keys on
                    // dune form, not on absolute elevation. (Measured: the
                    // first version of this test failed at gate 0.66 for
                    // exactly that reason.)
                    lowest.push((*sf.belts.get(x, y) - *sf.datum.get(x, y),
                                 *sf.hummock_gate.get(x, y)));
                }
            }
            lowest.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
            // over the lowest fifth of the belt surface, the gate must be shut
            let n = lowest.len() / 5;
            let mut worst = 0.0f64;
            for i in 0..n {
                worst = worst.max(lowest[i].1);
            }
            // The floors are QUIETER than the belts, not empty -- see
            // record.rs::hummock_floor. What must hold is that the tier is
            // clearly weaker down there, not that it is absent.
            let (d, _) = built(s, None);
            assert!(worst < d.hummock_floor + 0.30,
                    "seed {s}: hummock gate reached {worst:.2} on the interdune floor \
                     (floor dial {:.2})", d.hummock_floor);
        }
    }

    #[test]
    fn the_hummock_tier_adds_the_band_it_was_built_for() {
        // Ablation as a test: the tier must move 64-400 m energy and must not
        // be doing its work at megaform scale.
        for s in [3u64, 7, 19] {
            let id = course_seed::RunIdentity::from_seed(s);
            let d = draw::site(&id, Some(Mode::Aeolian), Some(crate::FormClass::Train));
            let mut r = rng::stream(&id, rng::WIND);
            let f = wind::build(&mut r, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                                d.wind_wander_m, d.kappa, 0.0);
            let mut hr = rng::stream(&id, rng::HUMMOCK);
            let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                                 d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
            let mut off = d;
            off.hummock_relief_m = 0.0;
            let mut br = rng::stream(&id, rng::PATCHY);
            let a = build(&mut br, &f, &hw, &off);
            let mut br2 = rng::stream(&id, rng::PATCHY);
            let b = build(&mut br2, &f, &hw, &d);
            let rough = |g: &Grid<f64>| {
                // short-scale energy: rms of the 3x3 Laplacian
                let sp = g.spec;
                let mut acc = 0.0;
                let mut n = 0.0;
                for y in 1..sp.ny - 1 {
                    for x in 1..sp.nx - 1 {
                        let l = g.get(x + 1, y) + g.get(x - 1, y) + g.get(x, y + 1)
                            + g.get(x, y - 1) - 4.0 * g.get(x, y);
                        acc += l * l;
                        n += 1.0;
                    }
                }
                (acc / n).sqrt()
            };
            let ratio = rough(&b.height) / rough(&a.height);
            // 1.5 -> 1.3 (2026-09-13): the hummock relief was pulled x0.7 to
            // the real dune courses and seed 19 measures 1.47; the ablation
            // is about the band existing, not its size
            assert!(ratio > 1.3,
                    "seed {s}: the hummock tier added no short-scale energy (ratio {ratio:.2})");
            assert!(relief(&b.height) < relief(&a.height) * 1.9,
                    "seed {s}: the hummock tier is doing megaform work");
        }
    }

    #[test]
    fn deterministic() {
        let (_, a) = built(4, None);
        let (_, b) = built(4, None);
        assert_eq!(a.height.data, b.height.data);
    }

    #[test]
    fn relief_lands_near_the_drawn_dune_relief() {
        for s in 0..24 {
            let (d, sf) = built(s, None);
            let r = relief(&sf.height);
            // The datum tilt adds a little; the p95-p5 clip removes a little.
            assert!(r > d.dune_relief_m * 0.55 && r < d.dune_relief_m * 1.9,
                    "seed {s}: relief {r:.1} against drawn {:.1}", d.dune_relief_m);
        }
    }

    #[test]
    fn macro_angles_are_gentle_because_golf_bounds_the_relief() {
        // Pins the reasoning in the module doc. If a future edit makes the
        // MACRO profile steep, either the relief broke its golf ceiling or the
        // wavelength collapsed -- both worth failing on.
        for s in 0..24 {
            let (d, _) = built(s, None);
            let (stoss, lee) = derived_angles(&d);
            assert!(stoss < lee, "seed {s}: stoss {stoss:.2} not gentler than lee {lee:.2}");
            // MEASURED 2026-08-27 at macro scale on the real tiles: the
            // Nebraska corpus runs p99 6.5 deg / max 8.5 deg, but tiles
            // centred on real Sandhills COURSES reach p99 12.6 / max 17.2.
            // The old 12.0 was a guess that excluded exactly the dramatic
            // ground Sand Hills and Dismal River are built on. 19 deg leaves
            // headroom over the measured course-site maximum without
            // admitting slip faces, which belong to blowouts and texture.
            assert!(lee < 19.0,
                    "seed {s}: macro lee {lee:.2} deg -- past the measured \
                     course-site maximum; slip faces belong to blowouts and \
                     texture, not the km-scale profile");
        }
    }

    #[test]
    fn every_seed_clears_the_golf_relief_floor() {
        // proxy_thresholds.json relief band floor = 7.033 m.
        for s in 0..48 {
            let (_, sf) = built(s, None);
            assert!(relief(&sf.height) >= 7.033, "seed {s} is under the proxy relief floor");
        }
    }

    #[test]
    fn the_surface_is_finite_everywhere() {
        let (_, sf) = built(9, None);
        assert!(sf.height.data.iter().all(|z| z.is_finite()));
    }

    #[test]
    fn resolution_is_the_declared_macro_grid() {
        assert_eq!(RES_M, 8.0);
        let (_, sf) = built(1, None);
        assert_eq!(sf.height.spec.nx, 376);
    }
}
