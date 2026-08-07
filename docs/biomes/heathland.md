# Heathland

**Role: the adversarial golden archetype.** The most-different biome in the
base set. It exercises the paths piedmont never touches, and it is where three
separate architectural rules earn their keep.

## Identity

Poorly-drained acid heath on a young, disorganized surface — glacial outwash and
till that has not been around long enough for drainage to integrate. The
defining feature is what is *missing*: there is no proper stream network.
Water sits in kettle holes and bogs rather than flowing away, and the channels
that exist are short, disconnected, and go nowhere.

Above ground: heather, gorse, scattered pine, thin acid soil over sand. Firm,
fast, open. The golf is links-adjacent — small firm greens, running ground
shots, heather as unrecoverable rough.

## Why it is the adversary

Heathland breaks the fluvial engine's central assumption. Three specific
stresses:

1. **[S2](../stages/stage-02-skeleton-kernel.md): negative drainage
   integration.** Density ~0.07 — a deranged network that never reaches base
   level. The engine must produce a *disconnected* result without a special
   case.
2. **[S4](../stages/stage-04-finishers.md): the restructuring bound is tightest
   here.** Heathland's network is the easiest in the base set to accidentally
   *integrate* — a slightly-too-strong rilling pass will connect the
   disconnected channels, which is precisely the restructuring the finisher
   contract forbids. Heathland sets S4's hard envelope ceiling.
3. **[S7](../stages/stage-07-earthmoving.md): kettles are borrow sites *and*
   intended depressions at the same time.** The Priority-Flood verify will
   correctly identify every kettle as a depression, and repairing them away
   destroys the biome. This is the case that justifies S3's basin inventory
   being a first-class artifact rather than a byproduct.

Plus the ordinary empty cases: empty strata, near-featureless C1, low
`grain_strength`.

## Kernel & modules

Kernel I (fluvial), run against its own assumptions.

| Module | Intensity | Expression |
|---|---|---|
| Trunk-river | **zero** | There is no trunk worth the name. |
| Stratigraphy / bench | **zero** | Empty strata stack. |
| Closed basin | **dominant** | Kettle holes. Point-process embryo at high density. |
| Aeolian | **zero to trace** | Some outwash sand reworking; effectively zero. |
| Drainage integration | **negative** | ~0.07 density. The signature dial. |

## Stage dials

| Stage | Setting |
|---|---|
| [S1](../stages/stage-01-macro-primitives.md) | Low-mid relief; **empty strata**; uniform `hardness`; **low `grain_strength`**. Nearly featureless C1 by design — heathland's character is derangement, which is an S2 dial, not an S1 one. |
| [S2](../stages/stage-02-skeleton-kernel.md) | Density ~0.07, integration negative, closed-basin module dominant. |
| [S3](../stages/stage-03-transforms.md) | **Water-table datum dominant.** The table sits near the surface, so S2's closed basins fill: kettle ponds and bogs. Seasonal ones marked `permanent: false`. No floodplain. |
| [S4](../stages/stage-04-finishers.md) | **Low, and capped hard.** See the restructuring bound above. |
| [S9](../stages/stage-09-micro-repass.md) | Low amplitude, patchy, hummocky rather than directional — the one biome where `grain_strength` is low enough that orientation barely reads. |
| [S10](../stages/stage-10-zoning-aesthetics.md) | Heather and gorse, scattered pine, no closed canopy. |

## Scorer preset & plasticity

**Plasticity: low-mid.** The ground is firm and the character is the point;
grading heather flat produces a generic course.

Cost-field character: `borrow` high (kettles are ready-made pits), `water`
patchy and strongly fractional (many small bodies rather than a few large
ones), `clearing` low (no canopy to clear), `earthwork` moderate. Many small
obstacles rather than a few large ones — a different routing problem from
anything else in the base six.

## Calibration targets

**Corpus: none.** The v1 `glacial_moraine` archetype was a different definition
and its 62 extracted tiles do not transfer. Heathland needs a fresh tile
campaign — UK/Netherlands heath and North American outwash plains — and it is
the highest-priority collection because it is a golden archetype.

| Metric | Family | Notes |
|---|---|---|
| **Drainage density (~0.07)** | **process, identity-defining** | The single number that makes heathland heathland. |
| **Network connectivity** | **process, identity-defining** | Fraction of channel length reaching base level. Near zero. Needs to be added to the battery — see [../calibration/metric-battery.md](../calibration/metric-battery.md). |
| Closed-basin density | process | Kettles per km². |
| Hypsometric integral | process | |
| Local relief | amplitude | |
| Short-lag roughness | amplitude | Sets the S4 ceiling. |

## Signature exaggeration

**Heather colour saturation in bloom.** The purple is pushed beyond
photographic accuracy. Heathland's terrain signature is subtle — its visual
signature is not, and this is where it is spent.

## Open questions

1. **Does negative integration need its own code path?** Flagged in
   [S2's open questions](../stages/stage-02-skeleton-kernel.md). If a deranged
   network cannot be produced by the same growth with a dial turned down, that
   is a second kernel wearing a module's clothes and should be admitted as
   such. **This is the biggest open architectural risk in the base six.**
2. **Is network connectivity in the battery?** It is identity-defining for
   heathland and is not currently a standard terrain metric. It needs adding
   and validating.
3. **Are seasonal water bodies modelled downstream?** Heathland is the only
   biome that produces them. If nothing reads `permanent: false`, drop the flag
   rather than carry a lie — see
   [S3's open questions](../stages/stage-03-transforms.md).
4. **Kettle scale.** Real kettles range from a few metres to hundreds. Golf
   wants the middle of that range; does the point process need an explicit size
   distribution, or does the water table naturally select?
