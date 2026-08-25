//! Attempt 5 — the Sandhills archetype, built alone.
//!
//! See `docs/sandhills/README.md`. One archetype, two structural modes
//! ([`Mode::Aeolian`] = Nebraska dune trains, [`Mode::Fluvial`] = the Carolina
//! sand cap), unified by the sand mantle and gated separately.
//!
//! **Owns:** every stage from the archetype draw to the finished 2 m
//! heightfield, for this archetype only. Routing and green-siting are not in
//! scope.
//!
//! **The architectural spine:** a structure generator emits LINES; one surface
//! synthesiser hangs ground off a line set given a section program and a
//! composition sign. Dunes are positive, valleys negative. The synthesiser is
//! shared; the structure generators are not. That is what makes the next
//! archetype cheap.
//!
//! **Expected failures** (discipline rule 6): before the texture stage exists,
//! every texture and roughness metric fails and is supposed to. The aeolian
//! mode fails every drainage metric permanently and correctly.
//!
//! DEPENDENCY RULE: this crate may depend on `course-world` and `course-seed`
//! and NOTHING else — not the retired v2 pipeline crates, and not attempt 4's
//! generator crates (`course-draw`, `course-template`, `course-network`,
//! `course-relief`). Enforced by `tools/no_old_deps.sh`. Anything wanted must
//! be added to the allowlist in `docs/sandhills/README.md` §3 first, then
//! COPIED with a provenance comment naming its source commit.

pub mod blowout;
pub mod carve;
pub mod channel;
pub mod draw;
pub mod mode;
pub mod record;
pub mod rng;
pub mod surface;
pub mod texture;
pub mod water;
pub mod wind;

use course_seed::RunIdentity;
use course_world::grid::Grid;

pub use draw::Descriptors;

/// C1 + C2 of the fluvial mode: the sand-cap datum and the dendritic network
/// grown on it — everything the NETWORK VIEWER round reviews, and nothing
/// more. No carving happens here; C3 consumes this.
pub fn build_fluvial_skeleton(id: &RunIdentity)
    -> (Descriptors, Grid<f64>, channel::Network) {
    let d = draw::site(id, Some(Mode::Fluvial), None);
    let mut dr = rng::stream(id, rng::DATUM);
    let datum = channel::datum(&mut dr, &d);
    let mut cr = rng::stream(id, rng::CHANNEL);
    let net = channel::grow(&mut cr, &datum, &d);
    (d, datum, net)
}

/// C1–C5: the full fluvial surface — datum, network, carve, relict mantle,
/// grain-aligned texture. Water and bays are later stages.
pub fn build_fluvial_full(id: &RunIdentity, pack: &texture::PatchPack)
    -> (Descriptors, Grid<f64>, Grid<f64>, channel::Network) {
    let d = draw::site(id, Some(Mode::Fluvial), None);
    let mut dr = rng::stream(id, rng::DATUM);
    let datum = channel::datum(&mut dr, &d);
    let mut cr = rng::stream(id, rng::CHANNEL);
    let net = channel::grow(&mut cr, &datum, &d);
    let mut height = datum.clone();
    carve::carve(&mut cr, &mut height, &net, &datum, &d);

    // channel proximity + grain fields at 8 m: distance via chamfer over the
    // resampled network, tangent = the perpendicular of the smoothed
    // distance-field gradient (the SAME derivation the patch pack used on
    // the real tiles — symmetry is the point)
    let spec8 = height.spec;
    let big = 1e18f64;
    let mut dist = vec![big; spec8.len()];
    for c in &net.chans {
        for w in c.pts.windows(2) {
            let seg = w[0].distance(w[1]);
            let n = (seg / 6.0).ceil() as usize;
            for k in 0..=n {
                let t = k as f64 / n.max(1) as f64;
                let px = w[0].x + (w[1].x - w[0].x) * t;
                let py = w[0].y + (w[1].y - w[0].y) * t;
                let x = (px / 8.0).round().clamp(0.0, (spec8.nx - 1) as f64) as u32;
                let y = (py / 8.0).round().clamp(0.0, (spec8.ny - 1) as f64) as u32;
                dist[spec8.index(x, y)] = 0.0;
            }
        }
    }
    let (orth, diag) = (8.0, 8.0 * std::f64::consts::SQRT_2);
    for pass in 0..2 {
        let (ys, xs): (Vec<u32>, Vec<u32>) = if pass == 0 {
            ((0..spec8.ny).collect(), (0..spec8.nx).collect())
        } else {
            ((0..spec8.ny).rev().collect(), (0..spec8.nx).rev().collect())
        };
        for &y in &ys {
            for &x in &xs {
                let i = spec8.index(x, y);
                for (dx, dy, w) in [(-1i64, 0i64, orth), (0, -1, orth), (-1, -1, diag),
                                    (1, -1, diag), (1, 0, orth), (0, 1, orth),
                                    (1, 1, diag), (-1, 1, diag)] {
                    let (px, py) = (x as i64 + dx, y as i64 + dy);
                    if px >= 0 && px < spec8.nx as i64 && py >= 0 && py < spec8.ny as i64 {
                        let v = dist[spec8.index(px as u32, py as u32)] + w;
                        if v < dist[i] {
                            dist[i] = v;
                        }
                    }
                }
            }
        }
    }
    // smooth the distance field, then grain = perp of its gradient
    let mut dsm = Grid::filled(spec8, 0.0f64);
    dsm.data.copy_from_slice(&dist);
    for _ in 0..4 {
        let src = dsm.data.clone();
        for y in 1..spec8.ny - 1 {
            for x in 1..spec8.nx - 1 {
                let i = spec8.index(x, y);
                dsm.data[i] = 0.2 * src[i]
                    + 0.2 * (src[spec8.index(x - 1, y)] + src[spec8.index(x + 1, y)]
                        + src[spec8.index(x, y - 1)] + src[spec8.index(x, y + 1)]);
            }
        }
    }
    let mut grain = Grid::filled(spec8, 0.0f64);
    let mut prox = Grid::filled(spec8, 0.0f64);
    for y in 0..spec8.ny {
        for x in 0..spec8.nx {
            let i = spec8.index(x, y);
            let (xm, xp) = (x.saturating_sub(1), (x + 1).min(spec8.nx - 1));
            let (ym, yp) = (y.saturating_sub(1), (y + 1).min(spec8.ny - 1));
            let gx = (dsm.get(xp, y) - dsm.get(xm, y)) / 16.0;
            let gy = (dsm.get(x, yp) - dsm.get(x, ym)) / 16.0;
            grain.data[i] = course_world::math::atan2(gx, -gy);
            prox.data[i] = course_world::math::smoothstep(320.0, 60.0, dist[i]);
        }
    }

    // C4: the relict mantle on the interfluves — the aeolian hummock
    // machinery at low relief, gated AWAY from the valley zone
    let mut wr = rng::stream(id, rng::WIND);
    // κ clamped LOW: a relict, degraded mantle is isotropic mounds — a
    // train-class draw printed coherent wave trains across the interfluves
    // (seed 104 render), which is live aeolian fabric, not a relict one.
    let hw = wind::build(&mut wr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                         d.wind_wander_m * 0.45, d.hummock_kappa.min(0.7),
                         d.hummock_spread.max(0.5));
    let hf = surface::hummock_field(&hw);
    // CALM (review 2026-08-27: real interfluves are matte fine grain; the
    // full-strength mantle printed vermiculate swirls): low relief, and
    // gated by a two-octave patchiness field so mound fields survive in
    // PATCHES on an otherwise quiet upland — how relict topography ages.
    let mantle_relief = (d.hummock_relief_m * 0.28).min(1.2);
    let sp1 = wr.next_u32();
    let sp2 = wr.next_u32();
    for y in 0..spec8.ny {
        for x in 0..spec8.nx {
            let i = spec8.index(x, y);
            let p = spec8.world_of(x, y);
            let sup = 0.5
                + 0.5
                    * (course_world::noise::perlin2(p.x / 900.0, p.y / 900.0, sp1)
                        + 0.5 * course_world::noise::perlin2(p.x / 380.0, p.y / 380.0, sp2));
            let patch = course_world::math::smoothstep(0.35, 0.75, sup);
            let upland = 1.0 - prox.data[i];
            height.data[i] += mantle_relief * upland * patch * hf.data[i];
        }
    }

    // C5: the grain-aligned texture. Gain trimmed against the measured NC
    // fine band (real fine_std 0.317-0.405 across 8 tiles; the aeolian gain
    // draw ran ours to 0.40-0.49).
    let mut tr = rng::stream(id, rng::TEXTURE);
    let mut dt = d.clone();
    dt.texture_gain = d.texture_gain * 0.72;
    let tex = texture::quilt_fluvial(&mut tr, pack, &height, &grain, &prox, &dt);
    (d, datum, tex, net)
}

/// C1–C3: the skeleton plus the valley carve, at 8 m. What the CARVED
/// review round judges; later stages (mantle, texture, bays, water) build
/// on this. The carve draws continue on the channel stream, so a seed's
/// network is byte-identical with and without carving.
pub fn build_fluvial_carved(id: &RunIdentity)
    -> (Descriptors, Grid<f64>, Grid<f64>, channel::Network) {
    let d = draw::site(id, Some(Mode::Fluvial), None);
    let mut dr = rng::stream(id, rng::DATUM);
    let datum = channel::datum(&mut dr, &d);
    let mut cr = rng::stream(id, rng::CHANNEL);
    let net = channel::grow(&mut cr, &datum, &d);
    let mut height = datum.clone();
    carve::carve(&mut cr, &mut height, &net, &datum, &d);
    (d, datum, height, net)
}

pub use mode::Mode;
pub use record::FormClass;

/// The finished aeolian tile: the full A0–A6 pipeline for one seed.
pub struct Tile {
    pub d: Descriptors,
    /// 2 m textured heightfield with blowouts and the river valley carved.
    pub height: Grid<f64>,
    /// Water surface at 2 m; NaN = dry.
    pub water: Grid<f64>,
    pub lake_frac: f64,
    pub blowouts: Vec<blowout::Blowout>,
    pub river: Option<Vec<course_world::math::Vec2>>,
}

/// Run the whole aeolian pipeline. Stage order is load-bearing:
/// pans go UNDER the texture (they are 100–300 m landforms), blowouts go
/// OVER it (they are the sharpest features on the ground and must not be
/// blended), water is found last on the finished surface.
pub fn build_full(id: &RunIdentity, pack: &texture::PatchPack,
                  forced_form: Option<FormClass>) -> Tile {
    let d = draw::site(id, Some(Mode::Aeolian), forced_form);
    let mut wr = rng::stream(id, rng::WIND);
    let w = wind::build(&mut wr, d.wind_rad, d.wavelength_m, d.wind_wander_rad,
                        d.wind_wander_m, d.kappa, 0.0);
    let mut hr = rng::stream(id, rng::HUMMOCK);
    let hw = wind::build(&mut hr, d.wind_rad, d.hummock_lambda_m, d.wind_wander_rad,
                         d.wind_wander_m * 0.45, d.hummock_kappa, d.hummock_spread);
    let mut br = rng::stream(id, rng::PATCHY);
    let mut sf = surface::build(&mut br, &w, &hw, &d);

    // megaform position at 8 m — the shared axis every gate keys on
    let spec8 = sf.height.spec;
    let tnorm8 = {
        let mut sorted: Vec<f64> = sf.height.data.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let n = sorted.len();
        let mut g = Grid::filled(spec8, 0.0f64);
        for i in 0..n {
            g.data[i] = sorted.partition_point(|v| *v < sf.height.data[i]) as f64 / n as f64;
        }
        g
    };

    // A5a: deflation pans, under the texture
    let mut pr = rng::stream(id, rng::BLOWOUT);
    let _pan = blowout::pans(&mut pr, &mut sf.height, &tnorm8.data, &d);

    // A6a: PLAN the river and carve its CORRIDOR into the macro, before
    // texture — the corridor is a landform and belongs under the quilt
    // (review: the corridor floor was "much too smooth"). The 6 m channel
    // slot stays a sharp post-texture cut.
    let mut rr = rng::stream(id, rng::WATER);
    let river_plan = if d.allogenic_river {
        let pick = water::RiverStyle::PASSED[rr.below(water::RiverStyle::PASSED.len())];
        water::plan_river(&mut rr, &sf.height, &d, pick)
    } else {
        None
    };
    if let Some(pl) = &river_plan {
        water::carve_corridor(&mut sf.height, pl);
    }

    // T: texture at 2 m — now covering the corridor floor too
    let mut tr = rng::stream(id, rng::TEXTURE);
    let mut height = texture::quilt(&mut tr, pack, &sf.height, &d);

    // A5b: blowouts over the texture
    let avoid = river_plan.as_ref().map(|pl| {
        (pl.corr.as_slice(), (pl.floor_hw + pl.wall_m) * 1.4 + 60.0)
    });
    let blowouts = blowout::carve(&mut pr, &mut height, &tnorm8, &d, avoid);

    // A6b: lakes on the finished ground, then the sharp channel slot
    let mut water = water::find(&height, &sf.datum, &d, river_plan.as_ref(), &blowouts);
    let river = if let Some(pl) = &river_plan {
        water::cut_channel(&mut height, &mut water, pl);
        Some(pl.pts.clone())
    } else {
        None
    };

    Tile { d, height, water: water.surface, lake_frac: water.lake_frac,
           blowouts, river }
}
