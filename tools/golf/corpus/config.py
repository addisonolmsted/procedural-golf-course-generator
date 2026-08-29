"""Corpus-wide constants + provenance hash.

Etiquette and endpoint knowledge ported from tools/dtm_atlas/dtm_atlas/config.py
(endpoint rotation, courtesy sleep, the ~7M-total-pixel exportImage budget that
the documented 8000/dimension cap does not mention).
"""

from __future__ import annotations

import hashlib
import json
import pathlib

ROOT = pathlib.Path(__file__).resolve().parents[3]
PKG = pathlib.Path(__file__).resolve().parent
OUT = PKG / "out"
REGISTRY = OUT / "registry"
FETCH_CACHE = OUT / "fetch_cache"
TILES = OUT / "tiles"

# --- Overpass / Nominatim ---------------------------------------------------
OVERPASS_ENDPOINTS = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://overpass.osm.ch/api/interpreter",
]
NOMINATIM = "https://nominatim.openstreetmap.org/search"
COURTESY_SLEEP_S = 2.0     # between Overpass queries
NOMINATIM_SLEEP_S = 1.3    # max ~1 req/s per usage policy
UA = {"User-Agent": "GolfProceduralGenerator-corpus/0.1 "
                    "(golf terrain research; contact: repo owner)"}

# --- 3DEP -------------------------------------------------------------------
DEP3_EXPORT = ("https://elevation.nationalmap.gov/arcgis/rest/services/"
               "3DEPElevation/ImageServer/exportImage")
CELL_M = 2.0
NODATA = -9999.0
# Tile side is variable per course: at least 3 km, grown to hold the course
# polygon + margin, capped where 2 m stays under the ~7M-px exportImage budget
# (4500 m @ 2 m = 2250^2 = 5.06M px).
TILE_MIN_M = 3000.0
TILE_MAX_M = 4500.0
TILE_STEP_M = 500.0
TILE_MARGIN_M = 800.0
MAX_NODATA_FRAC = 0.02

# --- corpus gates -----------------------------------------------------------
MIN_COURSE_AREA_M2 = 2.0e5    # same floor as macro_campaign discovery
MIN_GREENS_KEEP = 7           # admits 9-holers with 2 unmapped greens
MULTI_COURSE_GREENS = 22      # >22 greens w/o sub-polygons -> unsplit property
GREEN_ASSIGN_NEAR_M = 150.0   # unassigned green adopted by nearest boundary

# --- fame -------------------------------------------------------------------
FAME_WEIGHTS = {3: 3.0, 2: 2.0, 1: 1.0}

# Discovery boxes: (region_tag, lat, lon, half_deg). MUST stay disjoint —
# macro_campaign/courses.py:50-57 records the Pinehurst double-count bug that
# overlapping boxes caused. Checked by discover.assert_disjoint().
SEARCH_BOXES = [
    ("sandhills_ne",  42.20, -100.90, 1.30),
    ("sandhills_nc",  35.20,  -79.40, 0.55),
    ("piedmont",      35.60,  -81.20, 0.90),
    ("piedmont_va",   37.60,  -78.60, 0.90),
    ("great_plains",  37.20,  -98.40, 1.20),
    ("river_valley",  32.70,  -91.60, 0.90),
    ("moraine_mi",    43.80,  -85.40, 1.00),
    ("heathland_nj",  40.00,  -74.50, 0.55),
    ("long_island",   40.85,  -72.85, 0.45),
    ("hill_country",  30.40,  -98.60, 0.90),
]


def params_hash() -> str:
    """sha256 over every store-shaping constant (dtm_atlas pattern) so a
    registry produced under different rules never silently mixes."""
    blob = json.dumps({
        "cell": CELL_M, "tmin": TILE_MIN_M, "tmax": TILE_MAX_M,
        "tstep": TILE_STEP_M, "margin": TILE_MARGIN_M,
        "min_area": MIN_COURSE_AREA_M2, "min_greens": MIN_GREENS_KEEP,
        "multi": MULTI_COURSE_GREENS, "near": GREEN_ASSIGN_NEAR_M,
        "boxes": SEARCH_BOXES,
    }, sort_keys=True)
    return hashlib.sha256(blob.encode()).hexdigest()[:16]
