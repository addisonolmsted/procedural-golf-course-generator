# Terrain-characterization metrics

Measurement stage that turns each reference course into a **feature vector**
describing its terrain, so a later step can fit a distribution / clustering
model and steer the procedural generator. **Measurement only** — no joint model,
no EMD/KS matching, no generator tuning here.

The load-bearing constraint: the metric functions in `metrics/core.py` are pure
`f(height, cell_size, mask)` — no file I/O, no raster/geo type, no "course"
concept — so the **same code scores the generator's own output** (`synthetic.py`
via `xtask dump-heightgrid`). Real and synthetic terrain live in one feature
space; see `out/report/synthetic_vs_real.png`.

## Data & the one important adaptation

The atlas is **not GeoTIFFs**. Each of the 200 courses
(`tools/parkland_atlas/out/cache/<key>.json`) is a **256×256 elevation grid**
packed over its survey extent (`b64` = little-endian u16 decimetres above
`emin`, `tb64`/`wb64` = MSB-first tree/water bit masks, row 0 = north), plus the
OSM hole centrelines (lat/lon). `io.py` decodes that and resamples to the common
grid. There is no rasterio/GeoTIFF path; `shapely` isn't needed (clip masks are
built with `skimage.draw` + distance transforms).

**Resolution.** Native cell size is `wm/256 × hm/256` ≈ **5–8 m** and non-square.
The task suggested 3 m; on this data 3 m is pure upsampling, so the common grid
defaults to **6 m** (config `resample.common_cell_size_m`, set 3.0 to follow the
task). Every course records `native_cell_x_m/native_cell_y_m`. Spectral and
roughness metrics are guarded by `spectrum.reliable_wavelength_min_m = 14 m`
(~2× native): treat features finer than that as resolution-limited on real
courses. The generator can emit true detail at any cell size, so when comparing,
band-limit to wavelengths ≳ 14 m or low-pass synthetic to the real resolution.

## Preprocessing (identical for real and synthetic)

1. **Resample** to `common_cell_size_m` (bilinear elevation, nearest masks).
2. **Clip** — two variants, kept separate:
   - `course_extent`: convex hull of hole vertices, dilated by `extent_buffer_m`
     (60 m). Falls back to the full survey if a course has no holes (flag
     `no_holes_full_extent`).
   - `fairway_corridor`: within `corridor_buffer_m` (50 m) of the OSM hole
     centrelines. If corridor geometry is missing the course yields
     `course_extent` only (row absent for corridor).
3. **Mask** water (`wb64`) and out-of-clip cells → `valid` (True = land + in
   clip). `n_valid_cells`/`valid_area_m2` recorded. Metrics exclude masked cells
   from aggregates; where a filled array is needed (gradient/FFT) masked cells
   are nearest-filled (elevation) or set to 0 (detrended residual).
4. **Detrend** (`detrend.method`, default `quadratic`) is removed **before**
   spectrum, variogram, and roughness (local texture, not regional tilt).
   **Raw** surface is used for relief, hypsometry, slope, hydrology. Curvature
   uses raw 2nd derivatives (invariant to a planar trend).
5. **Standardization** — `features_scalar.parquet` is raw units;
   `features_scalar_z.parquet` is z-scored **per clip variant**, with mean/std
   saved to `standardization.json`. Synthetic terrain is z-scored with the
   `course_extent` params so it is comparable.

Units: angles °, curvature 1/m, wavenumber cycles/m, wavelength m, lengths m.
`scalar` → a column in the feature table; `dist` → a fixed-length vector saved
per course in `distributions/<id>_<clip>.npz` on the shared axes in
`distributions/axes.npz`.

---

## A. Elevation & relief — raw surface

| metric | type | definition | units |
|---|---|---|---|
| `relief_p95_p5` | scalar | P95 − P5 of valid elevations (robust relief) | m |
| `elev_std`, `elev_skew`, `elev_kurtosis` | scalar | std / skew / excess kurtosis of the elevation distribution | m, –, – |
| `hypsometric_integral` | scalar | (mean − min)/(max − min) over land cells | – (0–1) |
| `hypsometric_curve` | dist | fraction of land area above each normalized-elevation level, at 21 fixed fractions 0..1 (monotone 1→0; its integral is HI) | – |

Mountain terrain runs high-HI/convex, lowland low-HI/concave, rolling ~S-curve.

## B. Slope — raw surface

Gradient by central differences on the nearest-filled surface; slope in degrees.

| metric | type | definition |
|---|---|---|
| `slope_mean/median/p90/p95/skew` | scalar | moments/percentiles of slope (°) |
| `frac_flat` | scalar | area fraction with slope `< flat_deg` (3°) |
| `frac_steep` | scalar | area fraction with slope `> steep_deg` (15°) |
| `slope_hist` | dist | area-fraction histogram over fixed 1.5° bins on [0,45°] (30 bins). The object the modelling step matches distributionally; the scalars are conveniences. |

## C. Multiscale roughness & spectrum — detrended

| metric | type | definition | notes |
|---|---|---|---|
| `radial_psd` | dist | radially-averaged 2-D power spectral density, interpolated (log-log) onto a shared 48-pt wavelength axis 14–900 m | Hann window + quadratic detrend; masked→0 |
| `spectral_slope_beta` | scalar | slope of log PSD vs log k over the mid-band (wavelengths 30–400 m); PSD ∝ k^−β. **β is the primary roughness invariant.** | fBm convention H=(β−2)/2 also reported as `hurst_from_beta`, but β is the measured quantity |
| `highfreq_energy_frac` | scalar | fraction of (reliable-band) spectral power at wavelength `< highfreq_cutoff_m` (30 m), annulus-weighted | resolution-limited on real courses |
| `variogram` | dist | omnidirectional semivariance γ(lag) on a shared 0–600 m lag axis (40 pts); directional-averaged over axis+diagonal offsets | detrended |
| `variogram_sill/range/nugget` | scalar | exponential-model fit; **range ≈ terrain correlation length** | model in config |
| `rms_roughness_{small,mid,large}` | scalar | mean local std of the detrended surface in ~30/100/300 m windows | m |

## D. Curvature & sharpness — raw 2nd derivatives

Sharpness = high-frequency content / curvature, distinct from slope.

| metric | type | definition |
|---|---|---|
| `mean_abs_profile_curv`, `profile_curv_p90` | scalar | |profile curvature| (down-slope), 1/m |
| `mean_abs_plan_curv`, `plan_curv_p90` | scalar | |plan curvature| (across-slope), 1/m |
| `laplacian_abs_mean`, `laplacian_abs_p95` | scalar | |∇²h| = |fxx+fyy|, 1/m |
| `highfreq_energy_frac` | scalar | (also listed in C) spectral high-frequency share |

Near-flat cells (|∇h|²<1e-8) report 0 curvature to avoid 0/0.

## E. Spatial organization & heterogeneity — raw surface

The discriminator for "flat floors surrounded by hills" vs uniform rolling.

| metric | type | definition |
|---|---|---|
| `local_relief_mean_{100,200}` | scalar | mean of windowed (max−min) local relief, m, at 100/200 m windows |
| `heterogeneity_index_{100,200}` | scalar | CV (std/mean) of the local-relief field — **high CV = strongly juxtaposed regimes**. Empirically anti-correlates with relief (flat courses are the most heterogeneous). |
| `slope_autocorr_length` | scalar | lag (m) where the radial autocorrelation of the slope field decays to 1/e |

## F. Drainage & hydrology — raw surface (priority-flood + D8)

Depressions filled (priority-flood, Barnes et al.), D8 steepest-descent flow
accumulation, channels where upslope drainage area ≥ `accum_area_threshold_m2`
(60,000 m²). *(The repo's flow code is Rust in `golf-terrain/erosion.rs`; a
minimal pure-numpy re-implementation lives in `core.py` so the function stays
pure and runs identically on real and synthetic arrays — the architectural
requirement. Threshold- and resolution-sensitive; documented in config.)*

| metric | type | definition |
|---|---|---|
| `drainage_density` | scalar | total channel length / area, 1/m |
| `valley_spacing` | scalar | ≈ 1/drainage_density, m |
| `concavity_theta` | scalar | θ from slope–area S = k·A^−θ fit over the channel network |

## G. Anisotropy — directional variograms

| metric | type | definition |
|---|---|---|
| `anisotropy_ratio` | scalar | max/min directional variogram range (8 orientations); range = 0.63·sill crossing |
| `anisotropy_orientation_deg` | scalar | orientation of maximum continuity (° from +x/east) |

## H. Land cover — atlas masks (full survey, clip-invariant)

Measured over the whole survey (matches the atlas `treepct`/`waterpct` and the
reference "parkland canopy ~40–52%" / "water inversely correlates with relief").

| metric | type | definition |
|---|---|---|
| `canopy_fraction` | scalar | tree cells / all cells (ESA WorldCover class 10). NaN for synthetic (no tree layer yet). |
| `water_fraction` | scalar | water cells / all cells (WorldCover 80 ∪ OSM hydrography) |
| `canopy_fraction_clip` | scalar | secondary: canopy within the clip (playing ground) |

---

## Outputs (`tools/metrics/out/`)

- `features_scalar.parquet` — one row per (course_id, clip_variant); metadata +
  all scalar metrics in raw units. `features_scalar_z.parquet` — z-scored.
- `standardization.json` — `{clip: {feature: {mean, std}}}`.
- `distributions/<id>_<clip>.npz` — `slope_hist`, `radial_psd`, `variogram`,
  `hypsometric_curve`; shared axes in `distributions/axes.npz`.
- `report/` — `small_multiples.png` (per-course hillshade/slope/PSD/variogram/
  hypsometric), `dist_overlays.png`, `scatters.png`, `correlation_heatmap.png`,
  `pca_scree.png`, `pca_scatter.png`, `dendrogram.png`, `clustering_bic.png`,
  `synthetic_vs_real.png`.
- `features_synthetic[_z].parquet` — generator seeds scored + z-scored to the
  real atlas. `explore_summary.txt` — the diagnostic read.

## Flags / assumptions (stated, not silently defaulted)

- `no_holes_full_extent` — course had no OSM holes; `course_extent` = full
  survey, no `fairway_corridor` row.
- `few_valid_cells` — < 64 valid cells after masking (tiny/over-watered clip).
- Real courses' effective resolution is ~5–8 m; sub-14 m spectral/roughness/
  high-freq features are resolution-limited (recorded native cell per course).
- `canopy_fraction` is full-survey (not the tight clip) to match the atlas %.
- Hydrology thresholds/priority-flood are documented in config and re-run
  identically on synthetic terrain.

## Exploratory (diagnostic — NOT the model)

`explore.py`: PCA scree + PC1/PC2 scatter, Ward dendrogram, GMM BIC/silhouette
over k=1..6 in reduced PC space (k=1 allowed). **Finding on the 200 courses:**
PC1 ≈ 35 % (ruggedness: slope/roughness/local relief), PC2 ≈ 15 % (sharpness/
curvature vs spectral slope), **13 PCs for 90 % variance**; the three relief
bands form ONE overlapping gradient — a high-dimensional **continuum**, not
discrete terrain types. Downstream should model the joint distribution
continuously; any k the GMM picks is density variation, not hard classes.

**First calibration signal (bonus, for the downstream task):** generated seeds
score in-range on ruggedness but sit systematically **low on PC2 (sharpness)** —
their `spectral_slope_beta` (~3.9–5.4) exceeds real courses (~3.1–4.0), i.e. the
generator is spectrally too smooth / high-frequency-deficient.

## Reproduce

```sh
cd tools/metrics
python3 -m pip install -r requirements.txt
python3 -m pytest tests/ -q          # synthetic-surface validation
python3 -m metrics.run               # 200 courses x 2 clips -> out/
python3 -m metrics.synthetic 2024 7 42   # score generator seeds in the same space
```
