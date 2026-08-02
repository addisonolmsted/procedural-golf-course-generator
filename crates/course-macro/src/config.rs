//! The analytic macro-terrain config and its composed evaluation — ported
//! from `terrain-v2:golf-landform/src/lib.rs` (Stage 2, "the hard part,
//! solved"). NO noise layer, NO erosion — those are steps 04/05.
//!
//! The composition order is structural: tilt (base field) → ridges
//! (smooth-max) → bluffs (additive step) → bowls (smooth-min) → valleys LAST
//! (smooth-min with the swept valley surface), so drainage carves through
//! everything and valley floors stay strictly monotone.
//!
//! Determinism: every stochastic choice (meander phase/jitter, bowl-rim
//! wobble) is keyed off explicit seeds inside the config; evaluation is pure
//! f64 arithmetic + libm via course-world. Same config + resolution → the
//! same bits on any platform. One code path at any grid step.

use course_world::grid::{Grid, GridSpec};
use course_world::math::Vec2;
use course_world::spline::{catmull_rom, Spine};
use serde::{Deserialize, Serialize};

use crate::prims::{bluff, bowl, meander, valley};
use crate::prims::bluff::Bluff;
use crate::prims::bowl::Bowl;
use crate::prims::meander::MeanderSpec;
use crate::prims::valley::{Ridge, Valley, MIN_FALL_GRADIENT};

/// Where a swept primitive's centerline comes from.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Path {
    /// Explicit control points → Catmull-Rom → arc-length spine.
    Points(Vec<Vec2>),
    /// Generated meander between two endpoints (see [`crate::prims::meander`]).
    Meander(MeanderSpec),
}

impl Path {
    pub(crate) fn to_spine(&self) -> Spine {
        match self {
            Path::Points(pts) => Spine::new(catmull_rom(pts, 10.0)),
            Path::Meander(spec) => meander::generate(spec),
        }
    }
}

/// Planar / low-order regional gradient — the base field.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tilt {
    /// Grade (rise/run) along +x and +y, about the window center.
    pub grade_x: f64,
    pub grade_y: f64,
    /// Low-order curvature: elevation delta from window corners to center
    /// (positive = dome, negative = basin). 0 = planar.
    pub curve_m: f64,
    /// Fraction of the regional grade retained ACROSS the routable core,
    /// along the downhill axis. 1 = a pure plane (the default, and what
    /// every existing config round-trips to).
    ///
    /// Below 1 the core becomes a flatter tread and the drop it sheds is
    /// added back to the two outer bands, so the TOTAL box drop — and
    /// therefore `relief_amp_m`, a FITTED knob — is conserved. That is the
    /// difference between this and scaling the grade: scaling destroys a
    /// fitted quantity in service of `core_relief_cap_m`, a design one.
    /// Geometrically it is the same thing as a bench tread.
    #[serde(default = "d_core_tread")]
    pub core_tread: f64,
}

fn d_core_tread() -> f64 {
    1.0
}

/// Hand-written so the default is a PURE PLANE. `#[derive(Default)]` would
/// give `core_tread: 0.0` — "shed the entire regional grade across the
/// core" — which is the opposite of the documented meaning and only ever
/// looked harmless because the derived grades are zero too.
impl Default for Tilt {
    fn default() -> Self {
        Tilt {
            grade_x: 0.0,
            grade_y: 0.0,
            curve_m: 0.0,
            core_tread: d_core_tread(),
        }
    }
}

/// Half-width of the smoothstep that joins the core tread to the outer
/// band, so the grade change is a curve rather than a crease.
const TREAD_KNEE_M: f64 = 150.0;

impl Tilt {
    pub(crate) fn eval(&self, p: Vec2, base: f64, extent: f64) -> f64 {
        let c = 0.5 * extent;
        let dx = p.x - c;
        let dy = p.y - c;
        let r2_corner = 2.0 * c * c;
        let curve = self.curve_m * (1.0 - (dx * dx + dy * dy) / r2_corner);
        if self.core_tread >= 1.0 {
            return base + self.grade_x * dx + self.grade_y * dy + curve;
        }
        base + self.tread_profile(dx, dy) + curve
    }

    /// Drop along the downhill axis with a flattened core tread.
    ///
    /// `t` is the projection onto the grade direction. The inner band (the
    /// core) uses `core_tread` of the regional slope and the outer bands
    /// take the rest, chosen so the end-to-end drop equals the pure plane's
    /// — the tread REDISTRIBUTES grade rather than removing it, which is why
    /// `relief_amp_m` survives.
    ///
    /// The profile is built by INTEGRATING a blended gradient, not by
    /// blending two elevation branches. That distinction is load-bearing.
    /// Blending elevations looks equivalent — the branches agree at the knee
    /// — but its derivative picks up a `(z_out - z_in)·w'` term that is
    /// negative just inside the knee, and with a strong tread it dominates:
    /// at `core_tread` 0.1 the outer band runs at 1.9x the regional grade,
    /// and the profile measurably REVERSED, scalloping a dip into the core
    /// edge. Piedmont seed 7 got 58.5 -> 59.2 m of core relief out of being
    /// treated, i.e. the flattening lever made the surface less flat.
    ///
    /// Integrating the gradient cannot do that: the integrand is
    /// `inner + (outer - inner)·smoothstep(...)`, which never drops below
    /// `inner > 0`, so the profile is strictly monotone (drains keep
    /// falling) and C1 (no crease). The drop still comes out exact, because
    /// a smoothstep is antisymmetric about its midpoint and so integrates
    /// over the knee to exactly `TREAD_KNEE_M` — which is what makes `outer`
    /// the same expression as the elevation-blend version used.
    fn tread_profile(&self, dx: f64, dy: f64) -> f64 {
        let g = (self.grade_x * self.grade_x + self.grade_y * self.grade_y).sqrt();
        if g <= 1e-12 {
            return self.grade_x * dx + self.grade_y * dy;
        }
        let (ux, uy) = (self.grade_x / g, self.grade_y / g);
        let t = dx * ux + dy * uy;
        // Half-ranges of the projection across the box and across the core.
        // Both scale with |ux|+|uy|, so `half == 2*core` in every direction —
        // a diagonal tilt simply sees a longer core diagonal (up to 2121 m,
        // not 1500), which is why a diagonal grade puts more relief in the
        // core than the box width suggests.
        let half = 1500.0 * (ux.abs() + uy.abs());
        let core = 750.0 * (ux.abs() + uy.abs());
        let inner = self.core_tread.clamp(0.0, 1.0);
        let outer = if half > core + 1.0 {
            (half - inner * core) / (half - core)
        } else {
            inner
        };
        let a = t.abs();
        g * t.signum() * (inner * a + (outer - inner) * Self::knee_integral(a, core))
    }

    /// `∫₀^a smoothstep(core - K, core + K, s) ds`, in closed form.
    ///
    /// Equals `a - core` once past the knee, and exactly `TREAD_KNEE_M` over
    /// the knee itself — the antisymmetry that conserves the total drop.
    #[inline]
    fn knee_integral(a: f64, core: f64) -> f64 {
        let k = TREAD_KNEE_M;
        let lo = core - k;
        if a <= lo {
            return 0.0;
        }
        if a >= core + k {
            return a - core;
        }
        let u = (a - lo) / (2.0 * k);
        2.0 * k * (u * u * u - 0.5 * u * u * u * u)
    }
}

/// The single versioned macro-terrain config. Emitted as the `config.json`
/// advisory sidecar of the step-03 artifact (re-rasterizable, hand-editable
/// in the lab); the rasterized `MacroSkeleton` is the contract downstream.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MacroConfig {
    #[serde(default = "d_schema")]
    pub schema_version: u32,
    /// Square world edge length, meters.
    pub extent_m: f64,
    /// Reference base elevation, meters.
    pub base_elev_m: f64,
    #[serde(default)]
    pub tilt: Tilt,
    /// Feature-relief weight at the middle of the routable core, eased back
    /// to 1 by the core shoulder (see [`Resolved::core_weight`]). 1 = the
    /// config as authored. Set by the budget solve; it lives HERE rather
    /// than only on `Resolved` because `config.json` is advertised as
    /// re-rasterizable — a knob that shapes the surface but does not
    /// serialize would make the sidecar produce a different heightfield.
    #[serde(default = "d_core_tread")]
    pub core_relax: f64,
    /// Whether `core_relax` also weights the VALLEYS.
    ///
    /// Normally it must not: a valley's floor lies along its own arc, and
    /// easing the carve where the floor is inside the core would raise that
    /// reach relative to its neighbours — a ponded drain. So the rule is
    /// "valleys are never weighted; their incision is placement-capped".
    ///
    /// The exception is exact rather than approximate. When every drain is
    /// routed OUTSIDE the core (a tangent-routed trunk, whose tributaries
    /// the planner forces to the outer side), no floor point lies in the
    /// core or its shoulder, so `core_weight` is exactly 1 everywhere along
    /// every floor and the blend can only touch WALLS. Incision is still
    /// uncapped by this; what it bounds is how far a wall reaches in.
    ///
    /// That reach is what the placement cap never bounded. Measured: a
    /// mountain trunk 368 m outside the core still cut 38.9 m below the
    /// terrain at the core corner, because the wall climbs at the fitted
    /// `valley_wall_grade` (0.10-0.15) while a bench scarp face beside it
    /// climbs at 0.35-0.75 — the wall can never catch up, at any cap. This
    /// is not fixable by raising the cap: incision is a share OF the cap, so
    /// a bigger cap digs a deeper valley (measured: mountain goes from 17/40
    /// to 28/40 over-cap seeds as the cap is raised 1.6x).
    #[serde(default)]
    pub core_relax_valleys: bool,
    /// Applied in this order (the composition rule); valleys always last.
    #[serde(default)]
    pub ridges: Vec<Ridge>,
    #[serde(default)]
    pub bluffs: Vec<Bluff>,
    #[serde(default)]
    pub bowls: Vec<Bowl>,
    #[serde(default)]
    pub valleys: Vec<Valley>,
}

fn d_schema() -> u32 {
    crate::SCHEMA_VERSION
}

impl MacroConfig {
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).expect("config serializes")
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }
}

/// The resolved (spine-built, junction-snapped) form — build once, evaluate
/// at any resolution.
pub struct Resolved {
    extent: f64,
    base: f64,
    tilt: Tilt,
    /// Relief weight at the centre of the routable core: 1 = as authored,
    /// 0 = the core contributes no feature relief at all. Mirrors
    /// [`MacroConfig::core_relax`].
    core_relax: f64,
    /// Mirrors [`MacroConfig::core_relax_valleys`].
    relax_valleys: bool,
    pub(crate) ridges: Vec<valley::ResolvedRidge>,
    pub(crate) bluffs: Vec<bluff::ResolvedBluff>,
    pub(crate) bowls: Vec<bowl::ResolvedBowl>,
    pub(crate) valleys: Vec<valley::ResolvedValley>,
}

/// Build spines, snap tributary junctions (accordant floors), precompute
/// bboxes. Panics on malformed configs (join_trunk pointing forward/at self).
pub fn resolve(cfg: &MacroConfig) -> Resolved {
    let ridges = cfg
        .ridges
        .iter()
        .map(|r| valley::ResolvedRidge::new(r, r.path.to_spine(), cfg.base_elev_m))
        .collect();
    let bluffs = cfg
        .bluffs
        .iter()
        .map(|b| bluff::ResolvedBluff::new(b, b.path.to_spine()))
        .collect();
    let bowls = cfg.bowls.iter().map(bowl::ResolvedBowl::new).collect();

    let mut valleys: Vec<valley::ResolvedValley> = Vec::with_capacity(cfg.valleys.len());
    for (i, v) in cfg.valleys.iter().enumerate() {
        let mut spine = v.path.to_spine();
        let mut rv_cfg = v.clone();
        let mut arc_cap = None;
        if let Some(t) = v.join_trunk {
            assert!(
                t < i,
                "join_trunk must reference an EARLIER valley (got {t} for valley {i})"
            );
            let trunk = &valleys[t];
            // Snap the tributary's end onto the trunk centerline. Trailing
            // points that already wandered inside the trunk's corridor are
            // trimmed first, so the final approach is a clean segment into the
            // junction instead of a hook along the trunk wall.
            let end = *spine.pts.last().unwrap();
            let hit = trunk.spine.project(end);
            let junction = trunk.spine.point_at(hit.u);
            let trunk_hw = trunk.hw.sample(hit.u);
            let trib_hw = v
                .floor_halfwidth
                .knots
                .iter()
                .map(|k| k.1)
                .fold(0.0, f64::max);
            let clear = trunk_hw + trib_hw + 20.0;
            let mut pts = spine.pts.clone();
            while pts.len() > 2 && trunk.spine.project(*pts.last().unwrap()).d < clear {
                pts.pop();
            }
            // approach segment into the junction…
            let prev = *pts.last().unwrap();
            pts.push(junction);
            // …then OVERSHOOT across the trunk floor so the tributary's radial
            // end-cap lands inside the trunk's carved volume (an open mouth,
            // not a blunt spoon). The floor holds the junction elevation over
            // the overshoot (arc_cap), so nothing cuts below the trunk floor.
            let dirn = (junction - prev).normalized();
            let overshoot = trunk_hw + rv_cfg.floor_round_m + 10.0;
            let arc_to_junction = {
                let mut total = 0.0;
                for w in pts.windows(2) {
                    total += w[0].distance(w[1]);
                }
                total
            };
            pts.push(junction + dirn * overshoot);
            spine = Spine::new(pts);
            arc_cap = Some(arc_to_junction);
            // Accordant floor: tributary floor at the JUNCTION equals the
            // trunk floor there, rising upstream by the tributary's own
            // cumulative drop over that arc — which is the constant-gradient
            // product only when `fall_profile` is None.
            let grad = v.fall_gradient.max(MIN_FALL_GRADIENT);
            rv_cfg.fall_gradient = grad;
            let rise = valley::cumulative_drop(&rv_cfg, spine.length(), arc_to_junction);
            rv_cfg.floor_z0_m = trunk.floor_z(hit.u) + rise;
        }
        valleys.push(valley::ResolvedValley::with_cap(&rv_cfg, spine, arc_cap));
    }

    Resolved {
        core_relax: cfg.core_relax.clamp(0.0, 1.0),
        relax_valleys: cfg.core_relax_valleys,
        extent: cfg.extent_m,
        base: cfg.base_elev_m,
        tilt: cfg.tilt,
        ridges,
        bluffs,
        bowls,
        valleys,
    }
}

impl Resolved {
    /// Composed elevation at a world point — the single source of truth
    /// (grids at every resolution sample exactly this).
    pub fn height_at(&self, p: Vec2) -> f64 {
        // One core weight per point, hoisted out of the primitive loops.
        // 1 everywhere unless the budget solve relaxed the core.
        let w = self.core_weight(p);
        // Valleys are weighted like everything else now. The old rule —
        // "never weight a valley unless every floor is outside the core" —
        // existed because the lever BLENDED THE COMPOSED SURFACE, which lifts
        // a floor that lies inside the core and ponds the drain. The lever
        // now scales the WALL RISE, which is exactly zero at the channel, so
        // floors cannot move whatever the weight is
        // (`core_weight_tests::weighted_rise_never_moves_the_floor`).
        //
        // Retiring the precondition is not a nicety: a grown network puts
        // floors inside the core on essentially every seed, so the gate was
        // permanently false and the core relax had no effect at all.
        let mut z = self.tilt.eval(p, self.base, self.extent);
        for r in &self.ridges {
            z = r.apply(p, z, w);
        }
        for b in &self.bluffs {
            z = b.apply(p, z, w);
        }
        for b in &self.bowls {
            z = b.apply(p, z, w);
        }
        for v in &self.valleys {
            z = v.apply(p, z, w);
        }
        z
    }

    /// Per-point relief weight: 1 = the plan as authored, `core_relax` at
    /// the middle of the routable core, eased over the core shoulder.
    ///
    /// This is the world-space handle the budget solve turns. The previous
    /// solve scaled feature amplitudes and the regional tilt GLOBALLY, which
    /// flattened the entire 3 km box to satisfy a cap that only governs the
    /// central 1.5 km — costing 0.2-0.6x of real whole-tile relief. Because
    /// `core_protect_at` is a product of smoothsteps, the weight is C1, so
    /// no seam appears where it varies.
    #[inline]
    pub fn core_weight(&self, p: Vec2) -> f64 {
        if self.core_relax >= 1.0 {
            return 1.0;
        }
        1.0 - (1.0 - self.core_relax) * crate::fields::core_protect_at(p)
    }

    /// Valley floor elevation along valley `i` at normalized arc `u`
    /// (for monotonicity property tests + the emitted drain spines).
    pub fn valley_floor(&self, i: usize, u: f64) -> (Vec2, f64) {
        let v = &self.valleys[i];
        (v.spine.point_at(u), v.floor_z(u))
    }

    pub fn n_valleys(&self) -> usize {
        self.valleys.len()
    }

    /// The resolved (junction-snapped) centerline spine of valley `i`.
    pub fn valley_spine(&self, i: usize) -> &Spine {
        &self.valleys[i].spine
    }

    /// Arc position (meters) past which valley `i`'s floor stops descending
    /// (the junction cap of a joined tributary), if any.
    pub fn valley_arc_cap(&self, i: usize) -> Option<f64> {
        self.valleys[i].arc_cap_m
    }

    /// Valley `i`'s own swept surface at `p`, before composition — for
    /// attributing which reach scours a trunk.
    pub fn valley_surface_at(&self, i: usize, p: Vec2) -> f64 {
        self.valleys[i].surface(p)
    }

    /// Floor half-width of valley `i` at normalized arc `u`.
    pub fn valley_halfwidth(&self, i: usize, u: f64) -> f64 {
        self.valleys[i].hw.sample(u)
    }

    pub fn n_ridges(&self) -> usize {
        self.ridges.len()
    }

    pub fn ridge_spine(&self, i: usize) -> &Spine {
        &self.ridges[i].spine
    }

    pub fn n_bluffs(&self) -> usize {
        self.bluffs.len()
    }

    pub fn bluff_spine(&self, i: usize) -> &Spine {
        &self.bluffs[i].spine
    }
}

/// Generate the composed field on a pixel-centered square grid at `res_m`.
/// (The step-03 skeleton raster instead uses the node-convention
/// [`course_world::world_spec`] — see [`crate::raster`]; both sample the same
/// `height_at`.)
pub fn generate(cfg: &MacroConfig, res_m: f64) -> Grid<f64> {
    let r = resolve(cfg);
    let n = (cfg.extent_m / res_m).round().max(2.0) as u32;
    let spec = GridSpec::new(Vec2::new(res_m / 2.0, res_m / 2.0), res_m, n, n);
    let mut g = Grid::filled(spec, 0.0f64);
    for y in 0..n {
        for x in 0..n {
            let p = spec.world_of(x, y);
            g.set(x, y, r.height_at(p));
        }
    }
    g
}

/// FNV-1a over the field bits — the golden-hash primitive.
pub fn field_hash(g: &Grid<f64>) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for v in &g.data {
        for b in v.to_bits().to_le_bytes() {
            h ^= b as u64;
            h = h.wrapping_mul(0x0000_0100_0000_01B3);
        }
    }
    h
}

#[cfg(test)]
mod tilt_tests {
    use super::*;

    fn tilt(core_tread: f64, gx: f64, gy: f64) -> Tilt {
        Tilt { grade_x: gx, grade_y: gy, curve_m: 0.0, core_tread }
    }

    /// `core_tread = 1` must be the pure plane, bit for bit — that is what
    /// keeps every existing config and golden unchanged.
    #[test]
    fn unit_tread_is_the_plain_plane() {
        let a = tilt(1.0, -0.014, 0.006);
        let b = Tilt { core_tread: 1.0, ..a };
        for i in 0..40 {
            for j in 0..40 {
                let p = Vec2::new(i as f64 * 77.0, j as f64 * 77.0);
                let plain = 100.0 + b.grade_x * (p.x - 1500.0) + b.grade_y * (p.y - 1500.0);
                assert_eq!(a.eval(p, 100.0, 3000.0).to_bits(), plain.to_bits());
            }
        }
    }

    /// The tread REDISTRIBUTES grade instead of removing it: corner-to-corner
    /// drop along the grade axis is conserved, so `relief_amp_m` (a fitted
    /// knob) survives while `core_relief_cap_m` (a design knob) is satisfied.
    #[test]
    fn tread_conserves_total_drop() {
        for (gx, gy) in [(-0.02, 0.0), (0.0, 0.03), (-0.014, 0.006), (0.01, -0.01)] {
            let plane = tilt(1.0, gx, gy);
            for ct in [0.8, 0.5, 0.2, 0.0] {
                let t = tilt(ct, gx, gy);
                // Sample the extreme points along the grade direction.
                let g = (gx * gx + gy * gy).sqrt();
                let (ux, uy) = (gx / g, gy / g);
                let far = 1500.0 * (ux.abs() + uy.abs());
                let a = Vec2::new(1500.0 + ux * far, 1500.0 + uy * far);
                let b = Vec2::new(1500.0 - ux * far, 1500.0 - uy * far);
                let drop_plane = plane.eval(a, 0.0, 3000.0) - plane.eval(b, 0.0, 3000.0);
                let drop_tread = t.eval(a, 0.0, 3000.0) - t.eval(b, 0.0, 3000.0);
                assert!(
                    (drop_plane - drop_tread).abs() < 1e-6 * drop_plane.abs().max(1.0),
                    "grade ({gx},{gy}) tread {ct}: drop {drop_tread} != plane {drop_plane}"
                );
            }
        }
    }

    /// STRONG treads must stay monotone too — the case the elevation-blend
    /// formulation failed.
    ///
    /// Blending two elevation branches puts a `(z_out - z_in)·w'` term in the
    /// derivative that is negative just inside the knee. It is invisible at a
    /// mild tread and dominant at a strong one, because the outer band runs
    /// at `2 - core_tread` of the regional grade: at 0.1 that is 1.9x, and
    /// the profile reversed, scalloping a dip into the core edge and making
    /// core relief GROW under the knob whose whole job is to shrink it.
    /// Integrating the blended gradient cannot reverse — the integrand never
    /// drops below `core_tread·g`.
    ///
    /// Swept down to the solve's `TREAD_MIN`, at a station spacing fine
    /// enough to resolve the 150 m knee.
    #[test]
    fn a_strong_tread_never_reverses_slope() {
        for ct in [0.5, 0.3, 0.2, 0.1] {
            for (gx, gy) in [(-0.02, 0.0), (-0.014, 0.006), (0.01, -0.01)] {
                let t = tilt(ct, gx, gy);
                let g = (gx * gx + gy * gy).sqrt();
                let (ux, uy) = (gx / g, gy / g);
                let far = 1500.0 * (ux.abs() + uy.abs());
                let at = |s: f64| t.eval(Vec2::new(1500.0 + ux * s, 1500.0 + uy * s), 0.0, 3000.0);
                // Direction-agnostic: every step must carry the same sign as
                // the first. A reversal is the defect, whichever way the
                // grade happens to point.
                let n = 4000;
                let step = 2.0 * far / n as f64;
                let mut prev = at(-far);
                let mut sign = 0.0f64;
                for i in 1..=n {
                    let s = -far + step * i as f64;
                    let z = at(s);
                    let d = z - prev;
                    if d.abs() > 1e-12 {
                        if sign == 0.0 {
                            sign = d.signum();
                        }
                        assert!(
                            d.signum() == sign,
                            "tread {ct} grade ({gx},{gy}): slope reverses at s={s:.0} \
                             (step {d:.6} m; knee at +/-{:.0} m)",
                            750.0 * (ux.abs() + uy.abs())
                        );
                    }
                    prev = z;
                }
                assert!(sign != 0.0, "tread {ct} grade ({gx},{gy}): flat profile");
            }
        }
    }

    /// The core really is flatter, and the surface stays monotone along the
    /// grade axis (drains must keep falling).
    #[test]
    fn tread_flattens_core_and_stays_monotone() {
        let (gx, gy) = (-0.02, 0.008);
        let t = tilt(0.3, gx, gy);
        let g = (gx * gx + gy * gy).sqrt();
        let (ux, uy) = (gx / g, gy / g);
        let far = 1500.0 * (ux.abs() + uy.abs());
        let at = |s: f64| t.eval(Vec2::new(1500.0 + ux * s, 1500.0 + uy * s), 0.0, 3000.0);
        // monotone (non-increasing as we move up-grade)
        let mut prev = at(-far);
        let n = 600;
        for i in 1..=n {
            let s = -far + 2.0 * far * i as f64 / n as f64;
            let v = at(s);
            assert!(v >= prev - 1e-9, "not monotone at s={s}: {v} < {prev}");
            prev = v;
        }
        // and the middle is genuinely gentler than the ends
        let core = 750.0 * (ux.abs() + uy.abs());
        let mid_slope = (at(core * 0.5) - at(-core * 0.5)) / core;
        let end_slope = (at(far) - at(core * 1.5)) / (far - core * 1.5);
        assert!(
            mid_slope < 0.6 * end_slope,
            "core slope {mid_slope} not flatter than outer {end_slope}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presets::{preset, presets};

    #[test]
    fn config_roundtrips_json() {
        for (_name, cfg) in presets() {
            let j = cfg.to_json();
            let back = MacroConfig::from_json(&j).unwrap();
            assert_eq!(cfg, back);
        }
    }

    #[test]
    fn generation_is_deterministic() {
        let cfg = preset("barranca").unwrap();
        let a = generate(&cfg, 20.0);
        let b = generate(&cfg, 20.0);
        assert_eq!(field_hash(&a), field_hash(&b));
    }

    #[test]
    fn junction_floors_are_accordant() {
        let cfg = preset("floodplain").unwrap();
        let r = resolve(&cfg);
        if r.n_valleys() < 2 {
            return;
        }
        // tributary (1) floor at the JUNCTION (arc cap) == trunk (0) floor
        // at the same point; the overshoot tail past it stays flat.
        let trib = &r.valleys[1];
        let cap = trib.arc_cap_m.expect("joined tributary has an arc cap");
        let u_j = cap / trib.spine.length();
        let jp = trib.spine.point_at(u_j);
        let trib_floor = trib.floor_z(u_j);
        let trunk = &r.valleys[0];
        let hit = trunk.spine.project(jp);
        assert!(hit.d < 1.0, "junction not on trunk (d = {})", hit.d);
        let trunk_floor = trunk.floor_z(hit.u);
        assert!(
            (trib_floor - trunk_floor).abs() < 1e-6,
            "floors not accordant: {trib_floor} vs {trunk_floor}"
        );
        // past the junction the tail rises gently (never below the junction)
        assert!(trib.floor_z(1.0) >= trib_floor - 1e-9);
    }
}
