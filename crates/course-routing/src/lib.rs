//! S5 siting, the green pool, and S6 routing — the Rust port of the Python
//! prototype in `tools/golf/` (`siting.py`, `greens.py`, `routing.py`,
//! `dtm_primitives/geomorphons.py`), decided 2026-09-14:
//!
//! * the input is the frozen Sandhills terrain as it comes out of
//!   `course-sandhills` (2 m height, 2 m water surface, the creek lines) —
//!   the v2 C2/C3 contracts are retired on this branch;
//! * every measured dial is carried verbatim with its provenance comment;
//! * acceptance is STATISTICAL parity with the prototype on the same seeds
//!   (`tools/golf/batch_route.py` writes the reference): routed rate, par
//!   mix, total-length p10/50/90, crossings — plus the contact sheets — not
//!   bit-exactness against numpy;
//! * no random draws anywhere: fixed beam width x depth, total-order
//!   tie-breaks (stable sorts, index-keyed), no float-equality comparisons.
//!
//! Layout, in pipeline order:
//! `terrain` (input + loader) → `img` (the scipy/numpy kernels) → `fields`
//! (the 8 m stack) → `morph` (geomorphons) → `siting` (window, clubhouse,
//! loop probe) → `greens` (typed detectors, fitted scorer, approach tables)
//! → `route` (beam over green orderings + router-chosen par, detail
//! placement, exact rescore) → `dump` (JSON for the sheets and the batch).
//! `geom` holds the shared geometry (segments, crossings, clearance,
//! trapezoid).
//!
//! Coordinates follow the prototype: a point is `(y, x)` in world metres,
//! grids are row-major `[y * nx + x]`. `cell` is 8.0 on every field.

pub mod terrain;
pub mod img;
pub mod geom;
pub mod fields;
pub mod morph;
pub mod siting;
pub mod greens;
pub mod route;
pub mod dump;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// World metres, `(y, x)`, the prototype's convention.
pub type Yx = (f64, f64);

/// Which archetype built the tile; sets the play-window dims
/// (`siting::dims_for`): aeolian 1450 x 950 m, fluvial 1000 x 700 m.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mode {
    Aeolian,
    Fluvial,
}

/// The 8 m derived stack (`siting.Fields`). Every vector is `ny * nx`
/// row-major at `cell` = 8 m over the FULL 3 km extent.
#[derive(Clone, Debug)]
pub struct Fields {
    pub cell: f64,
    pub nx: usize,
    pub ny: usize,
    pub z8: Vec<f64>,
    /// rise/run, `np.gradient` convention (central, one-sided at edges)
    pub slope: Vec<f64>,
    /// percentile rank of the 400 m lowpass elevation
    pub relief_pos: Vec<f64>,
    pub tpi200: Vec<f64>,
    /// std of the 4 x 4 block of 2 m heights under each cell
    pub subgrid_rough: Vec<f64>,
    pub wet8: Vec<bool>,
    /// metres to the nearest wet cell, `f64::INFINITY` if the tile is dry
    pub d_water: Vec<f64>,
    /// disc-eroded pads (`playability.pads`); `pad_green` is the KNOWN-BAD
    /// GREEN tier kept only because `room` and the typed detectors read it
    pub pad_green: Vec<bool>,
    pub pad_tee: Vec<bool>,
    /// `slope <= 0.08 & !wet8`
    pub pad_fair: Vec<bool>,
    /// EDT inside `pad_green`, metres
    pub room: Vec<f64>,
}

/// Geomorphon morphology (`siting.Morphology`).
#[derive(Clone, Debug)]
pub struct Morphology {
    pub cls240: Vec<u8>,
    pub cls80: Vec<u8>,
    pub saddle: Vec<bool>,
    /// `[0, pi)` open axis
    pub fall_axis: Vec<f64>,
    /// `[0, pi)` blocked axis
    pub rise_axis: Vec<f64>,
    /// cells inside which classes are lookup artefacts
    pub collar: usize,
}

/// Pit and peak persistence over the core (`siting.Persistence`); zero
/// outside the core.
#[derive(Clone, Debug)]
pub struct Persistence {
    /// metres of held water before the spill
    pub pit: Vec<f64>,
    /// prominence, metres
    pub peak: Vec<f64>,
    /// radians, the punchbowl's OPEN bearing
    pub spill_dir: Vec<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clubhouse {
    pub yx: Yx,
    pub score: f64,
    pub reserved_green: Yx,
    pub reserved_tee: Yx,
    /// spatially distinct picks, best first; the router auditions each
    pub alternates: Vec<Clubhouse>,
}

/// The sited play window and its clubhouse (`siting.Siting`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Siting {
    /// top-left, 8 m indices `(i, j)` into the full grid
    pub window_ij: (usize, usize),
    /// `(min_y, min_x, h, w)` world metres
    pub window_m: (f64, f64, f64, f64),
    pub clubhouse: Clubhouse,
    pub pair_score: f64,
    /// `(ij, win_score, ch_score, loop_closed)` per shortlisted window
    pub shortlist: Vec<((usize, usize), f64, f64, usize)>,
}

/// How a green candidate would be built (`greens.Candidate.build`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Build {
    Natural,
    Graded,
}

pub const N_BEARINGS: usize = 16;

/// A green candidate (`greens.Candidate`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Candidate {
    pub yx: Yx,
    /// the typed detector that seeded it (the prototype's `kind` string)
    pub kind: String,
    pub score: f64,
    /// pad plane gradient `(gy, gx)`, rise/run
    pub grad: (f64, f64),
    pub persistence: f64,
    /// per bearing: `[visible, recept, backdrop, room]`
    pub approach: [[f64; 4]; N_BEARINGS],
    /// the loop-closure anchor slot
    pub reserved: bool,
    pub build: Build,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TeeBox {
    pub yx: Yx,
    pub size_m: f64,
    /// play axis, radians
    pub axis: f64,
    pub length_m: f64,
    /// placed on `tee_relaxed` ground: needs earthwork
    pub graded: bool,
    /// the 8 m `Fields::slope` at the box cell, rise/run
    pub slope: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BridgeKind {
    Walk,
    Spine,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Bridge {
    pub a_yx: Yx,
    pub b_yx: Yx,
    pub span_m: f64,
    pub kind: BridgeKind,
    pub hole: usize,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Walk {
    /// `[[y0, x0], [y1, x1]]`, a straight segment
    pub path: [Yx; 2],
    pub length_m: f64,
    pub grade_mean: f64,
    pub bridges: Vec<Bridge>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Hole {
    pub index: usize,
    pub par: u8,
    pub green_idx: usize,
    /// the green candidate's typed-detector `kind` (`Candidate::kind`)
    pub kind: String,
    pub green_yx: Yx,
    /// five, back tee first, `length_m` non-increasing
    pub tee_boxes: Vec<TeeBox>,
    /// `(y, x, r_m)`
    pub lzs: Vec<(f64, f64, f64)>,
    /// back tee -> LZs -> green
    pub spine: Vec<Yx>,
    pub length_m: f64,
    pub approach_bin: usize,
    pub bridges: Vec<Bridge>,
    pub walk_from_prev: Walk,
    pub terms: BTreeMap<String, f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Route {
    pub holes: Vec<Hole>,
    pub par_sequence: Vec<u8>,
    pub total_length_m: f64,
    pub total_walk_m: f64,
    pub clubhouse_yx: Yx,
    pub score: f64,
    pub terms: BTreeMap<String, f64>,
    /// `(hole_i, hole_j, (y, x))` play crossings
    pub crossings: Vec<(usize, usize, Yx)>,
    /// `(green_hole, spine_hole, d_m)`: greens within `route::GREEN_CLEAR_M`
    /// of another hole's spine, the successor's first `GREEN_JUNCTION_M` of
    /// arc exempt, hole 1's likewise against the last green (the clubhouse
    /// loop junction) (plan item 1); ascending `(green_hole, spine_hole)`.
    pub green_in_play: Vec<(usize, usize, f64)>,
    /// share of the window's 8 m cells within `route::COV_R_M` of a spine
    /// (plan item 4)
    pub coverage: f64,
}

/// The whole chain on one tile: siting, the green pool, routing. Returns
/// `None` when no route survives (the prototype's `run_routing` -> None).
pub fn route_tile(t: &terrain::Terrain) -> Option<(Siting, Route)> {
    let (sit, f, m, p) = siting::run_siting(t, siting::dims_for(t.mode), None);
    let pool = greens::generate(t, &sit, &f, &m, &p, greens::N_TARGET, None);
    let pool_fn = |ch: &Clubhouse| -> Vec<Candidate> {
        let mut s2 = sit.clone();
        s2.clubhouse = ch.clone();
        greens::generate(t, &s2, &f, &m, &p, greens::N_TARGET, None)
    };
    let r = route::run_routing(t, &sit, &f, &m, &p, &pool, Some(&pool_fn))?;
    Some((sit, r))
}
