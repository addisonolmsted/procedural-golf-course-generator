"""Fetch archetype exemplar tiles from USGS 3DEP as CGRID1.

One exportImage request per tile: 3 km at 2 m (1500 px) in the region's UTM
zone — under the service's pixel budget, no tiling/seams. Endpoint and
conventions follow tools/dtm_atlas/dtm_atlas/dep3.py.
"""

import io
import json
import pathlib
import time

import numpy as np
import requests

from . import cgrid
from .regions import CELL_M, REGIONS, TILE_M, tile_id
from .utm import to_utm

DEP3_EXPORT = (
    "https://elevation.nationalmap.gov/arcgis/rest/services/"
    "3DEPElevation/ImageServer/exportImage"
)
UA = "GolfProceduralGenerator-macro-campaign/1.0"
NODATA = -9999.0
OUT = pathlib.Path(__file__).resolve().parent.parent / "out"


def _read_tiff_f32(buf: bytes) -> np.ndarray:
    try:
        import tifffile

        return tifffile.imread(io.BytesIO(buf)).astype(np.float32)
    except ImportError:
        from PIL import Image

        return np.array(Image.open(io.BytesIO(buf)), dtype=np.float32)


def fetch_tile(lat: float, lon: float, epsg: int, zone: int, dest: pathlib.Path) -> dict:
    e, n = to_utm(lat, lon, zone)
    # Snap the window to even metres so grids nest across refetches.
    e0 = round((e - TILE_M / 2) / CELL_M) * CELL_M
    n0 = round((n - TILE_M / 2) / CELL_M) * CELL_M
    npx = int(TILE_M / CELL_M)
    params = {
        "f": "json",
        "bbox": f"{e0},{n0},{e0 + TILE_M},{n0 + TILE_M}",
        "bboxSR": epsg,
        "imageSR": epsg,
        "size": f"{npx},{npx}",
        "format": "tiff",
        "pixelType": "F32",
        "noData": NODATA,
        "interpolation": "RSP_BilinearInterpolation",
    }
    r = requests.get(DEP3_EXPORT, params=params, headers={"User-Agent": UA}, timeout=180)
    r.raise_for_status()
    meta = r.json()
    href = meta.get("href")
    if not href:
        raise RuntimeError(f"exportImage gave no href: {meta}")
    img = requests.get(href, headers={"User-Agent": UA}, timeout=300)
    img.raise_for_status()
    z = _read_tiff_f32(img.content)
    if z.shape != (npx, npx):
        raise RuntimeError(f"unexpected shape {z.shape}")
    z = np.where(np.isfinite(z) & (z > -1000.0) & (z < 9000.0), z, np.nan)
    # TIFF row 0 is the NORTH edge; CGRID1 row 0 is the SOUTH edge.
    cgrid.write_f32(dest, np.flipud(z).astype(np.float32), 0.0, 0.0, CELL_M)
    valid = float(np.isfinite(z).mean())
    return {
        "lat": lat,
        "lon": lon,
        "epsg": epsg,
        "easting0": e0,
        "northing0": n0,
        "valid_frac": valid,
    }


def run(limit: int | None = None, archetype: str | None = None):
    n_done = 0
    for arch, reg in REGIONS.items():
        if archetype and arch != archetype:
            continue
        d = OUT / "tiles" / arch
        d.mkdir(parents=True, exist_ok=True)
        for lat, lon in reg["tiles"]:
            if limit is not None and n_done >= limit:
                return
            tid = tile_id(lat, lon)
            dest = d / f"{tid}.cgrid"
            meta_p = d / f"{tid}.json"
            if dest.exists() and meta_p.exists():
                print(f"  [skip] {arch}/{tid}")
                continue
            t0 = time.time()
            try:
                meta = fetch_tile(lat, lon, reg["epsg"], reg["utm_zone"], dest)
            except Exception as exc:  # noqa: BLE001 — record and continue
                print(f"  [FAIL] {arch}/{tid}: {exc}")
                continue
            meta_p.write_text(json.dumps(meta, indent=1, sort_keys=True))
            print(
                f"  [ok] {arch}/{tid}  valid {meta['valid_frac']:.2%}  "
                f"{time.time() - t0:.1f}s"
            )
            n_done += 1
            time.sleep(1.0)  # be polite to the public service
