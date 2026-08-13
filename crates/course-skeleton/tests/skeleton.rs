//! Stage S2 acceptance suite (`docs/stages/stage-02-skeleton-kernel.md`).

use course_contracts::biome::BiomeId;
use course_seed::RunIdentity;
use course_skeleton::kernel::Skeleton;
use course_spec::v2::{SiteSpec, SpecOverridesV2};
use course_world::math::Vec2;
use course_world::world::fnv_f64;

fn build(seed: u64, biome: Option<BiomeId>) -> (SiteSpec, Skeleton) {
    let id = RunIdentity::from_seed(seed);
    let spec =
        SiteSpec::generate_builtin(id, &SpecOverridesV2 { forced_biome: biome });
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    (spec, sk)
}

fn digest(sk: &Skeleton) -> [u64; 6] {
    [
        fnv_f64(&sk.height.data),
        fnv_f64(&sk.flow_accum.data),
        fnv_f64(&sk.flow_distance.data),
        fnv_f64(&sk.flow_distance_norm.data),
        fnv_f64(&sk.hillslope_position.data),
        sk.channels.len() as u64,
    ]
}

#[test]
fn determinism_double_run() {
    let (_, a) = build(7, None);
    let (_, b) = build(7, None);
    assert_eq!(digest(&a), digest(&b));
}

#[test]
fn meta_is_bit_identical_to_c1() {
    let id = RunIdentity::from_seed(3);
    let spec = SiteSpec::generate_builtin(id, &Default::default());
    let c1 = course_primitives::generate(&spec, &id);
    let sk = course_skeleton::generate(&spec, &c1, &id);
    assert_eq!(sk.meta, c1.meta);
}

#[test]
fn all_six_biomes_produce_skeletons() {
    for biome in BiomeId::ALL {
        let (_, sk) = build(11, Some(biome));
        assert!(sk.height.data.iter().all(|v| v.is_finite()), "{biome:?}");
        assert!(
            sk.flow_distance_norm.data.iter().all(|v| (0.0..=1.0).contains(v)),
            "{biome:?} fdn out of range"
        );
        assert!(
            sk.hillslope_position.data.iter().all(|v| (0.0..=1.0).contains(v)),
            "{biome:?} hp out of range"
        );
    }
}

#[test]
fn sandhills_is_deranged_but_still_carries_dry_drainage() {
    // This test used to assert sandhills has NO channels. The corpus says
    // otherwise: measured with the same accumulation cut as everything
    // else, real sandhills carries d2c 118 m and 2.36 km/km² — flow
    // concentrates on any real surface whether or not a perennial stream
    // runs there, and those lines are the dry swales between dunes (the
    // review saw them on the tiles). What makes sandhills sandhills is
    // that its basins swallow the water: the network exists but does not
    // reach base level.
    let (_, sk) = build(21, Some(BiomeId::Sandhills));
    assert!(
        sk.diagnostics.channel_count > 0,
        "sandhills should carry dry drainage lines (corpus d2c 118 m)"
    );
    assert!(
        sk.diagnostics.connectivity < 0.5,
        "sandhills must be deranged, connectivity {}",
        sk.diagnostics.connectivity
    );
    assert!(sk.flow_distance.data.iter().all(|v| v.is_finite()));
    assert!(sk.flow_distance_norm.data.iter().all(|v| (0.0..=1.0).contains(v)));
}

#[test]
fn heathland_is_deranged_and_separates_from_integrated_biomes() {
    let (_, heath) = build(31, Some(BiomeId::Heathland));
    let (_, pied) = build(31, Some(BiomeId::Piedmont));
    // Heathland: sparse or no channels; what exists connects poorly.
    // Piedmont: integrated — most channel cells drain to the border.
    assert!(
        pied.diagnostics.connectivity > 0.6,
        "piedmont connectivity {}",
        pied.diagnostics.connectivity
    );
    if heath.diagnostics.channel_count > 0 {
        assert!(
            heath.diagnostics.connectivity < pied.diagnostics.connectivity,
            "heathland {} !< piedmont {}",
            heath.diagnostics.connectivity,
            pied.diagnostics.connectivity
        );
    }
    // Kettle embryos are heathland's mechanism and must be recorded.
    assert!(!heath.embryos.is_empty(), "heathland grew no kettles");
}

#[test]
fn network_is_hierarchical_on_integrated_biomes() {
    // Bands RE-BASED on the corpus (tools/macro_campaign/horton_real.py).
    // The old 3–5 / 1.5–3 band came from the authored engine, which built
    // its hierarchy top-down so the ratios held by construction. A DERIVED
    // network's ratios are an outcome, and they depend on the extraction
    // cut and the reach definition — so the corpus was measured with the
    // identical cut and reach splitting: Rb median 2.1–3.1 (p10 1.75–2.33)
    // and Rl median 0.92–1.16 with a p10 spread down to 0.51, across the
    // six biomes, with Ω of 2–3.
    // MEDIAN over several seeds, because that is how the corpus band was
    // measured (a per-tile median across 10 tiles). A single tile's ratio
    // is noisy on both sides — real piedmont spans 1.83–3.44 tile to tile,
    // and a one-seed assertion just samples that spread.
    let mut rbs = Vec::new();
    let mut rls = Vec::new();
    let mut max_orders = Vec::new();
    for seed in [41u64, 42, 43, 44, 45, 46, 47, 48] {
        let (_, sk) = build(seed, Some(BiomeId::Piedmont));
        max_orders.push(sk.channels.iter().map(|c| c.order).max().unwrap_or(0));
        if let Some(rb) = sk.diagnostics.bifurcation_ratio {
            rbs.push(rb);
        }
        if let Some(rl) = sk.diagnostics.length_ratio {
            rls.push(rl);
        }
    }
    max_orders.sort_unstable();
    assert!(
        max_orders[max_orders.len() / 2] >= 2,
        "median Strahler max {} < 2 — flat network",
        max_orders[max_orders.len() / 2]
    );
    rbs.sort_by(|a, b| a.total_cmp(b));
    rls.sort_by(|a, b| a.total_cmp(b));
    let rb = rbs[rbs.len() / 2];
    let rl = rls[rls.len() / 2];
    assert!((1.7..=4.5).contains(&rb), "median bifurcation ratio {rb} outside the corpus band");
    assert!((0.5..=1.9).contains(&rl), "median length ratio {rl} outside the corpus band");
}

#[test]
fn channels_flow_downhill_on_the_height_surface() {
    // Acceptance: every channel cell's flow_dir points downhill on height.
    // height2 at 8 m node positions equals height8 exactly (aligned nodes),
    // so sampling height at cell centers ± dir is a faithful check.
    let (_, sk) = build(41, Some(BiomeId::Piedmont));
    let spec8 = sk.flow_distance.spec;
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    // Ponded reaches are exempt: where fill raises the surface the channel
    // crosses a basin at spill level and the raw ground genuinely rises at
    // the lip (S4 paints these as water; the flat router crosses them).
    let mut h8 = course_world::grid::Grid::filled(spec8, 0.0f64);
    for y in 0..ny {
        for x in 0..nx {
            h8.data[y * nx + x] = *sk.height.get((x * 4) as u32, (y * 4) as u32);
        }
    }
    let ff = course_world::flow::route(&h8);
    let mut bad = 0;
    let mut total = 0;
    for lin in 0..nx * ny {
        if sk.flow_distance.data[lin] != 0.0 {
            continue; // not a channel cell
        }
        if ff.filled.data[lin] > h8.data[lin] + 1e-6 {
            continue; // ponded reach
        }
        let dir = sk.flow_dir_rad.data[lin];
        if !dir.is_finite() {
            continue; // outlet
        }
        total += 1;
        let (y, x) = (lin / nx, lin % nx);
        let p = Vec2::new(x as f64 * spec8.cell_size, y as f64 * spec8.cell_size);
        // The receiver sits one grid step away: 8 m cardinal, 8√2 m diagonal.
        // Sampling at the wrong radius lands mid-saddle and reads false
        // uphill on bilinear interpolation.
        let quarter = std::f64::consts::FRAC_PI_2;
        let is_cardinal = (dir / quarter - (dir / quarter).round()).abs() < 1e-9;
        let step = spec8.cell_size * if is_cardinal { 1.0 } else { std::f64::consts::SQRT_2 };
        let q = Vec2::new(p.x + libm::cos(dir) * step, p.y + libm::sin(dir) * step);
        if sk.height.bilinear(q) > sk.height.bilinear(p) + 1e-9 {
            bad += 1;
        }
    }
    assert!(total > 50, "too few channel cells to check ({total})");
    assert_eq!(bad, 0, "{bad}/{total} channel cells flow uphill");
}

#[test]
fn divides_do_not_cross_channels() {
    let (_, sk) = build(41, Some(BiomeId::Piedmont));
    let spec8 = sk.flow_distance.spec;
    let nx = spec8.nx as usize;
    let mut on_channel = 0;
    let mut total = 0;
    for line in &sk.divides {
        for p in line {
            total += 1;
            let (gx, gy) = (
                (p.x / spec8.cell_size) as usize,
                (p.y / spec8.cell_size) as usize,
            );
            if sk.flow_distance.data[gy * nx + gx] == 0.0 {
                on_channel += 1;
            }
        }
    }
    assert!(total > 0, "no divides extracted");
    assert!(
        (on_channel as f64) < 0.02 * total as f64,
        "{on_channel}/{total} divide points sit on channel cells"
    );
}

#[test]
fn no_biome_branching_in_the_crate() {
    // Acceptance: "no `if biome` anywhere in the crate — grep it."
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/src");
    fn scan(dir: &std::path::Path, hits: &mut Vec<String>) {
        for e in std::fs::read_dir(dir).unwrap().flatten() {
            let p = e.path();
            if p.is_dir() {
                scan(&p, hits);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                if text.contains("BiomeId") {
                    hits.push(p.display().to_string());
                }
            }
        }
    }
    let mut hits = Vec::new();
    scan(std::path::Path::new(src), &mut hits);
    assert!(hits.is_empty(), "crate sources reference BiomeId: {hits:?}");
}

#[test]
fn discontinuity_truncates_tributaries_at_scarps() {
    // Find a hill-country seed carrying a two-province scarp (piedmont's
    // envelope never draws scarps), then check no non-trunk channel crosses
    // the scarp curve's neighborhood.
    let mut found = false;
    for seed in 0..400u64 {
        let id = RunIdentity::from_seed(seed);
        let spec = SiteSpec::generate_builtin(
            id,
            &SpecOverridesV2 { forced_biome: Some(BiomeId::HillCountry) },
        );
        let has_scarp = spec.structure_class.boundary_kind
            == Some(course_contracts::biome::BoundaryKind::Scarp);
        if !has_scarp {
            continue;
        }
        let c1 = course_primitives::generate(&spec, &id);
        let sk = course_skeleton::generate(&spec, &c1, &id);
        if sk.channels.len() < 3 {
            continue;
        }
        found = true;
        let curve = &c1
            .meta
            .discontinuities
            .first()
            .expect("two provinces carry a discontinuity")
            .curve;
        // Side test against the ACTUAL bowed curve (nearest-segment cross
        // product), and only count sign flips that happen ≥ 50 m from the
        // curve — fingers legitimately run ALONG the scarp band (subsequent
        // drainage) and oscillate around it without crossing.
        let side = |p: Vec2| -> (f64, f64) {
            let mut best_d2 = f64::INFINITY;
            let mut s = 0.0;
            for w in curve.windows(2) {
                let ab = Vec2::new(w[1].x - w[0].x, w[1].y - w[0].y);
                let ap = Vec2::new(p.x - w[0].x, p.y - w[0].y);
                let len2 = (ab.x * ab.x + ab.y * ab.y).max(1e-12);
                let t = ((ap.x * ab.x + ap.y * ab.y) / len2).clamp(0.0, 1.0);
                let q = Vec2::new(w[0].x + ab.x * t, w[0].y + ab.y * t);
                let d2 = (p.x - q.x).powi(2) + (p.y - q.y).powi(2);
                if d2 < best_d2 {
                    best_d2 = d2;
                    s = ab.x * ap.y - ab.y * ap.x;
                }
            }
            (s, best_d2.sqrt())
        };
        let mut crossings = 0;
        for c in sk.channels.iter().filter(|c| c.parent.is_some()) {
            let (mut last_s, _) = side(c.pts[0]);
            let mut last_far = true;
            for &p in &c.pts[1..] {
                let (s, d) = side(p);
                let far = d > 50.0;
                if s * last_s < 0.0 && far && last_far {
                    crossings += 1;
                }
                if d > 20.0 {
                    last_s = s;
                    last_far = far;
                }
            }
        }
        assert_eq!(
            crossings, 0,
            "seed {seed}: {crossings} tributary segments cross the scarp"
        );
        break;
    }
    assert!(found, "no piedmont scarp seed with a network found in 0..400");
}

#[test]
fn golden_skeleton_seed_1() {
    // Goldens for the primary + adversarial biomes at seed 1. Re-bless:
    //   UPDATE_GOLDEN=1 cargo test -p course-skeleton golden -- --nocapture
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden_skeleton_seed_1.txt");
    let mut got = String::new();
    for biome in [BiomeId::Piedmont, BiomeId::Heathland] {
        let (_, sk) = build(1, Some(biome));
        let d = digest(&sk);
        got.push_str(&format!(
            "{:?} {:016x} {:016x} {:016x} {:016x} {:016x} channels={}\n",
            biome, d[0], d[1], d[2], d[3], d[4], d[5]
        ));
    }
    if std::env::var("UPDATE_GOLDEN").is_ok() {
        std::fs::write(path, &got).unwrap();
        println!("{got}");
        return;
    }
    let want = std::fs::read_to_string(path).expect("golden file (UPDATE_GOLDEN=1 to bless)");
    assert_eq!(got, want, "skeleton goldens diverged — re-bless if intended");
}

#[test]
fn authored_network_spacing_hits_the_shared_invariant() {
    // The stage's core calibration: dist-to-authored-channel p50 must land
    // in the shared-invariant band on integrated biomes AND not separate by
    // biome (real archetypes measure 104-120 m regardless of identity).
    // Battery-style accumulation extraction is re-checked post-texture at
    // G-TERRAIN; pre-texture, the authored network is the conditioning
    // quantity S3 consumes.
    let mut biome_medians = Vec::new();
    for biome in [
        BiomeId::Piedmont,
        BiomeId::GreatPlains,
        BiomeId::HillCountry,
        BiomeId::RiverValley,
    ] {
        let mut per_seed = Vec::new();
        for seed in [7u64, 41, 101] {
            let (_, sk) = build(seed, Some(biome));
            let mut d: Vec<f64> = sk.flow_distance.data.clone();
            d.sort_by(|a, b| a.total_cmp(b));
            let p50 = d[d.len() / 2];
            // Per-seed SANITY bound only — real per-tile p50s run 88–140
            // with structured outliers beyond; the gate criterion is the
            // biome MEDIAN (the D5 battery asserts it over 150 seeds).
            assert!(
                (75.0..=240.0).contains(&p50),
                "{biome:?} seed {seed}: d2c_p50 {p50:.0} m beyond sanity"
            );
            per_seed.push(p50);
        }
        per_seed.sort_by(|a, b| a.total_cmp(b));
        biome_medians.push(per_seed[per_seed.len() / 2]);
    }
    // The invariant is SHARED: biome medians must not separate (real
    // archetype medians span 102-122 m — a 20 m spread).
    let lo = biome_medians.iter().cloned().fold(f64::INFINITY, f64::min);
    let hi = biome_medians.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        hi - lo < 30.0,
        "biome-median separation on a shared invariant: {lo:.0}-{hi:.0} m"
    );
}

#[test]
fn no_channel_crossings() {
    // The review found real crossings THREE times while ad-hoc counters
    // reported zero (junction-zone exclusions and area epsilons kept
    // hiding exactly the failing pattern). This is the honest test:
    // proper segment intersection with 1 m clearances, only the anchor
    // contact (both starts within 20 m of a parent-child junction)
    // exempt.
    let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    for seed in [6u64, 36, 44, 739, 765, 788, 1006, 1008] {
        let (_, sk) = build(seed, None);
        let mut bad = 0;
        for i in 0..sk.channels.len() {
            for j in (i + 1)..sk.channels.len() {
                let (ci, cj) = (&sk.channels[i], &sk.channels[j]);
                let junction: Option<Vec2> = if cj.parent == Some(i as u32) {
                    Some(cj.pts[0])
                } else if ci.parent == Some(j as u32) {
                    Some(ci.pts[0])
                } else {
                    None
                };
                for wi in ci.pts.windows(2) {
                    for wj in cj.pts.windows(2) {
                        if let Some(jp) = junction {
                            let di = ((wi[0].x - jp.x).powi(2) + (wi[0].y - jp.y).powi(2)).sqrt();
                            let dj = ((wj[0].x - jp.x).powi(2) + (wj[0].y - jp.y).powi(2)).sqrt();
                            if di < 20.0 && dj < 20.0 {
                                continue;
                            }
                        }
                        let ln = |p: Vec2, q: Vec2| {
                            ((q.x - p.x).powi(2) + (q.y - p.y).powi(2)).sqrt().max(1e-9)
                        };
                        let (lab, lcd) = (ln(wi[0], wi[1]), ln(wj[0], wj[1]));
                        let (d1, d2) = (o(wi[0], wi[1], wj[0]) / lab, o(wi[0], wi[1], wj[1]) / lab);
                        let (d3, d4) = (o(wj[0], wj[1], wi[0]) / lcd, o(wj[0], wj[1], wi[1]) / lcd);
                        if d1.abs() > 1.0
                            && d2.abs() > 1.0
                            && d3.abs() > 1.0
                            && d4.abs() > 1.0
                            && d1 * d2 < 0.0
                            && d3 * d4 < 0.0
                        {
                            bad += 1;
                        }
                    }
                }
            }
        }
        assert_eq!(bad, 0, "seed {seed}: {bad} channel crossings");
    }
}
