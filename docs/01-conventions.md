# Conventions

Everything in this file is cross-stage. A stage doc may narrow a convention; it
may not contradict one.

## Units

| Quantity | Unit | Notes |
|---|---|---|
| Length, elevation | metres (f64) | Never feet, never "units". Suffix `_m` on every field. |
| Angle | radians (f64) | Suffix `_rad`. |
| Grade / slope | dimensionless rise-over-run | Not degrees, not percent. |
| Area | m² | Suffix `_m2`. |
| Volume | m³ | Suffix `_m3`. Earthmoving budgets are volumes. |
| Dials, intensities, fractions | dimensionless, [0, 1] | No suffix. Clamped, never wrapped. |
| Time budgets | milliseconds | Suffix `_ms`. |

Newtypes live in `course-contracts::units`. Bare `f64` is acceptable inside a
stage; anything crossing a contract boundary is a named field with a unit
suffix.

**Directions vs axes.** A *direction* is in `[0, 2π)` — it points somewhere
(flow direction, wind azimuth, tilt). An *axis* is in `[0, π)` — it has no
head or tail (grain, anisotropy, strata strike). Mixing them is the single most
common cross-stage bug in this codebase's history. Name them `dir_rad` and
`axis_rad` respectively and never assign one to the other.

## World geometry

Fixed for every course, defined in
[`crates/course-world/src/world.rs`](../crates/course-world/src/world.rs).
Do not redefine these; import them.

- **Extent**: 3 km × 3 km, origin at the SW corner, x east, y north
  (`EXTENT_M = 3000.0`).
- **Core**: the central 1.5 km × 1.5 km window `[750, 2250]²`
  (`CORE_MIN_M`, `CORE_MAX_M`, `in_core`). The **search region** for site
  selection; the surrounding margin exists so that landforms enter and leave
  the frame instead of terminating at it.
- **Play window**: an axis-aligned square chosen inside the core by
  [S5](stages/stage-05-siting-substrate.md), its side **per-biome data** in
  `ScorerPreset::play_m` — **1200 m** for the sprawling biomes (sandhills,
  great plains), **800 m** for the compact four — bounded by
  `PLAY_M_MIN`/`PLAY_M_MAX` (600–1400) in C2. The course is routed inside
  *this*, not the whole core.

  Measured (2026-08-28, corridor packing over 12 seeds/archetype): a 600 m
  window fits a 9-hole route on only 2/12 aeolian-sandhills seeds (median 6
  holes packed) but 11/12 fluvial seeds; at 1200 m aeolian reaches 11/12.
  Minimalist dune courses genuinely sprawl — grading between the dunes is not
  affordable — while parkland routes compactly. A biome is a data record, so a
  per-biome size is config, not a stage branch.

  The window must lie inside the core, and that containment IS the **750 m
  minimum terrain margin** — no separate centre-range constant exists to
  drift out of sync. Centre freedom follows from the size:
  ±(1500 − play_m)/2 per axis (±450 m at 600 m, ±150 m at 1200 m). The margin
  is a product requirement: a hole at the edge of play must still have ground
  running out past it, and the player must never see the world end.

  **Axis-aligned, never rotated.** Deliverable heightmaps are plain grids with
  no transform attached. Rotation would also break the nesting property below,
  and it is redundant anyway — S1 randomizes the grain axis and base-level
  edge, which is the same freedom.
- **Elevations** are metres in a *local datum* — 0 is a nominal core reference
  elevation, not sea level.

**No CRS.** The pipeline has no projection, no geographic coordinates, and no
georeferencing. It is a local Cartesian frame in metres, full stop. Real-world
coordinates appear only in the offline calibration harness, where lidar tiles
are reprojected to UTM before measurement and then discarded — see
[calibration/lidar-pipeline.md](calibration/lidar-pipeline.md). Nothing
downstream of ingestion knows where on Earth anything came from.

## The resolution ladder

Grids use the node convention `nx = extent/res + 1`, so nodes sit exactly on
the box edges and **coarse node sets nest inside fine ones** when the
resolutions divide evenly (8 m nodes coincide with every 4th 2 m node). This
nesting is what makes the ladder cheap: coarsening is subsampling, not
resampling.

| Rung | Constant | Grid | Used by |
|---|---|---|---|
| 8 m | `RES_PREVIEW_M` | 376² | S1, S2 growth phase, S6 cost fields, interactive preview |
| 2 m | `RES_FULL_M` | 1501² | S2 output onward — the canonical deliverable resolution |
| 0.5 m | `RES_EARTHWORKS_M` | local patches | S7, S8 surface space, S9 corridor detail |

Rules:

1. **Every stage is resolution-consistent** — the same code path at any cell
   size, with no magic constants that assume a rung. A stage that produces
   different structure at 8 m than at 2 m is broken, and S11 checks this.
2. **Climb the ladder once.** A stage may coarsen for an internal search and
   refine its result, but the pipeline as a whole goes 8 m → 2 m → 0.5 m and
   never back up.
3. **0.5 m is local only.** A full-extent 0.5 m grid is 6001² ≈ 36 M nodes and
   busts the budget on its own. S7/S8/S9 work in corridor-local patches.

## Determinism

The single hardest requirement in the project: **same seed + same params + same
resolution ⇒ byte-identical artifacts on every platform.**

- One master `seed: u64`. All randomness flows through
  `RunIdentity::stream(name)` with a name registered in
  [`crates/course-seed/src/streams.rs`](../crates/course-seed/src/streams.rs).
  The registry is test-enforced to mirror the table; an unregistered stream
  name is a compile-adjacent failure, not a runtime surprise.
- **No retry-forked RNG.** v2 never retries, so no stream may be advanced
  conditionally on a rejection. Draw counts must be a function of the
  parameters alone, never of the data. A loop like "keep drawing until the
  point lands outside water" is forbidden — draw a fixed count and resolve
  collisions deterministically instead. This is the rule most likely to be
  violated by accident.
- **No wall clock, no `HashMap` iteration order, no platform intrinsics, no
  parallel reduction over floats.** Iterate `BTreeMap`/`Vec`; if a stage
  parallelizes, it must partition into fixed chunks and combine in a fixed
  order.
- **Float discipline.** `libm` for transcendentals (`course-world::math`), not
  `std` — `std`'s are not required to be identically rounded across platforms.
  `serde_json` runs with `float_roundtrip` so f64 artifact JSON round-trips
  byte-identically.
- **Golden hash convention**: FNV-1a over f64 bit patterns,
  `course_world::world::fnv_f64`.

## Artifacts

- Every artifact carries its own version field. A shape change bumps that
  version *and* `PIPELINE_VERSION`, updates the stage doc, and re-blesses
  goldens — an explicit, reviewed event, never incidental.
- **Artifacts are files.** JSON for structure; CGRID1
  (`course-world::gridio`, mirrored in Python by
  `tools/macro_campaign/macro_campaign/cgrid.py`) for per-cell fields. A
  per-course manifest carries blake3 content hashes and the embedded
  `RunIdentity`.
- Artifact JSON is written as compact canonical bytes with loud load
  validation. A malformed or stale artifact fails at load with a specific
  error; it never degrades silently into a default.

## The empty case is legal

Sandhills has no surface water. Great Plains may have no ridge. A biome may run
any module at zero intensity. Every consumer must be correct on empty
collections — no `unwrap()` on "there is always at least one channel", no
division by a count that can be zero. This is a hard requirement, not a
defensive-coding preference, and it is the failure mode most likely to survive
testing on piedmont and die on sandhills.

## Naming

- Crates: `course-<role>`, one per stage plus shared substrate.
- Stage docs: `docs/stages/stage-NN-<slug>.md`, zero-padded.
- Artifacts: `<stage-slug>.json` / `.cgrid`.
- Stream names: `<domain>/<purpose>/v<N>`, e.g. `skeleton/tributary/v1`.
- Dials are named for what they *do*, not for the biome that uses them:
  `drainage_integration`, not `heathland_derangement`. A dial named after a
  biome is a biome branch wearing a disguise.
