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
| **Stage intensity dials** | S1–S4, S9, S10 | Per-stage, per-module scalars. |
| **Calibration targets** | offline only | The measured metric vector the biome is fit to ([../calibration/targets.md](../calibration/targets.md)). |
| **Scorer preset** | [S6](../stages/stage-06-routing.md), via C2 | The weights the router routes against. |
| **`plasticity`** | S6, S7 | 0 = terrain dictates routing, 1 = routing dictates terrain. |

### A module at zero, not a branch around it

The rule that makes this work in practice: **a biome that does not use a module
runs it at zero intensity.** Sandhills has no channels, so S2's tributary
growth runs with density ≈ 0 — it does not skip the growth code. Sandhills has
no rilling, so S4's rilling pass runs at intensity 0 — it does not skip the
pass.

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

| Biome | Corpus |
|---|---|
| Piedmont | v1 tiles exist; **must be re-measured** against the new battery |
| Sandhills | v1 tiles exist; **must be re-measured** |
| Hill Country | partial — v1 `mountain_bench` tiles are related but not the same biome |
| Heathland | **none** — v1 `glacial_moraine` was a different definition |
| Great Plains | **none** |
| River Valley | **none** |

This is the pipeline's largest open data debt and it blocks S0, S2, S4, and S9.
See [../calibration/targets.md](../calibration/targets.md).
