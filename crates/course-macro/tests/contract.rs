//! steps/03-macro-landform.md hard requirements, tested end-to-end against
//! the planner over all five archetypes (forced via step-02 overrides —
//! never by branching inside step 03).

use std::collections::BTreeMap;

use course_seed::RunIdentity;
use course_spec::{ArchetypeId, CourseSpec, SpecOverrides};
use course_world::math::Vec2;
use course_world::world::{in_core, CORE_MAX_M, CORE_MIN_M, EXTENT_M};
use course_macro::skeleton::SpineKind;
use course_macro::{generate_skeleton, planner, resolve};

fn spec_for(arch: ArchetypeId, seed: u64) -> CourseSpec {
    let overrides = SpecOverrides {
        forced_archetype: Some(arch),
        params: BTreeMap::new(),
    };
    CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &overrides).unwrap()
}

/// Hard requirement 1 — monotone drains, accordant junctions, and (unless
/// closed-basin-only) a drain that exits the box.
#[test]
fn drains_monotone_accordant_and_exiting() {
    const SMIN_DIP_TOL: f64 = 2.0;
    for arch in ArchetypeId::ALL {
        for seed in 0..10u64 {
            let spec = spec_for(arch, seed);
            let plan = planner::plan(&spec);
            let cfg = planner::lower(&plan);
            let r = resolve(&cfg);

            for vi in 0..r.n_valleys() {
                // Monotonicity is a property of the CARVING reach. Past its
                // arc cap a tributary's floor rises on purpose (`tail_rise`,
                // see ResolvedValley::with_cap) so the smooth-min against the
                // trunk is an exact no-op rather than a groove — scanning
                // into that tail measures the blending device, not the drain.
                let u_end = match r.valley_arc_cap(vi) {
                    Some(cap) => (cap / r.valley_spine(vi).length()).min(0.99),
                    None => 0.99,
                };
                let mut running_min = f64::INFINITY;
                let mut u = 0.01;
                while u <= u_end {
                    let (p, _floor) = r.valley_floor(vi, u);
                    if p.x >= 0.0 && p.x <= EXTENT_M && p.y >= 0.0 && p.y <= EXTENT_M {
                        let z = r.height_at(p);
                        assert!(
                            z <= running_min + SMIN_DIP_TOL,
                            "{arch:?} seed {seed} valley {vi}: rises {:.2} m at u={u:.2}",
                            z - running_min
                        );
                        running_min = running_min.min(z);
                    }
                    u += 0.005;
                }
            }

            // Accordant junctions: tributary floor at its junction == trunk
            // floor there (resolve() snapped it; verify end-to-end).
            for vi in 1..r.n_valleys() {
                let Some(cap) = r.valley_arc_cap(vi) else { continue };
                let u_j = cap / r.valley_spine(vi).length();
                let (jp, trib_floor) = r.valley_floor(vi, u_j);
                let hit = r.valley_spine(0).project(jp);
                let (_, trunk_floor) = r.valley_floor(0, hit.u);
                assert!(
                    (trib_floor - trunk_floor).abs() < 1e-6,
                    "{arch:?} seed {seed} trib {vi}: junction floors differ"
                );
            }

            // Exit: when drains exist, the trunk's downstream end leaves the box.
            if !plan.frame.closed_basin_only && r.n_valleys() > 0 {
                let sp = r.valley_spine(0);
                let end = *sp.pts.last().unwrap();
                let outside = end.x < 0.0 || end.x > EXTENT_M || end.y < 0.0 || end.y > EXTENT_M;
                assert!(outside, "{arch:?} seed {seed}: trunk does not exit ({end:?})");
                assert!(plan.frame.exit_edge.is_some());
            }
            if plan.frame.closed_basin_only {
                assert_eq!(r.n_valleys(), 0, "{arch:?} seed {seed}");
                assert!(plan.frame.exit_edge.is_none());
            }
        }
    }
}

/// Hard requirement 2 — core relief ≤ cap after the budget solve (measured
/// on the solve's own fixed 16 m preview; 5% tolerance for the preview's
/// sampling of extrema).
#[test]
fn core_relief_capped() {
    for arch in ArchetypeId::ALL {
        for seed in 0..10u64 {
            let spec = spec_for(arch, seed);
            let plan = planner::plan(&spec);
            let cap = plan.budget.cap_m;
            assert!(
                plan.budget.relief_final_m <= cap * 1.05,
                "{arch:?} seed {seed}: core relief {:.1} m > cap {:.1} m (raw {:.1}; notes {:?})",
                plan.budget.relief_final_m,
                cap,
                plan.budget.relief_raw_m,
                plan.budget.notes
            );
        }
    }
}

/// Hard requirement 2, adversarial: absurd overridden relief/tilt still
/// respects the cap (the budget solve is an enforcement, not a hope).
#[test]
fn core_relief_capped_adversarial() {
    let mut params = BTreeMap::new();
    params.insert("landform.relief_amp_m".to_string(), 250.0);
    params.insert("landform.tilt_grade".to_string(), 0.09);
    params.insert("landform.ridge_count".to_string(), 4.0);
    let overrides = SpecOverrides {
        forced_archetype: Some(ArchetypeId::MountainBench),
        params,
    };
    for seed in 0..5u64 {
        let spec =
            CourseSpec::generate_builtin(RunIdentity::from_seed(seed), &overrides).unwrap();
        let plan = planner::plan(&spec);
        assert!(
            plan.budget.relief_final_m <= plan.budget.cap_m * 1.05,
            "seed {seed}: {:.1} > cap {:.1} (notes {:?})",
            plan.budget.relief_final_m,
            plan.budget.cap_m,
            plan.budget.notes
        );
    }
}

/// Hard requirement 3 — fields consistent with the geometry they summarize.
#[test]
fn fields_match_geometry() {
    let spec = spec_for(ArchetypeId::Piedmont, 3);
    let res = generate_skeleton(&spec, 16.0);
    let s = &res.skeleton;
    let drains: Vec<_> = s
        .structure
        .spines
        .iter()
        .filter(|sp| sp.kind == SpineKind::Drain)
        .collect();
    assert!(!drains.is_empty(), "piedmont should have drains");

    // floor_damp ≈ 1 exactly on the drain centerline (in-box points).
    for sp in &drains {
        for p in sp.pts.iter().step_by(8) {
            if p.x < 20.0 || p.x > EXTENT_M - 20.0 || p.y < 20.0 || p.y > EXTENT_M - 20.0 {
                continue;
            }
            let fd = s.fields.floor_damp.bilinear(*p);
            assert!(fd > 0.95, "floor_damp {fd:.3} at {p:?}");
            // valley_dist ≈ 0 on the spine (bilinear + 10 m polyline slack).
            let vd = s.fields.valley_dist_m.bilinear(*p);
            assert!(vd < 30.0, "valley_dist {vd:.1} on spine at {p:?}");
        }
    }

    // valley_dist_m matches brute-force distance to the published drain
    // polylines at probe points.
    let mut rng = course_seed::DetRng::new(7, b"contract-probes");
    for _ in 0..40 {
        let p = Vec2::new(rng.range_f64(50.0, 2950.0), rng.range_f64(50.0, 2950.0));
        let brute = drains
            .iter()
            .flat_map(|sp| sp.pts.windows(2))
            .map(|w| seg_dist(p, w[0], w[1]))
            .fold(f64::INFINITY, f64::min);
        let field = s.fields.valley_dist_m.bilinear(p);
        assert!(
            (field - brute).abs() < 25.0,
            "valley_dist {field:.1} vs brute {brute:.1} at {p:?}"
        );
    }

    // core_protect: 1 well inside, 0 well outside.
    assert!(s.fields.core_protect.bilinear(Vec2::new(1500.0, 1500.0)) > 0.999);
    assert!(s.fields.core_protect.bilinear(Vec2::new(300.0, 300.0)) < 1e-9);
    assert!(in_core(Vec2::new((CORE_MIN_M + CORE_MAX_M) / 2.0, 1500.0)));
}

fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.dot(ab);
    let t = if len2 == 0.0 { 0.0 } else { ((p - a).dot(ab) / len2).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}

/// Hard requirement 4 — resolution consistency: a 48 m raster is a node
/// subset of the 16 m raster, bit-exact (same analytic sampling path).
#[test]
fn coarse_is_downsample_of_fine() {
    let spec = spec_for(ArchetypeId::GlacialMoraine, 5);
    let fine = generate_skeleton(&spec, 16.0);
    let coarse = generate_skeleton(&spec, 48.0);
    let fs = &fine.skeleton;
    let cs = &coarse.skeleton;
    let n = cs.base_height.spec.nx.min(fs.base_height.spec.nx / 3);
    for y in (0..n).step_by(7) {
        for x in (0..n).step_by(7) {
            let pc = cs.base_height.spec.world_of(x, y);
            let pf = fs.base_height.spec.world_of(x * 3, y * 3);
            assert_eq!(pc, pf, "node positions must coincide");
            assert_eq!(
                cs.base_height.get(x, y).to_bits(),
                fs.base_height.get(x * 3, y * 3).to_bits(),
                "base_height differs at {pc:?}"
            );
            assert_eq!(
                cs.fields.valley_dist_m.get(x, y).to_bits(),
                fs.fields.valley_dist_m.get(x * 3, y * 3).to_bits()
            );
            assert_eq!(
                cs.fields.grain_dir_rad.get(x, y).to_bits(),
                fs.fields.grain_dir_rad.get(x * 3, y * 3).to_bits()
            );
        }
    }
    // Structure graph identical across resolutions.
    assert_eq!(fs.structure, cs.structure);
}

/// Hard requirement 5 — deterministic double-run: byte-identical artifact dirs.
#[test]
fn deterministic_artifact_double_run() {
    let dir_a = std::env::temp_dir().join("macro_skel_det_a");
    let dir_b = std::env::temp_dir().join("macro_skel_det_b");
    for arch in [ArchetypeId::Piedmont, ArchetypeId::Sandhills] {
        let spec = spec_for(arch, 11);
        let a = generate_skeleton(&spec, 32.0);
        let b = generate_skeleton(&spec, 32.0);
        course_macro::artifact::write_dir(&dir_a, &spec, &a).unwrap();
        course_macro::artifact::write_dir(&dir_b, &spec, &b).unwrap();
        assert_eq!(
            course_macro::artifact::dir_fnv(&dir_a).unwrap(),
            course_macro::artifact::dir_fnv(&dir_b).unwrap(),
            "{arch:?}: artifact dirs differ"
        );
        // Round-trip: the contract part loads back.
        let (skel, meta) = course_macro::artifact::load_skeleton(&dir_a).unwrap();
        assert_eq!(meta.seed, 11);
        assert_eq!(skel.structure, a.skeleton.structure);
        let plan = course_macro::artifact::load_plan(&dir_a).unwrap();
        assert_eq!(plan, a.plan);
    }
}

/// Mountain always threads a bench system through the core with a clear
/// tread (its routability lever); the core-center band stays scarp-free.
#[test]
fn mountain_bench_through_core_with_clear_tread() {
    for seed in 0..8u64 {
        let spec = spec_for(ArchetypeId::MountainBench, seed);
        let plan = planner::plan(&spec);
        assert!(
            !plan.uplands.scarps.is_empty(),
            "seed {seed}: mountain has no bench scarps"
        );
        assert!(plan.uplands.core_tread_w_m >= 250.0);
        // No scarp crosses the core-center tread band.
        let d = plan.frame.downhill;
        let center = Vec2::new(1500.0, 1500.0);
        for sp in &plan.uplands.scarps {
            let spine = match &sp.bluff.path {
                course_macro::Path::Points(pts) => {
                    course_world::spline::Spine::new(course_world::spline::catmull_rom(pts, 10.0))
                }
                _ => unreachable!("scarps are point paths"),
            };
            for k in -8i32..=8 {
                let probe = center + d * (k as f64 * 25.0);
                assert!(
                    spine.project(probe).d > 40.0,
                    "seed {seed}: scarp crosses the core tread near {probe:?}"
                );
            }
        }
    }
}

/// Sandhills: dune grain rides in the fields (grain ≈ global azimuth away
/// from drains), and blowout basins exist without any drains.
#[test]
fn sandhills_grain_and_blowouts() {
    let mut saw_closed = false;
    for seed in 0..8u64 {
        let spec = spec_for(ArchetypeId::Sandhills, seed);
        let plan = planner::plan(&spec);
        assert!(plan.frame.dune_wavelength_m >= 150.0);
        if plan.frame.closed_basin_only {
            saw_closed = true;
            assert!(!plan.basins.bowls.is_empty(), "seed {seed}: no blowouts");
        }
    }
    assert!(saw_closed, "sandhills never drew the closed-basin case");
}

/// The budget solve may ease valley WALLS out of the core only where no
/// drain floor feels the core weight at all — otherwise the eased reach
/// lifts relative to its neighbours and the drain ponds (which is exactly
/// what `drains_monotone_accordant_and_exiting` caught when this flag was
/// inferred from `tangent_routed` instead of measured).
///
/// `plan()` decides the flag from the AUTHORED paths; this checks the
/// RESOLVED spines, which are what actually get carved — junction snapping
/// trims and overshoots a tributary's tail, so the two are not the same
/// curve.
#[test]
fn relaxed_valleys_never_feel_the_core() {
    let mut checked = 0;
    for arch in ArchetypeId::ALL {
        for seed in 0..24u64 {
            let plan = planner::plan(&spec_for(arch, seed));
            if !plan.drainage.floors_clear_of_core {
                continue;
            }
            let r = resolve(&planner::lower(&plan));
            for vi in 0..r.n_valleys() {
                let sp = r.valley_spine(vi);
                for k in 0..=400 {
                    let p = sp.point_at(k as f64 / 400.0);
                    let w = course_macro::fields::core_protect_at(p);
                    assert!(
                        w <= 0.0,
                        "{arch:?} seed {seed} valley {vi}: floor at {p:?} sits in the \
                         core-protect field (weight {w:.3}), so relaxing valleys there \
                         would pond the drain"
                    );
                }
            }
            checked += 1;
        }
    }
    assert!(checked > 0, "no relaxed-valley plan in the sweep — nothing was proved");
}
