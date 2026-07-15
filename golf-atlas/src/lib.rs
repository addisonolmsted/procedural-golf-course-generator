//! `golf-atlas`: real-course reference data from the Parkland Atlas survey.
//!
//! 27 surveyed courses, each a 256×256 elevation grid over its bounding box
//! plus a water mask and per-hole routing polylines. This is the ground truth
//! the noise sampler's distributions are fit against, and the target set for
//! the seed-matching search.
//!
//! Two on-disk forms:
//! - the original `parkland_atlas.html` ([`parse::parse_html`], used once to pack)
//! - a compact binary ([`binfmt`], `assets/atlas.bin`) loaded at runtime

pub mod binfmt;
pub mod parse;

use golf_core::{Grid, GridSpec, Vec2};

/// Source elevation grids are always this size.
pub const GRID_N: usize = 256;

/// Comparison windows are capped at the generator's world size (2 km).
pub const COMPARE_MAX_M: f64 = 2000.0;

/// The atlas's own three-band grouping of course styles.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StyleGroup {
    /// Under ~30 m of relief.
    Lowland,
    /// The classic parkland band.
    Rolling,
    /// Over ~80 m of relief.
    Mountain,
}

impl StyleGroup {
    pub fn label(self) -> &'static str {
        match self {
            StyleGroup::Lowland => "lowland",
            StyleGroup::Rolling => "rolling",
            StyleGroup::Mountain => "mountain & canyon",
        }
    }

    pub fn code(self) -> u8 {
        match self {
            StyleGroup::Lowland => 0,
            StyleGroup::Rolling => 1,
            StyleGroup::Mountain => 2,
        }
    }

    pub fn from_code(c: u8) -> Option<Self> {
        match c {
            0 => Some(StyleGroup::Lowland),
            1 => Some(StyleGroup::Rolling),
            2 => Some(StyleGroup::Mountain),
            _ => None,
        }
    }
}

/// One hole's line of play (tee → green), in survey lat/lon.
#[derive(Clone, Debug)]
pub struct Hole {
    pub ref_no: u32,
    pub par: Option<u32>,
    pub pts_ll: Vec<(f64, f64)>,
}

/// One surveyed course.
#[derive(Clone, Debug)]
pub struct Course {
    pub key: String,
    pub label: String,
    pub arch: String,
    pub group: StyleGroup,
    /// Survey extent in meters: west→east and north→south.
    pub wm: f64,
    pub hm: f64,
    /// Elevation range over the whole survey, meters ASL.
    pub emin: f64,
    pub emax: f64,
    /// `GRID_N × GRID_N` elevations (meters ASL), row-major, **row 0 = north edge**.
    pub heights: Vec<f32>,
    /// Water mask (0/1), same layout as `heights`.
    pub water: Vec<u8>,
    /// Tree-canopy mask (0/1), same layout as `heights` (ESA WorldCover class 10).
    pub trees: Vec<u8>,
    pub waterpct: f32,
    pub treepct: f32,
    /// Survey bounding box `[lat0, lon0, lat1, lon1]`.
    pub bbox: [f64; 4],
    pub holes: Vec<Hole>,
}

/// The loaded course set, in atlas display order (lowland → rolling → mountain).
#[derive(Clone, Debug)]
pub struct Atlas {
    pub courses: Vec<Course>,
    /// Content hash of the elevation data; embedded in search-result caches so
    /// stale results are detected if the atlas changes.
    pub fingerprint: u64,
}

impl Atlas {
    pub fn course(&self, key: &str) -> Option<&Course> {
        self.courses.iter().find(|c| c.key == key)
    }
}

/// A course's comparison window: the centered crop of its survey, capped at
/// 2 km per axis, resampled to square `mpp`-meter pixels.
///
/// The grid follows the generator's convention (**row 0 = south edge**, y up =
/// north), so it can be compared cell-for-cell against generated terrain
/// sampled at the same meters-per-pixel.
#[derive(Clone, Debug)]
pub struct Template {
    pub course_key: String,
    /// Window extent in meters — exactly `nx·mpp × ny·mpp`.
    pub win_w: f64,
    pub win_h: f64,
    /// Heights in meters ASL (absolute — matching removes only a DC offset).
    pub grid: Grid<f64>,
}

impl Course {
    /// Total relief of the full survey extent.
    pub fn relief(&self) -> f64 {
        self.emax - self.emin
    }

    /// Bilinear sample of the survey: `x_m` meters from the west edge,
    /// `y_from_north_m` meters from the north edge. Source cells are treated
    /// as pixel-centered; edges clamp.
    pub fn sample(&self, x_m: f64, y_from_north_m: f64) -> f64 {
        let n = GRID_N;
        let cw = self.wm / n as f64;
        let ch = self.hm / n as f64;
        let hi = (n - 1) as f64;
        let gx = (x_m / cw - 0.5).clamp(0.0, hi);
        let gy = (y_from_north_m / ch - 0.5).clamp(0.0, hi);
        let x0 = gx as usize;
        let y0 = gy as usize;
        let x1 = (x0 + 1).min(n - 1);
        let y1 = (y0 + 1).min(n - 1);
        let fx = gx - x0 as f64;
        let fy = gy - y0 as f64;
        let at = |x: usize, y: usize| self.heights[y * n + x] as f64;
        let a = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let b = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        a + (b - a) * fy
    }

    /// Comparison-window extent at `mpp`: the survey capped at 2 km per axis,
    /// floor-quantized to whole pixels.
    pub fn window_dims(&self, mpp: f64) -> (f64, f64) {
        let nx = ((self.wm.min(COMPARE_MAX_M) / mpp).floor() as u32).max(2);
        let ny = ((self.hm.min(COMPARE_MAX_M) / mpp).floor() as u32).max(2);
        (nx as f64 * mpp, ny as f64 * mpp)
    }

    /// Resample the centered `win_w × win_h` crop of the survey at `mpp`.
    /// Row 0 = south (generator convention). Use [`Course::window_dims`]-sized
    /// windows to re-extract the same crop at a different resolution.
    pub fn window_grid(&self, win_w: f64, win_h: f64, mpp: f64) -> Grid<f64> {
        let nx = ((win_w / mpp).round() as u32).max(2);
        let ny = ((win_h / mpp).round() as u32).max(2);
        let x0 = (self.wm - win_w) / 2.0;
        let y0n = (self.hm - win_h) / 2.0; // from the north edge

        let spec = GridSpec::new(Vec2::ZERO, mpp, nx, ny);
        let mut grid = Grid::filled(spec, 0.0f64);
        for iy in 0..ny {
            // Row 0 = south: flip against the source's north-first rows.
            let y_from_north = y0n + win_h - (iy as f64 + 0.5) * mpp;
            for ix in 0..nx {
                let x = x0 + (ix as f64 + 0.5) * mpp;
                grid.set(ix, iy, self.sample(x, y_from_north));
            }
        }
        grid
    }

    /// Resample the centered crop of the survey's WATER MASK (nearest
    /// neighbor, 1.0 = water). Same geometry/conventions as
    /// [`Course::window_grid`] — row 0 = south.
    pub fn water_window(&self, win_w: f64, win_h: f64, mpp: f64) -> Grid<f64> {
        let n = GRID_N;
        let nx = ((win_w / mpp).round() as u32).max(2);
        let ny = ((win_h / mpp).round() as u32).max(2);
        let x0 = (self.wm - win_w) / 2.0;
        let y0n = (self.hm - win_h) / 2.0;
        let cw = self.wm / n as f64;
        let ch = self.hm / n as f64;
        let hi = (n - 1) as f64;

        let spec = GridSpec::new(Vec2::ZERO, mpp, nx, ny);
        let mut grid = Grid::filled(spec, 0.0f64);
        for iy in 0..ny {
            let y_from_north = y0n + win_h - (iy as f64 + 0.5) * mpp;
            let gy = (y_from_north / ch - 0.5).round().clamp(0.0, hi) as usize;
            for ix in 0..nx {
                let x = x0 + (ix as f64 + 0.5) * mpp;
                let gx = (x / cw - 0.5).round().clamp(0.0, hi) as usize;
                grid.set(ix, iy, self.water[gy * n + gx] as f64);
            }
        }
        grid
    }

    /// Resample the centered crop of the survey's TREE-CANOPY MASK (nearest
    /// neighbor, 1.0 = tree). Same geometry/conventions as
    /// [`Course::water_window`] — row 0 = south.
    pub fn tree_window(&self, win_w: f64, win_h: f64, mpp: f64) -> Grid<f64> {
        let n = GRID_N;
        let nx = ((win_w / mpp).round() as u32).max(2);
        let ny = ((win_h / mpp).round() as u32).max(2);
        let x0 = (self.wm - win_w) / 2.0;
        let y0n = (self.hm - win_h) / 2.0;
        let cw = self.wm / n as f64;
        let ch = self.hm / n as f64;
        let hi = (n - 1) as f64;

        let spec = GridSpec::new(Vec2::ZERO, mpp, nx, ny);
        let mut grid = Grid::filled(spec, 0.0f64);
        for iy in 0..ny {
            let y_from_north = y0n + win_h - (iy as f64 + 0.5) * mpp;
            let gy = (y_from_north / ch - 0.5).round().clamp(0.0, hi) as usize;
            for ix in 0..nx {
                let x = x0 + (ix as f64 + 0.5) * mpp;
                let gx = (x / cw - 0.5).round().clamp(0.0, hi) as usize;
                grid.set(ix, iy, self.trees[gy * n + gx] as f64);
            }
        }
        grid
    }

    /// Extract the comparison window at `mpp` meters per pixel.
    pub fn template(&self, mpp: f64) -> Template {
        let (win_w, win_h) = self.window_dims(mpp);
        Template {
            course_key: self.key.clone(),
            win_w,
            win_h,
            grid: self.window_grid(win_w, win_h, mpp),
        }
    }
}

impl Template {
    /// Relief of the window between the 2nd and 98th height percentiles —
    /// robust to single-cell spikes in the survey data.
    pub fn relief_p2_p98(&self) -> f64 {
        percentile_span(&self.grid.data, 0.02, 0.98)
    }

    pub fn min_max(&self) -> (f64, f64) {
        let mut lo = f64::INFINITY;
        let mut hi = f64::NEG_INFINITY;
        for &v in &self.grid.data {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        (lo, hi)
    }
}

/// Span between two quantiles of `vals` (sorted copy; fine at template sizes).
pub fn percentile_span(vals: &[f64], lo_q: f64, hi_q: f64) -> f64 {
    if vals.is_empty() {
        return 0.0;
    }
    let mut v: Vec<f64> = vals.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = |q: f64| -> f64 {
        let i = q * (v.len() - 1) as f64;
        let i0 = i.floor() as usize;
        let i1 = (i0 + 1).min(v.len() - 1);
        v[i0] + (v[i1] - v[i0]) * (i - i0 as f64)
    };
    idx(hi_q) - idx(lo_q)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Synthetic course: elevation rises linearly toward the north edge.
    fn north_high_course() -> Course {
        let mut heights = vec![0.0f32; GRID_N * GRID_N];
        for y in 0..GRID_N {
            for x in 0..GRID_N {
                // Row 0 = north = highest.
                heights[y * GRID_N + x] = (GRID_N - y) as f32;
            }
        }
        Course {
            key: "test".into(),
            label: "Test".into(),
            arch: String::new(),
            group: StyleGroup::Rolling,
            wm: 1600.0,
            hm: 1200.0,
            emin: 0.0,
            emax: GRID_N as f64,
            heights,
            water: vec![0; GRID_N * GRID_N],
            trees: vec![0; GRID_N * GRID_N],
            waterpct: 0.0,
            treepct: 0.0,
            bbox: [0.0; 4],
            holes: Vec::new(),
        }
    }

    #[test]
    fn template_is_south_up_flipped() {
        let c = north_high_course();
        let t = c.template(25.0);
        // Generator convention: higher row index = further north = higher here.
        let south = *t.grid.get(0, 0);
        let north = *t.grid.get(0, t.grid.spec.ny - 1);
        assert!(north > south, "north row must be high: {north} vs {south}");
        // Window covers the full survey (both dims < 2 km), pixel count exact.
        assert_eq!(t.win_w, t.grid.spec.nx as f64 * 25.0);
        assert!(t.win_w <= c.wm && t.win_h <= c.hm);
    }

    #[test]
    fn template_caps_at_2km() {
        let mut c = north_high_course();
        c.wm = 2900.0;
        c.hm = 2100.0;
        let t = c.template(12.5);
        assert!(t.win_w <= COMPARE_MAX_M && t.win_h <= COMPARE_MAX_M);
        assert_eq!(t.grid.spec.nx, 160);
        assert_eq!(t.grid.spec.ny, 160);
    }

    #[test]
    fn percentile_span_basics() {
        let vals: Vec<f64> = (0..=100).map(|i| i as f64).collect();
        let s = percentile_span(&vals, 0.02, 0.98);
        assert!((s - 96.0).abs() < 1e-9);
    }
}
