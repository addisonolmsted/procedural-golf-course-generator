"""Stage-3T extraction campaign: every landform instrument over the tile
store -> per-tile records + the campaign parquets the prior fit consumes.

Per tile: NHD-truth valleys (D8 fallback, flagged) + VAA joins + meanders +
junction angles; geomorphon-confirmed ridges; bluffs (OSM-cliff seeded);
bowls; network stats; tilt. Outputs:
  out/extract/{key}.json            per-tile record (scalars + diagnostics)
  out/params_tiles.parquet          reach-level rows (kind column)
  out/networks_tiles.parquet        tile-level rows
ProcessPool spawn-safe (module-level worker, __main__ guard in CLI)."""

from __future__ import annotations

import json
import os
import time
import traceback

import numpy as np

from . import bridge

MAX_BRANCHES = 12          # bound per-tile cost: top branches by mouth area
MIN_BRANCH_LEN_M = 300.0


def _junction_angle(child_cl: np.ndarray, parent_cl: np.ndarray) -> float:
    """Angle (deg) between the tributary's approach (last 100 m tangent) and
    the parent's downstream tangent at the junction."""
    from .profiles import polyline_project
    if len(child_cl) < 3 or len(parent_cl) < 3:
        return float("nan")
    tail = child_cl[-min(21, len(child_cl)):]
    v1 = tail[-1] - tail[0]
    n1 = np.hypot(*v1)
    mouth = child_cl[-1]
    u, _d, _s, total = polyline_project(parent_cl, [mouth[0]], [mouth[1]])
    s_hit = float(u[0]) * total
    seg = np.hypot(*np.diff(parent_cl, axis=0).T)
    cum = np.concatenate([[0.0], np.cumsum(seg)])
    i0 = int(np.searchsorted(cum, s_hit))
    i1 = min(i0 + 20, len(parent_cl) - 1)
    if i1 <= i0:
        return float("nan")
    v2 = parent_cl[i1] - parent_cl[max(i0 - 1, 0)]
    n2 = np.hypot(*v2)
    if n1 < 1e-9 or n2 < 1e-9:
        return float("nan")
    cosang = float(np.clip(np.dot(v1, v2) / (n1 * n2), -1.0, 1.0))
    return float(np.degrees(np.arccos(cosang)))


def worker(key: str) -> dict:
    import warnings
    warnings.filterwarnings("ignore")
    t0 = time.time()
    from dtm_atlas import config as atlas_config
    from dtm_atlas import grids, osm as atlas_osm
    atlas_config.select_dataset("tiles")
    from . import geomorphons as G
    from . import nhdnet
    from .blufffit import detect_and_fit as fit_bluffs
    from .bowlfit import detect_and_fit as fit_bowls
    from .meanderfit import decompose
    from .ridgepipe import extract_ridges, spacing_stats
    from .transects import arc_length
    from .valleypipe import extract_valleys

    cfg = bridge.load_config()
    sdir = os.path.join(atlas_config.STORE, key)
    meta = json.load(open(os.path.join(sdir, "meta.json")))
    cell = 2.0
    arr, _tf, _e = grids.read_gtiff(os.path.join(sdir, "naturalized.tif"))
    nat = np.where(arr == atlas_config.NODATA, np.nan, arr.astype(float))
    valid = np.isfinite(nat)

    def msk(name):
        p = os.path.join(sdir, "masks", f"{name}.tif")
        if not os.path.exists(p):
            return np.zeros(nat.shape, np.uint8)
        a, _t, _e2 = grids.read_gtiff(p)
        return a

    masks = {"valid": valid.astype(np.uint8), "inpaint": msk("inpaint"),
             "water": msk("water"), "road": msk("road")}

    # geomorphons at 10 m (shared by ridges + recorded fractions)
    k10 = 5
    ny, nx = nat.shape[0] // k10, nat.shape[1] // k10
    import warnings as _w
    with _w.catch_warnings():
        _w.simplefilter("ignore", RuntimeWarning)
        z10 = np.nanmean(nat[: ny * k10, : nx * k10]
                         .reshape(ny, k10, nx, k10), axis=(1, 3))
    cls10 = G.classify(z10, 10.0, lookup_m=cfg["ridgefit"]["lookup_m"],
                       flat_deg=cfg["ridgefit"]["flat_deg"])
    geo_fracs = G.class_fractions(cls10, collar_px=25)

    # ---- valleys: NHD truth, D8 fallback ----
    net, vaa = nhdnet.load_network(key, meta)
    source = "nhd"
    if net is None or not net.branches:
        net, vaa, source = None, {}, "d8"
    else:
        # bound cost: top branches by mouth area, min length
        keep_idx = [i for i, b in enumerate(net.branches)
                    if arc_length(b.path_xy) >= MIN_BRANCH_LEN_M]
        keep_idx.sort(key=lambda i: -net.branches[i].mouth_area_m2)
        keep_idx = sorted(keep_idx[:MAX_BRANCHES])
        remap = {old: new for new, old in enumerate(keep_idx)}
        newb = []
        for old in keep_idx:
            b = net.branches[old]
            b.parent = remap.get(b.parent) if b.parent in remap else None
            newb.append(b)
        net.branches = newb
        vaa = {remap[o]: v for o, v in vaa.items() if o in remap}

    results, used_net = extract_valleys(nat, valid, masks, cell, [], cfg,
                                        network=net)
    valley_rows = []
    branch_meta = []
    for i, r in enumerate(results):
        v = vaa.get(i)
        mf = decompose(r.centerline, r.top_width_m, cfg) \
            if r.valley is not None else None
        for cf, rc in zip(r.crossfits, r.reaches):
            if not np.isfinite(cf.hw):
                continue
            a_mid = nhdnet.area_at_arc(v, rc.s_mid) if v else float("nan")
            valley_rows.append({
                "kind": "valley", "tile": key, "branch": i,
                "s_mid": rc.s_mid, "ok": bool(cf.ok), "reason": cf.reason,
                "incision": cf.inc, "hw": cf.hw, "wall_l": cf.ml,
                "wall_r": cf.mr, "floor_round": cf.r, "rms": cf.rms,
                "n_used": rc.n_used, "area_km2": a_mid,
                "order": v.stream_order if v else 0,
                "slope_vaa": v.slope_vaa if v else float("nan"),
                "source": source,
            })
        jang = float("nan")
        if r.branch.parent is not None and r.branch.parent < len(results):
            jang = _junction_angle(r.centerline,
                                   results[r.branch.parent].centerline)
        branch_meta.append({
            "branch": i, "parent": r.branch.parent,
            "len_m": arc_length(r.centerline),
            "fall_raw": r.longfit.fall_raw if r.longfit else float("nan"),
            "area_mouth_km2": v.area_mouth_km2 if v else float("nan"),
            "junction_angle_deg": jang,
            "sinuosity": mf.sinuosity if mf else float("nan"),
            "meander_lambda_m": mf.wavelength_m if mf else float("nan"),
            "meander_amp_m": mf.amplitude_m if mf else float("nan"),
            "meander_intensity": mf.intensity if mf else float("nan"),
            "wavelength_mult": mf.wavelength_mult if mf else float("nan"),
            "meander_basis": mf.basis if mf else "",
            "top_width_m": r.top_width_m,
            "artificial_frac": v.artificial_frac if v else 0.0,
        })

    # ---- ridges ----
    ridges, _rres = extract_ridges(nat, valid, cell, cfg, cls10=cls10)
    ridge_rows = [{
        "kind": "ridge", "tile": key, "branch": j, "s_mid": float("nan"),
        "ok": True, "reason": "ok", "incision": rr.prominence_med,
        "hw": rr.crest_hw_med, "wall_l": rr.flank_l, "wall_r": rr.flank_r,
        "floor_round": rr.crest_round, "rms": float("nan"),
        "n_used": len(rr.reaches), "area_km2": float("nan"), "order": 0,
        "slope_vaa": float("nan"), "source": "geomorphon",
        "len_m": rr.length_m, "crest_z": rr.crest_z0,
        "geomorphon_frac": rr.geomorphon_frac,
    } for j, rr in enumerate(ridges)]

    # ---- bluffs (cliff seeds from the NHD sidecar fetch) ----
    cliffs = []
    cliff_p = os.path.join(sdir, "nhd", "features_cliff.json")
    if os.path.exists(cliff_p):
        try:
            resp = json.load(open(cliff_p))
            x0, y0 = meta["window_utm"][0], meta["window_utm"][1]
            epsg = int(str(meta["crs"]).split(":")[1])
            for el in resp.get("elements", []):
                geom = el.get("geometry") or []
                if len(geom) >= 2:
                    lons = [g["lon"] for g in geom]
                    lats = [g["lat"] for g in geom]
                    xs, ys = grids.ll_to_utm(lons, lats, epsg)
                    cliffs.append(np.stack([np.asarray(xs) - x0,
                                            np.asarray(ys) - y0], 1))
        except Exception:
            pass
    corridors = [(r.centerline, r.top_width_m) for r in results
                 if r.valley is not None]
    bluffs = fit_bluffs(nat, valid, cell, cfg, valley_corridors=corridors,
                        cliff_lines=cliffs)
    bluff_rows = [{
        "kind": "bluff", "tile": key, "branch": j, "s_mid": float("nan"),
        "ok": True, "reason": "ok", "incision": bl.height_m,
        "hw": float("nan"), "wall_l": bl.face_grad, "wall_r": float("nan"),
        "floor_round": float("nan"), "rms": bl.rms, "n_used": 0,
        "area_km2": float("nan"), "order": 0, "slope_vaa": float("nan"),
        "source": "step+cliff" if bl.osm_cliff_near else "step",
        "len_m": bl.length_m,
    } for j, bl in enumerate(bluffs)]

    # ---- bowls ----
    bowls = fit_bowls(nat, valid, cell, cfg, water=masks["water"],
                      inpaint=masks["inpaint"], valley_corridors=corridors)
    bowl_rows = [{
        "kind": "bowl", "tile": key, "branch": j, "s_mid": float("nan"),
        "ok": True, "reason": "ok", "incision": bw.depth_m,
        "hw": bw.radius_m, "wall_l": bw.inner_grad, "wall_r": float("nan"),
        "floor_round": bw.rim_round_m, "rms": bw.rms, "n_used": 0,
        "area_km2": float("nan"), "order": 0, "slope_vaa": float("nan"),
        "source": "lake" if bw.is_lake else "spillway",
        "wobble": bw.wobble, "cycles": bw.cycles,
    } for j, bw in enumerate(bowls)]

    # ---- tile-level network stats ----
    nhd_len = sum(arc_length(b.path_xy) for b in used_net.branches) \
        if used_net else 0.0
    jangs = [b["junction_angle_deg"] for b in branch_meta
             if np.isfinite(b["junction_angle_deg"])]
    sp = spacing_stats(ridges)
    # robust tilt (carved zones excluded implicitly by robustness)
    ys, xs = np.nonzero(valid)
    step = max(1, ys.size // 100_000)
    ys, xs = ys[::step], xs[::step]
    A = np.stack([np.ones(ys.size), xs * cell, ys * cell], 1)
    zv = nat[ys, xs]
    coef, *_ = np.linalg.lstsq(A, zv, rcond=None)
    for _ in range(3):     # IRLS Huber
        res = zv - A @ coef
        mad = np.median(np.abs(res)) * 1.4826
        w = np.clip(1.345 * max(mad, 1e-6) / np.maximum(np.abs(res), 1e-9),
                    None, 1.0)
        coef, *_ = np.linalg.lstsq(A * w[:, None], zv * w, rcond=None)
    tile_row = {
        "tile": key,
        "n_branches": len(results),
        "n_ok_reaches": sum(1 for r_ in valley_rows if r_["ok"]),
        "valley_source": source,
        "drainage_density_km": nhd_len / 1000.0 / 9.0,
        "junction_angle_med": float(np.median(jangs)) if jangs
        else float("nan"),
        "n_junctions": len(jangs),
        "n_ridges": len(ridges),
        "ridge_spacing_m": sp["ridge_spacing_m"],
        "n_bluffs": len(bluffs),
        "n_bowls": len(bowls),
        "n_lakes": sum(1 for b in bowls if b.is_lake),
        "tilt_grade": float(np.hypot(coef[1], coef[2])),
        "geo_ridge_frac": geo_fracs.get("ridge", 0.0)
        + geo_fracs.get("spur", 0.0),
        "geo_valley_frac": geo_fracs.get("valley", 0.0)
        + geo_fracs.get("hollow", 0.0),
        "geo_flat_frac": geo_fracs.get("flat", 0.0),
        "inpaint_frac": meta["inpaint"]["frac"],
        "elapsed_s": round(time.time() - t0, 1),
    }

    out_dir = os.path.join(bridge.OUT, "extract")
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, f"{key}.json"), "w") as f:
        json.dump({"tile": tile_row, "branches": branch_meta,
                   "reaches": valley_rows + ridge_rows + bluff_rows
                   + bowl_rows}, f, sort_keys=True)
    return {"key": key, "ok": True, **{k_: tile_row[k_] for k_ in
            ("n_ok_reaches", "n_ridges", "n_bluffs", "n_bowls",
             "elapsed_s")}}


def _safe_worker(key: str) -> dict:
    try:
        return worker(key)
    except Exception:
        return {"key": key, "ok": False, "err": traceback.format_exc()[-600:]}


def run(keys: list[str] | None, workers: int) -> int:
    from concurrent.futures import ProcessPoolExecutor
    from dtm_atlas import config as atlas_config
    atlas_config.select_dataset("tiles")
    from dtm_atlas import tiles as atlas_tiles
    keys = keys or atlas_tiles.read_list()
    out_dir = os.path.join(bridge.OUT, "extract")
    os.makedirs(out_dir, exist_ok=True)
    todo = [k for k in keys
            if not os.path.exists(os.path.join(out_dir, f"{k}.json"))]
    print(f"extract: {len(keys)} tiles, {len(todo)} to run")
    fails = 0
    with ProcessPoolExecutor(max_workers=workers) as ex:
        for i, res in enumerate(ex.map(_safe_worker, todo, chunksize=1)):
            if res["ok"]:
                print(f"[{i + 1}/{len(todo)}] {res['key']}: "
                      f"reaches {res['n_ok_reaches']} ridges "
                      f"{res['n_ridges']} bluffs {res['n_bluffs']} bowls "
                      f"{res['n_bowls']} ({res['elapsed_s']}s)")
            else:
                fails += 1
                print(f"[{i + 1}/{len(todo)}] {res['key']}: FAIL\n"
                      f"{res['err']}")

    # assemble parquets from ALL per-tile jsons
    import pandas as pd
    reach_rows = []
    tile_rows = []
    branch_rows = []
    for k in keys:
        p = os.path.join(out_dir, f"{k}.json")
        if not os.path.exists(p):
            continue
        d = json.load(open(p))
        tile_rows.append(d["tile"])
        reach_rows += d["reaches"]
        for b in d["branches"]:
            branch_rows.append({"tile": k, **b})
    pd.DataFrame(reach_rows).to_parquet(
        os.path.join(bridge.OUT, "params_tiles.parquet"))
    pd.DataFrame(tile_rows).to_parquet(
        os.path.join(bridge.OUT, "networks_tiles.parquet"))
    pd.DataFrame(branch_rows).to_parquet(
        os.path.join(bridge.OUT, "branches_tiles.parquet"))
    print(f"extract done: {len(tile_rows)} tiles, {len(reach_rows)} records, "
          f"{fails} failures")
    return 0 if fails == 0 else 1
