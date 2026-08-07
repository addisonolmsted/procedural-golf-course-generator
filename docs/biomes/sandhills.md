# Sandhills

**Role: the over-constrained routing case, and the empty-collection stress
test.** Worth adding as a third golden alongside piedmont and heathland — it
produces three cost fields identically zero and no channels at all, which no
other base biome does.

## Identity

Stabilized aeolian dune trains on deep sand: grass-anchored dunes with
blowouts where the cover has broken, and **no surface water whatsoever**.
Rainfall infiltrates immediately. There are no creeks, no ponds, and no
drainage network to speak of.

The golf is the reason the biome exists. Wide sandy corridors between dune
ridges, greens in natural hollows, bunkers that are blowouts rather than
constructions. The land already is a golf course; the job is to find it, not to
build it.

## Why it stresses the pipeline

Sandhills breaks the fluvial engine's assumption from the opposite direction to
[heathland](heathland.md): heathland has a network that does not connect,
sandhills has **no network at all**.

- **[S2](../stages/stage-02-skeleton-kernel.md):** density ≈ 0. The engine runs,
  computes a flow field, and grows zero channels. Every downstream consumer must
  be correct on an empty channel list.
- **[S3](../stages/stage-03-transforms.md):** a full **identity pass** — runs
  the ordered transform list and applies nothing. Empty `water`, empty closed
  `basins`.
- **[S5](../stages/stage-05-substrate-assembly.md):** `water`, `clearing`, and
  `borrow` all identically zero. **Three fields at zero simultaneously** — the
  case most likely to divide by zero in a naive scorer.
- **[S6](../stages/stage-06-routing.md):** must route on that substrate without
  a special case, at low plasticity.
- **[S7](../stages/stage-07-earthmoving.md):** near-identity. Grading the dunes
  away is the one unforgivable failure in this biome.

## Kernel & modules

Kernel I (fluvial), producing no fluvial features.

| Module | Intensity | Expression |
|---|---|---|
| Trunk-river | **zero** | |
| Stratigraphy / bench | **zero** | Empty strata; deep uniform sand. |
| Closed basin | **low** | Interdune hollows, but dry — no water table to fill them. |
| Aeolian | **dominant** | Dune trains, blowouts, and the **S1/S2 amplitude fork** that sets dune scale. |
| Drainage integration | **n/a** | Meaningless with no network. Set to zero. |

## The S1/S2 amplitude fork

Dune amplitude is currently split across two stages: S1 contributes the
long-wavelength dune-train envelope through `relief`, and S2's aeolian module
contributes the individual dune form. The fork exists because dune trains have
structure at two clearly separated scales.

It is also flagged as an open question in
[S2's doc](../stages/stage-02-skeleton-kernel.md) — it may be cleaner to put
the whole thing in S2's aeolian module. Resolve against the corpus.

## Stage dials

| Stage | Setting |
|---|---|
| [S1](../stages/stage-01-macro-primitives.md) | Mid relief amplitude; **empty strata**; uniform `hardness`; **high `grain_strength` aligned to `wind_azimuth_rad`**. The one biome where grain is aeolian rather than material — same field, different physical origin, no branch. |
| [S2](../stages/stage-02-skeleton-kernel.md) | Density ≈ 0. Aeolian module dominant. |
| [S3](../stages/stage-03-transforms.md) | Identity pass. Water table far below the surface. |
| [S4](../stages/stage-04-finishers.md) | **Low, diffusion-only.** Sand does not rill: rilling intensity is exactly zero, creep carries all the weight. The clearest example in the base six of a module dialed to zero rather than branched around. |
| [S9](../stages/stage-09-micro-repass.md) | **Aeolian rumple dominant**, strongly aligned to `wind_azimuth_rad`. Swales at zero (no flow). **Highest micro amplitude of the six.** |
| [S10](../stages/stage-10-zoning-aesthetics.md) | Open sand and native grass; near-zero canopy. |

## Scorer preset & plasticity

**Plasticity: low** — the lowest in the base six, tied with Hill Country. The
dunes *are* the golf course.

The routing problem is **over-constrained**: strongly preferred corridors
between dune ridges, and little freedom to deviate because grading is not
affordable. `w_fit` dominates. The opposite end of the spectrum from
[Great Plains](great-plains.md), and together the two bracket routing
difficulty.

## Calibration targets

**Corpus: v1 sandhills tiles exist (20 extracted) and must be re-measured.**
Thin — 20 tiles is the smallest of the surviving v1 sets — and worth scaling up
given the biome's role as an empty-case golden.

| Metric | Family | Notes |
|---|---|---|
| **Drainage density (≈ 0)** | **process, identity-defining** | Trivially satisfied, but must be *measured*, not assumed. |
| **Directional variogram along wind azimuth** | **process** | Dune spacing. The primary S2 and S9 fit target. |
| Spectral break | process | The two-scale dune-train structure; validates the S1/S2 fork. |
| Anisotropy ratio | process | Sandhills is the most anisotropic biome in the set. |
| Local relief | amplitude | Dune height. |
| Short-lag roughness | amplitude | Sets S4's diffusion-only intensity. |

## Signature exaggeration

**Blowout scale.** Sand faces larger and steeper than reality — the visual
signature of the biome, and what makes a sandhills hole photograph the way
sandhills courses do.

## Open questions

1. **Where does the S1/S2 amplitude fork live?** See above and
   [S2](../stages/stage-02-skeleton-kernel.md). Leaning "all in S2".
2. **Is diffusion-only enough for S4?** Dune slip faces may need a distinct
   avalanche pass. If so it is a fourth pass at zero intensity elsewhere, not a
   branch — see [S4's open questions](../stages/stage-04-finishers.md).
3. **Should sandhills be a formal third golden?** It exercises the empty-case
   paths no other biome reaches. Recommendation in
   [S11's doc](../stages/stage-11-validation.md): yes.
4. **Is 20 tiles enough** for a biome whose identity rests on a directional
   variogram? Probably not. Scale-up should precede fitting.
5. **Interdune hollows and the closed-basin module.** They are closed basins
   with no water. Does the module produce them, or does the aeolian module?
   Currently ambiguous.
