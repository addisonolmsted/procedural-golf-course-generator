"""All Stage-0 constants: paths, resolutions, dilations, detector thresholds,
and the frozen GeoTIFF profile. Every threshold that shapes the store is here
(and hashed into meta.versions.params_hash) so a change is visible + auditable.
"""

from __future__ import annotations

import hashlib
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, ".."))          # tools/dtm_atlas
REPO = os.path.normpath(os.path.join(ROOT, "..", ".."))
PARKLAND = os.path.join(REPO, "tools", "parkland_atlas")    # course list + cache
OUT = os.path.join(ROOT, "out")
FETCH_CACHE = os.path.join(OUT, "fetch_cache")
STORE = os.path.join(OUT, "store")
QA = os.path.join(OUT, "qa")
REPORTS = os.path.join(OUT, "reports")
COURSES_US = os.path.join(ROOT, "courses_us.txt")
TILES_US = os.path.join(ROOT, "tiles_us.txt")
TILES_MANIFEST = os.path.join(ROOT, "tiles_us.json")

# --- datasets ----------------------------------------------------------------
# Two independent stores share the pipeline: "courses" (the frozen golf atlas,
# out/store + out/qa) and "tiles" (Stage-0T natural-tile analogs,
# out/store_tiles + out/qa_tiles, own MANIFEST). Tile keys come from
# tiles_us.txt + tiles_us.json (center + side); their boundary is a synthetic
# square (no OSM golf query). FETCH_CACHE is shared — keys are disjoint.
DATASET = os.environ.get("DTM_ATLAS_DATASET", "courses")


def select_dataset(name: str) -> None:
    """Point STORE/QA at the named dataset (runtime-mutable; all modules
    access config.STORE / config.QA at use time). Also exported via the
    environment so ProcessPool SPAWN workers — which re-import this module
    fresh — land on the same dataset as the parent."""
    global DATASET, STORE, QA
    if name not in ("courses", "tiles"):
        raise SystemExit(f"unknown dataset: {name}")
    DATASET = name
    os.environ["DTM_ATLAS_DATASET"] = name
    STORE = os.path.join(OUT, "store" if name == "courses" else "store_tiles")
    QA = os.path.join(OUT, "qa" if name == "courses" else "qa_tiles")


# --- grid ------------------------------------------------------------------
FETCH_RES_M = 1.0      # 3DEP native over most of CONUS; one export per course
WORK_RES_M = 2.0       # working resolution of the store (plan's recommendation)
MARGIN_M = 500.0       # beyond the property boundary (least-disturbed signal)
TILE_SIDE_M = 3000.0   # Stage-0T tile edge (the terrain-v2 working box)
TILE_MARGIN_M = 0.0    # tiles are fetched EXACTLY as screened (no margin ring)
NODATA = -9999.0
SNAP_M = 2.0           # window corners snap to even meters -> 1 m maps 2:1 onto 2 m

# --- 3DEP ------------------------------------------------------------------
DEP3_EXPORT = ("https://elevation.nationalmap.gov/arcgis/rest/services/"
               "3DEPElevation/ImageServer/exportImage")
DEP3_INDEX = ("https://index.nationalmap.gov/arcgis/rest/services/"
              "3DEPElevationIndex/MapServer")
# availability layers, best first: (layer id, label, approx res m)
DEP3_INDEX_LAYERS = [(1, "1m", 1.0), (2, "19as", 3.0), (4, "13as", 10.0)]
# The documented service cap is 8000 px per DIMENSION, but exports also 500
# ("Error exporting image") above ~7M TOTAL pixels (server render budget) —
# the 8 largest multi-course properties hit it at 1 m. 2500 px tiles keep the
# typical course single-request (median window ~2300 px) and split the big
# estates into a handful of safe requests.
DEP3_MAX_PX = 2500
USABLE_SOURCE_MAX_M = 3.0   # courses whose best source is coarser are flagged

# --- Overpass --------------------------------------------------------------
OVERPASS_ENDPOINTS = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.kumi.systems/api/interpreter",
    "https://overpass.osm.ch/api/interpreter",
]
COURTESY_SLEEP_S = 2.0
BOUNDARY_PAD_M = 800.0      # search pad around the cached course bbox
BOUNDARY_MIN_JACCARD = 0.34  # same name-guard threshold as parkland_atlas

# --- mask classes ----------------------------------------------------------
# class -> (dilation meters, is_line_class)
MASK_CLASSES = {
    "green": (25.0, False),
    "tee": (25.0, False),
    "bunker": (15.0, False),   # spec lists the class without a radius; 15 m
    "fairway": (0.0, False),   # kept for later stages, never inpainted
    "building": (10.0, False),
    "road": (10.0, True),      # parking/pitch polys +10 m; lines use per-class
                               # half-widths (ROAD_HALFWIDTH_M), not this radius
    "water": (30.0, False),    # polygons; waterway lines rasterized all_touched
    "earthwork": (15.0, True),  # embankment/dyke/levee/retaining_wall/dam lines
    "disturbed": (10.0, False),  # quarry/landfill/construction/pitch/track polys
}
# classes whose dilated union joins the inpaint mask (fairway + exterior never do)
INPAINT_CLASSES = ["green", "tee", "bunker", "building", "road", "water",
                   "earthwork", "disturbed"]
# non-golf inpaint classes: the consolidation-density driver (golf-class
# dilations would otherwise make every green complex read as "urban fabric")
NONGOLF_CLASSES = ["building", "road", "water", "earthwork", "disturbed"]

# road line half-widths (meters each side of the OSM centerline). A divided
# highway is 20-40 m of pavement plus graded cut/fill; the old flat 10 m left
# the outer carriageway and all embankment fill unmasked.
ROAD_HALFWIDTH_M = {
    "motorway": 20.0, "motorway_link": 20.0,
    "trunk": 12.5, "trunk_link": 12.5, "primary": 12.5, "primary_link": 12.5,
    "secondary": 9.0, "secondary_link": 9.0,
    "tertiary": 9.0, "tertiary_link": 9.0,
    "railway": 10.0,           # track(s) + ballast prism
    "path": 4.0,               # footway/cycleway/path/cartpath
    "default": 7.5,            # residential/service/unclassified
}
ROAD_WIDTH_TAG_PAD_M = 2.0     # explicit width tag: half = width/2 + pad
ROAD_LANE_WIDTH_M = 3.5        # lanes tag: half = lanes*3.5/2 + pad
ROAD_HALFWIDTH_MAX_M = 30.0    # sanity cap on tag-derived widths
ROAD_EARTHWORK_HALFWIDTH_M = 15.0  # floor when way has embankment/cutting=yes
PATH_HIGHWAYS = ("footway", "cycleway", "path", "bridleway", "steps", "track")

# --- flat-surface detector (F1: unmapped water + pavement) ------------------
# Hydro-flattened lidar water is planar to the centimeter; real ground
# micro-relief over a 14 m window exceeds 5-10 cm even on graded turf.
FLAT_WIN_PX = 7              # 14 m window at 2 m
FLAT_DETREND_STD_M = 0.04    # plane-detrended window-std threshold
FLAT_MIN_COMPONENT_HA = 0.10  # 250 cells at 2 m; 2.5x under the 0.25 ha gate
FLAT_DILATE_M = 30.0         # berm ring, same rationale as the water class

# --- consolidation + confidence (F3: urban membrane quilt) ------------------
# Where the NON-GOLF inpaint density is high, the kept cells between masked
# features are graded lots that would pin the fill into a building quilt —
# demote them so whole neighborhoods fill from the regional trend.
CONSOLIDATE_WIN_M = 128.0    # suburban-block scale (the observed quilt scale)
CONSOLIDATE_DENSITY = 0.50   # quilt zones measure 0.8-0.98; corridors < 0.3
CONFIDENCE_WIN_M = 128.0     # confidence = 1 - local inpaint fraction

# --- geometry ---------------------------------------------------------------
RING_SNAP_EPS_DEG = 1e-7     # ring-stitch endpoint tolerance (~1 cm)

# --- artifact detectors ----------------------------------------------------
CURV_Z_THRESH = 6.0          # robust |z| on Evans-Young profile curvature
CURV_DILATE_PX = 2
CURV_MAX_COMPONENT_PX = 2000  # larger components are real cliffs: log, don't mask
SEAM_MEDIAN_FACTOR = 4.0     # row/col median step vs global median step
SEAM_P90_FRAC = 0.60         # fraction of cells exceeding the global p90 step
SEAM_WIDTH_PX = 2
BRIDGE_CORRIDOR_M = 15.0     # water dilation searched for decks
BRIDGE_MIN_RISE_M = 1.0      # above component water median
BRIDGE_ASPECT = 2.5          # PCA aspect for ribbon-ness
BRIDGE_AREA_PX = (20, 5000)

# --- naturalization --------------------------------------------------------
PYRAMID_THRESHOLD_CELLS = 1_500_000   # spsolve below this, multigrid above
MG_COARSE_TARGET_CELLS = 200_000      # restrict /2 until the component fits this
MG_SMOOTH_SWEEPS = 20                 # fixed Jacobi sweeps per level (determinism)

# --- fetch cache ------------------------------------------------------------
# versioned features artifact: bumping forces a features-only re-fetch (DEM and
# boundary are reused byte-identical — 3DEP re-processing must not leak in)
FEATURES_FILE = "features_v2.json"

# --- QA --------------------------------------------------------------------
QA_MAX_PX = 1600
HILLSHADE_AZ_DEG = 315.0
HILLSHADE_ALT_DEG = 45.0

# --- frozen GeoTIFF creation profile (determinism) --------------------------
GTIFF_PROFILE = {
    "driver": "GTiff",
    "compress": "deflate",
    "zlevel": 9,
    "tiled": True,
    "blockxsize": 512,
    "blockysize": 512,
    "interleave": "band",
    "bigtiff": "NO",
}


def params_hash() -> str:
    """sha256 over every constant above that shapes store bytes — recorded in
    meta.versions so a threshold change is visible as a store change."""
    payload = {
        k: v for k, v in sorted(globals().items())
        if k.isupper() and isinstance(v, (int, float, str, list, dict, tuple))
        and k not in ("HERE", "ROOT", "REPO", "PARKLAND", "OUT", "FETCH_CACHE",
                      "STORE", "QA", "REPORTS", "COURSES_US", "TILES_US",
                      "TILES_MANIFEST", "DATASET")
    }
    return "sha256:" + hashlib.sha256(
        json.dumps(payload, sort_keys=True, default=list).encode()
    ).hexdigest()[:16]


select_dataset(DATASET)
