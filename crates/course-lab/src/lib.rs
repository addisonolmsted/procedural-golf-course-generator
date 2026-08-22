//! Attempt 4's visualiser.
//!
//! Written fresh rather than reusing `course-viz` for a technical reason, not
//! a procedural one: `course-viz` renders HEIGHTFIELDS, and steps 1-6 of this
//! pipeline have no heightfield. What needs looking at here is vector
//! geometry, direction fields and scalar fields over an empty tile. A
//! hillshade renderer is the wrong instrument for that.
//!
//! Deterministic and dependency-light: `course-world` + `image`.

use course_world::grid::Grid;
use course_world::math::Vec2;
use image::{Rgba, RgbaImage};

pub type Rgb = [u8; 3];

pub const BG: Rgb = [18, 20, 24];
pub const INK: Rgb = [235, 238, 242];
pub const MUTED: Rgb = [120, 128, 140];
pub const TRUNK: Rgb = [86, 180, 255];
pub const MOUTH: Rgb = [255, 208, 64];
pub const SCARP: Rgb = [255, 122, 90];
pub const GRAIN: Rgb = [150, 220, 170];

/// A pixel canvas over a square world tile.
pub struct Canvas {
    pub img: RgbaImage,
    /// World metres spanned by the image.
    pub extent_m: f64,
    pub px: u32,
}

impl Canvas {
    pub fn new(px: u32, extent_m: f64) -> Self {
        let mut img = RgbaImage::new(px, px);
        for p in img.pixels_mut() {
            *p = Rgba([BG[0], BG[1], BG[2], 255]);
        }
        Canvas { img, extent_m, px }
    }

    /// World metres -> pixel. Y is flipped so north is up.
    pub fn to_px(&self, w: Vec2) -> (f64, f64) {
        let s = self.px as f64 / self.extent_m;
        (w.x * s, (self.extent_m - w.y) * s)
    }

    pub fn blend(&mut self, x: i64, y: i64, c: Rgb, a: f64) {
        if x < 0 || y < 0 || x >= self.px as i64 || y >= self.px as i64 || a <= 0.0 {
            return;
        }
        let a = a.clamp(0.0, 1.0);
        let p = self.img.get_pixel_mut(x as u32, y as u32);
        for i in 0..3 {
            p[i] = (p[i] as f64 * (1.0 - a) + c[i] as f64 * a).round() as u8;
        }
    }

    /// Paint a scalar grid as a background, mapped through `colour`.
    /// `value` is sampled per pixel so grid resolution never shows as blocks.
    pub fn field<F: Fn(f64) -> Rgb>(&mut self, g: &Grid<f64>, lo: f64, hi: f64, colour: F) {
        let span = if (hi - lo).abs() < 1e-12 { 1.0 } else { hi - lo };
        for py in 0..self.px {
            for px in 0..self.px {
                let s = self.extent_m / self.px as f64;
                let w = Vec2::new((px as f64 + 0.5) * s, self.extent_m - (py as f64 + 0.5) * s);
                let t = ((g.bilinear(w) - lo) / span).clamp(0.0, 1.0);
                let c = colour(t);
                self.img.put_pixel(px, py, Rgba([c[0], c[1], c[2], 255]));
            }
        }
    }

    /// Anti-aliased line in WORLD coordinates, width in pixels.
    pub fn line(&mut self, a: Vec2, b: Vec2, c: Rgb, w_px: f64, alpha: f64) {
        let (x0, y0) = self.to_px(a);
        let (x1, y1) = self.to_px(b);
        let (dx, dy) = (x1 - x0, y1 - y0);
        let n = (dx.abs().max(dy.abs()) * 2.0).ceil().max(1.0) as i64;
        let r = (w_px * 0.5).max(0.5);
        for i in 0..=n {
            let t = i as f64 / n as f64;
            let (cx, cy) = (x0 + dx * t, y0 + dy * t);
            let ri = r.ceil() as i64;
            for oy in -ri..=ri {
                for ox in -ri..=ri {
                    let (gx, gy) = ((cx.floor() as i64) + ox, (cy.floor() as i64) + oy);
                    let d = ((gx as f64 + 0.5 - cx).powi(2) + (gy as f64 + 0.5 - cy).powi(2)).sqrt();
                    self.blend(gx, gy, c, alpha * (r + 0.5 - d).clamp(0.0, 1.0));
                }
            }
        }
    }

    pub fn polyline(&mut self, pts: &[Vec2], c: Rgb, w_px: f64, alpha: f64) {
        for p in pts.windows(2) {
            self.line(p[0], p[1], c, w_px, alpha);
        }
    }

    pub fn disc(&mut self, w: Vec2, r_px: f64, c: Rgb, alpha: f64) {
        let (cx, cy) = self.to_px(w);
        let ri = r_px.ceil() as i64 + 1;
        for oy in -ri..=ri {
            for ox in -ri..=ri {
                let (gx, gy) = ((cx.floor() as i64) + ox, (cy.floor() as i64) + oy);
                let d = ((gx as f64 + 0.5 - cx).powi(2) + (gy as f64 + 0.5 - cy).powi(2)).sqrt();
                self.blend(gx, gy, c, alpha * (r_px + 0.5 - d).clamp(0.0, 1.0));
            }
        }
    }

    /// Short strokes on a lattice showing a direction field (radians, axial).
    pub fn quiver(&mut self, g: &Grid<f64>, step_m: f64, len_m: f64, c: Rgb, alpha: f64) {
        let n = (self.extent_m / step_m).floor() as i64;
        for j in 0..=n {
            for i in 0..=n {
                let w = Vec2::new(i as f64 * step_m, j as f64 * step_m);
                let th = g.bilinear(w);
                let d = Vec2::new(course_world::math::cos(th), course_world::math::sin(th));
                let h = len_m * 0.5;
                self.line(
                    Vec2::new(w.x - d.x * h, w.y - d.y * h),
                    Vec2::new(w.x + d.x * h, w.y + d.y * h),
                    c, 1.2, alpha,
                );
            }
        }
    }

    /// The routable-core box, for scale.
    pub fn core_box(&mut self, lo: f64, hi: f64, c: Rgb, alpha: f64) {
        let p = [
            Vec2::new(lo, lo), Vec2::new(hi, lo),
            Vec2::new(hi, hi), Vec2::new(lo, hi), Vec2::new(lo, lo),
        ];
        self.polyline(&p, c, 1.0, alpha);
    }

    pub fn save(&self, path: &str) -> std::io::Result<()> {
        self.img.save(path).map_err(std::io::Error::other)
    }

    /// Save as JPEG at `quality`. These renders are mostly smooth colormapped
    /// gradients, which PNG stores badly -- a 460 px tile costs 229 KB as PNG
    /// and about a tenth of that as a high-quality JPEG. Only used for the
    /// viewer artifact, which has a hard total size budget.
    pub fn save_jpeg(&self, path: &str, quality: u8) -> std::io::Result<()> {
        let rgb = image::DynamicImage::ImageRgba8(self.img.clone()).to_rgb8();
        let mut f = std::fs::File::create(path)?;
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut f, quality)
            .encode_image(&image::DynamicImage::ImageRgb8(rgb))
            .map_err(std::io::Error::other)
    }
}

/// Diverging blue-grey-orange, for signed fields.
pub fn diverging(t: f64) -> Rgb {
    let lo: Rgb = [64, 116, 180];
    let mid: Rgb = [42, 46, 52];
    let hi: Rgb = [214, 138, 70];
    let (a, b, u) = if t < 0.5 { (lo, mid, t * 2.0) } else { (mid, hi, (t - 0.5) * 2.0) };
    [0, 1, 2].map(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * u).round() as u8)
}

/// Dark-to-light, for unsigned fields.
pub fn sequential(t: f64) -> Rgb {
    let a: Rgb = [24, 28, 34];
    let b: Rgb = [225, 232, 240];
    [0, 1, 2].map(|i| (a[i] as f64 + (b[i] as f64 - a[i] as f64) * t).round() as u8)
}

/// Banded, so resistance CONTACTS are visible as edges rather than a ramp.
pub fn banded(t: f64) -> Rgb {
    let s = sequential(t);
    let band = ((t * 7.0).floor() as i32 % 2) == 0;
    if band { s } else { [0, 1, 2].map(|i| (s[i] as f64 * 0.78).round() as u8) }
}

/// A grid of canvases with a colour tab per row, saved as one image.
///
/// Reviewing one tile at a time hides the thing worth seeing, which is how
/// archetypes differ from each other and how much a single archetype varies
/// between seeds. No font is linked, so rows are identified by a colour tab
/// and the caller states the order.
pub struct Sheet {
    cell: u32,
    pad: u32,
    tab: u32,
    rows: Vec<(Rgb, Vec<Canvas>)>,
}

impl Sheet {
    pub fn new(cell: u32) -> Self {
        Sheet { cell, pad: 6, tab: 10, rows: Vec::new() }
    }

    pub fn row(&mut self, tab: Rgb, cells: Vec<Canvas>) -> &mut Self {
        self.rows.push((tab, cells));
        self
    }

    pub fn save(&self, path: &str) -> std::io::Result<(u32, u32)> {
        let cols = self.rows.iter().map(|r| r.1.len()).max().unwrap_or(0) as u32;
        let nrows = self.rows.len() as u32;
        let w = self.tab + self.pad + cols * (self.cell + self.pad);
        let h = self.pad + nrows * (self.cell + self.pad);
        let mut img = RgbaImage::from_pixel(w.max(1), h.max(1), Rgba([12, 13, 16, 255]));
        for (r, (tab, cells)) in self.rows.iter().enumerate() {
            let y0 = self.pad + r as u32 * (self.cell + self.pad);
            for yy in 0..self.cell {
                for xx in 0..self.tab {
                    img.put_pixel(xx, y0 + yy, Rgba([tab[0], tab[1], tab[2], 255]));
                }
            }
            for (c, canvas) in cells.iter().enumerate() {
                let x0 = self.tab + self.pad + c as u32 * (self.cell + self.pad);
                for yy in 0..self.cell.min(canvas.px) {
                    for xx in 0..self.cell.min(canvas.px) {
                        img.put_pixel(x0 + xx, y0 + yy, *canvas.img.get_pixel(xx, yy));
                    }
                }
            }
        }
        if let Some(p) = std::path::Path::new(path).parent() {
            std::fs::create_dir_all(p).ok();
        }
        img.save(path).map_err(std::io::Error::other)?;
        Ok((w, h))
    }
}

/// Distinct row tabs, in a stable order.
pub const TABS: [Rgb; 6] = [
    [120, 200, 130],
    [225, 205, 110],
    [90, 165, 240],
    [230, 130, 95],
    [185, 130, 220],
    [235, 190, 150],
];

/// TINTED HILLSHADE — the canonical height view from the U gate on (user
/// direction): shading carries the form, a desaturated hypsometric tint
/// carries the elevation. Light NW at 45°, slope-darkened.
pub fn render_terrain(c: &mut Canvas, g: &Grid<f64>, z_lo: f64, z_hi: f64) {
    let span = (z_hi - z_lo).max(1e-9);
    let light = {
        let az = 315.0_f64.to_radians();
        let alt = 45.0_f64.to_radians();
        [az.cos() * alt.cos(), az.sin() * alt.cos(), alt.sin()]
    };
    // the preferred ramp (user): GREEN lows -> TAN highs
    let ramp = |t: f64| -> [f64; 3] {
        let stops: [(f64, [f64; 3]); 4] = [
            (0.0, [0.34, 0.50, 0.29]),
            (0.40, [0.62, 0.62, 0.38]),
            (0.75, [0.78, 0.68, 0.44]),
            (1.0, [0.87, 0.79, 0.60]),
        ];
        let t = t.clamp(0.0, 1.0);
        for w in stops.windows(2) {
            if t <= w[1].0 {
                let u = (t - w[0].0) / (w[1].0 - w[0].0).max(1e-9);
                return [0, 1, 2].map(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * u);
            }
        }
        stops[3].1
    };
    let s = c.extent_m / c.px as f64;
    for py in 0..c.px {
        for px in 0..c.px {
            let w = Vec2::new((px as f64 + 0.5) * s, c.extent_m - (py as f64 + 0.5) * s);
            let h = 8.0;
            // z-exaggeration for the SHADING NORMALS only (standard tinted-
            // hillshade practice): gentle landforms — dune trains at 4-7%
            // slope, heath ridges — carry real form the eye cannot see at
            // honest gain. The transect proved seed 201's dune trains were
            // IN the surface while the render showed blobs.
            const Z_EXAG: f64 = 2.4;
            let dx = Z_EXAG * (g.bilinear(Vec2::new(w.x + h, w.y)) - g.bilinear(Vec2::new(w.x - h, w.y))) / (2.0 * h);
            let dy = Z_EXAG * (g.bilinear(Vec2::new(w.x, w.y + h)) - g.bilinear(Vec2::new(w.x, w.y - h))) / (2.0 * h);
            let l = (dx * dx + dy * dy + 1.0).sqrt();
            let lam = ((-dx) * light[0] + (-dy) * light[1] + light[2]) / l;
            // SHADE dominates: full lambert range plus slope darkening
            // pure lambert — the slope-darkening term double-marked every steep
            // face (read as a bluff highlight pass; user: hillshade only)
            let shade = lam.max(0.0).powf(1.15);
            let z = g.bilinear(w);
            let tint = ramp((z - z_lo) / span);
            let px_col = [0, 1, 2].map(|i| {
                ((tint[i] * 0.85 + 0.15) * (0.25 + 0.95 * shade) * 235.0).min(255.0) as u8
            });
            c.img.put_pixel(px, py, image::Rgba([px_col[0], px_col[1], px_col[2], 255]));
        }
    }
}
