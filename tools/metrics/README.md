# tools/metrics — terrain characterization over the reference atlas

Turns each of the 200 reference courses (`tools/parkland_atlas`) into a terrain
**feature vector** — relief, slope, spectrum, variogram, curvature, hydrology,
anisotropy, land cover — plus per-course distribution vectors, so a later step
can fit a distribution/clustering model and retune the procedural generator.

**Measurement only.** No joint model, no EMD/KS matching, no generator tuning
here — this builds the numbers those steps consume. See **[METRICS.md](METRICS.md)**
for every metric's definition, units, detrend policy, resolution, and flags.

## The architecture that matters

`metrics/core.py` holds **pure** functions `f(height, cell_size, mask) -> scalar
| vector` — no I/O, no geo types, no course concept — so the **identical code
scores the generator's own terrain**. Real courses (`io.py`) and synthetic
terrain (`synthetic.py`, fed by `xtask dump-heightgrid`) go through the same
`features.compute` on the same shared axes, landing in one feature space
(`out/report/synthetic_vs_real.png`).

```
metrics/
  core.py       pure metric functions (+ tiny priority-flood/D8 for hydrology)
  features.py   the one place that assembles a tile -> (scalars, distributions)
  io.py         decode a course JSON -> resample -> clip -> mask -> contract
  synthetic.py  score generated seeds (xtask dump-heightgrid) in the same space
  run.py        orchestrate 200 courses x 2 clips -> tables/distributions/report
  report.py     small multiples, scatters, distribution overlays, corr heatmap
  explore.py    PCA + clustering (diagnostic only — not the model)
  config.yaml   resolution, clip variants, thresholds, band cutoffs, detrend
tests/          synthetic-surface validation (plane/tilt/cone/sinusoid/fBm)
```

## Use

```sh
cd tools/metrics
python3 -m pip install -r requirements.txt
python3 -m pytest tests/ -q                 # validate metrics on known surfaces
python3 -m metrics.run                       # full extraction -> out/
python3 -m metrics.run --limit 12            # quick subset while iterating
python3 -m metrics.synthetic 2024 7 42       # score generator seeds (needs xtask built)
```

Outputs land in `out/` (git-ignored; reproduce anytime): `features_scalar[_z].parquet`,
`standardization.json`, `distributions/*.npz`, `report/*.png`, `explore_summary.txt`.

## Key facts / adaptations

- The atlas is packed **256² grids** (native ~5–8 m), not GeoTIFFs. Common grid
  defaults to **6 m** (config; 3 m would be pure upsampling). `reliable_wavelength_min_m`
  guards the spectral fits. Native resolution is recorded per course.
- Two clip variants: `course_extent` (hull + 60 m) and `fairway_corridor`
  (50 m around OSM centrelines). Water is masked.
- Hydrology uses a pure-numpy priority-flood + D8 (the repo's flow code is Rust
  and can't be a pure Python `core` function).
- Exploratory result: the courses are a **high-dimensional continuum**
  (lowland→rolling→mountain gradient, 13 PCs for 90 %), so model the
  distribution continuously. First calibration signal: the generator is
  spectrally **too smooth** (higher β / low PC2 sharpness) vs real courses.
