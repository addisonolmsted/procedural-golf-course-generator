//! Trunk-river module — STRUCTURAL role only: floodplain accommodation and
//! terrace-flight geometry hung on the trunk. Scroll-bar and swale TEXTURE
//! moved to S3's dictionary. Dominant in river-valley; low in piedmont.
//!
//! Geometry: within the floodplain half-width the surface flattens to a
//! gentle gradient above the channel base; above it, up to two terrace
//! treads step the valley side at fixed riser heights. Everything here is
//! ≥ 64 m wavelength — the module never competes with S3's patch scale.

use course_world::grid::Grid;

use crate::fluvial::flow_distance::Nearest;

/// Apply in place on the 8 m base surface. `max_order` identifies trunk
/// cells in the wavefront payload (the module shapes only the trunk valley).
pub fn apply(z: &mut Grid<f64>, near: &[Nearest], intensity: f64, max_order: u8) {
    if intensity <= 0.0 || max_order == 0 {
        return;
    }
    let half_width = 60.0 + 320.0 * intensity; // floodplain half-width, m
    let flood_grade = 0.004; // valley-floor gradient away from the channel
    let riser_m = 2.2 * intensity; // terrace riser height
    let treads = if intensity > 0.55 { 2 } else { 1 };

    for (i, n) in near.iter().enumerate() {
        if n.order != max_order || !n.dist_m.is_finite() {
            continue;
        }
        let d = n.dist_m;
        if d <= half_width {
            // Floodplain: pull the surface down to a near-flat valley floor.
            let floor = n.z_channel + flood_grade * d;
            if z.data[i] > floor {
                let t = 1.0 - (d / half_width);
                // full flattening near the channel, easing off at the margin
                let s = t * t * (3.0 - 2.0 * t);
                z.data[i] += (floor - z.data[i]) * (0.35 + 0.65 * s) * intensity;
            }
        } else {
            // Terrace flight: soft steps in the first D beyond the floodplain.
            for k in 0..treads {
                let edge = half_width * (1.0 + 0.65 * (k as f64 + 1.0));
                let band = 0.18 * half_width;
                let t = ((d - edge) / band).clamp(-1.0, 1.0);
                // smoothstep riser centred on the tread edge
                let s = 0.5 + 0.25 * t * (3.0 - t * t);
                z.data[i] += riser_m * (s - 0.5) * 0.7;
            }
        }
    }
}
