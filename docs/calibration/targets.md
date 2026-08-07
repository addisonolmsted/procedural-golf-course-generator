# Calibration Targets

The per-biome target vectors the generator is fit to, and — for now — an honest
accounting of which of them do not exist yet.

## Read this first: the corpus is missing

**No v2 target vector has been measured.** Every number below is a placeholder
or absent. This is the pipeline's largest open data debt and it blocks
[S0](../stages/stage-00-archetype-draw.md),
[S2](../stages/stage-02-skeleton-kernel.md),
[S4](../stages/stage-04-finishers.md), and
[S9](../stages/stage-09-micro-repass.md).

The v1 corpus — 180 real 3DEP tiles, 2.3 GB, already extracted and culled — is
keyed to the **retired** archetype set (`florida_lowland`, `glacial_moraine`,
`mountain_bench`, `piedmont`, `sandhills`). The v2 biome set is a clean break,
and **none of the fitted data carries over**: not the 88-knob
`archetype_priors.json`, not the quantile tables, not the targets.

What *can* be salvaged is tiles, not fits — and only for three biomes:

| Biome | Corpus status | Action |
|---|---|---|
| [Piedmont](../biomes/piedmont.md) | v1 tiles exist (28 extracted) | **Re-measure** against the v2 battery. Best-supplied. |
| [Sandhills](../biomes/sandhills.md) | v1 tiles exist (20 extracted) | **Re-measure**, and **scale up** — 20 is thin for a biome whose identity is a directional variogram. |
| [Hill Country](../biomes/hill-country.md) | partial — v1 `mountain_bench` (28) is *related* but a different definition (mountain front vs dissected plateau) | Use as a seed; **supplement substantially**. Targets not transferable. |
| [Heathland](../biomes/heathland.md) | **none** — v1 `glacial_moraine` was a different definition | **Fresh campaign.** Highest priority: it is a golden archetype. Needs a non-US source (UK/NL heath). |
| [Great Plains](../biomes/great-plains.md) | **none** | **Fresh campaign.** US High Plains, Llano Estacado, southern Alberta. |
| [River Valley](../biomes/river-valley.md) | **none** | **Fresh campaign.** Mature meandering rivers with preserved terrace flights. |

Additionally, **four of the 22 metrics do not exist yet** —
`network_connectivity`, `hypsometric_bimodality`, `slope_bimodality`,
`bench_step_count_per_relief`, `terrace_step_spacing` (see
[metric-battery.md](metric-battery.md)) — so even the biomes with tiles cannot
be fully measured until those are implemented and gated.

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

Two shapes, matching the two metric families
([metric-battery.md](metric-battery.md)):

**Process metrics — identity.** A point value and a narrow band. The generated
value must land inside the band, pointwise, for ≥ 90% of seeds. These define
what the biome *is*, and they are nearly constant within it.

```
drainage_density: { target: 2.4, band: [2.1, 2.7], unit: "channel-km/km^2" }
```

**Amplitude metrics — per-site descriptors.** A distribution. The generated
*population* over many seeds must match the real population; individual courses
range freely within it. This is what makes two piedmont courses different sites
rather than the same site twice.

```
local_relief_p50: { dist: "lognormal", p25: 31.0, p50: 44.0, p75: 63.0, unit: "m" }
```

**A band is not a tolerance.** It is the measured spread of real landscapes of
that type. A wide band means the biome genuinely varies; it does not mean the
generator may be sloppy. Bands come from the corpus, never from what the
generator happens to achieve — fitting the band to the output is circular and
is the failure mode to watch for when a target proves hard to hit.

## Target vector template

Each biome gets one file section per this template once measured. Reproduced
here so the shape is fixed before the data arrives:

```
biome: <id>
corpus:
  n_tiles: <int>
  source: <3DEP | EA LIDAR | AHN | ...>
  exclusion_digest: <hex>       # provenance: which cull produced this
  battery_version: <int>
  despeckle_sensitivity: <band> # see the caveat below

process:                        # pointwise, narrow bands
  drainage_density:        { target, band }
  network_connectivity:    { target, band }
  slope_area_theta:        { target, band }
  hypsometric_integral:    { target, band }
  hypsometric_bimodality:  { target, band }
  slope_bimodality:        { target, band }
  slope_entropy:           { target, band }
  dist_to_channel_p50:     { target, band }
  flow_accum_gini:         { target, band }
  variogram_range:         { target, band }
  spectral_break_wavelength: { target, band }
  anisotropy_ratio:        { target, band }
  anisotropy_axis_alignment: { target, band }
  tpi_landform_fractions:  { target: [f_flat, f_riser, f_tread], band }
  bench_step_count_per_relief: { target, band }
  terrace_step_spacing:    { target, band }

amplitude:                      # distributional
  local_relief_p50:  { dist, p25, p50, p75 }
  elevation_range:   { dist, p25, p50, p75 }
  slope_p50:         { dist, p25, p50, p75 }
  slope_p90:         { dist, p25, p50, p75 }
  variogram_sill:    { dist, p25, p50, p75 }
  short_lag_roughness: { dist, p25, p50, p75 }
```

## Identity-defining metrics

The subset that carries each biome's identity — where a miss is a failure, not
a deviation:

| Biome | Identity-defining metrics |
|---|---|
| Piedmont | `drainage_density`, `slope_area_theta`, `hypsometric_integral` |
| Great Plains | `slope_bimodality`, `tpi_landform_fractions`, `drainage_density` (low) |
| River Valley | `terrace_step_spacing`, `hypsometric_bimodality`, meander wavelength |
| Sandhills | `drainage_density` (≈ 0), `anisotropy_ratio`, directional variogram |
| Heathland | **`network_connectivity`** (≈ 0), `drainage_density` (~0.07) |
| Hill Country | `bench_step_count_per_relief`, `tpi_landform_fractions` |

## Known measurement hazards

1. **The despeckle band.** [S9](../stages/stage-09-micro-repass.md)'s
   `variogram_range`/`sill` and [S4](../stages/stage-04-finishers.md)'s
   `short_lag_roughness` are the metrics most sensitive to the despeckle
   setting, and there is no single correct setting — see
   [lidar-pipeline.md](lidar-pipeline.md). **Report these as bands widened by a
   sensitivity analysis**, not as point estimates.
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

1. **What is the minimum viable corpus per biome?** 30 tiles is the working
   target, but a biome resting on a directional statistic (Sandhills) or a
   bimodality (Great Plains) plausibly needs more. Determine by bootstrapping
   the target's confidence interval against tile count on Piedmont first.
2. **Cross-source comparability.** Heathland needs UK/NL data. Are metrics
   measured on EA LIDAR comparable to 3DEP? Must be established before
   heathland's targets are trusted, or the biome is fit to a sensor difference.
3. **Should amplitude distributions be fit jointly?** Local relief and slope are
   correlated; independent marginals produce parameter combinations that exist
   nowhere. **This was measurably v1's problem** — its prior had no covariance
   at all. Joint fitting is the presumed answer and it is the reason S0 draws
   descriptors jointly.
4. **Do the golden archetypes need tighter bands** than the rest? They are the
   regression baseline, so arguably yes.
