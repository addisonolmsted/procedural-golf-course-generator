"""F2 — 10 m fine screen: full 17-metric S1 vector per surviving candidate,
matched (shrunk-Mahalanobis) against the region's member courses in the
course frame. Course targets are computed by the SAME function from the
frozen 2 m store block-meaned to 10 m + 3 km center crop — candidates and
targets live in one estimator space.

The S1 recipe mirrors dtm_metrics.candidates' S1 block (unit weights,
valid ≡ finite); constants are pinned here so dtm_metrics config drift can't
silently move the screen."""

from __future__ import annotations

import os

import numpy as np
import pandas as pd

from . import sconfig

# pinned Stage-1 S1 constants (dtm_metrics/config.yaml provenance)
LOWPASS_L_M = 200.0
TPI_WINDOW_M = 300.0
MACRO_BANDS = [(800.0, np.inf), (400.0, 800.0), (200.0, 400.0), (50.0, 200.0)]
DRAINAGE_THRESH_M2 = 60000.0
ANISO_MAX_LAG_M = 600.0
ANISO_N_DIR = 8

S1_COLS = ["s1_relief_p95_p5", "s1_relief_p99_p1", "s1_lp_slope_p50",
           "s1_lp_slope_iqr", "s1_lp_slope_p90", "s1_hyps_q10", "s1_hyps_q25",
           "s1_hyps_q50", "s1_hyps_q75", "s1_hyps_q90", "s1_bp_800p",
           "s1_bp_400_800", "s1_bp_200_400", "s1_bp_50_200",
           "s1_drainage_density_coarse", "s1_aniso_ratio_macro",
           "s1_tilt_grade"]


def s1_vector(z_nan: np.ndarray, cell: float) -> dict:
    """The screening S1 vector on a bare array (NaN = invalid)."""
    from dtm_metrics import specwin, surfaces
    from dtm_metrics.weights import weighted_iqr, weighted_percentile
    from metrics import core as v1core

    valid = np.isfinite(z_nan)
    if valid.mean() < 0.5:
        return {c: np.nan for c in S1_COLS}
    z = np.where(valid, z_nan, 0.0)
    s = surfaces.build(z, valid, cell, LOWPASS_L_M, TPI_WINDOW_M)
    w = np.ones(int(valid.sum()))
    v = s.s1[valid]
    out: dict = {}
    p = weighted_percentile(v, w, [1.0, 5.0, 95.0, 99.0])
    out["s1_relief_p95_p5"] = float(p[2] - p[1])
    out["s1_relief_p99_p1"] = float(p[3] - p[0])
    ls = s.lp_slope[valid]
    out["s1_lp_slope_p50"] = float(weighted_percentile(ls, w, [50.0])[0])
    out["s1_lp_slope_iqr"] = float(weighted_iqr(ls, w))
    out["s1_lp_slope_p90"] = float(weighted_percentile(ls, w, [90.0])[0])
    lo, hi = weighted_percentile(v, w, [1.0, 99.0])
    span = max(hi - lo, 1e-6)
    zr = np.clip((v - lo) / span, 0.0, 1.0)
    for q in (0.10, 0.25, 0.50, 0.75, 0.90):
        out[f"s1_hyps_q{int(q * 100)}"] = float((zr < q).mean())
    resid, _tr = v1core.detrend(s.s1, cell, valid, "quadratic")
    P, f = specwin.window_periodogram(resid, cell, pre_detrended=True)
    rms = np.sqrt(specwin.band_variances(P, f, MACRO_BANDS))
    for (blo, bhi), val in zip(MACRO_BANDS, rms):
        name = (f"s1_bp_{int(blo)}p" if np.isinf(bhi)
                else f"s1_bp_{int(blo)}_{int(bhi)}")
        out[name] = float(val)
    k = max(1, int(round(8.0 / cell)))       # 10 m -> k=1 (nearest achievable)
    zb = _block_mean(s.s1, k) if k > 1 else s.s1
    vb = (_block_mean(valid.astype(float), k) > 0.5) if k > 1 else valid
    hyd = v1core.hydrology_metrics(zb, cell * k, vb, DRAINAGE_THRESH_M2)
    out["s1_drainage_density_coarse"] = float(hyd["drainage_density"])
    an = v1core.anisotropy(s.s1, cell, valid, "quadratic",
                           max_lag_m=ANISO_MAX_LAG_M, n_dir=ANISO_N_DIR)
    out["s1_aniso_ratio_macro"] = float(an["anisotropy_ratio"])
    ys, xs = np.nonzero(valid)
    step = max(1, ys.size // 200_000)
    ys, xs = ys[::step], xs[::step]
    A = np.stack([np.ones(ys.size), xs * cell, ys * cell], 1)
    coef, *_ = np.linalg.lstsq(A, s.s1[ys, xs], rcond=None)
    out["s1_tilt_grade"] = float(np.hypot(coef[1], coef[2]))
    return out


def _block_mean(a: np.ndarray, k: int) -> np.ndarray:
    ny, nx = a.shape[0] // k, a.shape[1] // k
    return a[: ny * k, : nx * k].reshape(ny, k, nx, k).mean(axis=(1, 3))


def _nan_block_mean(a: np.ndarray, k: int) -> np.ndarray:
    import warnings
    ny, nx = a.shape[0] // k, a.shape[1] // k
    blocks = a[: ny * k, : nx * k].reshape(ny, k, nx, k)
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        return np.nanmean(blocks, axis=(1, 3))


def course_targets(cfg: dict, force: bool = False) -> pd.DataFrame:
    """Course S1 vectors in the F2 estimator space: 2 m store -> 10 m block
    means -> 3 km center crop -> s1_vector."""
    out_p = sconfig.out_path("f2_course_targets.parquet")
    if os.path.exists(out_p) and not force:
        return pd.read_parquet(out_p)
    from dtm_atlas import config as atlas_config
    from dtm_atlas import grids
    atlas_config.select_dataset("courses")
    res = cfg["f2"]["res_m"]
    k = int(round(res / 2.0))
    n = int(cfg["tile"]["side_m"] / res)
    rows = {}
    keys = list(pd.read_parquet(
        sconfig.repo_path(cfg["paths"]["course_metrics_parquet"])).index)
    for key in keys:
        arr, _tf, _e = grids.read_gtiff(
            os.path.join(atlas_config.STORE, key, "naturalized.tif"))
        z = np.where(arr == atlas_config.NODATA, np.nan, arr.astype(float))
        zb = _nan_block_mean(z, k)
        r0 = max((zb.shape[0] - n) // 2, 0)
        c0 = max((zb.shape[1] - n) // 2, 0)
        rows[key] = s1_vector(zb[r0:r0 + n, c0:c0 + n], res)
    df = pd.DataFrame(rows).T.sort_index()[S1_COLS]
    df.to_parquet(out_p)
    return df


def fetch10(row, cfg: dict, sess) -> np.ndarray:
    from dtm_atlas import config as atlas_config
    from dtm_atlas import dep3, grids
    res = cfg["f2"]["res_m"]
    side = cfg["tile"]["side_m"]
    path = sconfig.out_path("dem10", f"{row.key}.tif")
    if os.path.exists(path):
        arr, _tf, _e = grids.read_gtiff(path)
    else:
        window = (row.x0, row.y0, row.x0 + side, row.y0 + side)
        arr = dep3.export_dem(window, int(row.epsg), res, sess)
        tf = grids.work_transform(window, res)
        grids.write_gtiff(path, arr, tf, int(row.epsg),
                          nodata=atlas_config.NODATA)
    return np.where(arr == atlas_config.NODATA, np.nan, arr.astype(float))


def _zframe(cfg: dict, df: pd.DataFrame, weighted: bool = False
            ) -> pd.DataFrame:
    """Project raw S1 vectors into the course frame (transforms + scales
    from archetypes.json — the single source of the course z-space).
    weighted=True applies the drift-calibrated per-metric weights (both
    sides of every distance use the same weighting)."""
    from . import archetypes, scoring
    a = archetypes.load(sconfig.out_path("archetypes.json"))
    acfg = cfg["archetypes"]
    t = scoring.transform(df, a.metric_cols, acfg["log1p_cols"])
    z = scoring.zscore(t, a.center, a.scale)
    if weighted:
        for c, w in cfg["f2"].get("metric_weights", {}).items():
            if c in z.columns:
                z[c] = z[c] * float(w)
    return z


def run(cfg: dict, only_region: str = "", force: bool = False) -> int:
    import requests
    from . import archetypes, regions as R, scoring
    f2 = cfg["f2"]
    regs = {r.name: r for r in R.load(sconfig.out_path("regions.json"))}
    cands = pd.read_parquet(sconfig.out_path("f0_candidates.parquet"))
    f1 = pd.read_parquet(sconfig.out_path("f1_survivors.parquet"))
    df = cands.merge(f1, on="key", how="inner")
    df = df[df["f0_keep"]]
    if only_region:
        df = df[df["region"] == only_region]
    if "f1_pass_relaxed" not in df.columns:
        df = df.assign(f1_pass_relaxed=df["f1_pass"])
    # budget: top 3x quota per region by f0 rank; shortfall regions top up
    # from the RELAXED tier (big roads/rail/residential/dev stay hard zeros)
    parts = []
    for rname, sub in df.groupby("region"):
        cap = f2["budget_per_region_factor"] * regs[rname].quota
        strict = sub[sub["f1_pass"]].sort_values(["f0_dist", "key"])
        take = strict.head(cap).assign(f1_relaxed=False)
        if len(take) < cap:
            extra = sub[~sub["f1_pass"] & sub["f1_pass_relaxed"]] \
                .sort_values(["f0_dist", "key"]) \
                .head(cap - len(take)).assign(f1_relaxed=True)
            if len(extra):
                print(f"  {rname}: +{len(extra)} relaxed-tier entrants")
            take = pd.concat([take, extra])
        parts.append(take)
    df = pd.concat(parts).sort_values("key").reset_index(drop=True)
    if df.empty:
        print("f2: no entrants — nothing to score")
        return 0
    print(f"f2: scoring {len(df)} candidates "
          f"({int(df['f1_relaxed'].sum())} relaxed-tier)")

    targets = course_targets(cfg)
    a = archetypes.load(sconfig.out_path("archetypes.json"))
    tz = _zframe(cfg, targets, weighted=True).fillna(0.0)
    L = scoring.whitener(tz.to_numpy())

    sess = requests.Session()
    recs = []
    for row in df.itertuples():
        z10 = fetch10(row, cfg, sess)
        vec = s1_vector(z10, f2["res_m"])
        recs.append({"key": row.key, **vec})
    vdf = pd.DataFrame(recs).set_index("key")
    vz = _zframe(cfg, vdf, weighted=True).fillna(0.0)

    # distance to nearest member course (whitened), + agri penalty
    out_rows = []
    for row in df.itertuples():
        reg = regs[row.region]
        zc = tz.loc[[m for m in reg.members]].to_numpy()
        zt = vz.loc[[row.key]].to_numpy()
        dists = scoring.pairwise_dist(L, zc, zt)[:, 0]
        j = int(np.argmin(dists))
        out_rows.append({
            "key": row.key, "region": row.region,
            "f2_dist": round(float(dists[j] + row.f1_penalty), 4),
            "f2_best_course": reg.members[j],
            "f1_relaxed": bool(row.f1_relaxed),
            "lat": row.lat, "lon": row.lon, "epsg": row.epsg,
            "x0": row.x0, "y0": row.y0,
        })
    out = pd.DataFrame(out_rows).sort_values("key").reset_index(drop=True)
    out = out.join(vdf, on="key")

    # per-region overshoot keep + global cap
    keep_keys = []
    for rname, sub in out.groupby("region"):
        n_keep = int(np.ceil(f2["overshoot_factor"] * regs[rname].quota))
        keep_keys += list(sub.sort_values(["f2_dist", "key"])
                          .head(n_keep)["key"])
    out["f2_keep"] = out["key"].isin(keep_keys)
    kept = out[out["f2_keep"]].sort_values(["f2_dist", "key"])
    if len(kept) > f2["global_finalist_cap"]:
        drop = set(kept["key"].tail(len(kept) - f2["global_finalist_cap"]))
        out.loc[out["key"].isin(drop), "f2_keep"] = False

    out_p = sconfig.out_path("f2_scores.parquet")
    if only_region and os.path.exists(out_p):
        prev = pd.read_parquet(out_p)
        prev = prev[prev["region"] != only_region]
        out = pd.concat([prev, out], ignore_index=True) \
            .sort_values("key").reset_index(drop=True)
    out.to_parquet(out_p)
    print(f"wrote {out_p}: {int(out['f2_keep'].sum())} finalists "
          f"of {len(out)} scored")
    return 0


def drift_check(cfg: dict, n_courses: int = 5) -> int:
    """Screening-validity check: fetch REAL 10 m DEMs over course windows and
    compare the fetched-10m S1 vector against the store-derived targets —
    quantifies fetch-vs-derived comparability before any tile is trusted."""
    import requests
    from dtm_atlas import config as atlas_config
    from dtm_atlas import dep3, grids
    atlas_config.select_dataset("courses")
    targets = course_targets(cfg)
    # spread across the relief range for a representative check
    order = targets.sort_values("s1_relief_p95_p5").index
    picks = [order[i] for i in
             np.linspace(0, len(order) - 1, n_courses).astype(int)]
    res = cfg["f2"]["res_m"]
    side = cfg["tile"]["side_m"]
    sess = requests.Session()
    rows = {}
    for key in picks:
        import json
        meta = json.load(open(os.path.join(atlas_config.STORE, key,
                                           "meta.json")))
        x0, y0, x1, y1 = meta["window_utm"]
        cx, cy = (x0 + x1) / 2, (y0 + y1) / 2
        win = (cx - side / 2, cy - side / 2, cx + side / 2, cy + side / 2)
        epsg = int(str(meta["crs"]).split(":")[1])
        path = sconfig.out_path("dem10", f"drift_{key}.tif")
        if os.path.exists(path):
            arr, _tf, _e = grids.read_gtiff(path)
        else:
            arr = dep3.export_dem(win, epsg, res, sess)
            grids.write_gtiff(path, arr, grids.work_transform(win, res), epsg,
                              nodata=atlas_config.NODATA)
        z = np.where(arr == atlas_config.NODATA, np.nan, arr.astype(float))
        rows[key] = s1_vector(z, res)
    fetched = pd.DataFrame(rows).T[S1_COLS]
    fz = _zframe(cfg, fetched)
    tzz = _zframe(cfg, targets.loc[picks])
    dz = (fz - tzz).abs()
    lines = [f"fetched-10m vs store-derived-10m |dz| per metric "
             f"(n={n_courses} courses: {list(picks)}):"]
    for c in S1_COLS:
        lines.append(f"  {c:32s} median {dz[c].median():.3f}  "
                     f"max {dz[c].max():.3f}")
    lines.append(f"overall median |dz| = {dz.stack().median():.3f} "
                 f"(z-units of the course frame)")
    report = "\n".join(lines)
    print(report)
    with open(sconfig.out_path("reports", "drift_10m.txt"), "w") as f:
        f.write(report + "\n")
    return 0
