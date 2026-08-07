# Contract C2 — The Routing Substrate

**Between:** [S5 substrate assembly](../stages/stage-05-substrate-assembly.md) →
[S6 routing](../stages/stage-06-routing.md)
**Type:** `course_contracts::contracts::routing_substrate::RoutingSubstrate`
**Status:** guarded, and the most tightly guarded of the three. Golden-seed
tested on both archetypes. Changes bump `C2_VERSION` and `PIPELINE_VERSION`.

## Why this contract exists

C2 is the **waist of the pipeline**. Everything above it is landscape
generation; everything below it is golf. The router must be able to do its job
knowing nothing whatsoever about how the terrain was made — not the biome, not
the kernel, not which modules ran, not whether a lake is a kettle or an oxbow.

The test of whether C2 is right is not whether it works for piedmont. It is
whether a kernel nobody has written yet can populate it and get a sensible
course out. **Lowcountry Marsh** (Coastal pack) is the designated adversary:
near-zero relief, water as the dominant structure, routability determined
almost entirely by what is *not* water. When it lands it will be C2's real
exam. Design for it now.

The corollary, and it is a strong one: **if S6 ever needs to ask a question C2
cannot answer, the answer is to add a field to C2 — never to let S6 reach
upstream.** A single import of `course_skeleton` from `course_routing` destroys
the contract.

## Shape

```
RoutingSubstrate {
  c2_version: u32,
  seed: u64,
  spec_digest: [u8; 32],

  grid_full:  GridSpec,          // 2 m rung, 1501^2 -- the height field
  grid_cost:  GridSpec,          // 8 m rung, 376^2  -- all cost/mask fields

  height:     Field<f64>,        // m, at grid_full

  masks:      ExclusionMasks,    // at grid_cost
  cost:       CostFields,        // at grid_cost
  meta:       SubstrateMeta,
  preset:     ScorerPreset,
}
```

Two rungs, deliberately. The router searches on the 8 m cost grid — 16× cheaper
and the right scale for a golf hole — but hole geometry is realized against the
2 m height field. The rungs nest ([../01-conventions.md](../01-conventions.md)),
so an 8 m cost cell corresponds to exactly 16 height nodes with no resampling.

## `height`

The 2 m heightfield, in metres in the local datum, after S4's finishers. This
is the ground as it stands *before any earthmoving*.

Invariants: no NaN, no holes, defined at every node. Under water bodies it
carries the **bed** elevation, not the water surface — the water surface is in
`masks.water_surface_m`. Getting this backwards makes every lake a plateau and
is the most likely single implementation bug in S5.

## `masks: ExclusionMasks`

All at `grid_cost` (8 m). Each is a per-cell field the router reads directly.

```
ExclusionMasks {
  water:            Field<f64>,   // [0,1] fractional coverage per cell
  water_surface_m:  Field<f64>,   // m; meaningful only where water > 0
  protected:        Field<bool>,  // may not be routed through or graded
  out_of_bounds:    Field<bool>,  // outside the core, or otherwise unusable
  unbuildable:      Field<f64>,   // [0,1] soft -- slope/material infeasibility
}
```

**`water`** is *fractional*, not boolean, because an 8 m cell straddling a
creek bank is genuinely half water and rounding it either way produces either
impassable creeks or invisible ones. The router treats it as a cost multiplier
below a threshold and as exclusion above one; the threshold is in `preset`.

**`protected`** and **`out_of_bounds`** are the only hard booleans in C2.
Everything else is soft. This is a deliberate design rule: **hard constraints
are the mechanism that forces retries**, and v2 has no retries. Two hard masks
is the irreducible minimum, and both are known-good by construction (the core
window, and features S3 marked untouchable) rather than derived from anything
that could go wrong.

**`unbuildable`** is soft precisely because it is the one that *would* have been
hard. A 40% slope is not impossible — it is expensive, and at high plasticity
it is affordable. Encoding it as cost rather than exclusion is what lets
plasticity work at all.

## `cost: CostFields`

All at `grid_cost` (8 m). Non-negative, unbounded above, in comparable units so
the scorer can combine them linearly.

```
CostFields {
  earthwork:      Field<f64>,   // m^3 per cell to make it playable
  clearing:       Field<f64>,   // m^3-equivalent to clear cover
  borrow:         Field<f64>,   // m^3 of fill obtainable from this cell
  drainage_risk:  Field<f64>,   // dimensionless; ponding/erosion exposure
}
```

**`earthwork`** — cut-plus-fill volume in m³ to bring the cell to a playable
grade, computed against the biome's target grades. This is the field
`plasticity` scales against: the router's willingness to pay it is what
plasticity *means*.

**`clearing`** — cover removal cost, expressed as an m³-equivalent so it can be
summed with `earthwork` without a unit conversion in the scorer. Zero on open
biomes; dominant on closed-canopy ones.

**`borrow`** — how much fill can be *taken* from this cell cheaply, typically
because a lake or pond wants to be there anyway. This is the field that makes
S7's mass-balance rule solvable: without it the router routes a course that
needs 40 000 m³ of fill from nowhere. Note it is a *resource*, not a cost —
the scorer subtracts it. Do not sign-flip it into the cost sum by accident.

**`drainage_risk`** — dimensionless exposure to ponding or erosion. Feeds S6's
penalties and, unusually, is also read directly by S7 to prioritize its single
template pass. It is the only cost field with a consumer past S6, and that is
recorded here so a future edit does not treat it as router-private.

## `meta: SubstrateMeta`

```
SubstrateMeta {
  flow_dir_rad:     Field<f64>,   // DIRECTION [0,2pi), at grid_cost
  flow_accum:       Field<f64>,   // upstream contributing area, m^2
  grain_axis_rad:   f64,          // AXIS [0,pi) -- from C1, unchanged
  wind_azimuth_rad: f64,          // DIRECTION [0,2pi) -- from C1, unchanged
  water_plane_origin: WaterPlaneOrigin,  // provenance: how water was decided
  plasticity:       f64,          // [0,1], carried from S0
}
```

This is the **provenance thread**. `grain_axis_rad` and `wind_azimuth_rad` pass
through from C1 byte-identically — S5 may not recompute or refine them. That
identity is what keeps S9's micro detail aligned with S2's macro structure nine
stages later; if the axis drifts anywhere along the chain, the terrain stops
reading as one place and no single stage looks wrong.

`water_plane_origin` records *why* water is where it is (water-table
intersection, floodplain datum, closed basin, or none). S10 dresses a kettle
pond differently from an oxbow, and this is how it knows without asking S3.

`plasticity` is duplicated here rather than re-read from the spec so that C2 is
genuinely self-sufficient — a fixture C2 loaded from disk is a complete input
to S6 with no other file required. That self-sufficiency is what makes S6
testable before S5 exists.

## `preset: ScorerPreset`

The biome's routing weights, carried as data:

```
ScorerPreset {
  w_earthwork, w_clearing, w_drainage, w_fit: f64,   // scorer weights
  water_exclude_threshold: f64,                      // [0,1] on masks.water
  target_grade_fairway, target_grade_green: f64,     // rise/run
  hole_length_m: [f64; 9],                           // target lengths
  feasibility_strictness: f64,                       // [0,1], plasticity-derived
}
```

S6 contains **no biome knowledge whatsoever** — every number it needs to behave
differently on sandhills than on heathland arrives in this struct. If a router
change ever wants a `match biome`, the fix is a new preset field.

`hole_length_m` is 9 entries because the course is a 9-hole par-36 round;
`PAR_SEQUENCE` in `course-spec` is the authority on par.

## Invariants

1. **Self-sufficient.** A `RoutingSubstrate` loaded from disk is a complete
   input to S6. No other artifact is required. This is testable and S6's
   fixtures depend on it.
2. **No upstream imports.** `course-routing` depends on `course-contracts` and
   `course-world` only. Never on `course-skeleton`, `course-transforms`,
   `course-finishers`, or `course-spec`. This is checkable in CI from the
   dependency graph and should be.
3. **Two hard masks, everything else soft.** `protected` and `out_of_bounds`
   are the only fields that can make a cell impossible.
4. **Costs are non-negative and finite.** No infinities — an infinity is a hard
   constraint smuggled into a soft field, and it reintroduces the failure mode
   that requires retries.
5. **Rungs nest.** `grid_cost` is exactly `grid_full` coarsened 4×; an 8 m cost
   cell maps to 16 height nodes with no interpolation.
6. **Metadata passes through unchanged.** `grain_axis_rad` and
   `wind_azimuth_rad` are bit-identical to their C1 values.
7. **The empty case is legal.** `water` may be identically zero (Sandhills);
   `borrow` may be identically zero; `clearing` may be identically zero. S6
   must route all three.
8. **Bed, not surface.** `height` under water is the bed.

## Versioning

`C2_VERSION` bumps on any field addition, removal, unit change, range change,
or rung change — including additions to `ScorerPreset`, since a preset field
the router ignores is a silent behaviour change on every biome.

Because C2 is the waist, a C2 bump is the most expensive change in the project:
it re-blesses every golden downstream of S5. Batch C2 changes; do not drip them.

## Future-biome seams

- **Lowcountry Marsh** (the adversary): tests whether `water` as a fractional
  field plus `water_surface_m` is enough to express tidal marsh, where the
  boundary is genuinely fuzzy and moves. If it is not, the likely addition is a
  `water_permanence: Field<f64>` — noted now so the shape is anticipated.
- **Desert:** needs no new fields; runs `clearing` at zero and `unbuildable`
  high.
- **Highlands / Fire & Stone:** stress `unbuildable` and `earthwork` into
  ranges the base six never reach. No new fields expected, but the scorer
  weights will need a much wider certified envelope.
