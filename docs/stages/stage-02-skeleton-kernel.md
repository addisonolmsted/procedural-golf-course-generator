# Stage S2 — Skeleton Kernel †

**Status:** unclaimed
**Crate:** [`crates/course-skeleton/`](../../crates/course-skeleton/)

## Purpose

Build the **structure**: a hierarchical drainage network, the divides it
implies, a flow-distance field, and a smooth catena base surface hung off them.
All six base biomes are served by **one fluvial engine** plus five modules.

S2 *authors* structure rather than simulating it. The previous authored
generator failed because it authored in **landform-primitive space** — place a
ridge, hope the statistics follow. S2 authors in **flow-distance / drainage
space**, which produces network statistics by construction. See
[../00-architecture.md](../00-architecture.md).

### What S2 is not responsible for

**Archetype identity.** This is a deliberate narrowing, and it is
evidence-driven: `tools/macro_campaign/macro_campaign/structure.py` measures
`dist_to_channel_p50` at **104–120 m in every archetype regardless of relief**,
and flags it as the most robust metric in the set. Drainage spacing is a
**shared invariant** of real landscapes, not a signature — a sandhills tile and
a hill-country tile have nearly the same channel spacing.

So the network must be *right*, but getting it right does not make a course
look like anywhere in particular. Identity lives in hillslope form and texture,
which is [S3](stage-03-amplification.md)'s job.

This also relieves the catena of a burden it could not carry. Elevation as
`f(flow_distance, hillslope_position)` gives every point equidistant from a
channel the same treatment — smooth, tubular interfluves, the characteristic
"candle wax" signature of hydrology-first synthesis. **That is now expected and
fine.** S2's output is meant to be a correct, characterless base; S3 supplies
the spur-and-hollow structure that no distance transform can.

## Position

- **Upstream:** [S1](stage-01-macro-primitives.md) via
  **[C1](../contracts/C1-primitives-to-kernel.md)**; `CourseSpec` dials.
- **Downstream:** [S3](stage-03-amplification.md), which reads the base surface
  *and* the skeleton geometry — the network is S3's conditioning coordinate,
  not just a thing it decorates around.
- **Contracts:** consumes C1. Emits no guarded contract — the base surface and
  the skeleton travel to S3 as an internal artifact.
- **Streams:** `skeleton/trunk/v1`, `skeleton/tributary/v1`,
  `skeleton/module/v1`.

## Inputs / Outputs

**In:** `PrimitiveField` (C1), including the discontinuity list; descriptors
`relief_budget_m`, `density_target`; dials `skeleton.*` including the five
module intensities.

**Out:** `skeleton.json` + CGRID1 fields:

```
Skeleton {
  skeleton_version: u32,
  kernel: KernelId,                 // I (fluvial) for all base six
  height:            Field<f64>,    // m, at 2 m -- the BASE surface
  flow_dir_rad:      Field<f64>,    // DIRECTION [0,2pi), at 8 m
  flow_accum:        Field<f64>,    // m^2, at 8 m
  flow_distance:     Field<f64>,    // m to nearest channel, at 8 m
  flow_distance_norm: Field<f64>,   // [0,1] channel -> divide, at 8 m
  hillslope_position: Field<f64>,   // [0,1] relative elevation, at 8 m
  channels: Vec<Channel>,           // with Strahler order
  divides:  Vec<Polyline>,          // derived, not authored
  meta: StructureMeta,              // passed through from C1, unchanged
}
```

`flow_distance_norm` and `hillslope_position` exist for
[S3](stage-03-amplification.md): they are two of its six conditioning fields,
and computing them here — where the network is known exactly — is cheaper and
more accurate than re-deriving them downstream.

## Responsibilities

### The fluvial engine (Kernel I), in order

1. **Trunk splines.** Root the trunk at `meta.base_level` and grow it upstream,
   biased by C1's `relief` and `accommodation`. This is the drainage backbone
   everything else hangs from.
2. **Hierarchical tributary growth.** Grow tributaries as a **Horton–Strahler
   network**, not a flat trunk-plus-branches set: order-1 streams feeding
   order-2 feeding the trunk, with the bifurcation and length ratios real
   networks obey (bifurcation typically 3–5, length ratio ~2). Spacing is set
   by `density_target`; deflection is biased by `hardness`, elongated along
   `grain_axis_rad`, and **deflected or truncated by C1's discontinuities**.

   Hierarchy matters for realism in a way a two-level network cannot fake — a
   landscape whose every tributary is the same size reads wrong even when its
   density is exactly right. `tools/macro_campaign/macro_campaign/netstats.py`
   already implements `_strahler()`, junction spacing/angle statistics, and the
   pivot-centred log-log fits, so the targets are measurable with existing code.
3. **Flow-distance transform.** Compute distance-to-nearest-channel over the
   grid, plus the normalized channel→divide coordinate and hillslope position.
   These are the coordinates the catena is expressed in, and the conditioning
   coordinates S3 reconstructs against.
4. **Derived divides.** Extract divides *from the flow field*, never author
   them. An authored divide that disagrees with the flow field is the exact
   failure mode of the retired generator.
5. **Catena base elevation.** Assign elevation as a function of flow distance
   and hillslope position, from a calibrated profile family. The profile law's
   exponent θ is a fit target
   ([../calibration/metric-battery.md](../calibration/metric-battery.md)).

   **Keep this simple.** The catena's job is a correct mean surface, not a
   characterful one — it should hit slope–area scaling and the hypsometric
   integral and stop. Effort spent making it look good is effort spent
   competing with [S3](stage-03-amplification.md), which has real data and will
   win. Earlier drafts specified a rich sampled "catena library"; a parametric
   family is now the right call.

### The five modules

Each is a **dial, not a branch**. A biome that does not use one runs it at zero
intensity; the code path is identical.

| Module | What it does | Primary biomes |
|---|---|---|
| **Trunk-river** | Floodplain, terrace flight, oxbow/scroll stamps | River Valley (dominant), Piedmont (low) |
| **Stratigraphy / bench** | Differential resistance from the strata stack ⇒ benches and scarps instead of smooth slopes | Hill Country, Great Plains, River Valley (shared) |
| **Closed basin** | Closed-basin tolerance + a point-process embryo that seeds basins | Heathland (kettles) |
| **Aeolian** | Micro-band hook oriented by `wind_azimuth_rad`; carries the **S1/S2 amplitude fork** for dune scale | Sandhills |
| **Drainage integration** | How completely the network connects. Run **negative** for a deranged, poorly-integrated network | Heathland (~0.07 density) |

## Biome expression

| Biome | Kernel + module configuration |
|---|---|
| **Piedmont** | Reference case. Mid density, trunk-river low, stratigraphy thin. Everything else zero. The primary golden. |
| **Great Plains** | Low density, very low relief, stratigraphy present (the caprock that keeps it flat rather than smooth). Integration high. |
| **River Valley** | Trunk-river dominant: broad floodplain, full terrace flight, scroll stamps. Stratigraphy in the valley sides. Density mid-high. |
| **Sandhills** | Aeolian module dominant with the S1/S2 amplitude fork; density ≈ 0 — **no surface channels at all**. The engine still runs and still computes a flow field; it simply grows no channels. Integration effectively meaningless. |
| **Heathland** | The most-different base biome and the adversarial golden. Drainage integration **negative**, density ~0.07 — a deranged network that does not reach the base level. Closed-basin module dominant (kettles). Strata empty. |
| **Hill Country** | Stratigraphy dominant: thick stack, strong benching, high relief. Density mid; the channels are incised, not broad. |

Note that Sandhills and Heathland break the engine's central assumption
(integrated drainage) from opposite directions. If both work, the engine is
sound.

## Determinism & budget

**900 ms** — the largest pre-routing cost
([../02-performance-budget.md](../02-performance-budget.md)). Growth phases run
at 8 m; the catena raster is written at 2 m. The flow-distance transform
dominates; use a multi-source BFS/Dijkstra over the 8 m grid, not a per-node
search.

Fixed iteration counts on tributary growth — **never grow until a density
target is met**, which is a data-dependent loop and a budget hazard. Draw a
fixed number of tributaries and let the achieved density be what it is; the
envelope guarantees it lands near the target.

## Calibration †

S2 is fitted almost entirely to **shared invariants** — quantities every real
archetype hits at nearly the same value. That is unusual and it is the point:
S2 must be *correct* rather than *distinctive*.

| Dial | Metric it is fit to | Class |
|---|---|---|
| Tributary spacing | `drainage_density`, `dist_to_channel_p50` | **shared invariant** |
| Bifurcation / length ratios | Horton ratios, junction spacing (`netstats.py`) | **shared invariant** |
| Profile-law exponent θ | slope–area scaling | shared invariant |
| Catena form | hypsometric integral, slope distribution | process |
| Drainage integration | `network_connectivity` | **discriminant** (heathland) |
| Relief amplitude | local relief, elevation range | amplitude |

Only `network_connectivity` genuinely discriminates, and only for heathland.
Everything else S2 does should come out looking much the same across the six —
if drainage density differs measurably between generated piedmont and generated
sandhills, something is wrong, because it does not differ between the real
ones.

Classes are defined in
[../calibration/metric-battery.md](../calibration/metric-battery.md); targets in
[../calibration/targets.md](../calibration/targets.md).

**Tile data required:** 3DEP DTM tiles for all six biomes, ~30+ per biome.
Currently available only for the retired archetype set. **Three of the six
biomes have no corpus.** See
[../calibration/lidar-pipeline.md](../calibration/lidar-pipeline.md).

## Future-biome seams

`kernel::SkeletonKernel` is *the* future-biome seam. Kernels II–V (glacial,
aeolian-dominant, karst, volcanic) implement the same trait, consume the same
C1, and produce the same `Skeleton`. Nothing outside this crate changes.

Design the trait so a kernel that grows no channels is natural rather than a
special case — Sandhills already exercises that path within Kernel I, which is
useful validation of the trait's shape before an external kernel needs it.

## Open questions

1. **Does negative integration need its own code path?** Heathland's deranged
   network may not be expressible as "the same growth with a dial turned down".
   If it needs different growth, that is a second kernel wearing a module's
   clothes and should be admitted as such. **The largest open architectural
   risk in the base six**, which is why heathland is second in the corpus
   collection order.
2. **How does a discontinuity act on the network?** C1 can now carry a scarp,
   valley wall, or material contact. Real ones deflect channels, truncate them,
   or nucleate new ones along the base — differently per kind. Needs
   specifying per `boundary_kind`. Mirrors
   [S1's open question 4](stage-01-macro-primitives.md).
3. **Horton ratio enforcement.** Fixed-count growth cannot guarantee a target
   bifurcation ratio. Grow to a fixed count and report the achieved ratio, or
   construct the hierarchy top-down so ratios hold by construction? Leaning
   top-down — it makes the invariant structural rather than hoped-for.
4. **Where does the S1/S2 amplitude fork live?** The dune amplitude split
   currently spans both stages. With S3 owning texture, the fork may collapse
   entirely: S1 keeps the dune-train envelope, S3 keeps the dune form, and S2's
   aeolian module does little. Confirm against the sandhills corpus.
5. **How is achieved density reported?** If fixed-count growth systematically
   misses `density_target`, the envelope is wrong. S2 should emit achieved
   density and achieved Horton ratios as diagnostics so calibration can close
   the loop.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical skeleton, cross-platform.
- [ ] All six biomes produce a skeleton; **Sandhills produces zero channels and
      Heathland a disconnected network, and both are handled without special
      cases** (no `if biome` anywhere in the crate — grep it).
- [ ] Divides are derived from the flow field: no divide crosses a channel, and
      flow on either side of a divide diverges.
- [ ] Flow field is consistent with the heightfield: every channel cell's
      `flow_dir_rad` points downhill on `height`.
- [ ] `meta` is bit-identical to C1's `meta` (assert equality directly).
- [ ] Fixed draw and iteration counts, asserted — no data-dependent growth.
- [ ] **The network is hierarchical**: Strahler order ≥ 3 on integrated biomes,
      with bifurcation and length ratios inside the measured bands. A flat
      trunk-plus-branches network fails this even at correct density.
- [ ] Measured `drainage_density` and `dist_to_channel_p50` land within the
      **shared-invariant** band — and, as a cross-check, do **not** differ
      significantly between generated biomes, because they do not differ
      between real ones.
- [ ] `network_connectivity` separates heathland from the other five.
- [ ] **Discontinuities act on the network**: where C1 carries a boundary, the
      network is measurably deflected or truncated by it rather than crossing
      it as if it were not there.
- [ ] `flow_distance_norm` and `hillslope_position` are defined everywhere, in
      `[0, 1]`, and correct at the degenerate case (Sandhills, no channels).
- [ ] Resolution consistency: 8 m and 2 m runs produce the same channel network.
- [ ] Golden skeleton for seed 1, piedmont and heathland.
- [ ] Budget: < 900 ms.
