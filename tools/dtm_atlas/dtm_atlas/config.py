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

# --- grid ------------------------------------------------------------------
FETCH_RES_M = 1.0      # 3DEP native over most of CONUS; one export per course
WORK_RES_M = 2.0       # working resolution of the store (plan's recommendation)
MARGIN_M = 500.0       # beyond the property boundary (least-disturbed signal)
NODATA = -9999.0
SNAP_M = 2.0           # window corners snap to even meters -> 1 m maps 2:1 onto 2 m

# --- 3DEP ------------------------------------------------------------------
DEP3_EXPORT = ("https://elevation.nationalmap.gov/arcgis/rest/services/"
               "3DEPElevation/ImageServer/exportImage")
DEP3_INDEX = ("https://index.nationalmap.gov/arcgis/rest/services/"
              "3DEPElevationIndex/MapServer")
# availability layers, best first: (layer id, label, approx res m)
DEP3_INDEX_LAYERS = [(1, "1m", 1.0), (2, "19as", 3.0), (4, "13as", 10.0)]
DEP3_MAX_PX = 7500     # service cap is 8000; tile below it defensively
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
    "road": (10.0, True),      # highway=* incl. cart paths + parking + railway
    "water": (30.0, False),    # polygons; waterway lines rasterized all_touched
}
# classes whose dilated union joins the inpaint mask (fairway + exterior never do)
INPAINT_CLASSES = ["green", "tee", "bunker", "building", "road", "water"]

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
PYRAMID_THRESHOLD_CELLS = 1_500_000   # spsolve below this, pyramid fill above
PYRAMID_SWEEPS = 60                   # fixed Jacobi sweeps per level (determinism)

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
                      "STORE", "QA", "REPORTS", "COURSES_US")
    }
    return "sha256:" + hashlib.sha256(
        json.dumps(payload, sort_keys=True, default=list).encode()
    ).hexdigest()[:16]
