//! A3 + A4 — the interdune datum, and the dune bodies hung off the phase.
//!
//! There is no N-way envelope here, and that is the payoff of the phase
//! formulation. `heartland` and attempt 4 both had to compose overlapping
//! valley frames with a soft-min and fight the seams; a phase field has
//! exactly ONE dune body per cycle by construction, so the surface is
//!
//! ```text
//! z(p) = datum(p) + relief * profile(cycle(p))
//! ```
//!
//! and there is nothing to blend.
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

use course_world::grid::Grid;
use course_world::math;

use crate::draw::Descriptors;
use crate::wind::{macro_spec, WindField, RES_M};

pub struct Surface {
    pub height: Grid<f64>,
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
    if t < lee {
        // Lee face. Near-planar -- a slip face is a plane, not a cosine --
        // with a small ease at the toe so the join to the stoss ramp is C1
        // and does not print a crease along every trough.
        let u = t / lee;
        let planar = 1.0 - u;
        let ease = math::smoothstep(0.82, 1.0, u);
        planar * (1.0 - ease)
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
        let u = (t - lee) / stoss_share.max(1e-6);
        u.powf(1.15)
    }
}

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

pub fn build(w: &WindField, d: &Descriptors) -> Surface {
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
    let mut height = Grid::filled(spec, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            let z = datum.get(x, y) + d.dune_relief_m * profile(w.cycle(x, y), d.stoss_share);
            height.set(x, y, z);
        }
    }

    Surface { height, datum }
}

/// Peak-to-trough relief of the built surface, p95 - p5.
pub fn relief(g: &Grid<f64>) -> f64 {
    let mut v: Vec<f64> = g.data.iter().copied().filter(|z| z.is_finite()).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() - 1) as f64 * p).round() as usize];
    q(0.95) - q(0.05)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{draw, rng, wind, Mode};
    use course_seed::RunIdentity;

    fn built(seed: u64, form: Option<crate::FormClass>) -> (Descriptors, Surface) {
        let id = RunIdentity::from_seed(seed);
        let d = draw::site(&id, Some(Mode::Aeolian), form);
        let mut r = rng::stream(&id, rng::WIND);
        let w = wind::build(&mut r, d.wind_rad, d.wavelength_m, d.wind_wander_rad, d.wind_wander_m);
        let s = build(&w, &d);
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
            assert!(lee < 12.0,
                    "seed {s}: macro lee {lee:.2} deg -- steep sand belongs to \
                     blowouts and texture, not the km-scale profile");
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
