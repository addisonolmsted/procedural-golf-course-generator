"""Water masks for corpus tiles: OSM polygons UNION flat-plane detection.

Same detector as tools/golf/fetch_real_water.py (whose flat_water is imported
directly), generalised to the corpus's variable tile origin/side and cached
Overpass client. Writes <tile>.water.npy beside each cgrid.
"""

from __future__ import annotations

import json
import pathlib
import sys

import numpy as np
from PIL import Image, ImageDraw

from . import config, net, registry
from .geo import to_utm

_TOOLS = pathlib.Path(__file__).resolve().parents[2]
for _p in ("golf", "dtm_atlas", "macro_campaign"):
    if str(_TOOLS / _p) not in sys.path:
        sys.path.insert(0, str(_TOOLS / _p))
from fetch_real_water import QUERY, flat_water        # noqa: E402
from dtm_atlas.osm import element_polygons            # noqa: E402
from macro_campaign import cgrid                      # noqa: E402


def osm_water(rec: dict, shape, cell: float) -> tuple[np.ndarray, int]:
    t = rec["tile"]
    zone = rec["utm"]["zone"]
    lat, lon = rec["centroid_ll"]
    half_deg = (t["side_m"] / 2 + 400.0) / 111_000.0
    bbox = f"{lat-half_deg},{lon-half_deg},{lat+half_deg},{lon+half_deg}"
    # Water queries over lake-dense regions (Michigan moraine) return
    # megabyte responses and the mirrors time out; a stalled run cost 24
    # minutes on one course. OSM water is an ENHANCEMENT over the flat-plane
    # detector, not a prerequisite -- fail fast and let the caller fall back.
    els = net.overpass(QUERY.format(b=bbox), tries=2,
                       timeout=60).get("elements", [])
    im = Image.new("1", (shape[1], shape[0]), 0)
    dr = ImageDraw.Draw(im)
    n_poly = 0
    for el in els:
        for poly in element_polygons(el):
            for ri, ring in enumerate(poly["coordinates"]):
                pts = []
                for (lo, la) in ring:
                    ee, nn = to_utm(la, lo, zone)
                    pts.append(((ee - t["e0"]) / cell, (nn - t["n0"]) / cell))
                if len(pts) >= 3:
                    dr.polygon(pts, fill=(0 if ri else 1))
                    n_poly += ri == 0
    return np.array(im, bool), n_poly


def run(limit: int | None = None) -> dict:
    stats = {"written": 0, "cached": 0, "skipped": 0, "failed": 0,
             "flat_only": 0}
    seen = set()
    for rec in registry.all_records()[:limit]:
        t = rec.get("tile")
        if not t or "path" not in t:
            stats["skipped"] += 1
            continue
        tile = pathlib.Path(t["path"])
        if t.get("shared_with") or tile in seen:
            continue
        seen.add(tile)
        dest = tile.with_suffix(".water.npy")
        if dest.exists():
            stats["cached"] += 1
            continue
        try:
            z, (_, _, cell) = cgrid.read_f32(tile)
            zz = np.where(np.isfinite(z), z, np.nanmean(z)).astype(np.float64)
            try:
                mask, _ = osm_water(rec, z.shape, cell)
                src = "osm+flat"
            except Exception as exc:                  # noqa: BLE001
                print(f"  [osm-skip] {rec['course_id']}: {exc}", flush=True)
                mask = np.zeros(z.shape, bool)
                src = "flat"
                stats["flat_only"] += 1
            np.save(dest, mask | flat_water(zz, cell))
            rec["water_mask"] = dict(path=str(dest), source=src)
            registry.write(rec)
            stats["written"] += 1
        except Exception as exc:                      # noqa: BLE001
            print(f"  [fail] {rec['course_id']}: {exc}", flush=True)
            stats["failed"] += 1
    (config.OUT / "water_report.json").write_text(json.dumps(stats, indent=1))
    return stats
