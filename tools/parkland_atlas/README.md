# Parkland Atlas collector

Offline data-prep tool that surveys real **parkland** golf courses — terrain,
tree canopy, water, and hole routings — and emits both a self-contained HTML
viewer and a JSON in the `window.COURSES` schema the Rust `golf-atlas` crate
ingests (`xtask atlas-pack`). It reproduces the methodology of the original
reference `parkland_atlas.html` and grows the survey from 27 to ~200 courses.

This tool is **not** part of the Cargo workspace — it's Python, run by hand when
you want to refresh or extend the atlas. Its outputs (`out/parkland_atlas.html`
and the packed `assets/atlas.bin`) are what the game/generator side consumes.

## Data sources (same as the reference)

- **Hole routings** — OpenStreetMap `golf=hole` ways (Overpass API). Multi-course
  clubs are split to a single routing by tee-to-green continuity.
- **Elevation** — AWS/Mapzen terrarium DEM tiles, zoom 15.
- **Tree canopy + water** — ESA WorldCover 2021 v200 (class 10 = tree, 80 = water),
  water unioned with OSM hydrography polygons.

## Setup

```sh
python3 -m pip install -r requirements.txt   # numpy, Pillow, requests, rasterio
```

`rasterio` (bundled GDAL) is the only non-trivial dependency; it does the
ESA WorldCover COG windowed reads over `/vsicurl`.

## Usage

```sh
# 1. seed the cache with the reference 27 (already collected, byte-identical)
python3 seed_reference.py ~/Downloads/parkland_atlas.html

# 2a. validation batch — collect 15 new courses, emit HTML+JSON
python3 collect.py --limit 15

# 2b. full run — every candidate in courses.py (resumable; re-runs skip cache)
python3 collect.py

# rebuild the HTML/JSON from whatever is cached (no network)
python3 collect.py --emit-only

# render a course to PNG for a quick server-side look (no browser needed)
python3 preview.py oakmont muirfieldvillage belair
```

Everything is **resumable**: each accepted course is cached under
`out/cache/<key>.json` and never re-fetched. Failures are appended to
`out/failures.log` and never abort the batch. Be polite — the driver throttles
Overpass (~2 s) and Nominatim (~1 s) between calls.

## The parkland gate

`collect.py` keeps a candidate only if it resolves **≥ 12 mapped holes** and has
**≥ 12% tree canopy** over the survey extent — dropping links/desert/heathland
courses that slip into the curated list. `courses.py` is over-listed (~250+
names) so the ~200 target survives geocode/OSM misses.

## Outputs

- `out/parkland_atlas.html` — self-contained viewer. Layer toggles:
  **terrain · tree canopy · water · holes**; courses grouped lowland / rolling /
  mountain by relief; per-hole elevation profile with map-hover crosshair.
- `out/parkland_atlas.json` — the `window.COURSES` map (every course carries the
  `tb64` tree mask the old reference lacked).

## Feeding the Rust side

```sh
cargo run -p xtask --release -- atlas-pack tools/parkland_atlas/out/parkland_atlas.html
```

packs all courses (elevation + water + **trees** + holes) into `assets/atlas.bin`
(`GATLAS2`). The viewer's **Parkland Atlas** tab, the **Match** tab, and the
`atlas-stats` / `atlas-water` / `atlas-routing` instruments then run over the
full set.

## Files

| file | role |
|------|------|
| `courses.py` | curated candidate list (reference keys + ~250 parkland names) |
| `sources.py` | OSM holes, terrarium DEM, WorldCover + OSM masks, profiles |
| `common.py` | base64 pack/decode + geometry (schema-exact with the reference) |
| `collect.py` | driver: geocode → fetch → parkland gate → cache → emit |
| `viewer.py` | self-contained HTML emitter |
| `seed_reference.py` | import the reference 27 from the original HTML |
| `preview.py` | server-side PNG render of a cached course |
