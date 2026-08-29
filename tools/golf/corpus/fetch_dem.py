"""3DEP tiles for corpus courses: variable side, hardened fetch.

macro_campaign.fetch semantics (two-hop JSON->TIFF, origin snapped to CELL_M,
row 0 = south) plus the dtm_atlas/dep3.py hardening the direct path lacked:
bounded retries with linear backoff, TIFF magic validation, nodata-fraction
gate, courtesy sleep, resume-by-existence.

Side per course: clamp(TILE_MIN, polygon long span + 2*TILE_MARGIN rounded up
to TILE_STEP, TILE_MAX). 4500 m @ 2 m = 2250^2 = 5.06M px, under the ~7M-px
practical exportImage budget dtm_atlas measured. Tiles shared between courses
of one property dedupe on the snapped (epsg, e0, n0, side) key.
"""

from __future__ import annotations

import io
import json
import pathlib
import sys
import time

import numpy as np
import requests

from . import config, geo, registry

_TOOLS = pathlib.Path(__file__).resolve().parents[2]
if str(_TOOLS / "macro_campaign") not in sys.path:
    sys.path.insert(0, str(_TOOLS / "macro_campaign"))
from macro_campaign import cgrid  # noqa: E402


def _read_tiff_f32(buf: bytes) -> np.ndarray:
    try:
        import tifffile
        return tifffile.imread(io.BytesIO(buf)).astype(np.float32)
    except ImportError:
        from PIL import Image
        return np.array(Image.open(io.BytesIO(buf)), dtype=np.float32)


def side_for(rec: dict) -> float:
    span = max(rec["osm"]["pca_long_m"], 1.0) + 2 * config.TILE_MARGIN_M
    side = np.ceil(span / config.TILE_STEP_M) * config.TILE_STEP_M
    return float(np.clip(side, config.TILE_MIN_M, config.TILE_MAX_M))


def _export(e0: float, n0: float, side: float, epsg: int,
            tries: int = 4) -> np.ndarray:
    npx = int(side / config.CELL_M)
    params = {
        "f": "json",
        "bbox": f"{e0},{n0},{e0 + side},{n0 + side}",
        "bboxSR": epsg, "imageSR": epsg,
        "size": f"{npx},{npx}",
        "format": "tiff", "pixelType": "F32",
        "noData": config.NODATA,
        "interpolation": "RSP_BilinearInterpolation",
    }
    last = None
    for attempt in range(tries):
        try:
            r = requests.get(config.DEP3_EXPORT, params=params,
                             headers=config.UA, timeout=180)
            if r.status_code == 200:
                href = r.json().get("href")
                if href:
                    img = requests.get(href, headers=config.UA, timeout=300)
                    if (img.status_code == 200
                            and img.content[:2] in (b"II", b"MM")):
                        z = _read_tiff_f32(img.content)
                        if z.shape == (npx, npx):
                            return z
                        last = f"shape {z.shape} != {npx}"
                    else:
                        last = f"tiff {img.status_code} {img.content[:80]!r}"
                else:
                    last = "no href"
            else:
                last = f"{r.status_code}"
        except (requests.RequestException, ValueError) as e:
            last = f"{e}"
        time.sleep(3.0 * (attempt + 1))
    raise RuntimeError(f"3DEP export failed: {last}")


def run(limit: int | None = None, statuses=("keeper", "multi_course_unsplit",
                                            "few_greens")) -> dict:
    config.TILES.mkdir(parents=True, exist_ok=True)
    by_key: dict[tuple, str] = {}
    stats = {"fetched": 0, "reused": 0, "shared": 0, "bad": 0}
    recs = [r for r in registry.all_records() if r["status"] in statuses]
    for rec in recs[:limit]:
        lat, lon = rec["centroid_ll"]
        zone = rec["utm"]["zone"]
        epsg = 26900 + zone
        side = side_for(rec)
        yx = geo.ll_to_m([lat], [lon], zone)[0]
        e0 = round((yx[1] - side / 2) / config.CELL_M) * config.CELL_M
        n0 = round((yx[0] - side / 2) / config.CELL_M) * config.CELL_M
        key = (epsg, e0, n0, side)
        if key in by_key:
            owner = by_key[key]
            rec["tile"] = dict(path=str(config.TILES / f"{owner}.cgrid"),
                               side_m=side, cell_m=config.CELL_M,
                               e0=e0, n0=n0, epsg=epsg, shared_with=owner)
            registry.write(rec)
            stats["shared"] += 1
            continue
        dest = config.TILES / f"{rec['course_id']}.cgrid"
        if not dest.exists():
            try:
                z = _export(e0, n0, side, epsg)
            except RuntimeError as ex:
                rec["tile"] = dict(error=str(ex))
                registry.write(rec)
                stats["bad"] += 1
                continue
            z = np.where(np.isfinite(z) & (z > -1000.0) & (z < 9000.0)
                         & (z != config.NODATA), z, np.nan)
            nod = float(np.isnan(z).mean())
            if nod > config.MAX_NODATA_FRAC:
                rec["tile"] = dict(error=f"nodata_frac {nod:.3f}")
                registry.write(rec)
                stats["bad"] += 1
                continue
            # TIFF row 0 = north; CGRID1 row 0 = south
            cgrid.write_f32(dest, np.flipud(z).astype(np.float32),
                            0.0, 0.0, config.CELL_M)
            stats["fetched"] += 1
            time.sleep(config.COURTESY_SLEEP_S)
        else:
            stats["reused"] += 1
        by_key[key] = rec["course_id"]
        rec["tile"] = dict(path=str(dest), side_m=side, cell_m=config.CELL_M,
                           e0=e0, n0=n0, epsg=epsg, shared_with=None)
        registry.write(rec)
    registry.write_index()
    (config.OUT / "fetch_report.json").write_text(json.dumps(stats, indent=1))
    return stats
