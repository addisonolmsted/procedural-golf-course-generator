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

/// The class's structural relief contribution (metres, amplitude `amp`) and
/// its accommodation field value in [0, 1], for a world point.
///
/// `along` = signed coordinate toward the base-level edge (0 at the far side,
/// EXTENT at the edge); `cross` = perpendicular offset from the site axis
/// (centered at 0). Both in metres.
pub fn shape(class: WindowClass, along: f64, cross: f64, amp: f64) -> (f64, f64) {
    let mid = EXTENT_M / 2.0;
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
        WindowClass::EscarpmentFace => {
            let riser = sstep((along - mid + 300.0) / 600.0);
            (amp * (0.5 - riser), 0.3 + 0.4 * riser)
        }
        // The rim of a basin whose centre lies beyond the window: relief
        // rises away from the edge, accommodation rises toward it.
        WindowClass::BasinMargin => {
            let toward = sstep(along / EXTENT_M);
            (amp * (0.5 - 0.9 * toward), 0.3 + 0.55 * toward)
        }
        // A plain strong ramp: the class is carried by tilt, not relief.
        WindowClass::PiedmontSlope => (0.0, 0.45),
        // Three broad treads stepping toward the edge.
        WindowClass::TerraceFlight => {
            let s = along / EXTENT_M * 3.0;
            let tread = s.floor().clamp(0.0, 2.0);
            let riser = sstep((s - tread) * 3000.0 / 400.0 / 3.0);
            let stepped = (tread + riser) / 3.0;
            (amp * (0.5 - stepped), 0.35 + 0.3 * (1.0 - stepped))
        }
    }
}

/// Extra tilt-drop multiplier per class (piedmont_slope is carried by tilt).
pub fn tilt_share(class: WindowClass) -> f64 {
    match class {
        WindowClass::PiedmontSlope => 0.9,
        WindowClass::EscarpmentFace => 0.5,
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
