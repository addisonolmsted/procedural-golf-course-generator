//! The derived channel network: erode, then extract.
//!
//! S2 used to AUTHOR channel polylines — grow a trunk, sprout tributaries,
//! then defend the result with crossing guards, separation neighbourhoods,
//! mouth-angle enforcement, curl penalties and a final trim sweep. Ten
//! structural patches later the review still found loops and long parallel
//! pairs (seed 48). The paradigm was the bug: geometry authored in path
//! space has to be *argued* into consistency with the terrain it sits on.
//!
//! Here the terrain decides. A light stream-power erosion runs on C1's
//! implied surface (plus a sub-macro roughness seed that gives drainage
//! something to compete over), and the network is EXTRACTED from the
//! resulting flow field. What the review asked for becomes structural:
//!
//! - **No loops.** A D8 receiver graph is a forest; a loop is unreachable.
//! - **No crossings.** Two flow paths that meet share every cell after the
//!   meeting point — they merge, they cannot cross.
//! - **Sensible junctions.** Confluences sit where terrain converges, and
//!   the merge angle is the angle at which the two valleys arrive.
//! - **Parallel only where earned.** Neighbouring channels stay separate
//!   exactly when a divide separates them.
//!
//! Density stops being a growth budget and becomes one extraction
//! threshold: rills below it stay in the surface as texture for S3.
//!
//! Everything is fixed-count and position-seeded: N erosion iterations, a
//! fixed wave transcript, no data-dependent loops (stage-02's budget rule).

use course_contracts::metadata::Edge;
use course_seed::DetRng;
use course_world::flow;
use course_world::grid::{Grid, GridSpec};
use course_world::math::Vec2;
use course_world::world::EXTENT_M;

use crate::kernel::Channel;

/// Sub-macro roughness waves: the convergence seed. C1's field is
/// band-limited to >= 400 m, and flow over a surface that smooth runs in
/// near-parallel sheets — the roughness is what makes valleys compete and
/// capture. Wavelengths sit in S2's own band (>= 64 m), so this is not S3
/// texture leaking upstream; it is the spur-and-hollow degree of freedom
/// the pipeline revision identified as missing.
pub const N_WAVES: usize = 32;
pub const WAVE_BAND_M: (f64, f64) = (140.0, 520.0);
/// Draws consumed: 4 per wave + 1 threshold jitter.
pub const DRAWS: usize = N_WAVES * 4 + 1;

/// Default erosion iterations (fixed — never "until converged").
pub const ITERS: usize = 15;
/// Per-iteration vertical clamp (m): keeps a single step from cutting a
/// gorge where accumulation is huge.
pub const STEP_CLAMP_M: f64 = 0.45;
/// Minimum roughness amplitude (m) regardless of relief — the flat-biome
/// symmetry breaker (see `carve`).
pub const ROUGH_FLOOR_M: f64 = 0.55;
/// Stability ceiling for the explicit creep step (5-point Laplacian).
pub const CREEP_ALPHA_MAX: f64 = 0.25;
/// Correlation length of the routing-wander field, as box3 passes
/// (~120 m at the 8 m grid).
pub const WANDER_BLUR_PASSES: usize = 9;
/// Length scale that turns the unit wander field into metres of relief
/// per unit slope: deflection angle ≈ atan(wander · 2π · WANDER_LEN_M / λ).
pub const WANDER_LEN_M: f64 = 24.0;
/// Minimum share of `cut_spread_m` applied even in full channels.
pub const SPREAD_CHANNEL_FLOOR: f64 = 0.30;
/// Wander amplitude floor (m of relief per unit dial) on ground too flat
/// for the slope-proportional term to reach. Ladder-picked: 0.5 bends the
/// low-gradient reaches without letting the field out-vote the grade.
pub const WANDER_FLOOR_M: f64 = 0.5;
/// Macro slope (300 m lowpass) a tier-2 head needs. Dendritic gullies
/// dissect FLANKS; on a footslope plain the tier has no relief above its
/// path to cut and only draws lines.
pub const TIER2_MIN_SLOPE: f64 = 0.035;

/// The carve's own dials, derived from the biome dials by the caller.
pub struct CarveParams {
    /// Roughness amplitude as a fraction of the C1 relief amplitude.
    pub roughness_frac: f64,
    /// Stream-power coefficient (metres per unit sqrt(area)*slope).
    pub k: f64,
    /// Channel-extraction threshold in drained AREA (m²).
    pub area_threshold_m2: f64,
    /// Scales total incision (negative integration shrinks it).
    pub incision_scale: f64,
    /// Base-level drawdown (m) applied as a ramp into the base edge —
    /// the tile drains to something, and that something is lower.
    pub base_drop_m: f64,
    /// Upstream drainage area (m²) entering at the trunk inlet — the
    /// catchment that lies OUTSIDE the tile. Zero disables the inlet.
    pub inflow_area_m2: f64,
    /// Rim the three non-base borders so all outflow leaves via the base
    /// edge (S2's contract). See `carve`.
    pub close_borders: bool,
    /// Keep NATURAL closed depressions unfilled, in [0,1] — the fraction
    /// of the drawn integration dial's derangement. A deranged landscape
    /// does not route its water to base level; its basins swallow it.
    pub derangement: f64,
    /// Erosion iterations.
    pub iters: usize,
    /// Per-iteration vertical clamp (m).
    pub step_clamp_m: f64,
    /// Tier-2 side-valley extraction: multiplier (< 1.0) on the area
    /// cut for a second, PURELY MORPHOLOGICAL channel tier — carved and
    /// catena'd at area-scaled width but never entering `channel_of`,
    /// Strahler, the traced network, or the flow-distance metrics (the
    /// D5 invariant and every downstream consumer stay tier-1). 1.0
    /// disables the tier.
    pub tributary_reach: f64,
    /// Hillslope creep coefficient per erosion iteration (0 disables).
    /// See `hillslope_creep` — this is the diffusive half of the erosion
    /// law, and it only runs where the stream-power loop runs.
    pub creep: f64,
    /// Slope-proportional routing wander (0 disables): how far flow paths
    /// are allowed to stray from the fall line before D8 quantizes them.
    /// ~0.25 gives ≈15° of deflection at any steepness. ROUTING SURFACE
    /// ONLY — the terrain never carries this field.
    pub route_wander: f64,
    /// Lateral spread (m) of the HILLSLOPE share of each iteration's cut
    /// (0 disables). Channels keep a crisp cut; sub-threshold ground gets
    /// its incision smeared across the fall line, which is what turns a
    /// one-cell D8 rill into diffuse wash.
    pub cut_spread_m: f64,
    /// Fraction of the erosion run to withhold the external inflow for
    /// (0 = inject from iteration 1, the historic behaviour). See the
    /// loop: injecting early locks the trunk onto its straightest path.
    pub inflow_start_frac: f64,
    /// Discharge exponent m in the stream-power law `k·A^m·S`. The
    /// classic 0.5 erodes divides almost as fast as channels over a short
    /// run; higher values concentrate the cut where the water is and
    /// leave interfluves standing. Normalized at the extraction threshold
    /// so `k` keeps its calibrated meaning.
    pub area_exp: f64,
    /// TOTAL rock uplift (m) spread over the erosion run, applied to the
    /// interior against a fixed base edge (0 disables).
    ///
    /// Without it the carve can only ever REMOVE relief: it starts from
    /// S1's macro surface and erodes, so more erosion means a flatter
    /// tile, never a more dissected one. Measured directly — raising the
    /// stream-power boost 1.0→4.5 lowered piedmont's crest p90 5.98→4.64 m
    /// while its valleys stayed at 3.8 m, and featureless ground rose
    /// 30.5→34.8 %. Real landscapes are valley-dominated because uplift
    /// keeps feeding the interfluves while the drainage cuts down; this is
    /// that term, and it is what turns the carve from a shave into an
    /// incision.
    pub uplift_m: f64,
}

/// What the carve produces: the eroded surface plus the extracted network.
pub struct Carved {
    /// The carved base surface (8 m) — valleys are cut into it already.
    pub z: Grid<f64>,
    /// Per-cell channel membership: `channel_of[lin]` = Some(index).
    pub channel_of: Vec<Option<u32>>,
    /// Strahler order per channel cell (0 where not a channel).
    pub order_at: Vec<u8>,
    /// D8 receiver index per cell on the carved surface (-1 = outlet).
    pub rec: Vec<i64>,
    /// Drained area, m².
    pub area: Vec<f64>,
    /// The traced vector network (for QA instruments and the viewer).
    pub channels: Vec<Channel>,
    /// Where external inflow enters (usize::MAX if none) — diagnostic.
    pub inlet: usize,
    /// Pre-rim heights of the rimmed border rows. The rim is ROUTING
    /// CONSTRUCTION — every flow decision is made against the walls —
    /// but it must never reach S3: amplify smears the ~200 m single-row
    /// spike into a 40 m-wide 50-78 deg skirt that hillshades as a
    /// trench/berm band on every rimmed edge (user report, twice). The
    /// kernel strips the walls from the PRESENTED height only, after
    /// all routing/divide/connectivity derivations.
    pub pre_rim: Vec<(usize, f64)>,
    /// Tier-2 side-valley cells (dendritic morphology tier; empty set
    /// when `tributary_reach` >= 1.0). Never part of the channel set.
    pub tier2: Vec<bool>,
    /// The tier-2 extraction cut actually used (m²; 0 when disabled) —
    /// the kernel's depth taper needs the same number.
    pub tier2_cut_m2: f64,
    /// Tier-2 reaches as SMOOTHED world-space polylines (Chaikin ×2 on the
    /// receiver chain). The raw chain is a D8 staircase, and at gully
    /// width the staircase IS the feature; the kernel cuts along these
    /// instead of along the cells.
    pub tier2_paths: Vec<Vec<Vec2>>,
}

/// Amplitude and band of the flat-breaking perturbation (see `carve`).
pub const DEFLAT_AMP_M: f64 = 0.10;
/// Roughness amplitude as a fraction of the C1 relief amplitude.
pub const ROUGHNESS_FRAC: f64 = 0.05;
/// Stream-power coefficient (review-tuned; the fit target is valley depth
/// against the corpus transects, scheduled with E7).
pub const K_STREAM_POWER: f64 = 0.9;
/// Base-level drawdown (m) at the outlet edge.
pub const BASE_DROP_M: f64 = 4.0;

/// Slope above which the routing dither fades out entirely: ground this
/// steep resolves its own flow directions.
pub const DITHER_SLOPE_MAX: f64 = 0.02;

/// Routing-shoulder raise (m) at a rimmed border and the band it eases
/// over. Routing-surface only — steers collector channels inboard of the
/// frame without leaving a berm in the terrain (see `carve`).
/// Tuning note: this pair is a SWEET SPOT, not a lower bound. 8 m (and a
/// full-height plateau variant) both dammed interior lows into eps-flat
/// fill-lakes that the router wall-followed even harder (seed 12
/// re-hugged at 100%); the quadratic 5 m profile keeps a continuous
/// inward gradient with no flat to follow.
pub const SHOULDER_M: f64 = 5.0;
pub const SHOULDER_W_M: f64 = 260.0;

/// Length of the base-level drawdown ramp (m).
pub const BASE_RAMP_M: f64 = 900.0;
/// External catchment (m²) entering at the trunk inlet, per unit of the
/// biome's `trunk_river` dial. A river valley (dial 1.0) gets 20 km² —
/// two orders above the tile's own 9 km², which is what makes its trunk a
/// through-going river rather than one more tributary; at 2.5 km² (the
/// first attempt) the external river was merely comparable to the tile's
/// own catchments and failed to dominate on half the seeds. Biomes with
/// the dial at zero (heathland kettle country, sandhills) get no external
/// river at all, which is correct for them.
pub const INFLOW_PER_TRUNK_DIAL_M2: f64 = 2.0e7;

/// Default extraction threshold (m² of drained area). The corpus cut is
/// 6e4 (extract_v2's CHANNEL_AREA_M2), but that number is not directly
/// transferable: our tile is rimmed (all water leaves by one edge) and its
/// routing surface is perturbed to avoid grid artifacts, both of which
/// change how accumulation concentrates. The dial is therefore fitted to
/// the corpus's measured DENSITY band (2.3–2.7 km/km²) rather than copied,
/// and sits on the low side of it by review request — sub-threshold rills
/// stay in the surface as texture for S3's dictionary. Final calibration
/// happens against the D5 battery once the engine is integrated.
pub const AREA_THRESHOLD_M2: f64 = 1.2e5;
/// Slope-adaptive channel initiation, TILE-RELATIVE: the per-cell
/// threshold scales by (median slope / local slope), clamped. Real
/// channel heads initiate at smaller source areas on steeper ground
/// (the A·Sⁿ initiation literature); a pure area cut met the d2c
/// invariant only while every biome's macro was spectrally similar —
/// parallel-ridge terrain (hill-country trains) elongates catchments
/// and pushed d2c to ~150 m against the corpus 103–118 m, while its
/// steep flanks are exactly where real gullies come in early. Relative
/// to the tile median (not absolute slope) so flat biomes are
/// untouched and no biome is ever named.
pub const SLOPE_INIT_CLAMP: (f64, f64) = (0.14, 1.25);
/// Majors hierarchy: top-K systems by mouth discharge get extra
/// deepening sweeps (reviewer: "fewer channels with larger effects").
pub const MAJORS_CAP: usize = 2;
pub const MAJORS_THIRD_FRAC: f64 = 0.6;
pub const MAJORS_K_BOOST: f64 = 2.2;
pub const MAJORS_ITERS: usize = 6;
/// Response exponent on the STEEP side only (local > median): the
/// 150-seed battery showed the linear response left parallel-ridge
/// flanks 15 m short of the shared d2c invariant while the linear
/// gentle side kept flat biomes in band.
pub const SLOPE_INIT_STEEP_EXP: f64 = 1.8;
fn base_ramp_m() -> f64 { std::env::var("RAMP").ok().and_then(|v| v.parse().ok()).unwrap_or(BASE_RAMP_M) }
pub const DEFLAT_BAND_M: (f64, f64) = (48.0, 160.0);

/// Deterministic roughness field: a fixed-count wave sum, mean-zero.
fn roughness(spec: &GridSpec, rng: &mut DetRng, amp: f64) -> Vec<f64> {
    roughness_at(spec, rng, amp, WAVE_BAND_M, N_WAVES)
}

/// Wave-sum field with an explicit band and count.
fn roughness_at(
    spec: &GridSpec,
    rng: &mut DetRng,
    amp: f64,
    band: (f64, f64),
    count: usize,
) -> Vec<f64> {
    let (lo, hi) = band;
    let log_ratio = libm::log(hi / lo);
    let mut waves = Vec::with_capacity(count);
    for _ in 0..count {
        let (u_lam, u_dir, u_phi, u_amp) =
            (rng.next_f64(), rng.next_f64(), rng.next_f64(), rng.next_f64());
        let lam = lo * libm::exp(u_lam * log_ratio);
        let dir = u_dir * std::f64::consts::PI;
        let phase = u_phi * std::f64::consts::TAU;
        // mild red tilt: longer waves carry more amplitude
        let a = libm::pow(lam / hi, 0.5) * (0.6 + 0.8 * u_amp);
        waves.push((lam, libm::cos(dir), libm::sin(dir), phase, a));
    }
    let var: f64 = waves.iter().map(|w| w.4 * w.4 * 0.5).sum();
    let norm = amp / var.sqrt().max(1e-9);
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let mut out = vec![0.0; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let p = spec.world_of(x as u32, y as u32);
            let mut v = 0.0;
            for (lam, ca, sa, phase, a) in &waves {
                let u = p.x * ca + p.y * sa;
                v += a * libm::sin(u / lam * std::f64::consts::TAU + phase);
            }
            out[y * nx + x] = v * norm;
        }
    }
    out
}

/// Erode `implied` into a carved surface whose flow field IS the network.
///
/// `keep_pit` marks cells that must stay sinks (basin embryos): they are
/// excluded from depression filling, so they capture their catchment and
/// deranged drainage emerges instead of being special-cased.
/// `erodibility` is a per-cell multiplier (C1 hardness): resistant ground
/// deflects channels physically, which is what the old geometric
/// discontinuity rules were imitating.
pub fn carve(
    spec: &GridSpec,
    implied: &Grid<f64>,
    erodibility: &[f64],
    keep_pit: &[bool],
    base_edge: Edge,
    rng: &mut DetRng,
    p: &CarveParams,
    relief_amp_m: f64,
) -> Carved {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let n = nx * ny;
    let cell = spec.cell_size;
    let cell_area = cell * cell;

    // ---- seed surface: implied + roughness -----------------------------
    // ABSOLUTE FLOOR on the roughness amplitude. Scaling it purely by
    // relief starves the flat biomes (river_valley's relief_amp is ~1.4 m,
    // giving 7 cm of seed) and D8 then routes their flats along grid rows —
    // the first carve renders showed exactly that: kilometre-long dead
    // straight horizontal channels on bottomland. The floor is what breaks
    // the symmetry that flats otherwise resolve axis-aligned.
    let amp = (p.roughness_frac * relief_amp_m).max(ROUGH_FLOOR_M);
    let rough = roughness(spec, rng, amp);
    let mut z = implied.clone();
    for i in 0..n {
        z.data[i] += rough[i];
    }

    // BASE-LEVEL DRAWDOWN. All four borders are open outlets to the flow
    // router, so without this the tile drains radially like an island and
    // the drawn base edge carries whatever share it happens to win (12–82%
    // measured across seeds). Real tiles are not single catchments either
    // (corpus max-edge share is 46–78%), but the base edge must be the
    // DOMINANT one — it is where S1 says base level sits. A ramp lowering
    // the last ~900 m into that edge states it physically, without
    // touching interior relief.
    {
        let (nx_, ny_) = (spec.nx as usize, spec.ny as usize);
        for y in 0..ny_ {
            for x in 0..nx_ {
                let d = edge_distance_m(spec, base_edge, x, y);
                let t = (1.0 - (d / base_ramp_m()).min(1.0)).clamp(0.0, 1.0);
                z.data[y * nx_ + x] -= p.base_drop_m * t * t * (3.0 - 2.0 * t);
            }
        }
    }

    // CLOSED BOUNDARIES. The flow router treats every border cell as an
    // open outlet, so a tile drains radially like an island: measured base-
    // edge outflow was 12–82% and a regional gradient up to 1.5x relief did
    // not fix it (water leaves by whichever border it reaches first).
    // S2's contract is explicit that the trunk runs from the interior out
    // through the base-level band, so the three non-base borders are RIMMED
    // — raised out of reach — and every drop of water has to find the base
    // edge. This is the standard landscape-evolution boundary condition:
    // one open boundary, the rest no-flux.
    let mut pre_rim: Vec<(usize, f64)> = Vec::new();
    if p.close_borders {
        let rim = 40.0 + 4.0 * relief_amp_m;
        for y in 0..ny {
            for x in 0..nx {
                let on_border = y == 0 || y == ny - 1 || x == 0 || x == nx - 1;
                if on_border && !outlet_side(base_edge, spec, x, y) {
                    let lin = y * nx + x;
                    pre_rim.push((lin, z.data[lin]));
                    z.data[lin] += rim;
                }
            }
        }
    }

    // Outlet band: the base-level edge must stay the lowest ground so the
    // network drains THERE rather than off an arbitrary side.
    let outlet = outlet_mask(spec, base_edge);

    // TRUNK INLET: the lowest border cell that is NOT on the base edge —
    // where the through-going river enters. A tile is a window in a larger
    // landscape, and a bottomland tile's main river carries a catchment
    // that lies mostly outside it; without external inflow the biggest
    // channel only ever drains the tile's own 9 km², which is why river
    // valley seeds read as random dendritic drainage instead of "a river
    // crossing the tile" (review observation 3).
    // The inlet sits on the FAR half of the tile from base level: a main
    // river should cross the tile edge to edge, and picking the globally
    // lowest non-base border cell often put the inlet right beside the
    // outlet, so the trunk clipped a corner instead of spanning the site
    // (review: "a single meandering channel from one edge to another").
    let inlet = if p.inflow_area_m2 > 0.0 {
        let mut best = (f64::INFINITY, usize::MAX);
        for y in 0..ny {
            for x in 0..nx {
                let on_border = y == 0 || y == ny - 1 || x == 0 || x == nx - 1;
                if !on_border || outlet_side(base_edge, spec, x, y) {
                    continue;
                }
                // Require the far half AND the middle 60% of that edge:
                // the lowest far cell is often a corner, and a corner inlet
                // makes the trunk hug the rimmed border instead of crossing
                // the site (the old engine placed its outlet in the middle
                // 60% for the same reason).
                if edge_distance_m(spec, base_edge, x, y) < 0.5 * EXTENT_M {
                    continue;
                }
                let (fx, fy) = (x as f64 / (nx - 1) as f64, y as f64 / (ny - 1) as f64);
                let along = if matches!(base_edge, Edge::E | Edge::W) { fy } else { fx };
                if !(0.2..=0.8).contains(&along) {
                    continue;
                }
                let lin = y * nx + x;
                if z.data[lin] < best.0 {
                    best = (z.data[lin], lin);
                }
            }
        }
        best.1
    } else {
        usize::MAX
    };

    // ---- erosion loop (fixed iterations) -------------------------------
    let mut rec;
    let mut area = vec![cell_area; n];
    let mut area_natural: Option<Vec<f64>> = None;
    {
        // Start from a depression-free surface: the roughness sum and C1's
        // own wave interference both create closed lows, and a pooled cell
        // has zero slope so the carve can never drain it. Pre-filling makes
        // every lake a flat at spill level whose OUTLET carries the full
        // upstream area — the outlet then incises and the lake drains, the
        // way drainage integration actually works.
        // NATURAL CLOSED BASINS. A deranged landscape keeps its own
        // depressions: interdune lows in the sandhills swallow their
        // catchment and never pass it on (the Nebraska Sandhills famously
        // shed no surface water). Filling them instead forces flow ACROSS
        // each basin floor, and a filled floor is flat, so the crossing is
        // a dead-straight line — 61% of sandhills channel cells sat in
        // filled pools and the review saw the result as "awkward straight
        // sections". The deepest depressions are handed to the router as
        // kept pits, in proportion to the drawn derangement, so flow ends
        // in them exactly as it does in the field.
        let mut pits: Vec<bool> = keep_pit.to_vec();
        if pits.len() < n {
            pits.resize(n, false);
        }
        if p.derangement > 0.0 {
            let zf0 = flow::fill_depressions(&z);
            let mut depth: Vec<(f64, usize)> = (0..n)
                .map(|i| (zf0.data[i] - z.data[i], i))
                .filter(|(d, _)| *d > 0.05)
                .collect();
            depth.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
            let keep_n = ((depth.len() as f64) * p.derangement) as usize;
            for &(_, i) in depth.iter().take(keep_n) {
                pits[i] = true;
            }
        }
        let keep_pit: &[bool] = &pits;
        z = flow::fill_depressions_masked(&z, keep_pit);
        // De-flatten — ON THE ROUTING SURFACE ONLY. A priority-flood fill
        // grades its lakes by epsilon (8e-4 m), and an eps-flat under a
        // PLANAR macro tilt gives D8 an exactly axis-parallel descent (the
        // probe found a 712 m dead-straight due-east channel, invariant to
        // every erosion dial). A short-wave perturbation gives the flow
        // real terrain to follow across former lake floors. Applying it to
        // the TERRAIN instead dug fresh pits that the next fill turned back
        // into lakes — 47% of channel cells ended up pooled, and the
        // straight runs returned. The routing surface is the only place it
        // is needed, and the carved terrain stays depression-free.
        // ISOTROPIC per-cell dither, NOT a wave sum. A sum of a few plane
        // waves has parallel crests and troughs; used as the routing
        // surface's tie-breaker it handed flow a corduroy to follow, and
        // the review immediately saw dead-straight parallel tributaries
        // across the flats. Position-hashed noise has no preferred
        // direction, so ties break every which way.
        let deflat: Vec<f64> = (0..n)
            .map(|i| {
                let (y, x) = ((i / nx) as u64, (i % nx) as u64);
                let mut h = x
                    .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                    ^ y.rotate_left(32).wrapping_mul(0xBF58_476D_1CE4_E5B9)
                    ^ 0x51C3_7A2E_9D4B_F015;
                h ^= h >> 30;
                h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
                h ^= h >> 27;
                h = h.wrapping_mul(0x94D0_49BB_1331_11EB);
                h ^= h >> 31;
                ((h >> 11) as f64 / (1u64 << 53) as f64 - 0.5) * 2.0 * DEFLAT_AMP_M
            })
            .collect();
        // BLUR the dither into a spatially correlated field (~40 m). Pure
        // per-cell white noise breaks ties, but it also re-randomises the
        // flow direction at every step, so tributaries arrive at whatever
        // angle the noise dictates: the T-junction share sat at 25% against
        // a real 4-20%. A correlated field still has no preferred
        // direction — so no corduroy — but neighbouring cells now agree
        // about which way is downhill.
        let deflat = {
            let mut d = deflat;
            for _ in 0..3 {
                let mut out = vec![0.0; n];
                for y in 0..ny {
                    for x in 0..nx {
                        let mut acc = 0.0;
                        let mut cnt = 0.0;
                        for dy in -1i64..=1 {
                            for dx in -1i64..=1 {
                                let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                                if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                                    continue;
                                }
                                acc += d[yy as usize * nx + xx as usize];
                                cnt += 1.0;
                            }
                        }
                        out[y * nx + x] = acc / cnt;
                    }
                }
                d = out;
            }
            // blurring shrinks the amplitude; restore it
            let rms = (d.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt().max(1e-12);
            let g = (DEFLAT_AMP_M / 1.732) / rms;
            d.iter().map(|v| v * g).collect::<Vec<f64>>()
        };
        // ROUTING WANDER — the second routing-surface field, and the one
        // that decides whether the drainage looks made or found.
        //
        // C1 is band-limited to >= 400 m, so a hillslope is locally a
        // near-perfect plane, and D8 on a plane picks the SAME neighbour
        // every step: paths lock onto an axis or a 45 deg diagonal and run
        // straight for hundreds of metres. That is the grid signature the
        // S2-isolation probe measured (orientation excess 1.7-1.9 against
        // 1.02 on real tiles) and no amount of smoothing removes it —
        // blurring a straight line leaves a straight line.
        //
        // The deflat dither cannot help: it is deliberately flat-ground
        // only (slope < DITHER_SLOPE_MAX) and fixed-amplitude, so on any
        // real gradient it is invisible. This field is SLOPE-PROPORTIONAL
        // instead: amplitude = wander * slope * WANDER_LEN_M gives the same
        // angular deflection (~15 deg at 0.25) at every steepness, which is
        // what sub-grid roughness does to a real flow path. Correlated at
        // ~120 m so neighbouring cells agree on the deflection — the
        // T-junction lesson from the dither applies here too.
        let wander: Vec<f64> = {
            let mut d: Vec<f64> = (0..n)
                .map(|i| {
                    let (y, x) = ((i / nx) as u64, (i % nx) as u64);
                    let mut h = x
                        .wrapping_mul(0xD6E8_FEB8_6659_FD93)
                        ^ y.rotate_left(32).wrapping_mul(0xA0761D6478BD642F)
                        ^ 0x2545_F491_4F6C_DD1D;
                    h ^= h >> 33;
                    h = h.wrapping_mul(0xFF51_AFD7_ED55_8CCD);
                    h ^= h >> 33;
                    h = h.wrapping_mul(0xC4CE_B9FE_1A85_EC53);
                    h ^= h >> 33;
                    (h >> 11) as f64 / (1u64 << 53) as f64 - 0.5
                })
                .collect();
            for _ in 0..WANDER_BLUR_PASSES {
                let mut out = vec![0.0; n];
                for y in 0..ny {
                    for x in 0..nx {
                        let mut acc = 0.0;
                        let mut cnt = 0.0;
                        for dy in -1i64..=1 {
                            for dx in -1i64..=1 {
                                let (yy, xx) = (y as i64 + dy, x as i64 + dx);
                                if yy < 0 || xx < 0 || yy >= ny as i64 || xx >= nx as i64 {
                                    continue;
                                }
                                acc += d[yy as usize * nx + xx as usize];
                                cnt += 1.0;
                            }
                        }
                        out[y * nx + x] = acc / cnt;
                    }
                }
                d = out;
            }
            let rms = (d.iter().map(|v| v * v).sum::<f64>() / n as f64).sqrt().max(1e-12);
            d.iter().map(|v| v / rms).collect::<Vec<f64>>()
        };
        // ROUTING SHOULDER — on the ROUTING SURFACE ONLY, like the dither.
        // The rim is a single-row wall, so any cross-tilt piles flow
        // against it and erosion carves the collector channel RIGHT ALONG
        // the border (wall-following): rv seeds 2/4/12 ran their river
        // pinned to a rimmed edge for up to the full tile. A smooth raise
        // over the last ~260 m into each RIMMED border pushes the
        // collector inboard of the frame (river rule: ≥ 2× width from the
        // edge unless terminating), while the terrain keeps no berm. The
        // base edge is exempt (drawdown owns it), so nothing blocks the
        // outlet, and the inlet still enters by descending the shoulder.
        let shoulder: Vec<f64> = {
            let cellm = spec.cell_size;
            let in_base = |b: Edge| -> bool {
                use Edge::*;
                match base_edge {
                    S => matches!(b, S),
                    N => matches!(b, N),
                    W => matches!(b, W),
                    E => matches!(b, E),
                    CornerSw => matches!(b, S | W),
                    CornerSe => matches!(b, S | E),
                    CornerNw => matches!(b, N | W),
                    CornerNe => matches!(b, N | E),
                }
            };
            (0..n)
                .map(|i| {
                    let (y, x) = (i / nx, i % nx);
                    let (dl, dr) = (x as f64 * cellm, (nx - 1 - x) as f64 * cellm);
                    let (db, dt) = (y as f64 * cellm, (ny - 1 - y) as f64 * cellm);
                    let mut d = f64::INFINITY;
                    if !in_base(Edge::W) { d = d.min(dl); }
                    if !in_base(Edge::E) { d = d.min(dr); }
                    if !in_base(Edge::S) { d = d.min(db); }
                    if !in_base(Edge::N) { d = d.min(dt); }
                    let t = (1.0 - d / SHOULDER_W_M).clamp(0.0, 1.0);
                    SHOULDER_M * t * t
                })
                .collect()
        };
        let route_surface = |z: &Grid<f64>, keep_pit: &[bool]| -> Grid<f64> {
            // Dither FIRST, then flood once: applying it after the fill
            // needs a second flood to clear the pits it creates, and the
            // priority-flood is this stage's dominant cost (two per
            // iteration put S2 at 828 ms against a 900 ms budget).
            let mut zf = z.clone();
            // Only FLAT ground gets the dither. Applied everywhere it adds
            // cell-scale noise to slopes that already drain perfectly well,
            // which showed up as a 25% T-junction share (real: 4-20%) and
            // 15-20% excess sinuosity — tributaries arriving at whatever
            // angle the noise dictated instead of the angle the valley
            // dictates. The local gradient is computed on the RAW surface,
            // before any fill, so no ordering problem arises.
            let (gnx, gny) = (zf.spec.nx as usize, zf.spec.ny as usize);
            let gcell = zf.spec.cell_size;
            for y in 0..gny {
                for x in 0..gnx {
                    let i = y * gnx + x;
                    let xm = x.saturating_sub(1);
                    let xp = (x + 1).min(gnx - 1);
                    let ym = y.saturating_sub(1);
                    let yp = (y + 1).min(gny - 1);
                    let gx = (z.data[y * gnx + xp] - z.data[y * gnx + xm]) / (2.0 * gcell);
                    let gy = (z.data[yp * gnx + x] - z.data[ym * gnx + x]) / (2.0 * gcell);
                    let slope = (gx * gx + gy * gy).sqrt();
                    // taper in over the last decade of slope so there is no
                    // seam between dithered and undithered ground
                    let w = (1.0 - slope / DITHER_SLOPE_MAX).clamp(0.0, 1.0);
                    // The floor matters where it is least obvious: a valley
                    // FLOOR has almost no slope, so a purely slope-keyed
                    // wander vanishes exactly on the reaches whose
                    // straightness is most visible. The deflat dither is
                    // 0.10 m at 40 m — enough to break ties, not enough to
                    // bend a trunk.
                    let amp = (slope * WANDER_LEN_M).max(WANDER_FLOOR_M);
                    zf.data[i] += deflat[i] * w * w + shoulder[i]
                        + wander[i] * amp * p.route_wander;
                }
            }
            // The flood then guarantees a depression-free routing surface,
            // dither included — applying the dither afterwards left every
            // cell it pushed below its neighbours as an artificial PIT,
            // which the router reads as an outlet, and the network
            // shattered into disconnected rills (seed 48's 20 km² river
            // died two cells after its inlet).
            flow::fill_depressions_masked(&zf, keep_pit)
        };
        // Uplift weight: zero at the base edge (the outlet elevation is the
        // datum the whole network grades to and must not move), full
        // beyond the drawdown ramp. Same profile the drawdown uses, so the
        // two are consistent.
        let uplift_w: Vec<f64> = if p.uplift_m > 0.0 {
            (0..n)
                .map(|i| {
                    let (y, x) = (i / nx, i % nx);
                    let d = edge_distance_m(spec, base_edge, x, y);
                    let t = (d / base_ramp_m()).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                })
                .collect()
        } else {
            Vec::new()
        };
        let uplift_step = if p.iters > 0 { p.uplift_m / p.iters as f64 } else { 0.0 };
        // The imported catchment arrives LATE. Injected from iteration 1 it
        // lands on the smoothest version of the surface — where D8 gives
        // the straightest path it will ever give — and 10-20 km² of
        // discharge cuts that path so deep on the first pass that no later
        // iteration can move it: the trunk locks in, ruler-straight, and
        // stays. Holding it back lets the terrain grow its own main stem
        // first, and the river then inherits a valley rather than drawing
        // one. Zero keeps the historic behaviour.
        let inflow_from = ((p.inflow_start_frac.clamp(0.0, 0.9) * p.iters as f64) as usize)
            .min(p.iters.saturating_sub(1));
        for it in 0..(if p.k > 0.0 { p.iters } else { 0 }) {
            // Uplift FIRST, then route and cut: the drainage spends the
            // iteration cutting back down through what just rose, which is
            // the loop that leaves interfluves standing between valleys.
            if !uplift_w.is_empty() {
                for i in 0..n {
                    z.data[i] += uplift_step * uplift_w[i];
                }
            }
            let zf = route_surface(&z, keep_pit);
            let (r, slope) = flow::receivers(&zf, cell);
            let acc = flow::accumulate(&r);
            for i in 0..n {
                area[i] = acc[i] as f64 * cell_area;
            }
            if it >= inflow_from {
                add_inflow(&mut area, &r, inlet, p.inflow_area_m2);
            }
            rec = r;
            // Carve DOWNSTREAM-FIRST, capping each cell's cut at half its
            // drop to the already-carved receiver. Two artifacts died here:
            // unconstrained carving digs pits (fill turns them into flat
            // pools, and D8 crosses a pool along grid rows), while
            // repairing pits afterwards flattens whole reaches onto a
            // uniform minimum-drop ramp — which reads as a dead-straight
            // channel, exactly the 696 m horizontal run the probe found.
            // Capping by the local drop keeps the surface strictly
            // drainable with the terrain's own gradients intact.
            carve_downstream(
                &mut z, &rec, &slope, &area, erodibility, &outlet, cell_area, p,
            );
            // ...then let the hillslopes relax. Stream power alone writes
            // its cut along ONE D8 receiver per cell, and a D8 chain can
            // only run along an axis or a diagonal: 15 iterations of it
            // etch an 8-24 m rectilinear comb into every flank (the S2
            // isolation probe found the comb complete in this stage,
            // untouched by the catena, and present with every later tier
            // switched off). Real flanks are smooth BETWEEN their gullies
            // because creep diffuses them; this is that missing half of
            // the erosion law, and the pairing is what gives convex
            // interfluves with concave valleys between them.
            hillslope_creep(&mut z, &area, p);
        }
        // Final route on the carved surface: this is the field the network
        // is extracted from, so it must match the surface exactly. It runs
        // for EVERY biome, including the ones that do not erode (k = 0):
        // flow still concentrates on their surface, and those lines are the
        // dry drainage the corpus measures at d2c ~103–118 m everywhere.
        let zf = route_surface(&z, keep_pit);
        let (r, _) = flow::receivers(&zf, cell);
        let acc = flow::accumulate(&r);
        rec = r;
        for i in 0..n {
            area[i] = acc[i] as f64 * cell_area;
        }
        // NATURAL accumulation snapshot before the trunk inflow: the
        // imported discharge exists for the trunk's own realism (meander
        // wavelength, width, incision), not to mint tributaries — with
        // inflow included in the extraction test the corridor crossed
        // the cut wholesale and rv's d2c sat 15 m below the corpus band
        // at every initiation setting.
        area_natural = Some(area.clone());
        add_inflow(&mut area, &rec, inlet, p.inflow_area_m2);
    }

    // ---- extraction ----------------------------------------------------
    // Extraction is INDEPENDENT of how hard the biome erodes. The corpus
    // measures d2c 103–118 m and density 2.3–2.7 km/km² in EVERY biome
    // including heathland and sandhills — flow concentrates on any real
    // surface whether or not a perennial stream runs there, and those are
    // the "dried channel-like features" the review saw on heathland tiles.
    // What differs per biome is incision depth (k) and integration, not
    // whether the lines exist. Gating extraction on k produced zero
    // channels for two biomes against a measured invariant.
    // A deranged landscape's basins swallow their catchments, so no cell
    // ever accumulates much: at the integrated threshold sandhills fell to
    // 0.42 km/km² against a corpus 2.36. Its swales are real, they simply
    // drain small areas — the cut scales down with derangement so the
    // measured density band is met without re-opening the basins.
    let a_ext: &[f64] = area_natural.as_deref().unwrap_or(&area);
    let thresh = p.area_threshold_m2 * (1.0 - 0.85 * p.derangement).max(0.05);
    // slope-adaptive initiation (see SLOPE_INIT_CLAMP doc). The slope is
    // taken on a ~300 m lowpass of the carved surface: what should pull
    // channel heads upslope is the MACRO flank (a kilometre ridge stays
    // steep after the lowpass), not ordinary carved valley walls (100 m
    // features wash out) — the first cut used near-raw slope and dragged
    // every biome's d2c down together instead of fixing the outlier.
    let slope = {
        let nx = spec.nx as usize;
        let cell = spec.cell_size;
        let half = ((300.0 / cell) as usize / 2).max(1) as i64;
        let mut lp = z.data.clone();
        for _ in 0..2 {
            // separable box, horizontal then vertical
            let mut t = vec![0.0f64; n];
            for y in 0..nx {
                for x in 0..nx {
                    let mut acc = 0.0;
                    let mut cnt = 0.0;
                    for dx in -half..=half {
                        let xx = (x as i64 + dx).clamp(0, nx as i64 - 1) as usize;
                        acc += lp[y * nx + xx];
                        cnt += 1.0;
                    }
                    t[y * nx + x] = acc / cnt;
                }
            }
            for x in 0..nx {
                for y in 0..nx {
                    let mut acc = 0.0;
                    let mut cnt = 0.0;
                    for dy in -half..=half {
                        let yy = (y as i64 + dy).clamp(0, nx as i64 - 1) as usize;
                        acc += t[yy * nx + x];
                        cnt += 1.0;
                    }
                    lp[y * nx + x] = acc / cnt;
                }
            }
        }
        let mut s = vec![0.0f64; n];
        for y in 0..nx {
            for x in 0..nx {
                let xm = x.saturating_sub(1);
                let xp = (x + 1).min(nx - 1);
                let ym = y.saturating_sub(1);
                let yp = (y + 1).min(nx - 1);
                let gx = (lp[y * nx + xp] - lp[y * nx + xm])
                    / (((xp - xm).max(1)) as f64 * cell);
                let gy = (lp[yp * nx + x] - lp[ym * nx + x])
                    / (((yp - ym).max(1)) as f64 * cell);
                s[y * nx + x] = (gx * gx + gy * gy).sqrt();
            }
        }
        s
    };
    // Reference = median floored at 0.6×mean. The median alone is
    // degenerate on a bimodal tile (flat floodplain + steep bluffs):
    // near-zero reference handed the bluffs maximum boost at any
    // exponent and pushed that biome's d2c ~15 m below the corpus band.
    // On such tiles the mean sits far above the median and lifts the
    // reference; on unimodal hillslope terrain the two agree and
    // nothing changes. (A p70 reference was tried and rejected: it
    // raised thresholds for 70% of cells and moved EVERY biome up
    // ~20 m.)
    let s_med = {
        let mut v: Vec<f64> = slope.iter().copied().filter(|x| *x > 1e-9).collect();
        v.sort_by(|a, b| a.total_cmp(b));
        if v.is_empty() {
            1e-9
        } else {
            let med = v[v.len() / 2];
            let mean = v.iter().sum::<f64>() / v.len() as f64;
            med.max(0.6 * mean)
        }
    };
    let is_channel: Vec<bool> = (0..n)
        .map(|i| {
            let r = s_med / slope[i].max(1e-9);
            let f = if r < 1.0 { libm::pow(r, SLOPE_INIT_STEEP_EXP) } else { r }
                .clamp(SLOPE_INIT_CLAMP.0, SLOPE_INIT_CLAMP.1);
            a_ext[i] >= thresh * f
        })
        .collect();
    // Tier-2 side-valleys: same slope-adaptive initiation at a lower cut,
    // so heads concentrate on steep flanks — exactly where real dendritic
    // networks dissect high ground.
    //
    // TRACED, not thresholded. The first cut of this tier marked every
    // cell that passed the area test, and cells pass it INTERMITTENTLY
    // along a path (the slope factor varies cell to cell), so the tier
    // came out as disconnected 40-200 m dashes that read as marks
    // scattered on an unchanged hillside. Here a qualifying cell is a
    // HEAD, and each head's receiver chain is walked downstream and
    // marked until it meets tier-1: connectivity is then structural, and
    // what gets carved is a branch of the drainage rather than a dash.
    // Walks stop on already-marked cells, so the whole pass is O(n).
    //
    // Heads inside the routing frame are refused. The closed border
    // collects flow along its whole length, so the old 40 m margin let a
    // 2 km border-parallel drawdown line qualify as one enormous
    // "side-valley" — the densest tier-2 feature in the tile was
    // construction. Walks also stop when they enter the frame band.
    let tier2_cut_m2 = thresh * p.tributary_reach;
    let tier2: Vec<bool> = if p.tributary_reach < 1.0 {
        let nx = spec.nx as usize;
        let head_margin = ((SHOULDER_W_M / spec.cell_size) as usize).max(2);
        let walk_margin = ((SHOULDER_W_M / 2.0 / spec.cell_size) as usize).max(2);
        let in_band = |i: usize, m: usize| -> bool {
            let (y, x) = (i / nx, i % nx);
            x < m || y < m || x >= nx - m || y >= nx - m
        };
        let mut mask = vec![false; n];
        for i in 0..n {
            if is_channel[i] || in_band(i, head_margin) || slope[i] < TIER2_MIN_SLOPE {
                continue;
            }
            let r = s_med / slope[i].max(1e-9);
            let f = if r < 1.0 { libm::pow(r, SLOPE_INIT_STEEP_EXP) } else { r }
                .clamp(SLOPE_INIT_CLAMP.0, SLOPE_INIT_CLAMP.1);
            if a_ext[i] < tier2_cut_m2 * f {
                continue;
            }
            let mut cur = i;
            let mut guard = 0usize;
            while guard < n {
                guard += 1;
                // A gully dies where the flank does. Without this the walk
                // runs out across the footslope plain as a dead-straight
                // D8 line — flat ground has no relief above the path for
                // the banks profile to cut, so it draws a line and carves
                // nothing, which is the worst of both.
                if is_channel[cur]
                    || mask[cur]
                    || in_band(cur, walk_margin)
                    || slope[cur] < TIER2_MIN_SLOPE * 0.6
                {
                    break;
                }
                mask[cur] = true;
                match rec[cur] {
                    r if r >= 0 => cur = r as usize,
                    _ => break,
                }
            }
        }
        mask
    } else {
        vec![false; n]
    };
    // Tier-2 reaches → smoothed polylines. Same head/confluence split the
    // tier-1 tracer uses, then Chaikin ×2: a 25-50 m gully cut along a raw
    // D8 chain reproduces every 45° corner at a scale where the corner is
    // as wide as the valley, which is the "not organic" tell at close zoom.
    let tier2_paths: Vec<Vec<Vec2>> = if tier2.iter().any(|&t| t) {
        let nxu = spec.nx as usize;
        let mut donors = vec![0u32; n];
        for i in 0..n {
            if !tier2[i] {
                continue;
            }
            if let Some(r) = usize::try_from(rec[i]).ok().filter(|&r| tier2[r]) {
                donors[r] += 1;
            }
        }
        (0..n)
            .filter(|&i| tier2[i] && donors[i] != 1)
            .map(|s| {
                let mut path = vec![s];
                let mut cur = s;
                while let Some(r) = usize::try_from(rec[cur]).ok().filter(|&r| tier2[r]) {
                    path.push(r);
                    if donors[r] >= 2 {
                        break;
                    }
                    cur = r;
                }
                let pts: Vec<Vec2> = path
                    .iter()
                    .map(|&l| spec.world_of((l % nxu) as u32, (l / nxu) as u32))
                    .collect();
                chaikin(&pts, 2)
            })
            .filter(|p| p.len() > 1)
            .collect()
    } else {
        Vec::new()
    };
    let order_at = strahler(&rec, &is_channel, n);
    let (channels, channel_of) = trace(spec, &rec, &is_channel, &order_at, &area, &z);

    // ---- MAJORS pass (reviewer hierarchy spec) -------------------------
    // Real tiles read as a FEW channels with major effects over a field
    // of subtle swales; uniform incision spread the depth budget across
    // the whole network and every cut looked equally irregular. Rank the
    // channel SYSTEMS (root trees) by mouth discharge, take the top two
    // (a third only if it carries >= MAJORS_THIRD_FRAC of the second),
    // and deepen ONLY their cells with extra stream-power sweeps on the
    // FIXED receiver forest - the certified network topology, the
    // extraction, and d2c are untouched; only their relief deepens.
    if !channels.is_empty() && p.k > 0.0 {
        let mut root_of = vec![0u32; channels.len()];
        for ci in 0..channels.len() {
            let mut r = ci as u32;
            while let Some(par) = channels[r as usize].parent {
                r = par;
            }
            root_of[ci] = r;
        }
        let mut roots: Vec<u32> = root_of.clone();
        roots.sort_unstable();
        roots.dedup();
        let mut ranked: Vec<(f64, u32)> = roots
            .iter()
            .map(|&r| (channels[r as usize].area_m2, r))
            .collect();
        ranked.sort_by(|a, b| b.0.total_cmp(&a.0));
        let mut majors: Vec<u32> = ranked.iter().take(MAJORS_CAP).map(|&(_, r)| r).collect();
        if ranked.len() > MAJORS_CAP
            && ranked[MAJORS_CAP].0 >= MAJORS_THIRD_FRAC * ranked[MAJORS_CAP - 1].0
        {
            majors.push(ranked[MAJORS_CAP].1);
        }
        let major_erod: Vec<f64> = (0..n)
            .map(|i| match channel_of[i] {
                Some(ci) if majors.contains(&root_of[ci as usize]) => {
                    erodibility[i] * MAJORS_K_BOOST
                }
                _ => 0.0,
            })
            .collect();
        let (_, slope_now) = flow::receivers(&z, spec.cell_size);
        let outlet_mask: Vec<bool> = (0..n).map(|i| rec[i] < 0).collect();
        for _ in 0..MAJORS_ITERS {
            carve_downstream(
                &mut z,
                &rec,
                &slope_now,
                &area,
                &major_erod,
                &outlet_mask,
                spec.cell_size * spec.cell_size,
                p,
            );
        }
    }

    Carved {
        z,
        channel_of,
        order_at,
        rec,
        area,
        channels,
        inlet,
        pre_rim,
        tier2,
        tier2_cut_m2,
        tier2_paths,
    }
}

/// One carving pass in downstream-to-upstream order: a cell is cut only
/// after its receiver, and never by more than half its remaining drop to
/// that receiver. Strict monotonicity holds by construction (no pits, so
/// no flat pools) without imposing any artificial uniform gradient.
/// O(n), no recursion, deterministic.
#[allow(clippy::too_many_arguments)]
fn carve_downstream(
    z: &mut Grid<f64>,
    rec: &[i64],
    slope: &[f64],
    area: &[f64],
    erodibility: &[f64],
    outlet: &[bool],
    cell_area: f64,
    p: &CarveParams,
) {
    let n = rec.len();
    // The DESIRED cut, before the drainability caps. Computing it up front
    // is what makes the hillslope share smearable: on sub-threshold ground
    // a D8 chain concentrates the whole iteration's cut into a one-cell
    // groove, and fifteen of those are the rectilinear comb. Channels are
    // genuinely narrow, so their cut stays exactly where the flow put it —
    // the blend is keyed on drained area.
    // Stream power with a TUNABLE discharge exponent. At the classic m=0.5
    // a divide cell erodes only ~40× less than a threshold channel, so
    // fifteen iterations lower the interfluves nearly as much as the
    // valleys and the tile stays symmetric rolling ground — measured
    // crest/valley p90 ratio ~1.6 against a corpus 0.76-0.85, and neither
    // a higher ceiling, a bigger k, uplift, nor less S1 relief moved it.
    // Raising m concentrates the same erosion into the channels and leaves
    // the interfluves standing, which is the shape the corpus has.
    // Normalized at the extraction threshold so k keeps its meaning.
    let a_ref = (p.area_threshold_m2 / cell_area).max(1.0);
    let norm = libm::pow(a_ref, 0.5 - p.area_exp);
    let mut want: Vec<f64> = (0..n)
        .map(|i| {
            if outlet[i] {
                0.0
            } else {
                p.k * norm
                    * libm::pow((area[i] / cell_area).max(1.0), p.area_exp)
                    * slope[i]
                    * erodibility[i]
                    * p.incision_scale
            }
        })
        .collect();
    if p.cut_spread_m > 0.0 {
        let (nx, ny) = (z.spec.nx as usize, z.spec.ny as usize);
        let r = (p.cut_spread_m / z.spec.cell_size / 1.6).round().max(1.0) as usize;
        let spread = box_blur_sep(&want, nx, ny, r, 2);
        let thresh = p.area_threshold_m2.max(1.0);
        for i in 0..n {
            let u = ((area[i] - 0.5 * thresh) / thresh).clamp(0.0, 1.0);
            // Channels keep most of their crispness, but not all of it: a
            // fully unspread cut lands entirely in ONE 8 m cell and reads
            // as a black one-pixel slot on the hillshade (the catena
            // shapes banks, it does not smooth a notch). The floor gives
            // the cut a three-cell cross-section — still a channel, no
            // longer a knife line.
            let hill = (1.0 - u * u * (3.0 - 2.0 * u)).max(SPREAD_CHANNEL_FLOOR);
            want[i] += hill * (spread[i] - want[i]);
        }
    }
    let mut donors: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut stack: Vec<usize> = Vec::new();
    for i in 0..n {
        let r = rec[i];
        if r >= 0 {
            donors[r as usize].push(i as u32);
        } else {
            stack.push(i);
        }
    }
    while let Some(i) = stack.pop() {
        for &d in &donors[i] {
            let d = d as usize;
            if !outlet[d] {
                // never take more than half the drop to the receiver, and
                // never more than the per-iteration clamp
                let head = (z.data[d] - z.data[i]).max(0.0) * 0.5;
                z.data[d] -= want[d].min(p.step_clamp_m).min(head);
            }
            stack.push(d);
        }
    }
}

/// Separable box blur, `passes` × (2r+1) taps, edge-clamped.
fn box_blur_sep(src: &[f64], nx: usize, ny: usize, r: usize, passes: usize) -> Vec<f64> {
    let mut cur = src.to_vec();
    let mut tmp = vec![0.0f64; nx * ny];
    for _ in 0..passes {
        for y in 0..ny {
            for x in 0..nx {
                let mut acc = 0.0;
                for dx in -(r as i64)..=(r as i64) {
                    let xx = (x as i64 + dx).clamp(0, nx as i64 - 1) as usize;
                    acc += cur[y * nx + xx];
                }
                tmp[y * nx + x] = acc / (2 * r + 1) as f64;
            }
        }
        for x in 0..nx {
            for y in 0..ny {
                let mut acc = 0.0;
                for dy in -(r as i64)..=(r as i64) {
                    let yy = (y as i64 + dy).clamp(0, ny as i64 - 1) as usize;
                    acc += tmp[yy * nx + x];
                }
                cur[y * nx + x] = acc / (2 * r + 1) as f64;
            }
        }
    }
    cur
}

/// Hillslope creep — the diffusive half of the erosion law.
///
/// One explicit 5-point step per erosion iteration: `z += α·w·(mean4 − z)`.
/// Over `ITERS` steps that is a Gaussian of σ ≈ `cell·√(N·α/2)` — ~7 m at
/// α = 0.10, which erases the D8 comb (8–24 m) while leaving the bands S2
/// owns (≥64 m) nearly intact.
///
/// Two things it must not do:
/// - **Fill the channels.** `w` fades creep out as drained area approaches
///   the extraction threshold: fluvial transport dominates there, and the
///   carve's cut is the whole point.
/// - **Smear the rim.** The closed border is a single-row wall tens of
///   metres tall (routing construction, stripped before presentation).
///   Averaging against it would pull a berm inboard — the mirror image of
///   the catena's moat. Border cells are neither updated nor read: a
///   neighbour on the border contributes the centre value instead, which
///   is a zero-flux wall.
fn hillslope_creep(z: &mut Grid<f64>, area: &[f64], p: &CarveParams) {
    let alpha = p.creep.clamp(0.0, CREEP_ALPHA_MAX);
    if alpha <= 0.0 {
        return;
    }
    let (nx, ny) = (z.spec.nx as usize, z.spec.ny as usize);
    let src = z.data.clone();
    let thresh = p.area_threshold_m2.max(1.0);
    for y in 1..ny - 1 {
        for x in 1..nx - 1 {
            let i = y * nx + x;
            let u = ((area[i] - 0.5 * thresh) / thresh).clamp(0.0, 1.0);
            let w = 1.0 - u * u * (3.0 - 2.0 * u);
            if w <= 0.0 {
                continue;
            }
            let nb = |xx: usize, yy: usize| -> f64 {
                if xx == 0 || yy == 0 || xx == nx - 1 || yy == ny - 1 {
                    src[i]
                } else {
                    src[yy * nx + xx]
                }
            };
            let mean4 =
                0.25 * (nb(x - 1, y) + nb(x + 1, y) + nb(x, y - 1) + nb(x, y + 1));
            z.data[i] += alpha * w * (mean4 - src[i]);
        }
    }
}

/// Distance (m) from a cell to the base-level edge.
fn edge_distance_m(spec: &GridSpec, edge: Edge, x: usize, y: usize) -> f64 {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    let cell = spec.cell_size;
    let (dl, dr) = (x as f64 * cell, (nx - 1 - x) as f64 * cell);
    let (db, dt) = (y as f64 * cell, (ny - 1 - y) as f64 * cell);
    match edge {
        Edge::S => db,
        Edge::N => dt,
        Edge::W => dl,
        Edge::E => dr,
        Edge::CornerSw => db.min(dl),
        Edge::CornerSe => db.min(dr),
        Edge::CornerNw => dt.min(dl),
        Edge::CornerNe => dt.min(dr),
    }
}

/// Is this border cell on the base-level side?
fn outlet_side(edge: Edge, spec: &GridSpec, x: usize, y: usize) -> bool {
    edge_distance_m(spec, edge, x, y) < 1.0
}

/// Add an external upstream catchment entering at `inlet`, propagated
/// downstream along the receiver chain.
fn add_inflow(area: &mut [f64], rec: &[i64], inlet: usize, extra: f64) {
    if inlet == usize::MAX || extra <= 0.0 {
        return;
    }
    let mut cur = inlet;
    let mut guard = 0usize;
    loop {
        area[cur] += extra;
        let r = rec[cur];
        if r < 0 {
            break;
        }
        cur = r as usize;
        guard += 1;
        if guard > rec.len() {
            break; // defensive: the flow graph is a forest, this cannot loop
        }
    }
}

/// Cells on (or just inside) the base-level edge: protected from carving so
/// the outlet band stays the drain.
fn outlet_mask(spec: &GridSpec, edge: Edge) -> Vec<bool> {
    let (nx, ny) = (spec.nx as usize, spec.ny as usize);
    // ONE cell. A wide protected band is uncarved ground that arriving
    // flow has to cross with no channel to follow, so the eps-graded fill
    // routes it along grid rows — the long straight horizontal runs at the
    // base edge in the first carve renders. Pinning only the border row
    // keeps base level fixed while letting channels cut right up to it.
    let band = 1usize;
    let mut m = vec![false; nx * ny];
    for y in 0..ny {
        for x in 0..nx {
            let on = match edge {
                Edge::S => y < band,
                Edge::N => y + band >= ny,
                Edge::W => x < band,
                Edge::E => x + band >= nx,
                Edge::CornerSw => y < band || x < band,
                Edge::CornerSe => y < band || x + band >= nx,
                Edge::CornerNw => y + band >= ny || x < band,
                Edge::CornerNe => y + band >= ny || x + band >= nx,
            };
            m[y * nx + x] = on;
        }
    }
    m
}

/// Strahler order over the flow forest, restricted to channel cells.
/// Computed bottom-up in the same in-degree topological order `accumulate`
/// uses, so it is O(n) and deterministic.
fn strahler(rec: &[i64], is_channel: &[bool], n: usize) -> Vec<u8> {
    let mut order = vec![0u8; n];
    let mut indeg = vec![0u32; n];
    for i in 0..n {
        if !is_channel[i] {
            continue;
        }
        let r = rec[i];
        if r >= 0 && is_channel[r as usize] {
            indeg[r as usize] += 1;
        }
    }
    // heads first
    let mut stack: Vec<usize> = (0..n)
        .filter(|&i| is_channel[i] && indeg[i] == 0)
        .collect();
    // per-cell running state: (max child order, count of children at max)
    let mut best = vec![0u8; n];
    let mut best_count = vec![0u32; n];
    let mut pending = indeg.clone();
    while let Some(i) = stack.pop() {
        let o = if best_count[i] >= 2 { best[i] + 1 } else { best[i].max(1) };
        order[i] = o;
        let r = rec[i];
        if r >= 0 && is_channel[r as usize] {
            let r = r as usize;
            match o.cmp(&best[r]) {
                std::cmp::Ordering::Greater => {
                    best[r] = o;
                    best_count[r] = 1;
                }
                std::cmp::Ordering::Equal => best_count[r] += 1,
                std::cmp::Ordering::Less => {}
            }
            pending[r] -= 1;
            if pending[r] == 0 {
                stack.push(r);
            }
        }
    }
    order
}

/// Trace the raster network into polylines: one `Channel` per maximal
/// constant-order reach, walking downstream through `rec` exactly the way
/// `tools/macro_campaign/real_planform.py` traces real lidar — so the QA
/// instruments compare like with like.
#[allow(clippy::too_many_arguments)]
fn trace(
    spec: &GridSpec,
    rec: &[i64],
    is_channel: &[bool],
    order_at: &[u8],
    area: &[f64],
    z: &Grid<f64>,
) -> (Vec<Channel>, Vec<Option<u32>>) {
    let (nx, _ny) = (spec.nx as usize, spec.ny as usize);
    let n = rec.len();
    let center = |lin: usize| -> Vec2 {
        spec.world_of((lin % nx) as u32, (lin / nx) as u32)
    };
    // A reach STARTS at a channel cell that is either a head (no channel
    // donor) or a confluence (>=2 channel donors), and runs downstream
    // until the next confluence or the network's end.
    let mut donors = vec![0u32; n];
    for i in 0..n {
        if !is_channel[i] {
            continue;
        }
        let r = rec[i];
        if r >= 0 && is_channel[r as usize] {
            donors[r as usize] += 1;
        }
    }
    let mut channel_of: Vec<Option<u32>> = vec![None; n];
    let mut starts: Vec<usize> = (0..n)
        .filter(|&i| is_channel[i] && donors[i] != 1)
        .collect();
    // Deterministic order: by linear index (already ascending).
    starts.sort_unstable();

    let mut channels: Vec<Channel> = Vec::new();
    // reach id per START cell, so children can look up their parent later
    let mut reach_of_cell: Vec<Option<u32>> = vec![None; n];
    let mut raw: Vec<(Vec<usize>, u8)> = Vec::new();
    for &s in &starts {
        let mut path = vec![s];
        let mut cur = s;
        loop {
            let r = rec[cur];
            if r < 0 || !is_channel[r as usize] {
                break;
            }
            let r = r as usize;
            path.push(r);
            // stop when we reach a confluence (it starts its own reach)
            if donors[r] >= 2 {
                break;
            }
            cur = r;
        }
        if path.len() < 2 {
            continue;
        }
        let o = order_at[s].max(1);
        let id = raw.len() as u32;
        for &c in &path[..path.len() - 1] {
            channel_of[c] = Some(id);
            reach_of_cell[c] = Some(id);
        }
        raw.push((path, o));
    }

    // Parent linkage: a reach's parent is the reach owning its downstream
    // terminal cell. Arc position = distance along that parent to the cell.
    for (id, (path, o)) in raw.iter().enumerate() {
        let pts: Vec<Vec2> = path.iter().map(|&l| center(l)).collect();
        let end = *path.last().unwrap();
        let parent = channel_of[end].filter(|&p| p != id as u32);
        let junction_arc_m = match parent {
            Some(p) => {
                let ppath = &raw[p as usize].0;
                let mut arc = 0.0;
                let mut acc = 0.0;
                for w in ppath.windows(2) {
                    let (a, b) = (center(w[0]), center(w[1]));
                    acc += ((b.x - a.x).powi(2) + (b.y - a.y).powi(2)).sqrt();
                    if w[1] == end {
                        arc = acc;
                        break;
                    }
                }
                arc
            }
            None => f64::NAN,
        };
        // pts[0] is the UPSTREAM end here; the rest of the crate expects
        // pts[0] = downstream (the old grower emitted mouth-first), so
        // reverse and translate the arc accordingly.
        let mut pts_rev = pts.clone();
        pts_rev.reverse();
        // A traced path is a chain of cell CENTRES, so it staircases at
        // 45°/90° every cell — real lidar paths are measured after a ~50 m
        // smoothing for exactly this reason, and without the same
        // treatment the planform instrument read 1.32–1.43 sinuosity
        // against a real 1.06–1.10 (all of it grid staircase). Two
        // endpoint-preserving Chaikin passes put the emitted geometry on
        // the same footing as the corpus. The RASTER is untouched: this is
        // the vector representation S3, S4 and the viewer consume.
        let pts_rev = chaikin(&pts_rev, 2);
        channels.push(Channel {
            pts: pts_rev,
            order: *o,
            parent,
            junction_arc_m,
            area_m2: area[end],
        });
    }
    let _ = z;
    (channels, channel_of)
}

/// Arc length of a polyline, metres.
pub fn arc_len(pts: &[Vec2]) -> f64 {
    pts.windows(2)
        .map(|w| ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt())
        .sum()
}

/// Horton bifurcation and length ratios from the extracted network.
/// Both are `None` when fewer than two orders are present (sandhills at
/// zero density, or any tile whose network never branches).
pub fn horton_ratios(channels: &[Channel]) -> (Option<f64>, Option<f64>) {
    let max_o = channels.iter().map(|c| c.order).max().unwrap_or(0) as usize;
    if max_o < 2 {
        return (None, None);
    }
    let mut count = vec![0.0f64; max_o + 1];
    let mut length = vec![0.0f64; max_o + 1];
    for c in channels {
        let o = c.order as usize;
        if o == 0 || o > max_o {
            continue;
        }
        count[o] += 1.0;
        length[o] += arc_len(&c.pts);
    }
    let ratio = |v: &[f64], mean_len: bool| -> Option<f64> {
        let mut acc = Vec::new();
        for o in 1..max_o {
            let (a, b) = if mean_len {
                (
                    v[o + 1] / count[o + 1].max(1.0),
                    v[o] / count[o].max(1.0),
                )
            } else {
                (v[o], v[o + 1])
            };
            if b > 0.0 && a > 0.0 {
                acc.push(a / b);
            }
        }
        if acc.is_empty() {
            None
        } else {
            acc.sort_by(|x, y| x.total_cmp(y));
            Some(acc[acc.len() / 2])
        }
    };
    (ratio(&count, false), ratio(&length, true))
}

/// Point at arc position `s` along a polyline (clamped), with the local
/// tangent. Used by the junction-angle instruments.
pub fn point_at_arc(pts: &[Vec2], s: f64) -> (Vec2, Vec2) {
    let norm = |d: Vec2| {
        let l = (d.x * d.x + d.y * d.y).sqrt();
        if l > 1e-12 { Vec2::new(d.x / l, d.y / l) } else { Vec2::new(1.0, 0.0) }
    };
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let d = Vec2::new(w[1].x - w[0].x, w[1].y - w[0].y);
        let l = (d.x * d.x + d.y * d.y).sqrt();
        if acc + l >= s && l > 1e-12 {
            let t = (s - acc) / l;
            return (Vec2::new(w[0].x + d.x * t, w[0].y + d.y * t), norm(d));
        }
        acc += l;
    }
    let n = pts.len();
    let tan = if n >= 2 {
        norm(Vec2::new(pts[n - 1].x - pts[n - 2].x, pts[n - 1].y - pts[n - 2].y))
    } else {
        Vec2::new(1.0, 0.0)
    };
    (pts[n - 1], tan)
}

/// Endpoint-preserving Chaikin corner cutting.
fn chaikin(pts: &[Vec2], passes: usize) -> Vec<Vec2> {
    let mut cur = pts.to_vec();
    for _ in 0..passes {
        if cur.len() < 3 {
            break;
        }
        let mut out = Vec::with_capacity(cur.len() * 2);
        out.push(cur[0]);
        for w in cur.windows(2) {
            let (a, b) = (w[0], w[1]);
            out.push(Vec2::new(0.75 * a.x + 0.25 * b.x, 0.75 * a.y + 0.25 * b.y));
            out.push(Vec2::new(0.25 * a.x + 0.75 * b.x, 0.25 * a.y + 0.75 * b.y));
        }
        out.push(*cur.last().unwrap());
        cur = out;
    }
    cur
}
