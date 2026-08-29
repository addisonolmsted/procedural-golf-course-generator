"""Phase 6: boundary shapes + fitted-window coverage vs the 70-90% target.

Only courses with status "keeper" enter coverage (multi_course_unsplit is
excluded by design — greens from several courses under one polygon is the
exact confound that made NC read 56%). 18-hole courses get a 2x-area window
of the same shape family; the coverage target applies per assigned course.
"""

from __future__ import annotations

import json

import numpy as np

from . import config, geo, registry
from .features import greens_local


def shape_stats() -> dict:
    """Course polygon geometry by hole count — the shape family evidence."""
    rows = []
    for r in registry.all_records():
        if r["status"] not in ("keeper", "few_greens"):
            continue
        o = r["osm"]
        rows.append(dict(course_id=r["course_id"], region=r["region_tag"],
                         holes=r.get("holes_estimate") or 0,
                         area_ha=o["area_m2"] / 1e4,
                         aspect=o["pca_long_m"] / max(o["pca_short_m"], 1.0),
                         long_m=o["pca_long_m"], short_m=o["pca_short_m"],
                         compact=o["compactness"]))
    out = {}
    for h in (9, 18):
        sub = [r for r in rows if r["holes"] == h]
        if len(sub) < 5:
            continue
        out[str(h)] = {k: [float(np.percentile([r[k] for r in sub], p))
                           for p in (25, 50, 75)]
                       for k in ("area_ha", "aspect", "long_m", "short_m",
                                 "compact")}
        out[str(h)]["n"] = len(sub)
    out["rows"] = rows
    return out


def best_cover(gs: np.ndarray, H: float, W: float,
               step: float = 25.0) -> tuple[float, tuple]:
    """Max fraction of greens inside any axis-aligned H x W placement (both
    orientations tried by the caller). Candidate edges = green coords and
    green-minus-side (maximal-rectangle argument) on a step grid."""
    best, arg = 0.0, None
    ys = np.unique(np.r_[gs[:, 0], gs[:, 0] - H])
    xs = np.unique(np.r_[gs[:, 1], gs[:, 1] - W])
    for y0 in ys:
        iny = (gs[:, 0] >= y0) & (gs[:, 0] <= y0 + H)
        if iny.mean() <= best:
            continue
        for x0 in xs:
            m = iny & (gs[:, 1] >= x0) & (gs[:, 1] <= x0 + W)
            fr = float(m.mean())
            if fr > best:
                best, arg = fr, (float(y0), float(x0), H, W)
    return best, arg


def coverage(dims9: dict[str, tuple[float, float]] | None = None) -> dict:
    """Per-keeper-course green coverage by the fitted window.

    dims9: {region_tag: (long_m, short_m)} 9-hole dims; default = shipped
    per-biome dims. 18-hole courses use the same aspect at 2x area."""
    if dims9 is None:
        dims9 = {}
    rows = []
    for r in registry.all_records():
        if r["status"] != "keeper" or not r.get("tile") or "path" not in r["tile"]:
            continue
        gs = greens_local(r)
        if len(gs) < config.MIN_GREENS_KEEP:
            continue
        long9, short9 = dims9.get(r["region_tag"], (1450.0, 950.0)
                                  if r["region_tag"] == "sandhills_ne"
                                  else (1000.0, 700.0))
        if r["holes_estimate"] >= 18:
            s = np.sqrt(2.0)
            H, W = long9 * s, short9 * s
        else:
            H, W = long9, short9
        cov, arg = 0.0, None
        for (a, b) in ((H, W), (W, H)):
            c, g = best_cover(gs, a, b)
            if c > cov:
                cov, arg = c, g
        rows.append(dict(course_id=r["course_id"], region=r["region_tag"],
                         holes=int(r["holes_estimate"]), n_greens=len(gs),
                         cover=cov * 100.0, window=arg))
    covs = [r["cover"] for r in rows]
    out = dict(rows=rows, n=len(rows),
               median=float(np.median(covs)) if covs else -1.0,
               frac_ge70=float(np.mean([c >= 70.0 for c in covs]) * 100)
               if covs else -1.0)
    (config.OUT / "coverage.json").write_text(json.dumps(out, indent=1))
    return out
