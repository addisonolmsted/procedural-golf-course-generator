# dtm_atlas — Stage 0: Data Foundation & Naturalization (terrain-v2)

Builds the lidar DTM atlas store for the terrain-v2 pipeline
([docs/terrain-v2-plan.md](../../docs/terrain-v2-plan.md), Stage 0): USGS 3DEP
1 m DTMs + OSM feature masks for the **US subset (135) of the 200 parkland-atlas
courses**, naturalized (man-made features masked + Laplace-inpainted), with a
QA gallery as the assessment gate.

## Run

```
pip install -r requirements.txt          # numpy scipy requests rasterio Pillow
python3 -m dtm_atlas courses             # check courses_us.txt vs derivation
python3 -m dtm_atlas all --course riviera,valhalla   # smoke (network: fetch)
python3 -m dtm_atlas all                 # full 135-course campaign (~2 h fetch)
python3 -m dtm_atlas verify --write      # store manifest (idempotency gate)
open out/qa/index.html
```

Subcommands: `courses | fetch | raster | masks | artifacts | naturalize | qa |
verify | all`, with `--course k1,k2`, `--limit N`, `--force`, `--offline`.
Only `fetch` touches the network (cache-first, resumable, failures.log +
failed.json negative cache — delete to retry).

## Sources

- **Elevation:** USGS 3DEP dynamic ImageServer `exportImage` (keyless), fetched
  at 1 m in the course's NAD83 UTM zone, stored at 2 m (2×2 block mean).
  Effective source tier recorded from the 3DEPElevationIndex (1 m / 1/9 as /
  1/3 as) + an empirical sub-2 m texture fraction; courses without a ≤3 m
  source get `usable_for_calibration: false`.
- **Boundary + masks:** Overpass (`leisure=golf_course` with full geometry;
  `golf=green|tee|bunker|fairway|water_hazard`, `building`, `highway`,
  `amenity=parking`, `railway`, `natural=water`, `waterway`,
  `landuse=reservoir`). Property boundary chosen by name-Jaccard + holes-bbox
  overlap; fallback = dilated hull of the parkland-atlas hole polylines
  (flagged `holes_hull_fallback`).

## Store layout (out/ is gitignored)

```
out/fetch_cache/{key}/   dem_1m.tif, boundary/features/availability.json,
                         picked_boundary.json, provenance.json  (timestamps HERE only)
out/store/{key}/         raw.tif  naturalized.tif  meta.json
                         masks/{green,tee,bunker,fairway,building,road,water,
                                boundary_interior,exterior,nodata,
                                artifact_{curvature,seam,bridge},
                                inpaint_classes,inpaint}.tif
out/qa/                  index.html + {key}.html + img/{key}_{raw,nat,mask,diff}.png
out/reports/             margin_coverage.csv
out/store/MANIFEST.sha256
```

Windows are the property boundary + 500 m margin, snapped to even meters
(the 1 m fetch grid nests 2:1 in the 2 m working grid). `exterior.tif` flags
the margin (least-disturbed calibration signal — never inpainted);
`margin.exterior_inpaint_frac` in meta tells how much of a course's margin is
man-made anyway (urban courses ≈ 1.0).

## Naturalization

Per-class metric dilations (greens/tees +25 m, water +30 m, buildings/roads
+10 m, bunkers +15 m; true-Euclidean EDT) union with the artifact detectors
(profile-curvature robust-z, axis-aligned tile-seam finder, bridge-deck ribbon
heuristic) into `inpaint.tif`, filled by component-wise Laplace membrane
(scipy sparse solve, Dirichlet ring, Neumann at nodata). Invariants asserted
per course: cells outside the mask bit-exact-unchanged; fill range within the
Dirichlet ring range (maximum principle = "invents nothing"); every water cell
inside the inpaint mask.

Known behavior: the curvature detector flags some *natural* sharp canyon rims
(thin bands; components > 2000 px are spared as cliffs). The membrane fill
spans thin bands without flattening the step, and Stage 1's macro metrics are
low-passed > 200 m, so the impact is bounded — quantified by Stage 1's
robustness harness.

## Determinism

Derive stages are pure functions of the fetch cache + config: frozen GeoTIFF
creation profile (no DATETIME/SOFTWARE tags, PAM disabled), sorted-key JSON,
single-threaded solves, no RNG. All timestamps live in provenance.json (written
once at fetch). Gate procedure: `all` → `verify --write` → re-run derive stages
with `--force` → `verify` must pass. Byte-identity is scoped to one
environment; requirements are version-pinned and recorded.
