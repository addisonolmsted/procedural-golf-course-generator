# Step 03 — Macro Landform

**Status:** unclaimed

## Purpose

Author the large-scale terrain skeleton the whole course is organized around:
the base heightfield (before noise/erosion), the conditioning fields that
steer downstream steps, and the structural graph (drainage spines, ridge
lines, bench edges). This is where the routable core is won or lost — macro
structure is placed so the core stays playable per archetype.

## Position

- Upstream: step 02 (`CourseSpec`, reads `params.landform`).
- Downstream: step 04 (conditions noise on the fields), step 05 (conditions
  erosion on drain spines), steps 07/08 (read the structure graph for
  corridor hints).

## Contract

Input: `CourseSpec`, `res_m`.

Output — `MacroSkeleton`:

```
MacroSkeleton {
  base_height: grid<f64>,          // metres, full 3 km box at res_m
  fields: {                        // all same grid spec, all [0,1] unless noted
    floor_damp:    grid<f64>,      // 1 on drainage floors → 0 by ~150 m out
    slope_gain:    grid<f64>,      // structurally steep ground
    grain_dir_rad: grid<f64>,      // local structural grain (radians, mod π)
    valley_dist_m: grid<f64>,      // metres to nearest drain spine
    core_protect:  grid<f64>,      // 1 in routable core, smooth shoulder out
  },
  structure: {
    spines: [ { kind: drain | ridge_line | bench_edge,
                pts: [[x,y], …] } ],   // world coords; drains ordered downstream
  },
}
```

Primitive vocabulary (drawn per archetype from `params.landform`): regional
tilt, ridges, drainage spines (valleys), bench/terrace systems with scarps,
closed basins (kettles / future lakes), dune trains (wavelength only — the
carrier waves themselves belong to step 04). Randomness: `macro/place/v1`.

## Per-archetype behavior (via params, never branches)

- `sandhills`: dune-train grain direction + blowout basins; no drain spines
  needed (valley_count may be 0).
- `piedmont`: 1–3 drain spines crossing the box (the creek corridor), rolling
  interfluves.
- `florida_lowland`: near-flat plane, basin field (lakes-to-be), tilt ~0.
- `glacial_moraine`: many closed basins + short ridges; drainage may be
  entirely internal (spines optional).
- `mountain_bench`: dominant relief with bench/scarp systems; at least one
  bench system oriented THROUGH the core — this is mountain's gate-pass
  lever.

## Hard requirements

1. **Monotone drains:** `base_height` sampled along any `drain` spine falls
   monotonically; tributary junctions are accordant (equal floor elevation at
   the junction). Unless the config is closed-basin-only, at least one drain
   exits the box.
2. **Core cap:** relief inside the routable core ≤
   `params.landform.core_relief_cap_m`.
3. Fields are consistent with the geometry they summarize (e.g. `floor_damp`
   ≈ 1 exactly where a drain spine's carve is deepest; `valley_dist_m`
   matches distance to the emitted spines).
4. Resolution-consistent: coarse and fine rasters of the same config agree
   (coarse is a downsample, not a different terrain).
5. Deterministic double-run.

## References

- `terrain-v2:golf-landform/src/valley.rs` — monotone swept-profile valleys,
  accordant junctions, minimum-fall gradients (the hard part, solved).
- `terrain-v2:golf-landform/src/{bluff.rs,bowl.rs,presets.rs}` — scarps,
  basins with spillway/lake outlets, and a primitive vocabulary.
- `archetype-pipeline:course-contracts/src/fixtures.rs::fixture_skeleton` —
  a minimal contract-faithful example.

## Open questions

- Should the analytic primitive config itself be part of the artifact (for
  hand-editing in the lab), or is the rasterized output the whole contract?
- Bench geometry: uniform tread grade vs terraced quantization — pick after
  first mountain renders.
