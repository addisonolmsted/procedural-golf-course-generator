# Calibration Targets

The per-biome target vectors the generator is fit to, and — for now — an honest
accounting of which of them do not exist yet.

## Read this first: the corpus is missing

**No v2 target vector has been measured.** Every number below is a placeholder
or absent. This is the pipeline's largest open data debt and it blocks
[S0](../stages/stage-00-archetype-draw.md),
[S2](../stages/stage-02-skeleton-kernel.md),
[S3](../stages/stage-03-amplification.md), and
[S9](../stages/stage-09-micro-repass.md).

The v1 corpus is keyed to the **retired** archetype set (`florida_lowland`,
`glacial_moraine`, `mountain_bench`, `piedmont`, `sandhills`). The v2 biome set
is a clean break, and **none of the fitted data carries over**: not the 88-knob
`archetype_priors.json`, not the quantile tables, not the targets.

It is also thinner than it looks. Of 90 tiles on disk, **52 are excluded** and
only **38 are clean**:

| Biome | Corpus status | Action |
|---|---|---|
| [Piedmont](../biomes/piedmont.md) | v1 tiles exist, **6 clean** of 14 | **Re-measure** against the v2 battery, and **scale up**. Best-supplied and still thin. |
| [Sandhills](../biomes/sandhills.md) | v1 tiles exist, **10 clean** of 10 | **Re-measure**, **scale up** — thin for a biome whose identity is a directional variogram. |
| [Hill Country](../biomes/hill-country.md) | partial — v1 `mountain_bench`, **8 clean** of 14, and a *different definition* (mountain front vs dissected plateau) | Seed only; **supplement substantially**. Targets not transferable. |
| [Heathland](../biomes/heathland.md) | **none** — v1 `glacial_moraine` was a different definition and is down to **5 clean** tiles | **Fresh campaign.** Highest priority: golden archetype. Needs a non-US source (UK/NL heath). |
| [Great Plains](../biomes/great-plains.md) | **none** | **Fresh campaign.** US High Plains, Llano Estacado, southern Alberta. |
| [River Valley](../biomes/river-valley.md) | **none** | **Fresh campaign.** Mature meandering rivers with preserved terrace flights. |

**The campaign target is ~30 clean tiles per biome, ~180 total.** At the v1
attrition rate (~40%) that means fetching ~300.

### Exemplar synthesis needs more than metric fitting does

[S3](../stages/stage-03-amplification.md)'s patch dictionary is fitted from the
same corpus, and its requirement is different in kind: metric fitting needs
enough tiles for a stable *mean*, but a dictionary needs **coverage of the
conditioning space** — flow distance × hillslope position × slope × aspect ×
TPI × relief position.

Patch *count* is not the constraint. One 1500² tile yields thousands of
overlapping 32² patches, so even 38 tiles give millions. **Diversity is the
constraint**: a bucket populated entirely from one tile will reproduce that
tile. Report per-bucket **tile diversity**, not just patch count, and treat a
bucket sourced from fewer than ~5 distinct tiles as under-supported.

### Also missing: the metrics themselves

**Four of the 22 metrics do not exist yet** — `network_connectivity`,
`hypsometric_bimodality`, `slope_bimodality`, `horton_ratios` (see
[metric-battery.md](metric-battery.md)) — so even the biomes with tiles cannot
be fully measured until those are implemented and gated.

And `tools/metrics` / `tools/dtm_metrics` are currently **non-runnable**: both
read `tools/parkland_atlas/out/cache`, which is absent from this branch.
Recover from `main` before anything else here can start.

### Recommended collection order

1. **Piedmont** — re-measure. It is the anchor; every other biome's targets are
   validated partly by how they differ from it, so nothing else can be
   sanity-checked until it exists.
2. **Heathland** — golden archetype, and the biome most likely to invalidate an
   architectural assumption ([S2](../stages/stage-02-skeleton-kernel.md)'s
   negative-integration question). Find out early.
3. **Sandhills** — golden candidate, empty-case coverage, cheap scale-up.
4. **Hill Country**, **Great Plains**, **River Valley** — in whatever order
   collection is practical.

## How a target vector is expressed

Two shapes for *how* to match (process vs amplitude), crossed with two roles
for *what it tells you* (shared invariant vs discriminant). See
[metric-battery.md](metric-battery.md).

**Process metrics.** A point value and a narrow band. The generated value must
land inside the band, pointwise, for ≥ 90% of seeds.

```
drainage_density: { target: 2.4, band: [2.1, 2.7], unit: "channel-km/km^2" }
```

**Amplitude metrics.** A distribution. The generated *population* over many
seeds must match the real population; individual courses range freely within
it. This is what makes two piedmont courses different sites rather than the
same site twice.

```
local_relief_p50: { dist: "lognormal", p25: 31.0, p50: 44.0, p75: 63.0, unit: "m" }
```

**A band is not a tolerance.** It is the measured spread of real landscapes of
that type. A wide band means the biome genuinely varies; it does not mean the
generator may be sloppy. Bands come from the corpus, never from what the
generator happens to achieve — fitting the band to the output is circular and
is the failure mode to watch for when a target proves hard to hit.

### Shared invariants are written once, not per biome

A shared invariant has, by definition, nearly the same value in every
archetype — so it belongs in **one global block**, not repeated six times with
cosmetically different numbers.

This is not a formatting preference. Six per-biome entries invite six slightly
different fitted values, and the generator would then reproduce differences
that do not exist in the real world. **A single shared block makes the
invariance structural.** If a biome genuinely departs on one of these, that is
a finding worth arguing for explicitly, not something that slips in through
independent fitting.

```
shared_invariants:              # all six biomes, one block
  drainage_density:    { target, band }
  dist_to_channel_p50: { target, band }   # ~104-120 m, measured
  slope_area_theta:    { target, band }
  hypsometric_integral: { target, band }
  horton_ratios:       { bifurcation: {target, band}, length: {target, band} }
  anisotropy_orientation_deg: { follows: grain_axis_rad, tol_deg }
```

**Per-biome files then carry discriminants only** — which is a much shorter and
much more honest document than the previous draft, because it contains exactly
the quantities that actually differ.

## Target vector template

Each biome gets one file section per this template once measured. Reproduced
here so the shape is fixed before the data arrives:

```
biome: <id>
corpus:
  n_tiles: <int>
  n_clean: <int>                # after the cull -- report BOTH
  source: <3DEP | EA LIDAR | AHN | ...>
  exclusion_digest: <hex>       # provenance: which cull produced this
  battery_version: <int>
  despeckle_sensitivity: <band> # see the caveat below

discriminants:                  # what makes this biome this biome
  process:                      # pointwise, narrow bands
    network_connectivity:    { target, band }
    spectral_slope_beta:     { target, band }   # the primary one
    variogram_range:         { target, band }
    mean_abs_profile_curv:   { target, band }
    plan_curv_p90:           { target, band }
    anisotropy_ratio:        { target, band }
    tpi_landform_fractions:  { target: [f_flat, f_riser, f_tread], band }
    slope_bimodality:        { target, band }
    hypsometric_bimodality:  { target, band }
    slope_entropy:           { target, band }
  amplitude:                    # distributional
    local_relief_p50:    { dist, p25, p50, p75 }
    elevation_range:     { dist, p25, p50, p75 }
    slope_p50:           { dist, p25, p50, p75 }
    slope_p90:           { dist, p25, p50, p75 }
    variogram_sill:      { dist, p25, p50, p75 }
    short_lag_roughness: { dist, p25, p50, p75 }

exemplar_pool:                  # S3's dictionary source
  tiles: [<id>, ...]
  bucket_coverage: { <bucket>: { n_patches, n_distinct_tiles } }
  course_grids: [<id>, ...]     # weighted separately -- built, not natural
```

## What carries each biome's identity

The discriminants where a miss is a failure, not a deviation. Note how many of
them are **texture** — that is [S3](../stages/stage-03-amplification.md)'s
dictionary doing the work, not the skeleton.

| Biome | Identity-carrying discriminants |
|---|---|
| Piedmont | `mean_abs_profile_curv` (spur/hollow), `spectral_slope_beta`, `variogram_range` |
| Great Plains | `slope_bimodality`, `tpi_landform_fractions`, low `variogram_sill` |
| River Valley | `hypsometric_bimodality`, within-tile texture contrast (floodplain vs valley side) |
| Sandhills | `anisotropy_ratio`, directional variogram. **The exception**: also `drainage_density` ≈ 0, the one place a shared invariant genuinely departs — no channels at all. |
| Heathland | **`network_connectivity`** (≈ 0). The only biome whose identity S2 carries. |
| Hill Country | `slope_bimodality` (treads vs risers), `tpi_landform_fractions` |

## Known measurement hazards

1. **The despeckle band.** [S3](../stages/stage-03-amplification.md)'s
   `variogram_range`/`sill`, `spectral_slope_beta`, and `short_lag_roughness`
   are the metrics most sensitive to the despeckle setting, and there is no
   single correct setting — see [lidar-pipeline.md](lidar-pipeline.md).
   **Report these as bands widened by a sensitivity analysis**, not as point
   estimates.

   This matters far more now than it did. Under the old design despeckle
   affected an erosion intensity dial; under the new one it determines **what
   the dictionary contains**. Under-despeckle and S3 reconstructs sensor noise
   as terrain, faithfully, everywhere.
2. **Canopy bias.** Bare-earth DTM under dense canopy is smoother than the
   ground. Piedmont's roughness targets are biased low by an unknown amount.
   Record canopy fraction per tile and check for correlation; if the target
   tracks canopy, it is measuring the classifier.
3. **Bimodality is fragile.** Dip tests and mixture fits both depend on
   bandwidth. Great Plains and River Valley both rest on a bimodality metric,
   so a fragile estimator puts two biomes' identities on sand.
4. **Circular bands.** Stated above and worth repeating: bands come from the
   corpus. If a target proves hard to hit, the answer is a generator change or
   an admitted failure — never a wider band.
5. **Anthropogenic residue** is worst in Great Plains and River Valley, the two
   most-farmed biomes, and both need fresh campaigns. The QA cull is the only
   defence.

## Open questions

1. **Confirm the invariant/discriminant split empirically.** The split is a
   hypothesis extrapolated from one measurement. Compute between-biome variance
   over within-biome variance for all 22 metrics across the six corpora and let
   the data assign the roles. Cheap, and it should happen before any target is
   set. See [metric-battery.md](metric-battery.md) open question 2.
2. **What is the minimum viable corpus per biome?** 30 tiles is the working
   target for metric fitting, but the **dictionary's bucket-diversity
   requirement is the real constraint** and it is not yet quantified. Determine
   by bootstrapping both the target confidence interval and per-bucket
   diversity against tile count, on Piedmont first.
3. **Cross-source comparability.** Heathland needs UK/NL data. Are metrics
   measured on EA LIDAR comparable to 3DEP? Must be established before
   heathland's targets are trusted, or the biome is fit to a sensor difference.
   **Worse for the dictionary than for metrics** — a bucket mixing 3DEP and EA
   patches would blend two sensors' noise characteristics into one texture.
4. **Should amplitude distributions be fit jointly?** Local relief and slope are
   correlated; independent marginals produce parameter combinations that exist
   nowhere. **This was measurably v1's problem** — its prior had no covariance
   at all. Joint fitting is the presumed answer and it is the reason S0 draws
   descriptors jointly.
5. **How are structural-class weights fitted?** S0 now draws a categorical
   window class, and how often a real landscape of a given biome presents as a
   valley floor versus an interfluve is measurable from the corpus. Needs a
   classification procedure.
6. **Do the golden archetypes need tighter bands** than the rest? They are the
   regression baseline, so arguably yes.
