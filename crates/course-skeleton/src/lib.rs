//! Stage S2 — the skeleton kernel. The base six are all served by one
//! fluvial engine: a light stream-power CARVE of C1's macro surface, the
//! channel network EXTRACTED from the resulting flow field, derived
//! divides, and a bank profile hung off the carved valleys. Five modules —
//! dials, not branches — add the structural forms.
//!
//! S2 no longer authors channel paths. The previous engine grew trunk and
//! tributary polylines and then defended them with crossing guards,
//! separation neighbourhoods, mouth-angle enforcement and a trim sweep;
//! ten structural patches later the review still found loops and parallel
//! pairs. Geometry authored in path space has to be argued into agreement
//! with the terrain it sits on. Here the terrain decides, and the review's
//! requirements hold structurally: a D8 receiver graph is a forest, so
//! loops are unreachable; two paths that meet share every cell afterwards,
//! so they merge rather than cross; confluences sit where terrain
//! converges. See `fluvial::carve`.
//!
//! S2 does NOT carry archetype identity (drainage spacing is a shared
//! invariant of real landscapes — 104–120 m in every archetype, heathland
//! and sandhills included). Its output is a correct, characterless base;
//! S3 supplies texture. Candle-wax interfluves are expected and fine here.
//!
//! Determinism: two streams with fixed transcripts —
//!   `skeleton/carve/v2`      carve::DRAWS uniforms (roughness waves)
//!   `skeleton/module/v1`     MAX_EMBRYOS×3 + aeolian::DRAWS uniforms
//! Every count is a compile-time constant; the erosion runs a fixed
//! iteration count and never loops until a target is met. No `if biome`
//! anywhere in this crate (grep it).
//!
//! Stage doc: `docs/stages/stage-02-skeleton-kernel.md`.

pub mod fluvial;
pub mod kernel;
pub mod modules;

use course_contracts::contracts::primitive_field::PrimitiveField;
use course_seed::{streams, RunIdentity};
use course_spec::v2::SiteSpec;
use course_world::grid::Grid;
use course_world::world::{world_spec, RES_FULL_M};

use fluvial::flow_distance::{self, Nearest};
use fluvial::{carve, catena, divides};
use kernel::{KernelId, Skeleton, SkeletonDiagnostics, SKELETON_VERSION};
use modules::{aeolian, closed_basin, integration, stratigraphy, trunk_river};

/// Light seam pass over the assembled tier-2 cut — enough to heal the
/// crease where two tier-2 fields meet, not enough to flatten a gully.
pub const TIER2_CUT_BLUR_PASSES: usize = 1;

/// THE stage-02 entry point (Kernel I, fluvial).
pub fn generate(spec: &SiteSpec, c1: &PrimitiveField, identity: &RunIdentity) -> Skeleton {
    let dial = |k: &str, default: f64| spec.dials.get(k).copied().unwrap_or(default);
    let density = dial("skeleton.density_target", 1.0);
    let m_trunk_river = dial("skeleton.trunk_river", 0.0);
    let m_strat = dial("skeleton.stratigraphy", 0.0);
    let m_basin = dial("skeleton.closed_basin", 0.0);
    // Couple the mid-band aeolian intensity to the SAME train-vs-mound
    // continuum S1's macro dunes drew (shared course scalar): a mound-
    // field course keeps small dunes but not train-strength ridges.
    let ae_u = identity.course_scalar(course_seed::RunIdentity::AEOLIAN_SALT);
    let ae_w = ae_u * ae_u * (3.0 - 2.0 * ae_u);
    let m_aeolian = dial("skeleton.aeolian", 0.0) * (0.40 + 0.60 * ae_w);
    let m_integration = dial("skeleton.integration", 0.5);
    let relief_budget = spec.descriptors.relief_budget_m;

    let spec8 = c1.grid;
    let (nx, ny) = (spec8.nx as usize, spec8.ny as usize);
    let n8 = nx * ny;

    // The implied macro surface: tilt + relief, C1's two elevation carriers.
    let mut implied = c1.tilt.clone();
    for (i, v) in implied.data.iter_mut().enumerate() {
        *v += c1.relief.data[i];
    }

    // ---- draws (fixed transcripts) ------------------------------------
    let mut carve_rng = identity.stream(streams::SKELETON_CARVE);
    let mut module_rng = identity.stream(streams::SKELETON_MODULE);
    let embryo_draws: Vec<f64> = (0..closed_basin::MAX_EMBRYOS * closed_basin::DRAWS_PER_EMBRYO)
        .map(|_| module_rng.next_f64())
        .collect();
    let aeolian_draws: Vec<f64> = (0..aeolian::DRAWS).map(|_| module_rng.next_f64()).collect();

    // ---- carve: erode, then extract the network ------------------------
    // Erodibility from C1 hardness: resistant ground deflects channels
    // physically, which is what the retired engine's geometric
    // discontinuity rules were imitating.
    let erodibility: Vec<f64> = c1
        .hardness
        .data
        .iter()
        .map(|h| (1.6 - h).clamp(0.3, 1.6))
        .collect();
    // Intended pits (S2's closed-basin embryos are stamped after the
    // carve, so the carve's own kept pits are the NATURAL depressions a
    // deranged landscape keeps — see carve::CarveParams::derangement).
    let keep_pit: Vec<bool> = vec![false; n8];
    let relief_amp = dial("primitives.relief_amp_m", 8.0);
    let carve_params = carve::CarveParams {
        roughness_frac: dial("skeleton.roughness_frac", carve::ROUGHNESS_FRAC),
        k: if density >= 0.5 { carve::K_STREAM_POWER } else { 0.0 },
        // The initiation area, and the only lever that re-centres d2c once
        // something else changes how many channels the terrain carries. It
        // was a bare constant, so a network change had nothing to trade
        // against the D5 band; the slope-adaptive factor scales THIS.
        area_threshold_m2: dial("skeleton.area_threshold_m2", carve::AREA_THRESHOLD_M2),
        incision_scale: integration::incision_scale(m_integration)
            * dial("skeleton.incision_boost", 1.0),
        base_drop_m: carve::BASE_DROP_M,
        inflow_area_m2: carve::INFLOW_PER_TRUNK_DIAL_M2 * m_trunk_river,
        close_borders: true,
        derangement: ((0.5 - m_integration) * 1.6).clamp(0.0, 0.9),
        iters: carve::ITERS,
        // THE depth lever. `iters × step_clamp` is a hard ceiling on how
        // far the drainage can cut into the macro surface — at the old
        // 15 × 0.45 m it was 6.75 m, and measured valley depth below the
        // local 200 m mean came out at 2.7-3.8 m against a corpus 2.8-6.1
        // (piedmont) and 8.5-13.6 (hill country). Raising the ceiling
        // costs nothing (same iteration count) and lets the physical
        // constraint — half the drop to the receiver — bind instead.
        step_clamp_m: dial("skeleton.cut_ceiling_m", carve::STEP_CLAMP_M)
            * dial("skeleton.incision_boost", 1.0).sqrt(),
        tributary_reach: dial("skeleton.tributary_reach", 1.0),
        creep: dial("skeleton.creep", 0.0),
        route_wander: dial("skeleton.route_wander", 0.0),
        route_meander: dial("skeleton.route_meander", 0.0),
        majors_cap: dial("skeleton.majors_cap", carve::MAJORS_CAP as f64).round().max(1.0) as usize,
        majors_boost: dial("skeleton.majors_boost", carve::MAJORS_K_BOOST),
        majors_iters: dial("skeleton.majors_iters", carve::MAJORS_ITERS as f64).round().max(0.0)
            as usize,
        cut_spread_m: dial("skeleton.cut_spread_m", 0.0),
        channel_spread: dial("skeleton.channel_spread", carve::SPREAD_CHANNEL_FLOOR),
        bank_angle_deg: dial("skeleton.bank_angle_deg", 0.0),
        inflow_start_frac: dial("skeleton.inflow_start", 0.0),
        area_exp: dial("skeleton.area_exp", 0.5),
        slope_init_floor: dial("skeleton.slope_init_floor", carve::SLOPE_INIT_CLAMP.0),
        shadow_prune_m: dial("skeleton.shadow_prune_m", 0.0),
        uplift_m: dial("skeleton.uplift_m", 0.0),
    };
    let carved = carve::carve(
        &spec8,
        &implied,
        &erodibility,
        &keep_pit,
        c1.meta.base_level.edge,
        &mut carve_rng,
        &carve_params,
        relief_amp,
    );
    let channels = carved.channels.clone();

    // ---- flow-distance transform ---------------------------------------
    // Seeded from the SMOOTHED channel polylines, not from the raw D8
    // chain. This field is the coordinate every cut in S2 is expressed in,
    // so the plan-view boundary of every cut is one of its iso-contours —
    // and the offset curve of a staircase is a staircase. A right-angle
    // turn in the chain came out as a rounded rectangle in the terrain,
    // which is the "square cut outs on the surface" the review kept
    // finding (confirmed by ablation: switching the trunk-river and
    // stratigraphy modules off left the blocks exactly where they were).
    // The traced polylines are already Chaikin-smoothed for the vector
    // export; walking them at half-cell steps costs one pass and gives
    // the whole stage a curved axis.
    let base_elev = c1.meta.base_level.elev_m;
    let seeds: Vec<(usize, Nearest)> = {
        let cell = spec8.cell_size;
        let mut v: Vec<(usize, Nearest)> = Vec::new();
        let mut seen = vec![false; n8];
        for ch in &channels {
            for seg in ch.pts.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                let len = (b.x - a.x).hypot(b.y - a.y).max(1e-9);
                let steps = ((len / (cell * 0.5)).ceil() as usize).max(1);
                for s in 0..=steps {
                    let t = s as f64 / steps as f64;
                    let px = a.x + (b.x - a.x) * t;
                    let py = a.y + (b.y - a.y) * t;
                    let cx = (px / cell).round();
                    let cy = (py / cell).round();
                    if cx < 0.0 || cy < 0.0 || cx >= nx as f64 || cy >= ny as f64 {
                        continue;
                    }
                    let lin = cy as usize * nx + cx as usize;
                    if seen[lin] {
                        continue;
                    }
                    seen[lin] = true;
                    v.push((
                        lin,
                        Nearest {
                            dist_m: 0.0,
                            z_channel: carved.z.data[lin],
                            implied_channel: implied.data[lin],
                            order: ch.order.max(1),
                            area_m2: carved.area[lin],
                        },
                    ));
                }
            }
        }
        // A traced reach can miss a carved channel cell at a junction; the
        // extraction's own cells backfill so nothing that IS a channel is
        // left without a zero-distance seed.
        for lin in 0..n8 {
            if carved.channel_of[lin].is_some() && !seen[lin] {
                seen[lin] = true;
                v.push((
                    lin,
                    Nearest {
                        dist_m: 0.0,
                        z_channel: carved.z.data[lin],
                        implied_channel: implied.data[lin],
                        order: carved.order_at[lin].max(1),
                        area_m2: carved.area[lin],
                    },
                ));
            }
        }
        if v.is_empty() {
            flow_distance::edge_seeds(&spec8, c1.meta.base_level.edge, base_elev, &implied)
        } else {
            v
        }
    };
    let near = flow_distance::euclidean(&spec8, &seeds);
    let has_channels = carved.channel_of.iter().any(|c| c.is_some());

    // ---- bank profile + modules ----------------------------------------
    // The carve did the incising, so the catena only shapes BANKS here:
    // order-scaled floors and the groove/hillslope blend, applied to the
    // carved surface rather than cutting a second set of valleys into the
    // implied one.
    // S2_DUMP=<prefix>: per-stage 8 m surfaces for the "where does the
    // straightness come from" probe (same shape as S3_TRACE/HYDRO_TRACE).
    let stage_dump = |name: &str, v: &[f64]| {
        if let Ok(prefix) = std::env::var("S2_DUMP") {
            let mut b = Vec::with_capacity(v.len() * 4);
            for x in v {
                b.extend_from_slice(&(*x as f32).to_le_bytes());
            }
            let _ = std::fs::write(format!("{prefix}_{name}.f32"), &b);
        }
    };
    stage_dump("carve", &carved.z.data);
    stage_dump("dist", &near.iter().map(|n| n.dist_m).collect::<Vec<_>>());
    let mut height8 =
        catena::banks(
            &spec8,
            &carved.z,
            &near,
            dial("skeleton.incision_boost", 1.0),
            dial("skeleton.valley_reach_m", catena::D_FULL_FLOOR_M),
            dial("skeleton.groove_scale", 1.0),
            dial("skeleton.floor_hw_min_m", catena::FLOOR_HW_MIN_M),
            dial("skeleton.groove_small_m", catena::GROOVE_SMALL_M),
            dial("skeleton.groove_share", catena::GROOVE_SHARE_SMALL),
            dial("skeleton.floor_width_scale", 1.0),
            dial("skeleton.floodplain_km2", catena::GROOVE_FULL_KM2),
        );
    stage_dump("banks", &height8.data);
    // Tier-2 dendritic side-valleys (purely morphological — the tier is
    // NOT in `channels`/`flow_distance`, so the D5 invariant, S3's
    // taper/restore and S4's river selection all still measure tier-1).
    // Distance measured to the SMOOTHED POLYLINE, in metres, not by a
    // grid Dijkstra from chain cells. Both alternatives failed for the
    // same reason: a gully is 25-50 m wide, so anything that quantizes
    // its axis to the 8 m grid prints the D8 staircase AS the valley.
    // (Rasterizing the Chaikin line back to cells re-quantizes it —
    // measured no different from the raw chain; blurring the distance
    // field or the finished cut rounds the corners but flattens the
    // gully with them.) Stamping exact distances from sample points along
    // the line gives a sub-cell axis, and the profile stays sharp.
    let mut tier2_mask = vec![false; n8];
    let near2: Option<Vec<Nearest>> = if carved.tier2_paths.is_empty() {
        None
    } else {
        const R_M: f64 = 64.0;
        let cell = spec8.cell_size;
        let r_cells = (R_M / cell).ceil() as i64;
        let mut d = vec![f64::INFINITY; n8];
        let mut src = vec![usize::MAX; n8];
        for path in &carved.tier2_paths {
            for seg in path.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                let len = (b.x - a.x).hypot(b.y - a.y).max(1e-9);
                let steps = ((len / (cell * 0.5)).ceil() as usize).max(1);
                for s in 0..=steps {
                    let t = s as f64 / steps as f64;
                    let px = a.x + (b.x - a.x) * t;
                    let py = a.y + (b.y - a.y) * t;
                    let cx = (px / cell).round() as i64;
                    let cy = (py / cell).round() as i64;
                    if cx < 0 || cy < 0 || cx >= nx as i64 || cy >= ny as i64 {
                        continue;
                    }
                    let s_lin = cy as usize * nx + cx as usize;
                    tier2_mask[s_lin] = true;
                    for dy in -r_cells..=r_cells {
                        for dx in -r_cells..=r_cells {
                            let (xx, yy) = (cx + dx, cy + dy);
                            if xx < 0 || yy < 0 || xx >= nx as i64 || yy >= ny as i64 {
                                continue;
                            }
                            let lin = yy as usize * nx + xx as usize;
                            let q = spec8.world_of(xx as u32, yy as u32);
                            let dist = (q.x - px).hypot(q.y - py);
                            if dist < d[lin] {
                                d[lin] = dist;
                                src[lin] = s_lin;
                            }
                        }
                    }
                }
            }
        }
        Some(
            (0..n8)
                .map(|lin| {
                    let s = src[lin];
                    Nearest {
                        dist_m: d[lin],
                        z_channel: if s == usize::MAX { 0.0 } else { carved.z.data[s] },
                        implied_channel: if s == usize::MAX { 0.0 } else { implied.data[s] },
                        order: 1,
                        area_m2: if s == usize::MAX { 0.0 } else { carved.area[s] },
                    }
                })
                .collect(),
        )
    };
    // Tier-2 morphology: the banks profile hung off the smoothed axis, and
    // nothing else. Its depth trench went the same way as the tier-1 one —
    // same construction, same constant-depth-on-a-line objection. Whether
    // the tier earns its keep at all is re-decided against the deeper
    // carve; it ships off.
    if let Some(n2f) = &near2 {
        let cut = catena::banks_small_cut(&spec8, &height8, n2f);
        let cut = catena::smooth_cut(&spec8, cut, TIER2_CUT_BLUR_PASSES);
        for lin in 0..n8 {
            height8.data[lin] -= cut[lin];
        }
        stage_dump("banks2", &height8.data);
    }
    let max_order = channels.iter().map(|c| c.order).max().unwrap_or(0);
    trunk_river::apply(&mut height8, &near, m_trunk_river, max_order);
    stratigraphy::apply(&mut height8, &c1.hardness, &near, &spec.descriptors.strata, m_strat);
    let embryos = closed_basin::apply(&mut height8, &near, &embryo_draws, m_basin, relief_budget);
    aeolian::apply(
        &mut height8,
        &aeolian_draws,
        m_aeolian,
        c1.meta.wind_azimuth_rad,
        relief_budget,
    );
    // Channel cells keep the carved elevation EXACTLY, whatever the
    // modules did around them — monotone descent along the network is
    // structural, and the carve guaranteed it by construction.
    for lin in 0..n8 {
        if carved.channel_of[lin].is_some() {
            height8.data[lin] = carved.z.data[lin].min(height8.data[lin]);
        }
    }
    // ...and then ENFORCE the descent, because the pin above cannot. The
    // carve's own profile is monotone by construction, but the pin takes a
    // MINIMUM against a surface the modules have already edited, and a
    // module that lowers an upstream cell further than its receiver leaves
    // a channel cell draining uphill. The stamped trench used to hide
    // this: it cut hardest where discharge was highest, i.e. downstream,
    // which happened to restore the ordering. With the stamp gone the
    // acceptance test found 10 of 1790 cells, so the invariant gets stated
    // outright instead of relying on a side effect.
    //
    // Sorting by the carved elevation is a valid topological order of the
    // receiver chain (the carve descends), so ONE pass fixes every case,
    // and it only ever lowers — the discipline every other edit here keeps.
    {
        const MIN_DROP_M: f64 = 0.002;
        let mut chan: Vec<usize> =
            (0..n8).filter(|&i| carved.channel_of[i].is_some()).collect();
        chan.sort_by(|&a, &b| carved.z.data[b].total_cmp(&carved.z.data[a]));
        for &i in &chan {
            let Ok(r) = usize::try_from(carved.rec[i]) else { continue };
            if carved.channel_of[r].is_none() {
                continue;
            }
            let need = height8.data[i] - MIN_DROP_M;
            if height8.data[r] > need {
                height8.data[r] = need;
            }
        }
    }

    // The boosted CHANNEL TRENCH used to be stamped here, and it is gone.
    // It existed because carve depth looked capped no matter how hard the
    // stream power was driven, so `incision_boost`'s depth half was faked:
    // a discharge-keyed cut of CONSTANT depth for a given drained area,
    // laid along a D8 axis. Constant depth on a grid-locked line is a boxy
    // ditch, and that is exactly how the review read it.
    //
    // The cap was never base level — it was `iters × step_clamp` (6.75 m)
    // and, above that, an area exponent of 0.5 that let divides erode
    // almost as fast as channels. With `skeleton.cut_ceiling_m` and
    // `skeleton.area_exp` the carve reaches that depth itself (hill
    // country valley p90 3.7 → 4.7 m, crest/valley 1.33 → 1.05 at the
    // shipped setting), so the stamp has nothing left to fake.
    stage_dump("final", &height8.data);

    // ---- flow fields on the built surface ------------------------------
    let ff = course_world::flow::route(&height8);
    let mut flow_dir = Grid::filled(spec8, f64::NAN);
    let mut flow_accum = Grid::filled(spec8, 0.0f64);
    let cell_area = spec8.cell_size * spec8.cell_size;
    for lin in 0..n8 {
        flow_accum.data[lin] = ff.acc[lin] as f64 * cell_area;
        let r = ff.rec[lin];
        if r >= 0 {
            let (y, x) = ((lin / nx) as f64, (lin % nx) as f64);
            let (ry, rx) = ((r as usize / nx) as f64, (r as usize % nx) as f64);
            let dir = libm::atan2(ry - y, rx - x);
            flow_dir.data[lin] = course_contracts::units::normalize_direction(dir);
        }
    }

    // ---- divides (derived), connectivity, normalized coordinates -------
    let mut channel_of = vec![0u32; n8];
    for lin in 0..n8 {
        if let Some(c) = carved.channel_of[lin] {
            channel_of[lin] = c + 1;
        }
    }
    let labels = divides::basin_labels(&ff.rec, &channel_of);
    let dmask = divides::divide_mask(&labels, &channel_of, &spec8);
    let divide_lines = divides::polylines(&dmask, &spec8, 240.0);

    // Connectivity (stage-02 calibration table): fraction of channel
    // LENGTH in a connected component that reaches base level. A component
    // reaches base level iff its root trunk's downstream end sits on the
    // box border (a deranged trunk was cut and dangles in the interior).
    // Connectivity is now measured on the FLOW FIELD rather than on a
    // parent tree: the fraction of channel cells whose receiver chain
    // reaches the box border. A deranged landscape's basins swallow their
    // water, so its channel cells terminate in interior sinks and this
    // falls toward zero — the same separation the old tree-walk gave, but
    // read from the thing that actually routes.
    let connectivity = {
        let mut reaches_edge = vec![false; n8];
        let mut order: Vec<usize> = (0..n8).collect();
        order.sort_by(|&a, &b| carved.area[b].total_cmp(&carved.area[a]));
        for lin in 0..n8 {
            let (y, x) = (lin / (spec8.nx as usize), lin % (spec8.nx as usize));
            let on_border = y == 0
                || x == 0
                || y + 1 == spec8.ny as usize
                || x + 1 == spec8.nx as usize;
            if on_border && carved.rec[lin] < 0 {
                reaches_edge[lin] = true;
            }
        }
        // propagate upstream: a cell reaches the edge iff its receiver does
        let mut stack: Vec<usize> = (0..n8).filter(|&i| reaches_edge[i]).collect();
        let mut donors: Vec<Vec<u32>> = vec![Vec::new(); n8];
        for i in 0..n8 {
            let r = carved.rec[i];
            if r >= 0 {
                donors[r as usize].push(i as u32);
            }
        }
        while let Some(i) = stack.pop() {
            for &d in &donors[i] {
                let d = d as usize;
                if !reaches_edge[d] {
                    reaches_edge[d] = true;
                    stack.push(d);
                }
            }
        }
        let total = carved.channel_of.iter().filter(|c| c.is_some()).count();
        if total == 0 {
            0.0
        } else {
            let conn = (0..n8)
                .filter(|&i| carved.channel_of[i].is_some() && reaches_edge[i])
                .count();
            conn as f64 / total as f64
        }
    };

    // Divide-distance wavefront for the normalized coordinate + hillslope.
    let divide_seeds: Vec<(usize, Nearest)> = dmask
        .iter()
        .enumerate()
        .filter(|&(_, &m)| m)
        .map(|(lin, _)| {
            (
                lin,
                Nearest {
                    dist_m: 0.0,
                    z_channel: height8.data[lin], // divide elevation rides here
                    implied_channel: 0.0,
                    order: 0,
                    area_m2: 0.0,
                },
            )
        })
        .collect();
    let div_near = flow_distance::dijkstra(&spec8, &divide_seeds);

    let mut fdn = Grid::filled(spec8, 1.0f64);
    let mut hp = Grid::filled(spec8, 0.5f64);
    if !has_channels {
        // Degenerate case: all hillslope. flow_distance_norm = 1 everywhere;
        // hillslope_position = normalized relative elevation.
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in &height8.data {
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let span = (hi - lo).max(1e-9);
        for lin in 0..n8 {
            hp.data[lin] = (height8.data[lin] - lo) / span;
        }
    } else {
        for lin in 0..n8 {
            let d = near[lin].dist_m;
            let dd = div_near[lin].dist_m;
            fdn.data[lin] = if d + dd > 0.0 { d / (d + dd) } else { 0.0 };
            let z_ch = near[lin].z_channel;
            let z_div = div_near[lin].z_channel;
            let denom = (z_div - z_ch).max(0.5);
            hp.data[lin] = ((height8.data[lin] - z_ch) / denom).clamp(0.0, 1.0);
        }
    }

    // ---- strip the rim from the PRESENTED height -----------------------
    // The walls stayed on for every routing/divide/connectivity
    // derivation above (stripping them before flow::route opened all
    // borders and the trunk walk died at the nearest edge — rv "no
    // river"); only the surface handed to S3 loses them. See
    // Carved::pre_rim.
    for &(lin, zv) in &carved.pre_rim {
        height8.data[lin] = zv;
    }

    // REPOSE ANGLE, and it runs HERE — after the rim strip — for a reason.
    // Applied inside the carve loop it measured almost nothing (p99 50.0 ->
    // 47.1 deg), because the catena, the modules and the channel pin all
    // re-cut afterwards. Applied before the rim strip it missed the worst
    // faces in the tile: the border row is restored to its pre-rim height
    // while the row inboard of it has been carved, catena'd and trenched,
    // so the frame leaves a step exactly where the TRUNK crosses it — the
    // trunk is the outlet reach, so the tile's widest channel is always in
    // that band. Measured: trunk-bank p99 39.2 deg over the whole tile
    // against 30.7 with the frame band excluded, and 20.6 for interior
    // wide channels (the corpus reads 20.4-24.3). Running last, over every
    // cell including the border, caps the step from whichever side is
    // high. Only lowers, so the enforced descent still holds.
    //
    // The cap is DISCHARGE-KEYED. A hillslope can stand at its repose
    // angle; the bank of a wide channel cannot, and the corpus says so —
    // real trunk banks read p99 20.4 deg while ours read 30.8-39.2, and
    // our interior WIDE channels are already gentler than real (12.6-25.9
    // against 24.3-48.3). So the defect is specific: big channels want a
    // lower ceiling over a wider skirt, small ones do not.
    {
        let base = dial("skeleton.bank_angle_deg", 0.0);
        let trunk = dial("skeleton.trunk_bank_angle_deg", base);
        let cap: Vec<f64> = (0..n8)
            .map(|i| {
                let nr = &near[i];
                if base <= 0.0 || !nr.dist_m.is_finite() {
                    return base;
                }
                let km2 = (nr.area_m2 / 1.0e6).max(0.0);
                // how much of a "big channel" this is: nothing under
                // 0.3 km², fully by 4 km²
                let u = ((km2 - 0.3) / 3.7).clamp(0.0, 1.0);
                let big = u * u * (3.0 - 2.0 * u);
                // ...and how close, over a skirt that widens with it
                let skirt = 90.0 + 210.0 * big;
                let v = (1.0 - nr.dist_m / skirt).clamp(0.0, 1.0);
                let near_w = v * v * (3.0 - 2.0 * v);
                base + (trunk - base) * big * near_w
            })
            .collect();
        carve::talus(&mut height8, &cap);
    }

    // ---- 2 m presentation ---------------------------------------------
    // CATMULL-ROM, not bilinear. Bilinear is only C0: its gradient jumps at
    // every 8 m node, so the presented surface carries a curvature spike on
    // every grid line — measured 749x the local level at the first line out
    // from a channel, against 1.15x (no spike at all) on corpus tiles, and
    // decaying copies at 16, 24, 32 m. That is the "sharp and uniform edges"
    // read: the seams are hardest where the surface bends most, which is
    // exactly the banks. S3's mid band hit the same wall and took the same
    // fix (`blend::upsample_catmull`, "bilinear left piecewise-linear
    // facets ... a pixelated cross-hatch"); this is the base surface's turn.
    let spec2 = world_spec(RES_FULL_M);
    let mut height2 = Grid::filled(spec2, 0.0f64);
    {
        let cr = |t: f64| -> [f64; 4] {
            let (t2, t3) = (t * t, t * t * t);
            [
                -0.5 * t3 + t2 - 0.5 * t,
                1.5 * t3 - 2.5 * t2 + 1.0,
                -1.5 * t3 + 2.0 * t2 + 0.5 * t,
                0.5 * t3 - 0.5 * t2,
            ]
        };
        let taps = |f: f64, n: usize| -> ([usize; 4], [f64; 4]) {
            let f = f.clamp(0.0, (n - 1) as f64);
            let i1 = f.floor() as usize;
            (
                [
                    i1.saturating_sub(1),
                    i1,
                    (i1 + 1).min(n - 1),
                    (i1 + 2).min(n - 1),
                ],
                cr(f - i1 as f64),
            )
        };
        let (n2x, n2y) = (spec2.nx as usize, spec2.ny as usize);
        // separable: horizontal into a scratch row set, then vertical
        let xmap: Vec<([usize; 4], [f64; 4])> = (0..n2x)
            .map(|x| {
                let p = spec2.world_of(x as u32, 0);
                taps((p.x - spec8.origin.x) / spec8.cell_size, nx)
            })
            .collect();
        let mut tmp = vec![0.0f64; ny * n2x];
        for y in 0..ny {
            let row = &height8.data[y * nx..(y + 1) * nx];
            for (x, (idx, w)) in xmap.iter().enumerate() {
                tmp[y * n2x + x] = row[idx[0]] * w[0]
                    + row[idx[1]] * w[1]
                    + row[idx[2]] * w[2]
                    + row[idx[3]] * w[3];
            }
        }
        for y in 0..n2y {
            let p = spec2.world_of(0, y as u32);
            let (idx, w) = taps((p.y - spec8.origin.y) / spec8.cell_size, ny);
            for x in 0..n2x {
                height2.data[y * n2x + x] = tmp[idx[0] * n2x + x] * w[0]
                    + tmp[idx[1] * n2x + x] * w[1]
                    + tmp[idx[2] * n2x + x] * w[2]
                    + tmp[idx[3] * n2x + x] * w[3];
            }
        }
    }

    // ---- flow distance (metres) + diagnostics --------------------------
    let mut flow_distance_m = Grid::filled(spec8, 0.0f64);
    for lin in 0..n8 {
        flow_distance_m.data[lin] = near[lin].dist_m;
    }
    let total_len_m: f64 = channels.iter().map(|c| carve::arc_len(&c.pts)).sum();
    let (rb, rl) = carve::horton_ratios(&channels);
    let diagnostics = SkeletonDiagnostics {
        target_density_km_km2: density,
        achieved_density_km_km2: total_len_m / 1000.0 / 9.0,
        channel_count: channels.len(),
        bifurcation_ratio: rb,
        length_ratio: rl,
        connectivity,
    };

    let trunk_inlet = if m_trunk_river > 0.0 { Some(carved.inlet) } else { None };
    Skeleton {
        skeleton_version: SKELETON_VERSION,
        kernel: KernelId::Fluvial,
        height: height2,
        flow_dir_rad: flow_dir,
        flow_accum,
        flow_distance: flow_distance_m,
        flow_distance_norm: fdn,
        hillslope_position: hp,
        channels,
        embryos,
        divides: divide_lines,
        trunk_inlet,
        tier2: near2.is_some().then(|| {
            let mut g = Grid::filled(spec8, 0u8);
            for lin in 0..n8 {
                g.data[lin] = tier2_mask[lin] as u8;
            }
            g
        }),
        meta: c1.meta.clone(),
        diagnostics,
    }
}
