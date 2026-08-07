# Biomes

## Config, not code

**Stages never branch per biome.** There is no `if piedmont` anywhere in the
pipeline, and adding a biome adds no code to any stage. This is the single
architectural rule that most shapes day-to-day work here, so it is worth being
precise about what it means.

A biome is a **data record** with five parts:

| Part | Consumed by | Meaning |
|---|---|---|
| **Kernel selection** | [S2](../stages/stage-02-skeleton-kernel.md) | Which `SkeletonKernel` builds the structure. All six base biomes select Kernel I, the fluvial engine. |
| **Exemplar pool** | [S3](../stages/stage-03-amplification.md) | The real tiles whose residual patches supply this biome's texture. **This is where identity actually lives** — see below. |
| **Stage intensity dials** | S1–S4, S9, S10 | Per-stage, per-module scalars. |
| **Calibration targets** | offline only | The measured metric vector the biome is fit to ([../calibration/targets.md](../calibration/targets.md)). |
| **Scorer preset** | [S6](../stages/stage-06-routing.md), via C2 | The weights the router routes against. |
| **`plasticity`** | S6, S7 | 0 = terrain dictates routing, 1 = routing dictates terrain. |

### Identity is texture, not structure

The measurement that decided this: `structure.py` reports `dist_to_channel_p50`
at **104–120 m in every real archetype regardless of relief**. Drainage spacing
is a law of landscapes, not a signature — a sandhills tile and a hill-country
tile have nearly the same channel spacing.

So a biome's structural dials mostly make it *correct*, and its **exemplar pool
makes it recognizable**. Piedmont's spur-and-hollow interfluves, sandhills' dune
flanks, hill country's stepped risers — those are texture, reconstructed at
[S3](../stages/stage-03-amplification.md) from real terrain of that type.

The practical consequence: **adding a biome is mostly a data-collection job.**

### A module at zero, not a branch around it

The rule that makes this work in practice: **a biome that does not use a module
runs it at zero intensity.** Sandhills has no channels, so S2's tributary
growth runs with density ≈ 0 — it does not skip the growth code. Great Plains
has no scroll arcs, so S9's scroll family runs at amplitude 0 — it does not
skip the family.

This costs a little performance and buys three things: every code path is
exercised by every biome (so a bug in the rare path is found immediately rather
than by the one biome that uses it), dials are continuous and therefore
calibratable, and a new biome that wants 30% of a module gets it for free.

The corresponding naming rule from [../01-conventions.md](../01-conventions.md):
**dials are named for what they do, not for the biome that uses them.**
`drainage_integration`, never `heathland_derangement`. A dial named after a
biome is a biome branch wearing a disguise, and it will eventually acquire a
matching `if`.

### Where you *are* allowed to differ

Per-biome **thresholds** are legitimate — a sandhills course genuinely has more
blind shots than a piedmont one, and [S11](../stages/stage-11-validation.md)
compares against per-biome numbers. Those numbers live in the biome record and
arrive as data. The distinction: differing *thresholds* on a shared measurement
is config; differing *code* to do the measurement is a branch.

## The base six — "Heartland"

All served by Kernel I plus its five modules.

| Biome | Identity | Water | Relief | Canopy | Plasticity |
|---|---|---|---|---|---|
| [Piedmont](piedmont.md) | Rolling fluvial, creek corridors | streams | mid | closed | mid |
| [Great Plains](great-plains.md) | Flat, caprock-held, big sky | little/none | low | open | mid-high |
| [River Valley](river-valley.md) | Floodplain + terrace flight | river | low-mid | mixed | mid |
| [Sandhills](sandhills.md) | Aeolian dune trains | **none** | mid | open | **low** |
| [Heathland](heathland.md) | Deranged drainage, kettles | ponds/bogs | low-mid | open | low-mid |
| [Hill Country](hill-country.md) | Benched, stratified, steep | streams | **high** | mixed | **low** |

They were chosen to span the space: no-water to water-dominant, flat to steep,
open to closed canopy, and — most importantly — **integrated to deranged
drainage**. Sandhills (no network) and Heathland (a disconnected network) break
the fluvial engine's central assumption from opposite directions. A pipeline
that handles both is a pipeline whose abstractions are real.

Great Plains and Sandhills also bracket routing difficulty from opposite ends:
Great Plains is under-constrained (costs near zero everywhere, so the scorer has
nothing to grip) and Sandhills is over-constrained (low plasticity, fit or
fail).

## Golden archetypes

- **Piedmont — primary.** The reference case; every module at moderate
  intensity, nothing degenerate.
- **Heathland — adversarial.** The most-different biome in the base set.

**Lowcountry Marsh** (future, Coastal pack) resumes the adversarial
contract-tester role for [C2](../contracts/C2-routing-substrate.md) when it
lands. Heathland remains the adversary for the base six; Lowcountry Marsh
becomes the adversary for the contract itself. See
[future-biomes.md](future-biomes.md).

## What each biome file contains

Identity · Kernel & modules · Stage dials · Scorer preset & plasticity ·
Calibration targets (with corpus status) · Signature exaggeration ·
Open questions.

## Corpus status

**Three of the six have no calibration corpus.** The 2.3 GB tile corpus on this
repo is keyed to the retired v1 archetype set (`florida_lowland`,
`glacial_moraine`, `mountain_bench`, `piedmont`, `sandhills`) and none of its
fitted data carries over.

| Biome | Corpus | Clean tiles |
|---|---|---:|
| Piedmont | v1 tiles exist; **re-measure and scale up** | 6 of 14 |
| Sandhills | v1 tiles exist; **re-measure and scale up** | 10 of 10 |
| Hill Country | partial — v1 `mountain_bench` is related but a different definition | 8 of 14 |
| Heathland | **none** — v1 `glacial_moraine` was a different definition | 5 of 31 |
| Great Plains | **none** | 0 |
| River Valley | **none** | 0 |

**38 clean tiles total**, across the *retired* archetype set. The campaign
target is ~30 clean per biome, ~180 total.

This is the pipeline's largest open data debt, and it now blocks more than it
used to: S0, S2, and S9 need it for targets, and **S3 needs it to exist at all**
— without an exemplar corpus there is no dictionary and no texture. See
[../calibration/targets.md](../calibration/targets.md).
