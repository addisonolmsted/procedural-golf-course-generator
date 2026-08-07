# Stage S2 — Skeleton Kernel †

**Status:** unclaimed
**Crate:** [`crates/course-skeleton/`](../../crates/course-skeleton/)

## Purpose

Build the structural heightfield. S2 is where the terrain actually gets made:
all six base biomes are served by **one fluvial engine** plus five modules that
dial in identity.

S2 *authors* structure. This is a deliberate reversal of the previous
architecture, where a landscape-evolution model was the co-author — see
[../00-architecture.md](../00-architecture.md) for the reasoning and for what is
being traded away. The short version: the previous authored generator failed
because it authored in **landform-primitive space** (place a ridge, hope the
statistics follow). S2 authors in **flow-distance / drainage space**, which
produces drainage-network statistics by construction.

**S2 is the bet this architecture rests on.** If the fluvial engine cannot hit
the process metrics in [../calibration/targets.md](../calibration/targets.md),
the bet was wrong and the simulation has to come back.

## Position

- **Upstream:** [S1](stage-01-macro-primitives.md) via
  **[C1](../contracts/C1-primitives-to-kernel.md)**; `CourseSpec` dials.
- **Downstream:** [S3](stage-03-transforms.md).
- **Contracts:** consumes C1. Emits no guarded contract — the heightfield and
  the flow field travel to S3 as an internal artifact.
- **Streams:** `skeleton/trunk/v1`, `skeleton/tributary/v1`,
  `skeleton/module/v1`.

## Inputs / Outputs

**In:** `PrimitiveField` (C1); descriptors `relief_budget_m`, `density_target`;
dials `skeleton.*` including the five module intensities.

**Out:** `skeleton.json` + CGRID1 fields:

```
Skeleton {
  skeleton_version: u32,
  kernel: KernelId,                 // I (fluvial) for all base six
  height:        Field<f64>,        // m, at 2 m -- the structural surface
  flow_dir_rad:  Field<f64>,        // DIRECTION [0,2pi), at 8 m
  flow_accum:    Field<f64>,        // m^2, at 8 m
  flow_distance: Field<f64>,        // m to the nearest channel, at 8 m
  channels:      Vec<Channel>,      // trunk + tributaries, as polylines
  divides:       Vec<Polyline>,     // derived, not authored
  meta: StructureMeta,              // passed through from C1, unchanged
}
```

## Responsibilities

### The fluvial engine (Kernel I), in order

1. **Trunk splines.** Root the trunk at `meta.base_level` and grow it upstream,
   biased by C1's `relief` and `accommodation`. This is the drainage backbone
   everything else hangs from.
2. **Tributary growth.** Grow tributaries from the trunk at a spacing set by
   `density_target`. Spacing is *the* drainage-density dial and a primary fit
   target. Deflection is biased by `hardness` and elongated along
   `grain_axis_rad`.
3. **Flow-distance transform.** Compute distance-to-nearest-channel over the
   grid. This is the coordinate the catena library is expressed in, and it is
   the reason the engine produces plausible hillslopes without simulating
   anything.
4. **Derived divides.** Extract divides *from the flow field*, never author
   them. An authored divide that disagrees with the flow field is the exact
   failure mode of the retired generator.
5. **Catena-library elevation.** Assign elevation as a function of flow
   distance and position on the hillslope, drawn from a calibrated catena
   library. The profile law's exponent θ is a fit target
   ([../calibration/metric-battery.md](../calibration/metric-battery.md)).

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

S2 carries the most calibration weight in the pipeline.

| Dial | Metric it is fit to | Family |
|---|---|---|
| Tributary spacing | drainage density (channel-km/km²) | process |
| Profile-law exponent θ | slope–area scaling | process |
| Catena statistics | hypsometric curve, slope distribution | process |
| Aeolian band spacing | spectral break, variogram range | process |
| Relief amplitude | local relief, elevation range | amplitude |
| Bench step height | curvature/TPI landform fractions | amplitude |

**Process metrics are identity** — they define what a biome *is*, and they must
match. **Amplitude metrics are per-site continuous descriptors** — they vary
within a biome and are matched in distribution, not pointwise. See
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

1. **Catena library representation.** A parametric profile family is cheap and
   certifiable; a sampled library from lidar is more faithful but harder to
   guarantee. Leaning parametric, fit to lidar. **Blocks implementation.**
2. **Does negative integration need its own code path?** Heathland's deranged
   network may not be expressible as "the same growth with a dial turned down".
   If it needs different growth, that is a second kernel wearing a module's
   clothes and should be admitted as such.
3. **Where does the S1/S2 amplitude fork live?** The dune amplitude split
   currently spans both stages. Cleaner to put it wholly in S2's aeolian module;
   confirm against the sandhills corpus.
4. **How is achieved density reported?** If fixed-count growth systematically
   misses `density_target`, the envelope is wrong. S2 should emit achieved
   density as a diagnostic so calibration can close the loop.

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
- [ ] Measured drainage density lands within the biome's target band for ≥ 90%
      of 1 000 seeds (the process-metric identity check).
- [ ] Resolution consistency: 8 m and 2 m runs produce the same channel network.
- [ ] Golden skeleton for seed 1, piedmont and heathland.
- [ ] Budget: < 900 ms.
