# Hill Country

**Role: the high-relief case, and the hardest routing problem in the base six.**
Feasible ground is genuinely scarce.

## Identity

A dissected limestone plateau: flat-topped divides held up by resistant beds,
stepping down through benches and small scarps into incised valleys. The
character is **stratigraphic** — every slope is a staircase, because the rock
is layered and the layers erode at different rates. Thin soil, rock outcrop at
every riser, spring-fed streams in the valley bottoms.

The distinguishing feature against every other high-relief terrain: slopes here
are **stepped, not smooth**. A smooth steep slope is a mountain; a stepped one
is hill country. If the benches are not legible, the biome has failed regardless
of how correct the relief statistics are.

The golf is dramatic and constrained: holes on the plateau tops, holes dropping
through the benches, greens benched into slopes, substantial elevation change
per hole.

## Kernel & modules

Kernel I (fluvial), with stratigraphy carrying the identity.

| Module | Intensity | Expression |
|---|---|---|
| Trunk-river | **low** | Incised streams in narrow valley bottoms. No floodplain worth the name. |
| Stratigraphy / bench | **dominant** | **Thick stack, strong benching.** The biome's entire character. Shared with [Great Plains](great-plains.md) and [River Valley](river-valley.md). |
| Closed basin | **zero to trace** | Some solution features; effectively zero. |
| Aeolian | **zero** | |
| Drainage integration | **high, positive** | Integrated and deeply incised. |

## Stage dials

| Stage | Setting |
|---|---|
| [S1](../stages/stage-01-macro-primitives.md) | **Thick strata stack ⇒ strongly stepped `hardness`.** Highest relief amplitude in the base six. `accommodation` low outside valley floors. Together with [River Valley](river-valley.md), one of only two biomes that genuinely move S1's dials. |
| [S2](../stages/stage-02-skeleton-kernel.md) | Mid drainage density; channels **incised, not broad**. Stratigraphy dominant. |
| [S3](../stages/stage-03-amplification.md) | **Stepped texture** — benches and risers with terracettes on the treads. The texture *is* the stratigraphy showing through, so conditioning must pick up `tpi` strongly. Highest amplitude after Sandhills. |
| [S4](../stages/stage-04-hydrology.md) | Light floodplain in the incised valleys; deep water table. **Benches must be untouched** — a datum op that flattens a bench has destroyed the biome. |
| [S9](../stages/stage-09-micro-repass.md) | **Terracettes dominant** on grazed slopes — strongly contour-aligned. Swales in the incised valleys. |
| [S10](../stages/stage-10-zoning-aesthetics.md) | Scrub oak and cedar, thin soil, rock outcrop at every riser. |

## Scorer preset & plasticity

**Plasticity: low.** Tied with [Sandhills](sandhills.md) for the lowest. Not
because the terrain is precious in the same way, but because at this relief,
grading is prohibitively expensive — moving a bench costs more earth than the
whole course budget.

`earthwork` and `unbuildable` dominate the cost fields and reach values no
other base biome approaches. Feasible ground is scarce and often
non-contiguous, which makes this **the hardest search problem in the base six**:
unlike [Great Plains](great-plains.md), where every route scores the same, here
most routes are simply not viable.

**Highest risk of a hole exhausting its earthwork budget** at
[S7](../stages/stage-07-earthmoving.md). What grading does happen is
concentrated and deep, and `unresolved` is most likely to be non-empty here.

## Calibration targets

**Corpus: partial.** The v1 `mountain_bench` archetype (28 extracted tiles) is
related but not the same biome — it was a mountain-front definition rather than
a dissected plateau. Tiles are a useful starting point; the targets are not
transferable. Needs supplementing — Texas Hill Country, the Ozarks, the
Cumberland Plateau, Causses.

| Metric | Role | Notes |
|---|---|---|
| **`slope_bimodality`** ★ | **discriminant** | Treads and risers are two modes. **This is what separates hill country from any other steep terrain** — its slopes are stepped, not smooth. Shares the metric with Great Plains at completely different proportions. |
| **`tpi_landform_fractions`** | **discriminant** | The flat/riser/tread decomposition. |
| `variogram_sill` | **discriminant** | High amplitude, and stepped — the dictionary must reproduce risers, not just roughness. |
| `drainage_density`, `slope_area_theta` | shared invariant | Steep and incised, but at the same spacing as everywhere else. |
| `local_relief_p50` | amplitude | Highest in the base six. |

## Signature exaggeration

**Bench and scarp verticality.** Risers steeper and treads flatter than
reality, so the staircase reads clearly from a distance and from the fairway.
The same exaggeration family as [River Valley](river-valley.md)'s terrace
flight, applied to a different structure.

## Open questions

1. **Does S1's `hardness` field have enough expressive power?** Hill Country
   needs "hard in thin bands", and a [0,1] field with no length scale may not
   express it — flagged in
   [S1's open questions](../stages/stage-01-macro-primitives.md). The strata
   stack may cover it. **Resolve before S1 is implemented**; Hill Country is
   the biome that will expose the gap.
2. **How is bench legibility measured?** "The benches must read" needs a metric.
   Bench step count per unit relief is proposed above but unvalidated.
3. **Search viability.** If feasible ground is non-contiguous, the beam search
   may fail to find any route on some seeds. That must be impossible under
   v2's no-retry rule, which means the **certified envelope has to guarantee
   sufficient contiguous feasible area**. This is a concrete, checkable
   envelope constraint and it should be stated as one — see
   [../calibration/envelope-certification.md](../calibration/envelope-certification.md).
4. **Does the taper bound conflict with the appearance target?** Hill country
   wants high texture amplitude; benches and the channel taper both constrain
   it. If the appearance target sits above the certified ceiling, the answer is
   more robust bench structure at S2 — not weaker texture at S3.
