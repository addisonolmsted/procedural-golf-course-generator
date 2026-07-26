# Step 09 — Hole Earthmoving

**Status:** unclaimed

## Purpose

Do what a course builder does: local, budgeted terrain edits that turn routed
corridors into playable holes — level tee pads, shaped green complexes and
surrounds, bunkers, and fairway softening — without disturbing the terrain
beyond each feature's feathered footprint.

## Position

- Upstream: step 08 (`RoutingPlan`), step 05 (`HydroBundle` — final base
  terrain + water), step 02 (`params.earthworks`).
- Downstream: step 10 audits the composed result; the game renders base +
  patches.

## Contract

Input: `RoutingPlan`, `HydroBundle`, `CourseSpec`. Randomness:
`earthworks/v1` (bunker shapes etc.).

Output — `EarthworksDelta`:

```
EarthworksDelta {
  patches: [ {                    // local grids at 0.5 m
    delta_m: grid<f64>,           // ADDED to base terrain; edges feather to 0
  } ],
  features: [ {
    kind: tee_pad | green_complex | bunker | swale,
    hole: u32,                    // 1-based; 0 = course-wide
    center: [x,y],
    radius_m: f64,
  } ],
}

final_height(p) = base_height(p) + Σ patches covering p   // THE composition rule
```

Per hole (sizes from `params.earthworks`): tee pads leveled to a plane;
green complexes leveled-then-contoured (subtle internal slopes, playable
pinnable areas) with surrounds tied in; bunkers as sand-lined depressions
consistent with step 06's sand class; optional fairway softening within the
corridor.

## Per-archetype behavior (via params + emergent context)

Budgets and feather widths differ (mountain's `cut_fill_budget_m3` is
largest; florida's greens may need raising above the water table — budget
covers it). Bunker styling per archetype is an ART decision keyed off the
archetype id downstream, not a shape decision here.

## Hard requirements

1. **Feather to zero:** every patch's boundary ring is exactly 0 (tested);
   patches tie in smoothly (no cliffs at patch edges at any resolution).
2. **Budget:** Σ |cut| + |fill| ≤ `cut_fill_budget_m3`, reported in the
   artifact.
3. **Never dam:** no patch raises ground across a stream polyline or into a
   lake below its surface elevation (step 10 will catch violations — don't
   create them knowingly).
4. Green surfaces: final slope within playable bounds (state them); tee pads
   level within tolerance.
5. Patches only where routing placed features (no stray edits); overlapping
   patches compose additively and deliberately (surround overlapping green).
6. Deterministic double-run.

## References

- `main:golf-holes/` — v1 build-out: `tee.rs`, `green.rs`, `bunker.rs`,
  `fairway.rs`, `water_fit.rs` (fitting builds around water) — behavior
  reference; types are v1's.
- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_earthworks` —
  feathered level-pad patches, the minimal honest version.

## Open questions

- Green internal contouring: authored templates (library of green pads) vs
  procedural (noise within slope bounds)? Templates likely win for
  playability; decide with the user.
- Do bunkers edit terrain here AND cover (re-classify their cells to sand),
  or does step 06 re-run over the composed terrain? Draft: this step emits a
  cover-patch list alongside height patches — contract addition to resolve
  before build.
