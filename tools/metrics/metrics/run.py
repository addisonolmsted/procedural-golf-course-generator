"""Orchestrate the metric extraction over all reference courses.

    python3 -m metrics.run            # all 200 courses, both clip variants
    python3 -m metrics.run --limit 12 # quick subset while iterating
    python3 -m metrics.run --no-report

Writes under tools/metrics/out/:
  features_scalar.parquet     one row per (course_id, clip_variant), raw units
  features_scalar_z.parquet   z-scored (per clip; params in standardization.json)
  standardization.json        {clip: {feature: {mean, std}}}
  distributions/<id>_<clip>.npz   slope_hist, radial_psd, variogram, hypsometric_curve
  distributions/axes.npz          the shared axes (saved once)
  report/ ...                 plots (see report.py) + exploratory PCA (explore.py)
"""

from __future__ import annotations

import argparse
import json
import os
from concurrent.futures import ProcessPoolExecutor

import numpy as np
import pandas as pd
import yaml

from . import features, io

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.normpath(os.path.join(HERE, "..", "out"))


def _process(task):
    """Worker: (path, clip, cfg, axes) -> (meta, scalars, dists) or None."""
    path, clip, cfg, axes = task
    try:
        course = io.load_course(path, cfg, clip)
    except Exception as e:                    # never let one bad course kill the run
        return {"error": f"{os.path.basename(path)}/{clip}: {e}"}
    if course is None:
        return None
    S, D = features.compute(course.height, course.cell_size, course.valid,
                            cfg, axes, tree=course.tree, water=course.water)
    return {"meta": course.meta, "scalars": S, "dists": D}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--limit", type=int, default=0)
    ap.add_argument("--no-report", action="store_true")
    ap.add_argument("--workers", type=int, default=max(1, (os.cpu_count() or 2) - 1))
    args = ap.parse_args()

    cfg = yaml.safe_load(open(os.path.join(HERE, "config.yaml")))
    axes = features.build_axes(cfg)
    os.makedirs(OUT, exist_ok=True)
    dist_dir = os.path.join(OUT, "distributions")
    os.makedirs(dist_dir, exist_ok=True)
    np.savez_compressed(os.path.join(dist_dir, "axes.npz"), **axes)

    paths = io.list_courses(cfg)
    if args.limit:
        paths = paths[:args.limit]
    tasks = [(p, clip, cfg, axes) for p in paths for clip in cfg["clip"]["variants"]]
    print(f"{len(paths)} courses x {len(cfg['clip']['variants'])} clips = {len(tasks)} tiles, "
          f"{args.workers} workers")

    rows = []
    errors = []
    done = 0
    with ProcessPoolExecutor(max_workers=args.workers) as ex:
        for res in ex.map(_process, tasks, chunksize=2):
            done += 1
            if done % 40 == 0:
                print(f"  {done}/{len(tasks)}")
            if res is None:
                continue
            if "error" in res:
                errors.append(res["error"])
                continue
            meta, S, D = res["meta"], res["scalars"], res["dists"]
            cid, clip = meta["course_id"], meta["clip_variant"]
            row = {k: meta.get(k) for k in
                   ("course_id", "label", "arch", "group", "clip_variant",
                    "native_cell_x_m", "native_cell_y_m", "common_cell_m",
                    "survey_area_m2", "valid_area_m2", "n_valid_cells",
                    "grid_ny", "grid_nx", "n_holes", "treepct_atlas",
                    "waterpct_atlas", "emin", "emax", "flags")}
            row.update({c: S.get(c, float("nan")) for c in features.SCALAR_COLUMNS})
            rows.append(row)
            np.savez_compressed(os.path.join(dist_dir, f"{cid}_{clip}.npz"), **D)

    df = pd.DataFrame(rows).sort_values(["clip_variant", "course_id"]).reset_index(drop=True)
    df.to_parquet(os.path.join(OUT, "features_scalar.parquet"))
    print(f"wrote features_scalar.parquet  ({len(df)} rows)")
    if errors:
        with open(os.path.join(OUT, "errors.log"), "w") as f:
            f.write("\n".join(errors))
        print(f"  {len(errors)} tiles errored (see out/errors.log)")

    # standardization per clip, then z-scored table -------------------------
    std = {}
    dz = df.copy()
    for clip in df["clip_variant"].unique():
        sub = df[df["clip_variant"] == clip]
        std[clip] = {}
        for c in features.SCALAR_COLUMNS:
            m = float(np.nanmean(sub[c])); s = float(np.nanstd(sub[c]))
            std[clip][c] = {"mean": m, "std": s}
            if s > 0:
                dz.loc[dz["clip_variant"] == clip, c] = (sub[c] - m) / s
    json.dump(std, open(os.path.join(OUT, "standardization.json"), "w"), indent=1)
    dz.to_parquet(os.path.join(OUT, "features_scalar_z.parquet"))
    print("wrote features_scalar_z.parquet + standardization.json")

    if not args.no_report:
        from . import report, explore
        report.generate(df, axes, cfg, OUT)
        explore.run(df, OUT)


if __name__ == "__main__":
    main()
