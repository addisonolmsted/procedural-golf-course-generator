# tools/calibration — continuum calibration of the generator to the atlas metrics

Closes the loop with `tools/metrics`: defines the generator's parameter vector
**θ** (bound to the real `TerrainParams`/`ErosionParams`), runs a forward
campaign θ → generated terrain → the **same metrics**, and fits a **continuous**
`p(θ)` whose output reproduces the 200 courses' metric distribution. Ends in a
`sample_courses(n) → [GeneratorConfig]` sampler. Continuum, not classes — see
**[CALIBRATION.md](CALIBRATION.md)** for the reconciliation, findings, and gaps.

```
calibration/
  param_schema.py  canonical θ (12 knobs) + transforms + drives + theta<->config
  sample.py        Sobol/LHS design in transformed space
  forward.py       θ -> xtask forward-grid -> metrics.features.compute -> x  (reuses metrics)
  coverage.py      real vs generator reachability gate (run before matching)
  emulator.py      GBT surrogate per metric + SALib Sobol sensitivity vs intent map
  inverse.py       density-ratio weighting -> p(θ) + sample_courses()
  validation.py    round-trip vs held-out real (energy dist / KS / PCA) + baseline
  run.py           orchestrate; config.yaml pins budget/transforms/matching
CALIBRATION.md
```

## Use
```sh
cd tools/calibration
python3 -m pip install -r requirements.txt
python3 -m calibration.run --campaign 512     # full loop -> out/
python3 -c "from calibration.inverse import sample_courses; print(sample_courses(5))"
```
Needs the xtask binary built (`cargo build -p xtask --release`) for `forward-grid`.
Headline: within reach, p(θ) beats the current sampler by ~17% (energy distance);
the generator is **too smooth/isotropic/homogeneous** to reach the roughest real
courses (β driven by `diffusion`, not the noise knobs) — a generator capability
gap, documented not forced.
