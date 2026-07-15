//! Regular sampled grids with an explicit spec.
//!
//! Resolution is a first-class, explicit concept: COARSE (hydrology) and FINE
//! (playable surface) grids are separate `GridSpec`s and never implicitly the
//! same grid. The single sanctioned bridge between resolutions is
//! [`Grid::bilinear`] sampling.

use crate::math::{floor, Vec2};
use serde::{Deserialize, Serialize};

/// Describes a regular grid of nodes. Node (x, y) sits at
/// `origin + (x, y) * cell_size` in world space (node convention, not cell).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct GridSpec {
    pub origin: Vec2,
    pub cell_size: f64,
    pub nx: u32,
    pub ny: u32,
}

impl GridSpec {
    pub fn new(origin: Vec2, cell_size: f64, nx: u32, ny: u32) -> Self {
        GridSpec {
            origin,
            cell_size,
            nx,
            ny,
        }
    }

    pub fn len(&self) -> usize {
        self.nx as usize * self.ny as usize
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn index(&self, x: u32, y: u32) -> usize {
        debug_assert!(x < self.nx && y < self.ny);
        y as usize * self.nx as usize + x as usize
    }

    /// World position of node (x, y).
    pub fn world_of(&self, x: u32, y: u32) -> Vec2 {
        Vec2::new(
            self.origin.x + x as f64 * self.cell_size,
            self.origin.y + y as f64 * self.cell_size,
        )
    }
}

/// A dense grid of `T` laid out row-major (y outer, x inner). The layout is
/// fixed so serialized bytes and content hashes are stable.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Grid<T> {
    pub spec: GridSpec,
    pub data: Vec<T>,
}

impl<T: Clone> Grid<T> {
    pub fn filled(spec: GridSpec, value: T) -> Self {
        let data = vec![value; spec.len()];
        Grid { spec, data }
    }

    #[inline]
    pub fn get(&self, x: u32, y: u32) -> &T {
        &self.data[self.spec.index(x, y)]
    }

    #[inline]
    pub fn set(&mut self, x: u32, y: u32, v: T) {
        let i = self.spec.index(x, y);
        self.data[i] = v;
    }
}

impl<T> Grid<T> {
    pub fn from_data(spec: GridSpec, data: Vec<T>) -> Self {
        assert_eq!(spec.len(), data.len(), "grid data length must match spec");
        Grid { spec, data }
    }
}

impl Grid<f64> {
    /// Bilinear sample at an arbitrary world position, clamped to the grid
    /// bounds at the edges. This is the one resample operator that bridges
    /// coarse<->fine resolutions.
    pub fn bilinear(&self, world: Vec2) -> f64 {
        let s = &self.spec;
        if s.nx == 0 || s.ny == 0 {
            return 0.0;
        }
        // Convert to fractional grid coordinates.
        let gx = (world.x - s.origin.x) / s.cell_size;
        let gy = (world.y - s.origin.y) / s.cell_size;

        let clampf = |v: f64, hi: u32| -> f64 {
            if v < 0.0 {
                0.0
            } else {
                let m = (hi - 1) as f64;
                if v > m {
                    m
                } else {
                    v
                }
            }
        };
        let gx = clampf(gx, s.nx);
        let gy = clampf(gy, s.ny);

        let x0 = floor(gx) as u32;
        let y0 = floor(gy) as u32;
        let x1 = (x0 + 1).min(s.nx - 1);
        let y1 = (y0 + 1).min(s.ny - 1);
        let tx = gx - x0 as f64;
        let ty = gy - y0 as f64;

        let v00 = *self.get(x0, y0);
        let v10 = *self.get(x1, y0);
        let v01 = *self.get(x0, y1);
        let v11 = *self.get(x1, y1);
        let a = v00 + (v10 - v00) * tx;
        let b = v01 + (v11 - v01) * tx;
        a + (b - a) * ty
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec() -> GridSpec {
        GridSpec::new(Vec2::ZERO, 1.0, 3, 2)
    }

    #[test]
    fn index_layout_is_row_major() {
        let s = spec();
        assert_eq!(s.index(0, 0), 0);
        assert_eq!(s.index(2, 0), 2);
        assert_eq!(s.index(0, 1), 3);
        assert_eq!(s.len(), 6);
    }

    #[test]
    fn get_set_roundtrip() {
        let mut g = Grid::filled(spec(), 0.0f64);
        g.set(2, 1, 9.0);
        assert_eq!(*g.get(2, 1), 9.0);
    }

    #[test]
    fn bilinear_hits_nodes_exactly() {
        let mut g = Grid::filled(spec(), 0.0f64);
        g.set(0, 0, 1.0);
        g.set(1, 0, 3.0);
        assert_eq!(g.bilinear(Vec2::new(0.0, 0.0)), 1.0);
        assert_eq!(g.bilinear(Vec2::new(1.0, 0.0)), 3.0);
        // Midpoint interpolates.
        assert_eq!(g.bilinear(Vec2::new(0.5, 0.0)), 2.0);
    }

    #[test]
    fn bilinear_clamps_out_of_bounds() {
        let mut g = Grid::filled(spec(), 0.0f64);
        g.set(0, 0, 5.0);
        assert_eq!(g.bilinear(Vec2::new(-10.0, -10.0)), 5.0);
    }
}
