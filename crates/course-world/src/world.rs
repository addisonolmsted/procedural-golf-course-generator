//! The fixed world geometry (ARCHITECTURE.md "World geometry"): 3 km × 3 km,
//! origin at the SW corner, x east, y north, elevations in metres, with the
//! central 1.5 km × 1.5 km routable core. Ported from
//! `archetype-pipeline:course-contracts/src/lib.rs`.

use crate::grid::GridSpec;
use crate::math::Vec2;

/// Generated world: 3 km × 3 km, origin at the SW corner.
pub const EXTENT_M: f64 = 3000.0;
/// Routable core: the central 1.5 km × 1.5 km window `[CORE_MIN_M, CORE_MAX_M]²`.
pub const CORE_MIN_M: f64 = 750.0;
pub const CORE_MAX_M: f64 = 2250.0;

/// Canonical working resolution for steps 3–8 grids.
pub const RES_FULL_M: f64 = 2.0;
/// GUI / fast-iteration preview resolution (same code path, coarser grid).
pub const RES_PREVIEW_M: f64 = 8.0;
/// Step-9 earthworks patches are local grids at this resolution.
pub const RES_EARTHWORKS_M: f64 = 0.5;

/// Grid spec covering the full extent at `res_m` (node convention: nx = n+1,
/// so nodes sit exactly on the box edges and coarse node sets nest in fine
/// ones when resolutions divide evenly).
pub fn world_spec(res_m: f64) -> GridSpec {
    let n = (EXTENT_M / res_m).round() as u32;
    GridSpec::new(Vec2 { x: 0.0, y: 0.0 }, res_m, n + 1, n + 1)
}

/// True if the world-space point is inside the routable core window.
pub fn in_core(p: Vec2) -> bool {
    p.x >= CORE_MIN_M && p.x <= CORE_MAX_M && p.y >= CORE_MIN_M && p.y <= CORE_MAX_M
}

/// FNV-1a over f64 bit patterns — the workspace's golden-hash convention.
pub fn fnv_f64(data: &[f64]) -> u64 {
    let mut h = 1469598103934665603u64;
    for z in data {
        h ^= z.to_bits();
        h = h.wrapping_mul(1099511628211);
    }
    h
}

/// FNV-1a over raw bytes.
pub fn fnv_bytes(data: &[u8]) -> u64 {
    let mut h = 1469598103934665603u64;
    for b in data {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_spec_node_counts() {
        let full = world_spec(RES_FULL_M);
        assert_eq!((full.nx, full.ny), (1501, 1501));
        let prev = world_spec(RES_PREVIEW_M);
        assert_eq!((prev.nx, prev.ny), (376, 376));
        // Edge nodes land exactly on the box boundary.
        assert_eq!(full.world_of(1500, 1500), Vec2::new(EXTENT_M, EXTENT_M));
    }

    #[test]
    fn coarse_nodes_nest_in_fine() {
        // 8 m nodes coincide with every 4th 2 m node.
        let fine = world_spec(RES_FULL_M);
        let coarse = world_spec(RES_PREVIEW_M);
        for k in [0u32, 1, 100, 375] {
            assert_eq!(coarse.world_of(k, 0), fine.world_of(k * 4, 0));
        }
    }

    #[test]
    fn core_window() {
        assert!(in_core(Vec2::new(1500.0, 1500.0)));
        assert!(!in_core(Vec2::new(500.0, 1500.0)));
        assert!(in_core(Vec2::new(CORE_MIN_M, CORE_MAX_M)));
    }
}
