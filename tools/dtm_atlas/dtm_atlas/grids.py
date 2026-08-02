"""Grid/CRS plumbing: per-course NAD83 UTM zone, window snapping (1 m fetch grid
maps 2:1 exactly onto the 2 m working grid), lat/lon<->UTM transforms, the
2x2 block-mean downsample, and the ONE deterministic GeoTIFF writer every
raster goes through (frozen creation profile, no timestamp/software tags).
"""

from __future__ import annotations

import os

import numpy as np
import rasterio
from rasterio.crs import CRS
from rasterio.transform import Affine
from rasterio.warp import transform as warp_transform

from . import config

# In-process: never write .aux.xml sidecars (they'd carry stats + timestamps).
os.environ.setdefault("GDAL_PAM_ENABLED", "NO")


def utm_epsg(lon: float, lat: float) -> int:
    """NAD83 UTM zone EPSG (269xx) for a CONUS/AK/HI longitude."""
    zone = int((lon + 180.0) // 6.0) + 1
    zone = max(1, min(60, zone))
    return 26900 + zone


def ll_to_utm(lons, lats, epsg: int):
    xs, ys = warp_transform(CRS.from_epsg(4326), CRS.from_epsg(epsg), lons, lats)
    return xs, ys


def utm_to_ll(xs, ys, epsg: int):
    lons, lats = warp_transform(CRS.from_epsg(epsg), CRS.from_epsg(4326), xs, ys)
    return lons, lats


def snap_window(minx: float, miny: float, maxx: float, maxy: float,
                margin_m: float = config.MARGIN_M, snap: float = config.SNAP_M):
    """Boundary bounds + margin, snapped OUTWARD to even meters -> the 1 m fetch
    grid and the 2 m working grid share corners exactly (2:1 nesting)."""
    x0 = float(np.floor((minx - margin_m) / snap) * snap)
    y0 = float(np.floor((miny - margin_m) / snap) * snap)
    x1 = float(np.ceil((maxx + margin_m) / snap) * snap)
    y1 = float(np.ceil((maxy + margin_m) / snap) * snap)
    return x0, y0, x1, y1


def work_transform(window_utm, res_m: float = config.WORK_RES_M) -> Affine:
    """North-up affine for the working grid (row 0 = north)."""
    x0, _y0, _x1, y1 = window_utm
    return Affine(res_m, 0.0, x0, 0.0, -res_m, y1)


def work_shape(window_utm, res_m: float = config.WORK_RES_M) -> tuple[int, int]:
    x0, y0, x1, y1 = window_utm
    return (round((y1 - y0) / res_m), round((x1 - x0) / res_m))


def block_mean_2x2(dem_1m: np.ndarray, nodata: float = config.NODATA) -> np.ndarray:
    """1 m -> 2 m by exact 2x2 block mean; a block with ANY nodata cell becomes
    nodata (conservative edges). Deterministic (fixed reduction order)."""
    h2, w2 = dem_1m.shape[0] // 2, dem_1m.shape[1] // 2
    v = dem_1m[: h2 * 2, : w2 * 2].astype(np.float64)
    blocks = v.reshape(h2, 2, w2, 2)
    valid = (blocks != nodata).all(axis=(1, 3))
    mean = blocks.mean(axis=(1, 3))
    out = np.where(valid, mean, np.float64(nodata)).astype(np.float32)
    return out


def write_gtiff(path: str, arr: np.ndarray, transform: Affine, epsg: int,
                nodata: float | None = None) -> None:
    """The one writer: frozen profile, predictor by dtype, no PAM/DATETIME."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    prof = dict(config.GTIFF_PROFILE)
    prof.update(
        height=arr.shape[0], width=arr.shape[1], count=1,
        dtype=str(arr.dtype), crs=CRS.from_epsg(epsg), transform=transform,
        predictor=3 if arr.dtype.kind == "f" else 2,
    )
    if nodata is not None:
        prof["nodata"] = nodata
    tmp = path + ".tmp"
    with rasterio.open(tmp, "w", **prof) as ds:
        ds.write(arr, 1)
    os.replace(tmp, path)


def read_gtiff(path: str):
    """-> (array, transform, epsg)."""
    with rasterio.open(path) as ds:
        return ds.read(1), ds.transform, ds.crs.to_epsg()
