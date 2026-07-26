# Step 06 — Cover Assignment

**Status:** unclaimed

## Purpose

Classify every cell's ground cover and canopy density from terrain + water +
archetype parameters. The (cover class, archetype) pair is the key the art
layer will use; cover also defines what ground is routable for steps 07–08.

## Position

- Upstream: step 05 (`HydroBundle`), step 02 (`params.cover`).
- Downstream: step 07 (routable mask, blockage), step 08 (corridor cost),
  the game's art layer.

## Contract

Input: `HydroBundle`, `CourseSpec`, `res_m`. Randomness: `cover/clump/v1`.

Output — `CoverMap`:

```
CoverMap {
  class:  grid<u8>,    // enum below
  canopy: grid<f64>,   // tree density 0..1, independent of class
}

class ∈ { turf=0, rough=1, sand=2, wetland=3, water=4, rock=5, forest=6 }
routable(class) = class ∈ { turf, rough, sand }     // THE definition steps 07/08 use
```

Assignment drivers: moisture proxies from the flow model (HAND, water
adjacency), slope, curvature, plus archetype propensities (`canopy_frac`,
`sand_exposure`, `wetland_bias`) clumped at `clump_scale_m` so cover forms
mosaics, not per-cell speckle.

Core rule: the routable core must end up more open than the outer ring
(canopy/wetland propensity reduced inside the core) — this step owns the
cover half of keeping the core routable.

## Per-archetype behavior (via params)

- `sandhills`: 25–60% exposed sand/waste, open canopy, no wetland.
- `piedmont`: 50–80% canopy, wetland fringes along creeks.
- `florida_lowland`: heavy wetland fringe around lakes, mid canopy.
- `glacial_moraine`: open heathland, kettle-bog wetlands, some sand.
- `mountain_bench`: rock above a slope threshold, forest on mid slopes,
  open benches.

## Hard requirements

1. Water cells in `class` agree exactly with step 05's lakes/streams
   footprint (no dry lakes, no water where the graph has none).
2. Empty hydro graph (sandhills) ⇒ zero water/wetland cells, and the step
   still produces a full, sensible map.
3. Class mosaics are stable under resolution change (same mosaic, finer
   edges) and deterministic double-run.
4. Achieved cover fractions land within stated tolerance of the params that
   requested them (canopy_frac, sand_exposure) — measured and reported in
   the artifact's metadata.

## References

- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_cover` —
  minimal rule set with clumping.
- `terrain-v2:tools/dtm_atlas` masks + `assets/atlas.bin` tree/water masks —
  ground truth for canopy/water fractions per real course (calibration
  targets for this step).
- `main:golf-terrain/src/water.rs` HAND computation — the moisture proxy.

## Open questions

- Do fairway-candidate zones (turf) get decided here or by step 08 after
  routing (rough-only here)? Current draft: `turf` is a candidacy hint, step
  09 finalizes.
- Should canopy be a density grid only (current) or also carry a specimen
  tree list for the art layer?
