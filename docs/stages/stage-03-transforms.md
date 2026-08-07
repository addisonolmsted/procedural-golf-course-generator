# Stage S3 — Transforms & Datum Ops

**Status:** unclaimed
**Crate:** [`crates/course-transforms/`](../../crates/course-transforms/)

## Purpose

Apply an ordered list of modifiers to the skeleton, and extract water. For the
base six that means two datum operations — water table and floodplain — plus
the basin inventory that S5 masks and S10 dresses.

S3 is also where the **future-biome transform seams** live: glacial and
boundary retreat, declared and ordered and empty. Their presence is the point.
A glacial transform is a modifier over a fluvial substrate, not a different
pipeline, and having the slot already cut is what keeps that true.

## Position

- **Upstream:** [S2](stage-02-skeleton-kernel.md) — `Skeleton`.
- **Downstream:** [S4](stage-04-finishers.md) (height),
  [S5](stage-05-substrate-assembly.md) (water polygons, basin inventory).
- **Contracts:** none directly. Its water output becomes C2's `masks.water`,
  `masks.water_surface_m`, and `meta.water_plane_origin`.
- **Streams:** `transforms/v1`.

## Inputs / Outputs

**In:** `Skeleton`; descriptors `water_table_m`; dials `transforms.*`.

**Out:** `transforms.json` + CGRID1 fields:

```
Transformed {
  transforms_version: u32,
  height:      Field<f64>,          // m, at 2 m -- modified skeleton
  water:       Vec<WaterBody>,
  basins:      Vec<Basin>,
  applied:     Vec<TransformId>,    // the ordered list, as run
}

WaterBody {
  polygon: Vec<Vec2>,
  surface_m: f64,
  origin: WaterPlaneOrigin,   // WaterTable | Floodplain | ClosedBasin
  permanent: bool,
}

Basin { polygon: Vec<Vec2>, lowest_m: f64, closed: bool, area_m2: f64 }
```

## Responsibilities

1. **Run the modifier list in declared order.** Order is part of the config and
   is recorded in `applied`. Transforms are not commutative — a floodplain
   datum applied before versus after a water-table intersection gives different
   answers — so the order is data, not an implementation detail.
2. **Water-table datum.** Intersect the surface with the water table at
   `water_table_m` below the local datum. Where the surface falls below it,
   water. This is the mechanism for kettle ponds and for any biome whose water
   is groundwater-fed rather than channel-fed.
3. **Floodplain datum.** Flatten toward the trunk within the flood envelope,
   using S2's flow distance. This is what makes a river valley floor read as a
   floor rather than as a very wide channel.
4. **Basin inventory.** Enumerate basins, closed and open, with their areas and
   low points. S5 needs the closed ones for `borrow`; S7 needs to know which
   depressions were *intended* so its Priority-Flood verify does not "repair"
   a kettle pond out of existence. **This is the load-bearing reason the basin
   inventory is a first-class output rather than a byproduct.**
5. **Record water provenance** in each `WaterBody.origin`, carried to C2's
   `meta.water_plane_origin`. S10 dresses a kettle differently from an oxbow
   and this is how it knows without asking S3.

## Biome expression

| Biome | Transforms |
|---|---|
| **Piedmont** | Floodplain at low intensity along the trunk. Water table below the surface almost everywhere — creeks, no lakes. |
| **Great Plains** | Minimal. Shallow floodplain; water table deep. Often produces **zero water bodies**, which is legal. |
| **River Valley** | Floodplain dominant — a broad flattened floor with the terrace flight preserved above it. The most transform-heavy biome. |
| **Sandhills** | **Identity pass.** No floodplain (no channels to have one), water table far below the surface. Runs the ordered list and applies nothing. This must work and produce a valid artifact with empty `water`. |
| **Heathland** | Water-table datum dominant: the table sits near the surface, so S2's closed basins fill and become kettle ponds and bogs. `permanent: false` for the seasonal ones. |
| **Hill Country** | Light floodplain in the incised valleys; water table deep. Benches are untouched — a datum op that flattens a bench has destroyed the biome's identity. |

## Determinism & budget

**250 ms** at the 2 m rung
([../02-performance-budget.md](../02-performance-budget.md)). Datum ops are
per-cell and trivially parallel; if parallelized, partition into fixed chunks
and combine in a fixed order
([../01-conventions.md](../01-conventions.md)).

Basin extraction is the non-trivial cost. Use a single connected-component pass
over the 8 m grid, not a flood-fill per candidate.

## Future-biome seams

Declared in `seams/`, empty by design:

- **`glacial`** (The North pack) — scouring, drumlin fields, moraine
  deposition, kettle-and-kame. Needs the ice-flow direction anticipated as a C1
  `meta` addition. Sits *before* the datum ops in the order.
- **`boundary_retreat`** (Coastal pack) — cliff and shoreline retreat.
  Lowcountry Marsh will need it. Sits *after* the datum ops, since it operates
  on the water boundary the datums produced.

Adding either is: implement the module, add it to the biome's ordered list.
No other stage changes.

## Open questions

1. **Should transforms be able to modify the flow field, or only height?**
   Height-only is simpler and keeps S2's flow field authoritative, but a
   floodplain datum genuinely does change where water goes. Proposal:
   height-only, and S5 recomputes flow on the final surface. Needs confirming —
   it affects whether S3 can be a pure per-cell pass.
2. **Are seasonal water bodies (`permanent: false`) modelled at all downstream?**
   Heathland's bogs are seasonal. If nothing downstream reads the flag, drop
   it rather than carry a lie.
3. **Terrace preservation under the floodplain datum.** River Valley needs the
   floodplain flattened and the terraces above it untouched, which means the
   datum needs an upper bound. Is that bound a dial or derived from the terrace
   flight? Leaning derived from S2's output.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical output, cross-platform.
- [ ] **Sandhills identity pass**: runs the full ordered list, applies nothing,
      emits a valid artifact with empty `water` and empty closed `basins`. No
      special-casing to achieve it.
- [ ] Great Plains commonly produces zero water bodies without error.
- [ ] Transform order is honoured and recorded in `applied`; reordering the
      config demonstrably changes the output.
- [ ] Every `WaterBody` carries a correct `origin`; polygons are closed,
      non-self-intersecting, and have positive area.
- [ ] Water surfaces are level: every cell under a body has the same
      `surface_m`.
- [ ] Basin inventory distinguishes closed from open basins, and closed basins
      survive into the artifact so S7 does not later repair them away.
- [ ] Hill Country benches are preserved: bench step count before and after S3
      is unchanged.
- [ ] The `seams/` modules exist, are ordered, and are empty.
- [ ] Golden output for seed 1, piedmont and heathland.
- [ ] Budget: < 250 ms.
