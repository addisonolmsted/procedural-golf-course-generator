//! Valley/canyon — the drainage-carving swept primitive — and Ridge, its
//! sign-flipped sibling.
//!
//! A valley is authored by its FLOOR: a strictly monotone longitudinal
//! elevation (`floor_z0_m` falling at `fall_gradient` per meter of arc), a
//! per-station floor half-width, and left/right wall gradients (asymmetry).
//! The swept surface is floor + C1-rounded walls, and it is composed into the
//! terrain LAST via smooth-min ("drainage carves last"), so the floor is
//! monotone end-to-end by construction no matter what it cuts through.

use course_world::math::Vec2;
use course_world::profile::Profile;
use serde::{Deserialize, Serialize};

use course_world::ease::{ramp, smax, smin, smoothstep};
use course_world::spline::Spine;

/// Interpolate a primitive's contribution toward "not applied at all".
///
/// `w` is the CORE WEIGHT: 1 = the plan as authored, 0 = this primitive
/// contributes nothing here. The budget solve uses it to relax relief
/// inside the routable core WITHOUT touching the outer ring — the old
/// solve scaled amplitudes globally, which flattened the whole 3 km box to
/// satisfy a cap that only governs the middle 1.5 km.
#[inline]
pub(crate) fn blend(z: f64, full: f64, w: f64) -> f64 {
    if w >= 1.0 { full } else { z + w * (full - z) }
}

/// Distance from `p` to an axis-aligned box (0 inside). O(1), and a lower
/// bound on the distance to anything the box contains — which is what makes
/// the primitives' `surface` bounds below exact.
#[inline]
fn dist_to_bbox(p: Vec2, bb: (Vec2, Vec2)) -> f64 {
    let dx = (bb.0.x - p.x).max(p.x - bb.1.x).max(0.0);
    let dy = (bb.0.y - p.y).max(p.y - bb.1.y).max(0.0);
    (dx * dx + dy * dy).sqrt()
}
use crate::config::Path;

fn d_one() -> f64 { 1.0 }
fn d_floor_round() -> f64 { 6.0 }
fn d_shoulder() -> f64 { 4.0 }
fn d_crest_rise() -> f64 { 20.0 }

/// Minimum enforced fall gradient — "strictly monotone end to end".
pub const MIN_FALL_GRADIENT: f64 = 5e-4;

/// Stations in the cumulative-drop table built from `fall_profile`. Fixed so
/// the table (and therefore the surface) is resolution-independent.
const FALL_TABLE_N: usize = 128;

/// Floor drop over `arc` metres of a valley of total length `len`, honouring
/// `fall_profile` when present.
///
/// Accordance needs this BEFORE the valley is resolved (it sets `floor_z0`
/// so the tributary meets its trunk exactly), which is why it takes the
/// config rather than the resolved form.
pub(crate) fn cumulative_drop(v: &Valley, len: f64, arc: f64) -> f64 {
    let Some(prof) = &v.fall_profile else {
        return v.fall_gradient.max(MIN_FALL_GRADIENT) * arc;
    };
    if len <= 0.0 {
        return 0.0;
    }
    let n = FALL_TABLE_N;
    let target = arc.clamp(0.0, len);
    let mut acc = 0.0;
    for i in 0..n {
        let (u0, u1) = (i as f64 / n as f64, (i + 1) as f64 / n as f64);
        let (a0, a1) = (u0 * len, u1 * len);
        if a0 >= target {
            break;
        }
        let hi = a1.min(target);
        let g0 = prof.sample(u0).max(MIN_FALL_GRADIENT);
        let g1 = prof.sample(u1).max(MIN_FALL_GRADIENT);
        // linear-in-u gradient over the (possibly partial) station
        let f = (hi - a0) / (a1 - a0).max(1e-12);
        let g_hi = g0 + (g1 - g0) * f;
        acc += 0.5 * (g0 + g_hi) * (hi - a0);
    }
    acc
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Valley {
    pub path: Path,
    /// Floor elevation at s = 0 (the upstream end), meters.
    pub floor_z0_m: f64,
    /// Floor drop per meter of arc (>= MIN_FALL_GRADIENT enforced).
    pub fall_gradient: f64,
    /// Floor half-width per station (meters).
    pub floor_halfwidth: Profile,
    /// Wall gradients (rise/run) on the left/right of the spine tangent —
    /// unequal values give the asymmetric barranca cross-section.
    pub wall_grad_left: f64,
    pub wall_grad_right: f64,
    /// C1 rounding width at the floor-wall seam, meters.
    #[serde(default = "d_floor_round")]
    pub floor_round_m: f64,
    /// Smooth-min blend band at the shoulder (meters of elevation).
    #[serde(default = "d_shoulder")]
    pub shoulder_k_m: f64,
    /// Smooth-min band at the DIVIDE, where this valley's wall meets a
    /// neighbour's. `None` (the default, and what every existing config
    /// round-trips to) keeps one constant band everywhere.
    ///
    /// One band cannot do both jobs. The band is what rounds the seam where
    /// two surfaces meet — and a valley wall has two such seams with
    /// opposite requirements. At the FLOOR it meets the terrain and wants a
    /// small band, or the channel dissolves. At the DIVIDE it meets the
    /// neighbouring valley's wall and wants a large one, or the interfluve
    /// is a knife-edge crease where two planes intersect, which is not what
    /// a real interfluve looks like and not what the geomorphon ridge class
    /// has area of.
    ///
    /// Measured with one constant band: raising it 3 -> 45 m took
    /// `ridge_mask_area_frac` 0.258 -> 0.330 (real 0.408) and made every
    /// gate pass — while visibly destroying the channels, because a 45 m
    /// band swallows the valley floor everywhere except the centerline. The
    /// terrain passed the numbers and looked worse. Hence a band that
    /// GROWS with height above the floor instead of a single compromise.
    #[serde(default)]
    pub crest_k_m: Option<f64>,
    /// Wall rise over which the band grows from `shoulder_k_m` to
    /// `crest_k_m`, metres. Roughly the hillslope relief — the height of a
    /// divide above its channel.
    #[serde(default = "d_crest_rise")]
    pub crest_rise_m: f64,
    /// Hillslope convexity, 0..1. 0 = a straight constant-gradient wall (the
    /// default, bit-for-bit what every existing config produces). Above 0 the
    /// wall gradient decays to `1 - convexity` of its channel value over
    /// `crest_rise_m` of rise, giving the convex upper hillslope real
    /// creep-driven terrain has. See [`ResolvedValley::wall_rise`].
    #[serde(default)]
    pub wall_convexity: f64,
    /// Snap this valley's END onto an earlier valley (by index) with an
    /// accordant floor elevation (tributary junction).
    #[serde(default)]
    pub join_trunk: Option<usize>,
    /// Per-station fall gradient along normalized arc. `None` (the default,
    /// and what every existing config round-trips to) means the constant
    /// `fall_gradient`.
    ///
    /// A reach's drainage area GROWS downstream at every child mouth, and
    /// channel gradient follows a slope-area law, so a single scalar cannot
    /// carry a real long profile — which is concave, steep at the head and
    /// gentle at the mouth. Knots are clamped to at least
    /// [`MIN_FALL_GRADIENT`] and integrated into a cumulative drop table at
    /// resolve time, so the floor stays monotone by construction.
    #[serde(default)]
    pub fall_profile: Option<Profile>,
}

pub(crate) struct ResolvedValley {
    pub spine: Spine,
    pub floor_z0: f64,
    pub grad: f64,
    pub hw: Profile,
    pub wl: f64,
    pub wr: f64,
    pub round: f64,
    pub k: f64,
    /// Smooth-min band at the divide, and the rise over which the band
    /// grows into it. `crest_k == k` reproduces the constant-band case.
    crest_k: f64,
    crest_rise: f64,
    convexity: f64,
    /// Arc position (meters) past which the floor stops descending — set for
    /// tributaries whose spine overshoots into the trunk: the authored part
    /// (entry -> junction) descends strictly; past the junction the tail
    /// RISES gently (`tail_rise` per meter) so the smooth-min against the
    /// trunk floor becomes an exact no-op instead of grooving it (smin dips
    /// k/4 where two surfaces coincide exactly).
    pub arc_cap_m: Option<f64>,
    /// Cumulative floor DROP from u=0, sampled uniformly. Monotone
    /// non-decreasing. `None` = the constant-gradient case.
    fall_cum: Option<Vec<f64>>,
    pub tail_rise: f64,
    pub bbox: (Vec2, Vec2),
    /// Spine bbox with NO pad, plus the lowest floor elevation and the
    /// widest floor — together a cheap exact lower bound on `surface`
    /// (see `apply`).
    tight: (Vec2, Vec2),
    floor_min: f64,
    hw_max: f64,
    wall_min: f64,
}

impl ResolvedValley {
    pub fn with_cap(v: &Valley, spine: Spine, arc_cap_m: Option<f64>) -> Self {
        let tail_rise = match arc_cap_m {
            Some(cap) => {
                let overshoot = (spine.length() - cap).max(1.0);
                1.5 * v.shoulder_k_m.max(0.1) / overshoot
            }
            None => 0.0,
        };
        let hw_max = v.floor_halfwidth.knots.iter().map(|k| k.1).fold(0.0, f64::max);
        let wall_min = v.wall_grad_left.min(v.wall_grad_right).max(0.05);
        // walls can rise ~300 m before smin is guaranteed to no-op — generous
        let pad = hw_max + 300.0 / wall_min + v.floor_round_m + 50.0;
        let bbox = spine.bbox(pad);
        let tight = spine.bbox(0.0);
        // The floor only ever falls with arc (fall_gradient >= 0), so the
        // downstream end is the minimum — except past `arc_cap_m`, where
        // `tail_rise` lifts it again. Taking the un-risen value keeps the
        // bound conservative.
        let grad = v.fall_gradient.max(MIN_FALL_GRADIENT);
        let floor_min = v.floor_z0_m
            - match &v.fall_profile {
                Some(prof) => {
                    // Upper bound on total drop: the steepest knot over the
                    // whole length (conservative, keeps the M1 bound sound).
                    let gmax = prof.knots.iter().map(|k| k.1).fold(0.0, f64::max);
                    gmax.max(MIN_FALL_GRADIENT) * spine.length()
                }
                None => grad * spine.length(),
            };
        // The bound must use the TRUE minimum wall gradient, not the
        // `.max(0.05)`-clamped one the pad uses: `ramp` grows with
        // gradient, so clamping it UP would make the "lower" bound sit
        // ABOVE the real surface and cull cells the valley still carves.
        let wall_min_true = v.wall_grad_left.min(v.wall_grad_right).max(0.0);
        let fall_cum = v.fall_profile.as_ref().map(|prof| {
            // Trapezoid-integrate the gradient profile into cumulative drop.
            // Clamping each knot at MIN_FALL_GRADIENT is what guarantees the
            // table is strictly increasing, hence the floor monotone.
            let n = FALL_TABLE_N;
            let len = spine.length();
            let mut cum = Vec::with_capacity(n + 1);
            cum.push(0.0);
            let mut acc = 0.0;
            for i in 0..n {
                let u0 = i as f64 / n as f64;
                let u1 = (i + 1) as f64 / n as f64;
                let g0 = prof.sample(u0).max(MIN_FALL_GRADIENT);
                let g1 = prof.sample(u1).max(MIN_FALL_GRADIENT);
                acc += 0.5 * (g0 + g1) * (u1 - u0) * len;
                cum.push(acc);
            }
            cum
        });
        ResolvedValley {
            fall_cum,
            tight,
            floor_min,
            hw_max,
            wall_min: wall_min_true,
            spine,
            floor_z0: v.floor_z0_m,
            grad: v.fall_gradient.max(MIN_FALL_GRADIENT),
            hw: v.floor_halfwidth.clone(),
            wl: v.wall_grad_left,
            wr: v.wall_grad_right,
            round: v.floor_round_m.max(0.5),
            k: v.shoulder_k_m.max(0.1),
            // Never BELOW the shoulder band: the band must grow with height,
            // never shrink, or the early-out's `k_max` stops bounding it.
            crest_k: v.crest_k_m.unwrap_or(v.shoulder_k_m).max(v.shoulder_k_m.max(0.1)),
            crest_rise: v.crest_rise_m.max(1.0),
            // Capped below 1: at exactly 1 the wall goes flat at the crest
            // and can never clear the terrain, so `smin` would carve to the
            // bbox edge and seam there.
            convexity: v.wall_convexity.clamp(0.0, 0.85),
            arc_cap_m,
            tail_rise,
            bbox,
        }
    }

    /// Floor elevation at normalized arc `u`.
    pub fn floor_z(&self, u: f64) -> f64 {
        let arc = u.clamp(0.0, 1.0) * self.spine.length();
        match self.arc_cap_m {
            Some(cap) if arc > cap => {
                self.floor_z0 - self.drop_at(cap) + self.tail_rise * (arc - cap)
            }
            _ => self.floor_z0 - self.drop_at(arc),
        }
    }

    /// Cumulative floor drop over `arc` metres from the head.
    #[inline]
    fn drop_at(&self, arc: f64) -> f64 {
        let Some(cum) = &self.fall_cum else {
            return self.grad * arc;
        };
        let len = self.spine.length();
        if len <= 0.0 {
            return 0.0;
        }
        let x = (arc / len).clamp(0.0, 1.0) * FALL_TABLE_N as f64;
        let i = (x.floor() as usize).min(FALL_TABLE_N - 1);
        let t = x - i as f64;
        cum[i] + (cum[i + 1] - cum[i]) * t
    }

    /// The swept valley surface at `p` (rises with distance; far away it
    /// exceeds any terrain and the smin below is an exact no-op).
    pub fn surface(&self, p: Vec2) -> f64 {
        self.surface_and_rise(p).0
    }

    /// The surface, plus how far up the wall it sits above the local floor.
    /// The rise is what selects the blend band: 0 at the channel, growing
    /// to `crest_rise` at a divide.
    #[inline]
    fn surface_and_rise(&self, p: Vec2) -> (f64, f64) {
        let hit = self.spine.project(p);
        let hw = self.hw.sample(hit.u);
        let wall = if hit.side > 0.0 { self.wl } else { self.wr };
        let rise = self.wall_rise(ramp(hit.d - hw, wall, self.round));
        (self.floor_z(hit.u) + rise, rise)
    }

    /// Shape the wall from a straight ramp into a hillslope.
    ///
    /// `v` is the linear rise the constant-gradient ramp would give. A real
    /// hillslope is not that line: it is CONVEX over its upper half, because
    /// soil creep transports proportionally to slope and the divide is a
    /// no-flux boundary, so the gradient must go to zero at the crest. That
    /// convexity is most of why 41% of a real piedmont tile classifies as
    /// geomorphon RIDGE while the spike's straight-walled version classifies
    /// at 24% — a plane is "slope", not "ridge", however you compose it.
    ///
    /// `convexity` = 0 keeps the straight ramp bit-for-bit (and is the
    /// default, so nothing existing moves). Above 0 the gradient decays
    /// toward `1 - convexity` of its channel value over `crest_rise` of
    /// rise:
    ///
    /// ```text
    ///   S(v) = g * (1 - c * smoothstep(0, R, v))
    /// ```
    ///
    /// integrated exactly below. It is deliberately NOT allowed to reach
    /// zero gradient: a wall that flattens completely never climbs above the
    /// surrounding terrain, so `smin` would keep carving it out to the bbox
    /// edge and leave a hard seam there. Convexity is a shape, not a ceiling.
    #[inline]
    fn wall_rise(&self, v: f64) -> f64 {
        if self.convexity <= 0.0 || v <= 0.0 {
            return v;
        }
        let r = self.crest_rise;
        let c = self.convexity;
        if v >= r {
            // Past the knee the gradient is constant at (1 - c); the
            // integral of the smoothstep over [0, r] is exactly r/2, by its
            // antisymmetry about the midpoint.
            v - c * (0.5 * r + (v - r))
        } else {
            // int_0^v smoothstep(0, r, t) dt with t = v/r:
            //   r * (t^3 - t^4/2)
            let t = v / r;
            v - c * r * (t * t * t - 0.5 * t * t * t * t)
        }
    }

    /// Blend band at a point `rise` metres up the wall. Smoothstep, so the
    /// band varies C1 and the composed surface stays C1 with it.
    #[inline]
    fn band(&self, rise: f64) -> f64 {
        if self.crest_k <= self.k {
            return self.k;
        }
        self.k + (self.crest_k - self.k) * smoothstep(0.0, self.crest_rise, rise)
    }

    /// Conservative lower bound on [`Self::surface`] from ONE distance
    /// instead of the full spine projection: the true wall distance is at
    /// least the distance to the un-padded spine bbox, `ramp` is monotone
    /// in both distance and gradient, and the floor never drops below
    /// `floor_min`.
    #[inline]
    fn surface_lower_bound(&self, p: Vec2) -> f64 {
        let d = dist_to_bbox(p, self.tight);
        // `wall_rise` is monotone in its argument and only ever LOWERS it, so
        // the bound has to pass through it too. Skipping it would leave the
        // "lower" bound above the real convex surface and cull cells the
        // valley still carves — the same class of error the M1 bounds hit
        // twice before they were right.
        self.floor_min + self.wall_rise(ramp(d - self.hw_max, self.wall_min, self.round))
    }

    #[inline]
    pub fn apply(&self, p: Vec2, z: f64, w: f64) -> f64 {
        if w <= 0.0 {
            return z;
        }
        if p.x < self.bbox.0.x || p.x > self.bbox.1.x
            || p.y < self.bbox.0.y || p.y > self.bbox.1.y
        {
            return z;
        }
        // `smin(z, s, k)` saturates to exactly z once s >= z + k, so a lower
        // bound on the surface that already clears the terrain proves this
        // valley cannot change the result — skipping the projection. Exact,
        // not a tolerance. The padded bbox above is relief-scaled and so
        // covers most of the box (1.3-6.1 km on a 3 km world); this is what
        // actually culls, and it is what makes a 30-valley network payable.
        // `crest_k` is the LARGEST band this valley can use, and smin
        // saturates later with a bigger band, so bounding with it keeps the
        // early-out exact rather than merely conservative. The weighted-rise
        // path only ever LOWERS the surface, so a bound computed for w == 1
        // stays a valid lower bound for any w — culling can only become
        // conservative, never wrong.
        if self.surface_lower_bound(p) >= z + self.crest_k {
            return z;
        }
        let (surf, rise) = self.surface_and_rise(p);
        // Weight the WALL RISE, not the composed surface.
        //
        // Before M5 valleys were exempt from the core weight because blending
        // the composed surface toward the terrain lifts the FLOOR wherever it
        // is inside the core, and a floor that lifts relative to its
        // neighbours is a ponded drain. With a grown network the floors are
        // unavoidably inside the core, so exemption is no longer an option and
        // the old lever would break drainage outright.
        //
        // Scaling the rise instead lowers DIVIDES and leaves floors exactly
        // where they were: at `rise == 0` — the channel — the weight has no
        // effect at all, so `floor_z` is untouched along every centerline and
        // monotonicity is preserved by construction, not by hoping. Pinned by
        // `weighted_rise_never_moves_the_floor`.
        let s = if w >= 1.0 { surf } else { surf - rise * (1.0 - w) };
        smin(z, s, self.band(rise))
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Ridge {
    pub path: Path,
    /// Crest elevation at s = 0, meters.
    pub crest_z0_m: f64,
    /// Crest drop per meter of arc (0 = level crest; may be negative to rise).
    #[serde(default)]
    pub fall_gradient: f64,
    /// Crest half-width per station (meters).
    pub crest_halfwidth: Profile,
    /// Flank gradients (rise/run) down from the crest, left/right.
    pub flank_grad_left: f64,
    pub flank_grad_right: f64,
    /// C1 rounding at the crest edge, meters.
    #[serde(default = "d_floor_round")]
    pub crest_round_m: f64,
    /// Smooth-max blend band at the base (meters of elevation).
    #[serde(default = "d_shoulder")]
    pub base_k_m: f64,
    /// Multiplier on crest height per station (tapers the spine ends), 0-1.
    #[serde(default = "d_one_profile")]
    pub emphasis: Profile,
}

fn d_one_profile() -> Profile {
    Profile::constant(d_one())
}

pub(crate) struct ResolvedRidge {
    pub spine: Spine,
    pub crest_z0: f64,
    pub grad: f64,
    pub hw: Profile,
    pub fl: f64,
    pub fr: f64,
    pub round: f64,
    pub k: f64,
    pub emphasis: Profile,
    pub base_ref: f64,
    pub bbox: (Vec2, Vec2),
    tight: (Vec2, Vec2),
    crest_max: f64,
    hw_max: f64,
    flank_min: f64,
}

impl ResolvedRidge {
    /// `base_ref` = a reference base elevation (config base) the emphasis
    /// profile fades the crest toward at the spine ends.
    pub fn new(r: &Ridge, spine: Spine, base_ref: f64) -> Self {
        let hw_max = r.crest_halfwidth.knots.iter().map(|k| k.1).fold(0.0, f64::max);
        let flank_min = r.flank_grad_left.min(r.flank_grad_right).max(0.05);
        let pad = hw_max + 300.0 / flank_min + r.crest_round_m + 50.0;
        let bbox = spine.bbox(pad);
        let tight = spine.bbox(0.0);
        // Emphasis interpolates the crest between `base_ref` and the full
        // crest and only ever subtracts past that, so neither endpoint can
        // be exceeded.
        // `fall_gradient` may be negative (a crest that RISES with arc), so
        // both ends have to be considered. `crest` is a convex combination
        // of `base_ref - 3k` and the full crest, so neither endpoint can be
        // exceeded.
        let crest_end = r.crest_z0_m - r.fall_gradient * spine.length();
        let crest_max = r.crest_z0_m.max(crest_end).max(base_ref);
        // See the valley note: the bound needs the TRUE minimum flank.
        let flank_min_true = r.flank_grad_left.min(r.flank_grad_right).max(0.0);
        ResolvedRidge {
            tight,
            crest_max,
            hw_max,
            flank_min: flank_min_true,
            spine,
            crest_z0: r.crest_z0_m,
            grad: r.fall_gradient,
            hw: r.crest_halfwidth.clone(),
            fl: r.flank_grad_left,
            fr: r.flank_grad_right,
            round: r.crest_round_m.max(0.5),
            k: r.base_k_m.max(0.1),
            emphasis: r.emphasis.clone(),
            base_ref,
            bbox,
        }
    }

    pub fn surface(&self, p: Vec2) -> f64 {
        let hit = self.spine.project(p);
        let hw = self.hw.sample(hit.u);
        let flank = if hit.side > 0.0 { self.fl } else { self.fr };
        let crest_full =
            self.crest_z0 - self.grad * hit.u.clamp(0.0, 1.0) * self.spine.length();
        // Emphasis fades the crest toward — and at the ends BELOW — the base
        // reference. The extra 3k dip guarantees the faded surface falls under
        // any nearby terrain (base is a global reference; local tilt may sit
        // lower), so ridge tips vanish instead of smearing base-level pancakes.
        let e = self.emphasis.sample(hit.u).clamp(0.0, 1.0);
        let crest = self.base_ref + (crest_full - self.base_ref) * e - 3.0 * self.k * (1.0 - e);
        crest - ramp(hit.d - hw, flank, self.round)
    }

    /// Conservative UPPER bound on [`Self::surface`] — the mirror of the
    /// valley's lower bound, because a ridge composes with `smax`.
    #[inline]
    fn surface_upper_bound(&self, p: Vec2) -> f64 {
        let d = dist_to_bbox(p, self.tight);
        self.crest_max - ramp(d - self.hw_max, self.flank_min, self.round)
    }

    #[inline]
    pub fn apply(&self, p: Vec2, z: f64, w: f64) -> f64 {
        if w <= 0.0 {
            return z;
        }
        if p.x < self.bbox.0.x || p.x > self.bbox.1.x
            || p.y < self.bbox.0.y || p.y > self.bbox.1.y
        {
            return z;
        }
        // `smax` saturates to exactly z once the surface sits k BELOW it.
        if self.surface_upper_bound(p) <= z - self.k {
            return z;
        }
        blend(z, smax(z, self.surface(p), self.k), w)
    }
}

#[cfg(test)]
mod bound_tests {
    use super::*;
    use course_world::profile::Profile;
    use course_world::spline::catmull_rom;

    fn spine() -> Spine {
        Spine::new(catmull_rom(
            &[
                Vec2::new(200.0, 900.0),
                Vec2::new(1400.0, 1600.0),
                Vec2::new(2700.0, 1200.0),
            ],
            10.0,
        ))
    }

    /// The early-out is only sound if the bound never exceeds the surface.
    #[test]
    fn valley_lower_bound_never_exceeds_surface() {
        // Convexity is swept HERE rather than in its own test because it is
        // exactly the kind of change that breaks this bound: `wall_rise`
        // only ever LOWERS the surface, so a bound that skipped the shaping
        // would sit above it and cull cells the valley still carves. The M1
        // bounds were wrong twice for this class of reason.
        for (wl, wr, cap, cx) in [
            (0.30, 0.30, None, 0.0),
            (0.08, 0.45, None, 0.0),
            (0.02, 0.02, None, 0.0),
            (0.20, 0.35, Some(900.0), 0.0),
            (0.30, 0.30, None, 0.85),
            (0.08, 0.45, None, 0.5),
            (0.20, 0.35, Some(900.0), 0.7),
        ] {
            let v = Valley {
                path: crate::config::Path::Points(vec![Vec2::ZERO, Vec2::new(1.0, 0.0)]),
                floor_z0_m: 120.0,
                fall_gradient: 0.01,
                floor_halfwidth: Profile::new(vec![(0.0, 8.0), (0.5, 22.0), (1.0, 14.0)]),
                wall_grad_left: wl,
                wall_grad_right: wr,
                floor_round_m: 6.0,
                shoulder_k_m: 4.0,
                crest_k_m: None,
                crest_rise_m: 20.0,
                wall_convexity: cx,
                join_trunk: None,
                fall_profile: None,
            };
            let r = ResolvedValley::with_cap(&v, spine(), cap);
            let mut worst = f64::NEG_INFINITY;
            let mut worst_p = Vec2::ZERO;
            for i in 0..260 {
                for j in 0..260 {
                    let p = Vec2::new(-300.0 + i as f64 * 14.0, -300.0 + j as f64 * 14.0);
                    let err = r.surface_lower_bound(p) - r.surface(p);
                    if err > worst {
                        worst = err;
                        worst_p = p;
                    }
                }
            }
            assert!(
                worst <= 1e-9,
                "valley (wl {wl}, wr {wr}, cap {cap:?}, convexity {cx}) bound exceeds \
                 surface by {worst:.4} at {worst_p:?}"
            );
        }
    }

    /// How much the early-out actually changes the surface. The bound is
    /// exact, so the only difference is arithmetic: `smin` on its saturated
    /// branch evaluates `s + (z - s)`, which is algebraically z but drifts
    /// by an ULP. Returning z verbatim is the MORE accurate of the two —
    /// this pins the magnitude so a future regression cannot hide here.
    #[test]
    fn valley_early_out_is_ulp_scale() {
        let v = Valley {
            path: crate::config::Path::Points(vec![Vec2::ZERO, Vec2::new(1.0, 0.0)]),
            floor_z0_m: 120.0,
            fall_gradient: 0.01,
            floor_halfwidth: Profile::new(vec![(0.0, 8.0), (0.5, 22.0), (1.0, 14.0)]),
            wall_grad_left: 0.18,
            wall_grad_right: 0.31,
            floor_round_m: 6.0,
            shoulder_k_m: 4.0,
            crest_k_m: None,
            crest_rise_m: 20.0,
            wall_convexity: 0.0,
            join_trunk: None,
            fall_profile: None,
        };
        let r = ResolvedValley::with_cap(&v, spine(), None);
        let mut worst = 0.0f64;
        let mut n_skipped = 0usize;
        for i in 0..260 {
            for j in 0..260 {
                let p = Vec2::new(-300.0 + i as f64 * 14.0, -300.0 + j as f64 * 14.0);
                let z = 150.0 + 0.01 * p.x;
                if r.surface_lower_bound(p) >= z + r.k {
                    n_skipped += 1;
                    worst = worst.max((smin(z, r.surface(p), r.k) - z).abs());
                }
            }
        }
        assert!(n_skipped > 1000, "expected the early-out to fire, fired {n_skipped}x");
        assert!(worst < 1e-9, "early-out changes the surface by {worst:.3e} m — not ULP scale");
    }

    #[test]
    fn ridge_upper_bound_never_below_surface() {
        for (fl, fr, grad) in [(0.30, 0.30, 0.0), (0.10, 0.50, 0.02), (0.25, 0.25, -0.02)] {
            let rg = Ridge {
                path: crate::config::Path::Points(vec![Vec2::ZERO, Vec2::new(1.0, 0.0)]),
                crest_z0_m: 180.0,
                fall_gradient: grad,
                crest_halfwidth: Profile::new(vec![(0.0, 10.0), (0.5, 30.0), (1.0, 10.0)]),
                flank_grad_left: fl,
                flank_grad_right: fr,
                crest_round_m: 12.0,
                base_k_m: 6.0,
                emphasis: Profile::new(vec![(0.0, 0.0), (0.2, 1.0), (0.8, 1.0), (1.0, 0.0)]),
            };
            let r = ResolvedRidge::new(&rg, spine(), 100.0);
            let mut worst = f64::NEG_INFINITY;
            let mut worst_p = Vec2::ZERO;
            for i in 0..260 {
                for j in 0..260 {
                    let p = Vec2::new(-300.0 + i as f64 * 14.0, -300.0 + j as f64 * 14.0);
                    let err = r.surface(p) - r.surface_upper_bound(p);
                    if err > worst {
                        worst = err;
                        worst_p = p;
                    }
                }
            }
            assert!(
                worst <= 1e-9,
                "ridge (fl {fl}, fr {fr}, grad {grad}) surface exceeds bound by {worst:.4} at {worst_p:?}"
            );
        }
    }
}

#[cfg(test)]
mod core_weight_tests {
    use super::*;
    use course_world::profile::Profile;
    use course_world::spline::catmull_rom;

    fn valley(convex: f64) -> ResolvedValley {
        let v = Valley {
            path: crate::config::Path::Points(vec![Vec2::ZERO, Vec2::new(1.0, 0.0)]),
            floor_z0_m: 120.0,
            fall_gradient: 0.01,
            floor_halfwidth: Profile::constant(12.0),
            wall_grad_left: 0.25,
            wall_grad_right: 0.25,
            floor_round_m: 6.0,
            shoulder_k_m: 3.0,
            crest_k_m: Some(30.0),
            crest_rise_m: 20.0,
            wall_convexity: convex,
            join_trunk: None,
            fall_profile: None,
        };
        let sp = Spine::new(catmull_rom(
            &[Vec2::new(0.0, 500.0), Vec2::new(1500.0, 520.0), Vec2::new(3000.0, 480.0)],
            10.0,
        ));
        ResolvedValley::with_cap(&v, sp, None)
    }

    /// The M5 core lever weights the wall RISE. At the channel the rise is
    /// zero, so no weight may move the floor — that is what keeps drains
    /// monotone when the network runs through the core and the old
    /// "valleys are exempt" rule is no longer available.
    #[test]
    fn weighted_rise_never_moves_the_floor() {
        for convex in [0.0, 0.5] {
            let r = valley(convex);
            for k in 0..=200 {
                let u = k as f64 / 200.0;
                let p = r.spine.point_at(u);
                let full = r.apply(p, 1e6, 1.0);
                for w in [0.75, 0.4, 0.05] {
                    let got = r.apply(p, 1e6, w);
                    assert!(
                        (got - full).abs() < 1e-9,
                        "convexity {convex}, w {w}: floor moved {:.6} m at u={u:.3}",
                        got - full
                    );
                }
            }
        }
    }

    /// ...while divides DO come down, or the lever would not be a lever.
    #[test]
    fn weighted_rise_lowers_the_divide() {
        let r = valley(0.5);
        let p = r.spine.point_at(0.5) + Vec2::new(0.0, 220.0);
        let full = r.apply(p, 1e6, 1.0);
        let half = r.apply(p, 1e6, 0.4);
        assert!(half < full - 1.0, "divide only moved {:.3} m", full - half);
    }
}
