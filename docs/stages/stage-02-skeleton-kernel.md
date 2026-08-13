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
- **Streams:** `skeleton/carve/v2`, `skeleton/module/v1`. (The v1 trunk and
  tributary streams retired with the authored engine.)

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

**S2 derives its network; it does not author one.** The first implementation
grew trunk and tributary polylines and then defended them with crossing
guards, separation neighbourhoods, mouth-angle enforcement, curl penalties
and a whole-network trim sweep. Ten structural patches later the review was
still finding loops and long parallel pairs. Geometry authored in path space
has to be *argued* into agreement with the terrain it sits on, and every
closed hole moved a rare variant somewhere else. Letting the terrain decide
makes the same properties structural:

- **No loops.** A D8 receiver graph is a forest; a loop is unreachable.
- **No crossings.** Two paths that meet share every cell after the meeting
  point — they merge, they cannot cross.
- **Sensible confluences.** Junctions sit where terrain converges, at the
  angle the two valleys arrive.
- **Parallel only where earned.** Neighbouring channels stay separate
  exactly when a divide separates them.

1. **Roughness seed.** C1 is band-limited to ≥ 400 m, and flow over a
   surface that smooth runs in near-parallel sheets. A fixed-count wave sum
   in S2's own band (140–520 m) gives drainage something to compete over.
   This is also the spur-and-hollow degree of freedom the pipeline revision
   identified as missing.
2. **Boundary condition.** The three non-base borders are rimmed and the
   base edge is drawn down: one open boundary, the rest no-flux. Without it
   the tile drains radially like an island — measured base-edge outflow was
   12–82%, and a regional gradient up to 1.5× relief did not fix it, since
   water leaves by whichever border it reaches first. Real 3 km windows *do*
   drain multi-edge (corpus max-edge share 46–78%), but our tile is a site
   draining to a declared base level.
3. **Trunk inflow.** A tile is a window in a larger landscape, and a
   bottomland river carries a catchment that lies mostly outside it. An
   external area (20 km² per unit of the `trunk_river` dial) enters at the
   middle of the far boundary, so a through-going main river exists rather
   than only the tile's own 9 km².
4. **Erosion.** A fixed 15 iterations of `dz = k·√A·S`, carved
   downstream-first with each cut capped at half the drop to the
   already-carved receiver — that cap is what keeps the surface drainable by
   construction, so no pits form and no flat pools appear for D8 to cross in
   straight lines. `k` scales with C1 `hardness`, so resistant ground
   deflects channels *physically* instead of by geometric rule.
   **Routing dither:** the flood grades its flats by epsilon, and an
   eps-flat under a planar tilt gives D8 an exactly axis-parallel descent.
   A correlated (~40 m), isotropic, slope-tapered dither on the *routing
   surface* breaks that without touching the terrain.
5. **Derangement.** A deranged landscape keeps its depressions: interdune
   lows and kettles swallow their catchment and never pass it on. That
   fraction of the deepest natural depressions is handed to the router as
   kept pits, so heathland and sandhills derangement **emerges** from the
   same code path rather than being special-cased (this closes open
   question 1 below).
6. **Extraction.** Channel raster = drained area ≥ threshold; the threshold
   IS the density dial, and sub-threshold rills stay in the surface as
   texture for S3. Strahler order comes from the flow forest; the vector
   network is traced downstream from each head through the receivers — the
   same method `tools/macro_campaign/real_planform.py` uses on real lidar,
   so generated and corpus planform statistics mean the same thing. Two
   Chaikin passes smooth the emitted polylines (a chain of cell centres
   staircases at 45°/90°, which alone reads as ~1.3 sinuosity); the raster
   is untouched.
7. **Flow-distance transform.** Multi-source Dijkstra from the channel
   cells: distance-to-channel, the normalized channel→divide coordinate,
   and hillslope position. These are the coordinates S3 reconstructs
   against.
8. **Derived divides.** Extract divides *from the flow field*, never author
   them. An authored divide that disagrees with the flow field is the exact
   failure mode of the retired generator.
9. **Bank profile.** The erosion already cut the valleys, so the catena
   shapes BANKS only: order-scaled flat floors and a groove/hillslope
   blend, applied to the carved surface. It only ever lowers ground, so the
   carve's drainage monotonicity survives.

   **Keep this simple.** The catena's job is a correct mean surface, not a
   characterful one. Effort spent making it look good is effort spent
   competing with [S3](stage-03-amplification.md), which has real data and
   will win.

### The five modules — structure only

Each is a **dial, not a branch**. A biome that does not use one runs it at zero
intensity; the code path is identical.

The modules were inherited from a design where S2 owned identity, and have been
**pruned to structural roles** so that S2 and S3 never fight over the same
feature. The dividing rule is crisp:

> **An S2 module may alter only the network and the base surface at
> wavelengths at or above the dictionary's patch scale (~64 m). Everything
> below that wavelength belongs to S3.**

| Module | Structural role (S2 keeps) | Moved to S3's dictionary |
|---|---|---|
| **Trunk-river** | Floodplain accommodation, terrace-flight **geometry**, meander planform + its provenance record | Scroll-bar and swale **texture** on the floodplain |
| **Stratigraphy / bench** | Bench and scarp **geometry** — tread/riser positions from the strata stack | Riser-face and tread **texture** |
| **Closed basin** | The point-process **embryos** that seed basins, and closed-basin tolerance in the flow field. Embryos are recorded in the artifact — S3's polish and S7's repair both need to know which pits are intended | Hummock texture around kettles |
| **Aeolian** | Dune-**train** ridges in the base surface — the 100–400 m structural forms, oriented by `wind_azimuth_rad`. These sit above S1's ≥400 m band and above S3's patch scale, so they have no other owner | Dune-flank form, blowouts, rumple — everything below ~64 m |
| **Drainage integration** | How completely the network connects; run **negative** for a deranged network | — (purely structural) |

This also **resolves the S1/S2 amplitude fork** from the earlier draft: S1 owns
the ≥ 400 m dune-train envelope, S2's aeolian module owns the 64–400 m dune
forms, S3 owns everything finer. Three owners, three disjoint bands, no fork.

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
([../02-performance-budget.md](../02-performance-budget.md)). Everything runs
at 8 m; the presentation raster is written at 2 m. Measured: **570–600 ms**
integrated, 240 ms deranged.

The **priority-flood dominates**, one per erosion iteration. That is worth
knowing precisely: dithering the routing surface *after* the fill needs a
second flood to clear the pits the dither creates, which cost 828 ms of the
900 ms budget. Dithering *before* it is equivalent and needs one.

Fixed iteration counts everywhere — **never iterate until a target is met**,
which is a data-dependent loop and a budget hazard. The erosion runs a fixed
15 iterations and the achieved density is whatever the extraction threshold
yields.

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

**Closed by the derived-network rework (2026-08-12):**
- *Does negative integration need its own code path?* No. Kept pits give
  derangement through the identical path; heathland and sandhills measure
  connectivity 0.05–0.06 against 1.00 for the integrated four.
- *Horton ratio enforcement — grow to a fixed count or construct top-down?*
  Neither: the ratios are now an OUTCOME, and the acceptance band was
  re-based on the corpus measured with the identical extraction and reach
  definition (Rb 2.14–3.08, Rl 0.92–1.16, Ω 2–3 —
  `tools/macro_campaign/horton_real.py`). The old 3–5 / 1.5–3 band was only
  ever achievable by construction.


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
4. ~~Where does the S1/S2 amplitude fork live?~~ **Resolved** by the wavelength
   rule above: S1 ≥ 400 m, S2 64–400 m, S3 < 64 m. Validate the band boundaries
   against the sandhills corpus's spectral break, but the ownership question is
   closed.
5. **How is achieved density reported?** If fixed-count growth systematically
   misses `density_target`, the envelope is wrong. S2 should emit achieved
   density and achieved Horton ratios as diagnostics so calibration can close
   the loop.

**Exit gate: G-SKELETON** in
[../03-success-indicators.md](../03-success-indicators.md) — including the
conditioning-space-overlap leading indicator, the golfability proxy, and the
explicit rule that S2 is **not** judged on texture. The criteria below are the
correctness axis of that gate.

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
