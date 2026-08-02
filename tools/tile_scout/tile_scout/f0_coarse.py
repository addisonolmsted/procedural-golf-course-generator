"""F0 — regional 60 m DEM screen. One export per region; per-candidate coarse
stats on 50x50-px slices; rank against the region's MEMBER course targets
computed by the SAME stats function from the stored 2 m rasters (same
estimator space — no cross-resolution calibration guesswork)."""

from __future__ import annotations

import os

import numpy as np
import pandas as pd

from . import sconfig
from .regions import Region

F0_STATS = ["f0_relief", "f0_slope_p50", "f0_slope_p90", "f0_bp_800p",
            "f0_bp_200_800", "f0_flat_frac"]
ENVELOPE_STATS = ["f0_relief", "f0_slope_p50", "f0_slope_p90"]


def f0_stats(z: np.ndarray, cell: float, flat_slope: float) -> dict:
    """Coarse character stats on one tile-sized patch (NaN = invalid)."""
    fin = np.isfinite(z)
    out = {k: np.nan for k in F0_STATS}
    out["f0_nodata_frac"] = float(1.0 - fin.mean())
    if fin.mean() < 0.5:
        return out
    v = z[fin]
    out["f0_relief"] = float(np.percentile(v, 95) - np.percentile(v, 5))
    zf = np.where(fin, z, float(np.median(v)))
    gy, gx = np.gradient(zf, cell)
    g = np.hypot(gx, gy)[fin]
    out["f0_slope_p50"] = float(np.percentile(g, 50))
    out["f0_slope_p90"] = float(np.percentile(g, 90))
    out["f0_flat_frac"] = float((g < flat_slope).mean())
    # plane-detrended Hann periodogram, annulus band variances
    h, w = zf.shape
    yy, xx = np.mgrid[0:h, 0:w]
    A = np.stack([np.ones(zf.size), xx.ravel(), yy.ravel()], 1)
    coef, *_ = np.linalg.lstsq(A, zf.ravel(), rcond=None)
    det = (zf.ravel() - A @ coef).reshape(h, w)
    win = np.outer(np.hanning(h), np.hanning(w))
    W2 = (win ** 2).sum()
    F = np.fft.rfft2(det * win)
    P = (np.abs(F) ** 2) / W2                      # Welch-normalized power
    fy = np.fft.fftfreq(h, d=cell)[:, None]
    fx = np.fft.rfftfreq(w, d=cell)[None, :]
    fr = np.hypot(fy, fx)
    def band(lo_m, hi_m):
        sel = (fr > 1.0 / hi_m) & (fr <= 1.0 / max(lo_m, 2 * cell))
        return float(np.sqrt(P[sel].sum() / (h * w))) if sel.any() else 0.0
    out["f0_bp_800p"] = band(800.0, 1e9)
    out["f0_bp_200_800"] = band(200.0, 800.0)
    return out


# --- course targets (same estimator, from the frozen 2 m store) -------------

def course_targets(cfg: dict, force: bool = False) -> pd.DataFrame:
    out_p = sconfig.out_path("f0_course_targets.parquet")
    if os.path.exists(out_p) and not force:
        return pd.read_parquet(out_p)
    from dtm_atlas import config as atlas_config
    from dtm_atlas import grids
    atlas_config.select_dataset("courses")
    side = cfg["tile"]["side_m"]
    res = cfg["f0"]["res_m"]
    k = int(round(res / 2.0))              # 2 m store -> 60 m blocks
    n50 = int(side / res)
    rows = {}
    import pandas as pd_
    keys = list(pd_.read_parquet(
        sconfig.repo_path(cfg["paths"]["course_metrics_parquet"])).index)
    for key in keys:
        p = os.path.join(atlas_config.STORE, key, "naturalized.tif")
        arr, _tf, _e = grids.read_gtiff(p)
        z = np.where(arr == atlas_config.NODATA, np.nan, arr.astype(float))
        h, w = z.shape
        hb, wb = h // k, w // k
        zb = np.nanmean(
            z[:hb * k, :wb * k].reshape(hb, k, wb, k), axis=(1, 3))
        r0 = max((hb - n50) // 2, 0)
        c0 = max((wb - n50) // 2, 0)
        crop = zb[r0:r0 + n50, c0:c0 + n50]
        rows[key] = f0_stats(crop, res, cfg["f0"]["flat_slope"])
    df = pd.DataFrame(rows).T.sort_index()
    df.to_parquet(out_p)
    return df


# --- regional sweep ----------------------------------------------------------

def region_dem(region: Region, cfg: dict, sess) -> tuple[np.ndarray, tuple]:
    """The region's 60 m DEM (cached GeoTIFF) + its UTM window."""
    from dtm_atlas import config as atlas_config
    from dtm_atlas import dep3, grids
    res = cfg["f0"]["res_m"]
    rad = cfg["regions"]["search_radius_km"] * 1000.0 \
        + cfg["tile"]["side_m"]
    la, lo = region.center_ll
    xs, ys = grids.ll_to_utm([lo], [la], region.epsg)
    xc, yc = float(np.asarray(xs)[0]), float(np.asarray(ys)[0])
    window = grids.snap_window(xc - rad, yc - rad, xc + rad, yc + rad,
                               margin_m=0.0, snap=res)
    path = sconfig.out_path("dem60", f"{region.name}.tif")
    if os.path.exists(path):
        arr, _tf, _e = grids.read_gtiff(path)
    else:
        arr = dep3.export_dem(window, region.epsg, res, sess)
        tf = grids.work_transform(window, res)
        grids.write_gtiff(path, arr, tf, region.epsg,
                          nodata=atlas_config.NODATA)
    from dtm_atlas import config as ac
    z = np.where(arr == ac.NODATA, np.nan, arr.astype(float))
    return z, window


def run(cfg: dict, only_region: str = "", force: bool = False) -> int:
    import requests
    from dtm_atlas import grids
    from . import lattice as L
    from . import regions as R
    regs = R.load(sconfig.out_path("regions.json"))
    if only_region:
        regs = [r for r in regs if r.name == only_region]
        if not regs:
            raise SystemExit(f"unknown region {only_region}")
    targets = course_targets(cfg, force=False)
    med = targets.median()
    mad = ((targets - med).abs().median() * 1.4826).where(
        lambda s: s > 1e-9, targets.std().fillna(1.0)).fillna(1.0)

    side = cfg["tile"]["side_m"]
    res = cfg["f0"]["res_m"]
    n50 = int(side / res)
    sess = requests.Session()
    frames = []
    for reg in regs:
        z, window = region_dem(reg, cfg, sess)
        rx0, ry0, rx1, ry1 = window
        cands = L.candidates(reg, cfg)
        stats = []
        for row in cands.itertuples():
            c0 = int(round((row.x0 - rx0) / res))
            r0 = int(round((ry1 - (row.y0 + side)) / res))
            if r0 < 0 or c0 < 0 or r0 + n50 > z.shape[0] \
                    or c0 + n50 > z.shape[1]:
                stats.append({k: np.nan for k in F0_STATS}
                             | {"f0_nodata_frac": 1.0})
                continue
            stats.append(f0_stats(z[r0:r0 + n50, c0:c0 + n50], res,
                                  cfg["f0"]["flat_slope"]))
        df = pd.concat([cands.reset_index(drop=True),
                        pd.DataFrame(stats)], axis=1)
        # ll bbox of the exact tile square (for F1 queries)
        lon0, lat0 = grids.utm_to_ll(list(df["x0"]), list(df["y0"]), reg.epsg)
        lon1, lat1 = grids.utm_to_ll(list(df["x0"] + side),
                                     list(df["y0"] + side), reg.epsg)
        df["bb_s"] = np.round(np.minimum(lat0, lat1), 6)
        df["bb_n"] = np.round(np.maximum(lat0, lat1), 6)
        df["bb_w"] = np.round(np.minimum(lon0, lon1), 6)
        df["bb_e"] = np.round(np.maximum(lon0, lon1), 6)

        # distance to the nearest MEMBER course in pooled robust-z space
        tz = (targets.loc[[m for m in reg.members]] - med) / mad
        cz = (df[F0_STATS] - med[F0_STATS]) / mad[F0_STATS]
        d = np.full(len(df), np.inf)
        best = np.array([""] * len(df), dtype=object)
        for m in reg.members:
            dm = np.sqrt(((cz - tz.loc[m, F0_STATS]) ** 2).sum(axis=1))
            take = dm.to_numpy() < d
            d[take] = dm.to_numpy()[take]
            best[take] = m
        df["f0_dist"] = np.round(d, 4)
        df["f0_best_course"] = best

        f0c = cfg["f0"]
        gate = (df["f0_nodata_frac"] <= f0c["max_nodata_frac"]) \
            & (df["f0_flat_frac"] <= f0c["max_flat_frac"])
        for s in ENVELOPE_STATS:
            lo_v = targets.loc[reg.members, s].min() - f0c["z_window_sd"] * mad[s]
            hi_v = targets.loc[reg.members, s].max() + f0c["z_window_sd"] * mad[s]
            gate &= df[s].between(lo_v, hi_v)
        df["f0_pass"] = gate & np.isfinite(df["f0_dist"])
        keep_n = f0c["keep_factor"] * reg.quota
        ranked = df[df["f0_pass"]].sort_values(["f0_dist", "key"])
        df["f0_keep"] = df["key"].isin(ranked["key"].head(keep_n))
        frames.append(df)
        print(f"{reg.name}: {len(df)} candidates, "
              f"{int(df['f0_pass'].sum())} pass, keep {int(df['f0_keep'].sum())}")

    out = L.dedup(frames, regs, cfg) if len(frames) > 1 else frames[0]
    out = out.sort_values("key").reset_index(drop=True)
    out_p = sconfig.out_path("f0_candidates.parquet")
    if only_region and os.path.exists(out_p):
        prev = pd.read_parquet(out_p)
        prev = prev[prev["region"] != only_region]
        out = pd.concat([prev, out], ignore_index=True) \
            .sort_values("key").reset_index(drop=True)
    out.to_parquet(out_p)
    print(f"wrote {out_p} ({int(out['f0_keep'].sum())} kept "
          f"of {len(out)})")
    return 0
