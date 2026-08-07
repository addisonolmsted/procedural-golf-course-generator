# Stage 01 — Site Framing & Structure

**Status:** built (crates/course-framing + the stage-lab viewer;
claimed/built 2026-08-02)

## Purpose

Decide WHERE in an implied larger landscape this 3 km window sits and what
structural skeleton it inherits. These few categorical + vector draws are the
biggest single source of between-seed variety: the same archetype reads
completely differently on a valley floor vs an interfluve vs an escarpment
face.

## Position

- Upstream: Stage 00 (`CourseSpec`, knobs `framing.*`).
- Downstream: Stage 02 (mask placement respects base-level direction),
  Stage 03 (strokes are framed by it), Stage 04 (regional trend + grain →
  forcing fields), Stage 05 (boundary conditions).
- Streams: `framing/v1` — **stable scope** (survives rerolls).

## Contract (as built)

Implementation: [`crates/course-framing/`](../crates/course-framing/)
(`generate(spec) -> Framing`, artifact version `FRAMING_VERSION = 1`).

Input: `CourseSpec`. Output — `framing.json` (small, pure JSON; compact
canonical bytes, loud load validation):

```
Framing {
  seed: u64, pipeline_version: u32, framing_version: u32,
                                 // header — deliberately NOT RunIdentity:
                                 // no attempt field, so reroll stability is
                                 // literal byte-identity across the chain
  window: valley_floor | interfluve | escarpment_face | basin_margin
        | piedmont_slope | terrace_flight,      // categorical, archetype-weighted
  base_level: { edge: n|e|s|w|corner_ne|corner_nw|corner_se|corner_sw,
                elev_m: f64 },   // where water leaves; fixes drainage direction.
                                 // elev_m = -framing.base_drop_m in the local
                                 // datum (0 = nominal core reference elevation)
  regional_tilt: { dir_rad: f64, grade: f64 }, // dir ∈ [0,2π), within
                                 // TILT_TOL_RAD = π/4 of base_level.edge
  grain: { dir_rad: f64, anisotropy: f64 },    // AXIS ∈ [0,π); strength [0,1]
  provinces: {
    count: 1 | 2,                // 2 ⇔ boundary present
    boundary: null | { curve: [[x,y],…],       // enters AND exits the box
                       kind: scarp | valley_wall | material_contact },
  },
  skeleton: {                    // v2 — the structural skeleton
    trunk: null | { spine: [[x,y],…],          // monotone toward base_level.edge
                    halfwidth_m: f64 },        // PLANFORM corridor width
    branches: [ [[x,y],…], … ],  // head→mouth; mouth ON the trunk spine
    ridges:   [ [[x,y],…], … ],  // grain-aligned axes, upstream end first
    steps:    [ [[x,y],…], … ],  // contour-parallel, enter-and-exit
  },
}
```

### Guidance, not landforms

The skeleton says WHERE the land's macro organization runs — nothing about
how strong it is. Relief lives in θ (`framing.topo_ridge_relief_m`,
`topo_trunk_carve_m`, `topo_step_riser_m`), read downstream from
`CourseSpec.params` exactly like `framing.province_relief_m`. Stage 04
compiles the curves into shallow forcing biases; the Stage-05 LEM does the
finishing. This boundary is the lesson of the retired step-03 generator
(commit `0313432`), which authored finished landforms and measured 22/71
knobs inside the real-tile IQR.

Organization comes from deriving everything from the same few controls —
tilt, grain, base level, one wavelength per family — with jitter kept small
relative to spacing: a stratified lattice (never iid placement), one shared
cross-grain axis for the whole ridge train, an axis-jitter cap that forbids
drifting more than a third of the lattice pitch over an axis's own length,
meander amplitude solved for a measurable sinuosity target, junction sides
alternating by index, and end tapers everywhere.

Families may cross (grain-parallel ridges necessarily meet a downhill
trunk). Nothing is trimmed: precedence is a Stage-04 composition rule — the
corridor carve composes last, so water gaps and cut terraces EMERGE.

**Empty is normal.** florida_lowland draws an all-empty skeleton on every
seed; sandhills never draws a trunk. Every consumer must handle it.

Window-position weights, tilt/grain ranges, and province probability are all
`framing.*` knobs per archetype (16 knobs; categorical weights are degenerate
constant tables in the prior — fixed archetype data). `framing.province_relief_m`
is a knob CONSUMED DOWNSTREAM (stages 03/04 read it from `CourseSpec.params`);
the artifact carries only what stage 01 decides. `water_table` archetypes:
no mode branch in stage 01 — consumers read `base_level.elev_m` as the
water-table elevation and every edge drains to it (florida's `base_drop_m`
table is 0–2 m, its window weights ≈ basin_margin).

### The draw transcript (order-sensitive, `framing/v1`)

Base fields: a FIXED 14-uniform transcript; draws 7–14 (boundary geometry)
happen unconditionally and are discarded when `count == 1`, so draw
positions are outcome-independent. Reordering/adding a draw =
`FRAMING_VERSION` bump + golden re-bless. See the module doc of
`crates/course-framing/src/generate.rs`: window, edge (code-constant
weights: cardinals 1.0, corners 0.5), tilt jitter, grain spread, province
count, boundary kind, chord anchor, chord skew, 3×(lateral wobble,
along-chord jitter). The boundary chord is oriented by kind (scarp ⟂ tilt,
valley_wall ∥ tilt, material_contact ∥ grain) and extends 60 m past both box
edges — enters-and-exits by construction.

Skeleton (v2): draws **15–18** are one `u64` SUB-SEED per family (trunk,
branch, ridge, step), always drawn. Each family then runs its own
`DetRng::new(sub_seed, b"framing/<family>/v1")` — derived RNGs, not registry
streams. Families are isolated: changing ridge generation cannot shift the
trunk's draws (tested: `family_isolation`). Each family draws a FIXED budget
sized by its cap (`MAX_BRANCHES`/`MAX_RIDGES`/`MAX_STEPS`) and uses the
first `n`, so a count change cannot shift later draws within the family.

Because the sub-seeds are appended after draw 14, and every skeleton knob is
named `framing.topo_*` — which sorts after `framing.tilt_grade`, the last
SAMPLED base knob, in the positional θ draw order — **every v1 base field
keeps its exact value at v2** (pinned by `base_fields_survive_v2`). A future
knob named before `tilt_grade` fails that test loudly.

### Per-family construction

- **Trunk** — chord from an offset entry anchor to the outlet edge, extended
  60 m past both. Meander offset = sine (+ perlin jitter at an
  incommensurate wavelength, so lobes never repeat) × end taper; amplitude
  found by a DRAW-FREE 24-step bisection on achieved arc/chord against
  `topo_trunk_sinuosity` — a measurable, campaign-fitted target — then
  scaled down (≤4 bounded steps) until the bend radius clears 2.5 channel
  widths. Monotone toward the outlet by construction: the chord IS the
  downhill direction and all displacement is perpendicular.
- **Branches** — mouths stratified along the trunk's arc (0.15–0.85), sides
  alternating by index, junction angle from `topo_branch_junction_deg`, an
  elbow 34% up from the mouth; head shrunk (never moved) to stay in the box.
- **Ridges** — one shared cross-grain axis for the whole train; the k-th
  axis sits in the k-th band at the archetype's pitch with bounded jitter;
  lattice span capped at 95% of the box so bands never clamp-collapse;
  axis jitter capped by pitch/length (see above); single mid bow ≤10%.
- **Steps** — stratified along the fall line with the parameter folded
  around a reserved central band [0.40, 0.60], so risers never cut the
  middle of the site; chord contour-parallel ±0.12 rad; bow limited by
  distance to that band.

## Per-archetype behavior (via knobs)

- sandhills: grain = dominant dune orientation (strong anisotropy); windows
  favor interfluve/basin_margin; provinces rare. Skeleton: 4–8 tight dune
  axes at a 150–400 m pitch (copied from the campaign-fitted
  `landform.dune_wavelength_m`); NO trunk — infiltrated, no surface water.
- piedmont: windows favor piedmont_slope/valley_floor; moderate grain.
  Skeleton: the meandering trunk archetype (`topo_trunk_p` 0.9, sinuosity
  1.19–1.28 from `landform.meander_sinuosity`) with 1–3 tributaries and 1–2
  broad interfluves.
- florida_lowland: near-zero tilt; window ≈ basin_margin; provinces rare.
  Skeleton: EMPTY, every seed (all topo counts/probabilities are 0).
- glacial_moraine: window ∈ {interfluve, basin_margin}; provinces common
  (moraine front = material_contact boundary). Skeleton: 1–3 axes, an
  occasional short low-sinuosity trunk (`topo_trunk_p` 0.3), no steps.
- mountain_bench: windows favor escarpment_face/terrace_flight; provinces
  common (scarp boundary); steepest tilt range. Skeleton: 2–4 ridge spurs,
  a steep low-sinuosity valley trunk, and a 2–4 riser terrace flight on
  terrace/escarpment windows.

## Hard requirements

1. Internal consistency: `regional_tilt.dir` points at `base_level.edge`
   within `TILT_TOL_RAD = π/4`; a province boundary curve enters and exits
   the box (no dead-end curves). (`consistency_invariants`, and
   `Framing::validate` on every load.)
2. Stable across rerolls (stream scope `stable`; attempt-free artifact
   header). (`stable_across_reroll_chain` — byte-identity over all 8
   attempts.)
3. Deterministic double-run; canonical JSON artifact.
   (`determinism_double_run`, `canonical_roundtrip`,
   `golden_framing_seed_1`; re-bless via
   `cargo run -p course-framing --example bless_golden`.)
4. Every categorical outcome reachable: over N seeds each window position
   with nonzero weight appears — and zero-weight ones never do.
   (`categorical_reachability`, 512 seeds × 5 archetypes;
   `skeleton_reachability` for the per-family character.)
5. Skeleton coherence, validated on every load: the trunk crosses the box
   monotonically and reaches the outlet edge; branches only exist with a
   trunk and their mouths sit ON its spine; ridges stay inside the box;
   steps cross the box contour-parallel within `STEP_AXIS_TOL_RAD` (π/5);
   family counts respect their caps. (`skeleton_invariants`,
   `loud_load_rejections_skeleton`.) θ-dependent rules — lattice spacing,
   the reserved central band — are tested, not validated.
6. Base-field compatibility: the v1 values of window/base_level/tilt/grain/
   provinces survive the v2 bump byte-exactly (`base_fields_survive_v2`
   against `tests/base_fields_v1_fixture.json`).

## Viewer

`cargo run -p stage-lab --release` — schematic (grain glyphs, tilt arrow,
outlet edge band, province spline by kind, skeleton families by color:
trunk/branch blue with a corridor band, ridges tan, steps violet), a
toggleable ILLUSTRATIVE implied-terrain hillshade (regional plane + ridge
bumps + step risers + corridor carve + grain noise, amplitudes from the θ
relief knobs — explicitly not a pipeline artifact), live `framing.*`
overrides, attempt scrubber (demonstrates requirement 2), seed-sweep gallery
(filter/force per archetype). Headless:
`cargo run -p stage-lab --release --example framing_snapshots`.

## References

- `crates/course-world/src/spline.rs` (`catmull_rom`) — the boundary curve.
- Retired step-03 `planner.rs` (commit 0313432,
  `crates/course-macro/src/planner.rs`) — its Frame/Edge drainage-direction
  logic was the prior art for base-level/tilt consistency.

## Open questions

- ~~Is `window` purely a prior on downstream knobs or does it also gate the
  Stage-03 grammar?~~ Resolved (2026-08-02): both — the stage-03 grammar
  table may key on `window` (e.g. terrace_flight ⇒ bench strokes); no code
  in stage 01 itself.
- ~~Do we need 3 provinces ever?~~ Resolved (2026-08-02): capped at 2 until
  a real course demands more (`count ∈ {1,2}` is contract-validated).
- No window↔edge coupling in v1 (any window class can draw any outlet
  edge); downstream stages read the pair jointly. Revisit if a stage doc
  needs correlated draws.
- Outlet-edge weights are code constants (cardinals 1.0, corners 0.5), not
  knobs — promote to `framing.*` the day an archetype differentiates.
- Ridge axes are straight-with-bow; they do not bend around the trunk
  corridor. The archive's angle-doubled alignment field (`fields.rs::
  grain_at`, commit 0313432) is the machinery if that ever reads wrong.
- Trunk presence is one knob with no window modulation (a valley_floor
  window might reasonably boost it). Steps ARE window-gated
  ({terrace_flight, escarpment_face}); a per-window multiplier table is the
  refinement if the binary gate proves blunt.
- Ridge placement is a stratified lattice, chosen so one code path serves
  both dune trains and sparse interfluves. Mitchell best-candidate (soft
  blue noise, archive `planner.rs`) is the alternative if ridges ever need
  to avoid other features rather than tile a field.
