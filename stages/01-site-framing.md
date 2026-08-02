# Stage 01 — Site Framing & Structure

**Status:** unclaimed

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

## Contract

Input: `CourseSpec`. Output — `framing.json` (small, pure JSON):

```
Framing {
  window: valley_floor | interfluve | escarpment_face | basin_margin
        | piedmont_slope | terrace_flight,      // categorical, archetype-weighted
  base_level: { edge: n|e|s|w|corner_ne|…, elev_m: f64 },
                                 // where water leaves; fixes drainage direction
  regional_tilt: { dir_rad: f64, grade: f64 }, // consistent with base_level
  grain: { dir_rad: f64, anisotropy: f64 },    // structural grain vector + strength
  provinces: {
    count: 1 | 2,
    boundary: null | { curve: [[x,y],…],       // spline across the box
                       kind: scarp | valley_wall | material_contact | none },
  },
}
```

Window-position weights, tilt/grain ranges, and province probability are all
`framing.*` knobs per archetype. `water_table` archetypes: base_level is the
water-table elevation and every edge drains to it (window ∈ {basin_margin,
interfluve} effectively flat).

## Per-archetype behavior (via knobs)

- sandhills: grain = dominant dune orientation (strong anisotropy); windows
  favor interfluve/basin_margin; provinces rare.
- piedmont: windows favor piedmont_slope/valley_floor; moderate grain.
- florida_lowland: near-zero tilt; window ≈ basin_margin; provinces rare.
- glacial_moraine: window ∈ {interfluve, basin_margin}; provinces common
  (moraine front = material_contact boundary).
- mountain_bench: windows favor escarpment_face/terrace_flight; provinces
  common (scarp boundary); steepest tilt range.

## Hard requirements

1. Internal consistency: `regional_tilt.dir` points at `base_level.edge`
   within a stated tolerance; a province boundary curve enters and exits the
   box (no dead-end curves).
2. Stable across rerolls (stream scope `stable`).
3. Deterministic double-run; canonical JSON artifact.
4. Every categorical outcome reachable: over N seeds each window position
   with nonzero weight appears (test with a fixed seed sweep).

## References

- `crates/course-world/src/spline.rs` (`catmull_rom`, `Spine`) for the
  boundary curve representation.
- Retired step-03 `planner.rs` (commit 0313432,
  `crates/course-macro/src/planner.rs`) — its Frame/Edge drainage-direction
  logic is prior art for base-level/tilt consistency.

## Open questions

- Is `window` purely a prior on downstream knobs (amplitude/tilt/datum
  choices) or does it also gate the Stage-03 grammar (e.g. terrace_flight
  requires bench strokes)? Draft: both, via the grammar table in stage 03.
- Do we need 3 provinces ever? Draft: cap at 2 until a real course demands
  more.
