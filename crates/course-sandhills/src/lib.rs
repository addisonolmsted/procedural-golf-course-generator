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

pub mod assemble;
pub mod blowout;
pub mod carve;
pub mod channel;
pub mod draw;
pub mod gorge;
pub mod meander;
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
pub use mode::Mode;
pub use record::FormClass;

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

/// X3 — the finished fluvial tile at 2 m: the assembled landform with the
/// grain-aligned texture quilted on.
///
/// The division of labour is the one both failed experiments in X0c argued
/// for: the 8 m layer supplies LANDFORM (bed, valley profile, shoulders,
/// rounded divides) and the 2 m layer supplies GRAIN. Neither tries to do
/// the other's job — noise at landform scale mottles, and texture cannot
/// round a 100 m ridge.
pub fn build_fluvial_textured(id: &RunIdentity, prof: &assemble::HandProfile,
                              pack: &texture::PatchPack)
    -> (Descriptors, channel::Network, assemble::Assembled, Grid<f64>,
        water::Water, Vec<blowout::Bay>) {
    let (d, net, asm, beds_for_water) = build_fluvial_macro_beds(id, prof);
    let spec = asm.height.spec;

    // Grain = the local channel tangent, from the assembler's own distance
    // field, derived exactly as the patch pack derives it from a real
    // tile's: the perpendicular of the smoothed distance gradient. Both
    // sides of the comparison are built the same way, which is what makes
    // pasted fabric run along OUR valleys the way it ran along theirs.
    let mut dsm = asm.dist.data.clone();
    for _ in 0..6 {
        let src = dsm.clone();
        for y in 0..spec.ny as i64 {
            for x in 0..spec.nx as i64 {
                let at = |dx: i64, dy: i64| {
                    src[spec.index((x + dx).clamp(0, spec.nx as i64 - 1) as u32,
                                   (y + dy).clamp(0, spec.ny as i64 - 1) as u32)]
                };
                dsm[spec.index(x as u32, y as u32)] =
                    0.2 * (at(0, 0) + at(-1, 0) + at(1, 0) + at(0, -1) + at(0, 1));
            }
        }
    }
    // The grain is an AXIS, not a direction, and it must be smoothed as
    // one. Taken raw, the distance gradient reverses across every divide —
    // it points away from whichever channel happens to be nearest — so
    // neighbouring patches on a ridge were pasted at alternating
    // orientations, which is what read as a regular herringbone. Averaging
    // the DOUBLED angle (cos 2t, sin 2t) treats t and t+180 as the same
    // axis, so the field varies slowly and the flip disappears.
    let mut c2 = vec![0.0f64; spec.len()];
    let mut s2 = vec![0.0f64; spec.len()];
    for y in 0..spec.ny {
        for x in 0..spec.nx {
            let i = spec.index(x, y);
            let (xm, xp) = (x.saturating_sub(1), (x + 1).min(spec.nx - 1));
            let (ym, yp) = (y.saturating_sub(1), (y + 1).min(spec.ny - 1));
            let gx = (dsm[spec.index(xp, y)] - dsm[spec.index(xm, y)]) / 16.0;
            let gy = (dsm[spec.index(x, yp)] - dsm[spec.index(x, ym)]) / 16.0;
            let t = 2.0 * course_world::math::atan2(gx, -gy);
            c2[i] = course_world::math::cos(t);
            s2[i] = course_world::math::sin(t);
        }
    }
    for _ in 0..10 {
        for f in [&mut c2, &mut s2] {
            let src = f.clone();
            for y in 0..spec.ny as i64 {
                for x in 0..spec.nx as i64 {
                    let at = |dx: i64, dy: i64| {
                        src[spec.index((x + dx).clamp(0, spec.nx as i64 - 1) as u32,
                                       (y + dy).clamp(0, spec.ny as i64 - 1) as u32)]
                    };
                    f[spec.index(x as u32, y as u32)] =
                        0.2 * (at(0, 0) + at(-1, 0) + at(1, 0) + at(0, -1) + at(0, 1));
                }
            }
        }
    }
    let mut grain = Grid::filled(spec, 0.0f64);
    let mut gate = Grid::filled(spec, 0.0f64);
    for i in 0..spec.len() {
        grain.data[i] = 0.5 * course_world::math::atan2(s2[i], c2[i]);
        gate.data[i] = (1.0 - 0.55 * asm.u.data[i]).clamp(0.0, 1.0);
    }

    let mut tr = rng::stream(id, rng::TEXTURE);
    let mut dt = d.clone();
    dt.texture_gain = d.texture_gain * d.fluvial_tex_k;
    // Measured amplitude-by-relief curve (RPROF1). Optional: without it the
    // quilt behaves exactly as before.
    let rprof = texture::ReliefProfile::load(std::path::Path::new(
        "assets/sandhills_relief_profile.txt")).ok();
    let mut tex = match texture::TexProfile::load(std::path::Path::new(
        "assets/sandhills_texture_profile.txt")) {
        Ok(tp) => texture::quilt_fluvial_v2(&mut tr, pack, &asm.height, &grain,
                                            &asm.u, &tp, rprof.as_ref(), &dt),
        Err(_) => texture::quilt_fluvial(&mut tr, pack, &asm.height, &grain,
                                         &gate, &dt),
    };
    let _ = &gate;

    // C6 — Carolina bays, at 2 m and OVER the texture: they are the sharpest
    // landform on this ground and must not be blended away (the aeolian
    // mode learned the same about blowouts).
    let u2 = {
        let mut g = Grid::filled(tex.spec, 0.0f64);
        for y in 0..tex.spec.ny {
            for x in 0..tex.spec.nx {
                g.set(x, y, asm.u.bilinear(tex.spec.world_of(x, y)));
            }
        }
        g
    };
    let d2 = {
        let mut g = Grid::filled(tex.spec, 0.0f64);
        for y in 0..tex.spec.ny {
            for x in 0..tex.spec.nx {
                g.set(x, y, asm.dist.bilinear(tex.spec.world_of(x, y)));
            }
        }
        g
    };
    // Bays retired at the reviewer's call (2026-08-26). The stream is still
    // opened and `u2` still built, and the `n_bays` DRAW is still taken in
    // draw.rs, so the transcript and every aeolian tile stay byte-stable --
    // only the call that cut them into the ground is gone.
    //
    // This removal was lost once already: reverting round 9 with a
    // `git checkout` of lib.rs took it with it, and seed 66331 came back
    // carrying a bay. Caught by the gallery render, not by a test.
    let _br = rng::stream(id, rng::BLOWOUT);
    let _ = &u2;
    let bays: Vec<blowout::Bay> = Vec::new();

    // X4 — water last, on the finished ground
    let mut wr2 = rng::stream(id, rng::WATER);
    let tiers: Vec<u8> = net.chans.iter().map(|c| c.tier).collect();
    let water = water::fluvial(&mut wr2, &mut tex, &beds_for_water, &tiers, &u2, &d);
    (d, net, asm, tex, water, bays)
}

/// X1+X2 — the fluvial surface at 8 m: network, graded beds, and the
/// measured HAND profile assembled over them. This is the whole macro
/// stage; the 2 m texture rides on top.
pub fn build_fluvial_macro(id: &RunIdentity, prof: &assemble::HandProfile)
    -> (Descriptors, channel::Network, assemble::Assembled) {
    let (d, net, asm, _) = build_fluvial_macro_beds(id, prof);
    (d, net, asm)
}

/// As `build_fluvial_macro`, also returning the graded beds — the water
/// stage needs them to know where each creek's floor actually is.
pub fn build_fluvial_macro_beds(id: &RunIdentity, prof: &assemble::HandProfile)
    -> (Descriptors, channel::Network, assemble::Assembled,
        Vec<(Vec<course_world::math::Vec2>, Vec<f64>)>) {
    let d = draw::site(id, Some(Mode::Fluvial), None);
    let mut dr = rng::stream(id, rng::DATUM);
    let datum = channel::datum(&mut dr, &d);
    let mut cr = rng::stream(id, rng::CHANNEL);
    let net = channel::grow(&mut cr, &datum, &d);
    let mut beds = carve::beds(&mut cr, &net, &datum, &d);
    let mut ar = rng::stream(id, rng::HAND);
    // pass 1: the trunk-and-tributary skeleton defines the valley floors
    let main: Vec<(Vec<course_world::math::Vec2>, Vec<f64>)> = beds
        .iter()
        .zip(net.chans.iter())
        .filter(|(_, c)| c.tier < 4)
        .map(|(b, _)| b.clone())
        .collect();
    let main_tiers: Vec<u8> = net.chans.iter().filter(|c| c.tier < 4)
        .map(|c| c.tier).collect();
    let first = assemble::assemble(&mut ar, &main, &main_tiers, prof, &d);
    // pass 2: hanging gullies re-based onto that ground, then everything
    carve::rebase_hanging(&mut beds, &net, &first.height);
    let mut ar2 = rng::stream(id, rng::HAND);
    let tiers: Vec<u8> = net.chans.iter().map(|c| c.tier).collect();
    let asm = assemble::assemble(&mut ar2, &beds, &tiers, prof, &d);
    (d, net, asm, beds)
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

    // A6a: the river GORGE, cut into the macro before texture — the valley
    // is a landform and belongs under the quilt.
    //
    // Replaces the old plan_river / carve_corridor / carve_rim_draws stack
    // (review 2026-08-26: those valleys read as painted on). That was a
    // smooth per-station trough with a sinuous line in it; a real Sandhills
    // river runs in a dissected canyon. See gorge.rs.
    let mut rr = rng::stream(id, rng::WATER);
    let gorge = if d.allogenic_river {
        gorge::build(&mut rr, &mut sf.height, &d)
    } else {
        None
    };

    // T: texture at 2 m — now covering the corridor floor too
    let mut tr = rng::stream(id, rng::TEXTURE);
    let mut height = texture::quilt(&mut tr, pack, &sf.height, &d);

    // A5b: blowouts over the texture. A blowout inside the canyon would be
    // a dune feature cut into a river valley, so the gorge keeps them out.
    let avoid = gorge.as_ref().map(|g| (g.creek.as_slice(), 420.0));
    let blowouts = blowout::carve(&mut pr, &mut height, &tnorm8, &d, avoid);

    // A6b: lakes on the finished ground, then the sharp creek slot.
    // The gorge supplies the table drawdown: its floor sits ~50 m below the
    // regional table, and without the drawdown the whole canyon would read
    // as one lake.
    let dd = gorge.as_ref().map(|g| water::Drawdown {
        line: &g.creek,
        bed: &g.creek_z,
        inner: 200.0,
        reach: 620.0,
    });
    let mut water = water::find(&height, &sf.datum, &d, dd.as_ref(), &blowouts);
    // No standing lakes on the valley floor beside running water. Done
    // BEFORE cut_creek so it cannot remove the river it just laid down.
    if let Some(g) = &gorge {
        water::clear_lakes_near(&mut water, &g.creek, &g.creek_z);
    }
    let river = if let Some(g) = &gorge {
        water::cut_creek(&mut height, &mut water, &g.creek, &g.creek_z, d.river_w_m);
        Some(g.creek.clone())
    } else {
        None
    };

    Tile { d, height, water: water.surface, lake_frac: water.lake_frac,
           blowouts, river }
}
