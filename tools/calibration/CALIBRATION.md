# Generator parameter schema + continuum calibration

Closes the loop with the metrics package: define the generator's parameter
vector **θ**, run a forward campaign θ → generated terrain → the **same metrics**
we measured on the 200 real courses, and fit a **continuous** p(θ) whose
generated terrain reproduces the courses' metric distribution. Deliverable: a
`sample_courses(n) → [GeneratorConfig]` sampler. **This is a continuum, not a
taxonomy** — no terrain classes anywhere.

## Reconciliation (the prompt vs the real generator)

The task prompt was written without this project's context and assumes a ~40-
parameter generator (landform primitives, terracing, ridged fBm, iterative
warp, erosion-model blend, routing-exponent, m/n stream-power exponents,
planation, boundary inflow, thermal/talus). **The real `golf-terrain` generator
exposes 12 knobs**, and — importantly for fitting a *continuous* p(θ) — has **no
categorical switches to reparameterize**; it is already continuous. The real
config (bound in `param_schema.py`, not duplicated) is:

```
TerrainParams { amplitude, base_period, octaves, lacunarity, gain,
                warp_amp, warp_period, redistribution }
ErosionParams { intensity, deposition, diffusion, hardness_contrast }
```

`generate_course_with(spec, seed, &TerrainParams, &ErosionParams, &WaterParams)`
is the explicit-param entrypoint, invoked headlessly by the new
**`xtask forward-grid <manifest.json>`** (batch, rayon-parallel, dry water = pure
morphology; writes the same `HG01` grid the metrics package reads). Items with
no params fall back to the *current seed sampler* — that's the calibration
baseline.

Prompt → reality (full table in `param_schema.py`): `base_frequency`→`base_period`,
`relief_amplitude`→`amplitude` (the sampler solves it; here free),
`warp_strength/frequency`→`warp_amp/warp_period`, `K_strength`→`intensity`,
`deposition_rate`→`deposition`, `diffusion_D`→`diffusion`,
`erodibility_contrast`→`hardness_contrast` (a real capability the sampler
**disables at 0**; freed here). **Absent** (don't exist / fixed constants):
ridged_blend, warp_octaves/recursion, terrace_*/contrast_gain, all landform
primitives, erodibility_scale/mix, erosion_model_blend, routing_exponent, m/n
(fixed ½,1), planation_*, iterations (fixed 8 epochs), discharge_ref, thermal,
talus (fixed 35°). `boundary_inflow` is a water/scenario variable → out of scope
(morphology only). No categorical→continuous reparameterization was needed.

## Preprocessing / reuse

`forward.py` **reuses `metrics/features.compute` unchanged** under the metrics
package's own `config.yaml` — identical resample cell (6 m), detrend policy, and
shared axes. Clip = `course_extent` = whole generated tile (dry ⇒ no water to
mask). Objective = the 37 **morphology** scalars (canopy/water dropped —
land-cover is a separate pass). Transforms: log for scales/frequencies/rates,
linear for bounded exponents (see `param_schema.PARAMS`).

## Forward campaign

Sobol design of **512 θ** (transformed space) × **4 seeds** = 2048 generator
runs (~2 min batched). All produced valid terrain (0 flat/NaN). **Seed vs θ
variance split** (fraction of metric variance from the seed): mean ≈ 0.22 — θ
dominates for almost every metric (relief 0.12, slope_p95 0.02, β 0.11), so the
distribution match targets θ-level variation. `drainage_density` is the
exception (0.62, seed-dominated) and near-degenerate — flagged and dropped.

## Coverage gate (run BEFORE matching)

Can the generator reach where the real courses live? **Only 59 %** of real
courses fall inside the generator's metric envelope (PCA hull). The reach gaps
(`coverage/reachability.csv`) are the headline — real ranges the generator
**cannot produce**:

| gap | real | generator | reading |
|---|---|---|---|
| `spectral_slope_beta` / `hurst` | β down to 1.6 | β floors at 4.1 | **too smooth** — can't make rough terrain |
| `highfreq_energy_frac` | up to 0.09 | ~0 | no fine-scale content |
| curvature / `laplacian` / sharpness | — | systematically lower | surfaces too smooth |
| `anisotropy_ratio` | up to 4.0 | ≤ 1.7 | **too isotropic** — lacks directional ridge/valley grain |
| `elev_skew` / `elev_kurtosis` / `heterogeneity_index` | long tails | compressed | **too homogeneous** — no plateaus/benches/juxtaposition |
| `variogram_nugget` | up to 2.0 | 0 | no measurement-scale noise |

Matching proceeds only on the **32/37** metrics reachable for ≥ 50 % of real
courses; the rest are reported as **generator capability gaps**, not fitting
failures.

## Emulator + sensitivity (θ × metric)

Per-metric `HistGradientBoostingRegressor` on θ→E_seed[metric]; **34/37 pass CV
R² ≥ 0.5** (poorly-emulated metrics aren't trusted). SALib Sobol total-effect on
the emulator vs the hypothesized `drives` intent map
(`emulator/intent_comparison.txt`):

- **Confirmed (HIT):** relief/elev_std ← `amplitude`; hypsometric_integral,
  elev_skew ← `redistribution`; variogram_range ← `base_period`;
  highfreq_energy ← `diffusion`.
- **Surprises (the actionable cross-couplings):** slope stats and rms_roughness
  are dominated by **`amplitude`/`base_period`** (vertical/horizontal scale), NOT
  the spectral-shape knobs. **`spectral_slope_beta` (roughness) is driven by
  `diffusion` (0.67), not octaves/gain/lacunarity.** The fBm noise knobs have
  little leverage because the pipeline band-limits noise (~70 m min feature) and
  then erosion diffusion sets the final texture. → *You cannot re-roughen the
  generator by re-weighting existing noise params; it needs a mechanism change
  (finer noise, less diffusion, or the hardness field).*

## Continuum inverse: density-ratio weighting

Label-free. A calibrated (sigmoid) logistic classifier discriminates real vs
synthetic metric vectors; its odds give r(x) = p_real/p_synth (corrected for
class sizes). Each design θ is weighted by the mean r over its seeds. Because
coverage is partial, raw weights collapse (ESS 4/512); the fit **tempers**
(w = r^0.5), **clips** the top 2 %, and **matches only reachable metrics**, giving
**ESS 355/512** — a usable, diverse continuous p(θ). No class structure imposed;
p(θ) is the weighted design ensemble, resampled with small unit-space jitter by
`sample_courses()`.

The fitted p(θ) centre (`p_theta/summary.txt`) shifts toward rougher, more
differentiated terrain: `gain` 0.57 (> default 0.5) and — notably — the
currently-disabled **`hardness_contrast` reactivated to 0.41**: the calibration
*discovered* that differential erodibility helps match the real slope/curvature
tails, a concrete recommendation to re-enable it in the sampler.

## Validation (round-trip, held-out)

30 % of real courses held out of the fit. Sample θ ~ p(θ), generate, measure,
compare to the held-out real distribution (`validation/`):

- **Energy distance to held-out real: p(θ) 2.35 vs current sampler 2.83 → +17 %.**
- Worst per-metric KS are the curvature/sharpness metrics (resid ≈ −1.1 z,
  systematically too smooth) — i.e. the residual mismatch IS the coverage gap.
- PCA overlap (`pca_overlap.png`): matched p(θ) sits closer to real than the
  baseline, but real extends into a low-PC2 (rough/sharp) region neither reaches.

**Conclusion:** within the generator's reachable envelope the calibrated p(θ)
matches the atlas meaningfully better than the relief-fit sampler; the remaining
distance is the documented capability gaps (too smooth / too isotropic / too
homogeneous), whose mechanism-level cause the sensitivity analysis pins on
diffusion + band-limited noise. Fixing those is a generator change, tracked here
as the next step — not a fitting problem.

## Artifacts (`tools/calibration/out/`)

`param_schema.json`; `samples/forward.parquet` (θ, seed, metrics, config_hash,
status) + `design_U.npy`; `coverage/` (overlap.png, reachability.csv,
summary.txt); `emulator/` (cv_r2.csv, sobol_ST.csv, sensitivity_heatmap.png,
intent_comparison.txt); `p_theta/` (weights.parquet, weights.png, summary.txt);
`validation/` (report.txt, per_metric.csv, pca_overlap.png); `SUMMARY.txt`.

## Reproduce

```sh
cd tools/calibration
python3 -m pip install -r requirements.txt      # SALib + the metrics package deps
python3 -m calibration.run --campaign 512       # forward + coverage + emulator + inverse + validation
python3 -c "from calibration.inverse import sample_courses; print(sample_courses(5))"
```
