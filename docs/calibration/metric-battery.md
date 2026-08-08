# Metric Battery

The 22 metrics that define what a biome *is*, and which stage dial each one
calibrates.

## The purity rule

Every metric is a **pure function of `(height, cell_size, mask)`**. No I/O, no
geographic types, no knowledge of whether it is looking at a lidar tile or a
generated course.

This is the load-bearing property of the whole calibration architecture. Because
the function is pure, the *same code* measures a 3DEP tile and an
[S11](../stages/stage-11-validation.md) output, so real and synthetic terrain
land in one feature space and can be compared directly. Two implementations —
one in the ingestion path, one in the validation path — would drift, and the
drift would be indistinguishable from a generator improvement.

The rule has a concrete consequence for
[S11](../stages/stage-11-validation.md): it must call the same battery, not a
reimplementation. Test both paths against a shared analytic surface.

## Prior art

- **`tools/metrics/metrics/core.py`** (815 lines) — the pure kernels, including
  a pure-numpy priority-flood and D8. Already correct, already tested against
  analytic surfaces (plane, tilt, cone, sinusoid, fBm) in
  `tools/metrics/tests/test_metrics.py`. **Reuse; do not rewrite.**
- **`tools/metrics/metrics/features.py`** — `compute()`, the single assembly
  point. 49 scalars plus 4 distribution vectors on shared axes.
- **`tools/metrics/METRICS.md`** — per-metric definitions, units, detrend
  policy, resolution guards.
- **`tools/dtm_metrics/dtm_metrics/gates.py`** (241 lines) — the G1–G6
  admission gates. See below.

The v2 battery of 22 is a **subset of the existing 49, plus 4 new metrics**. The
v1 calibration harness used 37 and matched on 32; the reduction to 22 is a
deliberate narrowing to metrics that pass admission and have a named consumer.

## Admission gates

Not every metric that can be computed is a legitimate fit target.
`tools/dtm_metrics/dtm_metrics/gates.py` already implements the classification
and it should be run before any metric is trusted:

| Gate | Question |
|---|---|
| G1 | Injection robustness — does it respond to the signal it claims to measure? |
| G2 | Mask stability — does it survive a perturbed validity mask? |
| G3 | Split-half ICC — is it reliable within a single tile? |
| G4 | θ-share — does generator parameter variation move it more than seed variation? |
| G5 | Redundancy clustering — is it distinct from its neighbours? |
| G6 | Emulator identifiability — can it be inverted to a parameter? |

Output is a classification per metric:

- **`fit_target`** — may be used to fit a dial.
- **`validation`** — measured and reported, never fitted. Fitting to a metric
  that fails G4 or G6 optimizes noise.
- **`diagnostic`** — for humans, not for the loop.

**A metric's family, its role, and its gate class are three independent axes.**
An amplitude metric can be a fit target; a process metric can fail admission
and be validation-only.

## Axis 1 — process versus amplitude (how to match)

**Process metrics** describe *how the landscape is organized* — drainage
density, slope–area scaling, network connectivity, landform proportions. They
are nearly constant within a biome and **must match pointwise**, within a
narrow band.

**Amplitude metrics** are per-site continuous descriptors — local relief,
elevation range, variogram sill. They vary widely *within* a biome: real
piedmont sites range from 20 m to 90 m of local relief and all of them are
piedmont. They are matched **in distribution**, not pointwise, and S0's
descriptors draw from that distribution.

Getting this backwards produces the two classic failures: matching amplitude
pointwise gives every course in a biome the same relief, and matching process
in distribution gives biomes that blur into each other.

## Axis 2 — shared invariant versus discriminant (what it tells you)

The axis this project learned the hard way, and the one that reorganized the
whole target set.

**Shared invariants** are quantities every real archetype hits at nearly the
same value. The generator **must hit them** — they are what makes terrain look
like Earth — but hitting them tells you nothing about *which* biome you built.

The evidence: `tools/macro_campaign/macro_campaign/structure.py` measures
`dist_to_channel_p50` at **104–120 m in every archetype regardless of relief**,
and flags it as the most robust metric in the set. A sandhills tile and a
hill-country tile have nearly the same channel spacing. Drainage density is a
law of landscapes, not a signature.

**Discriminants** are the quantities that actually differ between archetypes —
texture, hillslope form, orientation, landform proportions. **These are what
identity means.** `METRICS.md` already singles out `spectral_slope_beta` as
*"the primary roughness invariant"*, and it is a discriminant in exactly this
sense: it separates.

Two consequences that shape the pipeline:

1. **[S2](../stages/stage-02-skeleton-kernel.md) is fitted almost entirely to
   shared invariants.** Its job is to be correct, not distinctive. If generated
   piedmont and generated sandhills differ measurably in drainage density,
   something is wrong — because the real ones do not.
2. **[S3](../stages/stage-03-amplification.md) carries identity.** Every
   discriminant except `network_connectivity` is a texture or form metric, and
   texture is what the patch dictionary reconstructs from real terrain.

An earlier draft of this document called drainage density "identity-defining"
for four biomes. That was wrong, and it would have aimed the entire calibration
effort at a metric that cannot separate anything.

## The 22

**I** = shared invariant · **D** = discriminant · ★ = not in the existing
battery, needs implementing and gating.

| # | Metric | Family | Role | Calibrates | Stage |
|---|---|---|---|---|---|
| **Relief & hypsometry** |||||
| 1 | `local_relief_p50` | amplitude | D | relief budget | [S0](../stages/stage-00-archetype-draw.md), [S1](../stages/stage-01-macro-primitives.md) |
| 2 | `elevation_range` | amplitude | D | relief budget | S0 |
| 3 | `hypsometric_integral` | process | I | catena form | [S2](../stages/stage-02-skeleton-kernel.md) |
| 4 | `hypsometric_bimodality` ★ | process | **D** | floor/step separation | S2 |
| **Slope** |||||
| 5 | `slope_p50` | amplitude | D | relief amplitude | S1, S2 |
| 6 | `slope_p90` | amplitude | D | steepness tail | S2 |
| 7 | `slope_bimodality` ★ | process | **D** | caprock/scarp structure | S2 stratigraphy |
| 8 | `slope_entropy` | process | D | slope organization | S2 |
| **Drainage & network** |||||
| 9 | `drainage_density` | process | **I** | tributary spacing | S2 |
| 10 | `dist_to_channel_p50` | process | **I** | flow-distance scale | S2 |
| 11 | `slope_area_theta` | process | **I** | profile-law exponent θ | S2 catena |
| 12 | `horton_ratios` ★ | process | **I** | bifurcation / length ratios | S2 hierarchy |
| 13 | `network_connectivity` ★ | process | **D** | **drainage integration dial** | **S2** |
| **Spectrum & texture** |||||
| 14 | `spectral_slope_beta` | process | **D — primary** | **dictionary bucket spectra** | [S3](../stages/stage-03-amplification.md) |
| 15 | `variogram_range` | process | **D** | detail wavelength band | S3, [S9](../stages/stage-09-micro-repass.md) |
| 16 | `variogram_sill` | amplitude | **D** | detail amplitude | S3, S9 |
| 17 | `short_lag_roughness` | amplitude | **D** | fine texture | S3 |
| **Curvature** |||||
| 18 | `mean_abs_profile_curv` | process | **D** | **hillslope form (spur/hollow)** | **S3** |
| 19 | `plan_curv_p90` | process | **D** | convergence structure | S3 |
| **Anisotropy** |||||
| 20 | `anisotropy_ratio` | process | **D** | grain strength | S1, S3 |
| 21 | `anisotropy_orientation_deg` | process | I | orientation coherence | S1 → S3 (the thread) |
| **Landform organization** |||||
| 22 | `tpi_landform_fractions` | process | **D** | flat/riser/tread proportions | S2 stratigraphy, S3 |

Count the roles: **6 shared invariants, 16 discriminants**, and of the
discriminants **9 are fitted at [S3](../stages/stage-03-amplification.md)**.
That distribution is the architecture in one table — the skeleton hits the laws
of landscapes, the dictionary carries the identity.

Dropped from the earlier draft: `flow_accum_gini` and `spectral_break_wavelength`
(both redundant against retained metrics under G5), and
`bench_step_count_per_relief` / `terrace_step_spacing`, which were four separate
proposals for measuring stepped structure that `tpi_landform_fractions` plus
`slope_bimodality` cover between them. Added: `spectral_slope_beta`,
`horton_ratios`, and the curvature pair.

### The new metrics

Each exists because a v2 biome's identity cannot be expressed without it, and
none is a standard terrain metric. All need implementing in `core.py` and
putting through G1–G6 before use.

- **`network_connectivity`** ★ — fraction of channel length that reaches base
  level. Near zero for [Heathland](../biomes/heathland.md), near one for
  everything else. **The single number that defines heathland**, and the only
  discriminant S2 owns. Without it the biome cannot be fit at all.
- **`hypsometric_bimodality`** ★ and **`slope_bimodality`** ★ —
  [Great Plains](../biomes/great-plains.md) and
  [River Valley](../biomes/river-valley.md) both have genuinely bimodal
  distributions (surface/scarp, floor/terrace). A unimodal fit to a bimodal
  truth **scores acceptably and is wrong**, which is the worst kind of
  calibration failure because nothing flags it. `slope_bimodality` also carries
  Hill Country's stepped slopes — treads and risers are two modes.
- **`horton_ratios`** ★ — bifurcation and length ratios of the Strahler
  hierarchy. A shared invariant: real networks cluster tightly (bifurcation
  3–5, length ~2) whatever the biome. It exists to catch a *flat* generated
  network — one where every tributary is the same size, which reads wrong even
  at perfectly correct density.
  `tools/macro_campaign/macro_campaign/netstats.py` already implements
  `_strahler()` and the junction statistics, so this is a wrapper, not new
  science.

Note that `spectral_slope_beta`, `mean_abs_profile_curv`, `plan_curv_p90`, and
`anisotropy_ratio` are **not** new — they are already in
`tools/metrics/metrics/core.py`. They were simply not being used as identity
metrics, which was the mistake.

## Distribution vectors

Alongside the 22 scalars, four distributions on shared axes, used for
distribution-matching rather than pointwise comparison: `slope_hist`,
`radial_psd`, `variogram`, `hypsometric_curve`. These come from the existing
`features.py` unchanged.

`radial_psd` and `variogram` do double duty for
[S3](../stages/stage-03-amplification.md): besides matching texture, they are
the **seam detectors**. Patch synthesis that tiles visibly shows up as a
spurious peak at the patch pitch or its harmonics, and nothing else in the
battery would catch it.

## Resolution guards

Every metric declares the resolutions at which it is valid.
`variogram_range` measured on a 2 m grid cannot resolve structure below ~6 m;
`drainage_density` depends on the channel-extraction threshold, which depends
on cell size. **Comparing a metric across resolutions without a guard is the
most common way to produce a confident wrong number.** The guards exist in
`METRICS.md`; carry them forward.

Since real tiles and generated courses are both at 2 m
([lidar-pipeline.md](lidar-pipeline.md)), this mostly does not bite — but it
will the moment someone measures a preview-resolution run.

## Open questions

1. **Are the new metrics admissible?** They may fail G4 (θ-share) or G6
   (identifiability). If `network_connectivity` fails, heathland cannot be
   fitted and the biome definition needs revisiting rather than the metric.
   **Measured caveat (B4, 2026-08-07):** on unconditioned real DTMs, road
   embankments dam streams at culverts, so even integrated piedmont reads
   ~0.2. The metric separates perfectly on analytic surfaces (integrated 1.0,
   kettle field 0.36, shallow-dimpled 1.0 — the pit-depth tolerance works);
   its *corpus* use needs road-mask conditioning or culvert breaching first.
2. **Is the invariant/discriminant split itself measurable?** It should be:
   run every metric across the six biome corpora and compute between-biome
   variance over within-biome variance. **The split above is a hypothesis based
   on one measurement** (`dist_to_channel_p50`) and should be confirmed
   empirically for all 22 before targets are set. This is cheap and it would be
   embarrassing to skip.
3. **Is 22 the right number?** Narrowed from 37 for consumer-directedness. If
   the emulator cannot identify a dial from the reduced set, metrics come back.
4. **A bimodality measure that is not fragile.** Dip tests and mixture fits are
   both sensitive to bandwidth, and two biomes' identities rest on one. Needs a
   robust choice before implementation.
5. **Do the discriminants need to be measured on the residual specifically,
   rather than the finished surface?** S3 is fitted to residual statistics, but
   the battery measures whole terrain. Measuring both — residual for fitting,
   whole surface for validation — is probably right and needs deciding before
   the dictionary is built.
6. **Does the battery need a golf-specific metric?** Everything here is
   geomorphology. Nothing measures whether terrain is *good for golf* — that is
   [S11](../stages/stage-11-validation.md)'s playability sim, plus the siting
   scorer at [S5](../stages/stage-05-siting-substrate.md), neither of which can
   be measured on a lidar tile. The gap is deliberate; the 64 real course grids
   are how it gets closed.
