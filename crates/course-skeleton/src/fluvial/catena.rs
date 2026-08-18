//! The catena base surface: elevation as a function of flow distance and
//! the nearest channel's base elevation, hung off the C1 implied surface.
//!
//! Deliberately simple (stage-02: "the catena's job is a correct mean
//! surface, not a characterful one"). The construction:
//!
//!   z(x) = implied(x) − (implied_ch(x) − z_ch(x)) · (1 − w(d))
//!   w(d) = (d / (d + D_HALF))^θ
//!
//! where `implied_ch`/`z_ch` ride the Dijkstra wavefront from the nearest
//! channel cell. At a channel cell w=0 and z = z_ch exactly (the base
//! profile, monotone along the network by construction); at the divide
//! w→1 and z returns to C1's implied surface. θ is the profile-law
//! exponent — a calibrated fit target. Candle-wax smoothness between
//! channel and divide is EXPECTED here; texture is S3's job.

use course_world::grid::{Grid, GridSpec};

use super::flow_distance::Nearest;

/// Profile-law curvature exponent (calibration target; provisional).
pub const THETA: f64 = 0.62;
/// Distance over which the hillslope recovers the full incision — of the
/// order of the corpus's d2c p90 (230–270 m). Incision spans the whole
/// channel→divide hillslope; the earlier w=(d/(d+D))^θ (θ<1) profile had
/// divergent slope at d→0 — vertical walls at every channel — and the
/// review read the result as slot trenches in an undrained plateau. The
/// current form has finite channel-wall slope ((1+θ)·inc/D_FULL) and zero
/// slope at the divide (rounded crests).
pub const D_FULL_M: f64 = 260.0;
/// Hillslope reach for a threshold-area draw — the SHORT end of the
/// discharge-keyed range in `banks`. Ladder-tunable via the
/// `skeleton.valley_reach_m` dial, which overrides this floor.
pub const D_FULL_FLOOR_M: f64 = 70.0;
/// Drained area (km²) at which the reach reaches `D_FULL_M`.
pub const D_FULL_FULL_KM2: f64 = 2.0;
/// Along-channel variation of the valley-floor half-width, as a fraction.
pub const FLOOR_JITTER: f64 = 0.35;
/// Radius (m) over which the floor-to-groove corner is rounded.
pub const FLOOR_KNEE_M: f64 = 7.0;

/// Position-hashed field, box-blurred to a correlation length and
/// normalized to unit RMS — the same construction the carve's routing
/// dither uses, and deterministic for the same reason (no RNG, no draw
/// transcript, identical for a given grid).
fn jitter_field(spec: &GridSpec, salt: u64, passes: usize) -> Vec<f64> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut d: Vec<f64> = (0..nx * ny)
        .map(|i| {
            let (y, x) = ((i / nx) as u64, (i % nx) as u64);
            let mut h = x.wrapping_mul(salt) ^ y.rotate_left(32).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            h ^= h >> 30;
            h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
            h ^= h >> 31;
            (h >> 11) as f64 / (1u64 << 53) as f64 - 0.5
        })
        .collect();
    for _ in 0..passes {
        d = box3(&d, nx, ny);
    }
    let rms = (d.iter().map(|v| v * v).sum::<f64>() / (nx * ny) as f64).sqrt().max(1e-12);
    d.iter().map(|v| v / rms).collect()
}

/// Channel base profile: elevation gain above base level as a function of
/// network arc from the outlet. Concave (slope decays downstream) — the
/// slope–area relation's integral form, kept parametric.
pub fn channel_profile(outlet_arc_m: f64, max_arc_m: f64, incision_m: f64) -> f64 {
    if max_arc_m <= 0.0 {
        return 0.0;
    }
    let s = (outlet_arc_m / max_arc_m).clamp(0.0, 1.0);
    incision_m * libm::pow(s, 1.35)
}

/// Assemble the 8 m base surface from the implied surface + the transform.
/// `incision_scale` ∈ (0,1] shrinks valley depth (negative integration).
pub fn assemble(
    spec: &GridSpec,
    implied: &Grid<f64>,
    near: &[Nearest],
    incision_scale: f64,
) -> Grid<f64> {
    debug_assert_eq!(implied.data.len(), near.len());
    // The incision field is discontinuous across Dijkstra wavefront
    // boundaries (adjacent cells can carry different source channels with
    // different base elevations) and those Voronoi seams print through the
    // hillshade. Blur the INCISION — never the surface — with a 24 m box
    // footprint: well under the 64 m band boundary, so no structure is
    // touched, only the seam steps.
    let mut incision: Vec<f64> = near
        .iter()
        .zip(&implied.data)
        .map(|(n, _)| {
            if !n.dist_m.is_finite() {
                return 0.0;
            }
            // Composite concave hillslope with an order-scaled FLAT FLOOR:
            // without the floor the incision profile rises from the very
            // centreline and the channel renders as a one-cell knife cut
            // (the review's "sharp indentations"). Real valley floors have
            // width; the widths here are provisional placeholders until the
            // corpus transect fit (E7) supplies them per order.
            let floor_hw = match n.order {
                0 | 1 => 8.0,
                2 => 14.0,
                3 => 22.0,
                _ => 30.0,
            };
            let d_eff = (n.dist_m - floor_hw).max(0.0);
            // groove: 30% of the incision within the first 40 m of bank
            let ug = (d_eff / 40.0).min(1.0);
            let groove = 1.0 - (1.0 - ug) * (1.0 - ug);
            let uh = (d_eff / D_FULL_M).min(1.0);
            let hillslope = 1.0 - libm::pow(1.0 - uh, 1.0 + THETA);
            let w = 0.3 * groove + 0.7 * hillslope;
            (n.implied_channel - n.z_channel) * incision_scale * (1.0 - w)
        })
        .collect();
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    // 4 passes ≈ 72 m footprint: rounds the knife-edge crease where two
    // valleys' incision fields meet at a divide (the review's "ridges much
    // too thin"). Removing sub-64 m creases is band-legal; adding sub-64 m
    // detail would not be.
    for _ in 0..4 {
        incision = box3(&incision, nx, ny);
    }
    let mut out = implied.clone();
    for (i, inc) in incision.iter().enumerate() {
        out.data[i] = implied.data[i] - inc;
    }
    out
}

/// One 3×3 box-blur pass (edge-clamped).
fn box3(a: &[f64], nx: usize, ny: usize) -> Vec<f64> {
    let mut out = vec![0.0; a.len()];
    for y in 0..ny {
        for x in 0..nx {
            let mut sum = 0.0;
            let mut n = 0.0;
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                    if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                        continue;
                    }
                    sum += a[yy as usize * nx + xx as usize];
                    n += 1.0;
                }
            }
            out[y * nx + x] = sum / n;
        }
    }
    out
}

/// Bank profile for an ALREADY-CARVED surface.
///
/// The derived engine erodes the valleys itself, so the catena's job
/// shrank from "cut valleys into the implied surface" to "give the cut
/// ones a bank": an order-scaled flat floor and a groove/hillslope blend
/// over the first `D_FULL_M`. It only ever LOWERS ground toward the
/// nearest channel's elevation, never raises it, so the carve's drainage
/// monotonicity survives untouched.
pub fn banks(
    spec: &GridSpec,
    carved: &Grid<f64>,
    near: &[Nearest],
    incision_boost: f64,
    d_full_floor_m: f64,
) -> Grid<f64> {
    debug_assert_eq!(carved.data.len(), near.len());
    // Along-channel variation of the floor width. The bank break used to
    // sit at EXACTLY `floor_hw` on every reach in the tile — a curvature
    // spike measured 749x the surrounding level at a fixed 8 m offset,
    // against 1.15x (i.e. no spike at all) on corpus tiles. That is the
    // "sharp and uniform edges" tell: real banks break wherever the ground
    // happens to break, so no single offset stands out. A correlated field
    // (~150 m) keeps the section coherent across the channel while the
    // break wanders along it.
    let jitter = jitter_field(spec, 0x9E37_79B9_7F4A_7C15, 8);
    let mut cut: Vec<f64> = near
        .iter()
        .zip(&carved.data)
        .enumerate()
        .map(|(i, (n, z))| {
            if !n.dist_m.is_finite() {
                return 0.0;
            }
            // Floor width keys off DISCHARGE (the kernel doc always
            // called for this; order was the placeholder): a 20 km²
            // trunk gets a ~34 m half-width flat floor, a threshold
            // swale ~8 m. Major valleys also get a SHARPER groove so
            // the cut reads clean and decisive (reviewer gap #1) —
            // minors keep the soft profile and stay subtle.
            let km2 = (n.area_m2 / 1.0e6).max(0.0);
            // incision_boost (envelope, hc 1.6 / pied 1.35): ravines
            // and draws cut WIDER as well as deeper — the deeper cut
            // itself comes from the carve's boosted incision_scale
            let floor_hw = (9.5 * libm::pow(km2, 0.45) * incision_boost)
                .clamp(8.0, 34.0 * incision_boost)
                * (1.0 + FLOOR_JITTER * jitter[i]);
            let groove_d =
                (if km2 >= 2.0 { 26.0 } else { 40.0 }) * libm::sqrt(incision_boost);
            // SOFT knee. `max(0)` is a slope discontinuity — the profile is
            // flat inside the floor and rises immediately outside it, so
            // the second derivative is a delta function at the floor edge
            // and the hillshade draws a hard line there. This is the same
            // curve with the corner rounded over ~FLOOR_KNEE_M.
            let u = n.dist_m - floor_hw;
            let d_eff = 0.5 * (u + (u * u + FLOOR_KNEE_M * FLOOR_KNEE_M).sqrt()) - FLOOR_KNEE_M * 0.5;
            let ug = (d_eff / groove_d).min(1.0);
            let groove = 1.0 - (1.0 - ug) * (1.0 - ug);
            // Hillslope reach keys off DISCHARGE too. A fixed 260 m for
            // every channel is wider than the SPACING between them: the
            // corpus sits at d2c ~104-118 m, i.e. neighbours ~200-400 m
            // apart, so two fixed catenas overlap across the whole
            // interfluve and the pair reads as one broad swale with no
            // ridge between. Real terrain keeps the long reach for the
            // valley that earned it and gives a threshold draw a ~60-75 m
            // flank — which is what leaves a crest standing.
            let floor = d_full_floor_m.clamp(24.0, D_FULL_M);
            let d_full = (floor
                + (D_FULL_M - floor) * libm::sqrt((km2 / D_FULL_FULL_KM2).min(1.0)))
                .clamp(floor, D_FULL_M);
            let uh = (d_eff / d_full).min(1.0);
            let hillslope = 1.0 - libm::pow(1.0 - uh, 1.0 + THETA);
            let w = 0.3 * groove + 0.7 * hillslope;
            // how far this cell still stands above the channel it drains to
            ((z - n.z_channel).max(0.0)) * (1.0 - w)
        })
        .collect();
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    // Border rows carry the closed-border RIM (construction, up to
    // ~200 m above grade): their (z − z_channel) is monstrous, and the
    // seam blur below SMEARED it into the adjacent rows — a 5–30 m
    // moat ("ditch") along every rimmed edge, worst where channels
    // approach the border. Construction rows carry no catena cut.
    for y in 0..ny {
        for x in 0..nx {
            if x == 0 || y == 0 || x == nx - 1 || y == ny - 1 {
                cut[y * nx + x] = 0.0;
            }
        }
    }
    // same 4-pass smoothing the incision field used: removes the crease
    // where two valleys' fields meet, and stays under the 64 m band edge
    for _ in 0..4 {
        cut = box3(&cut, nx, ny);
    }
    let mut out = carved.clone();
    for (i, c) in cut.iter().enumerate() {
        out.data[i] = carved.data[i] - c;
    }
    out
}

/// Bank profile for the TIER-2 side-valley cells (dendritic morphology
/// tier), returned as a CUT to be combined with the tier's trench and
/// smoothed once — a tier-2 path is a raw D8 chain, so its axis carries
/// 45°/90° staircase corners, and blurring the assembled cut is what
/// rounds them into a valley. Area-scaled geometry: a threshold-area
/// tributary gets a ~4-10 m floor, ~20 m groove and a 70-110 m hillslope
/// tail. Only ever lowers ground toward the tier-2 cell's elevation.
pub fn banks_small_cut(spec: &GridSpec, surface: &Grid<f64>, near2: &[Nearest]) -> Vec<f64> {
    debug_assert_eq!(surface.data.len(), near2.len());
    let mut cut: Vec<f64> = near2
        .iter()
        .zip(&surface.data)
        .map(|(n, z)| {
            if !n.dist_m.is_finite() {
                return 0.0;
            }
            let km2 = (n.area_m2 / 1.0e6).max(0.0);
            // GULLY geometry, not valley geometry. The first cut used a
            // 70-110 m tail so the feature would survive S3's 64 m
            // lowpass, and the result was a 150 m bowl 5 m deep — a 3%
            // grade that no hillshade can show. Real gullies at this
            // discharge are 25-50 m across with 15-25% walls; keeping
            // them through S3 is the corridor restore's job, not the
            // width's.
            let floor_hw = (6.0 * libm::pow(km2, 0.45)).clamp(3.0, 7.0);
            let groove_d = 12.0;
            let d_full = 25.0 + 25.0 * (km2 / 0.12).clamp(0.0, 1.0);
            let d_eff = (n.dist_m - floor_hw).max(0.0);
            let ug = (d_eff / groove_d).min(1.0);
            let groove = 1.0 - (1.0 - ug) * (1.0 - ug);
            let uh = (d_eff / d_full).min(1.0);
            let hillslope = 1.0 - libm::pow(1.0 - uh, 1.0 + THETA);
            let w = 0.3 * groove + 0.7 * hillslope;
            ((z - n.z_channel).max(0.0)) * (1.0 - w)
        })
        .collect();
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    // construction rows carry no catena cut (the rim-moat lesson)
    for y in 0..ny {
        for x in 0..nx {
            if x == 0 || y == 0 || x == nx - 1 || y == ny - 1 {
                cut[y * nx + x] = 0.0;
            }
        }
    }
    cut
}

/// Smooth an assembled tier-2 cut field. Three box3 passes (~48 m) is
/// what turns the staircase into a valley without erasing it; the caller
/// subtracts the result.
pub fn smooth_cut(spec: &GridSpec, mut cut: Vec<f64>, passes: usize) -> Vec<f64> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    for _ in 0..passes {
        cut = box3(&cut, nx, ny);
    }
    cut
}
