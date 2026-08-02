//! Primitive-engine property tests, ported from
//! `terrain-v2:golf-landform/tests/property.rs` (goldens re-blessed for this
//! crate's RNG domain — course-seed's DetRng tag differs from golf-core's,
//! so meander phases legitimately changed).

use course_seed::DetRng;
use course_world::math::Vec2;
use course_world::spline::Spine;
use course_macro::prims::meander;
use course_macro::presets::{preset, presets};
use course_macro::{field_hash, generate, resolve, MeanderSpec, MIN_FALL_GRADIENT};

/// Composed elevation along every valley's floor centerline must be
/// non-increasing, up to the bounded smooth-min dip where the valley surface
/// and terrain nearly coincide (dip <= k/4 — a scour pool, never a dam).
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
                if f < prev {
                    prev = f;
                } else if f > prev {
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

/// C1 shoulders: transects across valley presets never jump in slope.
#[test]
fn transects_have_no_slope_jumps() {
    for name in ["barranca", "canyon_straight", "floodplain"] {
        let cfg = preset(name).unwrap();
        let r = resolve(&cfg);
        for &u in &[0.3, 0.5, 0.7] {
            let (c, _f) = r.valley_floor(0, u);
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

/// 1k random meanders — curvature clamp + separation always hold.
#[test]
fn meander_fuzz_1k() {
    let mut rng = DetRng::new(0x51AE, b"landform-fuzz");
    let mut degenerate = 0;
    for i in 0..1_000 {
        let spec = fuzz_spec(&mut rng, 2500.0);
        if spec.entry.distance(spec.exit) < 4.0 * spec.width_m {
            degenerate += 1;
            continue;
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
    assert!(degenerate < 500, "too many degenerate specs: {degenerate}");
}

/// Same config + resolution → identical bits (the primitive engine draws no
/// ambient RNG). The golden pin guards the port against silent drift.
#[test]
fn deterministic_and_golden() {
    let cfg = preset("barranca").unwrap();
    let a = generate(&cfg, 20.0);
    let b = generate(&cfg, 20.0);
    assert_eq!(field_hash(&a), field_hash(&b));
    let h = field_hash(&a);
    assert_eq!(h, GOLDEN_BARRANCA_20M, "landform golden drifted: {h}");
}

// Blessed on port (2026-07-26): course-seed DetRng domain tag differs from
// golf-core's, so meander phases differ from terrain-v2's pinned value.
// Re-bless only on an intentional MACRO_VERSION bump.
// Re-blessed 2026-08-01 (M1 perf): the Valley/Ridge exact early-outs. Their
// soundness is proven by `prims::valley::bound_tests` (the bound never
// crosses the surface) and the residual is measured at ULP scale by
// `valley_early_out_is_ulp_scale` — so this hash moved for arithmetic
// association, not geometry.
const GOLDEN_BARRANCA_20M: u64 = 8513372680968690812;


/// Accordance at DEPTH — a tributary of a tributary of a tributary.
///
/// `join_trunk` is an index to any EARLIER valley, so nesting has always
/// been legal, but the planner has only ever emitted `Some(0)` — every
/// junction in production is depth 1. The network work (M5) will emit depth
/// 3-5, and the corridor trim (`while pts.len() > 2 { pop }`) plus the
/// overshoot past the junction are exactly the parts that could misbehave
/// when the parent is itself short and already trimmed. This pins the
/// contract before the planner relies on it.
#[test]
fn accordance_holds_at_depth_four() {
    use course_macro::{resolve, MacroConfig, Path, Valley};
    use course_world::math::Vec2;
    use course_world::profile::Profile;

    let reach = |pts: Vec<Vec2>, parent: Option<usize>, hw: f64| Valley {
        path: Path::Points(pts),
        floor_z0_m: 200.0,
        fall_gradient: 0.02,
        floor_halfwidth: Profile::constant(hw),
        wall_grad_left: 0.25,
        wall_grad_right: 0.25,
        floor_round_m: 5.0,
        shoulder_k_m: 4.0,
        join_trunk: parent,
        crest_k_m: None,
        crest_rise_m: 20.0,
        wall_convexity: 0.0,
        fall_profile: None,
    };

    // A trunk crossing the box, then a chain: trib -> sub-trib -> sub-sub.
    let cfg = MacroConfig {
        schema_version: course_macro::SCHEMA_VERSION,
        extent_m: 3000.0,
        base_elev_m: 200.0,
        core_relax: 1.0,
        core_relax_valleys: false,
        tilt: course_macro::Tilt {
            grade_x: -0.004,
            grade_y: 0.0,
            curve_m: 0.0,
            core_tread: 1.0,
        },
        ridges: vec![],
        bluffs: vec![],
        bowls: vec![],
        valleys: vec![
            reach(vec![Vec2::new(-100.0, 1500.0), Vec2::new(3100.0, 1400.0)], None, 24.0),
            reach(vec![Vec2::new(900.0, 2600.0), Vec2::new(1200.0, 1480.0)], Some(0), 14.0),
            reach(vec![Vec2::new(500.0, 2500.0), Vec2::new(1010.0, 2180.0)], Some(1), 9.0),
            reach(vec![Vec2::new(300.0, 2900.0), Vec2::new(640.0, 2470.0)], Some(2), 6.0),
        ],
    };
    let r = resolve(&cfg);
    assert_eq!(r.n_valleys(), 4);

    // Every tributary's floor at its junction equals its parent's floor there.
    for (child, parent) in [(1usize, 0usize), (2, 1), (3, 2)] {
        let cap = r
            .valley_arc_cap(child)
            .unwrap_or_else(|| panic!("valley {child} has no arc cap — it did not snap"));
        let u_j = cap / r.valley_spine(child).length();
        let (jp, child_floor) = r.valley_floor(child, u_j);
        let hit = r.valley_spine(parent).project(jp);
        let (_, parent_floor) = r.valley_floor(parent, hit.u);
        assert!(
            (child_floor - parent_floor).abs() < 1e-6,
            "depth junction {child}->{parent}: child floor {child_floor} != parent {parent_floor}"
        );
    }

    // And the composed surface still falls monotonically down every reach.
    for vi in 0..r.n_valleys() {
        let u_end = match r.valley_arc_cap(vi) {
            Some(cap) => (cap / r.valley_spine(vi).length()).min(0.99),
            None => 0.99,
        };
        let mut running_min = f64::INFINITY;
        let mut u = 0.01;
        while u <= u_end {
            let (p, _) = r.valley_floor(vi, u);
            if p.x >= 0.0 && p.x <= 3000.0 && p.y >= 0.0 && p.y <= 3000.0 {
                let z = r.height_at(p);
                assert!(z <= running_min + 2.0, "valley {vi} rises {:.2} m at u={u:.2}", z - running_min);
                running_min = running_min.min(z);
            }
            u += 0.005;
        }
    }
}
