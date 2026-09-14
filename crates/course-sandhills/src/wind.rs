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
    /// One crest phase per wave. Level sets at multiples of `2*pi` are that
    /// wave's crest lines; the dune field is their superposition.
    ///
    /// At high concentration every wave points the same way and this collapses
    /// to the single-axis field that came before -- a train. At low
    /// concentration the directions spread and the dominant axis dissolves,
    /// which is the only way to reach the mound class: however much ONE plane
    /// wave wobbles it still has one axis, and orientation order measures
    /// exactly that axis. Measured cap for the single-wave field was A 0.622
    /// against a 0.356 mound target (02-dune-targets.md Sec 8).
    pub waves: Vec<Grid<f64>>,
    /// The wavelength every wave was built with, metres. Spacing is set here
    /// and nowhere else, so it does not move when the concentration does.
    pub lambda_m: f64,
    /// Circular standard deviation of the drawn wave directions, radians —
    /// what the concentration actually achieved for this seed.
    pub spread_rad: f64,
    /// The drawn wave directions (unit). Each wave carries `|k| = TAU/lambda`
    /// along its own axis.
    pub dirs: Vec<Vec2>,
    /// Per-wave wavenumber — spread, not shared. See `lambda_spread`.
    pub ks: Vec<f64>,
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
    /// Phase within wave `i`'s dune cycle, in `[0, 1)`. 0 is the crest.
    pub fn cycle(&self, i: usize, x: u32, y: u32) -> f64 {
        let t = self.waves[i].get(x, y) / std::f64::consts::TAU;
        t - t.floor()
    }
    pub fn n_waves(&self) -> usize {
        self.waves.len()
    }
}

/// How many waves the superposition carries.
///
/// Fixed rather than drawn: the CONCENTRATION is the dial, and letting the
/// count vary too would give two knobs for one effect.
///
/// EIGHT, and the number is not arbitrary. K unit vectors with uniform
/// directions have an expected resultant of `1/sqrt(K)`, so **the isotropic
/// floor of the measured orientation order is 1/sqrt(K)** — no concentration,
/// however low, gets below it. The mound target is 0.356 and `1/sqrt(8)` =
/// 0.354. Measured at K=6 the ladder bottomed out at 0.397 against a predicted
/// `1/sqrt(6)` = 0.408, which is how the relationship was identified.
pub const N_WAVES: usize = 8;

/// Draw one angle from a von Mises distribution — Best & Fisher (1979).
///
/// Rejection sampler, exact, no special functions. `kappa` 0 is uniform on the
/// circle; large `kappa` concentrates about `mu` with standard deviation
/// roughly `1/sqrt(kappa)`.
fn von_mises(rng: &mut DetRng, mu: f64, kappa: f64) -> f64 {
    if kappa < 1e-6 {
        return rng.range_f64(0.0, std::f64::consts::TAU);
    }
    let a = 1.0 + (1.0 + 4.0 * kappa * kappa).sqrt();
    let b = (a - (2.0 * a).sqrt()) / (2.0 * kappa);
    let r = (1.0 + b * b) / (2.0 * b);
    for _ in 0..128 {
        let u1 = rng.next_f64();
        let u2 = rng.next_f64();
        let z = math::cos(std::f64::consts::PI * u1);
        let f = (1.0 + r * z) / (r + z);
        let c = kappa * (r - f);
        if c * (2.0 - c) - u2 > 0.0 || (c / u2).max(1e-300).ln() + 1.0 - c >= 0.0 {
            let sign = if rng.next_f64() < 0.5 { -1.0 } else { 1.0 };
            return mu + sign * f.clamp(-1.0, 1.0).acos();
        }
    }
    mu // the sampler is exact; this is only reachable on a pathological kappa
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
             wander_rad: f64, wander_m: f64, kappa: f64,
             lambda_spread: f64) -> WindField {
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
    let mut curl_e = 0.0f64;
    for m in &modes {
        let qm = m.q.length();
        if qm <= 0.0 {
            continue;
        }
        let qh = Vec2::new(m.q.x / qm, m.q.y / qm);
        let along = w_perp.dot(qh); // gradient part
        let across = (1.0 - along * along).max(0.0).sqrt(); // curl part
        curl_e += (m.a * across).powi(2);
    }
    let curl_mag = curl_e.sqrt();

    // --- draw the wave directions -----------------------------------------
    // All at |k| = TAU/lambda, so the concentration cannot move the spacing.
    // One SHARED phase offset rather than one per wave: independent offsets
    // would stop the waves aligning at high concentration, and the train limit
    // (every wave parallel, superposition = one wave) is the property that
    // makes this one construction instead of two.
    let phi0 = rng.range_f64(0.0, std::f64::consts::TAU);
    let mut waves_dir = [Vec2::ZERO; N_WAVES];
    let mut waves_k = [0.0f64; N_WAVES];
    let mut sum = Vec2::ZERO;
    for i in 0..N_WAVES {
        let th = von_mises(rng, wind_rad, kappa);
        waves_dir[i] = Vec2::new(math::cos(th), math::sin(th));
        // WAVELENGTH SPREAD. Eight waves sharing one |k| is the recipe for
        // coherent interference -- a fixed beat pattern, which on the belts is
        // masked by the megaform relief and on the flat interdune floors is
        // naked, and reads as a crosshatch. Real dune fields carry a RANGE of
        // dune sizes, so spreading |k| both kills the beat and gives the size
        // variety a single wavelength cannot.
        let j = if N_WAVES > 1 { i as f64 / (N_WAVES - 1) as f64 } else { 0.5 };
        let f = 1.0 + lambda_spread * (2.0 * j - 1.0) * rng.range_f64(0.55, 1.0);
        waves_k[i] = k / f.max(0.25);
        sum = sum + waves_dir[i];
    }
    // Circular standard deviation of what was actually drawn.
    let rbar = (sum.length() / N_WAVES as f64).clamp(1e-9, 1.0);
    let spread_rad = (-2.0 * rbar.ln()).sqrt();

    // --- evaluate ----------------------------------------------------------
    // Per cell the mode arguments are shared across waves, so each wave costs
    // only a handful of multiply-adds on top of the one trig pass.
    let mut dir = Grid::filled(spec, 0.0f64);
    let mut waves: Vec<Grid<f64>> = (0..N_WAVES).map(|_| Grid::filled(spec, 0.0f64)).collect();
    let mut cosarg = [0.0f64; N_MODES];
    for y in 0..ny {
        for x in 0..nx {
            let p = spec.world_of(x, y);
            let mut dtheta = 0.0;
            for (mi, m) in modes.iter().enumerate() {
                let arg = m.q.x * p.x + m.q.y * p.y + m.ph;
                dtheta += m.a * math::sin(arg);
                cosarg[mi] = math::cos(arg);
            }
            dir.set(x, y, wind_rad + dtheta);
            for (wi, wd) in waves_dir.iter().enumerate() {
                // Each wave bends with the SAME wind, about its own axis, and
                // carries its OWN wavenumber.
                let kw = waves_k[wi];
                let wp = wd.perp();
                let mut psi = 0.0;
                for (mi, m) in modes.iter().enumerate() {
                    let qm = m.q.length();
                    if qm > 0.0 {
                        let qh = Vec2::new(m.q.x / qm, m.q.y / qm);
                        psi += -(m.a * kw * wp.dot(qh)) / qm * cosarg[mi];
                    }
                }
                waves[wi].set(x, y, kw * (p.x * wd.x + p.y * wd.y) + psi + phi0);
            }
        }
    }

    WindField { dir, waves, lambda_m, spread_rad, dirs: waves_dir.to_vec(), ks: waves_k.to_vec(), curl_mag }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_seed::RunIdentity;

    fn rng(s: u64) -> DetRng {
        crate::rng::stream(&RunIdentity::from_seed(s), crate::rng::WIND)
    }
    const TRAIN_K: f64 = 20.0;
    const MOUND_K: f64 = 0.4;

    #[test]
    fn deterministic() {
        let a = build(&mut rng(1), 0.7, 1300.0, 0.3, 2400.0, TRAIN_K, 0.0);
        let b = build(&mut rng(1), 0.7, 1300.0, 0.3, 2400.0, TRAIN_K, 0.0);
        assert_eq!(a.n_waves(), b.n_waves());
        for i in 0..a.n_waves() {
            assert_eq!(a.waves[i].data, b.waves[i].data);
        }
    }

    #[test]
    fn every_wave_advances_one_cycle_per_wavelength() {
        // Spacing is true BY CONSTRUCTION, not by tuning: every wave carries
        // |k| = TAU/lambda regardless of which way it points, so neither the
        // wander nor the concentration can move it.
        let f = build(&mut rng(3), 0.0, 1200.0, 0.0, 2000.0, MOUND_K, 0.0);
        let spec = macro_spec();
        for i in 0..f.n_waves() {
            let mut gmax: f64 = 0.0;
            for y in (2..spec.ny - 2).step_by(23) {
                for x in (2..spec.nx - 2).step_by(23) {
                    let gx = (f.waves[i].get(x + 1, y) - f.waves[i].get(x - 1, y)) / (2.0 * RES_M);
                    let gy = (f.waves[i].get(x, y + 1) - f.waves[i].get(x, y - 1)) / (2.0 * RES_M);
                    gmax = gmax.max((gx * gx + gy * gy).sqrt());
                }
            }
            let k = std::f64::consts::TAU / 1200.0;
            assert!((gmax - k).abs() < k * 0.05,
                    "wave {i}: |grad phi| {gmax:.6} against k {k:.6}");
        }
    }

    #[test]
    fn zero_wander_is_a_wallpaper_and_that_is_the_failure_mode() {
        // Documents what the wander exists to prevent. With no wander every
        // wave has an exactly constant gradient, so its crests are straight
        // parallel lines and no defect can exist at any amplitude.
        let f = build(&mut rng(5), 0.4, 1300.0, 0.0, 2000.0, TRAIN_K, 0.0);
        let spec = macro_spec();
        let mut g = Vec::new();
        for y in (2..spec.ny - 2).step_by(17) {
            for x in (2..spec.nx - 2).step_by(17) {
                g.push(f.waves[0].get(x + 1, y) - f.waves[0].get(x - 1, y));
            }
        }
        let m = g.iter().sum::<f64>() / g.len() as f64;
        let var = g.iter().map(|v| (v - m).powi(2)).sum::<f64>() / g.len() as f64;
        assert!(var < 1e-18, "zero-wander phase gradient is not constant (var {var:e})");
    }

    #[test]
    fn wander_creates_curl_and_zero_wander_does_not() {
        // curl_mag is the defect budget, and it must SCALE with the dial --
        // the property its first (ratio) version lacked, which read a constant
        // 0.703 across an entire ladder.
        assert_eq!(build(&mut rng(21), 0.5, 1300.0, 0.0, 2400.0, TRAIN_K, 0.0).curl_mag, 0.0);
        let a = build(&mut rng(21), 0.5, 1300.0, 0.30, 2400.0, TRAIN_K, 0.0).curl_mag;
        let b = build(&mut rng(21), 0.5, 1300.0, 0.90, 2400.0, TRAIN_K, 0.0).curl_mag;
        assert!(b > a * 2.5, "curl_mag did not scale with wander ({a:.3} -> {b:.3})");
    }

    #[test]
    fn concentration_sets_the_spread_and_the_ranges_do_not_meet() {
        // THE dial. High kappa pulls the waves onto one axis; low kappa lets
        // them spread, which is the only route to the mound class.
        let mut tr = Vec::new();
        let mut mo = Vec::new();
        for s in 0..24u64 {
            tr.push(build(&mut rng(s), 0.6, 1300.0, 0.3, 2400.0, TRAIN_K, 0.0).spread_rad);
            mo.push(build(&mut rng(s), 0.6, 1300.0, 0.3, 2400.0, MOUND_K, 0.0).spread_rad);
        }
        let med = |v: &mut Vec<f64>| {
            v.sort_by(|a, b| a.partial_cmp(b).unwrap());
            v[v.len() / 2]
        };
        let (t, m) = (med(&mut tr), med(&mut mo));
        assert!(t < 0.45, "train spread {t:.3} rad is not concentrated");
        assert!(m > 0.95, "mound spread {m:.3} rad is not dispersed");
    }

    #[test]
    fn wavelength_is_independent_of_both_dials() {
        // The property the single-wave construction did not have: its one dial
        // drove orientation order and spacing in OPPOSITE directions (A .898
        // -> .484 while lambda collapsed 1148 m -> 546 m). Here spacing lives
        // in |k| and nothing else touches it.
        let spec = macro_spec();
        let k = std::f64::consts::TAU / 1300.0;
        for (w, kap) in [(0.0, 20.0), (0.8, 20.0), (0.0, 0.4), (0.8, 0.4), (1.4, 0.15)] {
            let f = build(&mut rng(23), 0.0, 1300.0, w, 2400.0, kap, 0.0);
            for i in 0..f.n_waves() {
                // Project onto the wave's OWN axis. The mean of |grad| is
                // biased high whenever the gradient direction fluctuates
                // (Jensen: E|k+d| > |E(k+d)|) -- measured 15.7% at wander 0.8,
                // which is a property of the statistic and not a wavelength
                // drift. The perturbation is zero-mean ALONG the axis, so the
                // projection is the quantity that actually says "this wave
                // carries wavenumber k".
                let mut g = Vec::new();
                for y in (2..spec.ny - 2).step_by(29) {
                    for x in (2..spec.nx - 2).step_by(29) {
                        let gx = (f.waves[i].get(x + 1, y) - f.waves[i].get(x - 1, y)) / (2.0 * RES_M);
                        let gy = (f.waves[i].get(x, y + 1) - f.waves[i].get(x, y - 1)) / (2.0 * RES_M);
                        g.push(gx * f.dirs[i].x + gy * f.dirs[i].y);
                    }
                }
                let m = g.iter().sum::<f64>() / g.len() as f64;
                // 8%: at the most extreme rung (wander 1.4 rad = 80 deg of
                // swing, kappa 0.15) the projection reads 5.6% low, because
                // the small-angle step behind the modal correction stops being
                // small there. The point of the test is the ORDER of
                // magnitude -- the single-wave construction collapsed the
                // wavelength by 52% across its dial range.
                assert!((m - k).abs() < k * 0.08,
                        "wander {w} kappa {kap} wave {i}: k.grad {m:.6} vs k {k:.6}");
            }
        }
    }

    #[test]
    fn high_concentration_recovers_the_single_axis_field() {
        // The train limit is what makes this ONE construction rather than two:
        // as kappa grows the waves align and the superposition collapses back
        // to the single-wave field the previous version built. Asserted on the
        // DIRECTIONS, which is the actual claim -- an earlier version compared
        // mean cycle values over sparse samples and was measuring its own
        // sampling noise.
        let f = build(&mut rng(31), 0.9, 1300.0, 0.25, 2400.0, 400.0, 0.0);
        assert!(f.spread_rad < 0.12, "spread {:.4} at kappa 400", f.spread_rad);
        let mut worst: f64 = 0.0;
        for a in &f.dirs {
            for b in &f.dirs {
                worst = worst.max(a.dot(*b).clamp(-1.0, 1.0).acos());
            }
        }
        assert!(worst < 0.30,
                "widest angle between waves {worst:.3} rad at kappa 400 -- not one axis");
        // And the opposite end must genuinely spread.
        let lo = build(&mut rng(31), 0.9, 1300.0, 0.25, 2400.0, 0.2, 0.0);
        assert!(lo.spread_rad > f.spread_rad * 4.0,
                "low concentration did not disperse ({:.3} vs {:.3})", lo.spread_rad, f.spread_rad);
    }

    #[test]
    fn direction_swing_matches_the_drawn_wander() {
        // `wander_rad` is the STANDARD DEVIATION of the local azimuth, not a
        // hard bound -- the field is a sum of sinusoids normalised to that
        // sigma. An earlier version asserted |dev| <= wander_rad and failed
        // the moment the construction became modal.
        for seed in [11u64, 12, 13] {
            let d = crate::draw::site(&RunIdentity::from_seed(seed),
                                      Some(crate::Mode::Aeolian), None);
            let f = build(&mut rng(seed), d.wind_rad, d.wavelength_m,
                          d.wind_wander_rad, d.wind_wander_m, d.kappa, 0.0);
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
        }
    }

    #[test]
    fn von_mises_matches_its_own_parameters() {
        // The sampler is exact (Best & Fisher); this pins that it was
        // transcribed correctly, because a subtly wrong rejection test still
        // returns plausible angles.
        for kap in [0.5f64, 2.0, 20.0] {
            let mut r = rng(77);
            let mu = 0.8;
            let n = 4000;
            let mut s = Vec2::ZERO;
            for _ in 0..n {
                let t = von_mises(&mut r, mu, kap);
                s = s + Vec2::new(math::cos(t), math::sin(t));
            }
            let rbar = s.length() / n as f64;
            let mean_dir = math::atan2(s.y, s.x);
            assert!((mean_dir - mu).abs() < 0.12, "kappa {kap}: mean {mean_dir:.3} vs mu {mu}");
            // Resultant length rises monotonically with concentration.
            assert!(rbar > 0.15 && rbar < 1.0, "kappa {kap}: rbar {rbar:.3}");
        }
        let mut r = rng(78);
        let lo = (0..2000).map(|_| von_mises(&mut r, 0.0, 0.3)).collect::<Vec<_>>();
        let hi = (0..2000).map(|_| von_mises(&mut r, 0.0, 30.0)).collect::<Vec<_>>();
        let disp = |v: &[f64]| {
            let s = v.iter().fold(Vec2::ZERO, |a, t| a + Vec2::new(math::cos(*t), math::sin(*t)));
            s.length() / v.len() as f64
        };
        assert!(disp(&hi) > disp(&lo), "concentration did not concentrate");
    }

    #[test]
    fn cycle_is_a_unit_interval() {
        let f = build(&mut rng(13), 0.9, 1250.0, 0.3, 2200.0, MOUND_K, 0.0);
        let spec = macro_spec();
        for i in 0..f.n_waves() {
            for y in (0..spec.ny).step_by(31) {
                for x in (0..spec.nx).step_by(31) {
                    let c = f.cycle(i, x, y);
                    assert!((0.0..1.0).contains(&c), "cycle {c} outside [0,1)");
                }
            }
        }
    }
}
