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

## Stage 0 — Data Foundation & Naturalization — IN PROGRESS

Tool: `tools/dtm_atlas/` (`python3 -m dtm_atlas <cmd>`; see its README).

| Gate item | Evidence | Status |
|---|---|---|
| Spot-check 20 QA pages (masks align; inpainting invents nothing) | `out/qa/{key}.html` + `{key}_diff.png`; reviewed keys + verdicts recorded below | ☐ |
| Water bodies + berms fully excised in naturalized DTMs | automated per-course check `water_mask ⊆ inpaint` + bridge-component counts in meta | ☐ |
| Exterior-margin coverage report | `out/reports/margin_coverage.csv` + index column | ☐ |
| Idempotent + deterministic re-runs | `verify --write` → re-run derive stages `--force` → `verify` clean; MANIFEST hash pinned below | ☐ |
| Store layout + ingestion CLI per spec | `out/store/{key}/{raw.tif,naturalized.tif,masks/*.tif,meta.json}` | ☐ |

### Gate evidence (filled at gate time)

- Spot-checked keys + verdicts: _pending_
- MANIFEST.sha256 (double-run): _pending_
- Margin coverage summary: _pending_
- Courses flagged `usable_for_calibration:false`: _pending_
- Boundary fallbacks (`holes_hull_fallback`): _pending_

## Stage 1 — Metric Extraction — NOT STARTED

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
