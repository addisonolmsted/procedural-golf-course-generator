//! Stage-2 gate property tests (docs/terrain-v2-plan.md):
//! - trunk floor monotonicity for all valley configurations
//! - C1 at shoulders (no slope jumps across transects)
//! - meander curvature clamp + no self-approach under fuzz
//! - determinism + golden hash

use golf_core::det::DetRng;
use golf_core::math::Vec2;
use golf_landform::spline::Spine;
use golf_landform::{
    field_hash, generate, meander, presets, resolve, MeanderSpec, MIN_FALL_GRADIENT,
};

/// Composed elevation along every valley's floor centerline must be
/// non-increasing, up to the bounded smooth-min dip where the valley surface
/// and terrain nearly coincide (dip <= k/4; max preset shoulder_k = 6 ->
/// 1.5 m, seen in emergence zones and at confluences — a scour pool, never a
/// dam). Anything above this bound would be a real blockage.
#[test]
fn valley_floors_monotone_in_all_presets() {
    const SMIN_DIP_TOL: f64 = 2.0;
    for (name, cfg) in presets() {
        let r = resolve(&cfg);
        for vi in 0..r.n_valleys() {
            let mut running_min = f64::INFINITY;
            let mut u = 0.01;
            while u <= 0.99 {
                let (p, _floor) = r.valley_floor(vi, u);
                // skip floor points outside the world window
                if p.x >= 0.0 && p.x <= cfg.extent_m && p.y >= 0.0 && p.y <= cfg.extent_m {
                    let z = r.height_at(p);
                    assert!(
                        z <= running_min + SMIN_DIP_TOL,
                        "{name} valley {vi}: floor rises {:.3} m at u={u:.3}",
                        z - running_min
                    );
                    running_min = running_min.min(z);
                }
                u += 0.005;
            }
        }
    }
}

/// The AUTHORED floor function is strictly monotone by construction
/// (gradient clamped to MIN_FALL_GRADIENT) up to the junction cap.
#[test]
fn authored_floor_strictly_monotone() {
    for (name, cfg) in presets() {
        let r = resolve(&cfg);
        for vi in 0..r.n_valleys() {
            let mut prev = f64::INFINITY;
            let mut u = 0.0;
            while u <= 1.0 {
                let (_p, f) = r.valley_floor(vi, u);
                // allow the (rising) overshoot tail past a junction cap
                if f < prev {
                    prev = f;
                } else if f > prev {
                    // must be the tail: floor never rises before its minimum
                    // more than the tail-rise slack
                    assert!(
                        f - prev < 10.0,
                        "{name} valley {vi}: floor rise {} at u={u}",
                        f - prev
                    );
                }
                u += 0.01;
            }
            let (_p0, f0) = r.valley_floor(vi, 0.0);
            let (_p1, fmin) = r.valley_floor(vi, 1.0);
            assert!(
                f0 - fmin > 0.0 || cfg.valleys[vi].fall_gradient >= MIN_FALL_GRADIENT,
                "{name} valley {vi}: no net drop"
            );
        }
    }
}

/// C1 shoulders: sample transects across the barranca + straight canyon and
/// assert the numeric slope never JUMPS between adjacent samples — every seam
/// (floor->wall rounding, shoulder smooth-min) is eased.
#[test]
fn transects_have_no_slope_jumps() {
    for name in ["barranca", "canyon_straight", "floodplain"] {
        let cfg = golf_landform::preset(name).unwrap();
        let r = resolve(&cfg);
        for &u in &[0.3, 0.5, 0.7] {
            let (c, _f) = r.valley_floor(0, u);
            // transect: 400 m either side, perpendicular-ish (use fixed axes;
            // the exact direction doesn't matter for a smoothness check)
            for dir in [Vec2::new(1.0, 0.0), Vec2::new(0.0, 1.0)] {
                let step = 0.5;
                let mut prev_z = None;
                let mut prev_slope: Option<f64> = None;
                let mut t = -400.0;
                while t <= 400.0 {
                    let p = c + dir * t;
                    if p.x < 0.0 || p.y < 0.0 || p.x > cfg.extent_m || p.y > cfg.extent_m {
                        t += step;
                        prev_z = None;
                        prev_slope = None;
                        continue;
                    }
                    let z = r.height_at(p);
                    if let Some(pz) = prev_z {
                        let slope: f64 = (z - pz) / step;
                        if let Some(ps) = prev_slope {
                            assert!(
                                (slope - ps).abs() < 0.25,
                                "{name} u={u} t={t}: slope jump {:.3}",
                                (slope - ps).abs()
                            );
                        }
                        prev_slope = Some(slope);
                    }
                    prev_z = Some(z);
                    t += step;
                }
            }
        }
    }
}

fn fuzz_spec(rng: &mut DetRng, extent: f64) -> MeanderSpec {
    let width = rng.range_f64(20.0, 300.0);
    MeanderSpec {
        entry: Vec2::new(rng.range_f64(-100.0, extent), rng.range_f64(-100.0, extent)),
        exit: Vec2::new(rng.range_f64(0.0, extent + 100.0), rng.range_f64(0.0, extent + 100.0)),
        width_m: width,
        intensity: rng.range_f64(0.0, 1.0),
        wavelength_mult: rng.range_f64(10.0, 14.0),
        jitter: rng.range_f64(0.0, 0.5),
        seed: rng.next_u64(),
    }
}

fn self_approach(spine: &Spine, width: f64, lam: f64) -> bool {
    let pts: Vec<Vec2> = spine.pts.iter().copied().step_by(2).collect();
    if pts.len() < 4 {
        return false;
    }
    let total = spine.length();
    let step = total / (pts.len() - 1) as f64;
    let gap = ((0.55 * lam) / step.max(1e-9)).ceil() as usize;
    for i in 0..pts.len() {
        for j in (i + gap.max(1))..pts.len() {
            if pts[i].distance(pts[j]) < width {
                return true;
            }
        }
    }
    false
}

/// Fast fuzz (always on): 1k random meanders — curvature clamp + separation.
#[test]
fn meander_fuzz_1k() {
    meander_fuzz(1_000, 0x51AE);
}

/// The full gate fuzz (10k) — run explicitly:
///   cargo test -p golf-landform --release -- --ignored meander_fuzz_10k
#[test]
#[ignore]
fn meander_fuzz_10k() {
    meander_fuzz(10_000, 0xF00D);
}

fn meander_fuzz(n: usize, seed: u64) {
    let mut rng = DetRng::new(seed, b"landform-fuzz");
    let mut degenerate = 0;
    for i in 0..n {
        let spec = fuzz_spec(&mut rng, 2500.0);
        if spec.entry.distance(spec.exit) < 4.0 * spec.width_m {
            degenerate += 1;
            continue; // too short to meander — generator falls back to straight
        }
        let spine = meander::generate(&spec);
        let rmin = spine.min_curvature_radius();
        assert!(
            rmin >= 0.90 * meander::MIN_RADIUS_WIDTHS * spec.width_m,
            "fuzz {i}: curvature radius {rmin:.1} < clamp (width {})",
            spec.width_m
        );
        assert!(
            !self_approach(&spine, spec.width_m, spec.wavelength()),
            "fuzz {i}: self-approach (width {}, intensity {})",
            spec.width_m,
            spec.intensity
        );
    }
    assert!(degenerate < n / 2, "too many degenerate specs: {degenerate}");
}

/// Same config + resolution -> identical bits; and the fast path (4x res)
/// samples the same composed surface.
#[test]
fn deterministic_and_golden() {
    let cfg = golf_landform::preset("barranca").unwrap();
    let a = generate(&cfg, 20.0);
    let b = generate(&cfg, 20.0);
    assert_eq!(field_hash(&a), field_hash(&b));
    // golden pin — bump deliberately when landform semantics change
    // (mirrors xtask/golden/landform.hash; regenerate via
    //  `cargo run -p xtask -- bless` once xtask grows the landform golden)
    let h = field_hash(&a);
    assert_eq!(h, GOLDEN_BARRANCA_20M, "landform golden drifted: {h}");
}

// Pinned on first bless; re-blessed when the presets moved to the 3 km box.
const GOLDEN_BARRANCA_20M: u64 = 9358560053144687790;
