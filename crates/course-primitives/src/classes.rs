//! Window-class realization: the class-specific macro shapes, all built from
//! smooth primitives at >= 400 m scale so the C1 band-limit holds by
//! construction. This is where "different in kind" is made real — and what
//! the class-legibility gate (S1's exit test) judges.

use course_contracts::biome::WindowClass;
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

/// Smoothstep over [0, 1].
fn sstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Gaussian bump, width in metres.
fn bump(d: f64, width: f64) -> f64 {
    libm::exp(-(d * d) / (2.0 * width * width))
}

/// Feature-line curvature (review 2026-08-10): straight, perfectly
/// parallel class lines are an artificiality cue — real terrace fronts,
/// scarps and basin rims are arcuate. `c0` is the base curvature (a
/// constant second derivative: quadratic bow), `c1`+`phase` modulate it
/// slowly along the profile so adjacent "parallel" lines bow slightly
/// differently. Values come from the two class draws in the transcript
/// (previously reserved).
#[derive(Clone, Copy)]
pub struct LineCurve {
    pub c0: f64,
    pub c1: f64,
    pub phase: f64,
}

impl LineCurve {
    pub fn from_draws(a: f64, b: f64) -> LineCurve {
        LineCurve {
            // ±1.6e-4 /m ⇒ up to ~±180 m of bow at the tile edge
            c0: (a - 0.5) * 2.0 * 1.6e-4,
            c1: 0.8e-4,
            phase: b * std::f64::consts::TAU,
        }
    }

    /// Curvature at a profile position (metres along), slowly varying.
    fn at(&self, along: f64) -> f64 {
        self.c0
            + self.c1
                * libm::sin(along / EXTENT_M * std::f64::consts::TAU + self.phase)
    }
}

/// The class's structural relief contribution (metres, amplitude `amp`) and
/// its accommodation field value in [0, 1], for a world point.
///
/// `along` = signed coordinate toward the base-level edge (0 at the far side,
/// EXTENT at the edge); `cross` = perpendicular offset from the site axis
/// (centered at 0). Both in metres. Feature lines are bowed by `curve` —
/// the along/cross coordinates are warped before the class geometry reads
/// them, so every riser, rim and axis is arcuate rather than ruler-straight.
pub fn shape(
    class: WindowClass,
    along: f64,
    cross: f64,
    amp: f64,
    curve: LineCurve,
) -> (f64, f64) {
    let mid = EXTENT_M / 2.0;
    // Warp: along-features (risers, rims) bow with cross²; axial features
    // (valley/interfluve axes) bow with (along−mid)². Constant second
    // derivative per the review; the modulation varies it line-to-line.
    let along = along + 0.5 * curve.at(along) * cross * cross;
    let cross = cross + 0.5 * curve.at(cross + mid) * (along - mid) * (along - mid);
    match class {
        // A broad axial trough: relief dips along the axis, accommodation
        // concentrates in the floor.
        WindowClass::ValleyFloor => {
            let floor = bump(cross, 700.0);
            (-amp * floor, 0.35 + 0.6 * floor)
        }
        // The inverse: a broad axial crest, accommodation pushed off it.
        WindowClass::Interfluve => {
            let crest = bump(cross, 800.0);
            (amp * crest, 0.6 - 0.45 * crest)
        }
        // One big riser across mid-site, high side away from the edge.
        // The step CARRIES the class: benches above and below are near-
        // flat (tilt_share dropped to 0.2 — the old 0.5 ramp turned
        // "plateau / cliff / plain" into "ramp with a steeper bit", which
        // is basin_margin's silhouette; the review confused the two).
        WindowClass::EscarpmentFace => {
            let riser = sstep((along - mid + 250.0) / 500.0);
            (amp * (0.5 - riser), 0.3 + 0.4 * riser)
        }
        // The rim of a basin: the ramp DECELERATES into a terminal flat
        // and a shallow closed pocket sits at the low end — the visible
        // basin signature. (The first version was a plain monotone ramp,
        // and the blind session confused it with piedmont_slope every
        // time: a margin with no basin is just a slope.)
        WindowClass::BasinMargin => {
            let toward = sstep((along - 0.1 * EXTENT_M) / (0.55 * EXTENT_M));
            let pocket = bump(along - 0.86 * EXTENT_M, 550.0) * bump(cross, 1100.0);
            (
                amp * (0.5 - 0.9 * toward) - 0.5 * amp * pocket,
                0.3 + 0.5 * toward + 0.2 * pocket,
            )
        }
        // A plain strong ramp: the class is carried by tilt, not relief.
        WindowClass::PiedmontSlope => (0.0, 0.45),
        // Three treads stepping toward the edge, with CRISP risers
        // (~160 m) and genuinely flat treads — the blind session read the
        // old 400 m-soft staircase as a plain ramp.
        WindowClass::TerraceFlight => {
            let s = along / EXTENT_M * 3.0;
            let tread = s.floor().clamp(0.0, 2.0);
            let riser = sstep((s - tread) * (1000.0 / 320.0));
            let stepped = (tread + riser) / 3.0;
            (amp * (0.5 - stepped), 0.35 + 0.3 * (1.0 - stepped))
        }
    }
}

/// Extra tilt-drop multiplier per class (piedmont_slope is carried by tilt).
pub fn tilt_share(class: WindowClass) -> f64 {
    match class {
        WindowClass::PiedmontSlope => 0.9,
        WindowClass::EscarpmentFace => 0.2,
        WindowClass::TerraceFlight => 0.5,
        WindowClass::ValleyFloor | WindowClass::BasinMargin => 0.25,
        WindowClass::Interfluve => 0.35,
    }
}

/// World point -> (along, cross) for a unit direction toward the edge.
pub fn axial(p: Vec2, toward_edge: Vec2) -> (f64, f64) {
    let mid = EXTENT_M / 2.0;
    let rel = Vec2::new(p.x - mid, p.y - mid);
    let along = rel.x * toward_edge.x + rel.y * toward_edge.y + mid;
    let cross = -rel.x * toward_edge.y + rel.y * toward_edge.x;
    (along, cross)
}
