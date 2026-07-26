# Step 10 — Drainage Validation & Repair

**Status:** unclaimed

## Purpose

The final audit: prove that water still behaves after earthmoving. Re-run
flow on the composed terrain, find new ponding on playing surfaces or water
impounded against earthworks, repair with bounded swale/drain patches, and
emit the final course bundle. A course that passes step 10 is done.

## Position

- Upstream: steps 05 (pre-earthworks flow + hydro graph), 08 (playing
  surfaces), 09 (patches; composed terrain = base + patches).
- Downstream: the game (final bundle); the batch harness (pass rates).

## Contract

Input: `HydroBundle`, `RoutingPlan`, `EarthworksDelta`, `CourseSpec`. No
randomness (repairs are deterministic consequences of findings).

Output — `DrainageReport` (+ the assembled `CourseBundle`):

```
DrainageReport {
  pass: bool,
  metrics: { name → f64 },        // e.g. corridor_pit_cells, impounded_area_m2,
                                  //      repair_volume_m3, iterations
  repair_patches: [ { delta_m: grid<f64> } ],   // same rules as step 09 patches,
                                  // applied AFTER them in the composition
  notes: [string],
}

CourseBundle {                    // the game-facing hand-off (assembled here)
  spec, final terrain composition (base + step-09 + repairs),
  cover, hydro graph, routing plan, features, all content-hashed
}
```

Audit checks: (a) new pits (cells with no lower neighbor) inside corridors,
greens, tees that were not retained pits in step 05; (b) flow impounded
against any earthworks patch (dammed streams); (c) green/tee surfaces shed
water (positive outward gradient at their edges).

Repair: carve minimal swales from each offending pit to the nearest
step-05 drainage line or lake, as feathered 0.5 m patches obeying step 09's
patch rules; re-audit; bounded iterations (suggest 3) — if still failing,
`pass=false` with full findings (upstream bug, do not paper over).

## Per-archetype behavior

Mode-aware pit judgment: retained kettles (`kettle`) and water-table lakes
(`water_table`) are NOT defects even inside a corridor margin; sandhills
(`infiltrated`) treats small closed pits as legal sandy hollows below a
volume threshold from `params.hydro.depression_keep`.

## Hard requirements

1. Audit runs on the true composition (base + step-09 patches + prior
   repairs) at the audit resolution; composition rule identical to step 09's.
2. Repairs obey every step-09 patch rule (feather, budget accounting —
   repair volume counts against a stated reserve).
3. Never "repairs" step-05 water features themselves (no draining lakes).
4. `pass=false` is a legal, informative outcome; metrics always populated.
5. Deterministic double-run; bounded runtime (fixed iteration cap).

## References

- `main:golf-terrain/src/erosion.rs` — the flow re-routing needed for the
  audit (same recipe as step 05's router).
- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_drainage` —
  audit-only stand-in (pit census in corridors).

## Open questions

- Audit resolution: 0.5 m within corridors + 2 m elsewhere (mixed), or
  uniform 2 m with 0.5 m only under features? Cost/fidelity call.
- Should repairs be allowed to add *subsurface* drains (invisible, just
  removes water in the model) for archetypes where open swales look wrong
  (florida)? Art-adjacent; decide with the user.
- Exact `CourseBundle` file layout — finalize when the game-side consumer
  exists.
