//! 2D terrain visualization: a `Terrain` -> `RgbaImage`.
//!
//! Two views for the current noise-tuning phase:
//! - [`render_height`]: hypsometric elevation tint x hillshade, with water fill
//! - [`render_slope`]: slope magnitude as a heatmap
//!
//! Rendering is pure and deterministic. The 2D viewer will call these directly
//! (into egui textures); the dump tool writes them to PNG.

use golf_core::math::Vec2;
use golf_core::Grid;
use golf_terrain::Terrain;
use image::{Rgba, RgbaImage};

type Color = [f64; 3];

/// Default long edge of a rendered view, in pixels.
pub const VIEW_PX: u32 = 512;

/// Render the heightfield at the default size.
pub fn render_height(terrain: &Terrain) -> RgbaImage {
    render_height_px(terrain, VIEW_PX)
}

/// Render the slope field at the default size.
pub fn render_slope(terrain: &Terrain) -> RgbaImage {
    render_slope_px(terrain, VIEW_PX)
}

/// Render the heightfield (hypsometric color × hillshade) with a given
/// long-edge pixel size.
pub fn render_height_px(terrain: &Terrain, px: u32) -> RgbaImage {
    render_height_grid(&terrain.heights, px, None)
}

/// Render any heightfield grid — also used for atlas course windows and
/// matched-window comparisons. `range` overrides the color normalization so
/// two images can share one elevation scale; `None` normalizes to this grid.
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

/// Height view with the natural water layer: hypsometric + hillshade ground,
/// depth-ramped ponds/lakes, marsh-tinted wetlands, and streams overdrawn as
/// width-mapped vector polylines (a 6 m creek is sub-pixel at map scale and
/// must be drawn as a line to read at all).
pub fn render_height_water(
    h: &Grid<f64>,
    water: &golf_terrain::water::WaterLayer,
    px: u32,
    range: Option<(f64, f64)>,
) -> RgbaImage {
    use golf_terrain::water::{CLASS_DRY, CLASS_WETLAND};

    let spec = h.spec;
    let (min, max) = range.unwrap_or_else(|| min_max(&h.data));
    let span = (max - min).max(1e-6);
    let light = normalize3([-0.5, 0.6, 1.0]);
    // Fine-raster classes where available: stream ribbons stay connected on
    // diagonals and standing-water boundaries are rounded.
    let class_at = |world: Vec2| -> u8 { water.class_fine_at(world) };

    let mut img = render_field(px, spec.nx, spec.ny, spec.cell_size, |world| {
        let z = h.bilinear(world);
        let t = (z - min) / span;
        let base = hypsometric(t);
        let shade = hillshade(h, world, spec.cell_size, light);
        let ground = scale_color(base, shade);
        let wm = water.mask_soft.bilinear(world);
        if wm <= 0.30 {
            return ground;
        }
        let k = ((wm - 0.30) / 0.70).clamp(0.0, 1.0);
        let cls = class_at(world);
        if cls == CLASS_WETLAND {
            return blend(ground, [104.0, 142.0, 120.0], 0.65 * k);
        }
        if cls == CLASS_DRY {
            // Bank falloff only: keep the ground, slightly darkened.
            return scale_color(ground, 1.0 - 0.10 * k);
        }
        let d = water.depth.bilinear(world);
        let dt = (d / 3.0).clamp(0.0, 1.0);
        let water_col = blend([150.0, 190.0, 205.0], [22.0, 62.0, 132.0], dt);
        blend(ground, water_col, (0.55 + 0.45 * dt) * k)
    });

    // Vector overdraw of perennial streams.
    let w_ext = spec.nx as f64 * spec.cell_size;
    let h_ext = spec.ny as f64 * spec.cell_size;
    let scale = img.width().max(img.height()) as f64 / w_ext.max(h_ext);
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let to_img = |p: Vec2| -> (f64, f64) { (p.x * scale - 0.5, (h_ext - p.y) * scale - 0.5) };
    for l in &water.streams {
        if !l.perennial || l.pooled {
            // Pooled lone streams render as ponds via the corridor cells.
            continue;
        }
        for k in 1..l.pts.len() {
            // Don't draw the centerline across pond/lake surfaces.
            let mid = Vec2::new(
                0.5 * (l.pts[k - 1].x + l.pts[k].x),
                0.5 * (l.pts[k - 1].y + l.pts[k].y),
            );
            let c = class_at(mid);
            if c != CLASS_DRY && c != golf_terrain::water::CLASS_STREAM && c != CLASS_WETLAND {
                continue;
            }
            let (ax, ay) = to_img(l.pts[k - 1]);
            let (bx, by) = to_img(l.pts[k]);
            let wpx = (0.5 * (l.width[k - 1] + l.width[k]) * scale * 0.5).max(0.6);
            let dt = ((l.width[k] / 30.0).clamp(0.0, 1.0) * 0.7 + 0.2).min(1.0);
            let col = blend([120.0, 170.0, 200.0], [30.0, 80.0, 150.0], dt);
            let x0 = ((ax.min(bx) - wpx).floor() as i64).clamp(0, iw - 1);
            let x1 = ((ax.max(bx) + wpx).ceil() as i64).clamp(0, iw - 1);
            let y0 = ((ay.min(by) - wpx).floor() as i64).clamp(0, ih - 1);
            let y1 = ((ay.max(by) + wpx).ceil() as i64).clamp(0, ih - 1);
            let (dx, dy) = (bx - ax, by - ay);
            let len2 = (dx * dx + dy * dy).max(1e-9);
            for py in y0..=y1 {
                for pxx in x0..=x1 {
                    let t = (((pxx as f64 - ax) * dx + (py as f64 - ay) * dy) / len2)
                        .clamp(0.0, 1.0);
                    let (cx, cy) = (ax + t * dx, ay + t * dy);
                    let dd = ((pxx as f64 - cx).powi(2) + (py as f64 - cy).powi(2)).sqrt();
                    if dd <= wpx {
                        img.put_pixel(pxx as u32, py as u32, to_rgba(col));
                    }
                }
            }
        }
    }
    img
}

/// Per-hole overlay palette (hole 1–9) — readable on both green lowland and
/// pale mountain ground. Shared by the map overlay and the viewer's course
/// elevation profile.
pub const HOLE_COLORS: [Color; 9] = [
    [232.0, 62.0, 62.0],
    [242.0, 150.0, 40.0],
    [238.0, 222.0, 70.0],
    [140.0, 224.0, 60.0],
    [48.0, 212.0, 190.0],
    [70.0, 160.0, 242.0],
    [152.0, 112.0, 242.0],
    [232.0, 92.0, 202.0],
    [248.0, 248.0, 248.0],
];

/// Overlay a routed nine on an already-rendered world image (height/water
/// view of the full 2 km world): dashed walking paths, per-hole colored lines
/// of play, tee squares, green rings, hole numbers, and a clubhouse marker.
pub fn draw_routing(img: &mut RgbaImage, r: &golf_routing::Routing, world_ext: f64) {
    let scale = img.width().max(img.height()) as f64 / world_ext;
    let h_ext = world_ext;
    let to_img = |p: Vec2| -> (f64, f64) { (p.x * scale - 0.5, (h_ext - p.y) * scale - 0.5) };

    const PALETTE: [Color; 9] = HOLE_COLORS;

    // Walking paths first (underneath): dashed dark line.
    for h in &r.holes {
        for w in h.walk_to_next.windows(2) {
            draw_dashed(img, to_img(w[0]), to_img(w[1]), 1.1, [40.0, 40.0, 40.0], 5.0, 4.0);
        }
    }
    // Lines of play.
    for (i, h) in r.holes.iter().enumerate() {
        let col = PALETTE[i % 9];
        for s in h.pts.windows(2) {
            draw_capsule(img, to_img(s[0]), to_img(s[1]), 1.6, col);
        }
    }
    // Markers + hole numbers.
    for (i, h) in r.holes.iter().enumerate() {
        let col = PALETTE[i % 9];
        let tee = to_img(h.pts[0]);
        fill_rect(img, tee, 3, col);
        outline_rect(img, tee, 4, [20.0, 20.0, 20.0]);
        let green = to_img(*h.pts.last().unwrap());
        draw_disc(img, green, 3.4, col);
        draw_ring(img, green, 4.6, [20.0, 20.0, 20.0]);
        // Number beside the first segment's midpoint.
        let m0 = h.pts[0].lerp(h.pts[1], 0.5);
        let (mx, my) = to_img(m0);
        draw_number(img, mx + 5.0, my - 9.0, (i + 1) as u32, [255.0, 255.0, 255.0]);
    }
    // Clubhouse: dark-outlined diamond.
    let ch = to_img(r.clubhouse);
    draw_diamond(img, ch, 6.0, [252.0, 252.0, 252.0]);
    draw_diamond(img, ch, 3.2, [30.0, 30.0, 30.0]);
}

/// Mowing-zone tints for the build overlay + 3D vertex colors.
pub const ZONE_GREEN: Color = [96.0, 200.0, 96.0];
pub const ZONE_FRINGE: Color = [116.0, 178.0, 96.0];
pub const ZONE_FAIRWAY: Color = [124.0, 186.0, 88.0];
pub const ZONE_TEE: Color = [188.0, 208.0, 140.0];
pub const ZONE_SAND: Color = [226.0, 208.0, 158.0];

/// Overlay the built course zones (fairways, fringes, greens, tee pads) and
/// pins on a rendered world image. Zones are sampled through
/// `CourseBuild::zone_at` — the same classification the 3D view uses —
/// restricted to each hole's bounding boxes for speed.
pub fn draw_build(img: &mut RgbaImage, b: &golf_holes::CourseBuild, world_ext: f64) {
    let scale = img.width().max(img.height()) as f64 / world_ext;
    let (iw, ih) = (img.width() as i64, img.height() as i64);
    let to_img = |p: Vec2| -> (f64, f64) { (p.x * scale - 0.5, (world_ext - p.y) * scale - 0.5) };
    let to_world = |px: i64, py: i64| -> Vec2 {
        Vec2::new(
            (px as f64 + 0.5) / scale,
            world_ext - (py as f64 + 0.5) / scale,
        )
    };

    // Per-hole pixel rects (fairway bbox ∪ green bbox ∪ tee bbox).
    for h in &b.holes {
        let (mut lo, mut hi) = h.fairway_bbox;
        for &p in &h.green.outline {
            lo = Vec2::new(lo.x.min(p.x - 10.0), lo.y.min(p.y - 10.0));
            hi = Vec2::new(hi.x.max(p.x + 10.0), hi.y.max(p.y + 10.0));
        }
        let te = h.tee.a.max(h.tee.b) + 6.0;
        lo = Vec2::new(lo.x.min(h.tee.center.x - te), lo.y.min(h.tee.center.y - te));
        hi = Vec2::new(hi.x.max(h.tee.center.x + te), hi.y.max(h.tee.center.y + te));

        let (x0f, y1f) = to_img(lo);
        let (x1f, y0f) = to_img(hi);
        let x0 = (x0f.floor() as i64).clamp(0, iw - 1);
        let x1 = (x1f.ceil() as i64).clamp(0, iw - 1);
        let y0 = (y0f.floor() as i64).clamp(0, ih - 1);
        let y1 = (y1f.ceil() as i64).clamp(0, ih - 1);
        for py in y0..=y1 {
            for px in x0..=x1 {
                let w = to_world(px, py);
                let (tint, alpha) = match b.zone_at(w) {
                    golf_holes::Zone::Green(_) => (ZONE_GREEN, 0.75),
                    golf_holes::Zone::Fringe(_) => (ZONE_FRINGE, 0.6),
                    golf_holes::Zone::Tee(_) => (ZONE_TEE, 0.7),
                    golf_holes::Zone::Bunker(_) => (ZONE_SAND, 0.85),
                    golf_holes::Zone::Fairway(_) => (ZONE_FAIRWAY, 0.45),
                    golf_holes::Zone::Rough => continue,
                };
                let c = img.get_pixel(px as u32, py as u32);
                let blended = blend([c[0] as f64, c[1] as f64, c[2] as f64], tint, alpha);
                img.put_pixel(px as u32, py as u32, to_rgba(blended));
            }
        }
    }
    // Pins: white dot + flag tick.
    for h in &b.holes {
        let (x, y) = to_img(h.pin);
        draw_disc(img, (x, y), 1.6, [252.0, 252.0, 252.0]);
        draw_capsule(img, (x, y), (x, y - 5.0), 0.7, [252.0, 252.0, 252.0]);
        draw_capsule(img, (x, y - 5.0), (x + 3.0, y - 3.8), 0.8, [232.0, 62.0, 62.0]);
    }
}

/// Close-up of one built green: the composed surface with hillshade, 0.1 m
/// contour lines, the boundary, and the pin. `px` is the output size.
pub fn render_green_closeup(
    ct: &golf_terrain::CourseTerrain,
    b: &golf_holes::CourseBuild,
    hole: usize,
    px: u32,
) -> RgbaImage {
    let h = &b.holes[hole];
    let g = &h.green;
    let ext = g.a.max(g.b) * 1.35 + 6.0;
    let (c0, world) = (g.center - Vec2::new(ext, ext), 2.0 * ext);
    let scale = px as f64 / world;

    // Sample the composed surface once.
    let n = px as usize;
    let mut z = vec![0.0f64; n * n];
    for py in 0..n {
        for pxx in 0..n {
            let w = Vec2::new(
                c0.x + (pxx as f64 + 0.5) / scale,
                c0.y + world - (py as f64 + 0.5) / scale,
            );
            z[py * n + pxx] = b.surface_at(ct, w);
        }
    }
    let (mut zmin, mut zmax) = (f64::INFINITY, f64::NEG_INFINITY);
    for &v in &z {
        zmin = zmin.min(v);
        zmax = zmax.max(v);
    }
    let span = (zmax - zmin).max(0.2);

    let mut img = RgbaImage::new(px, px);
    let light = normalize3([-0.5, 0.6, 1.2]);
    let _ = (zmin, span);
    for py in 0..n {
        for pxx in 0..n {
            let w = Vec2::new(
                c0.x + (pxx as f64 + 0.5) / scale,
                c0.y + world - (py as f64 + 0.5) / scale,
            );
            let zc = z[py * n + pxx];
            // Hillshade from the sampled patch (strong exaggeration — green
            // relief is decimeters).
            let xm = pxx.saturating_sub(1);
            let xp = (pxx + 1).min(n - 1);
            let ym = py.saturating_sub(1);
            let yp = (py + 1).min(n - 1);
            let dzdx = (z[py * n + xp] - z[py * n + xm]) * scale * 9.0;
            let dzdy = (z[ym * n + pxx] - z[yp * n + pxx]) * scale * 9.0;
            let nrm = normalize3([-dzdx, -dzdy, 2.0]);
            let shade = 0.35 + 0.65 * (nrm[0] * light[0] + nrm[1] * light[1] + nrm[2] * light[2]).max(0.0);

            let rn = g.rnorm(w);
            let base = if rn <= 1.0 {
                ZONE_GREEN
            } else if rn <= 1.0 + 2.5 / g.a.min(g.b) {
                ZONE_FRINGE
            } else {
                match b.zone_at(w) {
                    golf_holes::Zone::Fairway(_) => ZONE_FAIRWAY,
                    golf_holes::Zone::Bunker(_) => ZONE_SAND,
                    _ => [98.0, 128.0, 76.0],
                }
            };
            let mut col = scale_color(base, shade);
            // 0.1 m contour lines: mark pixels where the iso level steps
            // against the left/up neighbor.
            if rn <= 1.02 {
                let lvl = (zc / 0.1).floor();
                let l_left = (z[py * n + xm] / 0.1).floor();
                let l_up = (z[ym * n + pxx] / 0.1).floor();
                if lvl != l_left || lvl != l_up {
                    col = scale_color(col, 0.70);
                }
            }
            img.put_pixel(pxx as u32, py as u32, to_rgba(col));
        }
    }
    // Boundary + pin.
    let to_img = |p: Vec2| -> (f64, f64) {
        ((p.x - c0.x) * scale - 0.5, (c0.y + world - p.y) * scale - 0.5)
    };
    for k in 0..g.outline.len() {
        let a = to_img(g.outline[k]);
        let bpt = to_img(g.outline[(k + 1) % g.outline.len()]);
        draw_capsule(&mut img, a, bpt, 1.0, [245.0, 245.0, 245.0]);
    }
    let (x, y) = to_img(h.pin);
    draw_disc(&mut img, (x, y), 2.2, [20.0, 20.0, 20.0]);
    draw_capsule(&mut img, (x, y), (x, y - 10.0), 1.1, [250.0, 250.0, 250.0]);
    draw_capsule(&mut img, (x, y - 10.0), (x + 6.0, y - 7.8), 1.4, [232.0, 62.0, 62.0]);
    draw_number(&mut img, 6.0, 6.0, (hole + 1) as u32, [255.0, 255.0, 255.0]);
    img
}

/// Zoomed, hole-aligned view of one built hole, rotated so the FIRST
/// line-of-play segment points up (+y). Deliberately flat/2D: pure zone
/// colors from the analytic shapes (green/fairway/tee boundaries, fine-
/// raster water) with no shading — the only relief cue is the togglable
/// 1 m contour set (sampled from the composed surface at `mpp` m/px).
/// Overlays: line of play, tee pad, green boundary, pin, north arrow,
/// 50 m scale bar.
pub fn render_hole_view(
    ct: &golf_terrain::CourseTerrain,
    b: &golf_holes::CourseBuild,
    r: &golf_routing::Routing,
    hole: usize,
    mpp: f64,
    contours: bool,
) -> RgbaImage {
    let rh = &r.holes[hole];
    let hb = &b.holes[hole];
    let o = rh.pts[0];
    let dir = (rh.pts[1] - rh.pts[0]).normalized();
    let right = Vec2::new(dir.y, -dir.x);
    let to_view = |p: Vec2| -> (f64, f64) { ((p - o).dot(right), (p - o).dot(dir)) };

    // Extent over everything that belongs to the hole.
    let (mut vx0, mut vx1, mut vy0, mut vy1) =
        (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
    let mut cover = |p: Vec2| {
        let (x, y) = to_view(p);
        vx0 = vx0.min(x);
        vx1 = vx1.max(x);
        vy0 = vy0.min(y);
        vy1 = vy1.max(y);
    };
    for &p in rh.pts.iter().chain(&hb.fairway_outline).chain(&hb.green.outline) {
        cover(p);
    }
    if let Some(su) = &hb.surround {
        for &p in &su.shape.outline {
            cover(p);
        }
    }
    let te = hb.tee.a + hb.tee.b;
    cover(hb.tee.center + Vec2::new(te, te));
    cover(hb.tee.center + Vec2::new(-te, -te));
    cover(hb.pin);
    const MARGIN: f64 = 25.0;
    vx0 -= MARGIN;
    vx1 += MARGIN;
    vy0 -= MARGIN;
    vy1 += MARGIN;
    // Requested layout resolution, capped so the image stays manageable.
    let mpp = mpp.max((vx1 - vx0).max(vy1 - vy0) / 4200.0);
    let iw = (((vx1 - vx0) / mpp).ceil() as u32).clamp(2, 4400);
    let ih = (((vy1 - vy0) / mpp).ceil() as u32).clamp(2, 4400);
    let to_world =
        |px: f64, py: f64| -> Vec2 { o + right * (vx0 + px * mpp) + dir * (vy1 - py * mpp) };
    let to_img = |p: Vec2| -> (f64, f64) {
        let (x, y) = to_view(p);
        ((x - vx0) / mpp - 0.5, (vy1 - y) / mpp - 0.5)
    };

    const ROUGH: [f64; 3] = [96.0, 126.0, 74.0];
    const WETLAND: [f64; 3] = [116.0, 152.0, 130.0];
    const WATER: [f64; 3] = [96.0, 152.0, 198.0];
    let (nw, nh) = (iw as usize, ih as usize);
    let mut img = RgbaImage::new(iw, ih);
    let rough_px = to_rgba(ROUGH);
    for p in img.pixels_mut() {
        *p = rough_px;
    }
    // Class map for pass precedence + contour masking:
    // 0 rough · 1 fairway · 2 fringe · 3 green · 4 tee · 5 wetland · 6 water.
    let mut class = vec![0u8; nw * nh];
    let paint = |img: &mut RgbaImage, class: &mut Vec<u8>, x: usize, y: usize, col: [f64; 3], c: u8| {
        img.put_pixel(x as u32, y as u32, to_rgba(col));
        class[y * nw + x] = c;
    };

    // View-space bbox of a world point set, as inclusive pixel ranges.
    let px_range = |pts: &[Vec2], margin: f64| -> Option<(usize, usize, usize, usize)> {
        let (mut x0, mut x1, mut y0, mut y1) =
            (f64::INFINITY, f64::NEG_INFINITY, f64::INFINITY, f64::NEG_INFINITY);
        for &p in pts {
            let (ix, iy) = to_img(p);
            x0 = x0.min(ix);
            x1 = x1.max(ix);
            y0 = y0.min(iy);
            y1 = y1.max(iy);
        }
        let m = margin / mpp;
        let (x0, x1, y0, y1) = (x0 - m, x1 + m, y0 - m, y1 + m);
        if !x0.is_finite() || x1 < 0.0 || y1 < 0.0 || x0 > nw as f64 - 1.0 || y0 > nh as f64 - 1.0 {
            return None;
        }
        Some((
            x0.floor().max(0.0) as usize,
            x1.ceil().min(nw as f64 - 1.0) as usize,
            y0.floor().max(0.0) as usize,
            y1.ceil().min(nh as f64 - 1.0) as usize,
        ))
    };

    // --- Fairways: even-odd scanline fill of the boundary polygons (the
    // actual shapes, sampled at ~1.5 m) for every hole in view. ---
    for h in &b.holes {
        if h.fw.is_none() {
            continue;
        }
        let len = h.spine.length();
        let n_out = (((h.fw.end - h.fw.start) * len / 1.5).ceil() as usize).max(8);
        let mut poly: Vec<(f64, f64)> = Vec::with_capacity(2 * n_out + 2);
        for k in 0..=n_out {
            let s = h.fw.start + (h.fw.end - h.fw.start) * k as f64 / n_out as f64;
            let p = h.spine.point_at(s);
            let (_, nrm) = h.spine.frame_at(s);
            poly.push(to_img(p + nrm * h.fw.half_width(len, s, true)));
        }
        for k in (0..=n_out).rev() {
            let s = h.fw.start + (h.fw.end - h.fw.start) * k as f64 / n_out as f64;
            let p = h.spine.point_at(s);
            let (_, nrm) = h.spine.frame_at(s);
            poly.push(to_img(p + nrm * -h.fw.half_width(len, s, false)));
        }
        fill_polygon_scan(&mut img, &mut class, nw, nh, &poly, ZONE_FAIRWAY, 1);
    }
    // --- Par-3 surrounds: the offset petal ring (fairway cut). ---
    for h in &b.holes {
        let Some(su) = &h.surround else { continue };
        let Some((x0, x1, y0, y1)) = px_range(&su.shape.outline, 1.0) else { continue };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let w = to_world(x as f64 + 0.5, y as f64 + 0.5);
                if su.shape.rnorm(w) <= 1.0 {
                    paint(&mut img, &mut class, x, y, ZONE_FAIRWAY, 1);
                }
            }
        }
    }
    // --- Bunkers: sand petals + spline trenches (validated clear of
    // greens/fringes, so ordering against them is cosmetic). ---
    for h in &b.holes {
        for bk in &h.bunkers {
            let out = bk.shape.outline(24);
            let Some((x0, x1, y0, y1)) = px_range(&out, 0.5) else { continue };
            for y in y0..=y1 {
                for x in x0..=x1 {
                    let w = to_world(x as f64 + 0.5, y as f64 + 0.5);
                    if bk.shape.contains(w) {
                        paint(&mut img, &mut class, x, y, ZONE_SAND, 7);
                    }
                }
            }
        }
    }
    // --- Fringes + greens (analytic petal boundaries). ---
    for h in &b.holes {
        let fringe_out = 1.0 + golf_holes::FRINGE_M / h.green.a.min(h.green.b).max(1.0);
        let Some((x0, x1, y0, y1)) = px_range(&h.green.outline, golf_holes::FRINGE_M + 1.0)
        else {
            continue;
        };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let w = to_world(x as f64 + 0.5, y as f64 + 0.5);
                let rn = h.green.rnorm(w);
                if rn <= 1.0 {
                    paint(&mut img, &mut class, x, y, ZONE_GREEN, 3);
                } else if rn <= fringe_out {
                    paint(&mut img, &mut class, x, y, ZONE_FRINGE, 2);
                }
            }
        }
    }
    // --- Tee pads. ---
    for h in &b.holes {
        let ext = h.tee.a.max(h.tee.b) + 1.0;
        let corners = [
            h.tee.center + Vec2::new(ext, ext),
            h.tee.center + Vec2::new(-ext, -ext),
            h.tee.center + Vec2::new(ext, -ext),
            h.tee.center + Vec2::new(-ext, ext),
        ];
        let Some((x0, x1, y0, y1)) = px_range(&corners, 0.5) else { continue };
        for y in y0..=y1 {
            for x in x0..=x1 {
                let w = to_world(x as f64 + 0.5, y as f64 + 0.5);
                if h.tee.rnorm(w) <= 1.0 {
                    paint(&mut img, &mut class, x, y, ZONE_TEE, 4);
                }
            }
        }
    }

    // --- Standing water + wetlands from the COURSE-REFINED smooth SDF
    // outlines (the build trims fairway intrusions and melts wacky thin
    // lobes near holes), sampled through a 0.5 m lattice. Water wins. ---
    {
        let k_px = 0.5 / mpp; // lattice spacing in pixels
        let lw = (nw as f64 / k_px).ceil() as usize + 2;
        let lh = (nh as f64 / k_px).ceil() as usize + 2;
        let mut lat_st = vec![-1.0e9f64; lw * lh];
        let mut lat_we = vec![-1.0e9f64; lw * lh];
        for iy in 0..lh {
            for ix in 0..lw {
                let w = to_world(ix as f64 * k_px, iy as f64 * k_px);
                lat_st[iy * lw + ix] = b.water.standing_sdf.bilinear(w);
                lat_we[iy * lw + ix] = b.water.wetland_sdf.bilinear(w);
            }
        }
        let lat_at = |lat: &[f64], px: f64, py: f64| -> f64 {
            let lx = (px / k_px).clamp(0.0, lw as f64 - 1.001);
            let ly = (py / k_px).clamp(0.0, lh as f64 - 1.001);
            let (x0, y0) = (lx as usize, ly as usize);
            let (tx, ty) = (lx - x0 as f64, ly - y0 as f64);
            let v00 = lat[y0 * lw + x0];
            let v10 = lat[y0 * lw + x0 + 1];
            let v01 = lat[(y0 + 1) * lw + x0];
            let v11 = lat[(y0 + 1) * lw + x0 + 1];
            (v00 + (v10 - v00) * tx) + ((v01 + (v11 - v01) * tx) - (v00 + (v10 - v00) * tx)) * ty
        };
        // Only pixel blocks whose lattice corners come near water.
        for iy in 0..lh - 1 {
            for ix in 0..lw - 1 {
                let near = [iy * lw + ix, iy * lw + ix + 1, (iy + 1) * lw + ix, (iy + 1) * lw + ix + 1]
                    .iter()
                    .any(|&i| lat_st[i] > -1.0 || lat_we[i] > -1.0);
                if !near {
                    continue;
                }
                let x0 = (ix as f64 * k_px).floor().max(0.0) as usize;
                let x1 = (((ix + 1) as f64 * k_px).ceil() as usize).min(nw - 1);
                let y0 = (iy as f64 * k_px).floor().max(0.0) as usize;
                let y1 = (((iy + 1) as f64 * k_px).ceil() as usize).min(nh - 1);
                for y in y0..=y1 {
                    for x in x0..=x1 {
                        let (pxf, pyf) = (x as f64 + 0.5, y as f64 + 0.5);
                        if lat_at(&lat_st, pxf, pyf) >= 0.0 {
                            paint(&mut img, &mut class, x, y, WATER, 6);
                        } else if lat_at(&lat_we, pxf, pyf) >= 0.0 {
                            paint(&mut img, &mut class, x, y, WETLAND, 5);
                        }
                    }
                }
            }
        }
    }

    // --- Contours (togglable): 1 m iso lines from the 0.5 m height
    // lattice, land only. ---
    if contours {
        let k_px = 0.5 / mpp;
        let lw = (nw as f64 / k_px).ceil() as usize + 2;
        let lh = (nh as f64 / k_px).ceil() as usize + 2;
        let mut lat_z = vec![0.0f64; lw * lh];
        for iy in 0..lh {
            for ix in 0..lw {
                lat_z[iy * lw + ix] =
                    b.surface_at(ct, to_world(ix as f64 * k_px, iy as f64 * k_px));
            }
        }
        let z_at = |px: f64, py: f64| -> f64 {
            let lx = (px / k_px).clamp(0.0, lw as f64 - 1.001);
            let ly = (py / k_px).clamp(0.0, lh as f64 - 1.001);
            let (x0, y0) = (lx as usize, ly as usize);
            let (tx, ty) = (lx - x0 as f64, ly - y0 as f64);
            let a = lat_z[y0 * lw + x0] + (lat_z[y0 * lw + x0 + 1] - lat_z[y0 * lw + x0]) * tx;
            let c =
                lat_z[(y0 + 1) * lw + x0] + (lat_z[(y0 + 1) * lw + x0 + 1] - lat_z[(y0 + 1) * lw + x0]) * tx;
            a + (c - a) * ty
        };
        let row_z = |y: usize| -> Vec<f64> {
            (0..nw).map(|x| z_at(x as f64 + 0.5, y as f64 + 0.5)).collect()
        };
        let mut prev = row_z(0);
        for y in 1..nh {
            let cur = row_z(y);
            for x in 0..nw {
                let lvl = cur[x].floor();
                let step_up = lvl != prev[x].floor();
                let step_left = x > 0 && lvl != cur[x - 1].floor();
                if (step_up || step_left) && class[y * nw + x] < 5 {
                    let p = img.get_pixel(x as u32, y as u32);
                    let col = scale_color([p[0] as f64, p[1] as f64, p[2] as f64], 0.78);
                    img.put_pixel(x as u32, y as u32, to_rgba(col));
                }
            }
            prev = cur;
        }
    }

    // --- Streams from their vector entities (per-segment capsule fill,
    // subdivided ~2 m for width interpolation) + refined saddle bridges.
    // Drawn after contours so channels stay clean blue. ---
    {
        let in_view = |p: Vec2, pad: f64| -> bool {
            let (x, y) = to_view(p);
            x >= vx0 - pad && x <= vx1 + pad && y >= vy0 - pad && y <= vy1 + pad
        };
        for &(a, bp, fam) in &b.water.bridges {
            if !in_view(a, 5.0) && !in_view(bp, 5.0) {
                continue;
            }
            let col = if fam == golf_terrain::water::CLASS_WETLAND {
                WETLAND
            } else {
                WATER
            };
            draw_capsule(&mut img, to_img(a), to_img(bp), 3.0 / mpp, col);
        }
        for l in &ct.water.streams {
            if !l.perennial || l.pooled {
                continue;
            }
            for k in 1..l.pts.len() {
                let (a, bp) = (l.pts[k - 1], l.pts[k]);
                let wmax = 0.5 * l.width[k - 1].max(l.width[k]);
                if !in_view(a, wmax + 2.0) && !in_view(bp, wmax + 2.0) {
                    continue;
                }
                let seg_len = a.distance(bp);
                let pieces = (seg_len / 2.0).ceil().max(1.0) as usize;
                for j in 0..pieces {
                    let t0 = j as f64 / pieces as f64;
                    let t1 = (j + 1) as f64 / pieces as f64;
                    let tm = 0.5 * (t0 + t1);
                    let half_w = 0.5 * (l.width[k - 1] + (l.width[k] - l.width[k - 1]) * tm);
                    let w_px = (half_w / mpp).max(0.8);
                    draw_capsule(
                        &mut img,
                        to_img(a.lerp(bp, t0)),
                        to_img(a.lerp(bp, t1)),
                        w_px,
                        WATER,
                    );
                }
            }
        }
    }

    // Line of play + markers.
    for wseg in rh.pts.windows(2) {
        draw_capsule(&mut img, to_img(wseg[0]), to_img(wseg[1]), 1.1, [240.0, 240.0, 240.0]);
    }
    for k in 1..rh.pts.len() - 1 {
        draw_disc(&mut img, to_img(rh.pts[k]), 2.4, [240.0, 240.0, 240.0]);
    }
    // Tee pad ellipse outline.
    {
        let n = 40;
        let (s, c) = (golf_core::math::sin(hb.tee.rot), golf_core::math::cos(hb.tee.rot));
        let mut prev = None;
        for k in 0..=n {
            let th = k as f64 / n as f64 * std::f64::consts::TAU;
            let q = Vec2::new(hb.tee.a * golf_core::math::cos(th), hb.tee.b * golf_core::math::sin(th));
            let p = hb.tee.center + Vec2::new(q.x * c - q.y * s, q.x * s + q.y * c);
            let ip = to_img(p);
            if let Some(pr) = prev {
                draw_capsule(&mut img, pr, ip, 0.8, [250.0, 250.0, 250.0]);
            }
            prev = Some(ip);
        }
    }
    // Green boundary + pin.
    for k in 0..hb.green.outline.len() {
        let a = to_img(hb.green.outline[k]);
        let bpt = to_img(hb.green.outline[(k + 1) % hb.green.outline.len()]);
        draw_capsule(&mut img, a, bpt, 0.9, [245.0, 245.0, 245.0]);
    }
    let (px, py) = to_img(hb.pin);
    draw_disc(&mut img, (px, py), 2.0, [20.0, 20.0, 20.0]);
    draw_capsule(&mut img, (px, py), (px, py - 11.0), 1.1, [250.0, 250.0, 250.0]);
    draw_capsule(&mut img, (px, py - 11.0), (px + 6.5, py - 8.6), 1.5, [232.0, 62.0, 62.0]);

    // North arrow (world +y in view coords), top-right corner.
    {
        let n_view = Vec2::new(right.y, dir.y); // (north·right, north·dir)
        let n_img = Vec2::new(n_view.x, -n_view.y).normalized();
        let cx = iw as f64 - 24.0;
        let cy = 24.0;
        let tip = (cx + n_img.x * 13.0, cy + n_img.y * 13.0);
        let tail = (cx - n_img.x * 13.0, cy - n_img.y * 13.0);
        draw_capsule(&mut img, tail, tip, 1.3, [250.0, 250.0, 250.0]);
        let side = Vec2::new(-n_img.y, n_img.x);
        for sgn in [1.0, -1.0] {
            let wing = (
                tip.0 - n_img.x * 6.0 + side.x * 4.0 * sgn,
                tip.1 - n_img.y * 6.0 + side.y * 4.0 * sgn,
            );
            draw_capsule(&mut img, tip, wing, 1.3, [250.0, 250.0, 250.0]);
        }
        draw_disc(&mut img, (tail.0, tail.1), 2.0, [250.0, 250.0, 250.0]);
    }
    // 50 m scale bar, bottom-left.
    {
        let barlen = 50.0 / mpp;
        let y = ih as f64 - 14.0;
        draw_capsule(&mut img, (12.0, y), (12.0 + barlen, y), 1.4, [250.0, 250.0, 250.0]);
        for x in [12.0, 12.0 + barlen] {
            draw_capsule(&mut img, (x, y - 4.0), (x, y + 4.0), 1.1, [250.0, 250.0, 250.0]);
        }
        draw_number(&mut img, 12.0 + barlen * 0.5 - 8.0, y - 18.0, 50, [250.0, 250.0, 250.0]);
    }
    // Hole number, top-left.
    draw_number(&mut img, 8.0, 8.0, (hole + 1) as u32, [255.0, 255.0, 255.0]);
    img
}

/// Even-odd scanline fill of a closed polygon given in image coordinates,
/// writing the color and a class byte.
fn fill_polygon_scan(
    img: &mut RgbaImage,
    class: &mut [u8],
    nw: usize,
    nh: usize,
    poly: &[(f64, f64)],
    col: Color,
    cls: u8,
) {
    if poly.len() < 3 {
        return;
    }
    let rgba = to_rgba(col);
    let mut xs: Vec<f64> = Vec::with_capacity(16);
    for y in 0..nh {
        let yc = y as f64 + 0.5;
        xs.clear();
        for i in 0..poly.len() {
            let a = poly[i];
            let b = poly[(i + 1) % poly.len()];
            if (a.1 <= yc) != (b.1 <= yc) {
                let t = (yc - a.1) / (b.1 - a.1);
                xs.push(a.0 + (b.0 - a.0) * t);
            }
        }
        xs.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
        for pair in xs.chunks_exact(2) {
            let x0 = pair[0].max(0.0).ceil() as usize;
            let x1 = pair[1].min(nw as f64 - 1.0).floor() as usize;
            if x0 > x1 || x0 >= nw {
                continue;
            }
            for x in x0..=x1 {
                img.put_pixel(x as u32, y as u32, rgba);
                class[y * nw + x] = cls;
            }
        }
    }
}

fn put_safe(img: &mut RgbaImage, x: i64, y: i64, c: Color) {
    if x >= 0 && y >= 0 && (x as u32) < img.width() && (y as u32) < img.height() {
        img.put_pixel(x as u32, y as u32, to_rgba(c));
    }
}

fn draw_capsule(img: &mut RgbaImage, a: (f64, f64), b: (f64, f64), w: f64, col: Color) {
    let (ax, ay) = a;
    let (bx, by) = b;
    let x0 = (ax.min(bx) - w).floor() as i64;
    let x1 = (ax.max(bx) + w).ceil() as i64;
    let y0 = (ay.min(by) - w).floor() as i64;
    let y1 = (ay.max(by) + w).ceil() as i64;
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = (dx * dx + dy * dy).max(1e-9);
    for py in y0..=y1 {
        for px in x0..=x1 {
            let t = (((px as f64 - ax) * dx + (py as f64 - ay) * dy) / len2).clamp(0.0, 1.0);
            let (cx, cy) = (ax + t * dx, ay + t * dy);
            let dd = ((px as f64 - cx).powi(2) + (py as f64 - cy).powi(2)).sqrt();
            if dd <= w {
                put_safe(img, px, py, col);
            }
        }
    }
}

fn draw_dashed(img: &mut RgbaImage, a: (f64, f64), b: (f64, f64), w: f64, col: Color, on: f64, off: f64) {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return;
    }
    let period = on + off;
    let mut s = 0.0;
    while s < len {
        let e = (s + on).min(len);
        let t0 = s / len;
        let t1 = e / len;
        draw_capsule(
            img,
            (a.0 + dx * t0, a.1 + dy * t0),
            (a.0 + dx * t1, a.1 + dy * t1),
            w,
            col,
        );
        s += period;
    }
}

fn draw_disc(img: &mut RgbaImage, c: (f64, f64), rad: f64, col: Color) {
    for py in (c.1 - rad).floor() as i64..=(c.1 + rad).ceil() as i64 {
        for px in (c.0 - rad).floor() as i64..=(c.0 + rad).ceil() as i64 {
            let dd = ((px as f64 - c.0).powi(2) + (py as f64 - c.1).powi(2)).sqrt();
            if dd <= rad {
                put_safe(img, px, py, col);
            }
        }
    }
}

fn draw_ring(img: &mut RgbaImage, c: (f64, f64), rad: f64, col: Color) {
    for py in (c.1 - rad).floor() as i64..=(c.1 + rad).ceil() as i64 {
        for px in (c.0 - rad).floor() as i64..=(c.0 + rad).ceil() as i64 {
            let dd = ((px as f64 - c.0).powi(2) + (py as f64 - c.1).powi(2)).sqrt();
            if (dd - rad).abs() <= 0.7 {
                put_safe(img, px, py, col);
            }
        }
    }
}

fn fill_rect(img: &mut RgbaImage, c: (f64, f64), half: i64, col: Color) {
    for py in -half..=half {
        for px in -half..=half {
            put_safe(img, c.0.round() as i64 + px, c.1.round() as i64 + py, col);
        }
    }
}

fn outline_rect(img: &mut RgbaImage, c: (f64, f64), half: i64, col: Color) {
    for py in -half..=half {
        for px in -half..=half {
            if px.abs() == half || py.abs() == half {
                put_safe(img, c.0.round() as i64 + px, c.1.round() as i64 + py, col);
            }
        }
    }
}

fn draw_diamond(img: &mut RgbaImage, c: (f64, f64), rad: f64, col: Color) {
    for py in (c.1 - rad).floor() as i64..=(c.1 + rad).ceil() as i64 {
        for px in (c.0 - rad).floor() as i64..=(c.0 + rad).ceil() as i64 {
            if (px as f64 - c.0).abs() + (py as f64 - c.1).abs() <= rad {
                put_safe(img, px, py, col);
            }
        }
    }
}

/// 3×5 bitmap digits, doubled to 6×10, with a dark drop shadow.
fn draw_number(img: &mut RgbaImage, x: f64, y: f64, num: u32, col: Color) {
    const GLYPHS: [[u8; 5]; 10] = [
        [0b111, 0b101, 0b101, 0b101, 0b111], // 0
        [0b010, 0b110, 0b010, 0b010, 0b111], // 1
        [0b111, 0b001, 0b111, 0b100, 0b111], // 2
        [0b111, 0b001, 0b111, 0b001, 0b111], // 3
        [0b101, 0b101, 0b111, 0b001, 0b001], // 4
        [0b111, 0b100, 0b111, 0b001, 0b111], // 5
        [0b111, 0b100, 0b111, 0b101, 0b111], // 6
        [0b111, 0b001, 0b010, 0b010, 0b010], // 7
        [0b111, 0b101, 0b111, 0b101, 0b111], // 8
        [0b111, 0b101, 0b111, 0b001, 0b111], // 9
    ];
    let digits: Vec<u32> = if num == 0 {
        vec![0]
    } else {
        let mut ds = Vec::new();
        let mut n = num;
        while n > 0 {
            ds.push(n % 10);
            n /= 10;
        }
        ds.reverse();
        ds
    };
    let mut ox = x.round() as i64;
    let oy = y.round() as i64;
    for d in digits {
        let g = GLYPHS[d as usize];
        for (row, bits) in g.iter().enumerate() {
            for cbit in 0..3 {
                if bits & (0b100 >> cbit) != 0 {
                    for sy in 0..2i64 {
                        for sx in 0..2i64 {
                            let px = ox + cbit as i64 * 2 + sx;
                            let py = oy + row as i64 * 2 + sy;
                            put_safe(img, px + 1, py + 1, [15.0, 15.0, 15.0]);
                            put_safe(img, px, py, col);
                        }
                    }
                }
            }
        }
        ox += 8;
    }
}

/// Tint an already-rendered grid image where a water mask is set (used by the
/// Match tab to overlay real course water and generated seed water on the
/// same footing). The mask grid must cover the same local extent the image
/// was rendered from.
pub fn tint_water_mask(img: &mut RgbaImage, mask: &Grid<f64>) {
    let spec = mask.spec;
    let w_ext = spec.nx as f64 * spec.cell_size;
    let h_ext = spec.ny as f64 * spec.cell_size;
    let scale = img.width().max(img.height()) as f64 / w_ext.max(h_ext);
    let water_col = [58.0, 110.0, 182.0];
    for py in 0..img.height() {
        for px in 0..img.width() {
            let world = Vec2::new(
                (px as f64 + 0.5) / scale,
                h_ext - (py as f64 + 0.5) / scale,
            );
            let m = mask.bilinear(world);
            if m >= 0.35 {
                let a = 0.6 * m.min(1.0);
                let p = img.get_pixel(px, py);
                let blended = blend(
                    [p[0] as f64, p[1] as f64, p[2] as f64],
                    water_col,
                    a,
                );
                img.put_pixel(px, py, to_rgba(blended));
            }
        }
    }
}

/// Tint an already-rendered grid image where a TREE-CANOPY mask is set
/// (Atlas tab: real course canopy over the hillshade). Mirror of
/// [`tint_water_mask`] with a forest-green wash.
pub fn tint_tree_mask(img: &mut RgbaImage, mask: &Grid<f64>) {
    let spec = mask.spec;
    let w_ext = spec.nx as f64 * spec.cell_size;
    let h_ext = spec.ny as f64 * spec.cell_size;
    let scale = img.width().max(img.height()) as f64 / w_ext.max(h_ext);
    let tree_col = [34.0, 78.0, 44.0];
    for py in 0..img.height() {
        for px in 0..img.width() {
            let world = Vec2::new((px as f64 + 0.5) / scale, h_ext - (py as f64 + 0.5) / scale);
            let m = mask.bilinear(world);
            if m >= 0.35 {
                let a = 0.55 * m.min(1.0);
                let p = img.get_pixel(px, py);
                let blended = blend([p[0] as f64, p[1] as f64, p[2] as f64], tree_col, a);
                img.put_pixel(px, py, to_rgba(blended));
            }
        }
    }
}

/// Overlay atlas hole routings on a rendered window image. `holes` are per-hole
/// polylines in WINDOW-LOCAL world meters (origin SW, x east, y north) — the
/// caller projects the course's lat/lon into this frame. Draws a dashed dark
/// underlay, a colored line of play, tee squares, green rings, and hole numbers
/// (shared `HOLE_COLORS`, cycled per 18). `win_w`/`win_h` are the window extent.
pub fn draw_atlas_holes(img: &mut RgbaImage, holes: &[Vec<Vec2>], win_w: f64, win_h: f64) {
    let scale = img.width().max(img.height()) as f64 / win_w.max(win_h);
    // image top = north (matches render_height_grid / tint_water_mask).
    let to_img = |p: Vec2| -> (f64, f64) { (p.x * scale - 0.5, (win_h - p.y) * scale - 0.5) };
    for pts in holes {
        for s in pts.windows(2) {
            draw_dashed(img, to_img(s[0]), to_img(s[1]), 1.0, [30.0, 34.0, 30.0], 6.0, 3.0);
        }
    }
    for (i, pts) in holes.iter().enumerate() {
        if pts.len() < 2 {
            continue;
        }
        let col = HOLE_COLORS[i % 9];
        for s in pts.windows(2) {
            draw_capsule(img, to_img(s[0]), to_img(s[1]), 1.5, col);
        }
        let tee = to_img(pts[0]);
        fill_rect(img, tee, 3, col);
        outline_rect(img, tee, 4, [20.0, 20.0, 20.0]);
        let green = to_img(*pts.last().unwrap());
        draw_disc(img, green, 3.2, col);
        draw_ring(img, green, 4.4, [20.0, 20.0, 20.0]);
    }
}

/// Flow view: log-scaled drainage area (dark ground → bright streams) with
/// lakes tinted blue by depth. Debug/tuning aid for the erosion pass.
pub fn render_flow(flow_area: &Grid<f64>, lake_depth: &Grid<f64>, px: u32) -> RgbaImage {
    let spec = flow_area.spec;
    let a0 = spec.cell_size * spec.cell_size;
    let max_a: f64 = flow_area.data.iter().cloned().fold(a0 * 4.0, f64::max);
    let denom = (max_a / a0).ln();
    let stops = [
        (0.00, [26.0, 34.0, 27.0]),
        (0.35, [44.0, 72.0, 50.0]),
        (0.65, [62.0, 132.0, 122.0]),
        (1.00, [170.0, 232.0, 255.0]),
    ];
    render_field(px, spec.nx, spec.ny, spec.cell_size, |world| {
        let lake = lake_depth.bilinear(world);
        if lake > 0.05 {
            let d = (lake / 6.0).clamp(0.0, 1.0);
            blend([96.0, 152.0, 200.0], [22.0, 62.0, 132.0], d)
        } else {
            let a = (flow_area.bilinear(world) / a0).max(1.0);
            let t = (a.ln() / denom).clamp(0.0, 1.0);
            ramp(&stops, t)
        }
    })
}

/// Render the slope field as a heatmap (flat = pale, steep = hot) at a given size.
pub fn render_slope_px(terrain: &Terrain, px: u32) -> RgbaImage {
    let s = &terrain.slope;
    let spec = s.spec;
    render_field(px, spec.nx, spec.ny, spec.cell_size, |world| {
        // Map slope 0..0.6 into the ramp; clamp above.
        let g = (s.bilinear(world) / 0.6).clamp(0.0, 1.0);
        slope_ramp(g)
    })
}

// --------------------------------------------------------------------------
// Core sampler: iterate output pixels, map to world, call `shade`.
// --------------------------------------------------------------------------

fn render_field(px: u32, nx: u32, ny: u32, cell: f64, shade: impl Fn(Vec2) -> Color) -> RgbaImage {
    // Extent from node spacing (n−1 cells between n nodes would leave a
    // half-texel seam; sampling n cells keeps prior behavior).
    let w_ext = nx as f64 * cell;
    let h_ext = ny as f64 * cell;
    let scale = px as f64 / w_ext.max(h_ext);
    let img_w = (w_ext * scale).round().max(1.0) as u32;
    let img_h = (h_ext * scale).round().max(1.0) as u32;

    let mut img = RgbaImage::new(img_w, img_h);
    for py in 0..img_h {
        for px in 0..img_w {
            // Pixel center -> world, y flipped so north is up.
            let world = Vec2::new(
                (px as f64 + 0.5) / scale,
                h_ext - (py as f64 + 0.5) / scale,
            );
            img.put_pixel(px, py, to_rgba(shade(world)));
        }
    }
    img
}

// --------------------------------------------------------------------------
// Color ramps & shading
// --------------------------------------------------------------------------

/// Vertical exaggeration for hillshading. Course-scale relief is gentle
/// relative to the map width, so slopes are amplified here purely for
/// *visualization* — it never touches the terrain data.
const HILLSHADE_EXAG: f64 = 8.0;

fn hillshade(h: &Grid<f64>, world: Vec2, cell: f64, light: [f64; 3]) -> f64 {
    let e = cell;
    let zx = h.bilinear(world + Vec2::new(e, 0.0)) - h.bilinear(world - Vec2::new(e, 0.0));
    let zy = h.bilinear(world + Vec2::new(0.0, e)) - h.bilinear(world - Vec2::new(0.0, e));
    let n = normalize3([-zx * HILLSHADE_EXAG, -zy * HILLSHADE_EXAG, 2.0 * e]);
    let d = (n[0] * light[0] + n[1] * light[1] + n[2] * light[2]).max(0.0);
    0.30 + 0.70 * d
}

/// Hypsometric tint: low green -> khaki -> tan -> pale highland.
fn hypsometric(t: f64) -> Color {
    // Caps at pale tan, not white — otherwise low-relief terrain fakes an
    // alpine snowcap at its (tiny) highest point.
    let stops = [
        (0.00, [64.0, 104.0, 62.0]),
        (0.40, [112.0, 138.0, 78.0]),
        (0.70, [158.0, 152.0, 106.0]),
        (1.00, [192.0, 182.0, 158.0]),
    ];
    ramp(&stops, t)
}

/// Slope heatmap: pale -> yellow -> orange -> red.
fn slope_ramp(t: f64) -> Color {
    let stops = [
        (0.00, [245.0, 245.0, 235.0]),
        (0.35, [230.0, 210.0, 120.0]),
        (0.70, [220.0, 130.0, 60.0]),
        (1.00, [170.0, 40.0, 40.0]),
    ];
    ramp(&stops, t)
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
    if vals.is_empty() {
        (0.0, 1.0)
    } else {
        (min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use golf_terrain::{generate, world_spec, TerrainParams};

    #[test]
    fn renders_deterministically_at_expected_size() {
        let t = generate(&world_spec(128), 2024, &TerrainParams::default());
        let a = render_height(&t);
        let b = render_height(&t);
        assert_eq!(a.as_raw(), b.as_raw());
        assert_eq!(a.width().max(a.height()), VIEW_PX);
        assert_eq!(render_slope(&t).dimensions(), a.dimensions());
        // A smaller thumbnail render.
        assert_eq!(render_height_px(&t, 128).width().max(render_height_px(&t, 128).height()), 128);
    }
}
