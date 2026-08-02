//! Bluff: an eased elevation step across a boundary spline — one side raised
//! by `height_m`, C1 crest/toe via the smoothstep face, height tapering to
//! zero over the terminal fraction of arc length so the step vanishes cleanly
//! at its ends. A river at the toe is a composition concern: author a valley
//! along the low side and the (last-applied) carve hugs the face.

use course_world::math::Vec2;
use course_world::profile::Profile;
use serde::{Deserialize, Serialize};

use course_world::ease::{eased_step, end_taper};
use course_world::spline::Spine;
use crate::config::Path;

fn d_taper() -> f64 { 0.18 }
fn d_height_profile() -> Profile { Profile::constant(1.0) }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bluff {
    pub path: Path,
    /// Peak step height, meters.
    pub height_m: f64,
    /// Per-station height multiplier (on top of the end taper).
    #[serde(default = "d_height_profile")]
    pub height: Profile,
    /// Face gradient (rise/run): face width = height / face_grad.
    pub face_grad: f64,
    /// Terminal taper fraction of arc length at each end (0.15-0.20).
    #[serde(default = "d_taper")]
    pub taper_frac: f64,
    /// Which side of the spine (relative to its tangent) is raised.
    pub raise_left: bool,
}

pub(crate) struct ResolvedBluff {
    pub spine: Spine,
    pub height_m: f64,
    pub height: Profile,
    pub face_grad: f64,
    pub taper: f64,
    pub sign: f64,
}

impl ResolvedBluff {
    pub fn new(b: &Bluff, spine: Spine) -> Self {
        ResolvedBluff {
            spine,
            height_m: b.height_m,
            height: b.height.clone(),
            face_grad: b.face_grad.max(0.02),
            taper: b.taper_frac.clamp(0.0, 0.5),
            sign: if b.raise_left { 1.0 } else { -1.0 },
        }
    }

    #[inline]
    pub fn apply(&self, p: Vec2, z: f64, w: f64) -> f64 {
        if w <= 0.0 {
            return z;
        }
        // NO bbox prefilter: a bluff is a STEP — the raised terrace extends
        // arbitrarily far from the spine, so every cell must be evaluated
        // (a bbox cut would truncate the plateau with a hard seam).
        //
        // A `SegIndex` does not help here either, and measurably hurts: the
        // spine crosses the whole box and is queried from everywhere, so the
        // candidate set is never small and the per-query allocation dominates
        // a 300-segment scan (41 s -> 172 s when tried). The index pays off
        // only for spines queried from NEAR themselves, which is what the
        // bbox-culled primitives are. Bluff cost needs a different idea —
        // an along-spine band bound, since the end taper zeroes the step
        // past the ends.
        let hit = self.spine.project(p);
        let h = self.height_m
            * self.height.sample(hit.u).clamp(0.0, 4.0)
            * end_taper(hit.u, self.taper);
        if h == 0.0 {
            return z;
        }
        // NOT named `w`: that shadowed the core weight parameter, so a bluff
        // silently ignored the budget solve's relax and only ever responded
        // to the `w <= 0.0` early-out above. It stayed invisible while the
        // solve was global (nothing passed a fractional weight), and showed
        // up the moment M3 started bisecting: mountain core relief was
        // IDENTICAL at every relax from 0.5 down to 0.05 and then collapsed
        // at exactly 0.0 — a switch, not a blend. Five 20-25 m bench scarps
        // are most of a mountain core's relief, so the solve had no lever.
        let face_w = (h.abs() / self.face_grad).max(1.0);
        let stepped = z + h * eased_step(self.sign * hit.side * hit.d, face_w);
        crate::prims::valley::blend(z, stepped, w)
    }
}
