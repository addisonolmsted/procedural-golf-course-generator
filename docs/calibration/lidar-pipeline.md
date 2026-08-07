# Lidar Pipeline

**Offline only.** Nothing in this document runs at generation time.

The job: turn public elevation data into clean, comparable terrain tiles that
the metric battery can measure, so that real and generated terrain land in one
feature space.

## Working prior art

All of this exists and runs on this branch — reuse it rather than rewriting.

| Module | Path | What it does |
|---|---|---|
| 3DEP fetcher | `tools/dtm_atlas/dtm_atlas/dep3.py` | USGS 3DEP `exportImage` GeoTIFF fetch, arbitrary bbox/SR/size, 1 m mosaic; plus 3DEPElevationIndex tier lookup. Keyless. |
| Grid writer | `tools/dtm_atlas/dtm_atlas/grids.py` | rasterio GeoTIFF writer; 2×2 block-mean 1 m → 2 m; deterministic profile. |
| Tile fetcher | `tools/macro_campaign/macro_campaign/fetch.py` | 3 km @ 2 m per tile in region UTM → **CGRID1**, the pipeline's own sidecar format. |
| CGRID1 codec | `tools/macro_campaign/macro_campaign/cgrid.py` | Mirrors `course-world::gridio`. This is what makes a real tile and a generated tile the same kind of object. |
| Hydrography | `tools/dtm_atlas/dtm_atlas/nhd.py` | NHD from `hydro.nationalmap.gov`. |
| Masks | `tools/dtm_atlas/dtm_atlas/{osm,masks}.py` | OSM feature masks via `rasterio.features.rasterize`. |
| Tile selection | `tools/tile_scout/` | F0 coarse (60 m regional screen) → F1 OSM → F2 terrain (10 m per-candidate), lattice, GMM scoring. |
| QA cull | `tools/macro_campaign/` + `crates/tile-lab/` | The exclusion loop, with per-tile reasons and an exclusion digest stamped into the fitted output. |

**Committed indices** (data itself is gitignored and refetchable):
`tools/dtm_atlas/dtm_atlas/tiles_us.json` — 205 tiles with `best_course`,
`center_ll`, `region`, `f2_dist`, `cfg_hash` — and `courses_us.txt`, ~135 US
course keys.

## The pipeline

### 1. Acquisition

**Source: USGS 3DEP**, keyless, via `exportImage`. Request in the tile's
**region UTM zone**, not in geographic coordinates — resampling from a
geographic grid introduces latitude-dependent anisotropy, which would be
measured as terrain anisotropy and taught to the generator as fabric.

**Native resolution 1 m** where available, block-meaned 2×2 to **2 m**. Two
reasons for 2 m rather than 1 m: it matches `RES_FULL_M`
([../01-conventions.md](../01-conventions.md)), so real and generated tiles are
directly comparable without resampling either; and 2× block-mean is a
well-understood, non-directional smoother, unlike an arbitrary despeckle.

**Tile size 3 km × 3 km**, matching `EXTENT_M`. Same reason.

**Format: CGRID1** — the same binary the generator writes. One codec, two
producers.

### 2. Physiographic pairing

Tiles are selected by **physiographic province**, not by proximity to a golf
course. This is a deliberate change from the v1 campaign, which selected around
existing courses and thereby sampled *built* landscapes — a course's site has
already been chosen for golf, and measuring it teaches the generator what golf
architects picked rather than what the landscape is.

Selection runs the existing three-filter cascade in `tools/tile_scout/`:
F0 coarse regional screen at 60 m, F1 OSM development screen, F2 per-candidate
terrain check at 10 m. Target **≥ 30 tiles per biome**, and more where the
biome's identity rests on a directional statistic.

### 3. Conditioning

In order:

1. **Void fill** — 3DEP has holes. Fill by inverse-distance over the void
   boundary and **record the filled fraction**; a tile above a threshold is
   excluded rather than trusted.
2. **Water flattening** — NHD polygons define water bodies; set them to a level
   surface and mask them. Unflattened water carries lidar return noise that
   measures as extreme roughness.
3. **Road and building removal** — OSM masks, buffered. This is where the v1
   campaign had known residuals (±10 m road masks, a building quilt in urban
   tiles). Under-masking leaves anthropogenic geometry that measures as terrain
   structure.
4. **Despeckle** — see the caveat below. This is the one step with no obviously
   correct setting.
5. **Mask assembly** — a single validity mask per tile combining void, water,
   road, building, and boundary. **The battery takes the mask as an argument**
   and every metric respects it; this is why the battery's signature is
   `f(height, cell_size, mask)`.

### 4. QA cull

The exclusion loop from the v1 campaign, retained wholesale:
per-tile reasons, human review through `crates/tile-lab/`, and an
**exclusion digest** stamped into the fitted output so a prior can always be
traced to the exact tile set that produced it.

**Expect ~40% attrition.** v1 excluded 52 of 90 — 38 auto-excluded on the OSM
developed-fraction screen (threshold 2%) and 14 on character. To land ~30 clean
tiles per biome, fetch ~50.

### 5. Skeleton and residual extraction

New in v2, and the step that makes the corpus a *generation* input rather than
only a validation one. For each clean tile:

1. **Extract the drainage skeleton** so the tile's terrain can be conditioned
   the same way the generator conditions its own —
   `dtm_primitives/network.py` (`extract_network`, `coarse_flow`),
   `ridgepipe.py` (divides, via the same pipeline on the inverted surface),
   `geomorphons.py` (landform class). Per-tile vector skeletons **already exist
   on disk** at `out/extract/*.regions.json` (channels, ridges, valley
   centrelines, basins, scarps, transects, in world metres).
2. **Decompose into lowpass + residual**, `s2 = s1 − lowpass`. Implemented in
   `tools/dtm_metrics/dtm_metrics/surfaces.py`, whose docstring states the
   purpose outright: *"the residual is what the noise layer must reproduce"*.
   Its macro provider is documented as **pluggable** — swapping the Gaussian
   lowpass for the generator's own S2 catena is the correct seam, and makes the
   fitted residual exactly the thing S3 must synthesize.
3. **Compute conditioning fields** — `lp_slope`, `lp_aspect`, `tpi`,
   `relief_pos` from `surfaces.py`, plus normalized flow distance and hillslope
   position from the skeleton. These are the six axes of
   [S3](../stages/stage-03-amplification.md)'s conditioning vector.
4. **Sample patches** with their conditioning vectors, and record **which tile
   each came from** — per-bucket tile diversity is the dictionary's real
   coverage constraint, not patch count
   ([targets.md](targets.md)).

### 6. The course-grid corpus

64 real golf-course grids sit in `out/courses/` (1500² @ 2 m; piedmont 18,
sandhills 12, florida 12, glacial 12, mountain 10) and have only ever been used
to fit one knob. They are worth far more than that, in two distinct roles:

- **Ground truth for the siting scorer**
  ([S5](../stages/stage-05-siting-substrate.md)). Measuring what terrain real
  architects actually chose turns site selection from a hand-authored heuristic
  into a fitted model. This is their higher-value use.
- **A second exemplar source** for the dictionary — real played ground has real
  golf texture.

**Weight them separately and keep them identifiable.** Course grids are
*graded*, not natural: blending them into the dictionary unweighted would teach
the generator to reproduce earthmoving as terrain. Suspect they belong only in
fairway-scale buckets, at low weight, and the weighting is itself a calibration
question ([S3's open questions](../stages/stage-03-amplification.md)).

## Caveats

### The despeckle caveat — the one that matters most

Lidar noise lives at **exactly the wavelengths
[S3](../stages/stage-03-amplification.md) and
[S9](../stages/stage-09-micro-repass.md) operate at**, and it cuts both ways:

- **Under-despeckle** and the dictionary learns sensor noise as terrain
  texture. The generated ground acquires a fine random crackle that looks like
  detail and is not.
- **Over-despeckle** and the real micro-texture is removed along with the
  noise, and the dictionary learns that the ground is smooth. Terrain then reads
  as plastic at close range.

There is no setting that is right for both, because the noise and the signal
overlap in frequency. **Do not pick one despeckle setting and proceed.** Run a
sensitivity analysis: measure the texture targets across a range of settings,
and report each as a band whose width reflects the methodological uncertainty
rather than a point estimate that pretends to a precision the data does not
support.

**This is far more consequential under v2 than it was.** Despeckle no longer
just sets an erosion intensity dial — it determines **what the dictionary
contains**. Under-despeckle and [S3](../stages/stage-03-amplification.md)
reconstructs sensor noise as terrain, faithfully, on every course in the biome.
Over-despeckle and the dictionary has no fine structure to reconstruct at all,
and the whole exemplar approach degrades to a smooth base surface.

Applies to `spectral_slope_beta`, `variogram_range`/`sill`, and
`short_lag_roughness` — four of S3's discriminants.

### Canopy

3DEP DTM is bare-earth, derived by classifying and removing vegetation returns.
Under dense canopy the classifier has less ground to work with and the DTM is
correspondingly smoother and less certain — **exactly in the closed-canopy
biomes**, [Piedmont](../biomes/piedmont.md) above all. A piedmont roughness
target measured under forest is biased low by an unknown amount.

Mitigation: prefer tiles with mixed cover, record canopy fraction per tile from
the same OSM/landcover pass, and check whether the roughness targets correlate
with it. If they do, the target is measuring the classifier and not the ground.

### Anthropogenic residue

Agricultural terracing, field boundaries, drainage ditches, old quarries — none
of it is in OSM and all of it measures as terrain structure. It is most severe
in exactly the biomes that are most farmed
([Great Plains](../biomes/great-plains.md),
[River Valley](../biomes/river-valley.md)). The QA cull is the only real defence
and it is a human one.

### Coverage

3DEP is US-only. [Heathland](../biomes/heathland.md)'s best reference
landscapes are in the UK and the Netherlands, which needs a second source
(Environment Agency LIDAR, AHN) and therefore a second ingestion path with its
own conditioning quirks. Plan for it; the metrics must be source-independent or
the comparison is meaningless.

**The dictionary is more exposed to this than the metrics are.** A metric
averages a sensor difference away; a dictionary bucket mixing 3DEP and EA
patches would blend two sensors' noise characteristics into one synthesized
texture. Either normalize across sources before fitting, or keep buckets
single-source and accept thinner coverage.

## Corpus status

**Three of six biomes have no corpus.** Details in
[targets.md](targets.md) and [../biomes/README.md](../biomes/README.md).

Also note: `tools/metrics` and `tools/dtm_metrics` are **currently
non-runnable** — both read `tools/parkland_atlas/out/cache`, which is not in
this branch's working tree. Recover from `main` (`tools/parkland_atlas/` plus
`assets/atlas.bin`, 29.7 MB) or re-collect. See
[MIGRATION.md](../../MIGRATION.md).
