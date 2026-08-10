//! S2 (skeleton) rendering for the stage-lab — the D4 review surface.
//!
//! The P1 lesson from v1: judge structure on a hillshade WITH the network
//! and divides drawn on top, never bare fields. The base view is therefore
//! hillshade + contours + order-colored network + derived divides +
//! embryos; the other views isolate the transform fields S3 will condition
//! on. Candle-wax smoothness between channels is EXPECTED here — texture
//! is S3's job (see stage-02 doc and C1_REVIEW.md's discipline).

use course_skeleton::kernel::Skeleton;
use course_world::grid::Grid;
use course_world::math::Vec2;
use course_world::world::{CORE_MAX_M, CORE_MIN_M, EXTENT_M};
use image::RgbaImage;

const DIVIDE: [u8; 4] = [235, 140, 50, 200];
const EMBRYO: [u8; 4] = [200, 90, 200, 255];
const BASE_EDGE: [u8; 4] = [64, 132, 244, 255];
const WINDOW_BOX: [u8; 4] = [255, 255, 255, 180];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum S2View {
    /// Hillshade of the base surface — the review view.
    Base,
    /// Metres to the nearest channel (the catena coordinate).
    FlowDistance,
    /// Normalized channel→divide coordinate, [0,1].
    FlowNorm,
    /// Hillslope position, [0,1].
    Hillslope,
    /// log10 drained area — where the surface concentrates flow.
    Accum,
}

pub const S2_VIEWS: [(S2View, &str); 5] = [
    (S2View::Base, "base surface"),
    (S2View::FlowDistance, "flow distance"),
    (S2View::FlowNorm, "flow dist (norm)"),
    (S2View::Hillslope, "hillslope pos"),
    (S2View::Accum, "flow accum (log)"),
];

fn order_color(order: u8) -> [u8; 4] {
    match order {
        1 => [120, 175, 250, 255],
        2 => [70, 135, 245, 255],
        3 => [35, 95, 230, 255],
        _ => [15, 60, 200, 255],
    }
}

pub fn render_s2(sk: &Skeleton, view: S2View, px: u32, overlays: bool) -> RgbaImage {
    let mut img = match view {
        S2View::Base => {
            let mut i = course_viz::render_height_grid(&sk.height, px, None);
            course_viz::overlay_contours(&mut i, &sk.height, 4.0);
            i
        }
        S2View::FlowDistance => {
            let hi = percentile(&sk.flow_distance, 0.95).max(1.0);
            course_viz::render_scalar_field(&sk.flow_distance, px, 0.0, hi)
        }
        S2View::FlowNorm => course_viz::render_scalar_field(&sk.flow_distance_norm, px, 0.0, 1.0),
        S2View::Hillslope => {
            course_viz::render_scalar_field(&sk.hillslope_position, px, 0.0, 1.0)
        }
        S2View::Accum => {
            let mut g = sk.flow_accum.clone();
            for v in &mut g.data {
                *v = (*v).max(1.0).log10();
            }
            let hi = percentile(&g, 0.999).max(1.0);
            course_viz::render_scalar_field(&g, px, 1.5, hi)
        }
    };
    if !overlays {
        return img;
    }
    let (w, h) = (img.width(), img.height());

    // Network, order-colored, thicker downstream.
    for c in &sk.channels {
        let color = order_color(c.order);
        let thick = ((c.order as i64) - 1).clamp(0, 2);
        for wnd in c.pts.windows(2) {
            let a = course_viz::to_px(wnd[0], w, h);
            let b = course_viz::to_px(wnd[1], w, h);
            course_viz::draw_line(&mut img, a, b, color, thick);
        }
    }
    // Derived divides — MAJOR chains only (≥ 600 m), smoothed. Drawing
    // every minor inter-finger catchment tessellates the tile into
    // square-ish cells (D8 boundaries staircase on a smooth surface — an
    // artifact S3's texture dissolves, not a landform to exhibit).
    let spec8 = sk.flow_distance.spec;
    let on_channel = |p: Vec2| {
        let gx = ((p.x / spec8.cell_size) as u32).min(spec8.nx - 1);
        let gy = ((p.y / spec8.cell_size) as u32).min(spec8.ny - 1);
        sk.flow_distance.data[(gy * spec8.nx + gx) as usize] == 0.0
    };
    for line in sk.divides.iter().filter(|l| l.len() >= 75) {
        let sm = smooth_polyline(line, 2);
        for wnd in sm.windows(2) {
            // The moving average can bridge a smoothed divide across a
            // channel meander — skip any segment touching a channel cell
            // (a divide crossing a channel is definitionally wrong).
            let mid = Vec2::new((wnd[0].x + wnd[1].x) * 0.5, (wnd[0].y + wnd[1].y) * 0.5);
            if on_channel(wnd[0]) || on_channel(mid) || on_channel(wnd[1]) {
                continue;
            }
            let a = course_viz::to_px(wnd[0], w, h);
            let b = course_viz::to_px(wnd[1], w, h);
            course_viz::draw_line(&mut img, a, b, DIVIDE, 0);
        }
    }
    // Intended pits.
    for e in &sk.embryos {
        draw_circle(&mut img, e.center, e.radius_m, EMBRYO);
    }
    // Base-level edge band + play-window core box.
    draw_edge_band(&mut img, sk.meta.base_level.edge);
    let c0 = course_viz::to_px(Vec2::new(CORE_MIN_M, CORE_MIN_M), w, h);
    let c1 = course_viz::to_px(Vec2::new(CORE_MAX_M, CORE_MAX_M), w, h);
    for (a, b) in [
        ((c0.0, c0.1), (c1.0, c0.1)),
        ((c1.0, c0.1), (c1.0, c1.1)),
        ((c1.0, c1.1), (c0.0, c1.1)),
        ((c0.0, c1.1), (c0.0, c0.1)),
    ] {
        course_viz::draw_line(&mut img, a, b, WINDOW_BOX, 0);
    }
    img
}

fn smooth_polyline(pts: &[Vec2], half: usize) -> Vec<Vec2> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    (0..pts.len())
        .map(|i| {
            let lo = i.saturating_sub(half);
            let hi = (i + half).min(pts.len() - 1);
            let n = (hi - lo + 1) as f64;
            let (sx, sy) = pts[lo..=hi]
                .iter()
                .fold((0.0, 0.0), |(ax, ay), p| (ax + p.x, ay + p.y));
            Vec2::new(sx / n, sy / n)
        })
        .collect()
}

fn percentile(g: &Grid<f64>, q: f64) -> f64 {
    let mut v: Vec<f64> = g.data.iter().copied().filter(|x| x.is_finite()).collect();
    if v.is_empty() {
        return 0.0;
    }
    v.sort_by(|a, b| a.total_cmp(b));
    v[((v.len() - 1) as f64 * q) as usize]
}

fn draw_circle(img: &mut RgbaImage, center: Vec2, radius_m: f64, color: [u8; 4]) {
    let (w, h) = (img.width(), img.height());
    let n = 40;
    let mut prev: Option<(i64, i64)> = None;
    for k in 0..=n {
        let a = k as f64 / n as f64 * std::f64::consts::TAU;
        let p = Vec2::new(center.x + radius_m * a.cos(), center.y + radius_m * a.sin());
        let q = course_viz::to_px(p, w, h);
        if let Some(pr) = prev {
            course_viz::draw_line(img, pr, q, color, 0);
        }
        prev = Some(q);
    }
}

fn draw_edge_band(img: &mut RgbaImage, edge: course_contracts::metadata::Edge) {
    use course_contracts::metadata::Edge;
    let (w, h) = (img.width(), img.height());
    let e = EXTENT_M;
    let segs: &[(Vec2, Vec2)] = match edge {
        Edge::S => &[(Vec2::new(0.0, 0.0), Vec2::new(e, 0.0))],
        Edge::N => &[(Vec2::new(0.0, e), Vec2::new(e, e))],
        Edge::W => &[(Vec2::new(0.0, 0.0), Vec2::new(0.0, e))],
        Edge::E => &[(Vec2::new(e, 0.0), Vec2::new(e, e))],
        Edge::CornerSw => &[
            (Vec2::new(0.0, 0.0), Vec2::new(e * 0.5, 0.0)),
            (Vec2::new(0.0, 0.0), Vec2::new(0.0, e * 0.5)),
        ],
        Edge::CornerSe => &[
            (Vec2::new(e * 0.5, 0.0), Vec2::new(e, 0.0)),
            (Vec2::new(e, 0.0), Vec2::new(e, e * 0.5)),
        ],
        Edge::CornerNw => &[
            (Vec2::new(0.0, e * 0.5), Vec2::new(0.0, e)),
            (Vec2::new(0.0, e), Vec2::new(e * 0.5, e)),
        ],
        Edge::CornerNe => &[
            (Vec2::new(e * 0.5, e), Vec2::new(e, e)),
            (Vec2::new(e, e * 0.5), Vec2::new(e, e)),
        ],
    };
    for (a, b) in segs {
        let pa = course_viz::to_px(*a, w, h);
        let pb = course_viz::to_px(*b, w, h);
        course_viz::draw_line(img, pa, pb, BASE_EDGE, 2);
    }
}
