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
        roughness_frac: carve::ROUGHNESS_FRAC,
        k: if density >= 0.5 { carve::K_STREAM_POWER } else { 0.0 },
        area_threshold_m2: carve::AREA_THRESHOLD_M2,
        incision_scale: integration::incision_scale(m_integration)
            * dial("skeleton.incision_boost", 1.0),
        base_drop_m: carve::BASE_DROP_M,
        inflow_area_m2: carve::INFLOW_PER_TRUNK_DIAL_M2 * m_trunk_river,
        close_borders: true,
        derangement: ((0.5 - m_integration) * 1.6).clamp(0.0, 0.9),
        iters: carve::ITERS,
        // the per-iteration clamp caps total incision at 6.75 m and big
        // channels saturate it — without scaling, incision_boost only
        // WIDENS (the ladder metric showed p90 depth flat-to-falling)
        step_clamp_m: carve::STEP_CLAMP_M * dial("skeleton.incision_boost", 1.0).sqrt(),
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
    // The carved surface already holds the valleys, so the channel cells
    // seed the wavefront directly; no polyline rasterization step.
    let base_elev = c1.meta.base_level.elev_m;
    let seeds: Vec<(usize, Nearest)> = {
        let v: Vec<(usize, Nearest)> = (0..n8)
            .filter(|&lin| carved.channel_of[lin].is_some())
            .map(|lin| {
                (
                    lin,
                    Nearest {
                        dist_m: 0.0,
                        z_channel: carved.z.data[lin],
                        implied_channel: implied.data[lin],
                        order: carved.order_at[lin].max(1),
                        area_m2: carved.area[lin],
                    },
                )
            })
            .collect();
        if v.is_empty() {
            flow_distance::edge_seeds(&spec8, c1.meta.base_level.edge, base_elev, &implied)
        } else {
            v
        }
    };
    let near = flow_distance::dijkstra(&spec8, &seeds);
    let has_channels = carved.channel_of.iter().any(|c| c.is_some());

    // ---- bank profile + modules ----------------------------------------
    // The carve did the incising, so the catena only shapes BANKS here:
    // order-scaled floors and the groove/hillslope blend, applied to the
    // carved surface rather than cutting a second set of valleys into the
    // implied one.
    let mut height8 =
        catena::banks(&spec8, &carved.z, &near, dial("skeleton.incision_boost", 1.0));
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

    // BOOSTED CHANNEL TRENCH. Absolute carve depth saturates against
    // base level (downstream-first carving converges on the outlet
    // profile — boosting k just reaches the same asymptote faster; the
    // incision ladder measured p90 depth FLAT across boost 1.0→2.5),
    // so incision_boost's DEPTH half comes from a local trench cut
    // below the graded catena surface: the incised-draw / arroyo form.
    // Discharge-keyed width and depth; identically zero at boost 1.0.
    {
        let boost = dial("skeleton.incision_boost", 1.0);
        if boost > 1.0 {
            for lin in 0..n8 {
                let nr = &near[lin];
                if !nr.dist_m.is_finite() {
                    continue;
                }
                let km2 = (nr.area_m2 / 1.0e6).max(0.0);
                let hw = (11.0 * libm::pow(km2.max(1e-6), 0.4)).clamp(9.0, 42.0);
                if nr.dist_m > hw {
                    continue;
                }
                // discharge TAPER, not a floor: the 0.5 factor floor
                // stamped every order-1 swale head with a ~20 m slot —
                // "cracked" piedmont tiles (a P2 tell). Zero at the
                // extraction threshold, full by ~0.5 km2.
                let u = ((nr.area_m2 - 1.5e5) / 3.5e5).clamp(0.0, 1.0);
                let taper = u * u * (3.0 - 2.0 * u);
                let t = 2.2
                    * (boost - 1.0)
                    * libm::pow(km2.max(0.05), 0.3).clamp(0.5, 2.0)
                    * taper;
                let u = nr.dist_m / hw;
                height8.data[lin] -= t * (1.0 - u * u).max(0.0);
            }
        }
    }

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

    // ---- 2 m presentation ---------------------------------------------
    let spec2 = world_spec(RES_FULL_M);
    let mut height2 = Grid::filled(spec2, 0.0f64);
    for y in 0..spec2.ny {
        for x in 0..spec2.nx {
            let p = spec2.world_of(x, y);
            height2.set(x, y, height8.bilinear(p));
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
        meta: c1.meta.clone(),
        diagnostics,
    }
}
