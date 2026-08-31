//! The meander planform: a Kinoshita curve with a modulated amplitude.
//!
//! WHY THIS EXISTS (review, 2026-08-30): "the creeks look too sinusoidal with
//! regular meandering that reads artificial". Both creek generators — the
//! aeolian gorge trunk and the Carolina valley creek — carried the same
//! offset,
//!
//! `off(s) = A * [ sin(phi) + 0.35 sin(phi / 2.7) ]`
//!
//! with A CONSTANT along the whole reach. Two sines with a fixed amplitude
//! and a single phase make every bend the same size and the same shape, and
//! the eye reads that regularity as manufactured almost immediately.
//!
//! Two corrections, both cheap, applied here so the two callers cannot drift
//! apart:
//!
//! A. KINOSHITA SKEW + FLATTENING. A pure sine bend is symmetric fore-and-aft;
//!    real meanders are not. Kinoshita (1961) generalises the sine-generated
//!    curve with two THIRD-harmonic terms — a cosine that leans the bend
//!    (skewness `J_S`) and a sine that flattens its apex (flatness `J_F`).
//!    Field values run about 1/32 to 1/16 for each; we take the middle of
//!    that range. The classic form is written on the direction angle; applied
//!    to the lateral offset it produces the same asymmetry, which is what the
//!    eye is reading, and it keeps the existing offset-from-centre-line
//!    formulation (the creek must stay in its valley — a free path cannot be
//!    constrained that way).
//!
//! B. AMPLITUDE MODULATION. The wavelength already drifts on a slow Perlin
//!    channel; the amplitude did not, so bends varied in spacing but never in
//!    SIZE. A second, independent noise channel at a different scale gives
//!    the run of tight-then-lazy bends a real creek shows.
//!
//! The curvature safety properties the old form was tuned for are preserved:
//! the caller still caps `amp` against the local wavelength (`MEANDER_RATIO`)
//! and the added harmonics are bounded, so the bank-angle argument still
//! holds — see `amp_cap_scale`.

use course_world::math;
use course_world::noise;

/// Kinoshita skewness — leans each bend so its apex sits off-centre.
pub const J_SKEW: f64 = 0.055;
/// Kinoshita flatness — broadens the apex instead of rounding it.
pub const J_FLAT: f64 = 0.045;
/// Amplitude modulation depth (fraction of the base amplitude).
pub const AMP_MOD: f64 = 0.55;
/// Arc-length scale of the amplitude channel, metres. Deliberately NOT the
/// wavelength channel's 470/640 m: sharing a scale would make big bends and
/// long bends coincide, which is its own kind of regular.
pub const AMP_MOD_M: f64 = 1150.0;

/// The bounded peak of `sin(x) + 0.35 sin(x/2.7) + J_S cos(3x) + J_F sin(3x)`.
///
/// The old two-sine form peaked at 1.35; adding the third harmonics can add
/// at most `J_SKEW + J_FLAT`. The caller's amplitude cap is expressed against
/// the OLD peak, so scale the cap by this ratio and the resulting bank angle
/// is unchanged — the curvature guard keeps meaning what it meant.
pub fn amp_cap_scale() -> f64 {
    1.35 / (1.35 + J_SKEW + J_FLAT)
}

/// Lateral offset shape at integrated phase `phi`, peak ~1.45.
///
/// `phi` MUST be the integrated phase (sum of `2*pi*ds/lambda(s)`), not
/// `2*pi*s/lambda(s)` — dividing arc length by a varying wavelength gives an
/// instantaneous frequency that drifts without bound, which is the bug the
/// gorge comments record at length.
pub fn shape(phi: f64) -> f64 {
    math::sin(phi)
        + 0.35 * math::sin(phi / 2.7)
        + J_SKEW * math::cos(3.0 * phi)
        + J_FLAT * math::sin(3.0 * phi)
}

/// Amplitude multiplier at arc length `s`, mean 1.0, in `[1-AMP_MOD, 1+AMP_MOD]`.
pub fn amp_mod(s_m: f64, seed: u32) -> f64 {
    1.0 + AMP_MOD * noise::perlin1(s_m / AMP_MOD_M, seed)
}
