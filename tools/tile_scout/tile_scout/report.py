"""Stage-0T gate report: per-course best-match table, energy distance +
per-metric KS (courses vs selected tiles), and the side-by-side gallery
(each course hillshade beside its nearest selected tile)."""

from __future__ import annotations

import json
import os

import numpy as np
import pandas as pd

from . import sconfig, scoring
from .f2_terrain import S1_COLS
from .select import S2_SUBSET


def _hillshade_png(store_root: str, key: str, out_png: str, max_px: int) -> bool:
    from dtm_atlas import grids, qa
    p = os.path.join(store_root, key, "naturalized.tif")
    if not os.path.exists(p):
        return False
    arr, _tf, _e = grids.read_gtiff(p)
    valid = arr != -9999.0
    img = qa.hillshade_rgb(arr, valid, 2.0)
    qa._save_png(out_png, img)
    return True


def run(cfg: dict) -> int:
    from . import archetypes
    from .f2_terrain import _zframe
    from dtm_atlas import config as atlas_config

    courses = pd.read_parquet(
        sconfig.repo_path(cfg["paths"]["course_metrics_parquet"]))
    tiles_all = pd.read_parquet(
        sconfig.repo_path("tools/dtm_metrics/out/metrics_tiles.parquet"))
    man = json.load(open(sconfig.repo_path(
        os.path.join(cfg["paths"]["atlas_root"], "tiles_us.json"))))
    sel_keys = [k for k, v in man.items() if v.get("selected")]
    tiles = tiles_all[tiles_all.index.isin(sel_keys)]

    # shared z-frame (S1 archetype frame + S2 course robust-z), same as select
    cols = [c for c in S1_COLS if c in tiles and c in courses]
    cz = _zframe(cfg, courses, weighted=True)[cols].fillna(0.0)
    tz = _zframe(cfg, tiles, weighted=True)[cols].fillna(0.0)
    extra = [c for c in S2_SUBSET if c in tiles and c in courses]
    if extra:
        ce = scoring.transform(courses, extra, [])
        med, sd = scoring.robust_scales(ce)
        cz = pd.concat([cz, scoring.zscore(ce, med, sd).fillna(0.0)], axis=1)
        tz = pd.concat([tz, scoring.zscore(
            scoring.transform(tiles, extra, []), med, sd).fillna(0.0)], axis=1)
    L = scoring.whitener(cz.to_numpy())
    Dc = scoring.pairwise_dist(L, cz.to_numpy(), tz.to_numpy())

    # per-course best match
    jbest = Dc.argmin(axis=1)
    best = pd.DataFrame({
        "course": courses.index,
        "best_tile": tiles.index[jbest],
        "distance": np.round(Dc.min(axis=1), 3),
        "region": [man[tiles.index[j]]["region"] for j in jbest],
    }).sort_values("distance", ascending=False)
    rp = sconfig.out_path("reports", "best_match.csv")
    best.to_csv(rp, index=False)

    # course-to-course NN baseline (context for the distance scale)
    Dcc = scoring.pairwise_dist(L, cz.to_numpy(), cz.to_numpy())
    np.fill_diagonal(Dcc, np.inf)
    nn = Dcc.min(axis=1)
    r_cover = float(np.median(nn))
    covered = float((Dc.min(axis=1) <= r_cover).mean())

    # distributional match
    ed = scoring.energy_distance(cz.to_numpy(), tz.to_numpy())
    # split-half course baseline for the energy scale
    rng = np.arange(len(cz))
    half = len(cz) // 2
    ed_base = scoring.energy_distance(cz.to_numpy()[rng[:half]],
                                      cz.to_numpy()[rng[half:2 * half]])
    from scipy.stats import ks_2samp
    ks = {c: round(float(ks_2samp(cz[c], tz[c]).statistic), 3) for c in cz}
    ks_sorted = dict(sorted(ks.items(), key=lambda kv: -kv[1]))

    summary = {
        "n_courses": len(courses), "n_selected_tiles": len(tiles),
        "coverage_at_r_cover": round(covered, 3),
        "r_cover_course_nn_median": round(r_cover, 3),
        "per_course_nn_p50": round(float(np.median(Dc.min(axis=1))), 3),
        "per_course_nn_p90": round(float(np.percentile(Dc.min(axis=1), 90)), 3),
        "per_course_nn_max": round(float(Dc.min(axis=1).max()), 3),
        "energy_distance": round(ed, 4),
        "energy_distance_course_splithalf": round(ed_base, 4),
        "ks_worst": dict(list(ks_sorted.items())[:6]),
        "worst_covered_courses": best.head(6)[
            ["course", "best_tile", "distance"]].to_dict("records"),
    }
    sconfig.save_json(sconfig.out_path("reports", "summary.json"), summary)
    print(json.dumps(summary, indent=1))

    # gallery: course hillshade | best selected tile hillshade
    gdir = sconfig.out_path("reports", "match_gallery", "img")
    os.makedirs(gdir, exist_ok=True)
    atlas_config.select_dataset("courses")
    course_store = atlas_config.STORE
    atlas_config.select_dataset("tiles")
    tile_store = atlas_config.STORE
    rows_html = []
    for row in best.sort_values("distance").itertuples():
        cpng = os.path.join(gdir, f"c_{row.course}.png")
        tpng = os.path.join(gdir, f"t_{row.best_tile}.png")
        _hillshade_png(course_store, row.course, cpng, cfg["qa"]["max_px"])
        _hillshade_png(tile_store, row.best_tile, tpng, cfg["qa"]["max_px"])
        rows_html.append(
            f"<tr><td>{row.course}</td>"
            f"<td><img src='img/c_{row.course}.png' width=320></td>"
            f"<td><img src='img/t_{row.best_tile}.png' width=320></td>"
            f"<td>{row.best_tile}<br>{man[row.best_tile]['region']}<br>"
            f"d={row.distance}</td></tr>")
    html = ("<!doctype html><meta charset=utf-8>"
            "<title>Stage-0T match gallery</title>"
            "<style>body{font-family:sans-serif}img{image-rendering:auto}"
            "td{vertical-align:top;padding:4px}</style>"
            f"<h2>Stage-0T: {len(tiles)} tiles vs {len(courses)} courses</h2>"
            f"<p>coverage@r_cover {covered:.0%}; energy {ed:.3f} "
            f"(course split-half {ed_base:.3f})</p>"
            "<table><tr><th>course</th><th>course DTM</th>"
            "<th>best tile DTM</th><th>match</th></tr>"
            + "".join(rows_html) + "</table>")
    with open(sconfig.out_path("reports", "match_gallery", "index.html"),
              "w") as f:
        f.write(html)
    print(f"wrote reports/best_match.csv, summary.json, match_gallery/")
    return 0
