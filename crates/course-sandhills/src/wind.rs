//! A1 + A2 — the wind field and the crest phase.
//!
//! THE MECHANISM. A dune field is a periodic pattern laid down by a wind that
//! is not perfectly uniform. Model the pattern by a **phase** field `phi`:
//! crests are the level sets `phi = 0 (mod 2*pi)`, and the local wavenumber is
//! `grad phi`, pointing downwind with magnitude `2*pi/lambda`.
//!
//! Everything the archetype needs falls out of that one field:
//!
//!   * crest LINES are contours of `phi mod 2*pi`
//!   * crest SPACING is `lambda`, exactly, by construction
//!   * the dune BODY is a 1-D profile of the phase within its cycle
//!     (surface.rs) -- so there is no N-way envelope to compose at all
//!   * **DEFECTS are free.** If the wind direction has curl, `phi` cannot be
//!     a single-valued potential, and where the accumulated phase mismatches
//!     the pattern the level sets FOLD: a crest terminates, or two merge into
//!     one. That is what a real barchanoid ridge field is -- chains of
//!     coalesced crescents -- and it is the property a wallpaper of parallel
//!     stripes cannot have at any amplitude.
//!
//! So defect density is not decorated on afterwards; it is a consequence of
//! the wind wandering, controlled by one dial (`wind_wander_rad`). The same
//! dial sets the measured orientation order, which is why the record carries
//! a different value per form class: trains are a low-wander, high-A field and
//! mounds are a high-wander, low-A field, and they are the SAME generator.

use course_seed::DetRng;
use course_world::grid::{Grid, GridSpec};
use course_world::math::{self, Vec2};
use course_world::noise;
use course_world::world::EXTENT_M;

/// Macro resolution. 8 m: the signal is km-scale and this is the grid the
/// planform family already declares (01-measurement-policy.md Sec 5).
pub const RES_M: f64 = 8.0;

pub fn macro_spec() -> GridSpec {
    let n = (EXTENT_M / RES_M).round() as u32 + 1;
    GridSpec::new(Vec2::ZERO, RES_M, n, n)
}

/// One sinusoidal mode of the wind-direction perturbation.
///
/// The direction field is built from an EXPLICIT mode sum rather than opaque
/// value noise, because the phase then has a closed form -- see `build`.
#[derive(Clone, Copy)]
struct Mode {
    /// Wavevector, rad/m.
    q: Vec2,
    /// Amplitude, radians of direction swing.
    a: f64,
    /// Phase offset.
    ph: f64,
}

pub struct WindField {
    /// Local wind azimuth, radians. Crests run PERPENDICULAR to this.
    pub dir: Grid<f64>,
    /// Crest phase. Level sets at multiples of `2*pi` are crest lines.
    pub phase: Grid<f64>,
    /// The wavelength the phase was built with, metres.
    pub lambda_m: f64,
    /// Magnitude of the CURL part of the target gradient field, in units of
    /// the base wavenumber. This is the defect budget: at zero the pattern is
    /// a wallpaper and no defect can exist; near and above 1 the level sets
    /// fold into terminations and Y-junctions.
    ///
    /// It is an ABSOLUTE magnitude, not a fraction. The first version returned
    /// `curl / (grad + curl)`, which is scale-invariant — every mode amplitude
    /// cancels — so it read a constant 0.703 across the entire wander ladder
    /// while the actual disorder went from nil to total. A diagnostic that
    /// cannot move is not a diagnostic.
    pub curl_mag: f64,
}

impl WindField {
    /// Phase within the current dune cycle, in `[0, 1)`. 0 is the crest.
    pub fn cycle(&self, x: u32, y: u32) -> f64 {
        let t = self.phase.get(x, y) / std::f64::consts::TAU;
        t - t.floor()
    }
}

/// Build the wind direction field and the crest phase that follows it.
///
/// THE CONSTRUCTION. We want a phase whose gradient tracks the local wind:
/// `grad phi = k * w_hat(p)`. That is a Poisson problem, `lap phi = div(k
/// w_hat)`, and it is NOT generally solvable exactly -- the wind field has
/// curl, and curl is precisely what a single-valued phase cannot represent.
/// The unrepresentable part is not an error to be minimised; it is where
/// crests terminate and merge, which is the whole point.
///
/// Rather than relax a PDE (Jacobi needs thousands of sweeps to move
/// 400-cell features, and there is no FFT in `course-world`), we solve it in
/// CLOSED FORM by making the direction field an explicit mode sum:
///
/// ```text
/// theta(p)  = theta0 + SUM_m a_m sin(q_m . p + ph_m)
/// grad phi ~= k w0 + k w0_perp SUM_m a_m sin(q_m . p + ph_m)      (small a)
/// ```
///
/// For one mode the target is `c sin(q.p + ph)` with `c = a k w0_perp`.
/// Helmholtz splits that into a gradient part along `q_hat` and a curl part
/// across it, and the gradient part integrates exactly:
///
/// ```text
/// psi_m = -(c . q_hat)/|q| * cos(q_m . p + ph_m)
/// ```
///
/// So the phase is the plane wave plus the sum of those, with no iteration and
/// no preferred origin. The discarded curl fraction is returned as
/// `curl_frac`.
///
/// This is what makes `wander_rad` the dial that actually separates the form
/// classes. The previous version built the phase from the GLOBAL paleowind and
/// never read the direction field, which left `wander_rad` inert and forced a
/// separate `disorder` dial to carry the pattern -- and that dial shortened the
/// wavelength as it worked (measured ladder: A 0.898 -> 0.484 while lambda
/// collapsed 1148 m -> 546 m, so orientation order and spacing fought each
/// other). Here they are independent by construction: `wander_rad` sets the
/// disorder, `lambda_m` sets the spacing.
pub fn build(rng: &mut DetRng, wind_rad: f64, lambda_m: f64,
             wander_rad: f64, wander_m: f64) -> WindField {
    let spec = macro_spec();
    let (nx, ny) = (spec.nx, spec.ny);
    let k = std::f64::consts::TAU / lambda_m;
    let w0 = Vec2::new(math::cos(wind_rad), math::sin(wind_rad));
    let w_perp = w0.perp();

    // --- draw the modes ----------------------------------------------------
    // Wavenumbers spread around the correlation length, directions uniform.
    // N_MODES is small enough to stay cheap and large enough that the field
    // does not read as one beat.
    const N_MODES: usize = 8;
    let mut modes = [Mode { q: Vec2::ZERO, a: 0.0, ph: 0.0 }; N_MODES];
    let mut norm = 0.0;
    for m in modes.iter_mut() {
        let dir = rng.range_f64(0.0, std::f64::consts::TAU);
        // 0.6..1.8 of the base wavenumber for the correlation length
        let qmag = (std::f64::consts::TAU / wander_m) * rng.range_f64(0.6, 1.8);
        let a = rng.range_f64(0.45, 1.0);
        *m = Mode {
            q: Vec2::new(math::cos(dir) * qmag, math::sin(dir) * qmag),
            a,
            ph: rng.range_f64(0.0, std::f64::consts::TAU),
        };
        norm += a * a;
    }
    // Normalise so the direction swing has standard deviation `wander_rad`.
    // Sum of independent sinusoids: var = sum(a^2)/2.
    let scale = if norm > 0.0 { wander_rad / (norm / 2.0).sqrt() } else { 0.0 };
    for m in modes.iter_mut() {
        m.a *= scale;
    }

    // How much of the target gradient field is curl -- the defect budget.
    let (mut grad_e, mut curl_e) = (0.0f64, 0.0f64);
    for m in &modes {
        let qm = m.q.length();
        if qm <= 0.0 {
            continue;
        }
        let qh = Vec2::new(m.q.x / qm, m.q.y / qm);
        let along = w_perp.dot(qh); // gradient part
        let across = (1.0 - along * along).max(0.0).sqrt(); // curl part
        grad_e += (m.a * along).powi(2);
        curl_e += (m.a * across).powi(2);
    }
    let curl_mag = curl_e.sqrt();

    // --- evaluate ----------------------------------------------------------
    let mut dir = Grid::filled(spec, 0.0f64);
    let mut phase = Grid::filled(spec, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            let p = spec.world_of(x, y);
            let mut dtheta = 0.0;
            let mut psi = 0.0;
            for m in &modes {
                let arg = m.q.x * p.x + m.q.y * p.y + m.ph;
                dtheta += m.a * math::sin(arg);
                let qm = m.q.length();
                if qm > 0.0 {
                    let qh = Vec2::new(m.q.x / qm, m.q.y / qm);
                    psi += -(m.a * k * w_perp.dot(qh)) / qm * math::cos(arg);
                }
            }
            dir.set(x, y, wind_rad + dtheta);
            phase.set(x, y, k * (p.x * w0.x + p.y * w0.y) + psi);
        }
    }

    WindField { dir, phase, lambda_m, curl_mag }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_seed::RunIdentity;

    fn rng(s: u64) -> DetRng {
        crate::rng::stream(&RunIdentity::from_seed(s), crate::rng::WIND)
    }

    #[test]
    fn deterministic() {
        let a = build(&mut rng(1), 0.7, 1300.0, 0.3, 2400.0);
        let b = build(&mut rng(1), 0.7, 1300.0, 0.3, 2400.0);
        assert_eq!(a.phase.data, b.phase.data);
        assert_eq!(a.dir.data, b.dir.data);
    }

    #[test]
    fn phase_advances_about_one_cycle_per_wavelength() {
        // The measurement that makes crest SPACING true by construction rather
        // than by tuning: walking one wavelength downwind must advance the
        // phase by ~2*pi.
        let f = build(&mut rng(3), 0.0, 1200.0, 0.0, 2000.0);   // no wander
        let spec = macro_spec();
        let steps = (1200.0 / RES_M).round() as u32;
        let y = spec.ny / 2;
        let d = f.phase.get(steps, y) - f.phase.get(0, y);
        assert!((d - std::f64::consts::TAU).abs() < 1e-6, "phase advanced {d}, want TAU");
    }

    #[test]
    fn zero_wander_is_a_wallpaper_and_that_is_the_failure_mode() {
        // Documents the thing the wander exists to prevent. With no wander the
        // phase gradient is exactly constant, so every crest is a straight
        // parallel line and no defect can exist.
        let f = build(&mut rng(5), 0.4, 1300.0, 0.0, 2000.0);
        let spec = macro_spec();
        let mut gx = Vec::new();
        for y in (2..spec.ny - 2).step_by(17) {
            for x in (2..spec.nx - 2).step_by(17) {
                gx.push(f.phase.get(x + 1, y) - f.phase.get(x - 1, y));
            }
        }
        let mean = gx.iter().sum::<f64>() / gx.len() as f64;
        let var = gx.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / gx.len() as f64;
        assert!(var < 1e-18, "zero-wander phase gradient is not constant (var {var:e})");
    }

    #[test]
    fn wander_makes_the_phase_gradient_vary() {
        let flat = build(&mut rng(7), 0.4, 1300.0, 0.0, 2400.0);
        let bent = build(&mut rng(7), 0.4, 1300.0, 0.35, 2400.0);
        let spread = |f: &WindField| {
            let spec = macro_spec();
            let mut v = Vec::new();
            for y in (2..spec.ny - 2).step_by(13) {
                for x in (2..spec.nx - 2).step_by(13) {
                    v.push(f.phase.get(x + 1, y) - f.phase.get(x - 1, y));
                }
            }
            let m = v.iter().sum::<f64>() / v.len() as f64;
            (v.iter().map(|q| (q - m).powi(2)).sum::<f64>() / v.len() as f64).sqrt()
        };
        assert!(spread(&bent) > spread(&flat) + 1e-6,
                "wander did not perturb the phase");
    }

    #[test]
    fn direction_swing_matches_the_drawn_wander() {
        // `wander_rad` is the STANDARD DEVIATION of the local azimuth about
        // the paleowind, not a hard bound -- the field is a sum of sinusoids
        // normalised to that sigma, so a few-sigma excursion is expected and
        // correct. This test pins the contract that the dial means what it
        // says; an earlier version asserted |dev| <= wander_rad and failed
        // the moment the construction became modal.
        for seed in [11u64, 12, 13] {
            let d = crate::draw::site(&RunIdentity::from_seed(seed),
                                      Some(crate::Mode::Aeolian), None);
            let f = build(&mut rng(seed), d.wind_rad, d.wavelength_m,
                          d.wind_wander_rad, d.wind_wander_m);
            let spec = macro_spec();
            let mut v = Vec::new();
            for y in (0..spec.ny).step_by(7) {
                for x in (0..spec.nx).step_by(7) {
                    v.push(f.dir.get(x, y) - d.wind_rad);
                }
            }
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            let sd = (v.iter().map(|q| (q - mean).powi(2)).sum::<f64>() / v.len() as f64).sqrt();
            let want = d.wind_wander_rad;
            assert!((sd - want).abs() < want * 0.35,
                    "seed {seed}: direction sigma {sd:.3} against drawn {want:.3}");
            let max = v.iter().fold(0.0f64, |a, q| a.max(q.abs()));
            assert!(max < want * 4.0, "seed {seed}: direction ran away ({max:.3})");
        }
    }

    #[test]
    fn wander_creates_curl_and_zero_wander_does_not() {
        // curl_frac is the defect budget. At zero wander there is nothing to
        // fold and the pattern is a wallpaper by construction; with wander the
        // target gradient field carries a curl component that no single-valued
        // phase can represent, and that is where crests terminate and merge.
        let flat = build(&mut rng(21), 0.5, 1300.0, 0.0, 2400.0);
        assert_eq!(flat.curl_mag, 0.0, "zero wander produced curl");
        // It must SCALE with the dial -- the property the ratio version lacked.
        let a = build(&mut rng(21), 0.5, 1300.0, 0.30, 2400.0).curl_mag;
        let b = build(&mut rng(21), 0.5, 1300.0, 0.90, 2400.0).curl_mag;
        assert!(b > a * 2.5, "curl_mag did not scale with wander ({a:.3} -> {b:.3})");
    }

    #[test]
    fn wavelength_is_independent_of_wander() {
        // THE property the previous construction did not have. The old dial
        // drove orientation order and spacing in opposite directions (measured
        // ladder: A .898 -> .484 while lambda collapsed 1148 -> 546 m). Here
        // the plane-wave term carries the spacing and the modal correction
        // carries the disorder, so the mean phase gradient along the wind is
        // fixed at k regardless of how much the field wanders.
        let spec = macro_spec();
        let k = std::f64::consts::TAU / 1300.0;
        for w in [0.0, 0.3, 0.8, 1.4] {
            let f = build(&mut rng(23), 0.0, 1300.0, w, 2400.0);
            let mut g = Vec::new();
            for y in (2..spec.ny - 2).step_by(11) {
                for x in (2..spec.nx - 2).step_by(11) {
                    g.push((f.phase.get(x + 1, y) - f.phase.get(x - 1, y)) / (2.0 * RES_M));
                }
            }
            let mean = g.iter().sum::<f64>() / g.len() as f64;
            // 10%: the modal correction has a small nonzero mean over a
            // finite domain (the tile is not a whole number of mode periods),
            // measured at ~4% by wander 0.8. The point of the test is the
            // ORDER of magnitude -- the previous construction collapsed the
            // wavelength by 52% (1148 m -> 546 m) across its dial range.
            assert!((mean - k).abs() < k * 0.10,
                    "wander {w}: mean phase gradient {mean:.6} against k {k:.6}");
        }
    }

    #[test]
    fn cycle_is_a_unit_interval() {
        let f = build(&mut rng(13), 0.9, 1250.0, 0.3, 2200.0);
        let spec = macro_spec();
        for y in (0..spec.ny).step_by(31) {
            for x in (0..spec.nx).step_by(31) {
                let c = f.cycle(x, y);
                assert!((0.0..1.0).contains(&c), "cycle {c} outside [0,1)");
            }
        }
    }
}
