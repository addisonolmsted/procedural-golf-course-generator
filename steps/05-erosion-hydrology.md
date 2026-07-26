# Step 05 — Erosion & Hydrology

**Status:** unclaimed — **critical path: build first**

## Purpose

Make the terrain geologically coherent and extract its water. Flow-routed
erosion (incision, deposition, diffusion) finishes the composed heightfield;
the resulting flow field is distilled into an explicit hydrographic graph
(streams, lakes, wetlands) that every later step and the game itself consume.

## Position

- Upstream: step 04 (`ComposedHeight`), step 02 (`params.hydro`,
  `hydrology_mode`).
- Downstream: step 06 (moisture proxies), 07 (blockage/crossings), 08 (water
  as hazard/constraint), 09 (don't-dam rule), 10 (re-validation baseline).

## Contract

Input: `ComposedHeight`, `CourseSpec`, `res_m`. Prefer NO randomness (legacy
erosion is RNG-free); if any, `hydro/v1`.

Output — `HydroBundle`:

```
HydroBundle {
  height: grid<f64>,            // post-erosion terrain (supersedes step 04)
  flow: {
    rcv:   [u32],               // D8 receiver per cell; rcv[i]==i ⇒ outlet or retained pit
    order: [u32],               // stable top-down processing order
    zfill: grid<f64>,           // depression-handled elevation used for routing
    flow_area_m2: grid<f64>,    // EFFECTIVE drained area = rainfall×(1−infiltration)-scaled
  },
  graph: {                      // ALL collections may be empty (legal case)
    streams:  [ { pts: [[x,y],…], strahler: u32, width_m: f64 } ],
    lakes:    [ { outline: [[x,y],…], surface_elev_m: f64 } ],
    wetlands: [ { outline: [[x,y],…] } ],
  },
}
```

Erosion recipe (reference implementation exists, see below): priority-flood
depression routing (ε-variant, fixed D8 scan order with documented
tie-breaks), implicit stream-power incision, graded-target deposition with an
exact mass ledger, hillslope diffusion, talus guard.

Mode semantics — parameters, not code branches:

- `fluvial` / `alpine`: full recipe; alpine = high erodibility + talus
  (scarps shed, benches hold).
- `infiltrated`: `runoff ≈ 0` ⇒ negligible incision and an **empty graph** —
  the sandhills case, and it must be exactly this, not a special path.
- `water_table`: build a water-table surface `water_table_depth_m` below the
  terrain reference; cells beneath it are lakes/wetlands; closed basins are
  NOT breached.
- `kettle`: depressions below the `depression_keep` retention threshold are
  preserved (not filled/breached) and marked as retained pits (`rcv[i]==i`).

Extraction: streams where effective area ≥ `channel_threshold_ha`; Strahler
order; hydraulic-geometry widths; lake surface elevations; HAND-style
wetlands.

## Hard requirements

1. Mass ledger balances (eroded = deposited + exported) to stated tolerance.
2. `flow` is self-consistent: following `rcv` from any cell reaches an
   outlet or a retained pit; `order` is a valid topological order; no cycles.
3. Streams' polylines descend monotonically in `zfill`.
4. Sandhills (`infiltration→1`) yields an empty graph — tested explicitly.
5. Deterministic and bit-stable: fixed scan orders, documented tie-breaks,
   no unordered parallelism. Double-run test.
6. Resolution-consistent within stated tolerance (erosion is the hardest
   step for this — state and test what coarse/fine agreement means).

## References

- `main:golf-terrain/src/erosion.rs` — **read first**: the full deterministic
  recipe (Barnes ε priority-flood, Braun–Willett ordering, implicit m=½ n=1
  incision, mass ledger, `fkey` heap trick), 782 lines, proven.
- `main:golf-terrain/src/water.rs` — entity extraction: streams/ponds/lakes/
  wetlands from the flow tree, HAND wetlands, widths, outlines.
- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_hydro` —
  contract shape on naive D8 (no erosion) — what downstream steps currently
  develop against.

## Open questions

- Erosion epochs/iterations at 2 m (1501²): performance budget and whether
  the canonical run needs a coarse-erode → fine-refine ladder.
- Wetland representation: outlines only (current) vs a per-cell moisture
  grid handed to step 06 (richer, bigger artifact).
- Lake outline topology: marching-squares polygons vs cell-set + derived
  outline.
