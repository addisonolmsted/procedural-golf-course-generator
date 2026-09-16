"""Measure our routed courses with the SHARED hole metrics.

  python3 tools/golf/route_metrics.py <dump_dir> <records.jsonl> <out.json> [--prefix m_]

Every hole of every routed record in the jsonl is measured by
`hole_metrics.hole_metrics` -- the same code, with the same arguments, that
measures a real corpus hole -- on its own dumped tile, read through
`route_audit.Terrain`. Our spines are already in the tile's frame, so the
polyline goes in as it stands.

Per hole the record keeps `seed`, `mode`, the hole index and the detector's
green-site `kind` beside the metrics; per course `course_metrics` is called
on the holes in play order with the matching spines, and keeps `seed`,
`mode` and the par sequence.

Writes {"holes": [...], "courses": [...]} and prints the distribution
summary: per par, and per par per mode.

Measurement only; nothing here changes a route, and nothing here redefines a
metric -- every number comes from `hole_metrics`.
"""
import sys, json, pathlib, time

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
from route_audit import Terrain                     # noqa: E402  (the one tile loader)
from hole_metrics import hole_metrics, course_metrics   # noqa: E402  (the one definition)

MODES = ("all", "aeolian", "fluvial")
PARS = (3, 4, 5)

HOLE_KEYS = ["length_m", "drive_dogleg_deg", "second_dogleg_deg", "lateral_offset_frac",
             "sinuosity", "net_dz", "total_climb", "mean_abs_grade", "above_chord",
             "below_chord", "approach_grade_pct", "cross_drive_pct", "cross_green_pct",
             "section_relief_drive_m", "section_relief_green_m"]
CROSS_KEYS = ["cross_drive_pct", "cross_green_pct"]
COURSE_KEYS = ["total_length_m", "par3_spread_m", "par4_spread_m", "par5_spread_m",
               "bearing_change_med", "same_dir_share", "walk_med_m", "corridor_gap_med_m",
               "corridor_gap_min_m", "parallel_pair_share", "cross_flip_share"]


# ------------------------------------------------------------------ measure --

def measure(dump, prefix, recs, progress=25):
    """(hole records, course records) for every routed record."""
    holes, courses = [], []
    t0 = time.time()
    for i, rec in enumerate(recs):
        if not rec.get("routed", True):
            continue
        seed, mode = rec["seed"], rec["mode"]
        t = Terrain(dump, prefix, seed)
        hs, lines, mine = [], [], []
        for k, h in enumerate(rec["holes"]):
            m = hole_metrics(t.z, t.c, h["spine"], h["par"])
            if m is None:
                print(f"  seed {seed} hole {k}: spine too short, skipped")
                continue
            r = dict(seed=seed, mode=mode, hole=k, kind=h.get("kind"))
            r.update(m)
            mine.append(r)
            hs.append(m)
            lines.append(h["spine"])
        holes.extend(mine)
        if hs:
            c = dict(seed=seed, mode=mode, pars=[h["par"] for h in hs])
            c.update(course_metrics(hs, lines))
            courses.append(c)
        if progress and (i + 1) % progress == 0:
            print(f"  {i + 1}/{len(recs)} seeds  ({time.time() - t0:.0f} s)", flush=True)
    return holes, courses


# ------------------------------------------------------------------ summary --

def vals(rows, key):
    return [r[key] for r in rows if r.get(key) is not None and np.isfinite(r[key])]


def pcts(v, ps=(10, 25, 50, 75, 90)):
    if not v:
        return None
    return [float(np.percentile(v, p)) for p in ps]


def fmt(v, w=9, d=2):
    return " " * w if v is None else f"{v:{w}.{d}f}"


def sub(rows, mode):
    return rows if mode == "all" else [r for r in rows if r["mode"] == mode]


def hole_table(holes, par):
    """Print the per-par block: one row per metric per mode."""
    hp = [h for h in holes if h["par"] == par]
    ns = {m: len(sub(hp, m)) for m in MODES}
    print(f"\n=== PAR {par} ===  n all {ns['all']}   aeolian {ns['aeolian']}   fluvial {ns['fluvial']}")
    print(f"{'metric':<24}{'mode':<9}{'n':>6}{'p10':>9}{'p25':>9}{'p50':>9}{'p75':>9}{'p90':>9}")
    for key in HOLE_KEYS:
        any_row = False
        for m in MODES:
            v = vals(sub(hp, m), key)
            q = pcts(v)
            if q is None:
                continue
            any_row = True
            print(f"{key if m == 'all' else '':<24}{m:<9}{len(v):>6}" + "".join(fmt(x) for x in q))
        if any_row:
            print()
    for key in CROSS_KEYS:
        line = f"|{key}| share"
        for m in MODES:
            v = vals(sub(hp, m), key)
            if not v:
                continue
            a = np.abs(np.asarray(v))
            print(f"{line if m == 'all' else '':<24}{m:<9}{len(v):>6}"
                  f"   >3 % {float((a > 3).mean()) * 100:5.1f} %   >6 % {float((a > 6).mean()) * 100:5.1f} %")
    if par == 5:
        for m in MODES:
            v = [h["reach_in_two"] for h in sub(hp, m) if h.get("reach_in_two") is not None]
            if v:
                print(f"{'reach_in_two' if m == 'all' else '':<24}{m:<9}{len(v):>6}"
                      f"   {float(np.mean(v)) * 100:5.1f} %")


def course_table(courses):
    print("\n=== COURSES ===  n all %d   aeolian %d   fluvial %d"
          % tuple(len(sub(courses, m)) for m in MODES))
    print(f"{'metric':<24}{'mode':<9}{'n':>6}{'p25':>9}{'p50':>9}{'p75':>9}")
    for key in COURSE_KEYS:
        for m in MODES:
            v = vals(sub(courses, m), key)
            q = pcts(v, (25, 50, 75))
            if q is None:
                continue
            print(f"{key if m == 'all' else '':<24}{m:<9}{len(v):>6}" + "".join(fmt(x) for x in q))
        print()


def kind_table(holes):
    kinds = sorted({h["kind"] for h in holes if h.get("kind") is not None})
    print("\n=== GREEN-SITE KIND per par ===")
    print(f"{'par':<6}{'mode':<9}{'n':>6}" + "".join(f"{k:>12}" for k in kinds))
    for par in PARS:
        hp = [h for h in holes if h["par"] == par]
        for m in MODES:
            rows = sub(hp, m)
            if not rows:
                continue
            cnt = {k: sum(1 for r in rows if r.get("kind") == k) for k in kinds}
            print(f"{par if m == 'all' else '':<6}{m:<9}{len(rows):>6}"
                  + "".join(f"{cnt[k]:>6} {100 * cnt[k] / len(rows):4.0f}%" for k in kinds))
        print()
    for m in MODES:
        rows = sub(holes, m)
        cnt = {k: sum(1 for r in rows if r.get("kind") == k) for k in kinds}
        print(f"{'all' if m == 'all' else '':<6}{m:<9}{len(rows):>6}"
              + "".join(f"{cnt[k]:>6} {100 * cnt[k] / len(rows):4.0f}%" for k in kinds))


def main():
    a = sys.argv[1:]
    prefix = "m_"
    if "--prefix" in a:
        i = a.index("--prefix"); prefix = a[i + 1]; a = a[:i] + a[i + 2:]
    if len(a) < 3:
        print(__doc__); sys.exit(1)
    dump, src, out = pathlib.Path(a[0]), a[1], pathlib.Path(a[2])
    recs = [json.loads(l) for l in open(src) if l.strip()]
    print(f"{len(recs)} records from {src}; tiles from {dump}/{prefix}*.cgrid")
    holes, courses = measure(dump, prefix, recs)
    out.parent.mkdir(parents=True, exist_ok=True)
    json.dump({"holes": holes, "courses": courses}, open(out, "w"))
    print(f"\nwrote {out}  ({len(holes)} holes, {len(courses)} courses)")
    for par in PARS:
        hole_table(holes, par)
    course_table(courses)
    kind_table(holes)


if __name__ == "__main__":
    main()
