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
2. **[S3](../stages/stage-03-amplification.md): the taper bound is tightest
   here.** Heathland's network is the easiest in the base set to accidentally
   *integrate* — residual amplitude that would be unremarkable elsewhere can
   connect the disconnected channels. Heathland therefore sets the **hard
   envelope ceiling** on amplification amplitude
   ([../calibration/envelope-certification.md](../calibration/envelope-certification.md)).
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
| [S3](../stages/stage-03-amplification.md) | Low amplitude, hummocky, **weakly oriented** — `grain_strength` is low enough that orientation barely reads. Near-isotropic conditioning; a good check that the dictionary does not force anisotropy where the biome has none. Amplitude **capped hard** by the taper bound. |
| [S4](../stages/stage-04-hydrology.md) | **Water-table datum dominant.** The table sits near the surface, so S2's closed basins fill: kettle ponds and bogs. Seasonal ones marked `permanent: false`. No floodplain. |
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

| Metric | Role | Notes |
|---|---|---|
| **`network_connectivity`** ★ | **discriminant** | Fraction of channel length reaching base level. Near zero. **The single number that makes heathland heathland, and the only discriminant S2 owns** — every other biome's identity is texture. Needs adding to the battery ([../calibration/metric-battery.md](../calibration/metric-battery.md)). |
| `drainage_density` (~0.07) | discriminant (exception) | One of only two places a shared invariant genuinely departs; the other is Sandhills' zero. |
| `anisotropy_ratio` (low) | **discriminant** | Near-isotropic — the check that the dictionary does not force orientation where there is none. |
| `variogram_sill` (low) | **discriminant** | Hummocky, low amplitude. Sets the taper bound. |
| Closed-basin density | discriminant | Kettles per km². |
| `local_relief_p50` | amplitude | |

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
2. **Is `network_connectivity` admissible?** It carries heathland's entire
   identity and is not a standard terrain metric. It needs implementing and
   putting through the G1–G6 gates. **If it fails admission, heathland cannot
   be fitted** and the biome definition needs revisiting rather than the
   metric.
3. **Are seasonal water bodies modelled downstream?** Heathland is the only
   biome that produces them. If nothing reads `permanent: false`, drop the flag
   rather than carry a lie — see
   [S4's open questions](../stages/stage-04-hydrology.md).
4. **Kettle scale.** Real kettles range from a few metres to hundreds. Golf
   wants the middle of that range; does the point process need an explicit size
   distribution, or does the water table naturally select?
