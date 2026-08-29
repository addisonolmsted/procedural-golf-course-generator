"""Phase 7 measurement: how real greens SPREAD — the anti-clustering targets.

Per keeper course: nearest-neighbour spacing percentiles, 8-sector occupancy
about the greens centroid, convex-hull area as a fraction of the fitted
window, and boundary-distance spread. P9 thinning is then held to these
numbers (hull fraction >= real p25, sector occupancy >= real median).
"""

from __future__ import annotations

import json

import numpy as np

from . import config, registry
from .features import greens_local


def _hull_area(pts: np.ndarray) -> float:
    """Convex hull area by monotone chain + shoelace (no scipy.spatial dep)."""
    p = pts[np.lexsort((pts[:, 0], pts[:, 1]))]
    if len(p) < 3:
        return 0.0

    def half(seq):
        out = []
        for q in seq:
            while len(out) >= 2 and np.cross(out[-1] - out[-2],
                                             q - out[-2]) <= 0:
                out.pop()
            out.append(q)
        return out

    hull = np.array(half(list(p)) [:-1] + half(list(p[::-1]))[:-1])
    y, x = hull[:, 0], hull[:, 1]
    return 0.5 * abs(np.dot(x, np.roll(y, -1)) - np.dot(y, np.roll(x, -1)))


def per_course(gs: np.ndarray, window_area_m2: float) -> dict:
    d = np.sqrt(((gs[:, None, :] - gs[None, :, :]) ** 2).sum(-1))
    np.fill_diagonal(d, np.inf)
    nn = d.min(1)
    cen = gs.mean(0)
    ang = np.arctan2(gs[:, 0] - cen[0], gs[:, 1] - cen[1])
    sectors = len(np.unique(((ang + np.pi) / (2 * np.pi / 8)).astype(int) % 8))
    return dict(nn_p10=float(np.percentile(nn, 10)),
                nn_p50=float(np.percentile(nn, 50)),
                nn_p90=float(np.percentile(nn, 90)),
                sectors=int(sectors),
                hull_frac=float(_hull_area(gs) / max(window_area_m2, 1.0)))


def run(coverage_path=None) -> dict:
    cov = json.loads((coverage_path or config.OUT / "coverage.json")
                     .read_text())
    win = {r["course_id"]: r["window"] for r in cov["rows"] if r["window"]}
    rows = []
    for r in registry.all_records():
        if r["status"] != "keeper" or r["course_id"] not in win:
            continue
        gs = greens_local(r)
        w = win[r["course_id"]]
        rows.append(dict(course_id=r["course_id"], region=r["region_tag"],
                         **per_course(gs, w[2] * w[3])))
    agg = {}
    for k in ("nn_p10", "nn_p50", "nn_p90", "sectors", "hull_frac"):
        v = [r[k] for r in rows]
        agg[k] = dict(p25=float(np.percentile(v, 25)),
                      p50=float(np.percentile(v, 50)),
                      p75=float(np.percentile(v, 75)))
    out = dict(rows=rows, agg=agg, n=len(rows))
    (config.OUT / "dispersion.json").write_text(json.dumps(out, indent=1))
    return out
