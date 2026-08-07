//! The stage-01 framing schematic: pure vectors on a flat neutral field.
//! Arrow/glyph helpers live HERE, not course-viz — promote to the shared
//! crate when a second lab wants them. No raster text: labels are egui chrome.

use course_framing::{BoundaryKind, EdgeId, Framing};
use course_viz::{
    dim_outside_core, draw_core_box, draw_disc, draw_line, draw_polyline, overlay_contours, to_px,
};
use course_world::ease::{eased_step, end_taper, ramp, smin};
use course_world::grid::{Grid, GridSpec};
use course_world::math::{cos, sin, smoothstep, Vec2};
use course_world::noise::perlin2;
use course_world::spline::{SegIndex, Spine};
use course_world::world::EXTENT_M;
use image::{Rgba, RgbaImage};

const BACKGROUND: [u8; 4] = [52, 54, 52, 255];
const BASE_EDGE: [u8; 4] = [64, 132, 244, 255]; // where water leaves (drain blue)
const TILT: [u8; 4] = [250, 250, 250, 255];
const GRAIN: [u8; 4] = [230, 230, 230, 150];
const SCARP: [u8; 4] = [220, 60, 60, 255];
const VALLEY_WALL: [u8; 4] = [64, 132, 244, 255];
const CONTACT: [u8; 4] = [235, 140, 50, 255];
// Skeleton families.
const TRUNK: [u8; 4] = [70, 160, 255, 255];
const TRUNK_BAND: [u8; 4] = [70, 160, 255, 60];
const BRANCH: [u8; 4] = [110, 190, 255, 210];
const RIDGE: [u8; 4] = [215, 160, 70, 255];
const STEP: [u8; 4] = [185, 120, 220, 255];

pub fn kind_color(kind: BoundaryKind) -> [u8; 4] {
    match kind {
        BoundaryKind::Scarp => SCARP,
        BoundaryKind::ValleyWall => VALLEY_WALL,
        BoundaryKind::MaterialContact => CONTACT,
    }
}

/// Render one framing artifact as a north-up schematic of the 3 km box.
pub fn render_framing_schematic(f: &Framing, px: u32) -> RgbaImage {
    let mut img = RgbaImage::from_pixel(px, px, Rgba(BACKGROUND));
    dim_outside_core(&mut img);

    // Structural grain: axis glyphs on a 250 m lattice, length ∝ anisotropy.
    let axis = Vec2::new(cos(f.grain.dir_rad), sin(f.grain.dir_rad));
    let half = 40.0 + 160.0 * f.grain.anisotropy;
    let mut y = 125.0;
    while y < EXTENT_M {
        let mut x = 125.0;
        while x < EXTENT_M {
            let p = Vec2::new(x, y);
            draw_polyline(&mut img, &[p - axis * half, p + axis * half], GRAIN, 0);
            x += 250.0;
        }
        y += 250.0;
    }

    draw_skeleton(&mut img, f);

    // Province boundary, colored by kind.
    if let Some(b) = &f.provinces.boundary {
        let pts: Vec<Vec2> = b.curve.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        draw_polyline(&mut img, &pts, kind_color(b.kind), 2);
    }

    // Base-level edge band: where water leaves the box.
    draw_edge_band(&mut img, f.base_level.edge);

    // Regional tilt: downhill arrow from the center, length ∝ grade.
    let c = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
    let dir = Vec2::new(cos(f.regional_tilt.dir_rad), sin(f.regional_tilt.dir_rad));
    let len = 150.0 + 750.0 * (f.regional_tilt.grade / 0.08).clamp(0.0, 1.0);
    draw_arrow(&mut img, c, c + dir * len, TILT, 1);

    draw_core_box(&mut img);
    img
}

/// A thick band hugging the outlet edge; a corner gets the two adjacent
/// half-edges.
fn draw_edge_band(img: &mut RgbaImage, edge: EdgeId) {
    const INSET: f64 = 35.0;
    let lo = INSET;
    let hi = EXTENT_M - INSET;
    let mid = EXTENT_M / 2.0;
    let mut band = |a: Vec2, b: Vec2| draw_polyline(img, &[a, b], BASE_EDGE, 3);
    match edge {
        EdgeId::N => band(Vec2::new(lo, hi), Vec2::new(hi, hi)),
        EdgeId::S => band(Vec2::new(lo, lo), Vec2::new(hi, lo)),
        EdgeId::E => band(Vec2::new(hi, lo), Vec2::new(hi, hi)),
        EdgeId::W => band(Vec2::new(lo, lo), Vec2::new(lo, hi)),
        EdgeId::CornerNe => {
            band(Vec2::new(mid, hi), Vec2::new(hi, hi));
            band(Vec2::new(hi, mid), Vec2::new(hi, hi));
        }
        EdgeId::CornerNw => {
            band(Vec2::new(lo, hi), Vec2::new(mid, hi));
            band(Vec2::new(lo, mid), Vec2::new(lo, hi));
        }
        EdgeId::CornerSe => {
            band(Vec2::new(mid, lo), Vec2::new(hi, lo));
            band(Vec2::new(hi, lo), Vec2::new(hi, mid));
        }
        EdgeId::CornerSw => {
            band(Vec2::new(lo, lo), Vec2::new(mid, lo));
            band(Vec2::new(lo, lo), Vec2::new(lo, mid));
        }
    }
}

fn poly(pts: &[[f64; 2]]) -> Vec<Vec2> {
    pts.iter().map(|p| Vec2::new(p[0], p[1])).collect()
}

/// The structural skeleton: ground families first, water on top.
fn draw_skeleton(img: &mut RgbaImage, f: &Framing) {
    let s = &f.skeleton;
    for r in &s.ridges {
        draw_polyline(img, &poly(r), RIDGE, 2);
    }
    for st in &s.steps {
        draw_polyline(img, &poly(st), STEP, 1);
    }
    if let Some(t) = &s.trunk {
        // Corridor band, then the centerline through it.
        let band_px = ((t.halfwidth_m / EXTENT_M) * f64::from(img.width())).round() as i64;
        draw_polyline(img, &poly(&t.spine), TRUNK_BAND, band_px.max(2));
        draw_polyline(img, &poly(&t.spine), TRUNK, 1);
    }
    for b in &s.branches {
        draw_polyline(img, &poly(b), BRANCH, 1);
    }
}

/// Relief amplitudes for the ILLUSTRATIVE preview. Read from θ
/// (`spec.param`) by the caller — never from the artifact, which carries
/// pure geometry.
#[derive(Debug, Clone, Copy)]
pub struct ImpliedRelief {
    pub ridge_relief_m: f64,
    pub trunk_carve_m: f64,
    pub step_riser_m: f64,
    pub province_relief_m: f64,
}

/// Fixed noise seed — this surface is a sketch of what the skeleton implies,
/// not a pipeline artifact, so its texture is deliberately not seed-derived.
const PREVIEW_NOISE_SEED: u32 = 0xF11A;
const RIDGE_HALFWIDTH_M: f64 = 180.0;
const PROVINCE_BLEND_M: f64 = 300.0;
const STEP_FACE_M: f64 = 60.0;

/// What the skeleton IMPLIES as a surface — an illustration for the lab, not
/// a pipeline artifact. Stage 04 compiles the real forcing fields and Stage
/// 05's LEM does the finishing; this just makes "does it read organized?"
/// answerable at a glance.
pub fn implied_height_grid(f: &Framing, r: &ImpliedRelief, res_m: f64) -> Grid<f64> {
    let n = (EXTENT_M / res_m).round() as u32;
    let spec = GridSpec::new(Vec2::ZERO, res_m, n + 1, n + 1);
    let mut g = Grid::filled(spec, 0.0);
    let center = Vec2::new(EXTENT_M / 2.0, EXTENT_M / 2.0);
    let downhill = Vec2::new(cos(f.regional_tilt.dir_rad), sin(f.regional_tilt.dir_rad));

    // Spines + indices built once, not per cell.
    let ridge_spines: Vec<(Spine, SegIndex, f64)> = f
        .skeleton
        .ridges
        .iter()
        .map(|p| {
            let s = Spine::new(poly(p));
            let ix = SegIndex::for_spine(&s);
            let len = s.length();
            (s, ix, len)
        })
        .collect();
    let step_spines: Vec<(Spine, SegIndex)> = f
        .skeleton
        .steps
        .iter()
        .map(|p| {
            let s = Spine::new(poly(p));
            let ix = SegIndex::for_spine(&s);
            (s, ix)
        })
        .collect();
    let province = f.provinces.boundary.as_ref().map(|b| {
        let s = Spine::new(poly(&b.curve));
        let ix = SegIndex::for_spine(&s);
        (s, ix)
    });
    let trunk = f.skeleton.trunk.as_ref().map(|t| {
        let s = Spine::new(poly(&t.spine));
        let ix = SegIndex::for_spine(&s);
        (s, ix, t.halfwidth_m)
    });
    let branches: Vec<(Spine, SegIndex)> = f
        .skeleton
        .branches
        .iter()
        .filter(|p| p.len() >= 2)
        .map(|p| {
            let s = Spine::new(poly(p));
            let ix = SegIndex::for_spine(&s);
            (s, ix)
        })
        .collect();

    let grain = Vec2::new(cos(f.grain.dir_rad), sin(f.grain.dir_rad));
    let noise_amp = (0.2 * r.ridge_relief_m).max(1.5);
    let stretch = 1.0 - 0.75 * f.grain.anisotropy;

    for y in 0..g.spec.ny {
        for x in 0..g.spec.nx {
            let p = g.spec.world_of(x, y);
            // 1. The regional plane.
            let mut z = -f.regional_tilt.grade * (p - center).dot(downhill);

            // 2. Province offset across the boundary.
            if let Some((s, ix)) = &province {
                let hit = s.project_with(ix, p);
                z += r.province_relief_m * (eased_step(hit.side * hit.d, PROVINCE_BLEND_M) - 0.5);
            }

            // 3. Ridge bumps, tapered at the axis ends.
            for (s, ix, len) in &ridge_spines {
                let hit = s.project_with(ix, p);
                if hit.d < RIDGE_HALFWIDTH_M && *len > 0.0 {
                    z += r.ridge_relief_m
                        * (1.0 - smoothstep(0.0, RIDGE_HALFWIDTH_M, hit.d))
                        * end_taper(hit.u, 0.15);
                }
            }

            // 4. Step risers along the fall line.
            for (s, ix) in &step_spines {
                let hit = s.project_with(ix, p);
                let signed = hit.side * hit.d;
                z += r.step_riser_m * (eased_step(signed, STEP_FACE_M) - 0.5);
            }

            // 5. Corridors carve LAST (the stage-04 precedence rule), so
            //    water cuts through ridges and terraces rather than being
            //    fenced out of them — the gaps emerge.
            if let Some((s, ix, hw)) = &trunk {
                z = carve(z, s.project_with(ix, p).d, *hw, r.trunk_carve_m);
            }
            for (s, ix) in &branches {
                z = carve(z, s.project_with(ix, p).d, 25.0, 0.6 * r.trunk_carve_m);
            }

            // 6. Grain-anisotropic texture.
            let (u, v) = (p.dot(grain), p.dot(grain.perp()));
            z += noise_amp * perlin2(u / 500.0, v / (500.0 * stretch.max(0.08)), PREVIEW_NOISE_SEED);

            let i = g.spec.index(x, y);
            g.data[i] = z;
        }
    }
    g
}

/// Blend a corridor floor into the surface with a soft valley wall.
fn carve(z: f64, dist: f64, halfwidth_m: f64, depth_m: f64) -> f64 {
    if depth_m <= 0.0 {
        return z;
    }
    let floor = z - depth_m + ramp((dist - halfwidth_m).max(0.0), 0.06, 20.0);
    smin(z, floor, 0.5 * depth_m)
}

/// The preview raster: hillshaded implied terrain + the skeleton overlaid.
pub fn render_implied_terrain(
    f: &Framing,
    r: &ImpliedRelief,
    px: u32,
    res_m: f64,
    contours: bool,
) -> RgbaImage {
    let h = implied_height_grid(f, r, res_m);
    let mut img = course_viz::render_height_grid(&h, px, None);
    if contours {
        let (lo, hi) = course_viz::finite_range(&h).unwrap_or((0.0, 1.0));
        let step = (((hi - lo) / 12.0).max(1.0) / 2.0).round() * 2.0;
        overlay_contours(&mut img, &h, step.max(2.0));
    }
    draw_skeleton(&mut img, f);
    if let Some(b) = &f.provinces.boundary {
        draw_polyline(&mut img, &poly(&b.curve), kind_color(b.kind), 2);
    }
    draw_core_box(&mut img);
    img
}

/// World-space arrow with a proportional head.
pub fn draw_arrow(img: &mut RgbaImage, from: Vec2, to: Vec2, c: [u8; 4], thick: i64) {
    let dir = (to - from).normalized();
    let head = ((to - from).length() * 0.18).clamp(40.0, 120.0);
    let barb = |angle: f64| {
        let (s, co) = (sin(angle), cos(angle));
        to + Vec2::new(dir.x * co - dir.y * s, dir.x * s + dir.y * co) * head
    };
    let (w, h) = (img.width(), img.height());
    draw_line(img, to_px(from, w, h), to_px(to, w, h), c, thick);
    let back = 150.0_f64.to_radians();
    for a in [back, -back] {
        draw_line(img, to_px(to, w, h), to_px(barb(a), w, h), c, thick);
    }
    draw_disc(img, from, 12.0, c);
}
