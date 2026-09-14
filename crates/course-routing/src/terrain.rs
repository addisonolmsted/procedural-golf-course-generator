//! The router's input: the frozen terrain as `course-sandhills` emits it.
//!
//! Two ways in: from the generator's grids in memory (the game), or from a
//! `.cgrid` pair on disk (`m_<seed>.cgrid` + `m_<seed>.water.cgrid`, the
//! form every screen and the Python reference read).

use course_world::grid::Grid;
use course_world::gridio;
use std::path::Path;

use crate::Mode;

#[derive(Clone, Debug)]
pub struct Terrain {
    pub mode: Mode,
    /// 2 m heightfield over the 3 km extent (1501 x 1501 nodes), NaN-free
    pub z2: Grid<f64>,
    /// 2 m wet mask: the water surface is finite there
    pub wet2: Vec<bool>,
    /// creek / river centre-lines in world metres `(x, y)` as the generator
    /// keeps them; empty on a dry tile. Not used by the router today (it
    /// reads water through `wet2`), carried for the sheets.
    pub lines: Vec<Vec<course_world::math::Vec2>>,
}

impl Terrain {
    /// From the generator's grids: `height` at 2 m, `water` the 2 m water
    /// surface with NaN where dry.
    pub fn from_grids(mode: Mode, height: &Grid<f64>, water: &Grid<f64>,
                      lines: Vec<Vec<course_world::math::Vec2>>) -> Terrain {
        let mut z2 = height.clone();
        // NaN-fill with the mean, as the reference does (`np.nanmean`)
        let finite: Vec<f64> = z2.data.iter().copied().filter(|v| v.is_finite()).collect();
        let mean = if finite.is_empty() { 0.0 } else { finite.iter().sum::<f64>() / finite.len() as f64 };
        for v in z2.data.iter_mut() {
            if !v.is_finite() {
                *v = mean;
            }
        }
        let wet2 = water.data.iter().map(|v| v.is_finite()).collect();
        Terrain { mode, z2, wet2, lines }
    }

    /// From a dump directory: `<prefix><seed>.cgrid`, `.water.cgrid`, and
    /// the optional `.creek.txt` (one `x y` per line).
    pub fn load(dir: &Path, prefix: &str, seed: u64, mode: Mode) -> std::io::Result<Terrain> {
        let height = gridio::read_grid_f32(&dir.join(format!("{prefix}{seed}.cgrid")))?;
        let wp = dir.join(format!("{prefix}{seed}.water.cgrid"));
        let water = if wp.exists() {
            gridio::read_grid_f32(&wp)?
        } else {
            Grid::filled(height.spec, f64::NAN)
        };
        let mut lines = Vec::new();
        let lp = dir.join(format!("{prefix}{seed}.creek.txt"));
        if let Ok(txt) = std::fs::read_to_string(&lp) {
            let pts: Vec<course_world::math::Vec2> = txt.lines().filter_map(|l| {
                let mut it = l.split_whitespace();
                let x: f64 = it.next()?.parse().ok()?;
                let y: f64 = it.next()?.parse().ok()?;
                Some(course_world::math::Vec2::new(x, y))
            }).collect();
            if pts.len() > 1 {
                lines.push(pts);
            }
        }
        Ok(Terrain::from_grids(mode, &height, &water, lines))
    }
}
