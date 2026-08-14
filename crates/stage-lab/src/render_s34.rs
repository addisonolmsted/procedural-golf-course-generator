//! S3 (amplify) and S4 (hydrology) rendering for the stage-lab.
//!
//! S3's review question is "did the dictionary add REAL texture without
//! moving the structure?" — so the tab's load-bearing control is the
//! instant base/amplified flip (B key): structure must hold still while
//! texture appears. The delta view isolates exactly what S3 added.
//!
//! S4's review question is "does water sit where the terrain says it
//! should?" — water bodies and basins ON the amplified hillshade, the
//! derived flow field, and the S2-vs-derived agreement overlay (the same
//! red/blue instrument the acceptance work used).

use course_skeleton::kernel::Skeleton;
use course_transforms::hydrology::Hydrology;
use course_world::grid::Grid;
use course_world::math::Vec2;
use image::RgbaImage;

const WATER_PERM: [u8; 4] = [50, 110, 220, 170];
const WATER_INTERMITTENT: [u8; 4] = [90, 150, 230, 110];
const BASIN_OPEN: [u8; 4] = [235, 180, 60, 220];
const BASIN_CLOSED: [u8; 4] = [235, 100, 60, 230];
const S2_CHANNEL: [u8; 4] = [255, 60, 40, 255];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum S3View {
    /// Amplified hillshade — the review view (B flips to base).
    Amplified,
    /// What S3 added: amplified − base, symmetric scale.
    Delta,
}

pub const S3_VIEWS: [(S3View, &str); 2] = [
    (S3View::Amplified, "surface (B flips base)"),
    (S3View::Delta, "delta (amp − base)"),
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum S4View {
    /// Hillshade + water bodies + basin inventory — the review view.
    Water,
    /// log10 drained area on the AMPLIFIED surface.
    Accum,
    /// S2 channels (red) over derived accumulation (blue): agreement.
    Agreement,
}

pub const S4_VIEWS: [(S4View, &str); 3] = [
    (S4View::Water, "water + basins"),
    (S4View::Accum, "flow accum (log)"),
    (S4View::Agreement, "S2 vs derived"),
];

/// Both flip faces on ONE elevation scale, so the flip shows shape
/// change only — a rescale between faces would read as false movement.
pub fn render_s3_pair(base: &Grid<f64>, amp: &Grid<f64>, px: u32) -> (RgbaImage, RgbaImage) {
    let range = course_viz::finite_range(amp);
    let mut a = course_viz::render_height_grid(amp, px, range);
    let mut b = course_viz::render_height_grid(base, px, range);
    course_viz::overlay_contours(&mut a, amp, 4.0);
    course_viz::overlay_contours(&mut b, base, 4.0);
    course_viz::draw_core_box(&mut a);
    course_viz::draw_core_box(&mut b);
    (b, a)
}

pub fn render_s3_delta(base: &Grid<f64>, amp: &Grid<f64>, px: u32) -> RgbaImage {
    let mut d = amp.clone();
    let (nx, ny) = (d.spec.nx as usize, d.spec.ny as usize);
    for y in 0..ny {
        for x in 0..nx {
            let p = d.spec.world_of(x as u32, y as u32);
            d.data[y * nx + x] -= base.bilinear(p);
        }
    }
    let mut mags: Vec<f64> = d.data.iter().map(|v| v.abs()).collect();
    mags.sort_by(|a, b| a.total_cmp(b));
    let hi = mags[((mags.len() - 1) as f64 * 0.98) as usize].max(0.05);
    let mut img = course_viz::render_scalar_field(&d, px, -hi, hi);
    course_viz::draw_core_box(&mut img);
    img
}

pub fn render_s4(h: &Hydrology, sk: &Skeleton, view: S4View, px: u32, overlays: bool) -> RgbaImage {
    match view {
        S4View::Water => {
            let mut img = course_viz::render_height_grid(&h.height, px, None);
            course_viz::overlay_contours(&mut img, &h.height, 4.0);
            if overlays {
                for w in &h.water {
                    let c = if w.permanent { WATER_PERM } else { WATER_INTERMITTENT };
                    fill_poly(&mut img, &w.polygon, c);
                    course_viz::draw_polyline(&mut img, &w.polygon, opaque(c), 0);
                    if let Some(last) = w.polygon.last() {
                        let a = course_viz::to_px(*last, img.width(), img.height());
                        let b = course_viz::to_px(w.polygon[0], img.width(), img.height());
                        course_viz::draw_line(&mut img, a, b, opaque(c), 0);
                    }
                }
                // The inventory keeps every >=0.25 m pit (S7's repair pass
                // wants them all); the REVIEW view shows only basins big
                // enough to matter to routing, else ~250 texture-scale
                // rings bury the hillshade.
                for b in &h.basins {
                    if b.area_m2 < 1.0e4 && !b.closed {
                        continue;
                    }
                    let c = if b.closed { BASIN_CLOSED } else { BASIN_OPEN };
                    draw_ring(&mut img, &b.polygon, c);
                }
            }
            course_viz::draw_core_box(&mut img);
            img
        }
        S4View::Accum => {
            let mut g = h.flow_accum.clone();
            for v in &mut g.data {
                *v = (*v).max(1.0).log10();
            }
            let mut img = course_viz::render_scalar_field(&g, px, 1.5, 7.0);
            course_viz::draw_core_box(&mut img);
            img
        }
        S4View::Agreement => {
            // The acceptance instrument: derived accumulation in blue,
            // S2's channel cells stamped red on top — magenta where they
            // coincide is agreement, isolated red is a buried S2 swale.
            let n8 = h.flow_accum.spec.nx as usize;
            let mut img = RgbaImage::from_pixel(px, px, image::Rgba([20, 20, 24, 255]));
            let (w, ht) = (img.width(), img.height());
            // Stamps must cover a full 8 m cell in pixels or sub-pixel
            // gaps render as a grid mesh; the floor keeps ordinary
            // hillslope cells dark so only concentrated flow shows.
            let side = (px as f64 / n8 as f64).ceil() as i64;
            for y in 0..n8 {
                for x in 0..n8 {
                    let i = y * n8 + x;
                    let a = h.flow_accum.data[i].max(64.0).ln();
                    let t = ((a - 4.0) / 10.0).clamp(0.0, 1.0);
                    if t > 0.25 {
                        let p = h.flow_accum.spec.world_of(x as u32, y as u32);
                        stamp(&mut img, p, [40, 70, (90.0 + t * 165.0) as u8, 255], w, ht, side);
                    }
                }
            }
            for y in 0..n8 {
                for x in 0..n8 {
                    if sk.flow_distance.data[y * n8 + x] == 0.0 {
                        let p = sk.flow_distance.spec.world_of(x as u32, y as u32);
                        stamp_add(&mut img, p, S2_CHANNEL, w, ht, side);
                    }
                }
            }
            course_viz::draw_core_box(&mut img);
            img
        }
    }
}

fn opaque(c: [u8; 4]) -> [u8; 4] {
    [c[0], c[1], c[2], 255]
}

fn stamp(img: &mut RgbaImage, p: Vec2, c: [u8; 4], w: u32, h: u32, side: i64) {
    let (x, y) = course_viz::to_px(p, w, h);
    for dy in 0..side {
        for dx in 0..side {
            course_viz::put(img, x + dx, y + dy, c);
        }
    }
}

/// Red is stamped over blue by REPLACING R+G and keeping B, so overlap
/// reads magenta instead of red hiding the accumulation beneath.
fn stamp_add(img: &mut RgbaImage, p: Vec2, c: [u8; 4], w: u32, h: u32, side: i64) {
    let (x, y) = course_viz::to_px(p, w, h);
    for dy in 0..side {
        for dx in 0..side {
            let (px, py) = (x + dx, y + dy);
            if px < 0 || py < 0 || px >= w as i64 || py >= h as i64 {
                continue;
            }
            let old = img.get_pixel(px as u32, py as u32).0;
            course_viz::put(img, px, py, [c[0], c[1], old[2].max(40), 255]);
        }
    }
}

fn draw_ring(img: &mut RgbaImage, poly: &[Vec2], c: [u8; 4]) {
    if poly.len() < 2 {
        return;
    }
    course_viz::draw_polyline(img, poly, c, 0);
    let a = course_viz::to_px(poly[poly.len() - 1], img.width(), img.height());
    let b = course_viz::to_px(poly[0], img.width(), img.height());
    course_viz::draw_line(img, a, b, c, 0);
}

/// Alpha-fill a simple polygon (even-odd rule — S4 outlines are traced
/// component boundaries and may be concave).
fn fill_poly(img: &mut RgbaImage, poly: &[Vec2], c: [u8; 4]) {
    if poly.len() < 3 {
        return;
    }
    let (w, h) = (img.width(), img.height());
    let pts: Vec<(f64, f64)> = poly
        .iter()
        .map(|p| {
            let (x, y) = course_viz::to_px(*p, w, h);
            (x as f64, y as f64)
        })
        .collect();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in &pts {
        x0 = x0.min(x);
        y0 = y0.min(y);
        x1 = x1.max(x);
        y1 = y1.max(y);
    }
    for py in (y0.max(0.0) as i64)..=(y1.min(h as f64 - 1.0) as i64) {
        for px in (x0.max(0.0) as i64)..=(x1.min(w as f64 - 1.0) as i64) {
            let (fx, fy) = (px as f64 + 0.5, py as f64 + 0.5);
            let mut crossings = 0;
            for i in 0..pts.len() {
                let (ax, ay) = pts[i];
                let (bx, by) = pts[(i + 1) % pts.len()];
                if (ay > fy) != (by > fy) {
                    let x_at = ax + (fy - ay) / (by - ay) * (bx - ax);
                    if x_at > fx {
                        crossings += 1;
                    }
                }
            }
            if crossings % 2 == 1 {
                course_viz::blend_px(img, px, py, c);
            }
        }
    }
}
