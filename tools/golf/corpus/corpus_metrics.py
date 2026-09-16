"""Run the shared hole/course metric definitions over the real-course corpus.

`tools/golf/hole_metrics.py` holds the single definition of every quantity;
this tool feeds it the OSM `golf=hole` ways that `corpus/shot_profiles.joined()`
already joins to their keeper course tile, so the corpus side and the generator
side are measured by literally the same code.

Holes are kept when the way carries par 3/4/5 and runs at least
`hole_metrics.MIN_LEN_M`.  Courses are assembled from the way `ref` tag: a
front nine is a course whose refs 1..9 each appear exactly once among the kept
holes, a back nine the same for 10..18; each is passed to `course_metrics` in
play order.

Offline by construction (it inherits `shot_profiles`' cache-only Overpass).

Run from the repo root:  python3 tools/golf/corpus/corpus_metrics.py
Writes:                  tools/golf/corpus/out/corpus_metrics.json
"""

from __future__ import annotations

import json
import pathlib
import sys
import time

import numpy as np

_GOLF = pathlib.Path(__file__).resolve().parents[1]
if str(_GOLF) not in sys.path:
    sys.path.insert(0, str(_GOLF))
import hole_metrics as hm                       # noqa: E402
from corpus import config                       # noqa: E402
from corpus.shot_profiles import joined         # noqa: E402

PARS = (3, 4, 5)
FRONT = tuple(range(1, 10))
BACK = tuple(range(10, 19))


# --------------------------------------------------------------------------
# pass 1: every joined way -> hole_metrics
# --------------------------------------------------------------------------
def collect() -> tuple[list[dict], dict[int, np.ndarray], dict]:
    holes: list[dict] = []
    lines: dict[int, np.ndarray] = {}
    stats = dict(n_par=0, n_short=0, n_none=0)
    for j in joined():
        par = j["par"]
        if par not in PARS:
            stats["n_par"] += 1
            continue
        if j["length_m"] < hm.MIN_LEN_M:
            stats["n_short"] += 1
            continue
        m = hm.hole_metrics(j["z2"], j["cell2"], j["pts_loc"], par)
        if m is None:
            stats["n_none"] += 1
            continue
        m.update(way_id=j["way_id"], course_id=j["course_id"],
                 region=j["region"], ref=j["ref"])
        holes.append(m)
        lines[j["way_id"]] = np.asarray(j["pts_loc"], float)
    return holes, lines, stats


# --------------------------------------------------------------------------
# pass 2: refs 1-9 / 10-18 present exactly once -> course_metrics
# --------------------------------------------------------------------------
def build_courses(holes: list[dict], lines: dict[int, np.ndarray]) -> list[dict]:
    by_course: dict[str, list[dict]] = {}
    for h in holes:
        by_course.setdefault(h["course_id"], []).append(h)
    out = []
    for cid, hs in sorted(by_course.items()):
        seen: dict[int, list[dict]] = {}
        for h in hs:
            if h["ref"] is not None:
                seen.setdefault(h["ref"], []).append(h)
        for which, refs in (("front", FRONT), ("back", BACK)):
            if not all(len(seen.get(r, ())) == 1 for r in refs):
                continue
            nine = [seen[r][0] for r in refs]
            cm = hm.course_metrics(nine, [lines[h["way_id"]] for h in nine])
            out.append(dict(
                course_id=cid, region=nine[0]["region"], which=which,
                refs=list(refs), pars=[h["par"] for h in nine],
                way_ids=[h["way_id"] for h in nine], **cm))
    return out


# --------------------------------------------------------------------------
# summary
# --------------------------------------------------------------------------
HOLE_FIELDS = [
    "length_m", "drive_dogleg_deg", "second_dogleg_deg", "lateral_offset_frac",
    "sinuosity", "net_dz", "total_climb", "mean_abs_grade", "above_chord",
    "below_chord", "approach_grade_pct", "cross_drive_pct", "cross_green_pct",
    "section_relief_drive_m", "section_relief_green_m",
]
CROSS_FIELDS = ("cross_drive_pct", "cross_green_pct")
COURSE_FIELDS = [
    "total_length_m", "par3_spread_m", "par4_spread_m", "par5_spread_m",
    "bearing_change_med", "same_dir_share", "walk_med_m", "corridor_gap_med_m",
    "corridor_gap_min_m", "parallel_pair_share", "cross_flip_share",
]


def _vals(rows, key):
    return np.array([r[key] for r in rows if r.get(key) is not None], float)


def _row(label, v, qs=(10, 25, 50, 75, 90)):
    if not len(v):
        return f"    {label:24s}      --"
    ps = "  ".join(f"{np.percentile(v, q):9.2f}" for q in qs)
    return f"    {label:24s} n={len(v):5d}  {ps}"


def summarize(holes: list[dict], courses: list[dict]) -> None:
    print("\n=== corpus metrics: per-hole ===")
    for par in PARS:
        rows = [h for h in holes if h["par"] == par]
        print(f"\n  par {par}  n = {len(rows)}")
        print(f"    {'':24s} {'':7s}  {'p10':>9s}  {'p25':>9s}  {'p50':>9s}"
              f"  {'p75':>9s}  {'p90':>9s}")
        for f in HOLE_FIELDS:
            print(_row(f, _vals(rows, f)))
        for f in CROSS_FIELDS:
            v = np.abs(_vals(rows, f))
            if len(v):
                print(f"    {f} |.|>3 % {100.0 * (v > 3.0).mean():5.1f} %"
                      f"   |.|>6 % {100.0 * (v > 6.0).mean():5.1f} %"
                      f"   (n={len(v)})")
        if par == 5:
            r = [h["reach_in_two"] for h in rows if h["reach_in_two"] is not None]
            print(f"    reach_in_two {100.0 * float(np.mean(r)):.1f} %  (n={len(r)})")

    print(f"\n=== corpus metrics: per-nine ===   n = {len(courses)}")
    fr = sum(c["which"] == "front" for c in courses)
    print(f"  front {fr}   back {len(courses) - fr}   "
          f"distinct courses {len({c['course_id'] for c in courses})}")
    print(f"    {'':24s} {'':7s}  {'p25':>9s}  {'p50':>9s}  {'p75':>9s}")
    for f in COURSE_FIELDS:
        print(_row(f, _vals(courses, f), qs=(25, 50, 75)))


def run() -> None:
    t0 = time.time()
    holes, lines, stats = collect()
    print(f"  {len(holes)} holes measured  "
          f"(skipped {stats['n_par']} par not in 3/4/5, "
          f"{stats['n_short']} < {hm.MIN_LEN_M:.0f} m, "
          f"{stats['n_none']} hole_metrics None)  "
          f"[{time.time() - t0:.0f} s]")
    t1 = time.time()
    courses = build_courses(holes, lines)
    print(f"  {len(courses)} nines  [{time.time() - t1:.0f} s]")
    out = config.OUT / "corpus_metrics.json"
    out.write_text(json.dumps({"holes": holes, "courses": courses}))
    print(f"  wrote {out}")
    summarize(holes, courses)


if __name__ == "__main__":
    run()
