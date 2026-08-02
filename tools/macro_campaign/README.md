# macro_campaign — step-03 archetype landform data campaign

Measures real macro landforms per archetype from USGS 3DEP lidar and fits
the `landform.*` knob quantiles consumed by step 02
(`crates/course-spec/data/archetype_priors.json`). This is the campaign
described in `steps/03-macro-landform.md` / PLAN.md: **landscape tiles are
primary** (raw archetype-exemplar land — dune fields, kettle moraine,
piedmont creek country, the Florida lake district, plateau bench margins);
the golf-course atlas (`tools/dtm_atlas`, 135 courses) is the validation
cross-check that generated courses sit in real-course metric space.

## Method

1. **Regions** (`macro_campaign/regions.py`): hand-picked 3 km tile centers
   inside well-known landform provinces, one list per archetype, with the
   region's UTM zone. **Centers must sit on protected or near-unpopulated
   land** — national forests, refuges, wilderness, state-forest interiors.
   The first pilot picked for landform character alone and every center
   landed in farmed or suburbanized country (8-64% built by the OSM screen);
   re-picking into Uwharrie NF, Ocala NF, Big South Fork and the northern
   Chequamegon/Huron-Manistee outwash moved almost every tile under 2%.
   Scale to ~30 by adding centers inside those same protected units (verify
   new tiles cluster correctly with `tools/tile_scout`'s GMM before trusting
   them).
1b. **Develop screen** (`develop`): per tile, ask OSM for roads, buildings,
   parking, railways, quarries and landfills, rasterize them to the tile
   grid (roads dilated by their half-width, buildings +15 m, disturbed
   ground +25 m), and record `developed_frac`. Tiles over
   `MAX_DEVELOPED_FRAC` (2%) are auto-culled into `out/exclude.json`;
   below it the mask is handed to `extract` so graded ground is kept out of
   the detectors. This is not cosmetic — a road embankment is real
   topography, so without the mask ditches fit as channels, cut-and-fill
   edges fit as scarps, and borrow pits fit as basins. Overpass is slow and
   rate-limited, so results cache under `out/develop/`.
2. **Fetch** (`fetch`): one 3DEP `exportImage` request per tile — 3 km at
   the pipeline's canonical 2 m in the region's UTM zone (bilinear,
   server-side reprojection; single request stays under the service's
   pixel budget). Written as CGRID1 (`out/tiles/<archetype>/<id>.cgrid`) —
   the same sidecar format the Rust pipeline uses, so tile-lab loads
   real tiles directly. Data is local-only (`out/` is
   gitignored) and refetchable.
3. **Extract** (`extract`): per-tile knob measurements
   (`out/extract/<archetype>/<id>.json`) using the ported analysis stack —
   `tools/metrics/metrics/core.py` (depression fill, D8 accumulation,
   anisotropy, PSD) and `tools/dtm_primitives` geomorphons. Placement tier:
   relief, tilt grade, channel count/fall/cross-section, ridge count/length,
   basin count/spacing/radius/depth/eccentricity, bench scarp/tread, dune
   wavelength + grain anisotropy. Also writes the QA overlay artifacts
   `<id>.classes.cgrid` (u8 bit-flags per cell) and `<id>.regions.json`
   (vector features in world metres) that `tile-lab` draws.
3b. **Shape** (`shape`): the deep `tools/dtm_primitives` transect pipeline —
   flow network, smoothed centerlines, perpendicular transects, cross-section
   and meander fits. Minutes per tile, so it is separate and resumable
   (`shape_version`). Until a knob has ≥ MIN_N samples the fit keeps its
   provisional table.
4. **Fit** (`fit`): per archetype × knob, an 11-point quantile table over
   tile values, merged into `archetype_priors.json` (schema unchanged —
   values only), `prior_version` bumped. **Gating policy** (encoded in
   `fit_knobs.py`, pilot review 2026-07-27 — measurements do NOT overwrite
   design intent blindly):
   - *Design knobs* (`core_relief_cap_m`) are never fitted — real landscapes
     are not golf-routable; the cap is the routability gate's lever.
   - *Quarantined estimators* keep provisional tables, each with its reason
     recorded verbatim in `QUARANTINED` (see the outcome below — that dict
     is the source of truth, not this list).
   - *Identity zeros*: where the provisional count-knob median is 0
     (sandhills valleys, non-mountain benches, piedmont/mountain basins),
     the whole family keeps its provisional tables — the zero is an
     archetype-identity decision detectors can't see (D8 fill artifacts
     fake sandhills channels; scarp-band false positives fake piedmont
     benches).
   - *Small-n winsorize*: below `WINSOR_N=10` tiles the table spans the
     p10–p90 sample range instead of min–max (with n≈5 the raw ends equal
     the single most extreme tile — one 309 m escarpment tile was the
     mountain q10). Self-relaxes at the ~30-tile scale-up.
   - *Excluded tiles*: `out/exclude.json` (written by `tile-lab`, see QA
     below) is skipped, and its digest is stamped into the prior's
     `fit_provenance` so a campaign version is reproducible from the corpus
     plus that cull.
   Outcome (`campaign-pilot-2`, 2026-07-28): 50 tables fitted, 55 kept
   provisional. Repaired out of quarantine: `valley_fall_grad` (measure on
   the RAW surface, excluding fill-raised cells) and `ridge_count`/
   `ridge_len_m` (ridgepipe traced crests gated to major ridges). Still
   quarantined, now with evidence rather than a to-do: `dune_wavelength_m`
   (falsified — orientation coherence ranks the dune field BELOW the
   plateau), `meander_intensity` (pinned at 1.0 — it is defined against the
   generator's own curvature clamp), `meander_wavelength_mult` (inherits a
   top-width fit that swings two orders of magnitude).
   Any prior edit is a golden re-bless in course-spec:
   ```
   cargo test -p course-spec            # fingerprint golden fails, prints new value
   # update FINGERPRINT_GOLDEN in crates/course-spec/src/prior.rs
   cargo run -p course-spec --example bless_golden > crates/course-spec/tests/golden_spec_seed_1.json
   cargo test --workspace               # course-macro goldens re-bless too
   ```

## Run

```
cd tools/macro_campaign
python3 -m macro_campaign fetch            # network: ~30 tiles × ~10 MB
python3 -m macro_campaign develop          # network: OSM screen, auto-culls built-up tiles
python3 -m macro_campaign extract          # placement tier, seconds/tile (--force to re-run)
python3 -m macro_campaign shape            # shape tier, minutes/tile; fills meander fits
python3 -m macro_campaign courses          # network: real-course core relief -> the cap's corpus
python3 -m macro_campaign fit              # writes a MERGED prior to out/, use --apply to install
python3 -m macro_campaign fit --apply      # rewrites crates/course-spec/data/archetype_priors.json
python3 -m macro_campaign compare          # measures generated tiles vs real -> out/report/compare.md
python3 -m macro_campaign report           # draws the same numbers -> out/report/progress.html
```

Single-tile smoke: `python3 -m macro_campaign fetch --limit 1`.

## QA the corpus before trusting a fit

`cargo run -p tile-lab --release` opens the fetched tiles with the
extractor's own classifications drawn on top (channels, ridges, accepted vs
rejected basins, depression-fill flats, bench bands, transects, fitted
centerlines), each tile's knobs colored against its archetype's committed
quantiles. It reads `out/extract/**` and measures nothing itself, so what
you see is exactly what the fit consumed. Mark a bad tile (town, quarry,
reservoir, heavy development) with a reason and it lands in
`out/exclude.json`, which `fit` then skips. Batch PNGs without the GUI:
`cargo run -p tile-lab --example tile_snapshots --release -- <dir>`.

## Validation loop

```
cargo run -p course-macro --example export_tiles --release   # generated -> out/generated/
python3 -m macro_campaign compare
python3 -m macro_campaign report
```

## Two corpora, deliberately

Most knobs are fitted from **exemplar landscape tiles** (`fetch` → `extract`):
what the landform does, measured on protected land.

`landform.core_relief_cap_m` is fitted from a **second corpus of real golf
courses** (`courses`), because it is the one knob a landscape corpus cannot
answer — real landscapes are not golf-routable, so nothing about them says
what a routable core may contain. A built course is routable by definition.
`fit_knobs.COURSE_SOURCED` routes it. The search boxes must stay disjoint and
`courses.run` asserts it: they overlapped once, and because courses are ranked
by area, every "piedmont" course came back as a Pinehurst course sandhills had
already claimed.

`compare` runs the SAME `measure_tile` over generated `base_height` as over
real tiles and flags every knob whose generated median falls outside the
real interquartile range. A flag is a *planner* finding, not a prior one:
the prior states what the landscape does, so failing to reproduce it means
the placement or shaping code is not consuming the knob faithfully.

`report` writes `out/report/progress.html` — the same numbers, drawn, plus
side-by-side hillshades of a real exemplar and a generated course per
archetype. It measures nothing: every value is read from what `extract` and
`compare` already wrote, and the gate bands and the real-vs-real noise floor
come from `compare.GATES` / `compare.baseline` themselves, so the page and
`compare --gate` cannot disagree. Both columns of every image pair go
through one renderer (`dtm_atlas.qa.hillshade_rgb`) from the raw `.cgrid`
heightfields, so nothing in the comparison comes from the drawing; the
shared-elevation-scale toggle re-stretches both to the same span, which is
the only way the relief deficit reads as a picture instead of a number. The
representative tile per side is the one whose 200 m relief is nearest that
side's own median (ties to the lowest name), so it is reproducible rather
than chosen. The page is self-contained — images inlined as data URIs, no
external assets — so it opens from disk and survives being published.
`--force` re-renders the cached JPEGs in `out/report/img/`; `--seeds N` caps
how many generated seeds per archetype are considered.


