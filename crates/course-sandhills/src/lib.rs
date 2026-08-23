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

    // T: texture at 2 m
    let mut tr = rng::stream(id, rng::TEXTURE);
    let mut height = texture::quilt(&mut tr, pack, &sf.height, &d);

    // A5b: blowouts over the texture
    let blowouts = blowout::carve(&mut pr, &mut height, &tnorm8, &d);

    // A6: lakes, then the redesigned creek-scale river on the coin.
    let mut water = water::find(&height, &sf.datum, &d);
    let mut rr = rng::stream(id, rng::WATER);
    water::river(&mut rr, &mut height, &mut water, &d);

    Tile { d, height, water: water.surface, lake_frac: water.lake_frac,
           blowouts, river: water.river }
}
