//! Rendering for the v2 tabs: contract C1 (stage S1) views.
//!
//! The "implied terrain" here is tilt + relief summed and hillshaded — the
//! macro predisposition surface. It is a real view of the C1 artifact (unlike
//! the v1 framing preview, which was illustrative): these two fields ARE the
//! elevation content of C1. What it deliberately lacks — drainage, texture —
//! is downstream's job; see C1_REVIEW.md for what to judge.

use course_contracts::biome::BoundaryKind;
use course_contracts::contracts::primitive_field::PrimitiveField;
use course_contracts::metadata::Edge;
use course_viz::{
    draw_core_box, draw_polyline, overlay_contours, render_height_grid, render_scalar_field,
};
use course_world::math::{cos, sin, Vec2};
use course_world::world::EXTENT_M;
use course_world::Grid;
use image::RgbaImage;

const BASE_EDGE: [u8; 4] = [64, 132, 244, 255];
const WIND: [u8; 4] = [240, 240, 150, 255];
const GRAIN: [u8; 4] = [230, 230, 230, 140];
const SCARP: [u8; 4] = [220, 60, 60, 255];
const VALLEY_WALL: [u8; 4] = [64, 132, 244, 255];
const CONTACT: [u8; 4] = [235, 140, 50, 255];

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum C1View {
    Implied,
    Relief,
    Tilt,
    Hardness,
    Accommodation,
}

pub const C1_VIEWS: [(C1View, &str); 5] = [
    (C1View::Implied, "implied terrain (tilt + relief)"),
    (C1View::Relief, "relief"),
    (C1View::Tilt, "tilt"),
    (C1View::Hardness, "hardness"),
    (C1View::Accommodation, "accommodation"),
];

fn kind_color(kind: BoundaryKind) -> [u8; 4] {
    match kind {
        BoundaryKind::Scarp => SCARP,
        BoundaryKind::ValleyWall => VALLEY_WALL,
        BoundaryKind::MaterialContact => CONTACT,
    }
}

/// The macro predisposition surface: tilt + relief, as one grid.
pub fn implied_surface(c1: &PrimitiveField) -> Grid<f64> {
    let mut g = c1.tilt.clone();
    for i in 0..g.data.len() {
        g.data[i] += c1.relief.data[i];
    }
    g
}

/// Render a C1 view with the structural overlays (discontinuities, base-level
/// edge, wind arrow, grain glyphs). `overlays=false` renders the bare surface
/// — the form used for the blind class-legibility set, where glyphs would
/// leak the answer.
pub fn render_c1(c1: &PrimitiveField, view: C1View, px: u32, overlays: bool) -> RgbaImage {
    let mut img = match view {
        C1View::Implied => {
            let g = implied_surface(c1);
            let mut img = render_height_grid(&g, px, None);
            overlay_contours(&mut img, &g, 2.0);
            img
        }
        C1View::Relief => render_height_grid(&c1.relief, px, None),
        C1View::Tilt => render_height_grid(&c1.tilt, px, None),
        C1View::Hardness => render_scalar_field(&c1.hardness, px, 0.0, 1.0),
        C1View::Accommodation => render_scalar_field(&c1.accommodation, px, 0.0, 1.0),
    };
    if overlays {
        draw_overlays(&mut img, c1);
    }
    draw_core_box(&mut img);
    img
}

fn draw_overlays(img: &mut RgbaImage, c1: &PrimitiveField) {
    // Grain: axis glyphs on a sparse lattice, length ∝ strength.
    let axis = Vec2::new(cos(c1.meta.grain_axis_rad), sin(c1.meta.grain_axis_rad));
    let half = 40.0 + 160.0 * c1.meta.grain_strength;
    let mut y = 250.0;
    while y < EXTENT_M {
        let mut x = 250.0;
        while x < EXTENT_M {
            let p = Vec2::new(x, y);
            draw_polyline(img, &[p - axis * half, p + axis * half], GRAIN, 0);
            x += 500.0;
        }
        y += 500.0;
    }
    // Discontinuities, colored by kind.
    for d in &c1.meta.discontinuities {
        draw_polyline(img, &d.curve, kind_color(d.kind), 2);
    }
    // Base-level edge band.
    draw_edge_band(img, c1.meta.base_level.edge);
    // Wind: an arrow anchored near the NE corner.
    let w = Vec2::new(cos(c1.meta.wind_azimuth_rad), sin(c1.meta.wind_azimuth_rad));
    let anchor = Vec2::new(EXTENT_M - 420.0, EXTENT_M - 420.0);
    draw_arrow(img, anchor - w * 180.0, anchor + w * 180.0, WIND);
}

fn draw_edge_band(img: &mut RgbaImage, edge: Edge) {
    let t = 40.0;
    let seg = |a: Vec2, b: Vec2, img: &mut RgbaImage| draw_polyline(img, &[a, b], BASE_EDGE, 3);
    let e = EXTENT_M;
    match edge {
        Edge::N => seg(Vec2::new(0.0, e - t), Vec2::new(e, e - t), img),
        Edge::S => seg(Vec2::new(0.0, t), Vec2::new(e, t), img),
        Edge::E => seg(Vec2::new(e - t, 0.0), Vec2::new(e - t, e), img),
        Edge::W => seg(Vec2::new(t, 0.0), Vec2::new(t, e), img),
        Edge::CornerNe => {
            seg(Vec2::new(e / 2.0, e - t), Vec2::new(e, e - t), img);
            seg(Vec2::new(e - t, e / 2.0), Vec2::new(e - t, e), img);
        }
        Edge::CornerNw => {
            seg(Vec2::new(0.0, e - t), Vec2::new(e / 2.0, e - t), img);
            seg(Vec2::new(t, e / 2.0), Vec2::new(t, e), img);
        }
        Edge::CornerSe => {
            seg(Vec2::new(e / 2.0, t), Vec2::new(e, t), img);
            seg(Vec2::new(e - t, 0.0), Vec2::new(e - t, e / 2.0), img);
        }
        Edge::CornerSw => {
            seg(Vec2::new(0.0, t), Vec2::new(e / 2.0, t), img);
            seg(Vec2::new(t, 0.0), Vec2::new(t, e / 2.0), img);
        }
    }
}

fn draw_arrow(img: &mut RgbaImage, from: Vec2, to: Vec2, c: [u8; 4]) {
    draw_polyline(img, &[from, to], c, 1);
    let d = Vec2::new(to.x - from.x, to.y - from.y);
    let len = (d.x * d.x + d.y * d.y).sqrt().max(1e-9);
    let u = Vec2::new(d.x / len, d.y / len);
    let n = Vec2::new(-u.y, u.x);
    let head = 60.0;
    draw_polyline(img, &[to, to - u * head + n * (head * 0.5)], c, 1);
    draw_polyline(img, &[to, to - u * head - n * (head * 0.5)], c, 1);
}
