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
