# Naturalization v2 — Stage-0 urban-residual overhaul (dtm_atlas + dtm_metrics)

## Context

The terrain-v2 assessment (2026-07-19) found three failure modes in the Stage-0 naturalized store that leak man-made structure into the calibration data:

- **F1 — Unmasked water on 66/135 courses.** Dead-flat components >1 ha survive in "trusted" cells with 0% water-mask overlap (kingsmill 138.8 ha James River, laketahoe 97.1 ha, minikahda 55.7 ha, oakmont 54.5 ha Allegheny, torreypines 33.2 ha Pacific). `osm.py` does fetch `natural=water` relations, so misses come from coastline (never queried) and relation fetch/assembly fragility (exact-float ring stitching).
- **F2 — Road/rail earthworks survive.** Roads mask as centerline ±10 m regardless of class; no embankment/cutting/retaining-wall/pitch/quarry tags fetched; bridge detector fires only within 15 m of *mapped* water; curvature detector spares components >2000 px.
- **F3 — Building quilt.** Urban courses keep only 14–17% of cells (graded suburban lots) which pin the Laplace membrane; masked buildings re-emerge as ~4 m RMS square pillows at <100 m scales.

User decisions (fixed): **(D1)** consolidate high-density inpaint zones + per-cell confidence channel; **(D2)** scope includes dtm_metrics consumption + Stage-1 re-extraction with a distribution-shift report; **(D3)** no course hard-dropped — continuous `calib_confidence` for Stage-7 weighting.

Key discoveries that shape the design:
- `Course.clean = valid & ~inpaint` (tools/dtm_metrics/dtm_metrics/store.py:62,98) — growing the inpaint mask via consolidation automatically flows into every clean-cell metric; the confidence raster is only needed for graded weighting + the per-course score.
- **The existing "pyramid" fill is not a pyramid** (naturalize.py:71-84): 31×31 uniform-filter seed + 60 full-res Jacobi sweeps (with `np.roll` edge wrap). Info propagates only ~120 m; a consolidated 1.5M+ cell urban component would come out median-flat with blend edges — worse than the quilt. Must be replaced with a true coarse-to-fine multigrid. ~41 courses have max components >750k cells; expect 15–25 on this path post-consolidation, so it becomes mainstream.
- **Consolidation density must be driven by *non-golf* inpaint only**: green/tee/bunker dilations (25/25/15 m) make normal course interiors locally >55% inpainted; a raw-density trigger would demote real fairway corridors (and OSM fairway polygons are too spotty to protect against that).
- **Fetch cache is keyed by course only, not query** (pipeline.py:93-102): adding tags requires a versioned features cache file (`features_v2.json`) + per-artifact-incremental fetch so DEMs and boundaries are never re-downloaded (3DEP re-processing drift would silently change `raw.tif`).

## A. dtm_atlas changes

### A1. `config.py` — new constants (all auto-enter `params_hash()`)

```python
# flat-surface detector (F1)
FLAT_WIN_PX = 7               # 14 m window at 2 m
FLAT_DETREND_STD_M = 0.04     # plane-detrended window std; hydro-flattened water ~0,
                              # real 14 m micro-relief > 5-10 cm even on graded turf
FLAT_MIN_COMPONENT_HA = 0.10  # 250 cells; 2.5x margin under the 0.25 ha acceptance bar
FLAT_DILATE_M = 30.0          # berm ring, same rationale as water class

# road class half-widths (F2), meters each side of centerline
ROAD_HALFWIDTH_M = {
    "motorway": 20.0, "motorway_link": 20.0,
    "trunk": 12.5, "trunk_link": 12.5, "primary": 12.5, "primary_link": 12.5,
    "secondary": 9.0, "secondary_link": 9.0, "tertiary": 9.0, "tertiary_link": 9.0,
    "railway": 10.0,
    "path": 4.0,              # footway/cycleway/track/golf cartpath
    "default": 7.5,           # residential/service/unclassified
}
ROAD_WIDTH_TAG_PAD_M = 2.0    # explicit width tag: half = width/2 + pad
ROAD_LANE_WIDTH_M = 3.5       # lanes tag: half = lanes*3.5/2 + pad
ROAD_HALFWIDTH_MAX_M = 30.0   # sanity cap on tag-derived widths
ROAD_EARTHWORK_HALFWIDTH_M = 15.0  # min half-width when way has embankment/cutting=yes

# new mask classes (F2) — added to MASK_CLASSES + INPAINT_CLASSES:
#   "earthwork": (15.0, True)    embankment/dyke/levee/retaining_wall/dam lines
#   "disturbed": (10.0, False)   quarry/landfill/construction/brownfield/pitch/track polys
# MASK_CLASSES["road"] dilation -> 0.0 (variable-width map carries the buffer)

# consolidation + confidence (F3/D1)
CONSOLIDATE_WIN_M = 128.0     # suburban-block scale, matches observed quilt scale
CONSOLIDATE_DENSITY = 0.50    # non-golf inpaint density trigger; quilt zones 0.8-0.98,
                              # course corridors < 0.3
CONFIDENCE_WIN_M = 128.0

# geometry
RING_SNAP_EPS_DEG = 1e-7      # ring-stitch endpoint tolerance (~1 cm) — F1 fragility fix

# naturalization
PYRAMID_THRESHOLD_CELLS = 1_500_000   # unchanged (spsolve memory ceiling)
MG_COARSE_TARGET_CELLS = 200_000      # multigrid restricts /2 until coarsest fits this
MG_SMOOTH_SWEEPS = 20                 # fixed Jacobi sweeps per level (determinism)
# PYRAMID_SWEEPS retired with _pyramid_fill
FEATURES_FILE = "features_v2.json"    # versioned fetch-cache artifact
```

### A2. `osm.py`

- `features_query`: add polygon selectors `landuse~"quarry|landfill|construction|brownfield"`, `leisure~"pitch|track"`, `landuse=parking`; line selectors (ways) `man_made~"embankment|dyke|levee"`, `barrier=retaining_wall`, `waterway=dam`, `natural=coastline`, `golf~"cartpath|path"`. **Skip coastline polygonization** (fragile ocean-side ring closing); coastline ways join `water` *lines* so the beach/seawall interface is excised, and the flat detector catches the ocean surface itself. `cutting=yes`/`embankment=yes` are attributes of already-fetched highway/railway ways — consumed in classification, no new selector.
- `classify_features`: road lines become `(coords, halfwidth_m)` tuples via new `road_halfwidth(tags) -> float` — explicit `width` → `min(width/2 + PAD, MAX)`; else `lanes` → `lanes*3.5/2 + PAD`; else class table (railway ways use `"railway"`); `embankment=yes|cutting=yes` bumps to `max(hw, ROAD_EARTHWORK_HALFWIDTH_M)`. Parse width/lanes defensively (strip units, try/except, deterministic fallback). New classes `earthwork` (lines) and `disturbed` (polys); `golf=cartpath|path` → road lines at `"path"` width. Insert new branches after the water branch so golf-tagged features keep golf precedence.
- `assemble_rings`: replace the four exact `==` endpoint comparisons with epsilon match (`RING_SNAP_EPS_DEG` per coordinate). Order-preserving → deterministic.

### A3. `pipeline.py` — per-artifact incremental fetch

`stage_fetch` computes `need_boundary / need_features / need_dem` from individual file existence; features cache file is `FEATURES_FILE` (`features_v2.json`). When only features are missing: rebuild the padded bbox from cached `picked_boundary.json` (deterministic), run the one Overpass request, append to `provenance.json`. **DEM and boundary are never re-fetched** unless individually missing or `--force`. The 135-course re-fetch = one Overpass request per course (~15 min with courtesy sleep). `artifacts` stage sentinel gains `confidence.tif` so v1 store dirs re-run.

### A4. `masks.py` — variable-width road rasterization

- `road.tif` (undilated truth) stays the union of raw rasterized geometry. New `masks/road_halfwidth.tif` (uint8 meters): rasterize each half-width group `all_touched`, per-cell max (sort groups ascending, later wins — deterministic).
- New `dilate_by_width(centers, halfwidth_m, res_m)`: one `distance_transform_edt(..., return_indices=True)`; cell masked when distance to nearest road cell ≤ that cell's half-width. Used for road's contribution to `inpaint_classes.tif`.
- Write `masks/inpaint_nongolf.tif` = dilated building ∪ width-dilated road ∪ dilated water ∪ dilated earthwork ∪ dilated disturbed — the consolidation driver, persisted here to avoid re-dilating in artifacts.
- New classes flow through the existing `MASK_CLASSES` loop; stats gain road `halfwidth_p50/max`.

### A5. `artifacts.py` — flat detector, de-tethered bridges, consolidation, confidence

New functions:
- `detrended_window_std(z, win)`: std of residual from best-fit plane per window, closed-form from `uniform_filter` moments (`var_resid = var(z) − cov(z,x)²/var(x) − cov(z,y)²/var(y)`; window-local x,y have `var=(win²−1)/12`). Six uniform filters — cheap. Detrending matters: hydro-flattened rivers are planar but sloped.
- `detect_flat(raw, valid)`: components of `detrended_window_std < FLAT_DETREND_STD_M` within **valid** (not eligible — half-mapped lakes must form one coherent component), area ≥ `FLAT_MIN_COMPONENT_HA` → `masks/artifact_flat.tif` (undilated) + stats (count, total ha, largest ha, water-mask overlap).
- `consolidate(inpaint_pre, nongolf_driver, golf_truth, valid)`: `density = uniform_filter(nongolf_driver, CONSOLIDATE_WIN_M)`; `demote = valid & ~inpaint_pre & ~golf_truth & (density > CONSOLIDATE_DENSITY)`; single pass, no fixpoint → bounded + deterministic. `golf_truth` = undilated fairway|green|tee|bunker.
- `confidence_raster(inpaint, valid)`: float32 `1 − uniform_filter(inpaint & valid, CONFIDENCE_WIN_M)` on kept cells, exactly 0 inside inpaint/nodata → `masks/confidence.tif`.

Stage order: curv/seam unchanged → `flat` + 30 m dilation → **bridges de-tethered**: `detect_bridges(raw, water | flat, valid)` so decks over unmapped water are found (F2) → `inpaint_pre = inp_classes | curv | seam | bridge | flat_d` → `demote` from `inpaint_nongolf.tif ∪ artifact masks` → `inpaint.tif = inpaint_pre | demote` (plus 3×3 binary closing to seal slivers; save `masks/consolidated.tif` = demote delta for QA) → confidence.

Meta additions: `artifacts.flat_*`, `artifacts.consolidated_*`, and
`confidence: {interior_kept_frac, exterior_kept_frac, residual_flat_frac, residual_flat_ha_max, calib_confidence}` where `calib_confidence` = mean confidence over interior valid cells (the D3 scalar).

### A6. `naturalize.py` — true multigrid for oversized components

Replace `_pyramid_fill` with `_multiscale_fill(z, comp, known)`: restrict by 2×2 block-mean (comp/known coarsened by any) until component ≤ `MG_COARSE_TARGET_CELLS`; **spsolve the coarsest level** with the existing `_solve_component`; prolong bilinear; `MG_SMOOTH_SWEEPS` masked Jacobi sweeps per level using padded-slice neighbors (**no `np.roll` wrap**). Deterministic: level count fixed by shape, fixed sweeps, no convergence branching. Max-principle: Jacobi + bilinear are convex combinations of the coarse solution, which spsolve pins to its Dirichlet ring — keep the assertion with small absolute slack (1e-4) on this path; `solver="laplace_spsolve+multigrid"`.

`stage_naturalize` excision gate extended: assert `flat & valid & ~golf_truth ⊆ inpaint` — the executable form of F1's acceptance bar, failing the run otherwise (mirrors the existing water assertion).

### A7. `qa.py` + new audit CLI

- `CLASS_COLORS` += earthwork (brown), disturbed (ochre), artifact_flat (cyan), consolidated (translucent white).
- Per-course PNGs += `{key}_conf.png` (confidence grayscale) and `{key}_flatres.png` (raw hillshade, **red = dead-flat kept cells** — the regression tripwire, should be ~empty post-fix).
- Index/CSV columns += `flat ha`, `flat kept ha`, `demoted %`, `kept %` (interior), `calib conf`.
- New `python3 -m dtm_atlas audit`: writes `out/reports/naturalize_audit.{csv,md}` — per course: kept dead-flat components >0.25 ha (F1), RMS of `naturalized − 50 m lowpass` over consolidated cells (F3), kept-cell count within class half-width of motorway/trunk/railway centerlines (F2). Read-only, deterministic.

### A8. Tests (`tools/dtm_atlas/tests/`)

- Flat detector: tilted-plane lake (0.2% slope) detected; 10 cm-noise field not detected; 0.1 ha floor respected.
- `road_halfwidth` tag parsing (width="12", "12 m", lanes=4, garbage → class default); `dilate_by_width` radii.
- Consolidation: demotes quilt fixture, never golf_truth; rural-density fixture demotes nothing.
- `_multiscale_fill` vs `_solve_component` on ~600² synthetic component: RMS agreement < 5 cm; double-run byte determinism; max principle holds.

## B. dtm_metrics changes

- `store.py`: `Course` gains `confidence` array + `calib_confidence` property (default 1.0 for schema tolerance); `load_course` reads `masks/confidence.tif`; `clean = valid & ~inpaint & (confidence >= clean_min)` with `clean_min: 0.35` from `config.yaml` (`confidence:` block), threaded through the `load_course` call sites (extract/robustness/stability). 0.35 is deliberately below `1 − CONSOLIDATE_DENSITY` so consolidation, not this threshold, does the heavy lifting.
- `surfaces.py weight_map`: multiply clean-cell weights by `clip(confidence, 0, 1)`; keep the existing margin-bonus logic (surfaces.py:74-79).
- `candidates.py compute_course`: pass confidence into `weight_map`; emit `ctx_calib_confidence`, `ctx_conf_mean_clean`, `ctx_demoted_frac`; `ctx_low_confidence` additionally ORs `calib_confidence < 0.25`.
- New `shiftreport.py` + CLI `python3 -m dtm_metrics shift --before out/metrics.parquet --after out/metrics_v2.parquet`: per-metric median/IQR before/after, paired per-course deltas, KS statistic, sorted by |shift|/IQR; dedicated table + top-10 movers for the 6 fit targets (s2_beta_char, s2_sf_rms_100, s2_res_extrema_ha, s2_res_slope_p50, s2_prof_curv_p90, s3_floor_upland_log_rms_ratio). Extract to `metrics_v2.parquet`; promote only after review (D2).

## C. Ordering of work

1. **Offline-testable dtm_atlas code first** (config, osm classify + ring epsilon, masks, artifacts, naturalize multigrid, tests) — everything except new tags validates against *existing* cached `features.json` (flat detector, consolidation, confidence, multigrid need no new OSM data).
2. **Pilot dry-run on old caches**: F1 five (kingsmill, laketahoe, minikahda, oakmont, torreypines) + F3 five (losangeles, riviera, prestonwood, colonial, charlottecc) + bethpage (multigrid) + rural controls (erin/wadehampton — expect demoted % ≈ 0). Tune `FLAT_DETREND_STD_M` / `CONSOLIDATE_DENSITY` **before** freezing constants.
3. Incremental fetch + `features_v2.json`; re-fetch 135 courses (~15 min, resumable).
4. Full re-derive `raster→masks→artifacts→naturalize→qa --force` (raster only refreshes params_hash; verify `raw.tif` bytes unchanged on a sample). Naturalize dominates runtime; batch overnight if needed.
5. `python3 -m dtm_atlas audit` — F1/F2/F3 acceptance; documented exceptions go in the audit md, not code.
6. Determinism: forced double-run on the seed-7 12-course subset (byte-identical), then `verify --write` to re-freeze `MANIFEST.sha256`.
7. dtm_metrics changes; extract `metrics_v2.parquet`; shift report; review fit-target movement; promote.
8. Docs: update `docs/terrain-v2-status.md` Stage-0 gate evidence (new store hash, flat⊆inpaint assertion 135/135, audit results, shift-report link, params_hash change note).

## D. Acceptance criteria

- **F1**: zero kept dead-flat components >0.25 ha across 135 courses (or per-course documented natural exceptions); runtime assertion `flat & ~golf_truth ⊆ inpaint` passes 135/135; the five headline water bodies visibly excised in `{key}_flatres.png`.
- **F2**: zero kept cells within class half-width of motorway/trunk/railway centerlines; bridge components found over previously unmapped water on the F1 five; earthwork/disturbed masks non-empty where tagged.
- **F3**: top-10 urban courses: RMS(`naturalized − 50 m lowpass`) over consolidated cells **< 0.5 m**; no square pillows in QA hillshades.
- **D2**: 135/135 extracted; shift report reviewed; fit-target deltas recorded.
- Determinism double-run byte-identical; manifest re-frozen.

## E. Risks

- **3DEP drift**: structurally avoided — DEMs never re-fetched (incremental fetch); spot-verify raw.tif hashes after step 4.
- **Overpass**: 135 single requests, 3-mirror rotation, resumable; epsilon ring-stitch reduces relation fragility; `artifact_flat` is the OSM-independent backstop.
- **Consolidation over-reach on rural courses**: non-golf driver + golf-truth protection; `demoted %` QA column; rural controls in the pilot.
- **Metric availability collapse on worst-urban courses**: clean fraction drops further (LA ~14%→~8%); NaN-availability logic already handles it; those courses get low `calib_confidence` instead of fake numbers (D3).
- **Multigrid quality**: validated on bethpage/losangeles vs spsolve on cropped sub-windows; unit test bounds disagreement.
- **False-positive flats** (playas, frozen lakes, big graded pads): legitimately non-calibratable → joining inpaint and lowering confidence is honest; audit md lists any >5 ha flat with 0% water overlap for human review.
