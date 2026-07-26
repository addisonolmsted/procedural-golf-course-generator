//! Deterministic, cheap stand-ins for every stage output.
//!
//! Purpose: (1) unblock downstream stage development before upstream stages
//! exist — build against `fixture_*` outputs, swap to the real crate later
//! with zero contract change; (2) give course-lab something to render for
//! every tab from day one; (3) anchor the course-fixtures golden hash.
//!
//! Fixtures are contract-faithful (monotone drain spines, empty hydro graph
//! under `Infiltrated`, feathered earthworks patches, …) but deliberately
//! crude as terrain. Real stages must NOT call into this module; the only RNG
//! stream used is [`crate::rng::CH_FIXTURE`].

use std::collections::BTreeMap;

use golf_core::{Grid, GridSpec, Vec2};

use crate::archetype::{ArchetypeId, HydrologyMode};
use crate::rng::{channel, CH_FIXTURE};
use crate::spec::{ArchetypePriors, CourseSpec, ParamPack};
use crate::stages::*;
use crate::{world_spec, CORE_MAX_M, CORE_MIN_M, EXTENT_M, PIPELINE_VERSION, RES_EARTHWORKS_M};

// ---------------------------------------------------------------------------
// Small deterministic helpers (fixture-local; real stages bring their own)
// ---------------------------------------------------------------------------

fn splitmix(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9e3779b97f4a7c15);
    x = (x ^ (x >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    x = (x ^ (x >> 27)).wrapping_mul(0x94d049bb133111eb);
    x ^ (x >> 31)
}

/// Lattice hash → [-1, 1].
fn lattice(seed: u64, salt: u64, ix: i64, iy: i64) -> f64 {
    let h = splitmix(seed ^ salt.wrapping_mul(0xd6e8feb86659fd93) ^ ((ix as u64) << 32) ^ (iy as u64 & 0xffff_ffff));
    (h >> 11) as f64 / ((1u64 << 53) as f64) * 2.0 - 1.0
}

fn smooth(t: f64) -> f64 {
    t * t * (3.0 - 2.0 * t)
}

/// Bilinear-smoothstep value noise at wavelength `wl`.
fn value_noise(seed: u64, salt: u64, p: Vec2, wl: f64) -> f64 {
    let x = p.x / wl;
    let y = p.y / wl;
    let (ix, iy) = (x.floor() as i64, y.floor() as i64);
    let (fx, fy) = (smooth(x - ix as f64), smooth(y - iy as f64));
    let v00 = lattice(seed, salt, ix, iy);
    let v10 = lattice(seed, salt, ix + 1, iy);
    let v01 = lattice(seed, salt, ix, iy + 1);
    let v11 = lattice(seed, salt, ix + 1, iy + 1);
    v00 + (v10 - v00) * fx + (v01 - v00) * fy + (v11 + v00 - v10 - v01) * fx * fy
}

fn fbm(seed: u64, salt: u64, p: Vec2, wl: f64, octaves: u32) -> f64 {
    let mut amp = 1.0;
    let mut w = wl;
    let mut sum = 0.0;
    let mut norm = 0.0;
    for o in 0..octaves {
        sum += amp * value_noise(seed, salt.wrapping_add(o as u64), p, w);
        norm += amp;
        amp *= 0.5;
        w *= 0.5;
    }
    sum / norm
}

fn smoothstep(lo: f64, hi: f64, x: f64) -> f64 {
    smooth(((x - lo) / (hi - lo)).clamp(0.0, 1.0))
}

/// Distance from `p` to a polyline + the tangent angle of the nearest segment.
fn polyline_dist(p: Vec2, pts: &[Vec2]) -> (f64, f64) {
    let mut best = f64::INFINITY;
    let mut ang = 0.0;
    for w in pts.windows(2) {
        let (a, b) = (w[0], w[1]);
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let len2 = dx * dx + dy * dy;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * dx + (p.y - a.y) * dy) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (cx, cy) = (a.x + t * dx, a.y + t * dy);
        let d2 = (p.x - cx) * (p.x - cx) + (p.y - cy) * (p.y - cy);
        if d2 < best {
            best = d2;
            ang = dy.atan2(dx);
        }
    }
    (best.sqrt(), ang)
}

// ---------------------------------------------------------------------------
// Stage 1–2
// ---------------------------------------------------------------------------

/// Stage 1–2 stand-in: archetype forced by the caller, every knob at the
/// prior median (real course-seed samples quantiles).
pub fn fixture_spec(seed: u64, archetype: ArchetypeId) -> CourseSpec {
    let priors = ArchetypePriors::builtin();
    CourseSpec {
        pipeline_version: PIPELINE_VERSION,
        seed,
        archetype,
        hydrology_mode: archetype.hydrology_mode(),
        params: ParamPack::from_prior_median(&priors, archetype),
    }
}

// ---------------------------------------------------------------------------
// Stage 3
// ---------------------------------------------------------------------------

/// One drain spine west→east through the core plus a ridge band, closed
/// basins, and (mountain) bench terracing — enough structure for every
/// downstream fixture to condition on.
pub fn fixture_skeleton(spec: &CourseSpec, res_m: f64) -> MacroSkeleton {
    let lf = &spec.params.landform;
    let gs = world_spec(res_m);
    let mut rng = channel(spec.seed, CH_FIXTURE);
    let phase = rng.range_f64(0.0, std::f64::consts::TAU);
    let ridge_y = rng.range_f64(2200.0, 2600.0);

    // Drain spine: gentle sine across the box, exiting east.
    let spine: Vec<Vec2> = (0..=60)
        .map(|i| {
            let x = EXTENT_M * i as f64 / 60.0;
            let y = 1500.0 + 350.0 * (x / EXTENT_M * std::f64::consts::TAU * 0.7 + phase).sin();
            Vec2 { x, y }
        })
        .collect();

    // Closed basins (kettles / future lakes), kept out of the exact center.
    let n_basins = lf.basin_count.round() as usize;
    let basins: Vec<(Vec2, f64, f64)> = (0..n_basins)
        .map(|_| {
            let c = Vec2 { x: rng.range_f64(300.0, 2700.0), y: rng.range_f64(300.0, 2700.0) };
            let r = rng.range_f64(60.0, 150.0);
            let depth = rng.range_f64(1.5, 4.0);
            (c, r, depth)
        })
        .collect();

    let relief = lf.relief_amp_m;
    let core_scale = (lf.core_relief_cap_m / relief).min(1.0);
    let valley_depth = (relief * 0.35).min(25.0);
    let bench_step = if lf.bench_count > 0.5 { relief * 0.25 } else { 0.0 };

    let mut base = Grid::filled(gs, 0.0);
    let mut floor_damp = Grid::filled(gs, 0.0);
    let mut slope_gain = Grid::filled(gs, 0.0);
    let mut grain = Grid::filled(gs, 0.0);
    let mut vdist = Grid::filled(gs, 0.0);
    let mut core = Grid::filled(gs, 0.0);

    for y in 0..gs.ny {
        for x in 0..gs.nx {
            let p = gs.world_of(x, y);
            let (d, tangent) = polyline_dist(p, &spine);

            // Fade relief-carrying features toward the core cap.
            let core_f = smoothstep(CORE_MIN_M - 200.0, CORE_MIN_M + 100.0, p.x)
                * smoothstep(CORE_MIN_M - 200.0, CORE_MIN_M + 100.0, p.y)
                * smoothstep(-(CORE_MAX_M + 200.0), -(CORE_MAX_M - 100.0), -p.x)
                * smoothstep(-(CORE_MAX_M + 200.0), -(CORE_MAX_M - 100.0), -p.y);
            let feat = 1.0 + (core_scale - 1.0) * core_f;

            // Tilt falls east so the spine (west→east) has a monotone floor.
            let mut z = lf.tilt_grade * (EXTENT_M - p.x);
            // Ridge band north of the corridor.
            let ry = (p.y - ridge_y) / 600.0;
            z += relief * 0.6 * (-ry * ry).exp() * feat;
            // Bench terracing (mountain): quantize the tilt+ridge surface.
            if bench_step > 0.0 {
                z = (z / bench_step).floor() * bench_step + (z / bench_step).fract() * bench_step * 0.25;
            }
            // Valley carve.
            let carve = valley_depth * (-(d / 140.0) * (d / 140.0)).exp() * feat;
            z -= carve;
            // Closed basins.
            for (c, r, depth) in &basins {
                let dd = ((p.x - c.x) * (p.x - c.x) + (p.y - c.y) * (p.y - c.y)).sqrt();
                z -= depth * smoothstep(-*r, -r * 0.3, -dd) * feat;
            }

            base.set(x, y, z);
            floor_damp.set(x, y, smoothstep(-160.0, -40.0, -d));
            slope_gain.set(x, y, (relief * 0.6 * (-ry * ry).exp() / relief.max(1.0)).clamp(0.0, 1.0));
            grain.set(x, y, tangent);
            vdist.set(x, y, d);
            core.set(x, y, core_f);
        }
    }

    // Monotone spine elevations for the structure graph (sampled from base).
    let structure = StructureGraph {
        spines: vec![Spine { kind: SpineKind::Drain, pts: spine }],
    };

    MacroSkeleton {
        base_height: base,
        fields: SkeletonFields {
            floor_damp,
            slope_gain,
            grain_dir_rad: grain,
            valley_dist_m: vdist,
            core_protect: core,
        },
        structure,
    }
}

// ---------------------------------------------------------------------------
// Stage 4
// ---------------------------------------------------------------------------

/// Skeleton-conditioned fBm: damped on floors, boosted on structural slopes,
/// anisotropic along the grain (dune trains fall out of high aniso + long
/// wavelength in the sandhills prior).
pub fn fixture_height(spec: &CourseSpec, skel: &MacroSkeleton) -> ComposedHeight {
    let np = &spec.params.noise;
    let gs = skel.base_height.spec;
    let mut height = skel.base_height.clone();
    let octaves = (np.octaves.round() as u32).clamp(1, 8);
    for y in 0..gs.ny {
        for x in 0..gs.nx {
            let p = gs.world_of(x, y);
            let g = *skel.fields.grain_dir_rad.get(x, y) * np.grain_align;
            // Rotate into the grain frame and stretch across-grain.
            let (s, c) = (g.sin(), g.cos());
            let q = Vec2 {
                x: (p.x * c + p.y * s),
                y: (-p.x * s + p.y * c) * np.aniso_ratio,
            };
            let n = fbm(spec.seed, 0x4e01, q, np.base_wavelength_m, octaves);
            let n = (1.0 - np.ridged_mix) * n + np.ridged_mix * (1.0 - 2.0 * n.abs());
            let damp = 1.0 - np.floor_damp * skel.fields.floor_damp.get(x, y);
            let gain = 1.0 + np.slope_gain * skel.fields.slope_gain.get(x, y);
            height.set(x, y, height.get(x, y) + np.base_amp_m * n * damp * gain);
        }
    }
    ComposedHeight { height }
}

// ---------------------------------------------------------------------------
// Stage 5
// ---------------------------------------------------------------------------

/// Fixed D8 neighbor scan order (dx, dy, distance factor) — documented
/// tie-break: first-listed wins.
const NBR: [(i64, i64, f64); 8] = [
    (1, 0, 1.0),
    (-1, 0, 1.0),
    (0, 1, 1.0),
    (0, -1, 1.0),
    (1, 1, std::f64::consts::SQRT_2),
    (1, -1, std::f64::consts::SQRT_2),
    (-1, 1, std::f64::consts::SQRT_2),
    (-1, -1, std::f64::consts::SQRT_2),
];

/// Naive D8 + runoff-scaled accumulation + threshold streams + water-table
/// lakes. No erosion (height passes through), no depression filling (zfill =
/// height) — contract-shaped, not geomorphology.
pub fn fixture_hydro(spec: &CourseSpec, composed: &ComposedHeight) -> HydroBundle {
    let hp = &spec.params.hydro;
    let gs = composed.height.spec;
    let (nx, ny) = (gs.nx as i64, gs.ny as i64);
    let h = &composed.height;
    let n = gs.len();

    // Receivers.
    let mut rcv = vec![0u32; n];
    for y in 0..ny {
        for x in 0..nx {
            let i = (y * nx + x) as usize;
            let zi = h.data[i];
            let mut best = i;
            let mut best_drop = 0.0;
            if x > 0 && x < nx - 1 && y > 0 && y < ny - 1 {
                for (dx, dy, dist) in NBR {
                    let j = ((y + dy) * nx + (x + dx)) as usize;
                    let drop = (zi - h.data[j]) / dist;
                    if drop > best_drop {
                        best_drop = drop;
                        best = j;
                    }
                }
            }
            rcv[i] = best as u32;
        }
    }

    // Top-down order: height desc, index asc (total, stable, deterministic).
    let mut order: Vec<u32> = (0..n as u32).collect();
    order.sort_by(|&a, &b| {
        h.data[b as usize]
            .total_cmp(&h.data[a as usize])
            .then(a.cmp(&b))
    });

    // Runoff-scaled accumulation: sandhills (infiltration→1) accumulate ~0.
    let runoff = (hp.rainfall * (1.0 - hp.infiltration)).max(0.0);
    let cell_area = gs.cell_size * gs.cell_size;
    let mut acc = vec![cell_area * runoff; n];
    for &i in &order {
        let i = i as usize;
        let r = rcv[i] as usize;
        if r != i {
            acc[r] += acc[i];
        }
    }
    let flow_area_m2 = Grid::from_data(gs, acc.clone());

    // Streams: threshold trace, heads first.
    let thresh = hp.channel_threshold_ha * 10_000.0;
    let in_set: Vec<bool> = acc.iter().map(|a| *a >= thresh).collect();
    let mut donors = vec![0u32; n];
    for i in 0..n {
        let r = rcv[i] as usize;
        if in_set[i] && r != i && in_set[r] {
            donors[r] += 1;
        }
    }
    let mut visited = vec![false; n];
    let mut streams = Vec::new();
    for i in 0..n {
        if !in_set[i] || donors[i] > 0 || visited[i] {
            continue;
        }
        let mut pts = Vec::new();
        let mut cur = i;
        let mut area = acc[i];
        loop {
            visited[cur] = true;
            area = area.max(acc[cur]);
            pts.push(gs.world_of((cur % nx as usize) as u32, (cur / nx as usize) as u32));
            let r = rcv[cur] as usize;
            if r == cur || !in_set[r] || visited[r] {
                break;
            }
            cur = r;
        }
        if pts.len() >= 3 {
            let width_m = (0.4 * (area / 10_000.0).sqrt()).clamp(0.5, 12.0);
            streams.push(Stream { pts, strahler: 1, width_m });
        }
    }

    // Water-table lakes.
    let mut lakes = Vec::new();
    let mut wetlands = Vec::new();
    if spec.hydrology_mode == HydrologyMode::WaterTable {
        let mut sorted = h.data.clone();
        sorted.sort_by(f64::total_cmp);
        // Table pinned to the 20th height percentile so lakes read as basins,
        // not a half-flooded map (fixture-grade calibration).
        let table = sorted[n / 5] - hp.water_table_depth_m;
        let wet: Vec<bool> = h.data.iter().map(|z| *z < table).collect();
        let mut seen = vec![false; n];
        for i in 0..n {
            if !wet[i] || seen[i] {
                continue;
            }
            // BFS component.
            let mut comp = vec![i];
            let mut qh = 0;
            seen[i] = true;
            while qh < comp.len() {
                let c = comp[qh];
                qh += 1;
                let (cx, cy) = ((c % nx as usize) as i64, (c / nx as usize) as i64);
                for (dx, dy, _) in NBR[..4].iter() {
                    let (px, py) = (cx + dx, cy + dy);
                    if px >= 0 && px < nx && py >= 0 && py < ny {
                        let j = (py * nx + px) as usize;
                        if wet[j] && !seen[j] {
                            seen[j] = true;
                            comp.push(j);
                        }
                    }
                }
            }
            if comp.len() < 12 {
                continue;
            }
            // Fixture-grade outline: boundary cells ordered by angle.
            let cx = comp.iter().map(|c| gs.world_of((c % nx as usize) as u32, (c / nx as usize) as u32).x).sum::<f64>() / comp.len() as f64;
            let cy = comp.iter().map(|c| gs.world_of((c % nx as usize) as u32, (c / nx as usize) as u32).y).sum::<f64>() / comp.len() as f64;
            let mut boundary: Vec<Vec2> = comp
                .iter()
                .map(|&c| gs.world_of((c % nx as usize) as u32, (c / nx as usize) as u32))
                .filter(|p| {
                    // keep ~ring: cells far from centroid relative to comp size
                    true && ((p.x - cx).abs() + (p.y - cy).abs()) > 0.0
                })
                .collect();
            boundary.sort_by(|a, b| {
                (a.y - cy)
                    .atan2(a.x - cx)
                    .total_cmp(&(b.y - cy).atan2(b.x - cx))
            });
            let step = (boundary.len() / 40).max(1);
            let outline: Vec<Vec2> = boundary.into_iter().step_by(step).collect();
            lakes.push(Lake { outline, surface_elev_m: table });
        }
        // One wetland fringe per lake (fixture-grade: scaled outline).
        for lake in &lakes {
            let cx = lake.outline.iter().map(|p| p.x).sum::<f64>() / lake.outline.len() as f64;
            let cy = lake.outline.iter().map(|p| p.y).sum::<f64>() / lake.outline.len() as f64;
            wetlands.push(Wetland {
                outline: lake
                    .outline
                    .iter()
                    .map(|p| Vec2 { x: cx + (p.x - cx) * 1.25, y: cy + (p.y - cy) * 1.25 })
                    .collect(),
            });
        }
    }

    HydroBundle {
        height: composed.height.clone(),
        flow: FlowModel { rcv, order, zfill: composed.height.clone(), flow_area_m2 },
        graph: HydroGraph { streams, lakes, wetlands },
    }
}

// ---------------------------------------------------------------------------
// Stage 6
// ---------------------------------------------------------------------------

pub fn fixture_cover(spec: &CourseSpec, hydro: &HydroBundle) -> CoverMap {
    let cp = &spec.params.cover;
    let gs = hydro.height.spec;
    let slope = slope_grid(&hydro.height);
    let mut class = Grid::filled(gs, CoverClass::Rough as u8);
    let mut canopy = Grid::filled(gs, 0.0);

    // Rasterize water: lakes by point-in-radius of outline centroid (fixture),
    // streams by stamping along polylines.
    let mut water = vec![false; gs.len()];
    for lake in &hydro.graph.lakes {
        // Water-table lakes: exact predicate is height < surface.
        for i in 0..gs.len() {
            if hydro.height.data[i] < lake.surface_elev_m {
                water[i] = true;
            }
        }
    }
    for s in &hydro.graph.streams {
        let r = (s.width_m / gs.cell_size).ceil() as i64;
        for p in &s.pts {
            let cx = ((p.x - gs.origin.x) / gs.cell_size).round() as i64;
            let cy = ((p.y - gs.origin.y) / gs.cell_size).round() as i64;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (cx + dx, cy + dy);
                    if x >= 0 && x < gs.nx as i64 && y >= 0 && y < gs.ny as i64 {
                        water[(y * gs.nx as i64 + x) as usize] = true;
                    }
                }
            }
        }
    }

    for y in 0..gs.ny {
        for x in 0..gs.nx {
            let i = (y * gs.nx + x) as usize;
            let p = gs.world_of(x, y);
            let sl = slope.data[i];
            let clump = value_noise(spec.seed, 0x6c01, p, cp.clump_scale_m) * 0.5 + 0.5;
            let clump2 = value_noise(spec.seed, 0x6c02, p, cp.clump_scale_m * 1.7) * 0.5 + 0.5;
            let c = if water[i] {
                CoverClass::Water
            } else if sl < 0.02
                && hydro.graph.lakes.iter().any(|l| hydro.height.data[i] < l.surface_elev_m + 0.6)
                && cp.wetland_bias > 0.2
            {
                CoverClass::Wetland
            } else if sl > 0.45 {
                CoverClass::Rock
            } else if clump < cp.sand_exposure {
                CoverClass::Sand
            } else if clump2 < cp.canopy_frac * if crate::in_core(p) { 0.55 } else { 1.0 } {
                // canopy thinned inside the routable core — the cover-side
                // half of the keep-the-core-routable rule
                CoverClass::Forest
            } else if sl < 0.06 {
                CoverClass::Turf
            } else {
                CoverClass::Rough
            };
            class.set(x, y, c as u8);
            let cn = if c == CoverClass::Forest {
                0.7 + 0.3 * clump
            } else {
                (cp.canopy_frac - clump2).max(0.0)
            };
            canopy.set(x, y, cn.clamp(0.0, 1.0));
        }
    }
    CoverMap { class, canopy }
}

// ---------------------------------------------------------------------------
// Stage 7
// ---------------------------------------------------------------------------

/// Real metrics, fixture terrain: slope CDF, largest routable patch, water
/// blockage, stream crossings — evaluated on the core window only.
pub fn fixture_gate(spec: &CourseSpec, hydro: &HydroBundle, cover: &CoverMap) -> GateReport {
    let gp = &spec.params.gate;
    let gs = hydro.height.spec;
    let slope = slope_grid(&hydro.height);
    let cell_ha = gs.cell_size * gs.cell_size / 10_000.0;

    let mut core_cells = 0u64;
    let mut lt8 = 0u64;
    let mut blocked = 0u64;
    let mut routable = vec![false; gs.len()];
    for y in 0..gs.ny {
        for x in 0..gs.nx {
            let p = gs.world_of(x, y);
            if !crate::in_core(p) {
                continue;
            }
            let i = (y * gs.nx + x) as usize;
            core_cells += 1;
            if slope.data[i] < 0.08 {
                lt8 += 1;
            }
            let c = CoverClass::from_u8(cover.class.data[i]).unwrap_or(CoverClass::Rough);
            if matches!(c, CoverClass::Water | CoverClass::Wetland) {
                blocked += 1;
            }
            routable[i] = c.routable() && slope.data[i] < 0.15;
        }
    }

    // Largest 4-connected routable component (core only).
    let mut seen = vec![false; gs.len()];
    let mut largest = 0u64;
    for start in 0..gs.len() {
        if !routable[start] || seen[start] {
            continue;
        }
        let mut comp = 0u64;
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(c) = stack.pop() {
            comp += 1;
            let (cx, cy) = ((c % gs.nx as usize) as i64, (c / gs.nx as usize) as i64);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] {
                let (px, py) = (cx + dx, cy + dy);
                if px >= 0 && px < gs.nx as i64 && py >= 0 && py < gs.ny as i64 {
                    let j = (py * gs.nx as i64 + px) as usize;
                    if routable[j] && !seen[j] {
                        seen[j] = true;
                        stack.push(j);
                    }
                }
            }
        }
        largest = largest.max(comp);
    }

    // Stream crossings of the core boundary as the crossing-pressure proxy.
    let crossings = hydro
        .graph
        .streams
        .iter()
        .filter(|s| s.pts.iter().any(|p| crate::in_core(*p)))
        .count() as f64;

    let slope_lt8 = lt8 as f64 / core_cells.max(1) as f64;
    let patch_ha = largest as f64 * cell_ha;
    let blockage = blocked as f64 / core_cells.max(1) as f64;
    // 9 corridors ≈ (2×480 + 5×370 + 2×160) m × 60 m ≈ 21 ha of routable need.
    let corridor_need_ha = 21.0;

    let mut metrics = BTreeMap::new();
    metrics.insert("core_slope_lt8_frac".into(), slope_lt8);
    metrics.insert("core_routable_patch_ha".into(), patch_ha);
    metrics.insert("core_water_blockage_frac".into(), blockage);
    metrics.insert("core_stream_crossings".into(), crossings);
    metrics.insert("corridor_need_ha".into(), corridor_need_ha);

    let mut reasons = Vec::new();
    if slope_lt8 < gp.slope_lt8_min {
        reasons.push(format!("slope_lt8 {slope_lt8:.2} < min {:.2}", gp.slope_lt8_min));
    }
    if patch_ha < gp.routable_patch_ha_min {
        reasons.push(format!("routable patch {patch_ha:.1} ha < min {:.1}", gp.routable_patch_ha_min));
    }
    if blockage > gp.water_blockage_max {
        reasons.push(format!("water blockage {blockage:.2} > max {:.2}", gp.water_blockage_max));
    }
    if crossings > gp.crossings_max {
        reasons.push(format!("crossings {crossings} > max {}", gp.crossings_max));
    }
    if patch_ha < corridor_need_ha {
        reasons.push(format!("patch {patch_ha:.1} ha < corridor need {corridor_need_ha} ha"));
    }

    GateReport { pass: reasons.is_empty(), metrics, reasons }
}

// ---------------------------------------------------------------------------
// Stage 8
// ---------------------------------------------------------------------------

/// Purely geometric returning nine: holes laid out as chords of a loop around
/// the core center. Contract-faithful shape; ignores terrain entirely.
pub fn fixture_routing(spec: &CourseSpec) -> RoutingPlan {
    let pars = spec.params.route.par_sequence.clone();
    let len = |par: u32| match par {
        3 => 155.0,
        5 => 470.0,
        _ => 360.0,
    };
    let total: f64 = pars.iter().map(|p| len(*p) + 30.0).sum();
    let r = total / std::f64::consts::TAU;
    let center = Vec2 { x: 1500.0, y: 1500.0 };
    let mut theta = -std::f64::consts::FRAC_PI_2; // start due south of center
    let at = |theta: f64| Vec2 { x: center.x + r * theta.cos(), y: center.y + r * theta.sin() };

    let mut holes = Vec::new();
    let clubhouse = at(theta);
    for par in &pars {
        let l = len(*par);
        let tee = at(theta);
        let theta_g = theta + l / r;
        let green = at(theta_g);
        // Corridor: rectangle around the chord, 55 m wide.
        let (dx, dy) = (green.x - tee.x, green.y - tee.y);
        let d = (dx * dx + dy * dy).sqrt().max(1.0);
        let (nx, ny) = (-dy / d * 27.5, dx / d * 27.5);
        let corridor = vec![
            Vec2 { x: tee.x + nx, y: tee.y + ny },
            Vec2 { x: green.x + nx, y: green.y + ny },
            Vec2 { x: green.x - nx, y: green.y - ny },
            Vec2 { x: tee.x - nx, y: tee.y - ny },
        ];
        holes.push(HolePlan { par: *par, tee, dogleg: None, green, corridor });
        theta = theta_g + 30.0 / r; // green→next-tee walk
    }
    RoutingPlan { clubhouse, holes }
}

// ---------------------------------------------------------------------------
// Stage 9
// ---------------------------------------------------------------------------

/// Level tee pads and green pads: patch = (target plane − terrain) feathered
/// to zero at the edges, on RES_EARTHWORKS_M local grids.
pub fn fixture_earthworks(
    spec: &CourseSpec,
    routing: &RoutingPlan,
    composed: &ComposedHeight,
) -> EarthworksDelta {
    let ep = &spec.params.earthworks;
    let mut patches = Vec::new();
    let mut features = Vec::new();

    let mut pad = |center: Vec2, area_m2: f64, kind: FeatureKind, hole: u32| {
        let half = (area_m2.sqrt() / 2.0) + ep.feather_m;
        let n = ((half * 2.0) / RES_EARTHWORKS_M).ceil() as u32 + 1;
        let gs = GridSpec::new(
            Vec2 { x: center.x - half, y: center.y - half },
            RES_EARTHWORKS_M,
            n,
            n,
        );
        let target = composed.height.bilinear(center);
        let mut delta = Grid::filled(gs, 0.0);
        for y in 0..n {
            for x in 0..n {
                let p = gs.world_of(x, y);
                let dx = (p.x - center.x).abs();
                let dy = (p.y - center.y).abs();
                let edge = half - dx.max(dy);
                let w = smoothstep(0.0, ep.feather_m, edge);
                delta.set(x, y, (target - composed.height.bilinear(p)) * w);
            }
        }
        patches.push(HeightPatch { delta_m: delta });
        features.push(EarthFeature { kind, hole, center, radius_m: (area_m2.sqrt()) / 2.0 });
    };

    for (i, h) in routing.holes.iter().enumerate() {
        let hole = i as u32 + 1;
        pad(h.tee, ep.tee_area_m2, FeatureKind::TeePad, hole);
        pad(h.green, ep.green_area_m2, FeatureKind::GreenComplex, hole);
    }
    EarthworksDelta { patches, features }
}

// ---------------------------------------------------------------------------
// Stage 10
// ---------------------------------------------------------------------------

/// Audit-only fixture: counts D8 pits inside hole corridors on the composed
/// (base + earthworks) terrain. No repairs.
pub fn fixture_drainage(
    routing: &RoutingPlan,
    composed: &ComposedHeight,
    earth: &EarthworksDelta,
) -> DrainageReport {
    let gs = composed.height.spec;
    let mut final_h = composed.height.clone();
    for p in &earth.patches {
        let s = p.delta_m.spec;
        let (w, h) = (s.cell_size * (s.nx - 1) as f64, s.cell_size * (s.ny - 1) as f64);
        for y in 0..gs.ny {
            for x in 0..gs.nx {
                let q = gs.world_of(x, y);
                if q.x >= s.origin.x && q.x <= s.origin.x + w && q.y >= s.origin.y && q.y <= s.origin.y + h {
                    final_h.set(x, y, final_h.get(x, y) + p.delta_m.bilinear(q));
                }
            }
        }
    }

    // Point-in-corridor: quad corridors, test via two triangles (fixture-grade
    // bbox test is fine for the audit stand-in).
    let in_corridor = |p: Vec2| {
        routing.holes.iter().any(|h| {
            let xs: Vec<f64> = h.corridor.iter().map(|c| c.x).collect();
            let ys: Vec<f64> = h.corridor.iter().map(|c| c.y).collect();
            let (x0, x1) = (xs.iter().cloned().fold(f64::INFINITY, f64::min), xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            let (y0, y1) = (ys.iter().cloned().fold(f64::INFINITY, f64::min), ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max));
            p.x >= x0 && p.x <= x1 && p.y >= y0 && p.y <= y1
        })
    };

    let (nx, ny) = (gs.nx as i64, gs.ny as i64);
    let mut ponding = 0u64;
    for y in 1..ny - 1 {
        for x in 1..nx - 1 {
            let p = gs.world_of(x as u32, y as u32);
            if !in_corridor(p) {
                continue;
            }
            let i = (y * nx + x) as usize;
            let zi = final_h.data[i];
            let is_pit = NBR
                .iter()
                .all(|(dx, dy, _)| final_h.data[((y + dy) * nx + (x + dx)) as usize] >= zi);
            if is_pit {
                ponding += 1;
            }
        }
    }

    let mut metrics = BTreeMap::new();
    metrics.insert("corridor_pit_cells".into(), ponding as f64);
    let pass = ponding < 25;
    DrainageReport {
        pass,
        metrics,
        repair_patches: Vec::new(),
        notes: if pass { vec![] } else { vec!["fixture audit: excessive corridor pits".into()] },
    }
}

// ---------------------------------------------------------------------------
// Whole-pipeline fixture
// ---------------------------------------------------------------------------

/// Everything the lab (and the golden) needs, in one deterministic call.
pub struct FixtureBundle {
    pub spec: CourseSpec,
    pub skeleton: MacroSkeleton,
    pub composed: ComposedHeight,
    pub hydro: HydroBundle,
    pub cover: CoverMap,
    pub gate: GateReport,
    pub routing: RoutingPlan,
    pub earthworks: EarthworksDelta,
    pub drainage: DrainageReport,
}

pub fn run_fixture_pipeline(seed: u64, archetype: ArchetypeId, res_m: f64) -> FixtureBundle {
    let spec = fixture_spec(seed, archetype);
    let skeleton = fixture_skeleton(&spec, res_m);
    let composed = fixture_height(&spec, &skeleton);
    let hydro = fixture_hydro(&spec, &composed);
    let cover = fixture_cover(&spec, &hydro);
    let gate = fixture_gate(&spec, &hydro, &cover);
    let routing = fixture_routing(&spec);
    let earthworks = fixture_earthworks(&spec, &routing, &composed);
    let drainage = fixture_drainage(&routing, &composed, &earthworks);
    FixtureBundle { spec, skeleton, composed, hydro, cover, gate, routing, earthworks, drainage }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fnv_f64;

    #[test]
    fn fixture_pipeline_is_deterministic() {
        let a = run_fixture_pipeline(11, ArchetypeId::Piedmont, 24.0);
        let b = run_fixture_pipeline(11, ArchetypeId::Piedmont, 24.0);
        assert_eq!(fnv_f64(&a.composed.height.data), fnv_f64(&b.composed.height.data));
        assert_eq!(a.gate, b.gate);
        assert_eq!(a.routing, b.routing);
        assert_eq!(a.hydro.flow.rcv, b.hydro.flow.rcv);
    }

    #[test]
    fn sandhills_hydro_graph_is_empty() {
        let b = run_fixture_pipeline(3, ArchetypeId::Sandhills, 24.0);
        assert!(b.hydro.graph.streams.is_empty(), "infiltrated ⇒ no streams");
        assert!(b.hydro.graph.lakes.is_empty());
    }

    #[test]
    fn florida_has_lakes_piedmont_has_streams() {
        let fl = run_fixture_pipeline(5, ArchetypeId::FloridaLowland, 24.0);
        assert!(!fl.hydro.graph.lakes.is_empty(), "water table ⇒ lakes");
        let pd = run_fixture_pipeline(5, ArchetypeId::Piedmont, 24.0);
        assert!(!pd.hydro.graph.streams.is_empty(), "fluvial ⇒ streams");
    }

    #[test]
    fn drain_spine_floor_is_monotone() {
        let spec = fixture_spec(9, ArchetypeId::Piedmont);
        let skel = fixture_skeleton(&spec, 24.0);
        for spine in &skel.structure.spines {
            if spine.kind != SpineKind::Drain {
                continue;
            }
            let zs: Vec<f64> = spine.pts.iter().map(|p| skel.base_height.bilinear(*p)).collect();
            for w in zs.windows(2) {
                assert!(w[1] <= w[0] + 0.75, "drain spine must fall (tolerance for carve overlap)");
            }
        }
    }

    #[test]
    fn routing_stays_in_core_and_sums_to_par_36() {
        let plan = fixture_routing(&fixture_spec(1, ArchetypeId::GlacialMoraine));
        assert_eq!(plan.holes.len(), 9);
        assert_eq!(plan.holes.iter().map(|h| h.par).sum::<u32>(), 36);
        for h in &plan.holes {
            assert!(crate::in_core(h.tee), "tee outside core");
            assert!(crate::in_core(h.green), "green outside core");
        }
    }

    #[test]
    fn earthworks_patches_feather_to_zero() {
        let spec = fixture_spec(2, ArchetypeId::Piedmont);
        let skel = fixture_skeleton(&spec, 24.0);
        let composed = fixture_height(&spec, &skel);
        let plan = fixture_routing(&spec);
        let ew = fixture_earthworks(&spec, &plan, &composed);
        assert_eq!(ew.patches.len(), 18);
        for p in &ew.patches {
            let s = p.delta_m.spec;
            for x in 0..s.nx {
                assert!(p.delta_m.get(x, 0).abs() < 1e-9, "edge must feather to 0");
                assert!(p.delta_m.get(x, s.ny - 1).abs() < 1e-9);
            }
        }
    }
}
