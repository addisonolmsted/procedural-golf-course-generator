"""Development screen: which cells of a tile are man-made.

Macro-landform priors must be fitted to LANDSCAPE, not to subdivisions,
section-line roads, quarries or graded pads. QA of the pilot corpus found
all of those, and they do not just look bad — a road embankment is real
graded topography, so roadside ditches become "channels", cut-and-fill
edges become "scarps", and a borrow pit becomes a "basin".

This module asks OSM what is built on a tile, rasterizes it to the tile
grid, and hands extract.py a mask so the detectors can ignore it. Tiles that
are too built-up to trust are auto-culled into `out/exclude.json`, which the
fit already honors.

Overpass is slow and rate-limited, so results are cached per tile under
`out/develop/<archetype>/<id>.json` (+ a `.cgrid` mask); re-running is free.
"""

import json
import pathlib
import sys

import numpy as np
import rasterio.features
import rasterio.warp
from affine import Affine

_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_ROOT / "dtm_atlas"))

from dtm_atlas import osm  # noqa: E402

from . import cgrid  # noqa: E402
from .regions import CELL_M, TILE_M  # noqa: E402

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"
DEVELOP = OUT / "develop"

DEVELOP_VERSION = 1
# A tile with more than this fraction of built cells is not a landscape
# sample any more. 2% of a 3 km tile is ~18 ha of roads and buildings —
# a couple of farm lanes pass, a subdivision does not.
MAX_DEVELOPED_FRAC = 0.02
# Buildings disturb ground beyond their footprint (pads, driveways, yards).
BUILDING_DILATE_M = 15.0
# Quarries/landfills/construction are wholesale earthmoving.
DISTURBED_DILATE_M = 25.0


def tile_bbox_ll(meta: dict) -> tuple[float, float, float, float]:
    """(south, west, north, east) lat/lon of the tile, from its UTM origin."""
    e0, n0 = meta["easting0"], meta["northing0"]
    xs = [e0, e0 + TILE_M, e0 + TILE_M, e0]
    ys = [n0, n0, n0 + TILE_M, n0 + TILE_M]
    lon, lat = rasterio.warp.transform(f"EPSG:{meta['epsg']}", "EPSG:4326", xs, ys)
    return (min(lat), min(lon), max(lat), max(lon))


def _to_local(coords, meta) -> list[list[float]]:
    """GeoJSON lon/lat ring or line -> tile-local metres (x east, y north)."""
    lon = [c[0] for c in coords]
    lat = [c[1] for c in coords]
    xs, ys = rasterio.warp.transform("EPSG:4326", f"EPSG:{meta['epsg']}", lon, lat)
    return [[x - meta["easting0"], y - meta["northing0"]] for x, y in zip(xs, ys)]


def _rasterize(geoms, shape) -> np.ndarray:
    """Burn tile-local geometries onto the tile grid.

    The affine uses a POSITIVE y term because campaign tiles are row 0 =
    south (`fetch.py` flips the north-up export), unlike the atlas store.
    """
    if not geoms:
        return np.zeros(shape, dtype=bool)
    transform = Affine(CELL_M, 0.0, 0.0, 0.0, CELL_M, 0.0)
    burned = rasterio.features.rasterize(
        geoms,
        out_shape=shape,
        transform=transform,
        fill=0,
        default_value=1,
        all_touched=True,
        dtype="uint8",
    )
    return burned.astype(bool)


def _dilate_m(mask: np.ndarray, radius_m: float) -> np.ndarray:
    if radius_m <= 0 or not mask.any():
        return mask
    from scipy import ndimage

    dist = ndimage.distance_transform_edt(~mask, sampling=CELL_M)
    return dist <= radius_m


def build_mask(meta: dict, shape: tuple[int, int]) -> tuple[np.ndarray, dict]:
    """Query OSM for the tile and rasterize a developed mask."""
    bbox = tile_bbox_ll(meta)
    resp = osm.overpass(osm.features_query(bbox))
    feats = osm.classify_features(resp)

    counts = {}
    mask = np.zeros(shape, dtype=bool)

    # roads / rail / parking: lines carry a half-width, polygons are lots
    road_lines = feats["road"]["lines"]
    road_polys = feats["road"]["polys"]
    counts["road_lines"] = len(road_lines)
    counts["road_polys"] = len(road_polys)
    for coords, halfwidth in road_lines:
        local = _to_local(coords, meta)
        geom = {"type": "LineString", "coordinates": local}
        burned = _rasterize([geom], shape)
        mask |= _dilate_m(burned, max(float(halfwidth), CELL_M))
    mask |= _rasterize(
        [{"type": "Polygon", "coordinates": [_to_local(r, meta) for r in p["coordinates"]]}
         for p in road_polys],
        shape,
    )

    for cls, dilate in (("building", BUILDING_DILATE_M), ("disturbed", DISTURBED_DILATE_M)):
        polys = feats[cls]["polys"]
        counts[cls] = len(polys)
        burned = _rasterize(
            [{"type": "Polygon", "coordinates": [_to_local(r, meta) for r in p["coordinates"]]}
             for p in polys],
            shape,
        )
        mask |= _dilate_m(burned, dilate)

    info = {
        "develop_version": DEVELOP_VERSION,
        "bbox_ll": list(bbox),
        "counts": counts,
        "developed_frac": float(mask.mean()),
    }
    return mask, info


def mask_path(archetype: str, tile: str) -> pathlib.Path:
    return DEVELOP / archetype / f"{tile}.cgrid"


def load_mask(archetype: str, tile: str) -> np.ndarray | None:
    """Cached developed mask for a tile, or None if the screen hasn't run."""
    p = mask_path(archetype, tile)
    if not p.exists():
        return None
    data, _ = cgrid.read_u8(p)
    return data > 0


def run(archetype: str | None = None, force: bool = False, auto_exclude: bool = True):
    tiles_dir = OUT / "tiles"
    summary = []
    for arch_dir in sorted(tiles_dir.iterdir()) if tiles_dir.exists() else []:
        if not arch_dir.is_dir() or (archetype and arch_dir.name != archetype):
            continue
        dest_dir = DEVELOP / arch_dir.name
        dest_dir.mkdir(parents=True, exist_ok=True)
        for tile in sorted(arch_dir.glob("*.cgrid")):
            meta_p = tile.with_suffix(".json")
            info_p = dest_dir / f"{tile.stem}.json"
            if info_p.exists() and not force:
                info = json.loads(info_p.read_text())
                if info.get("develop_version") == DEVELOP_VERSION:
                    print(f"  [skip] {arch_dir.name}/{tile.stem}"
                          f"  developed {info['developed_frac'] * 100:.2f}%")
                    summary.append((arch_dir.name, tile.stem, info["developed_frac"]))
                    continue
            meta = json.loads(meta_p.read_text())
            z, _ = cgrid.read_f32(tile)
            try:
                mask, info = build_mask(meta, z.shape)
            except Exception as exc:
                print(f"  [fail] {arch_dir.name}/{tile.stem}: {type(exc).__name__}: {exc}")
                continue
            cgrid.write_u8(mask_path(arch_dir.name, tile.stem),
                           mask.astype(np.uint8), 0.0, 0.0, CELL_M)
            info_p.write_text(json.dumps(info, indent=1, sort_keys=True) + "\n")
            frac = info["developed_frac"]
            summary.append((arch_dir.name, tile.stem, frac))
            flag = "  <-- OVER LIMIT" if frac > MAX_DEVELOPED_FRAC else ""
            print(f"  [ok] {arch_dir.name}/{tile.stem}  developed {frac * 100:.2f}%"
                  f"  ({info['counts']}){flag}")

    if auto_exclude and summary:
        _auto_exclude(summary)
    return summary


def _auto_exclude(summary):
    """Add over-limit tiles to the QA cull list (reversible in tile-lab)."""
    p = OUT / "exclude.json"
    doc = json.loads(p.read_text()) if p.exists() else {"version": 1, "tiles": []}
    existing = {(t["archetype"], t["tile"]) for t in doc["tiles"]}
    added = 0
    for arch, tile, frac in summary:
        if frac <= MAX_DEVELOPED_FRAC or (arch, tile) in existing:
            continue
        doc["tiles"].append(
            {"archetype": arch, "tile": tile,
             "reason": f"auto: developed {frac * 100:.1f}% (OSM roads/buildings)"}
        )
        added += 1
    if added:
        doc["tiles"].sort(key=lambda t: (t["archetype"], t["tile"]))
        p.write_text(json.dumps(doc, indent=1) + "\n")
    print(f"\nauto-excluded {added} tile(s) over {MAX_DEVELOPED_FRAC * 100:.0f}% developed"
          f" -> {p}")
