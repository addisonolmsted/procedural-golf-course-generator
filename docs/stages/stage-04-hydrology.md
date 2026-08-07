# Stage S4 — Hydrology & Transforms

**Status:** unclaimed
**Crate:** [`crates/course-transforms/`](../../crates/course-transforms/)

## Purpose

Read the finished surface and decide where the water is. Re-derive flow from
the ground as it actually stands after amplification, apply the ordered datum
modifiers, and emit water bodies plus the basin inventory.

S4 runs **after** [amplification](stage-03-amplification.md), deliberately, and
that ordering does real work. Deriving hydrology from the surface that exists —
rather than asserting an authored flow field still describes it — closes the
self-consistency gap between skeleton and ground. It is also why S3 needs no
"may never restructure" rule: if amplification nudges a channel head, S4 simply
measures where it went.

S4 is also where the **future-biome transform seams** live: glacial and
boundary retreat, declared, ordered, and empty. Their presence is the point — a
glacial transform is a modifier over a fluvial substrate, not a different
pipeline, and having the slot already cut is what keeps that true.

## Position

- **Upstream:** [S3](stage-03-amplification.md) — the amplified surface;
  [S2](stage-02-skeleton-kernel.md) — the authored channel network, for the
  agreement check; [S0](stage-00-archetype-draw.md) — `water_table_m` and
  `transforms.*` dials.
- **Downstream:** [S5](stage-05-siting-substrate.md).
- **Contracts:** none directly. Its output becomes C2's `masks.water`,
  `masks.water_surface_m`, `meta.flow_dir_rad`, `meta.flow_accum`, and
  `meta.water_plane_origin`.
- **Streams:** `hydro/v1`.

## Inputs / Outputs

**In:** the amplified 2 m heightfield; S2's channel polylines and divides (for
the agreement check only); `water_table_m`; the ordered transform list.

**Out:**

```
Hydrology {
  hydro_version: u32,
  height:        Field<f64>,        // m, at 2 m -- after datum ops
  flow_dir_rad:  Field<f64>,        // DIRECTION [0,2pi), at 8 m
  flow_accum:    Field<f64>,        // m^2, at 8 m
  water:         Vec<WaterBody>,
  basins:        Vec<Basin>,
  applied:       Vec<TransformId>,  // the ordered list, as run
  skeleton_agreement: f64,          // [0,1] -- derived vs authored network
}

WaterBody {
  polygon: Vec<Vec2>, surface_m: f64,
  origin: WaterPlaneOrigin,         // WaterTable | Floodplain | ClosedBasin
  permanent: bool,
}

Basin { polygon: Vec<Vec2>, lowest_m: f64, closed: bool, area_m2: f64 }
```

## Responsibilities

### 1. Re-derive flow — with flat resolution

Compute flow direction and accumulation from the amplified surface.

**Use Barnes-style flat resolution.** This is not optional and the repo has
already paid for learning it:
`tools/macro_campaign/macro_campaign/flow.py`'s docstring is a direct
indictment of the naive kernel it replaced —

> *"`metrics.core.d8_accumulation` has no flat handling… Together they draw
> dead-straight axis-aligned 'channels' across every lake, pond, sinkhole and
> graded flat, which is what the QA viewer showed on nearly every tile."*

`flow.py` (`receivers()`, `accumulate()`, `drop_to_receiver()`) is the working
reference. Its own tests caught a cycle bug in the first implementation — flat
cells adopting uphill neighbours — so candidate receivers must be at elevation
≤ their own.

### 2. Check agreement with the skeleton

Compare the derived network against S2's authored one and report
`skeleton_agreement`. **This is a diagnostic, not a gate** — nothing branches
on it. A low value means [S3](stage-03-amplification.md)'s near-channel taper
is too weak, which is a calibration finding, and this is how it surfaces.

### 3. Run the modifier list in declared order

Order is data and is recorded in `applied`. Transforms do not commute — a
floodplain datum applied before a water-table intersection gives a different
answer than after — so the order lives in config, not in the implementation.

- **Water-table datum** — intersect the surface with the table at
  `water_table_m` below the local datum. Where the surface falls below, water.
  The mechanism for kettle ponds and any groundwater-fed water.
- **Floodplain datum** — flatten toward the trunk within the flood envelope.
  What makes a river valley floor read as a floor rather than a very wide
  channel.

**Carve the trunk through a flattened floodplain.** Flattening destroys flow
direction in exactly the area that then needs it, and a flat floodplain is
where flow-routing artifacts are worst. The datum must leave the channel
incised.

### 4. Inventory basins

Enumerate basins, closed and open, with areas and low points.
[S5](stage-05-siting-substrate.md) needs the closed ones for `borrow`;
[S7](stage-07-earthmoving.md) needs to know which depressions were **intended**
so its Priority-Flood verify does not repair a kettle pond out of existence.
That is the load-bearing reason the inventory is a first-class output rather
than a byproduct.

### 5. Record water provenance

Each body's `origin` is carried to C2's `meta.water_plane_origin`.
[S10](stage-10-zoning-aesthetics.md) dresses a kettle differently from an
oxbow, and this is how it knows without asking S4.

## Biome expression

| Biome | Transforms |
|---|---|
| **Piedmont** | Light floodplain along the trunk. Water table below the surface almost everywhere — creeks, no lakes. |
| **Great Plains** | Minimal. Shallow floodplain, deep water table. Often produces **zero water bodies**, which is legal and must be handled. |
| **River Valley** | Floodplain dominant — a broad flattened floor with the terrace flight preserved above it. The most transform-heavy biome, and the one where trunk carving matters most. |
| **Sandhills** | **Identity pass.** Runs the ordered list and applies nothing; water table far below the surface, no floodplain. Emits empty `water` and empty closed `basins`. This must work without special-casing. |
| **Heathland** | **Water-table datum dominant.** The table sits near the surface, so closed basins fill: kettle ponds and bogs, the seasonal ones marked `permanent: false`. |
| **Hill Country** | Light floodplain in the incised valleys, deep water table. **Benches untouched** — a datum op that flattens a bench has destroyed the biome. |

## Determinism & budget

**250 ms** at the 2 m rung
([../02-performance-budget.md](../02-performance-budget.md)). Flow routing at
8 m plus per-cell datum ops. Basin extraction is a single connected-component
pass, not a flood-fill per candidate.

**Priority-Flood must break ties on equal elevations deterministically** — the
classic platform-dependence bug in flood-fill implementations, and flat regions
are exactly what a floodplain datum creates. Parallelize per-cell work by fixed
chunks only.

## Future-biome seams

Declared in `seams/`, empty by design:

- **`glacial`** (The North pack) — scouring, drumlins, moraine, kettle-and-kame.
  Needs the ice-flow direction anticipated as a C1 `meta` addition. Sits
  **before** the datum ops.
- **`boundary_retreat`** (Coastal pack) — cliff and shoreline retreat.
  Lowcountry Marsh needs it. Sits **after** the datum ops, since it operates on
  the water boundary the datums produced.

Adding either is: implement the module, add it to the biome's ordered list. No
other stage changes.

## Open questions

1. **Agreement tolerance.** `skeleton_agreement` needs a definition and a
   healthy range. Proposal: fraction of authored channel length within one 8 m
   cell of a derived channel. Below ~0.9, S3's taper needs strengthening.
2. **Where exactly does trunk carving happen** — inside the floodplain datum,
   or as a separate ordered modifier after it? Separate is more honest and more
   testable. Leaning separate.
3. **Are seasonal bodies (`permanent: false`) read downstream?** Heathland is
   the only biome producing them. If nothing consumes the flag, drop it rather
   than carry a lie.
4. **Terrace preservation under the floodplain datum.** River Valley needs the
   floor flattened and terraces untouched, which means an upper bound on the
   datum. Dial or derived from S2's terrace geometry? Leaning derived.
5. **Does S4 re-emit height, or only water?** Datum ops modify height, so yes —
   but that makes S4 the last stage to touch the macro surface, which should be
   stated somewhere prominent.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical output, cross-platform. Priority-Flood
      tie-breaks tested explicitly on equal-elevation plateaus.
- [ ] **Flat resolution works**: on a surface with a large flat (a lake, a
      flattened floodplain), no axis-aligned straight-line channels appear.
      This is a direct regression test against the documented v1 failure.
- [ ] No cycles in the receiver graph — candidate receivers are always at
      elevation ≤ their own.
- [ ] **Sandhills identity pass**: runs the full ordered list, applies nothing,
      emits a valid artifact with empty `water` and empty closed `basins`, no
      special-casing.
- [ ] Great Plains commonly produces zero water bodies without error.
- [ ] `skeleton_agreement` is computed and reported for every run, and is
      **not** consumed as a gate anywhere.
- [ ] Transform order is honoured and recorded; reordering the config
      demonstrably changes the output.
- [ ] Every `WaterBody` carries a correct `origin`; polygons closed,
      non-self-intersecting, positive area; surfaces level.
- [ ] **Trunk stays incised through a flattened floodplain** — flow along the
      trunk remains monotone after the datum.
- [ ] Closed basins survive into the artifact so S7 does not repair them away.
- [ ] Hill Country bench step count is unchanged across S4.
- [ ] The `seams/` modules exist, are ordered, and are empty.
- [ ] Golden output for seed 1, piedmont and heathland.
- [ ] Budget: < 250 ms.
