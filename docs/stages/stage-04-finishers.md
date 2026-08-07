# Stage S4 — Simulation Finishers †

**Status:** unclaimed
**Crate:** [`crates/course-finishers/`](../../crates/course-finishers/)

## Purpose

Add erosion texture, and nothing else.

S4 exists because authored terrain, however well its statistics are
constructed, looks authored at close range: slopes are too smooth, channel
heads are too clean, hillslopes lack the fine rilling and creep that real
ground has. A short erosion pass fixes that in a way no amount of noise can.

**The boundary is the contract: a finisher never restructures.** It may not
move a divide, open a new channel, capture a drainage, or change the network
S2 authored. That rule is what keeps the pipeline certifiable — S2's output is
what calibration measured, and a finisher that reorganizes drainage invalidates
every envelope. `validate` enforces it, and the check is not optional.

## Position

- **Upstream:** [S3](stage-03-transforms.md) — `Transformed`.
- **Downstream:** [S5](stage-05-substrate-assembly.md).
- **Contracts:** none directly; its height becomes C2's `height`.
- **Streams:** `finishers/v1`.

## Inputs / Outputs

**In:** `Transformed` (height, water bodies, basins); S2's flow field and
channel network for the restructuring check; dials `finishers.*` (per-pass
intensity).

**Out:** `finishers.json` + the textured 2 m heightfield, plus a diagnostic
record of the before/after network comparison.

## Responsibilities

1. **Run the texture passes** at per-biome intensity: hillslope diffusion
   (creep), fine rilling on steep slopes, and channel-bank detail. Fixed pass
   count from config.
2. **Preserve water bodies and basins.** A body's bed may gain texture; its
   shoreline may not move materially and its surface may not change. Closed
   basins from S3 must still be closed afterwards.
3. **Verify no restructuring.** Re-extract the channel network from the
   textured surface and compare against S2's. Divides in the same places,
   channels in the same places, no new confluences. A failure here is a
   **calibration bug** — the intensity envelope was too wide — and it is
   reported, loudly, not worked around.
4. **Instrument, in dev builds only.** Emit the metric battery over the
   finished surface so calibration can close the loop. Runtime builds compile
   this out entirely and **have no gate** — see
   [../calibration/envelope-certification.md](../calibration/envelope-certification.md).

## Biome expression

Intensity only. The pass structure is identical across all six; this is close
to an identity pass in *structure* and purely a dial in *amount*.

| Biome | Intensity | Character |
|---|---|---|
| **Piedmont** | mid | Balanced creep and rilling. The reference case. |
| **Great Plains** | low | Very little. Flat ground erodes slowly and the biome's identity is smoothness. |
| **River Valley** | low-mid | Creep on the valley sides; almost nothing on the floodplain floor. |
| **Sandhills** | low, and **diffusion-only** | Sand does not rill. Rilling intensity is zero; creep is the only pass with weight. The clearest example of a module dialed to zero rather than branched around. |
| **Heathland** | low | Deranged drainage means little concentrated flow to erode with. High rilling intensity here would begin integrating the network, which is exactly the restructuring the contract forbids — **heathland is where the restructuring check earns its keep**. |
| **Hill Country** | high | Steep slopes, active erosion, strong rilling. Also the biome most at risk of a finisher breaching a bench, which is restructuring. |

## Determinism & budget

**900 ms** at the 2 m rung
([../02-performance-budget.md](../02-performance-budget.md)).

**Fixed pass count. No convergence criterion.** "Erode until the residual drops
below ε" is a stage whose runtime depends on its data, which means a seed that
takes forty seconds exists somewhere in the space. The pass count comes from
config and never from the data.

Erosion is iterative and float-sensitive: partition into fixed chunks, combine
in a fixed order, use `libm` transcendentals, and never reduce floats in
parallel-nondeterministic order
([../01-conventions.md](../01-conventions.md)).

## Calibration †

| Dial | Metric it is fit to | Family |
|---|---|---|
| Creep intensity | roughness at short lag; variogram nugget/sill | amplitude |
| Rilling intensity | slope distribution tail; curvature variance | amplitude |
| Channel-bank detail | cross-section shape statistics | amplitude |
| **All intensities jointly** | **the restructuring bound** | **hard limit** |

The last row is the important one and it is not like the others. The
certification job for S4 is not only "which intensity looks right" but "what is
the maximum intensity at which restructuring does not occur", per biome. That
upper bound is a **hard envelope boundary** — S0 may never draw above it.
Heathland's bound will be the tightest, because its network is the easiest to
accidentally integrate.

**Tile data required:** the same per-biome DTM tiles as S2, measured at short
lag. Note the despeckle caveat in
[../calibration/lidar-pipeline.md](../calibration/lidar-pipeline.md) — lidar
noise lives at exactly the wavelengths S4 operates at, so an under-despeckled
tile will teach S4 to reproduce sensor noise as terrain texture.

## Future-biome seams

None structural. Future packs add intensity presets, not passes. The one
anticipated exception is a **chemical-weathering pass** for karst, which is a
genuinely different process and would be a new pass rather than a new dial —
but karst also restructures by nature (sinkholes capture drainage), so it may
need to be an S3 transform instead. Deferred with the pack.

## Open questions

1. **Where exactly is the restructuring threshold?** "The network is the same"
   needs a metric and a tolerance. Proposal: channel-head positions within one
   8 m cell, divide positions within two, and zero new confluences. Needs a
   number before implementation.
2. **Should S4 run at 2 m or at 4 m and upsample?** 4 m would buy ~4× budget
   headroom. Risk is that texture at 4 m upsampled is visibly smooth at 2 m.
   Test early; it is the cheapest available budget win in the pipeline.
3. **Is diffusion-only genuinely enough for Sandhills?** If dune faces need a
   distinct avalanche/slip-face pass, that is a fourth pass at zero intensity
   elsewhere, not a branch.

## Acceptance criteria

- [ ] Same seed ⇒ byte-identical output, cross-platform. (Iterative float code
      makes this the hardest determinism test in the pipeline — test it on two
      architectures explicitly.)
- [ ] **The restructuring check passes for all six biomes over 1 000 seeds**:
      channel heads within tolerance, divides within tolerance, zero new
      confluences. Heathland specifically must not integrate.
- [ ] Water bodies survive: shorelines within tolerance, surfaces unchanged,
      closed basins still closed.
- [ ] Fixed pass count, asserted. No convergence criterion anywhere in the
      crate.
- [ ] Runtime builds contain no instrumentation and no gate (check the
      feature-gated build compiles without the metric battery).
- [ ] Sandhills runs with rilling at zero and produces sensible dune texture.
- [ ] Short-lag roughness metrics land inside the biome's target band.
- [ ] Golden output for seed 1, piedmont and heathland.
- [ ] Budget: < 900 ms at 2 m.
