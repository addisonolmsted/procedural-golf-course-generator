# Great Plains

**Role: the under-constrained routing case.** Costs are near zero almost
everywhere, so the router's scorer has nothing to grip. Together with
[Sandhills](sandhills.md) it brackets routing difficulty from opposite ends.

## Identity

High flat grassland on near-horizontal sedimentary strata: a broad plain held
flat by a resistant caprock, cut by shallow, widely-spaced draws. Very low
relief, but not *smooth* — the flatness is structural, maintained by a hard
layer, and it breaks into small scarps and shallow benches where the caprock is
breached.

That distinction matters and is easy to get wrong. Great Plains is not "low
amplitude piedmont". The relief is low because a stratum is holding it up, and
the terrain reads as a **surface with edges** rather than as gentle rolls.

The golf is prairie links: enormous corridors, big sky, wind as the main
defence, and hazards doing nearly all the strategic work because the ground
does none.

## Kernel & modules

Kernel I (fluvial), with stratigraphy carrying the identity.

| Module | Intensity | Expression |
|---|---|---|
| Trunk-river | **low** | Shallow, widely-spaced draws. No floodplain to speak of. |
| Stratigraphy / bench | **dominant** | **The caprock.** This is what makes it flat rather than smooth. Shared with [Hill Country](hill-country.md) and [River Valley](river-valley.md). |
| Closed basin | **low** | Playa depressions — shallow, ephemeral, occasionally holding water. |
| Aeolian | **trace** | Loess veneer. Near zero. |
| Drainage integration | **high, positive** | Integrated, just sparse. |

## Stage dials

| Stage | Setting |
|---|---|
| [S1](../stages/stage-01-macro-primitives.md) | **Low relief amplitude**; strata present; **high `accommodation` almost everywhere**; low-mid `grain_strength`. |
| [S2](../stages/stage-02-skeleton-kernel.md) | **Low drainage density** — widely-spaced draws. Stratigraphy dominant: benching and small scarps where the caprock breaks. |
| [S3](../stages/stage-03-amplification.md) | **Low amplitude, but not smooth.** The caprock breaks into small scarps and benches — texture *with structure*. The biome most likely to be wrongly rendered as "flat plus noise", which is exactly what the dictionary exists to prevent. |
| [S4](../stages/stage-04-hydrology.md) | Minimal. Shallow floodplain, deep water table. **Often produces zero water bodies**, which is legal and must be handled. |
| [S9](../stages/stage-09-micro-repass.md) | Swales at **very low** amplitude. Scroll arcs and aeolian rumple at zero. The ground is genuinely smooth and adding texture to make it "interesting" destroys the biome. |
| [S10](../stages/stage-10-zoning-aesthetics.md) | Open grassland, isolated tree groups in the draws, big sky. |

## Scorer preset & plasticity

**Plasticity: mid-high** — the highest in the base six. There is nothing to
preserve and nothing in the way, so building what you want is both affordable
and appropriate.

**This is the under-constrained routing case.** `earthwork` is near zero almost
everywhere, `clearing` is near zero, `unbuildable` is near zero. The scorer's
cost surface is nearly flat across the whole search space, which means the
search can return an arbitrary, characterless route that scores as well as a
good one.

Two consequences:

1. **Fit terms carry the entire burden.** Whatever `w_fit` measures is the only
   thing distinguishing routes.
2. **The preset may need an explicit variety or interest term** to break the
   symmetry. Flagged as an open question in
   [S6's doc](../stages/stage-06-routing.md); Great Plains is the biome that
   will settle it.

**Highest feature budget of the six** at
[S8](../stages/stage-08-hole-layout.md) — bunkers and water carry the strategy
the terrain cannot.

Great Plains is also the one biome where [S7](../stages/stage-07-earthmoving.md)
earthmoving typically *increases* relief rather than removing obstruction.

## Calibration targets

**Corpus: none.** No v1 archetype corresponds. Needs a fresh tile campaign —
US High Plains, the Llano Estacado, southern Alberta.

| Metric | Role | Notes |
|---|---|---|
| **`slope_bimodality`** | **discriminant** | Strongly bimodal: near-zero on the surface, steep on the scarps. **The bimodality is the identity** — a unimodal fit would be a passing score on a wrong model. |
| **`tpi_landform_fractions`** | **discriminant** | The flat/scarp/bench decomposition is what separates Great Plains from "low-relief anything". |
| `variogram_sill` (low) | **discriminant** | Low texture amplitude — but the dictionary must still supply *structured* low amplitude, not smoothness. |
| `hypsometric_bimodality` | **discriminant** | High integral, with a break where the caprock is. |
| `drainage_density` | shared invariant | Widely-spaced draws, but no lower than any other biome's — resist the temptation to make it a signature. |
| `local_relief_p50` | amplitude | Low. |

## Signature exaggeration

**Horizon width and sky dominance.** Achieved through the light rig and camera
framing rather than terrain — the ground is what it is; the *sense* of space is
exaggerated.

## Open questions

1. **Does the router need a variety term?** See above. Great Plains is the test
   case for [S6's](../stages/stage-06-routing.md) open question 5, and the
   answer probably applies to every future flat biome.
2. **Bimodal slope fitting.** A single-mode fit will score acceptably and be
   wrong. The metric battery may need an explicit bimodality measure rather
   than relying on distribution matching.
3. **Playas — closed-basin module or a transform?** They are shallow ephemeral
   depressions that sometimes hold water. Currently assigned to the closed-basin
   module at low intensity; the water-table datum may be the better home.
4. **How low can relief go** before the router stops producing distinguishable
   courses at all? There is presumably a floor, and the certified envelope needs
   to sit above it.
