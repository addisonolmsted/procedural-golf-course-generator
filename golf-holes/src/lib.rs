//! `golf-holes`: hole build-out over a routed course.
//!
//! Turns each routed line of play into a built hole: an organic strategic
//! fairway boundary + subtle contouring, a strategic green shape with
//! enforced-pinnable contouring and a single pin, and an elliptical built-up
//! tee pad. Hazards and trees are later phases — this crate records the hooks
//! they need (outlines, landing zones, zone queries).
//!
//! Consumes `CourseTerrain` + `Routing` read-only. The composed playing
//! surface is `CourseBuild::surface_at`: the base terrain everywhere, locally
//! replaced by fine absolute-surface patches (fairways 2 m, tees/greens
//! 0.5 m) blended by feathered weights — bit-exact at any query resolution,
//! and bit-identical to `CourseTerrain::height_at` outside patch bounds.
//! Deterministic on one DetRng stream (`b"holes-v1"`), fixed iteration
//! counts, no map iteration.

pub mod bunker;
pub mod fairway;
pub mod green;
pub mod tee;
pub mod water_fit;

use golf_core::math::{self, Vec2};
use golf_core::spline::SpineCurve;
use golf_core::{Grid, GridSpec};
use golf_routing::Routing;
use golf_terrain::CourseTerrain;

pub const HOLES_VERSION: u32 = 4;

/// Ground classification at a point (mowing zones + sand).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Zone {
    Green(u8),
    Fringe(u8),
    Tee(u8),
    Bunker(u8),
    Fairway(u8),
    Rough,
}

/// A fine absolute-surface override: `z` is the target surface, `w` the
/// blend weight (1 = replace base, 0 = base). Weights feather to 0 well
/// inside the raster bounds so patch edges are seamless.
#[derive(Clone, Debug)]
pub struct Patch {
    pub z: Grid<f64>,
    pub w: Grid<f64>,
}

impl Patch {
    #[inline]
    pub fn contains(&self, p: Vec2) -> bool {
        let s = self.z.spec;
        let x1 = s.origin.x + (s.nx - 1) as f64 * s.cell_size;
        let y1 = s.origin.y + (s.ny - 1) as f64 * s.cell_size;
        p.x >= s.origin.x && p.x <= x1 && p.y >= s.origin.y && p.y <= y1
    }
}

/// Grid spec covering `[min, max]` at `cell` resolution (node-centered).
pub(crate) fn patch_spec(min: Vec2, max: Vec2, cell: f64) -> GridSpec {
    let nx = (((max.x - min.x) / cell).ceil() as u32 + 2).max(2);
    let ny = (((max.y - min.y) / cell).ceil() as u32 + 2).max(2);
    GridSpec::new(min, cell, nx, ny)
}

/// The elliptical built-up tee platform.
#[derive(Clone, Copy, Debug)]
pub struct TeePad {
    pub center: Vec2,
    /// Bearing of the opening shot (long axis).
    pub rot: f64,
    /// Semi-axes: `a` along the shot, `b` across.
    pub a: f64,
    pub b: f64,
    /// Flat platform elevation.
    pub z: f64,
}

impl TeePad {
    /// Normalized radial coordinate: ≤1 inside the ellipse.
    pub fn rnorm(&self, p: Vec2) -> f64 {
        let d = p - self.center;
        let (s, c) = (math::sin(-self.rot), math::cos(-self.rot));
        let q = Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c);
        math::hypot(q.x / self.a, q.y / self.b)
    }
}

/// A green boundary: a rotated, scaled superellipse modulated by petal terms
/// — one evaluator covers ellipse / rounded-square / petal / kidney.
///
/// Normalized boundary radius:
/// `ρ(θ) = (|cosθ|^n + |sinθ|^n)^(−1/n) · (1 + Σᵢ cᵢ·cos(iθ + φᵢ))`, i = 1..4.
#[derive(Clone, Debug)]
pub struct GreenShape {
    pub center: Vec2,
    /// Long-axis orientation.
    pub rot: f64,
    /// Semi-axes (long, short), meters.
    pub a: f64,
    pub b: f64,
    /// Superellipse exponent (2 = ellipse, 3–4 = rounded square).
    pub n: f64,
    /// Petal terms `(cᵢ, φᵢ)` for i = 1..=4 (zeroes for pure conics).
    pub coefs: [(f64, f64); 4],
    /// Boundary polygon (sampled; viz + distance tests).
    pub outline: Vec<Vec2>,
    pub area: f64,
    /// Direction from the green center toward an adjacent hazard, if any
    /// (shapes bend away from it; pins gravitate toward it).
    pub guard_dir: Option<Vec2>,
}

impl GreenShape {
    /// Normalized boundary radius at local angle θ.
    pub fn rho(&self, theta: f64) -> f64 {
        // n = 2 is the plain ellipse: the superellipse factor is exactly 1
        // (and libm::pow is the hot path in patch rasterization).
        let se = if self.n == 2.0 {
            1.0
        } else {
            let c = math::cos(theta).abs();
            let s = math::sin(theta).abs();
            math::pow(math::pow(c, self.n) + math::pow(s, self.n), -1.0 / self.n)
        };
        let mut m = 1.0;
        for (i, &(ci, phi)) in self.coefs.iter().enumerate() {
            if ci != 0.0 {
                m += ci * math::cos((i as f64 + 1.0) * theta + phi);
            }
        }
        se * m.max(0.35)
    }

    /// Local normalized polar coords of a world point: (r, θ) with the
    /// boundary at `r = ρ(θ)`.
    pub fn local(&self, p: Vec2) -> (f64, f64) {
        let d = p - self.center;
        let (s, c) = (math::sin(-self.rot), math::cos(-self.rot));
        let q = Vec2::new(d.x * c - d.y * s, d.x * s + d.y * c);
        let e = Vec2::new(q.x / self.a, q.y / self.b);
        (e.length(), math::atan2(e.y, e.x))
    }

    /// Inside test: normalized radius relative to the boundary (≤1 inside).
    pub fn rnorm(&self, p: Vec2) -> f64 {
        let (r, th) = self.local(p);
        r / self.rho(th).max(1e-9)
    }

    /// World boundary point at local angle θ.
    pub fn boundary(&self, theta: f64) -> Vec2 {
        let rho = self.rho(theta);
        let q = Vec2::new(self.a * rho * math::cos(theta), self.b * rho * math::sin(theta));
        let (s, c) = (math::sin(self.rot), math::cos(self.rot));
        self.center + Vec2::new(q.x * c - q.y * s, q.x * s + q.y * c)
    }

    /// Meters from `p` to the boundary polygon (positive inside or outside).
    pub fn boundary_dist(&self, p: Vec2) -> f64 {
        let mut d = f64::INFINITY;
        for w in self.outline.windows(2) {
            d = d.min(golf_routing::geometry::point_seg_dist(p, w[0], w[1]));
        }
        if let (Some(&first), Some(&last)) = (self.outline.first(), self.outline.last()) {
            d = d.min(golf_routing::geometry::point_seg_dist(p, last, first));
        }
        d
    }
}

/// One built hole.
#[derive(Clone, Debug)]
pub struct HoleBuild {
    /// Smooth Catmull-Rom line of play (arc-length parameterized).
    pub spine: SpineCurve,
    /// Fairway boundary: dense C¹ width knots per side with semicircular
    /// end caps (empty for surround-only par 3s).
    pub fw: fairway::FairwayWidth,
    /// Sampled boundary polygon (closed; viz + later hazard/tree phases).
    pub fairway_outline: Vec<Vec2>,
    /// Conservative bbox of the fairway (+margin) for query prefilter.
    pub fairway_bbox: (Vec2, Vec2),
    /// Landing zones `(s, world)` — width knots + bunkering hooks.
    pub landing: Vec<(f64, Vec2)>,
    /// Par-3 strategic fairway-cut lobes around the green (reception +
    /// runoff collection).
    pub surround: Option<green::GreenSurround>,
    /// Strategic sand (greenside, landing zones, dogleg liners, pots).
    pub bunkers: Vec<bunker::Bunker>,
    pub green: GreenShape,
    pub tee: TeePad,
    pub pin: Vec2,
    pub pin_tier: u8,
    /// Realized pinnable share of the green area (enforced ≥ 0.18).
    pub pinnable_share: f64,
}

impl HoleBuild {
    /// Effective fairway half-width at `s` for a side — the same C¹ formula
    /// as the outline and the patch mask (caps included).
    pub fn half_width(&self, s: f64, left: bool) -> f64 {
        self.fw.half_width(self.spine.length(), s, left)
    }

    /// Is `p` on this hole's fairway ribbon?
    pub fn on_fairway(&self, p: Vec2) -> bool {
        if self.fw.is_none() {
            return false;
        }
        let (lo, hi) = self.fairway_bbox;
        if p.x < lo.x || p.x > hi.x || p.y < lo.y || p.y > hi.y {
            return false;
        }
        let (u, v) = self.spine.project(p);
        v.abs() <= self.half_width(u, v >= 0.0)
    }

    /// Is `p` on this hole's green surround (the par-3 offset petal)?
    /// The mown collar is the surround shape minus the green + fringe
    /// (which win in the zone ordering).
    pub fn on_surround(&self, p: Vec2) -> bool {
        match &self.surround {
            Some(su) => su.shape.rnorm(p) <= 1.0,
            None => false,
        }
    }
}

/// The built course: 9 holes + the fine surface patches, in apply order
/// (fairways, then tee pads, then greens — later wins under its weight),
/// plus the course-refined water outlines (the terrain suggests; the build
/// decides near play).
#[derive(Clone, Debug)]
pub struct CourseBuild {
    pub holes: Vec<HoleBuild>,
    pub patches: Vec<Patch>,
    pub water: water_fit::WaterRefine,
}

/// Fringe band width outside the green boundary, meters.
pub const FRINGE_M: f64 = 2.5;

impl CourseBuild {
    /// The playing surface: base terrain with built patches blended in.
    pub fn surface_at(&self, ct: &CourseTerrain, p: Vec2) -> f64 {
        let mut h = ct.height_at(p);
        for patch in &self.patches {
            if patch.contains(p) {
                let w = patch.w.bilinear(p).clamp(0.0, 1.0);
                if w > 0.0 {
                    h += (patch.z.bilinear(p) - h) * w;
                }
            }
        }
        h
    }

    /// Mowing-zone classification (greens > tees > fairways > rough).
    pub fn zone_at(&self, p: Vec2) -> Zone {
        for (i, h) in self.holes.iter().enumerate() {
            let r = h.green.rnorm(p);
            if r <= 1.0 {
                return Zone::Green(i as u8);
            }
            if r <= 1.0 + FRINGE_M / h.green.b.min(h.green.a) {
                return Zone::Fringe(i as u8);
            }
        }
        for (i, h) in self.holes.iter().enumerate() {
            if h.tee.rnorm(p) <= 1.0 {
                return Zone::Tee(i as u8);
            }
        }
        for (i, h) in self.holes.iter().enumerate() {
            if h.bunkers.iter().any(|bk| bk.contains(p)) {
                return Zone::Bunker(i as u8);
            }
        }
        for (i, h) in self.holes.iter().enumerate() {
            if h.on_surround(p) {
                return Zone::Fairway(i as u8);
            }
        }
        for (i, h) in self.holes.iter().enumerate() {
            if h.on_fairway(p) {
                return Zone::Fairway(i as u8);
            }
        }
        Zone::Rough
    }
}

/// Build the holes over a routed course. Deterministic in `(ct, r, seed)`.
pub fn build(ct: &CourseTerrain, r: &Routing, seed: u64) -> CourseBuild {
    let mut rng = golf_core::det::DetRng::new(seed, b"holes-v1");
    // Course-level draw: the rare mound-cluster hole (Pinehurst-10 moment).
    let mound_hole: Option<usize> = if rng.next_f64() < 0.04 {
        Some(rng.below(9))
    } else {
        None
    };

    // ---- Pass 1: geometry against the terrain's water (the suggestion).
    // Fairways are realized once here so the water refinement can see where
    // they WANT to be.
    let dblock0 = fairway::blocking_dist(ct);
    struct Geo {
        spine: golf_core::spline::SpineCurve,
        green: GreenShape,
        surround: Option<green::GreenSurround>,
        tee: TeePad,
        draws: fairway::FwDraws,
    }
    let mut geos: Vec<Geo> = Vec::with_capacity(9);
    let mut prelim: Vec<HoleBuild> = Vec::with_capacity(9);
    for hole in &r.holes {
        let spine = fairway::smooth_spine(&hole.pts);
        let gr = green::shape(ct, hole, &spine, &dblock0, &mut rng);
        let dir_in = spine.tangent_at(1.0);
        let surround = if hole.par == 3 {
            Some(green::surround(ct, &gr, dir_in, &dblock0, &mut rng))
        } else {
            None
        };
        let draws = fairway::draw_params(hole, &spine, &gr, &mut rng);
        let te = tee::shape(ct, hole, &mut rng);
        let fws = fairway::realize(hole, &spine, &draws, &dblock0);
        prelim.push(HoleBuild {
            spine: fws.spine,
            fw: fws.fw,
            fairway_outline: fws.outline,
            fairway_bbox: fws.bbox,
            landing: fws.landing,
            surround: surround.clone(),
            bunkers: Vec::new(),
            green: gr.clone(),
            tee: te,
            pin: gr.center,
            pin_tier: 0,
            pinnable_share: 0.0,
        });
        geos.push(Geo {
            spine,
            green: gr,
            surround,
            tee: te,
            draws,
        });
    }

    // ---- Water refinement: trim fairway intrusions (unless cross hazards),
    // melt wacky lobes near holes. The refined water then frees ground the
    // preliminary fairways had ceded — realize again so they take it back.
    let water = water_fit::refine(ct, &prelim);
    let dblock1 = water_fit::refined_blocking_dist(ct, &water);

    // ---- Pass 2: final fairways + contouring + greens/pins + bunkers.
    let mut holes: Vec<HoleBuild> = Vec::with_capacity(9);
    let mut fw_patches: Vec<Patch> = Vec::with_capacity(9);
    let mut tee_patches: Vec<Patch> = Vec::with_capacity(9);
    let mut green_patches: Vec<Patch> = Vec::with_capacity(9);
    for (i, hole) in r.holes.iter().enumerate() {
        let g = &geos[i];
        let fws = fairway::realize(hole, &g.spine, &g.draws, &dblock1);
        let fwp = fairway::patch(ct, &fws, mound_hole == Some(i), &mut rng);
        let tep = tee::patch(ct, &g.tee);
        let (grp, pin, pin_tier, pinnable_share) =
            green::patch_and_pin(ct, &g.green, &fws, &mut rng);
        let dir_in = g.spine.tangent_at(1.0);
        let bunkers = bunker::place(
            hole, &g.spine, &fws.fw, &g.green, &g.tee, dir_in, &dblock1, &mut rng,
        );

        fw_patches.push(fwp);
        tee_patches.push(tep);
        green_patches.push(grp);
        holes.push(HoleBuild {
            spine: fws.spine,
            fw: fws.fw,
            fairway_outline: fws.outline,
            fairway_bbox: fws.bbox,
            landing: fws.landing,
            surround: g.surround.clone(),
            bunkers,
            green: g.green.clone(),
            tee: g.tee,
            pin,
            pin_tier,
            pinnable_share,
        });
    }

    let mut patches = fw_patches;
    patches.append(&mut tee_patches);
    patches.append(&mut green_patches);
    CourseBuild {
        holes,
        patches,
        water,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use golf_terrain::{generate_course, macro_spec};

    fn built(seed: u64) -> Option<(CourseTerrain, Routing, CourseBuild)> {
        let (ct, _) = generate_course(&macro_spec(), seed);
        let r = golf_routing::route(&ct, seed).ok()?;
        let b = build(&ct, &r, seed);
        Some((ct, r, b))
    }

    #[test]
    fn build_is_deterministic() {
        let (ct, r, a) = built(2024).expect("routes");
        let b = build(&ct, &r, 2024);
        assert_eq!(a.holes.len(), b.holes.len());
        for (x, y) in a.holes.iter().zip(&b.holes) {
            assert_eq!(x.pin.x.to_bits(), y.pin.x.to_bits());
            assert_eq!(x.pin.y.to_bits(), y.pin.y.to_bits());
            assert_eq!(x.fairway_outline.len(), y.fairway_outline.len());
        }
        for (p, q) in a.patches.iter().zip(&b.patches) {
            assert_eq!(p.z.data.len(), q.z.data.len());
            assert!(p.z.data.iter().zip(&q.z.data).all(|(u, v)| u.to_bits() == v.to_bits()));
            assert!(p.w.data.iter().zip(&q.w.data).all(|(u, v)| u.to_bits() == v.to_bits()));
        }
    }

    #[test]
    fn surface_matches_base_outside_patches() {
        let (ct, _r, b) = built(2024).expect("routes");
        // Points far outside the playable box are untouched by construction.
        for &(x, y) in &[(60.0, 60.0), (1950.0, 80.0), (100.0, 1900.0), (1980.0, 1960.0)] {
            let p = Vec2::new(x, y);
            assert!(!b.patches.iter().any(|pa| pa.contains(p)));
            assert_eq!(b.surface_at(&ct, p).to_bits(), ct.height_at(p).to_bits());
        }
    }

    #[test]
    fn every_hole_has_pin_inside_green_and_zones_agree() {
        for seed in [2024u64, 7, 11] {
            let Some((_ct, _r, b)) = built(seed) else { continue };
            for (i, h) in b.holes.iter().enumerate() {
                assert!(h.green.rnorm(h.pin) < 1.0, "seed {seed} hole {} pin outside", i + 1);
                assert!(
                    h.green.boundary_dist(h.pin) >= 2.6,
                    "seed {seed} hole {} pin too close to edge",
                    i + 1
                );
                assert_eq!(b.zone_at(h.pin), Zone::Green(i as u8));
                assert_eq!(b.zone_at(h.tee.center), Zone::Tee(i as u8));
            }
        }
    }
}
