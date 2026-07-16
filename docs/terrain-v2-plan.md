# Terrain Generation Pipeline — Staged Implementation Plan

**Purpose:** Rebuild the macro terrain stage around landform primitives + authored hydrology, and calibrate it against ~200 lidar-derived DTMs. Each stage is independently assessable; do not begin stage N+1 until stage N passes its gate. Each stage should be run as its own Claude Code session (or PR) with the "Definition of done" as the acceptance checklist.

**Cross-cutting requirements (apply to every stage):**
- Deterministic: every stochastic step keyed off an explicit seed; same seed → bit-identical output.
- Every stage ships an interactive HTML visualizer (hillshade + parameter sliders where relevant) — this is the primary assessment tool.
- Golden regression tests: fixed-seed outputs hashed and checked in CI so later stages can't silently regress earlier ones.
- Reduced-resolution fast path (e.g., 4× downsampled grid) for every generator stage, needed later by the calibration optimizer.
- All parameter structs serialize to a single versioned config schema; the calibration stage fits over exactly this schema.

---

## Stage 0 — Data Foundation & Naturalization

**Goal:** Convert 200 raw DTMs into a clean, uniform "atlas store" with man-made features masked and inpainted.

**Tasks:**
1. Ingest all DTMs; reproject to a common CRS and resample to a common grid resolution (recommend 2 m if source supports it, else 5 m). Record per-course provenance and native resolution.
2. Crop with margin: extend each course's raster 500 m beyond the property boundary where source data exists. Flag the exterior region in a mask channel — it is the least-disturbed calibration signal.
3. Build the mask stack per course from OSM + hydrography data already in the atlas workflow:
   - Class masks: greens, tees, bunkers, buildings, roads/cart paths/parking, water polygons.
   - Per-class dilation buffers (greens/tees +25 m to catch pad earthwork; water +30 m to catch dam berms; buildings/roads +10 m).
4. Scan-artifact detection: curvature-outlier detector (flag cells where profile curvature exceeds a robust z-threshold), tile-seam detector (linear discontinuity finder), bridge-deck heuristics (elevated ribbons over water polygons). Flagged cells join the mask stack.
5. Naturalization: inpaint all masked regions with thin-plate spline or Laplace fill. Cache the result as `naturalized.tif` alongside `raw.tif`.
6. QA gallery generator: side-by-side hillshades (raw vs. naturalized) with mask overlay, one page per course.

**Deliverables:** Atlas store directory layout (`{course}/raw.tif, naturalized.tif, masks/*.tif, meta.json`), QA gallery, ingestion CLI.

**Definition of done / assessment gate:**
- [ ] Spot-check 20 random QA pages: masks align with imagery; inpainting is smooth and invents no new structure (no bumps where a clubhouse was).
- [ ] Water bodies and their berms fully excised in naturalized DTMs.
- [ ] Exterior-margin coverage report: what fraction of courses have usable 500 m margin.
- [ ] Re-running ingestion is idempotent and deterministic.

---

## Stage 1 — Metric Extraction Pipeline

**Goal:** A per-course metric vector that is provably insensitive to residual man-made/scan artifacts. This vector is the sole fitting target for calibration — the generator is never fitted to pixels.

**Tasks:**
1. Implement the metric vector (computed on both raw and naturalized DTMs, stored separately):
   - **Spectral:** radially averaged power spectral density, reported as band powers. Macro bands: >800 m, 400–800 m, 200–400 m. Mid bands (informational, low fitting weight): 50–200 m.
   - **Relief:** p95–p5 elevation, p99–p1 (robust range pair).
   - **Slope/curvature:** median, IQR, and p90 of slope; median profile/plan curvature — all computed on a low-pass (>200 m) filtered surface for macro fitting.
   - **Hypsometry:** hypsometric-curve quantiles (area fraction below 10/25/50/75/90% relief).
   - **Drainage texture:** drainage density above a flow-accumulation threshold, computed at coarse scale on naturalized DTM.
   - **Water/land (raw DTM + hydrography only, never a macro-fit target):** water area fraction, count and size distribution of water bodies.
   - **Canopy fraction** from ESA WorldCover (already in atlas) — palette/validation metric, not terrain-fit metric.
2. Mask-aware sampling: slope/curvature/hypsometry statistics exclude masked cells and weight exterior-margin cells 2× when present. Spectral metrics computed on the naturalized surface (inpainting removes the need for masking in the frequency domain).
3. **Robustness harness (the key deliverable):** take 10 clean naturalized DTMs, inject synthetic contamination — green pads (+1 m, 30 m plateaus), pond excavations (−2 m, 80 m), 3 px spikes, a fake tile seam — recompute metrics, and report per-metric sensitivity. Macro-band metrics must move < 2% under worst-case injection; any metric exceeding tolerance gets redesigned or down-weighted.
4. Output: `metrics.parquet` (one row per course per surface type), plus a distribution-explorer HTML page (histograms, pairwise scatter, correlation matrix).

**Definition of done / assessment gate:**
- [ ] Robustness report: all macro metrics within tolerance under injection tests.
- [ ] Known empirical relationships reproduce (water fraction inversely correlated with relief; fBm-band amplitude bracket ~30–60 m for parkland set).
- [ ] Metric distributions have no unexplained outliers (outliers traced to data problems get fixed in Stage 0, not tolerated).

---

## Stage 2 — Landform Primitive Library

**Goal:** A standalone Rust module implementing the swept-profile primitive family and composition rules. No noise, no erosion — assessed on its own output.

**Tasks:**
1. Generalized primitive core: control geometry (Catmull-Rom spline / closed spline / point) → signed distance field → 1D profile function evaluated at distance → blended into base field. Per-station parameter interpolation along arc length. All profiles C1 at shoulders.
2. **Valley/canyon** (and ridge via sign flip): incision depth, floor half-width, wall slope, wall asymmetry, shoulder falloff, longitudinal fall gradient (floor strictly monotone end to end).
3. **Meander generator:** trend line between boundary entry/exit points + lateral displacement (sine + noise) with amplitude A, wavelength λ, driven by channel width and a meander-intensity scalar in [0,1] (λ ≈ 10–14× width, A ≈ 2–3× λ at intensity 1). Curvature clamp: minimum radius ≥ 2.5× total valley width; reject/regenerate self-approaching splines.
4. **Bluff:** boundary spline + eased step profile (height, face slope, crest/toe rounding), height taper to zero over terminal 15–20% of arc length, optional binding of a river spline to the toe.
5. **Bowl:** closed-spline boundary (preferred over ellipse) + revolved profile (depth, rim sharpness), low-frequency angular radius noise, and a mandatory choice: `outlet: spillway_notch(target_drainage) | intentional_lake`.
6. **Tilt field:** planar/low-order regional gradient.
7. **Composition rules:** ordered application (tilt → ridges/bluffs/bowls → drainage carve last within its influence mask, via min-with-valley-surface); smooth-min/max blending at seams; tributary junction snapping with accordant floor elevations.
8. Visualizer: hillshade + contours, live sliders for every parameter, preset buttons for target configurations.

**Definition of done / assessment gate:**
- [ ] Presets visually reproduce, by eye: deep asymmetric barranca (Riviera-like), broad low floodplain (Valhalla-like), straight steep canyon, ridge spine, bluff-with-river-at-toe, dry bowl with spillway, bowl-lake.
- [ ] No SDF artifacts at maximum meander intensity; curvature clamp verified by fuzz test (10k random splines, zero self-intersections).
- [ ] Trunk floor monotonicity property test passes for all valley configurations.
- [ ] Deterministic; golden hashes checked in.

---

## Stage 3 — Primitive Parameter Extraction from Real Courses

**Goal:** For each of the 200 courses, detect which primitives are present and measure their parameters directly from the naturalized DTM. These become *measured* inputs to calibration (Stage 7), shrinking the optimization problem.

**Tasks:**
1. Trunk-line detection: coarse-scale flow accumulation on the naturalized DTM, seeded/validated against OSM waterway geometry where present; fit a smoothed centerline spline.
2. Transect sampler: perpendicular transects every ~25 m along the centerline, discarding any transect intersecting a mask. Robust median profile across transects → incision depth, floor width, wall slopes, asymmetry. Longitudinal floor profile → fall gradient.
3. Meander measurement: decompose fitted centerline into trend line + lateral displacement; report amplitude, wavelength, intensity.
4. Bluff detection: step-edge finder on the low-passed surface (persistent linear features with one-sided elevation offset > threshold); transect fitting as above for height/face slope/rounding.
5. Bowl detection: closed-depression analysis (on the surface *before* any depression-filling), fit boundary spline + radial profile.
6. Tilt: robust planar fit to the exterior margin + unmasked interior.
7. Presence classification with confidence scores; manual-override file for cases the detectors get wrong.
8. Output: `primitives.json` per course + overlay visualizer (fitted splines and transect fans drawn on hillshade).

**Definition of done / assessment gate:**
- [ ] Riviera and Valhalla records reviewed in detail: fitted valley parameters match hand-measured cross-sections within 15%.
- [ ] Spot-check 15 additional courses across the relief spectrum; presence classification correct (or overridden) for all.
- [ ] Round-trip test: feed a course's fitted primitive record into the Stage 2 library, compare synthetic transect profiles against real median profiles — RMS profile error within agreed tolerance for 80%+ of valley courses.

---

## Stage 4 — Noise Layer with Primitive Modulation

**Goal:** fBm + domain warp composited over the primitive skeleton without destroying its structure.

**Tasks:**
1. Port existing fBm + domain-warp implementation; parameterize per config schema (amplitude, lacunarity, octaves, warp strength).
2. Modulation masks derived from primitive fields: amplitude damped to ~10–20% on valley floors, tapered on walls, full on uplands; warp field optionally grain-aligned with valley direction.
3. Property tests: trunk floor monotonicity preserved post-noise; no noise-created dams across any drainage primitive.
4. Visualizer: primitive-only / noise-only / composite toggle.

**Definition of done / assessment gate:**
- [ ] Monotonicity and no-dam property tests pass across a 500-seed fuzz sweep of the full parameter box.
- [ ] Visual review: noise reads as landscape character on the skeleton, not as interference; floodplains stay visibly flat.

---

## Stage 5 — Erosion as Finisher

**Goal:** Existing hydraulic + thermal sim re-scoped to weathering, with the trunk channel as base level.

**Tasks:**
1. Port sim; add boundary conditions: trunk channel cells act as sediment/water sink; channel bed protected (or re-asserted to authored profile each N iterations).
2. Reduced-strength parameter regime (finisher, not author); expose strength/iterations in config schema.
3. Property tests: post-erosion trunk monotonicity; no wall-gouging regression (reuse the earlier vertical-channel artifact detector); tributary gullies on valley walls orient toward trunk (flow-direction histogram check).
4. Visualizer: before/after erosion slider.

**Definition of done / assessment gate:**
- [ ] 500-seed fuzz sweep: zero drainage-integrity failures.
- [ ] Visual review: primitive/noise seams weathered away; valley walls show gully texture draining to trunk; no re-emergence of the historical vertical-gouge artifact.

---

## Stage 6 — Hydrology Derivation & Hydrology Graph

**Goal:** Structured hydrology output — the contract every downstream stage consumes.

**Tasks:**
1. Trunk finalization from spline: width profile along length, bank cross-section, stepped water-surface elevations down the fall gradient.
2. Emergent tributaries: flow accumulation on post-erosion surface, threshold, vectorize, snap confluences onto trunk spline with accordant elevations.
3. Lakes: fill-to-spill on composed surface; dammed-pond variant adjacent to trunk (parameterized frequency); wetland/swale masks from low-slope high-accumulation cells.
4. `HydrologyGraph` type: trunk spline, tributary polylines, lake polygons, junctions, water elevations; serialized into the manifest format.
5. Graph-integrity validator: every tributary terminates on trunk or lake; no orphan channels; water surfaces consistent at junctions.

**Definition of done / assessment gate:**
- [ ] Integrity validator passes across 500-seed sweep.
- [ ] Visual review of 30 seeds: rivers read as organizing landscape features (the original complaint is resolved); tributary density and placement look natural.

---

## Stage 7 — Calibration & Fitting (the core methodology)

**Goal:** A fitted sampling distribution over the full config schema such that generated courses match the real population statistically.

**Method — three sub-phases:**

**7a. Measured parameters pass through directly.** Primitive configurations and parameters come from Stage 3 records — they are not optimized. This removes ~10+ dimensions from the search and anchors each course fit to its true landform skeleton.

**7b. Per-course inverse fit of noise + erosion parameters (~8–12 dims).**
- For each course: fix its measured primitive record; search noise/erosion parameters to minimize objective `J(θ) = Σ_k w_k · ((m_k(gen(θ, seeds)) − m_k(course)) / σ_k)²` where `m_k` are the **macro-band metrics from Stage 1 computed on the naturalized DTM**, `σ_k` is the population std of metric k (standardization), and weights emphasize spectral bands and slope statistics.
- Generation at reduced resolution (Stage cross-cutting fast path); metrics averaged over a fixed panel of 3 seeds per evaluation to suppress seed noise while keeping the objective deterministic.
- Optimizer: CMA-ES (derivative-free, tolerant of the mildly noisy objective), 3 restarts from dispersed initial points; budget ~300 evaluations per restart. Nelder-Mead acceptable as a fallback but expect worse basins.
- Log full evaluation traces; flag courses whose best `J` exceeds a threshold — these indicate grammar gaps (something the generator can't express), which is signal, not failure.

**7c. Population distribution fit.**
- Assemble ~200 fitted vectors: primitive presence + parameters + fitted noise/erosion parameters. Encode primitive presence as continuous strength values (absent = 0), consistent with the everything-is-a-blend philosophy.
- Fit a Gaussian mixture in the standardized space; select component count by BIC (expect roughly 3–8 components). Components are inspectable and nameable ("deep-valley," "gentle open," "bluff-top") but are density concentrations, not code branches.
- Sampler: draw from GMM → clamp to valid ranges → generate.

**Validation:**
1. **Hold-out:** fit 7b/7c on 180 courses; verify the 20 held-out courses' metric vectors have non-negligible likelihood under the fitted GMM and that per-course inverse fits on them converge to comparable `J`.
2. **Population match:** sample 200 seeds from the GMM, generate, extract metrics; compare generated vs. real metric distributions per metric (Q–Q plots) and jointly (energy distance / MMD). Report a dashboard.
3. **Two-level closure (partial now, completed post-integration):** macro output matches *naturalized* metrics (this stage); full-pipeline output vs. *raw* metrics (including water coverage) is deferred to Stage 9 integration testing — record it as an open checklist item there.
4. Side-by-side gallery: each real course's hillshade next to its best-fit synthetic.

**Definition of done / assessment gate:**
- [ ] ≥85% of courses achieve per-course fit below the `J` threshold; the remainder are documented as grammar gaps with hypotheses.
- [ ] Riviera and Valhalla best-fit synthetics judged visually credible (the founding motivation — treat as a hard gate).
- [ ] Population Q–Q plots acceptable on all macro metrics; no metric where generated distribution is grossly narrower than real (variety collapse check).
- [ ] Hold-out validation passes.

---

## Stage 8 — Feasibility Gate & Production Sampler

**Goal:** Reject-and-resample loop guaranteeing every shipped seed is routable, without truncating the continuum more than necessary.

**Tasks:**
1. Gate metrics on the macro surface: routable-corridor area fraction by slope class, connectivity of buildable land on each side of the trunk (crossing-count-aware), water fragmentation, max sustained cross-slope along candidate loop bands.
2. Threshold calibration: run the gate on all 200 *real* course surfaces — thresholds must pass ~100% of reality (real courses are feasible by construction). Tighten only with evidence.
3. Rejection loop with structured logging (which metric failed, at what parameter vector) feeding a periodic re-tightening of the GMM.
4. Report: acceptance rate from the fitted sampler; gallery of accepted and rejected seeds.

**Definition of done / assessment gate:**
- [ ] All (or a documented near-all) real course surfaces pass the gate.
- [ ] Sampler acceptance rate ≥ ~70% (if lower, the mixture or gate needs work before proceeding — do not brute-force with rejection).
- [ ] Manual review of 20 rejected seeds confirms rejections are justified.

---

## Stage 9 — Integration Handoff (existing systems)

**Goal:** Wire the new macro stage into the established downstream pipeline. Smaller stage; mostly contract plumbing.

**Tasks:**
1. Routing: expose `HydrologyGraph` to the beam-search cost function (crossing budget, along-water and diagonal-carry rewards, valley-floor corridor recognition); clubhouse candidate scoring gains primitive-aware terms (shoulder siting).
2. Grading: no-fill constraint masks from trunk/tributary beds; grading budget scaled by local terrain difficulty.
3. Manifest: hydrology graph + primitive splines serialized into seed blob → placement manifest; bridge placement becomes a lookup from routing's declared crossings.
4. Complete the deferred closure test: full-pipeline outputs (with pond placement, grading) vs. raw-DTM metric distributions, including water coverage.

**Definition of done / assessment gate:**
- [ ] End-to-end seeds route successfully at ≥ target rate through existing routing.
- [ ] Full-pipeline vs. raw-metric closure test passes (notably water coverage — if it undershoots, the pond budget, not terrain, is adjusted).
- [ ] Three showcase seeds reviewed end to end: a barranca course, a floodplain course, a no-primitive gentle course.

---

## Suggested session sequencing for Claude Code

Run stages as separate sessions in order 0 → 8, with Stage 9 coordinated against the existing codebase. Stages 2 and 3 can proceed in parallel after Stage 1 (the primitive library needs no atlas data; extraction needs no library until its round-trip test). Provide each session: this plan, the config schema crate, the relevant Stage N section as the task spec, and the previous stage's Definition-of-done evidence. Instruct the agent to stop at the gate and produce the assessment artifacts rather than continuing.
