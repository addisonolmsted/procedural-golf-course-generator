//! `course-viz`: pure deterministic grid → `RgbaImage` rendering for the
//! step viewers. The load-bearing parts (hypsometric × hillshade, the
//! pixel-centered field sampler) are copied from `terrain-v2:golf-viz`
//! rather than depended on (that crate drags the whole v1 terrain stack).
//!
//! Render cost is O(output pixels), decoupled from grid resolution: each
//! output pixel maps to a world point and bilinearly samples the grid.

use course_world::math::Vec2;
use course_world::world::EXTENT_M;
use course_world::Grid;
use image::{Rgba, RgbaImage};

pub type Color = [f64; 3];

/// Default long edge of a rendered view, in pixels.
pub const VIEW_PX: u32 = 900;

/// Vertical exaggeration for hillshading. Course-scale relief is gentle
/// relative to the map width, so slopes are amplified purely for
/// *visualization* — it never touches the terrain data.
const HILLSHADE_EXAG: f64 = 8.0;

// --------------------------------------------------------------------------
// Core sampler: iterate output pixels, map to world (north up), call shade.
// --------------------------------------------------------------------------

pub fn render_field(px: u32, nx: u32, ny: u32, cell: f64, shade: impl Fn(Vec2) -> Color) -> RgbaImage {
    let w_ext = nx as f64 * cell;
    let h_ext = ny as f64 * cell;
    let scale = px as f64 / w_ext.max(h_ext);
    let img_w = (w_ext * scale).round().max(1.0) as u32;
    let img_h = (h_ext * scale).round().max(1.0) as u32;

    let mut img = RgbaImage::new(img_w, img_h);
    for py in 0..img_h {
        for pxx in 0..img_w {
            // Pixel center -> world, y flipped so north is up.
            let world = Vec2::new((pxx as f64 + 0.5) / scale, h_ext - (py as f64 + 0.5) / scale);
            img.put_pixel(pxx, py, to_rgba(shade(world)));
        }
    }
    img
}

/// Render any heightfield grid: hypsometric tint × hillshade. `range`
/// overrides color normalization so two images share one elevation scale
/// (generated-vs-real comparison); `None` normalizes to this grid.
pub fn render_height_grid(h: &Grid<f64>, px: u32, range: Option<(f64, f64)>) -> RgbaImage {
    let spec = h.spec;
    let (min, max) = range.unwrap_or_else(|| min_max(&h.data));
    let span = (max - min).max(1e-6);
    let light = normalize3([-0.5, 0.6, 1.0]);
    render_field(px, spec.nx, spec.ny, spec.cell_size, |world| {
        let z = h.bilinear(world);
        let t = (z - min) / span;
        let base = hypsometric(t);
        let shade = hillshade(h, world, spec.cell_size, light);
        scale_color(base, shade)
    })
}

/// Finite-only `(min, max)` of a grid; `None` when no cell is finite.
/// Real DTM tiles carry NaN where the elevation source had gaps, so the
/// plain min/max fold would poison the whole color scale.
pub fn finite_range(h: &Grid<f64>) -> Option<(f64, f64)> {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in &h.data {
        if v.is_finite() {
            min = min.min(v);
            max = max.max(v);
        }
    }
    (min <= max).then_some((min, max))
}

/// [`render_height_grid`] for grids that may contain NaN (real DTM tiles):
/// the scale comes from finite cells only and non-finite samples are
/// painted flat `nodata` with no hillshade (hillshading a NaN neighborhood
/// produces garbage normals).
pub fn render_height_grid_nan(
    h: &Grid<f64>,
    px: u32,
    range: Option<(f64, f64)>,
    nodata: [u8; 3],
) -> RgbaImage {
    let spec = h.spec;
    let (min, max) = range.or_else(|| finite_range(h)).unwrap_or((0.0, 1.0));
    let span = (max - min).max(1e-6);
    let light = normalize3([-0.5, 0.6, 1.0]);
    let nodata_c: Color = [nodata[0] as f64, nodata[1] as f64, nodata[2] as f64];
    render_field(px, spec.nx, spec.ny, spec.cell_size, |world| {
        let z = h.bilinear(world);
        if !z.is_finite() {
            return nodata_c;
        }
        let base = hypsometric(((z - min) / span).clamp(0.0, 1.0));
        let shade = hillshade_nan(h, world, spec.cell_size, light);
        scale_color(base, shade)
    })
}

/// Render a scalar field in `[lo, hi]` with a perceptual blue→yellow ramp
/// (the conditioning-field heatmap view).
pub fn render_scalar_field(g: &Grid<f64>, px: u32, lo: f64, hi: f64) -> RgbaImage {
    let spec = g.spec;
    let span = (hi - lo).max(1e-9);
    render_field(px, spec.nx, spec.ny, spec.cell_size, |world| {
        scalar_ramp(((g.bilinear(world) - lo) / span).clamp(0.0, 1.0))
    })
}

/// Render a direction field (radians mod π) as hue; magnitude is constant.
pub fn render_direction_field(g: &Grid<f64>, px: u32) -> RgbaImage {
    let spec = g.spec;
    render_field(px, spec.nx, spec.ny, spec.cell_size, |world| {
        let a = g.bilinear(world).rem_euclid(std::f64::consts::PI);
        hue_color(a / std::f64::consts::PI)
    })
}

// --------------------------------------------------------------------------
// Overlays (image-space; world→pixel assumes the full 3 km box, north up)
// --------------------------------------------------------------------------

pub fn to_px(p: Vec2, w: u32, h: u32) -> (i64, i64) {
    (
        (p.x / EXTENT_M * w as f64) as i64,
        ((1.0 - p.y / EXTENT_M) * h as f64) as i64,
    )
}

pub fn put(img: &mut RgbaImage, x: i64, y: i64, c: [u8; 4]) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, Rgba(c));
    }
}

/// Source-over blend one pixel: `c`'s alpha is the mix weight, the result
/// stays opaque. The `put*`/`draw*` family paints opaquely; QA overlays
/// need the terrain to read through them.
pub fn blend_px(img: &mut RgbaImage, x: i64, y: i64, c: [u8; 4]) {
    if x < 0 || y < 0 || (x as u32) >= img.width() || (y as u32) >= img.height() {
        return;
    }
    let a = c[3] as f64 / 255.0;
    let p = img.get_pixel(x as u32, y as u32).0;
    let mix = |dst: u8, src: u8| (dst as f64 * (1.0 - a) + src as f64 * a) as u8;
    img.put_pixel(
        x as u32,
        y as u32,
        Rgba([mix(p[0], c[0]), mix(p[1], c[1]), mix(p[2], c[2]), 255]),
    );
}

/// Blend `color` over every pixel whose nearest `classes` cell has any bit
/// of `mask` set. World mapping matches [`render_field`] (north up), so the
/// overlay lands exactly on the terrain render it is drawn onto.
pub fn overlay_class_mask(img: &mut RgbaImage, classes: &Grid<u8>, mask: u8, color: [u8; 4]) {
    let (w, h) = (img.width(), img.height());
    let spec = classes.spec;
    let (ext_x, ext_y) = (spec.nx as f64 * spec.cell_size, spec.ny as f64 * spec.cell_size);
    for py in 0..h {
        for px in 0..w {
            let wx = (px as f64 + 0.5) / w as f64 * ext_x;
            let wy = ext_y - (py as f64 + 0.5) / h as f64 * ext_y;
            let cx = (wx / spec.cell_size) as i64;
            let cy = (wy / spec.cell_size) as i64;
            if cx < 0 || cy < 0 || cx as u32 >= spec.nx || cy as u32 >= spec.ny {
                continue;
            }
            if classes.get(cx as u32, cy as u32) & mask != 0 {
                blend_px(img, px as i64, py as i64, color);
            }
        }
    }
}

/// Blend `color` over pixels whose projection onto `dir` falls inside any
/// `[t0, t1]` band — the bench cascade's scarp bands, which are defined as
/// ranges along the tilt axis rather than as polygons.
pub fn overlay_axis_bands(img: &mut RgbaImage, dir: Vec2, bands: &[(f64, f64)], color: [u8; 4]) {
    if bands.is_empty() {
        return;
    }
    let (w, h) = (img.width(), img.height());
    for py in 0..h {
        for px in 0..w {
            let world = Vec2::new(
                (px as f64 + 0.5) / w as f64 * EXTENT_M,
                EXTENT_M - (py as f64 + 0.5) / h as f64 * EXTENT_M,
            );
            let t = world.dot(dir);
            if bands.iter().any(|&(t0, t1)| t >= t0 && t <= t1) {
                blend_px(img, px as i64, py as i64, color);
            }
        }
    }
}

pub fn draw_line(img: &mut RgbaImage, a: (i64, i64), b: (i64, i64), c: [u8; 4], thick: i64) {
    let (dx, dy) = ((b.0 - a.0) as f64, (b.1 - a.1) as f64);
    let n = dx.abs().max(dy.abs()).max(1.0) as i64;
    for i in 0..=n {
        let t = i as f64 / n as f64;
        let x = a.0 + (dx * t) as i64;
        let y = a.1 + (dy * t) as i64;
        for ox in -thick..=thick {
            for oy in -thick..=thick {
                put(img, x + ox, y + oy, c);
            }
        }
    }
}

pub fn draw_polyline(img: &mut RgbaImage, pts: &[Vec2], c: [u8; 4], thick: i64) {
    let (w, h) = (img.width(), img.height());
    for seg in pts.windows(2) {
        draw_line(img, to_px(seg[0], w, h), to_px(seg[1], w, h), c, thick);
    }
}

pub fn draw_disc(img: &mut RgbaImage, center: Vec2, radius_m: f64, c: [u8; 4]) {
    let (w, h) = (img.width(), img.height());
    let (cx, cy) = to_px(center, w, h);
    let r = (radius_m / EXTENT_M * w as f64) as i64;
    for ox in -r..=r {
        for oy in -r..=r {
            if ox * ox + oy * oy <= r * r {
                put(img, cx + ox, cy + oy, c);
            }
        }
    }
}

/// Circle outline in world units.
pub fn draw_circle(img: &mut RgbaImage, center: Vec2, radius_m: f64, c: [u8; 4], thick: i64) {
    let n = 96;
    let pts: Vec<Vec2> = (0..=n)
        .map(|i| {
            let a = 2.0 * std::f64::consts::PI * i as f64 / n as f64;
            center + Vec2::new(a.cos() * radius_m, a.sin() * radius_m)
        })
        .collect();
    draw_polyline(img, &pts, c, thick);
}

/// Darken pixels on iso-band changes of the height grid (contour overlay).
pub fn overlay_contours(img: &mut RgbaImage, h: &Grid<f64>, step_m: f64) {
    let (w, hh) = (img.width(), img.height());
    let spec = h.spec;
    let ext_x = spec.nx as f64 * spec.cell_size;
    let ext_y = spec.ny as f64 * spec.cell_size;
    let band = |px: u32, py: u32| -> i64 {
        let world = Vec2::new(
            (px as f64 + 0.5) / w as f64 * ext_x,
            ext_y - (py as f64 + 0.5) / hh as f64 * ext_y,
        );
        (h.bilinear(world) / step_m).floor() as i64
    };
    for py in 1..hh {
        for px in 1..w {
            let b = band(px, py);
            if b != band(px - 1, py) || b != band(px, py - 1) {
                let p = img.get_pixel(px, py).0;
                img.put_pixel(
                    px,
                    py,
                    Rgba([
                        (p[0] as f64 * 0.55) as u8,
                        (p[1] as f64 * 0.55) as u8,
                        (p[2] as f64 * 0.55) as u8,
                        255,
                    ]),
                );
            }
        }
    }
}

/// Outline the routable core window.
pub fn draw_core_box(img: &mut RgbaImage) {
    use course_world::world::{CORE_MAX_M, CORE_MIN_M};
    let c = [255, 255, 255, 255];
    let pts = [
        Vec2::new(CORE_MIN_M, CORE_MIN_M),
        Vec2::new(CORE_MAX_M, CORE_MIN_M),
        Vec2::new(CORE_MAX_M, CORE_MAX_M),
        Vec2::new(CORE_MIN_M, CORE_MAX_M),
        Vec2::new(CORE_MIN_M, CORE_MIN_M),
    ];
    draw_polyline(img, &pts, c, 0);
}

/// Multiply RGB outside the core by 0.45 (gate-style dimming).
pub fn dim_outside_core(img: &mut RgbaImage) {
    use course_world::world::in_core;
    let (w, h) = (img.width(), img.height());
    for py in 0..h {
        for px in 0..w {
            let world = Vec2::new(
                (px as f64 + 0.5) / w as f64 * EXTENT_M,
                EXTENT_M - (py as f64 + 0.5) / h as f64 * EXTENT_M,
            );
            if !in_core(world) {
                let p = img.get_pixel(px, py).0;
                img.put_pixel(
                    px,
                    py,
                    Rgba([
                        (p[0] as f64 * 0.45) as u8,
                        (p[1] as f64 * 0.45) as u8,
                        (p[2] as f64 * 0.45) as u8,
                        255,
                    ]),
                );
            }
        }
    }
}

// --------------------------------------------------------------------------
// Color ramps & shading (copied from golf-viz)
// --------------------------------------------------------------------------

fn hillshade(h: &Grid<f64>, world: Vec2, cell: f64, light: [f64; 3]) -> f64 {
    let e = cell;
    let zx = h.bilinear(world + Vec2::new(e, 0.0)) - h.bilinear(world - Vec2::new(e, 0.0));
    let zy = h.bilinear(world + Vec2::new(0.0, e)) - h.bilinear(world - Vec2::new(0.0, e));
    let n = normalize3([-zx * HILLSHADE_EXAG, -zy * HILLSHADE_EXAG, 2.0 * e]);
    let d = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    0.30 + 0.70 * d
}

/// [`hillshade`] that degrades to flat lighting when any sample in the
/// gradient stencil is non-finite (NNaN normals would render as noise).
fn hillshade_nan(h: &Grid<f64>, world: Vec2, cell: f64, light: [f64; 3]) -> f64 {
    let e = cell;
    let s = [
        h.bilinear(world + Vec2::new(e, 0.0)),
        h.bilinear(world - Vec2::new(e, 0.0)),
        h.bilinear(world + Vec2::new(0.0, e)),
        h.bilinear(world - Vec2::new(0.0, e)),
    ];
    if s.iter().any(|v| !v.is_finite()) {
        return 1.0;
    }
    let n = normalize3([
        -(s[0] - s[1]) * HILLSHADE_EXAG,
        -(s[2] - s[3]) * HILLSHADE_EXAG,
        2.0 * e,
    ]);
    let d = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    0.30 + 0.70 * d
}

/// Hypsometric tint: low green → khaki → tan → pale highland (caps at pale
/// tan, not white — low-relief terrain must not fake an alpine snowcap).
fn hypsometric(t: f64) -> Color {
    let stops = [
        (0.00, [64.0, 104.0, 62.0]),
        (0.40, [112.0, 138.0, 78.0]),
        (0.70, [158.0, 152.0, 106.0]),
        (1.00, [192.0, 182.0, 158.0]),
    ];
    ramp(&stops, t)
}

fn scalar_ramp(t: f64) -> Color {
    let stops = [
        (0.00, [40.0, 50.0, 90.0]),
        (0.35, [50.0, 110.0, 140.0]),
        (0.70, [110.0, 180.0, 120.0]),
        (1.00, [240.0, 220.0, 90.0]),
    ];
    ramp(&stops, t)
}

fn hue_color(t: f64) -> Color {
    // Cyclic hue for undirected angles: t and t+1 wrap to the same color.
    let a = t * 2.0 * std::f64::consts::PI;
    [
        128.0 + 100.0 * a.cos(),
        128.0 + 100.0 * (a + 2.094).cos(),
        128.0 + 100.0 * (a + 4.189).cos(),
    ]
}

fn ramp(stops: &[(f64, Color)], t: f64) -> Color {
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        let (t0, c0) = w[0];
        let (t1, c1) = w[1];
        if t <= t1 {
            let k = if t1 == t0 { 0.0 } else { (t - t0) / (t1 - t0) };
            return blend(c0, c1, k);
        }
    }
    stops[stops.len() - 1].1
}

fn blend(a: Color, b: Color, t: f64) -> Color {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn scale_color(c: Color, s: f64) -> Color {
    [c[0] * s, c[1] * s, c[2] * s]
}

fn to_rgba(c: Color) -> Rgba<u8> {
    Rgba([
        c[0].clamp(0.0, 255.0) as u8,
        c[1].clamp(0.0, 255.0) as u8,
        c[2].clamp(0.0, 255.0) as u8,
        255,
    ])
}

fn normalize3(v: [f64; 3]) -> [f64; 3] {
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        [0.0, 0.0, 1.0]
    } else {
        [v[0] / len, v[1] / len, v[2] / len]
    }
}

fn min_max(vals: &[f64]) -> (f64, f64) {
    let mut min = f64::INFINITY;
    let mut max = f64::NEG_INFINITY;
    for &v in vals {
        min = min.min(v);
        max = max.max(v);
    }
    if min > max {
        (0.0, 1.0)
    } else {
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use course_world::GridSpec;

    #[test]
    fn render_smoke() {
        let spec = GridSpec::new(Vec2::ZERO, 100.0, 31, 31);
        let mut g = Grid::filled(spec, 0.0);
        for y in 0..31 {
            for x in 0..31 {
                g.set(x, y, (x as f64) * 0.5);
            }
        }
        let img = render_height_grid(&g, 64, None);
        assert_eq!(img.width(), 64);
        let mut img2 = render_scalar_field(&g, 64, 0.0, 15.0);
        overlay_contours(&mut img2, &g, 2.0);
        draw_core_box(&mut img2);
    }

    /// A NaN patch must not poison the color scale or the shading of the
    /// finite cells around it (the real-DTM tile case).
    #[test]
    fn nan_render_is_localized() {
        let spec = GridSpec::new(Vec2::ZERO, 100.0, 31, 31);
        let mut g = Grid::filled(spec, 0.0);
        for y in 0..31 {
            for x in 0..31 {
                g.set(x, y, x as f64 * 0.5);
            }
        }
        g.set(0, 0, f64::NAN);
        assert_eq!(finite_range(&g), Some((0.0, 15.0)));
        let img = render_height_grid_nan(&g, 64, None, [90, 90, 96]);
        // Bottom-left pixel is the NaN cell (y flipped); top-right is finite.
        assert_eq!(img.get_pixel(0, 63).0, [90, 90, 96, 255]);
        assert_ne!(img.get_pixel(63, 0).0, [90, 90, 96, 255]);
    }

    #[test]
    fn class_and_band_overlays_blend() {
        let spec = GridSpec::new(Vec2::ZERO, 100.0, 30, 30);
        let g = Grid::filled(spec, 0.0);
        let mut classes = Grid::filled(spec, 0u8);
        classes.set(0, 0, 2); // SW corner: channel bit
        let mut img = render_height_grid(&g, 60, None);
        let before = img.get_pixel(0, 59).0;
        overlay_class_mask(&mut img, &classes, 2, [0, 0, 255, 128]);
        let after = img.get_pixel(0, 59).0;
        assert_ne!(before, after);
        assert!(after[2] > before[2], "blend should push blue up");
        assert_eq!(after[3], 255, "result stays opaque");
        // A pixel far from the flagged cell is untouched.
        assert_eq!(img.get_pixel(59, 0).0, before);

        let mut img2 = render_height_grid(&g, 60, None);
        let b0 = img2.get_pixel(30, 30).0;
        overlay_axis_bands(&mut img2, Vec2::new(1.0, 0.0), &[(0.0, 3000.0)], [255, 0, 0, 100]);
        assert!(img2.get_pixel(30, 30).0[0] > b0[0]);
    }
}
