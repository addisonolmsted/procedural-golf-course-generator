"""ESA WorldCover land cover for every tile we already hold elevation for.

  python3 -m corpus.fetch_canopy courses          # the 651 golf course tiles
  python3 -m corpus.fetch_canopy natural          # the 572 macro_campaign tiles
  python3 -m corpus.fetch_canopy courses --limit 5

Trees are two different objects. Where trees COULD grow is a property of the
land; where trees ARE on a course is a construction decision. The natural tiles
answer the first question because no golf has touched them, and the course
tiles answer the second because the clearing is visible against the woods
around it. Both need the same raster, so both go through here.

Source: ESA WorldCover 2021 v200, 10 m, the same COGs
`tools/parkland_atlas/sources.py` already reads. The whole CLASS raster is
kept, not a tree mask, so water, shrub and grassland stay available downstream;
class 10 is tree cover.

10 m divides every observed tile side (3000, 3500, 4000, 4500), which 30 m
does not, so the canopy grid nests inside the 2 m elevation grid exactly.

Conventions are `fetch_dem.py`'s, deliberately: resume by existence, the origin
written as (0, 0) with the real one living in the registry record, and row 0 =
SOUTH, which means a flip because the source is north-up. The sidecar sits
beside the elevation as `<tile>.canopy.cgrid`, the way `water.py` writes
`<tile>.water.npy`.
"""
from __future__ import annotations

import json
import os
import pathlib
import sys
import time

import numpy as np

_TOOLS = pathlib.Path(__file__).resolve().parents[2]
if str(_TOOLS / "macro_campaign") not in sys.path:
    sys.path.insert(0, str(_TOOLS / "macro_campaign"))
from macro_campaign import cgrid  # noqa: E402

from corpus import config, registry  # noqa: E402

CELL_M = 10.0
WORLDCOVER = ("/vsicurl/https://esa-worldcover.s3.eu-central-1.amazonaws.com"
              "/v200/2021/map/ESA_WorldCover_10m_2021_v200_{name}_Map.tif")
SOURCE = "ESA WorldCover 2021 v200 (10 m)"
CLASS_TREE = 10

# NLCD Tree Canopy Cover, 30 m, PERCENT canopy per cell. It measures a
# different thing from WorldCover and both are wanted: WorldCover says whether
# a 10 m cell is treed LAND, TCC says what fraction of the ground is under
# canopy. A pine savanna reads near 100 % treed and near 50 % canopy, which is
# precisely the open-savanna signature (measured 2026-09-16: Pinehurst No. 8
# 77 % treed against 45 % canopy, a Michigan moraine course 51 against 26, a
# Nebraska farm-country course 7 against 1).
TCC_WMS = "https://www.mrlc.gov/geoserver/mrlc_download/wms"
TCC_LAYER = "nlcd_tcc_conus_2021_v2021-4"
TCC_CELL_M = 30.0
TCC_SOURCE = "NLCD TCC CONUS 2021 v2021-4 (30 m, percent)"
NATURAL_TILES = _TOOLS / "macro_campaign" / "out" / "tiles"
COURTESY_S = 0.4


def _wc_name(lat: float, lon: float) -> str:
    """The 3-degree WorldCover tile holding this point."""
    la = int(np.floor(lat / 3.0) * 3)
    lo = int(np.floor(lon / 3.0) * 3)
    return (f"{'N' if la >= 0 else 'S'}{abs(la):02d}"
            f"{'E' if lo >= 0 else 'W'}{abs(lo):03d}")


def _ll_corners(epsg: int, e0: float, n0: float, side: float):
    """The tile's four corners as (lat, lon), via rasterio's transform."""
    from rasterio.warp import transform as warp_transform
    xs = [e0, e0 + side, e0, e0 + side]
    ys = [n0, n0, n0 + side, n0 + side]
    lon, lat = warp_transform(f"EPSG:{epsg}", "EPSG:4326", xs, ys)
    return list(zip(lat, lon))


def classes_for_tile(epsg: int, e0: float, n0: float, side: float,
                     cell: float = CELL_M) -> np.ndarray:
    """WorldCover classes on the tile's own projected grid, row 0 = SOUTH.

    Reprojected straight into the destination grid rather than read and
    resampled, so the canopy cells line up with the elevation cells instead of
    merely covering the same ground. A tile can straddle two 3-degree source
    tiles; each is reprojected in turn and merged into the unclassified gaps.
    """
    import rasterio
    from rasterio.transform import from_origin
    from rasterio.warp import Resampling, reproject
    os.environ.setdefault("GDAL_DISABLE_READDIR_ON_OPEN", "EMPTY_DIR")
    os.environ.setdefault("CPL_VSIL_CURL_ALLOWED_EXTENSIONS", ".tif")

    n = int(round(side / cell))
    if abs(n * cell - side) > 1e-6:
        raise ValueError(f"{side} m does not divide by {cell} m")
    dst = np.zeros((n, n), np.uint8)                      # row 0 = NORTH here
    dst_tf = from_origin(e0, n0 + side, cell, cell)
    dst_crs = f"EPSG:{epsg}"
    names = {_wc_name(la, lo) for la, lo in _ll_corners(epsg, e0, n0, side)}
    got = False
    for name in sorted(names):
        try:
            with rasterio.open(WORLDCOVER.format(name=name)) as ds:
                part = np.zeros((n, n), np.uint8)
                reproject(source=rasterio.band(ds, 1), destination=part,
                          src_transform=ds.transform, src_crs=ds.crs,
                          dst_transform=dst_tf, dst_crs=dst_crs,
                          resampling=Resampling.nearest, dst_nodata=0)
        except Exception:
            continue
        take = (dst == 0) & (part != 0)
        dst[take] = part[take]
        got = True
    if not got:
        raise RuntimeError(f"worldcover: no covering tile for {sorted(names)}")
    return np.flipud(dst)                                  # -> row 0 = SOUTH


def tcc_for_tile(epsg: int, e0: float, n0: float, side: float) -> np.ndarray:
    """Percent canopy on the tile's projected grid, row 0 = SOUTH.

    The WMS returns a palettised image whose palette is a monotone ramp, so the
    PALETTE INDEX is the percentage; verified against the ramp's endpoints.
    Pixel count is rounded, since 30 m does not divide every tile side, and the
    true cell is recorded rather than assumed.
    """
    import io
    import requests
    from PIL import Image
    n = max(1, int(round(side / TCC_CELL_M)))
    params = dict(service="WMS", version="1.3.0", request="GetMap",
                  layers=TCC_LAYER, styles="", crs=f"EPSG:{epsg}",
                  bbox=f"{e0},{n0},{e0 + side},{n0 + side}",
                  width=n, height=n, format="image/geotiff")
    last = ""
    for attempt in range(4):
        try:
            r = requests.get(TCC_WMS, params=params, headers=config.UA, timeout=180)
            if r.status_code == 200 and r.content[:2] in (b"II", b"MM"):
                a = np.array(Image.open(io.BytesIO(r.content)))
                if a.shape == (n, n):
                    return np.flipud(a.astype(np.uint8))
                last = f"shape {a.shape} != {n}"
            else:
                last = f"http {r.status_code}"
        except Exception as ex:                      # noqa: BLE001
            last = repr(ex)[:80]
        time.sleep(3.0 * (attempt + 1))
    raise RuntimeError(f"tcc export failed: {last}")


def _write(dest: pathlib.Path, cls: np.ndarray) -> None:
    cgrid.write_u8(dest, cls.astype(np.uint8), 0.0, 0.0, CELL_M)


def run_courses(limit: int | None = None) -> dict:
    """The golf corpus tiles, whose registry record carries the tile origin."""
    stats = {"fetched": 0, "reused": 0, "bad": 0, "skipped": 0}
    for rec in registry.all_records():
        t = rec.get("tile") or {}
        if not t.get("path"):
            stats["skipped"] += 1
            continue
        dest = pathlib.Path(t["path"]).with_suffix(".canopy.cgrid")
        if dest.exists():
            stats["reused"] += 1
            continue
        try:
            epsg, e0, n0 = int(t["epsg"]), float(t["e0"]), float(t["n0"])
            side = float(t["side_m"])
            cls = classes_for_tile(epsg, e0, n0, side)
            _write(dest, cls)
            tcc = tcc_for_tile(epsg, e0, n0, side)
            tdest = pathlib.Path(t["path"]).with_suffix(".tcc.cgrid")
            cgrid.write_u8(tdest, tcc, 0.0, 0.0, side / tcc.shape[0])
            rec["canopy"] = dict(path=str(dest), cell_m=CELL_M, source=SOURCE,
                                 tree_frac=round(float((cls == CLASS_TREE).mean()), 4),
                                 tcc_path=str(tdest), tcc_cell_m=side / tcc.shape[0],
                                 tcc_source=TCC_SOURCE,
                                 tcc_mean=round(float(tcc.mean()), 2))
            registry.write(rec)
            stats["fetched"] += 1
            print(f"  {rec['course_id'][:40]:40s} treed {100 * (cls == CLASS_TREE).mean():5.1f} %"
                  f"  canopy {tcc.mean():5.1f} %", flush=True)
        except Exception as ex:
            stats["bad"] += 1
            print(f"  {rec['course_id'][:40]:40s} FAILED {ex}", flush=True)
        time.sleep(COURTESY_S)
        if limit and stats["fetched"] >= limit:
            break
    return stats


def run_natural(limit: int | None = None) -> dict:
    """The macro_campaign archetype tiles: natural land, no golf.

    Their JSON sidecar already carries epsg / easting0 / northing0, so no
    projection is needed at all. Tile side is a fixed 3000 m.
    """
    stats = {"fetched": 0, "reused": 0, "bad": 0}
    for js in sorted(NATURAL_TILES.glob("*/*.json")):
        dest = js.with_suffix(".canopy.cgrid")
        if dest.exists():
            stats["reused"] += 1
            continue
        m = json.loads(js.read_text())
        side = 3000.0
        try:
            epsg, e0, n0 = int(m["epsg"]), float(m["easting0"]), float(m["northing0"])
            cls = classes_for_tile(epsg, e0, n0, side)
            _write(dest, cls)
            tcc = tcc_for_tile(epsg, e0, n0, side)
            cgrid.write_u8(js.with_suffix(".tcc.cgrid"), tcc, 0.0, 0.0,
                           side / tcc.shape[0])
            stats["fetched"] += 1
            print(f"  {js.parent.name:16s} {js.stem:16s} treed "
                  f"{100 * (cls == CLASS_TREE).mean():5.1f} %  canopy "
                  f"{tcc.mean():5.1f} %", flush=True)
        except Exception as ex:
            stats["bad"] += 1
            print(f"  {js.parent.name:16s} {js.stem:16s} FAILED {ex}", flush=True)
        time.sleep(COURTESY_S)
        if limit and stats["fetched"] >= limit:
            break
    return stats


def main() -> None:
    a = sys.argv[1:]
    which = a[0] if a else "courses"
    limit = None
    if "--limit" in a:
        limit = int(a[a.index("--limit") + 1])
    t0 = time.time()
    stats = run_natural(limit) if which == "natural" else run_courses(limit)
    print(f"{which}: {stats}  [{time.time() - t0:.0f} s]")


if __name__ == "__main__":
    main()
