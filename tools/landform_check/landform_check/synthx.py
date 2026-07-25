"""Synthetic-grid extraction worker — the SAME instruments extract.py runs
on real tiles (valleys via D8, geomorphon ridges, bluffs, bowls, tile-level
network stats), pointed at a rendered HG01 grid. No store, no NHD, no
masks: all-valid, water/inpaint empty — exactly the D8-fallback path the
real campaign uses where NHD is absent. Row schemas match params_tiles /
networks_tiles / branches_tiles so the comparison code shares columns."""

from __future__ import annotations

import json
import os
import time

import numpy as np


# the REAL campaign's junction-angle measurement — imported, not mirrored,
# so the two populations are measured by the same code path
def _junction_angle(child_cl: np.ndarray, parent_cl: np.ndarray) -> float:
    from dtm_primitives.extract import _junction_angle as real
    return real(child_cl, parent_cl)


def worker(item: dict) -> dict:
    """item = {key, hg01, out_json}; returns a small status dict."""
    import warnings
    warnings.filterwarnings("ignore")
    t0 = time.time()
    from dtm_metrics.sandbox.campaign import read_hg01
    from dtm_primitives import bridge
    from dtm_primitives import geomorphons as G
    from dtm_primitives.blufffit import detect_and_fit as fit_bluffs
    from dtm_primitives.bowlfit import detect_and_fit as fit_bowls
    from dtm_primitives.meanderfit import decompose
    from dtm_primitives.ridgepipe import extract_ridges, spacing_stats
    from dtm_primitives.transects import arc_length
    from dtm_primitives.valleypipe import extract_valleys

    key = item["key"]
    cfg = bridge.load_config()
    h, cell = read_hg01(item["hg01"])
    nat = h[::-1].astype(float)  # TIF orientation (row 0 = north)
    valid = np.isfinite(nat)
    masks = {"valid": valid.astype(np.uint8),
             "inpaint": np.zeros(nat.shape, np.uint8),
             "water": np.zeros(nat.shape, np.uint8),
             "road": np.zeros(nat.shape, np.uint8)}

    # geomorphons at exactly 10 m (k10 x cell = 10)
    k10 = max(1, int(round(10.0 / cell)))
    ny, nx = nat.shape[0] // k10, nat.shape[1] // k10
    z10 = np.nanmean(nat[: ny * k10, : nx * k10]
                     .reshape(ny, k10, nx, k10), axis=(1, 3))
    cls10 = G.classify(z10, 10.0, lookup_m=cfg["ridgefit"]["lookup_m"],
                       flat_deg=cfg["ridgefit"]["flat_deg"])
    geo_fracs = G.class_fractions(cls10, collar_px=25)

    # valleys: D8 (no NHD on synthetic terrain — same fallback path)
    results, used_net = extract_valleys(nat, valid, masks, cell, [], cfg)
    valley_rows = []
    branch_meta = []
    for i, r in enumerate(results):
        mf = decompose(r.centerline, r.top_width_m, cfg) \
            if r.valley is not None else None
        for cf, rc in zip(r.crossfits, r.reaches):
            if not np.isfinite(cf.hw):
                continue
            valley_rows.append({
                "kind": "valley", "tile": key, "branch": i,
                "s_mid": rc.s_mid, "ok": bool(cf.ok), "reason": cf.reason,
                "incision": cf.inc, "hw": cf.hw, "wall_l": cf.ml,
                "wall_r": cf.mr, "floor_round": cf.r, "rms": cf.rms,
                "n_used": rc.n_used, "area_km2": float("nan"), "order": 0,
                "slope_vaa": float("nan"), "source": "d8",
            })
        jang = float("nan")
        if r.branch.parent is not None and r.branch.parent < len(results):
            jang = _junction_angle(r.centerline,
                                   results[r.branch.parent].centerline)
        branch_meta.append({
            "tile": key, "branch": i, "parent": r.branch.parent,
            "len_m": arc_length(r.centerline),
            "fall_raw": r.longfit.fall_raw if r.longfit else float("nan"),
            "area_mouth_km2": float("nan"),
            "junction_angle_deg": jang,
            "sinuosity": mf.sinuosity if mf else float("nan"),
            "meander_lambda_m": mf.wavelength_m if mf else float("nan"),
            "meander_amp_m": mf.amplitude_m if mf else float("nan"),
            "meander_intensity": mf.intensity if mf else float("nan"),
            "wavelength_mult": mf.wavelength_mult if mf else float("nan"),
            "meander_basis": mf.basis if mf else "",
            "top_width_m": r.top_width_m,
            "artificial_frac": 0.0,
        })

    ridges, _rres = extract_ridges(nat, valid, cell, cfg, cls10=cls10)
    ridge_rows = [{
        "kind": "ridge", "tile": key, "branch": j, "ok": True,
        "incision": rr.prominence_med, "hw": rr.crest_hw_med,
        "wall_l": rr.flank_l, "wall_r": rr.flank_r,
        "floor_round": rr.crest_round, "len_m": rr.length_m,
        "source": "geomorphon",
    } for j, rr in enumerate(ridges)]

    corridors = [(r.centerline, r.top_width_m) for r in results
                 if r.valley is not None]
    bluffs = fit_bluffs(nat, valid, cell, cfg, valley_corridors=corridors,
                        cliff_lines=[])
    bluff_rows = [{
        "kind": "bluff", "tile": key, "branch": j, "ok": True,
        "incision": bl.height_m, "hw": float("nan"), "wall_l": bl.face_grad,
        "wall_r": float("nan"), "floor_round": float("nan"),
        "len_m": bl.length_m, "source": "step",
    } for j, bl in enumerate(bluffs)]

    bowls = fit_bowls(nat, valid, cell, cfg, water=masks["water"],
                      inpaint=masks["inpaint"], valley_corridors=corridors)
    bowl_rows = [{
        "kind": "bowl", "tile": key, "branch": j, "ok": True,
        "incision": bw.depth_m, "hw": bw.radius_m, "wall_l": bw.inner_grad,
        "wall_r": float("nan"), "floor_round": bw.rim_round_m,
        "source": "lake" if bw.is_lake else "spillway",
        "wobble": bw.wobble, "cycles": bw.cycles, "len_m": float("nan"),
    } for j, bw in enumerate(bowls)]

    net_len = sum(arc_length(b.path_xy) for b in used_net.branches) \
        if used_net else 0.0
    jangs = [b["junction_angle_deg"] for b in branch_meta
             if np.isfinite(b["junction_angle_deg"])]
    sp = spacing_stats(ridges)
    ys, xs = np.nonzero(valid)
    step = max(1, ys.size // 100_000)
    ys, xs = ys[::step], xs[::step]
    A = np.stack([np.ones(ys.size), xs * cell, ys * cell], 1)
    zv = nat[ys, xs]
    coef, *_ = np.linalg.lstsq(A, zv, rcond=None)
    for _ in range(3):
        res = zv - A @ coef
        mad = np.median(np.abs(res)) * 1.4826
        w = np.clip(1.345 * max(mad, 1e-6) / np.maximum(np.abs(res), 1e-9),
                    None, 1.0)
        coef, *_ = np.linalg.lstsq(A * w[:, None], zv * w, rcond=None)
    tile_row = {
        "tile": key,
        "n_branches": len(results),
        "n_ok_reaches": sum(1 for r_ in valley_rows if r_["ok"]),
        "valley_source": "d8",
        "drainage_density_km": net_len / 1000.0 / 9.0,
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
        "elapsed_s": round(time.time() - t0, 1),
    }

    with open(item["out_json"], "w") as f:
        json.dump({"tile": tile_row, "branches": branch_meta,
                   "reaches": valley_rows + ridge_rows + bluff_rows
                   + bowl_rows}, f, sort_keys=True)
    return {"key": key, "ok": True, "elapsed_s": tile_row["elapsed_s"]}


def safe_worker(item: dict) -> dict:
    try:
        return worker(item)
    except Exception as e:  # recorded, never fatal to the fleet
        import traceback
        return {"key": item["key"], "ok": False,
                "err": f"{type(e).__name__}: {e}",
                "tb": traceback.format_exc(limit=4)}
