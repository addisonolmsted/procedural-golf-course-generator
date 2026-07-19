# Terrain v2 — Stage Status Tracker

Working tracker for the staged rework in [terrain-v2-plan.md](terrain-v2-plan.md).
One stage per session; a stage is DONE only when every gate item has recorded
evidence here. Do not start stage N+1 before stage N is signed off.

## Session decisions of record

- **Data sourcing (Stage 0):** US-3DEP-first — the US subset of the existing
  200-course parkland atlas (135 courses: 117 candidates with "USA" geocode +
  18 curated US reference keys). Store schema is multi-source-ready; England
  (DEFRA National LIDAR) is the planned second tier.
- **Working resolution:** fetch 1 m (3DEP native), store working rasters at 2 m
  (plan's recommendation), per-course NAD83 UTM (EPSG:269xx).
- **Visualizer:** generator stages (2+) get a native egui `terrain-lab` app;
  data stages ship static HTML QA galleries (server-rendered PNGs + JS toggles).
- **Branch:** `terrain-v2`; the legacy pipeline stays untouched until Stage 9.

## Stage 0 — Data Foundation & Naturalization — AT GATE (user review pending)

Tool: `tools/dtm_atlas/` (`python3 -m dtm_atlas <cmd>`; see its README).
**135/135 US courses stored** (2 m working grids from 1 m 3DEP fetches; the 8
largest multi-course properties needed tiled exports — the service 500s above
~7 M px/request). Review entry point: `tools/dtm_atlas/out/qa/index.html`.

| Gate item | Evidence | Status |
|---|---|---|
| Spot-check 20 QA pages (masks align; inpainting invents nothing) | seeded list (seed 2026) below; in-session deep review: riviera (barranca preserved, suburbs excised, bridge over barranca detected), wadehampton (mountain signal, 35% inpaint), wingedfoot (63% inpaint, diff confined to building/road/pond fabric), augusta/valhalla/skokie/castlepines metas; **user eyeball of the list pending** | ◐ |
| Water bodies + berms fully excised | hard per-course assertion `water & valid ⊆ inpaint` (naturalize fails otherwise) — 135/135 pass; 2333 bridge-deck components additionally masked | ☑ |
| Exterior-margin coverage report | `out/reports/margin_coverage.csv`: **135/135 have full 500 m margin DATA**; margin mostly-natural (≤30 % inpainted) on 24 (suburban US reality — `exterior_inpaint_frac` in meta quantifies per course); inpaint p50 52 % | ☑ |
| Idempotent + deterministic re-runs | no-force re-run skips (cache-first); forced double-run on 12 seeded courses (seed 7): **216/216 files byte-identical** (+ earlier 54/54 on riviera/augusta/valhalla); store frozen in `MANIFEST.sha256`: **2430 files, store hash 4d8a76135fa87005** | ☑ |
| Store layout + ingestion CLI per spec | `out/store/{key}/{raw.tif,naturalized.tif,masks/*.tif,meta.json}`; `python3 -m dtm_atlas {courses,fetch,raster,masks,artifacts,naturalize,qa,verify,all}` | ☑ |

### Gate evidence of record

- Spot-check list (seed 2026): aronimink, augusta, canterbury, cherryhills,
  chicagoac, colonialct, creekclub, hershey, houstonoaks, inverness, minikahda,
  newport, northberwick, roberttrent, sanfrancisco, skokie, stanford,
  wadehampton, wilmington, wingedfoot.
- Source tiers: 107 index-confirmed 1 m, 1 at 1/9 arc-sec, 20 "none" from a
  3DEPElevationIndex outage mid-campaign — all 20 texture-verified as real 1 m
  (sub-2 m texture fraction 0.83–0.99; the `usable_for_calibration` rule now
  accepts index-tier OR texture ≥ 0.7). **135/135 usable.**
- Boundary fallbacks (`holes_hull_fallback`): 51/135 — OSM lacks an acceptable
  `leisure=golf_course` polygon there; flagged per course in QA. Consequence is
  a hull-shaped (rather than property-shaped) margin, not missing data.
- 8 courses carry seam-detector flags (rows/cols in meta); 2 m raw grids are
  otherwise clean.
- Known noted behaviors: curvature detector masks some natural sharp canyon
  rims (thin bands; >2000 px components spared as cliffs); meta `params_hash`
  values are mixed across courses fetched before/after the in-campaign fixes
  (honest provenance — derive outputs themselves double-run byte-identical).

## Stage 1 — Metric Extraction + Parameter Identifiability — AT GATE (user review pending)

Tool: `tools/dtm_metrics/` (`python3 -m dtm_metrics <cmd>`); Rust sandbox
generator `golf-landform/src/noiselab.rs` + `cargo xtask noise-grid` (golden
13743951878413972362 in `xtask golden`). Scope per the approved expanded plan:
the metric battery AND a 19-parameter noise-identifiability study deciding
which Stage-4 parameters are worth keeping.

**Design spine (primitive-awareness):** metrics split S1 macro (primitives'
domain — validation only, never noise-fit targets, else we double-count what
primitives explain), S2 character (what Stage-4 noise must reproduce; the
Stage-7b objective candidates), S3 modulation contract (texture conditioned on
landform position). Primitive proxy until Stage 3 = Gaussian low-pass L=200 m,
pluggable `macro_override`. Fine bands come from clean-window PSDs (median
course is ~51% Laplace-inpainted; fill has no fine energy → global FFTs are
biased there by construction).

| Gate item (doc DoD + extension) | Evidence | Status |
|---|---|---|
| Robustness: macro metrics ≤2% under worst-case injection (pads/ponds/spikes/seam) | `out/reports/robustness_report.md` — S1 macro all ≤2% IQR-rel except pond-fragile valley metrics (demoted diagnostic); S2/S3 ≤0.10 pop-std except by-design detectors (QA band, laplacian, res_aniso — diagnostic) | ☑ |
| Empirical relationships reproduce (G5) | stage1_metrics.md: water↔relief Spearman −0.25 (doc expects negative ✓); relief p25–p75 = 18–52 m vs doc's 30–60 m amplitude bracket (overlaps; parkland set skews flatter); beta_char p25–p75 = 3.80–4.67 — steeper than the doc's [3, 4.5] prior (2 m block-meaned lidar rolls off faster than the generic-terrain prior; recorded as a finding, β target ranges come from data not the prior) | ☑ |
| No unexplained metric outliers | `out/reports/outlier_triage.md`: 161 flags = mountain-tail relief (belair/greenbrier/wadehampton 147–224 m vs median 31 m), water-dominated victoria, flat borderline-clean cohort; QA-band flags checked for seams (scattered extremes, max 1–4% on any row/col — no tile seams) → **no Stage-0 escalations** | ☑ |
| metrics.parquet + distribution explorer | `out/metrics.parquet` (135 courses × 81 columns + `__clean` variants), `out/explorer/index.html` (hists, \|ρ\| matrix, outliers, availability) | ☑ |
| EXTENSION: parameter decision table + recommended Stage-4 set | `out/reports/parameter_assessment.md` + `out/sandbox/stage4_recommended_params.json` — see verdicts below; **user review pending** | ◐ |

### Metric classification (gates G1–G6 → `out/reports/stage1_metrics.md`)

**6 fit_targets** (each a distinct character axis, G4-representative of its
redundancy cluster): `s2_beta_char` (spectral shape), `s2_sf_rms_100`
(amplitude — carries the 8-member cluster sf_rms_10/25/50/100 ≡ res_rms_* ≡
res_slope_p90), `s2_res_extrema_ha` (landform granularity), `s2_res_slope_p50`
(texture steepness), `s2_prof_curv_p90` (crease sharpness),
`s3_floor_upland_log_rms_ratio` (modulation contract). **10 validation** = S1
macro anchors (relief, lp_slope family, macro bands, aniso, tilt). **34
diagnostic** with recorded reasons (QA-by-design, sparse, low-ICC
heterogeneity, redundant non-representatives).

Gate recalibrations of record (documented in config + report preamble):
- G2 gates the masks_off variant only (ρ≥0.85); interior_only is descriptive —
  removing the 500 m margin measures a designed regional difference, not
  instability. Clean-window estimators (cw bands, beta_char) are G2-exempt:
  masks are load-bearing for them; masks_off reproduces the inpaint bias the
  estimator exists to avoid. Their stability evidence = sandbox θ-share
  (0.996+) + ICC + G1.
- G3-real ICC bar 0.3 (split-half ICC conflates within-course heterogeneity
  with noise; the sandbox between-θ share ≥0.5 is the controlled-replication
  stability gate — 28/31 pass).
- v1's roughness-organization axes (rough_cv/top10/moran) are drivable and
  robust but low-ICC → per-course diagnostic; flagged as candidate
  DISTRIBUTION-level 7b objectives (population quantile matching).

### Parameter verdicts (three evidence lines: Sobol ST influence ≥ max(0.05,
2×dummy)=0.05; fit-recovery Spearman via emulator inversion of 40 known
configs; leave-param-fixed coverage ≥ 0.145 self-calibrated floor)

**KEEP 7** — base_amp (recovery ρ 0.945), base_wavelength (0.826),
warp_wavelength (0.653), ridged_mix (0.820, +S5-check), redistribution
(0.795, +S5-check), aniso_ratio (0.872), floor_damp (0.901 on
truth-conditioned s3t ratio, ST 0.977 — the modulation contract is provably
closed-loop). **MERGE-OR-CONSTRAIN** — gain (0.473; confounded with
lacunarity exactly as pre-registered). **METRIC-GAP** (influential,
unrecoverable — fine-band energy attribution is ambiguous across the texture
stack): octaves, warp2_amp, tex_amp, tex_wavelength, nugget_amp, slope_gain.
**DROP** — lacunarity, warp_amp, tex_gain, grain_align (undrivable, ST 0.004).
Bounds JSON: `out/sandbox/stage4_recommended_params.json`; erosion-sensitivity
addendum contract (rerun emulate/identify/coverage with Stage-5 knobs before
7b) recorded in parameter_assessment.md.

### Notes of record

- Campaigns (all 2 m): A flat-base Sobol 512×3 seeds, B skeleton-on 256×2×3
  Stage-2 presets, C fit-recovery 40×(3 flat + 2 skeleton) — 0 scoring errors;
  grids generated/scored/deleted in batches. Emulators: 41/46 metrics CV
  R²≥0.5 (median 0.72); dummy ST ceiling 0.0126.
- Proxy-validation finding (campaign B): on sandbox fields the lowpass-proxy
  S3 metrics decorrelate from truth-conditioned s3t twins (noise's own
  low-frequency content dominates proxy TPI classes) → **Stage-7b must fit
  modulation against truth-conditioned metrics** (skeleton known at fit time);
  real-course s3 values remain valid measurements. s3 metrics draw sandbox
  gate evidence from s3t twins accordingly.
- `vg_nugget` ≈ 0 everywhere is honest (1 m→2 m block-mean killed sensor
  noise); emulator R² −0.36 → dropped by G6 degeneracy as designed.
- Determinism: extraction re-run on 10 courses byte-identical; noiselab golden
  in `xtask golden`; campaigns keyed by (θ_id, seed) with fixed Sobol seeds.

## Stage 1 follow-ups carried forward

- Stage-3 replaces the L=200 m low-pass proxy via `macro_override` (surfaces
  API already takes it); L-sensitivity table in `out/stability/`.
- Stage-4 build order: the 7 KEEP params are the core; METRIC-GAP texture
  params enter as fixed/derived until a separating metric exists (candidate:
  per-band amplitude fitting once Stage-7b fits jointly with erosion).
- Pre-7b: erosion-sensitivity addendum (re-run emulate/identify/coverage with
  Stage-5 knobs; re-check ridged_mix/redistribution overlap).

## Stage 2 — Landform Primitive Library — IN PROGRESS (parallel with Stage 0 per plan's parallelism note)

Crates: `golf-landform` (primitives + versioned MacroConfig schema, SCHEMA_VERSION/
LANDFORM_VERSION 1) and `terrain-lab` (egui visualizer: hillshade + contours, live
sliders, preset buttons, 4x fast path — `cargo run -p terrain-lab --release`).
Headless twin: `cargo run -p golf-landform --example dump_presets --release`
→ `output/landform_presets/*.png`.

| Gate item | Evidence | Status |
|---|---|---|
| Presets visually reproduce (barranca, floodplain, straight canyon, ridge spine, bluff+river, bowl spillway, bowl lake) | output/landform_presets/*.png — reviewed in-session (asymmetric walls, accordant tributary, spillway trench, terrace bluff all read); **user eyeball pending** | ◐ |
| No SDF artifacts at max meander intensity; curvature clamp fuzz (10k, zero self-approach) | `meander_fuzz_10k` run in release: PASS (also 1k always-on) | ☑ |
| Trunk floor monotonicity for all valley configs | `valley_floors_monotone_in_all_presets` + `authored_floor_strictly_monotone` (composed tolerance = smin dip k/4 ≤ 1.5 m, documented) | ☑ |
| Deterministic; golden hashes checked in | in-crate golden + `xtask golden` landform arm (17039205586666270760), both match | ☑ |

Notes of record: meander amplitude authority = curvature clamp (min radius
≥ 2.5 widths); the plan doc's "A ≈ 2–3× wavelength at intensity 1" cannot satisfy
its own clamp — intensity scales toward the clamp-allowed maximum instead.
Bluffs take no bbox prefilter (a step's raised terrace is unbounded).
Composed-floor scour dips ≤ k/4 at confluences are accepted physics.
## Stage 3 — Primitive Parameter Extraction — NOT STARTED
## Stage 4 — Noise Layer — NOT STARTED
## Stage 5 — Erosion as Finisher — NOT STARTED
## Stage 6 — Hydrology Graph — NOT STARTED
## Stage 7 — Calibration & Fitting — NOT STARTED
## Stage 8 — Feasibility Gate & Sampler — NOT STARTED
## Stage 9 — Integration Handoff — NOT STARTED
