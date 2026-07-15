//! Elliptical built-up tee pads.
//!
//! Real tees are constructed platforms — the atlas showed a +1 m median
//! drive drop that raw landform can't produce. Each pad is a flat ellipse
//! aligned with the opening shot, lifted 0.3–1.5 m (less where the ground
//! already falls away), feathered back to the terrain.

use golf_core::det::DetRng;
use golf_core::math::{self, Vec2};
use golf_core::Grid;
use golf_routing::construct::normal;
use golf_routing::Hole;
use golf_terrain::CourseTerrain;

use crate::{patch_spec, Patch, TeePad};

/// Feather ring beyond the pad edge, m.
const TEE_FEATHER: f64 = 3.5;

pub fn shape(ct: &CourseTerrain, hole: &Hole, rng: &mut DetRng) -> TeePad {
    let center = hole.pts[0];
    let dir = (hole.pts[1] - hole.pts[0]).normalized();
    let rot = math::atan2(dir.y, dir.x);
    let a = rng.range_f64(5.0, 7.0);
    let b = rng.range_f64(3.0, 4.5);

    // Local base: p75 of the landform within the ellipse footprint.
    let mut samples: Vec<f64> = Vec::with_capacity(9);
    for k in 0..8 {
        let th = k as f64 / 8.0 * std::f64::consts::TAU;
        let q = Vec2::new(0.7 * a * math::cos(th), 0.7 * b * math::sin(th));
        let (s, c) = (math::sin(rot), math::cos(rot));
        let p = center + Vec2::new(q.x * c - q.y * s, q.x * s + q.y * c);
        samples.push(ct.macro_heights.bilinear(p));
    }
    samples.push(ct.macro_heights.bilinear(center));
    samples.sort_by(|x, y| x.partial_cmp(y).unwrap());
    let base_hi = samples[6]; // ~p75 of 9

    // Lift less where the forward ground already drops away.
    let natural_drop = golf_routing::geometry::drive_drop(ct, &hole.pts);
    let lift = (0.9 + 0.35 * normal(rng) - 0.25 * (natural_drop / 5.0).clamp(-1.0, 1.0))
        .clamp(0.3, 1.5);

    TeePad {
        center,
        rot,
        a,
        b,
        z: base_hi + lift,
    }
}

pub fn patch(ct: &CourseTerrain, te: &TeePad) -> Patch {
    let ext = te.a.max(te.b) + TEE_FEATHER + 1.0;
    let spec = patch_spec(
        te.center - Vec2::new(ext, ext),
        te.center + Vec2::new(ext, ext),
        0.5,
    );
    let r_feather = 1.0 + TEE_FEATHER / te.a.min(te.b);
    let mut z = Grid::filled(spec, 0.0f64);
    let mut w = Grid::filled(spec, 0.0f64);
    for gy in 0..spec.ny {
        for gx in 0..spec.nx {
            let p = spec.world_of(gx, gy);
            let i = spec.index(gx, gy);
            let rn = te.rnorm(p);
            if rn > r_feather {
                // Sane default beyond the feather (w = 0 there).
                z.data[i] = ct.macro_heights.bilinear(p);
                continue;
            }
            if rn <= 1.0 {
                z.data[i] = te.z;
                w.data[i] = 1.0;
            } else {
                let t = math::smoothstep(0.0, 1.0, (rn - 1.0) / (r_feather - 1.0));
                z.data[i] = te.z + (ct.height_at(p) - te.z) * t;
                w.data[i] = 1.0 - t;
            }
        }
    }
    Patch { z, w }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ellipse_rnorm() {
        let te = TeePad {
            center: Vec2::new(100.0, 100.0),
            rot: 0.9,
            a: 6.0,
            b: 4.0,
            z: 10.0,
        };
        assert!(te.rnorm(te.center) < 1e-9);
        // Point on the long axis at distance a is on the boundary.
        let dir = Vec2::new(math::cos(0.9), math::sin(0.9));
        let p = te.center + dir * 6.0;
        assert!((te.rnorm(p) - 1.0).abs() < 1e-9);
    }
}
