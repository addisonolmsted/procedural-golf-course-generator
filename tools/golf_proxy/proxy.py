"""The golfability proxy (workplan B5).

"Will this ground take a golf course" — measurable from a surface alone,
with thresholds that come from REALITY: the 64 real golf-course grids in
`macro_campaign/out/courses/` define what routable ground measures as.
No router required; usable at G-SKELETON on base surfaces, as the siting
score's backbone at S5, and as a sanity check at G-TERRAIN.

Quantities per surface (or per window):
  frac_under_cap      fraction of cells at slope <= CAP (8%, the fairway cap)
  frac_under_steep    fraction at slope <= STEEP (15%, playable-rough cap)
  largest_contig_ha   largest connected component of sub-cap ground, hectares
  relief_p95_p5       relief span, m (a band: golf wants some, not too much)

Usage:
  python3 tools/golf_proxy/proxy.py measure   # -> proxy_thresholds.json
  python3 tools/golf_proxy/proxy.py score TILE.cgrid
  from proxy import score_surface, load_thresholds   # as a library
"""
from __future__ import annotations

import json
import os
import sys

import numpy as np
from scipy import ndimage

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.normpath(os.path.join(HERE, "..", ".."))
sys.path.insert(0, os.path.join(REPO, "tools", "macro_campaign"))

COURSES = os.path.join(REPO, "tools/macro_campaign/out/courses")
THRESHOLDS = os.path.join(HERE, "proxy_thresholds.json")

CAP = 0.08     # fairway slope cap, rise/run
STEEP = 0.15   # playable-rough cap


def quantities(z: np.ndarray, cell: float) -> dict:
    """The proxy quantities for one surface. Pure f(height, cell)."""
    valid = np.isfinite(z)
    gy, gx = np.gradient(np.where(valid, z, np.nan), cell)
    slope = np.hypot(gx, gy)
    s = slope[np.isfinite(slope)]
    if s.size == 0:
        return {k: float("nan") for k in
                ("frac_under_cap", "frac_under_steep", "largest_contig_ha",
                 "relief_p95_p5")}
    under = np.isfinite(slope) & (slope <= CAP)
    lab, n = ndimage.label(under)
    largest = int(np.bincount(lab.ravel())[1:].max()) if n else 0
    v = z[valid]
    return {
        "frac_under_cap": float((s <= CAP).mean()),
        "frac_under_steep": float((s <= STEEP).mean()),
        "largest_contig_ha": largest * cell * cell / 1e4,
        "relief_p95_p5": float(np.percentile(v, 95) - np.percentile(v, 5)),
    }


def load_thresholds(path: str = THRESHOLDS) -> dict:
    return json.load(open(path))


def score_surface(z: np.ndarray, cell: float, thresholds: dict) -> dict:
    """Score a surface (or window crop) against the real-course thresholds.

    `in_regime`: every floor met and relief inside its band. `score`: mean of
    per-quantity margins clamped to [0, 1] — a smooth fitness for siting, with
    the relief band scored by distance to the band's nearest edge.
    """
    q = quantities(z, cell)
    t = thresholds["thresholds"]
    margins = []
    ok = True
    for k in ("frac_under_cap", "frac_under_steep"):
        floor = t[k]["floor"]
        ok &= q[k] >= floor
        margins.append(min(max((q[k] - floor) / max(1.0 - floor, 1e-9) + 1.0, 0.0), 1.0)
                       if q[k] >= floor else max(q[k] / max(floor, 1e-9), 0.0))
    floor = t["largest_contig_ha"]["floor"]
    ok &= q["largest_contig_ha"] >= floor
    margins.append(min(q["largest_contig_ha"] / max(floor, 1e-9), 1.0))
    lo, hi = t["relief_p95_p5"]["band"]
    r = q["relief_p95_p5"]
    ok &= lo <= r <= hi
    if hi > lo:
        margins.append(max(0.0, 1.0 - 2.0 * abs(r - (lo + hi) / 2.0) / (hi - lo)))
    return {**q, "in_regime": bool(ok), "score": float(np.mean(margins))}


# ----------------------------------------------------------------- commands

def cmd_measure(window_m: float | None = None) -> int:
    """Measure the real-course floors; with `window_m`, measure the CENTRED
    window crop of each course instead of the full grid and write the
    parallel `proxy_thresholds_window<w>.json`.

    The window variant exists because the full-grid `largest_contig_ha`
    floor (123.7 ha, fitted on 900 ha grids) is UNSATISFIABLE on a play
    window -- an 800 m window is 64 ha total -- so applying it there
    silently disabled the term. Same instrument, honest scale.
    """
    from macro_campaign import cgrid

    rows = []
    seen: set[str] = set()
    for arch in sorted(os.listdir(COURSES)):
        d = os.path.join(COURSES, arch)
        if not os.path.isdir(d):
            continue
        for f in sorted(os.listdir(d)):
            if not f.endswith(".cgrid"):
                continue
            # DEDUPE by course id. Six Carolina tiles are filed under both
            # `piedmont` and `sandhills`; the old floors counted them twice
            # (n_courses said 64, the unique count is 58).
            cid = f[:-6]
            if cid in seen:
                print(f"  {arch}/{cid}: duplicate of an earlier archetype, skipped")
                continue
            seen.add(cid)
            z, (_ox, _oy, cell) = cgrid.read_f32(os.path.join(d, f))
            if window_m is not None:
                n = int(round(window_m / cell))
                y0 = (z.shape[0] - n) // 2
                x0 = (z.shape[1] - n) // 2
                z = z[y0:y0 + n, x0:x0 + n]
            q = quantities(z.astype(np.float64), cell)
            q.update(archetype=arch, course=cid)
            rows.append(q)
            print(f"  {arch}/{cid}: cap {q['frac_under_cap']:.2f}  "
                  f"contig {q['largest_contig_ha']:.0f} ha  "
                  f"relief {q['relief_p95_p5']:.1f} m")
    if not rows:
        print("no course grids found under", COURSES)
        return 1

    def col(k):
        return np.array([r[k] for r in rows if np.isfinite(r[k])])

    # Floors at the REAL p10: 90% of real course sites clear them. Relief is
    # a band (p5..p95): golf wants some relief, not unlimited relief.
    area_ha = None
    if window_m is not None:
        area_ha = window_m * window_m / 1e4
    out = {
        "version": 2,
        "n_courses": len(rows),
        "window_m": window_m,
        "source": "macro_campaign/out/courses (real 3DEP course grids, deduped)",
        "cap_rise_over_run": CAP,
        "steep_rise_over_run": STEEP,
        "thresholds": {
            "frac_under_cap": {"floor": float(np.percentile(col("frac_under_cap"), 10))},
            "frac_under_steep": {"floor": float(np.percentile(col("frac_under_steep"), 10))},
            "largest_contig_ha": {"floor": float(np.percentile(col("largest_contig_ha"), 10))},
            # As a FRACTION of the measured area, so the floor survives a
            # window-size change (percolation means it is still only valid at
            # the scale it was measured -- re-measure, don't rescale).
            "largest_contig_frac": {"floor": float(np.percentile(
                col("largest_contig_ha"), 10) / (area_ha if area_ha else 900.0))},
            "relief_p95_p5": {"band": [float(np.percentile(col("relief_p95_p5"), 5)),
                                        float(np.percentile(col("relief_p95_p5"), 95))]},
        },
        "per_course": rows,
    }
    dest = THRESHOLDS if window_m is None else os.path.join(
        HERE, f"proxy_thresholds_window{int(window_m)}.json")
    json.dump(out, open(dest, "w"), indent=1)
    print(f"\nwrote {dest} from {len(rows)} real courses")
    t = out["thresholds"]
    print(f"  floors: cap>={t['frac_under_cap']['floor']:.2f}  "
          f"steep>={t['frac_under_steep']['floor']:.2f}  "
          f"contig>={t['largest_contig_ha']['floor']:.0f} ha  "
          f"relief in [{t['relief_p95_p5']['band'][0]:.1f}, "
          f"{t['relief_p95_p5']['band'][1]:.1f}] m")

    # Sanity (B5 verification): real courses score >= their own floors.
    th = out
    n_ok = sum(score_surface_from_row(r, th) for r in rows)
    print(f"  sanity: {n_ok}/{len(rows)} real courses in-regime "
          f"(>= {int(0.75 * len(rows))} expected by construction)")
    return 0


def score_surface_from_row(row: dict, thresholds: dict) -> bool:
    t = thresholds["thresholds"]
    lo, hi = t["relief_p95_p5"]["band"]
    return (row["frac_under_cap"] >= t["frac_under_cap"]["floor"]
            and row["frac_under_steep"] >= t["frac_under_steep"]["floor"]
            and row["largest_contig_ha"] >= t["largest_contig_ha"]["floor"]
            and lo <= row["relief_p95_p5"] <= hi)


def cmd_score(path: str) -> int:
    from macro_campaign import cgrid

    z, (_ox, _oy, cell) = cgrid.read_f32(path)
    th = load_thresholds()
    s = score_surface(z.astype(np.float64), cell, th)
    print(json.dumps(s, indent=1))
    return 0


if __name__ == "__main__":
    if len(sys.argv) >= 2 and sys.argv[1] == "measure":
        w = float(sys.argv[2]) if len(sys.argv) >= 3 else None
        raise SystemExit(cmd_measure(w))
    if len(sys.argv) >= 3 and sys.argv[1] == "score":
        raise SystemExit(cmd_score(sys.argv[2]))
    print(__doc__)
    raise SystemExit(2)
