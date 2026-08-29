"""Phase 3 driver: features.jsonl over every keeper + unsplit course.

Parallel over courses (fields+morphology ~30-60 s per tile in numpy; the
persistence flood is bbox-restricted). Deterministic: courses sorted, controls
seeded by course id, rows written in course order. Unmatched-control rows are
written to features_unmatched.jsonl for the diagnostic fit (B).
"""

from __future__ import annotations

import json
import sys
from concurrent.futures import ProcessPoolExecutor

from . import config, controls, features, registry


def one_course(cid: str) -> tuple[str, list, list]:
    rec = registry.load(cid)
    arr = features.course_arrays(rec)
    if arr is None:
        return cid, [], []
    mt, um = controls.controls_for(rec, arr["wet2"], arr["cell2"])
    rows = []
    for label, pts in ((1, features.greens_local(rec)), (0, mt)):
        for (ym, xm) in pts:
            ft = features.point_features(arr, ym, xm)
            if ft is None:
                continue
            ft.update(label=label, course_id=cid,
                      region=rec["region_tag"], fame=rec.get("fame_tier", 1))
            rows.append(ft)
    urows = []
    for (ym, xm) in um:
        ft = features.point_features(arr, ym, xm)
        if ft is None:
            continue
        ft.update(label=0, course_id=cid,
                  region=rec["region_tag"], fame=rec.get("fame_tier", 1))
        urows.append(ft)
    return cid, rows, urows


def run(procs: int = 4, statuses=("keeper", "multi_course_unsplit"),
        limit: int | None = None) -> dict:
    cids = sorted(r["course_id"] for r in registry.all_records()
                  if r["status"] in statuses and r.get("tile")
                  and "path" in r["tile"])[:limit]
    stats = {"courses": 0, "rows": 0, "unmatched": 0, "empty": 0}
    with open(config.OUT / "features.jsonl", "w") as fh, \
         open(config.OUT / "features_unmatched.jsonl", "w") as uh:
        if procs <= 1:
            results = map(one_course, cids)
        else:
            ex = ProcessPoolExecutor(procs)
            results = ex.map(one_course, cids, chunksize=1)
        for cid, rows, urows in results:
            if not rows:
                stats["empty"] += 1
                continue
            for r in rows:
                fh.write(json.dumps(r, sort_keys=True) + "\n")
            for r in urows:
                uh.write(json.dumps(r, sort_keys=True) + "\n")
            stats["courses"] += 1
            stats["rows"] += len(rows)
            stats["unmatched"] += len(urows)
            print(f"  {cid}: {sum(r['label'] for r in rows)} greens, "
                  f"{len(rows)} rows", flush=True)
    (config.OUT / "extract_report.json").write_text(json.dumps(stats, indent=1))
    return stats


if __name__ == "__main__":
    procs = int(sys.argv[1]) if len(sys.argv) > 1 else 4
    print(run(procs=procs))
