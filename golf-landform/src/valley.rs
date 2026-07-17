//! Valley/canyon — the drainage-carving swept primitive — and Ridge, its
//! sign-flipped sibling.
//!
//! A valley is authored by its FLOOR: a strictly monotone longitudinal
//! elevation (`floor_z0_m` falling at `fall_gradient` per meter of arc), a
//! per-station floor half-width, and left/right wall gradients (asymmetry).
//! The swept surface is floor + C1-rounded walls, and it is composed into the
//! terrain LAST via smooth-min ("drainage carves last"), so the floor is
//! monotone end-to-end by construction no matter what it cuts through.

use golf_core::math::Vec2;
use golf_core::spline::Profile;
use serde::{Deserialize, Serialize};

use crate::ease::{ramp, smax, smin};
use crate::spline::Spine;
use crate::Path;

fn d_one() -> f64 { 1.0 }
fn d_floor_round() -> f64 { 6.0 }
fn d_shoulder() -> f64 { 4.0 }

/// Minimum enforced fall gradient — "strictly monotone end to end".
pub const MIN_FALL_GRADIENT: f64 = 5e-4;

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
    /// Snap this valley's END onto an earlier valley (by index) with an
    /// accordant floor elevation (tributary junction).
    #[serde(default)]
    pub join_trunk: Option<usize>,
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
    /// Arc position (meters) past which the floor stops descending — set for
    /// tributaries whose spine overshoots into the trunk: the authored part
    /// (entry -> junction) descends strictly; past the junction the tail
    /// RISES gently (`tail_rise` per meter) so the smooth-min against the
    /// trunk floor becomes an exact no-op instead of grooving it (smin dips
    /// k/4 where two surfaces coincide exactly).
    pub arc_cap_m: Option<f64>,
    pub tail_rise: f64,
    pub bbox: (Vec2, Vec2),
}

impl ResolvedValley {
    pub fn new(v: &Valley, spine: Spine) -> Self {
        Self::with_cap(v, spine, None)
    }

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
        ResolvedValley {
            spine,
            floor_z0: v.floor_z0_m,
            grad: v.fall_gradient.max(MIN_FALL_GRADIENT),
            hw: v.floor_halfwidth.clone(),
            wl: v.wall_grad_left,
            wr: v.wall_grad_right,
            round: v.floor_round_m.max(0.5),
            k: v.shoulder_k_m.max(0.1),
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
                self.floor_z0 - self.grad * cap + self.tail_rise * (arc - cap)
            }
            _ => self.floor_z0 - self.grad * arc,
        }
    }

    /// The swept valley surface at `p` (rises with distance; far away it
    /// exceeds any terrain and the smin below is an exact no-op).
    pub fn surface(&self, p: Vec2) -> f64 {
        let hit = self.spine.project(p);
        let hw = self.hw.sample(hit.u);
        let wall = if hit.side > 0.0 { self.wl } else { self.wr };
        self.floor_z(hit.u) + ramp(hit.d - hw, wall, self.round)
    }

    #[inline]
    pub fn apply(&self, p: Vec2, z: f64) -> f64 {
        if p.x < self.bbox.0.x || p.x > self.bbox.1.x
            || p.y < self.bbox.0.y || p.y > self.bbox.1.y
        {
            return z;
        }
        smin(z, self.surface(p), self.k)
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
}

impl ResolvedRidge {
    /// `base_ref` = a reference base elevation (config base) the emphasis
    /// profile fades the crest toward at the spine ends.
    pub fn new(r: &Ridge, spine: Spine, base_ref: f64) -> Self {
        let hw_max = r.crest_halfwidth.knots.iter().map(|k| k.1).fold(0.0, f64::max);
        let flank_min = r.flank_grad_left.min(r.flank_grad_right).max(0.05);
        let pad = hw_max + 300.0 / flank_min + r.crest_round_m + 50.0;
        let bbox = spine.bbox(pad);
        ResolvedRidge {
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

    #[inline]
    pub fn apply(&self, p: Vec2, z: f64) -> f64 {
        if p.x < self.bbox.0.x || p.x > self.bbox.1.x
            || p.y < self.bbox.0.y || p.y > self.bbox.1.y
        {
            return z;
        }
        smax(z, self.surface(p), self.k)
    }
}
