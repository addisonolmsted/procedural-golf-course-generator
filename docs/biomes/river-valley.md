# River Valley

**Role: the transform-heavy biome.** The only one where
[S3](../stages/stage-03-transforms.md) does substantial work, and the only one
where scroll arcs carry real amplitude at
[S9](../stages/stage-09-micro-repass.md).

## Identity

A mature river floodplain with its terrace flight: a broad flat valley floor
built of the river's own sediment, the meander scars and scroll bars it left
while building it, and above that a staircase of abandoned terraces marking
older, higher floodplains. Valley sides stratified and stepped.

The structure is genuinely two-part and that is the whole character — a
**flat floor** and a **stepped rise**, meeting at a distinct break. Terrain that
blends smoothly between them is not a river valley.

The golf is bottomland parkland: holes on the floodplain running along the
river, holes climbing the terrace flight, and the most interesting holes at the
break between.

## Kernel & modules

Kernel I (fluvial), running at full strength — the biome the engine was
designed around.

| Module | Intensity | Expression |
|---|---|---|
| Trunk-river | **dominant** | Broad floodplain, **full terrace flight**, oxbow and scroll-bar stamps. The signature module. |
| Stratigraphy / bench | **mid** | In the valley sides, above the floodplain. Shared with [Hill Country](hill-country.md) and [Great Plains](great-plains.md). |
| Closed basin | **low** | Abandoned oxbows holding water. |
| Aeolian | **zero to trace** | Some loess on the terraces. |
| Drainage integration | **high, positive** | Fully integrated. The reference case for a large trunk. |

## Stage dials

| Stage | Setting |
|---|---|
| [S1](../stages/stage-01-macro-primitives.md) | **`accommodation` is the dominant field** — a broad high-accommodation floor with low-accommodation valley sides. Gentle, strongly monotone tilt. Strata present. |
| [S2](../stages/stage-02-skeleton-kernel.md) | Mid-high drainage density. Trunk-river dominant; terrace flight constructed here as geometry. |
| [S3](../stages/stage-03-transforms.md) | **Floodplain datum dominant** — the most transform-heavy biome. Flatten the floor toward the trunk while **preserving the terraces above it**. Requires the datum to have an upper bound. |
| [S4](../stages/stage-04-finishers.md) | Low-mid. Creep on the valley sides; almost nothing on the floodplain floor, which is depositional rather than erosional. |
| [S9](../stages/stage-09-micro-repass.md) | **Scroll arcs dominant** on the floodplain — the only biome where they carry real amplitude, oriented by the trunk-river module's meander provenance. Swales on the valley sides. |
| [S10](../stages/stage-10-zoning-aesthetics.md) | Riparian gallery forest on the floor, open terraces above. Strong vegetation contrast at the break. |

## Scorer preset & plasticity

**Plasticity: mid.**

Cost-field character: sharp `unbuildable` contrast between floodplain (near
zero) and valley sides (high); `borrow` high on the floor; `water` present as
the river plus oxbows; `drainage_risk` high on the floodplain, which is after
all a flood plain.

The routing tension is the interesting part: **routability wants the floor,
interest wants the break.** A router that only optimizes cost puts all nine
holes on the flat and produces a boring course on the most characterful terrain
in the base six. This is the biome where the fit term's definition matters most.

High variance in per-hole earthwork budgets at
[S7](../stages/stage-07-earthmoving.md) — floodplain holes need almost nothing,
valley-side holes need a lot.

## Calibration targets

**Corpus: none.** No v1 archetype corresponds. Needs a fresh tile campaign —
mature meandering rivers with preserved terrace flights (Mississippi
tributaries, the Missouri breaks, European lowland rivers).

| Metric | Family | Notes |
|---|---|---|
| **Terrace step count and spacing** | **process, identity-defining** | The staircase. Needs adding to the battery. |
| **Hypsometric curve shape** | **process** | Strongly bimodal — floor and terraces — with a sharp break. Like [Great Plains](great-plains.md), a unimodal fit would score acceptably and be wrong. |
| Meander wavelength | process | Sets scroll-arc spacing at S9. |
| Drainage density | process | |
| Floodplain width / valley width ratio | process | |
| Local relief | amplitude | |
| Scroll-bar amplitude | amplitude | The S9 fit target. |

## Signature exaggeration

**The terrace flight read as stacked levels.** Step risers slightly steeper and
treads slightly flatter than reality, so the staircase is unmistakable from the
floor.

## Open questions

1. **Terrace preservation under the floodplain datum.** The datum must flatten
   the floor and leave the terraces alone, which means an upper bound. Is that
   bound a dial or derived from S2's terrace geometry? Leaning derived — see
   [S3's open questions](../stages/stage-03-transforms.md). **Blocks S3 for
   this biome.**
2. **Where are terraces built — S2 or S3?** Currently S2 constructs them as
   geometry and S3 must avoid destroying them. Building them in S3 as a
   transform would be more honest to the geomorphology (they *are* abandoned
   datums) and would remove the preservation problem entirely. Worth
   reconsidering before implementation.
3. **The floor-versus-break routing tension.** May need an explicit preset term
   rewarding holes that use the break, rather than relying on a generic fit
   term.
4. **Scroll-arc provenance.** S9 needs the meander history to orient arcs. Does
   S2 emit it explicitly, or does S9 infer it from the trunk geometry? Explicit
   is safer; it needs a field.
