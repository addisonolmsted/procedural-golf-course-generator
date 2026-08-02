//! SPIKE — does a valley envelope over a space-filling network reproduce
//! real macro character, or are the swept primitives the wrong tool?
//!
//! The question this answers: generated terrain reads as "a tilted plane
//! with a few objects on it", and the placed ridges look random because
//! they ARE random (the planner draws four candidate centres and keeps the
//! one farthest from drains). Before rebuilding the planner around a grown
//! network, this checks the underlying premise on a single hand-built
//! config, measured by the same extractor that measures the real exemplars.
//!
//! The premise, measured on real tiles first: fitting
//! `z = z(nearest channel) + g * dist_to_channel` explains R^2 0.87 moraine
//! / 0.78 piedmont / 0.65 sandhills of a real tile, against 0.33 / 0.56 /
//! 0.04 for that tile's own best-fit plane. So real fluvial terrain IS a
//! hillslope field hung on a channel network — and an `smin` envelope over
//! valley surfaces computes exactly that field, analytically. If that is
//! right, nothing is wrong with the primitives; what is wrong is using two
//! to four of them instead of forty-five, and placing ridges as objects
//! instead of letting the interfluves fall out as the network's complement.
//!
//! Everything here is deliberately hand-wired rather than routed through
//! `planner::plan`: no RNG, no plan, no goldens to move. It writes a
//! campaign-shaped tile so `macro_campaign` can measure it with no special
//! casing.
//!
//! Usage: `cargo run -p course-macro --example spike_network --release`

use std::path::PathBuf;

use course_macro::network::{self, GrowthSpec};
use course_macro::{resolve, MacroConfig, Path as MPath, Tilt, Valley};
use course_world::grid::Grid;
use course_world::gridio;
use course_world::math::Vec2;
use course_world::profile::Profile;
use course_world::world::{world_spec, EXTENT_M};

// Fitted piedmont values, campaign-m4 prior (medians). Hard-coded rather
// than sampled so the spike is one fixed geometry, not a distribution.
const JUNCTION_ANGLE_DEG: f64 = 61.15;
const BRANCH_LEN_M: f64 = 334.6;
const SLOPE_AREA_THETA: f64 = 0.3454;
const FALL_AT_A0: f64 = 0.02506;
const CHAN_HW_AT_A0_M: f64 = 6.507;
/// Quarantined in the prior (its fitted sign is wrong on sandhills), so the
/// literature value is used here — see `fit_knobs.QUARANTINED`.
const CHAN_HW_AREA_EXP: f64 = 0.40;
const WALL_GRADE: f64 = 0.1504;
const TILT_GRADE: f64 = 0.014405;

/// Drainage density target, km/km^2. The real corpus is 2.21-2.45 at the
/// nominal threshold; 9 km^2 of box therefore wants ~20 km of channel.
const DENSITY_KM_PER_KM2: f64 = 2.25;
/// Fine-threshold drainage density (real piedmont `drainage_density_0p25x`
/// = 4.30). Growing to THIS instead of the nominal 2.25 is what puts
/// low-order channels into the interfluves. argv[6] scales it.
fn density() -> f64 {
    std::env::args().nth(6).and_then(|s| s.parse().ok()).unwrap_or(DENSITY_KM_PER_KM2)
}
/// Minimum head distance, argv[7].
fn min_head() -> f64 {
    std::env::args().nth(7).and_then(|s| s.parse().ok()).unwrap_or(140.0)
}

const BASE_ELEV: f64 = 100.0;
const RES_M: f64 = 2.0;

/// Smooth-min blend band at the valley shoulder, metres of elevation.
///
/// This is what rounds a DIVIDE. Two neighbouring valley surfaces meet where
/// their walls intersect, and `smin` blends them over this band — so a small
/// k leaves a knife-edge crease and a large one leaves a broad convex crest.
/// Override with argv[1] to sweep it.
fn crest_k() -> f64 {
    std::env::args().nth(1).and_then(|s| s.parse().ok()).unwrap_or(0.0)
}
/// Hillslope convexity, argv[2].
fn convexity() -> f64 {
    std::env::args().nth(2).and_then(|s| s.parse().ok()).unwrap_or(0.0)
}
/// Channel sinuosity, argv[3] (fitted piedmont median 1.262).
fn sinuosity() -> f64 {
    std::env::args().nth(3).and_then(|s| s.parse().ok()).unwrap_or(1.0)
}
/// Wall gradient AT THE CHANNEL, argv[4]. The fitted `valley_wall_grade`
/// (0.150) is a whole-hillslope average; with convexity the near-channel
/// gradient must exceed it for the mean to come out right.
fn wall_grade() -> f64 {
    std::env::args().nth(4).and_then(|s| s.parse().ok()).unwrap_or(WALL_GRADE)
}

fn main() -> std::io::Result<()> {
    // Regional tilt, running down +x for simplicity (the spike is about
    // structure, not azimuth).
    // Regional grade DERIVED from the slope-area law at the box's own
    // drainage area, not read from `tilt_grade`. A landscape's overall grade
    // is set by its master stream; reading the two independently let them
    // disagree 1.6x (0.0144 vs 0.0092), so a constant hillslope lift could
    // not hold at both ends of the tile and the plane surfaced downstream as
    // an undissected arc. argv[5] = 1 restores the fitted `tilt_grade`.
    let regional = if std::env::args().nth(5).as_deref() == Some("1") {
        TILT_GRADE
    } else {
        network::slope_at(
            (EXTENT_M / 1000.0) * (EXTENT_M / 1000.0),
            FALL_AT_A0,
            SLOPE_AREA_THETA,
        )
    };
    println!("regional grade {regional:.5} (fitted tilt_grade {TILT_GRADE:.5})");
    let tilt = Tilt {
        grade_x: -regional,
        grade_y: 0.0,
        curve_m: 0.0,
        core_tread: 1.0,
    };
    let plane_at = |p: Vec2| BASE_ELEV + tilt.grade_x * (p.x - 0.5 * EXTENT_M);

    // Trunk: a chord across the box, slightly off-centre, with a gentle bow
    // so the network is not built on a perfectly straight spine.
    let trunk = vec![
        Vec2::new(-120.0, 1250.0),
        Vec2::new(900.0, 1400.0),
        Vec2::new(1800.0, 1560.0),
        Vec2::new(3120.0, 1700.0),
    ];

    let box_km2 = (EXTENT_M / 1000.0) * (EXTENT_M / 1000.0);
    let reaches = network::grow(&GrowthSpec {
        trunk,
        target_len_m: density() * box_km2 * 1000.0,
        junction_angle_deg: JUNCTION_ANGLE_DEG,
        branch_len_m: BRANCH_LEN_M,
        margin_m: 60.0,
        min_head_dist_m: Some(min_head()),
    });
    let total_len: f64 = reaches.iter().map(|r| r.len_m).sum();
    println!(
        "network: {} reaches, {:.1} km of channel, density {:.2} km/km^2, trunk order {}",
        reaches.len(),
        total_len / 1000.0,
        total_len / 1000.0 / box_km2,
        reaches[0].order
    );

    // THE HILLSLOPE LIFT — the whole point of the spike.
    //
    // A valley surface rises at `wall_grade` away from its floor and the
    // primitives compose by smooth-min, so the composed surface is the LOWER
    // ENVELOPE of every valley: a distance-to-channel hillslope field with
    // rounded divides, which is the shape the R^2 above says real terrain
    // has. It produces nothing today only because the regional plane sits a
    // few metres above the floors, so `smin` keeps the PLANE everywhere
    // except in the few valley corridors — a plane with grooves in it.
    //
    // Lifting the plane by `wall_grade * (farthest any point is from the
    // network)` puts it above the envelope everywhere, so the envelope wins
    // and the interfluves emerge where the walls of neighbouring valleys
    // intersect. Nothing places them.
    let mut dists: Vec<f64> = Vec::new();
    let probe = 120usize;
    for iy in 0..probe {
        for ix in 0..probe {
            let p = Vec2::new(
                (ix as f64 + 0.5) * EXTENT_M / probe as f64,
                (iy as f64 + 0.5) * EXTENT_M / probe as f64,
            );
            let mut d = f64::INFINITY;
            for r in &reaches {
                for w in r.pts.windows(2) {
                    d = d.min(seg_dist(p, w[0], w[1]));
                }
            }
            dists.push(d);
        }
    }
    dists.sort_by(|a, b| a.partial_cmp(b).unwrap());
    // A TYPICAL divide distance, not the worst one.
    //
    // Keying the lift off the maximum made one badly-served corner set the
    // height of the whole regional plane: at wall 0.25 the single farthest
    // point (402 m) lifted the plane 100 m, and every sparsely-served region
    // became an enormous smooth dome with no dissection in it. The p90 is
    // the divide height the tile actually has, and the few points beyond it
    // simply have the plane show through — which is what a broad upland IS,
    // rather than a defect.
    let d_typ = dists[dists.len() * 9 / 10];
    let d_max = *dists.last().unwrap();
    let lift = wall_grade() * d_typ;
    println!("hillslope lift: {lift:.1} m (p90 divide {d_typ:.0} m, worst {d_max:.0} m)");

    // Lower the reaches. Trunk floor is anchored `lift` below the plane at
    // its entry; every tributary floor is accordance-snapped at resolve time.
    let mut valleys: Vec<Valley> = Vec::with_capacity(reaches.len());
    for (i, r) in reaches.iter().enumerate() {
        let fall = network::slope_at(r.area_km2, FALL_AT_A0, SLOPE_AREA_THETA);
        let hw = network::halfwidth_at(r.area_km2, CHAN_HW_AT_A0_M, CHAN_HW_AREA_EXP);
        valleys.push(Valley {
            path: MPath::Points(network::wander(&r.pts, sinuosity(), i)),
            floor_z0_m: if i == 0 { plane_at(r.pts[0]) - lift } else { 0.0 },
            fall_gradient: fall,
            floor_halfwidth: Profile::constant(hw),
            wall_grad_left: wall_grade(),
            wall_grad_right: wall_grade(),
            floor_round_m: (0.35 * hw).clamp(2.0, 12.0),
            // Crisp at the channel, broad at the divide. One constant band
            // cannot do both: small keeps the channel and leaves a creased
            // interfluve; large rounds the interfluve and dissolves the
            // channel. `crest_rise_m` is the hillslope relief — wall grade
            // times the distance from a channel to its divide.
            shoulder_k_m: 3.0,
            crest_k_m: if crest_k() > 0.0 { Some(crest_k()) } else { None },
            crest_rise_m: wall_grade() * 135.0,
            wall_convexity: convexity(),
            join_trunk: r.parent,
            fall_profile: None,
        });
    }

    let cfg = MacroConfig {
        schema_version: course_macro::SCHEMA_VERSION,
        extent_m: EXTENT_M,
        base_elev_m: BASE_ELEV + lift,
        tilt,
        core_relax: 1.0,
        core_relax_valleys: false,
        ridges: vec![],  // NONE — interfluves must emerge, not be placed
        bluffs: vec![],
        bowls: vec![],
        valleys,
    };

    let t0 = std::time::Instant::now();
    let r = resolve(&cfg);
    let gs = world_spec(RES_M);
    let mut g = Grid::filled(gs, 0.0f64);
    for y in 0..gs.ny {
        for x in 0..gs.nx {
            let z = r.height_at(gs.world_of(x, y));
            g.set(x, y, z);
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    let (lo, hi) = g
        .data
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
    println!("raster {}x{} at {RES_M} m in {secs:.1} s; relief {:.1} m", gs.nx, gs.ny, hi - lo);

    let out = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tools/macro_campaign/out/generated_spike/piedmont");
    let tag = format!("d{:.0}h{:.0}", density() * 100.0, min_head());
    std::fs::create_dir_all(&out)?;
    let path = out.join(format!("seed1_{tag}.cgrid"));
    gridio::write_grid_f32(&path, &g)?;
    println!("wrote {}", path.display());
    println!("\nnow: python3 -m macro_campaign spike   (or measure_tile directly)");
    Ok(())
}

/// Point-to-segment distance.
fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let l2 = ab.dot(ab);
    let t = if l2 <= 1e-12 { 0.0 } else { ((p - a).dot(ab) / l2).clamp(0.0, 1.0) };
    p.distance(a + ab * t)
}
