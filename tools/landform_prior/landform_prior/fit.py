"""Fit landform_prior.json from the 3T extraction campaign.

Inputs (all produced by earlier stages, no network):
  tools/dtm_primitives/out/params_tiles.parquet    reach/feature records
  tools/dtm_primitives/out/networks_tiles.parquet  tile-level stats
  tools/dtm_primitives/out/branches_tiles.parquet  branch-level VAA+meander
  tools/tile_scout/out/archetypes.json             course GMM responsibilities
  tools/dtm_atlas/tiles_us.json                    tile -> best_course
  tools/dtm_metrics/out/sandbox/stage4_recommended_params.json  KEEP-7 bounds

Representation: 11-point weighted quantile tables (inverse-CDF sampling in
Rust), log-log hydraulic regressions on drainage area with residual deciles,
a Gaussian copula (Cholesky) over the reach residual vector, and shrinkage
lambda = n_eff/(n_eff+25) toward the pooled population per TABLE (n_eff =
Kish effective sample size of the responsibility weights actually used)."""

from __future__ import annotations

import json
import os

import numpy as np
import pandas as pd

from . import LANDFORM_PRIOR_VERSION, OUT, REPO

QS = np.linspace(0, 100, 11)
SHRINK_N0 = 25.0
COPULA_FIELDS = ["incision", "floor_halfwidth", "wall_mean", "wall_asym",
                 "floor_round"]
COPULA_MIN_N = 60          # hard-assigned rows needed for an own copula
HARD_RESP = 0.25           # responsibility for "hard" archetype membership
ARTIFICIAL_MAX = 0.5       # exclude branches mostly made of ArtificialPath


def _kish(w: np.ndarray) -> float:
    s = float(w.sum())
    q = float((w * w).sum())
    return s * s / q if q > 0 else 0.0


def _dec(vals, w) -> list[float] | None:
    """Weighted 11-point quantile table (None if no finite support)."""
    v = np.asarray(vals, float)
    w = np.asarray(w, float)
    m = np.isfinite(v) & (w > 0)
    v, w = v[m], w[m]
    if len(v) == 0:
        return None
    order = np.argsort(v, kind="stable")
    v, w = v[order], w[order]
    cw = np.cumsum(w) - 0.5 * w
    cw /= cw[-1] if cw[-1] > 0 else 1.0
    return [round(float(np.interp(q / 100.0, cw, v)), 5) for q in QS]


def _shrink_tab(own: list | None, pooled: list | None,
                n_eff: float) -> list | None:
    if pooled is None:
        return own
    if own is None:
        return pooled
    lam = n_eff / (n_eff + SHRINK_N0)
    return [round(lam * a + (1.0 - lam) * b, 5)
            for a, b in zip(own, pooled)]


def _wls_loglog(x, y, w):
    """Huber-IRLS weighted regression ln y = b0 + b1 ln x.

    Returns (b0, b1, resid_full) with resid_full full-length (NaN outside
    the fitted rows), or None with fewer than 8 usable rows."""
    x = np.asarray(x, float)
    y = np.asarray(y, float)
    w = np.asarray(w, float)
    m = np.isfinite(x) & np.isfinite(y) & (x > 0) & (y > 0) & (w > 0)
    if int(m.sum()) < 8:
        return None
    lx, ly, ww = np.log(x[m]), np.log(y[m]), w[m]
    A = np.stack([np.ones_like(lx), lx], 1)
    coef, *_ = np.linalg.lstsq(A * ww[:, None], ly * ww, rcond=None)
    for _ in range(3):
        r = ly - A @ coef
        mad = np.median(np.abs(r)) * 1.4826
        hub = np.minimum(1.0, 1.345 * max(mad, 1e-6)
                         / np.maximum(np.abs(r), 1e-9))
        wh = ww * hub
        coef, *_ = np.linalg.lstsq(A * wh[:, None], ly * wh, rcond=None)
    resid_full = np.full(len(x), np.nan)
    resid_full[m] = ly - A @ coef
    return float(coef[0]), float(coef[1]), resid_full


def _noise_defaults() -> dict:
    p = os.path.join(REPO, "tools", "dtm_metrics", "out", "sandbox",
                     "stage4_recommended_params.json")
    d = json.load(open(p))
    out = {}
    for name, spec in sorted(d["params"].items()):
        lo, hi = float(spec["lo"]), float(spec["hi"])
        mid = float(np.sqrt(lo * hi)) if spec.get("transform") == "log" \
            else (lo + hi) / 2.0
        out[name] = int(round(mid)) if spec.get("int") else round(mid, 4)
    return out


def load_inputs():
    prim = os.path.join(REPO, "tools", "dtm_primitives", "out")
    params = pd.read_parquet(os.path.join(prim, "params_tiles.parquet"))
    nets = pd.read_parquet(os.path.join(prim, "networks_tiles.parquet")) \
        .sort_values("tile").reset_index(drop=True)
    branches = pd.read_parquet(os.path.join(prim, "branches_tiles.parquet")) \
        .sort_values(["tile", "branch"]).reset_index(drop=True)
    arch = json.load(open(os.path.join(REPO, "tools", "tile_scout", "out",
                                       "archetypes.json")))
    man = json.load(open(os.path.join(REPO, "tools", "dtm_atlas",
                                      "tiles_us.json")))
    k = len(arch["names"])
    resp_by_course = {c: arch["resp"][i] for i, c in enumerate(arch["keys"])}
    uniform = [1.0 / k] * k
    tile_resp = {t: resp_by_course.get(v.get("best_course"), uniform)
                 for t, v in man.items()}
    return params, nets, branches, tile_resp, arch


def fit() -> dict:
    params, nets, branches, tile_resp, arch = load_inputs()
    k_arch = len(arch["names"])
    noise_mid = _noise_defaults()

    # ---- assemble analysis tables (once) --------------------------------
    br_art = branches.set_index(["tile", "branch"])["artificial_frac"]

    val = params[(params.kind == "valley") & params.ok
                 & (params.source == "nhd")].copy()
    val = val.join(br_art, on=["tile", "branch"])
    val = val[val["artificial_frac"].fillna(0.0) <= ARTIFICIAL_MAX]
    val["wall_mean"] = (val.wall_l + val.wall_r) / 2.0
    val["wall_asym"] = np.log(np.maximum(val.wall_l.to_numpy(float), 1e-3)
                              / np.maximum(val.wall_r.to_numpy(float), 1e-3))
    val = val.sort_values(["tile", "branch", "s_mid"]).reset_index(drop=True)

    natural = branches[branches.artificial_frac.fillna(0.0)
                       <= ARTIFICIAL_MAX].reset_index(drop=True)
    meand = natural[np.isfinite(natural.wavelength_mult)]

    # junction area log-ratio child/parent + depth-2 flag
    kids = natural.dropna(subset=["parent"]).copy()
    kids["parent"] = kids["parent"].astype(int)
    kids = kids.merge(
        branches[["tile", "branch", "area_mouth_km2", "parent"]].rename(
            columns={"branch": "parent", "area_mouth_km2": "parent_A",
                     "parent": "grandparent"}),
        on=["tile", "parent"], how="left")
    kids["area_logratio"] = np.log10(
        np.maximum(kids.area_mouth_km2.to_numpy(float), 1e-4)
        / np.maximum(kids.parent_A.to_numpy(float), 1e-4))
    depth2 = kids["grandparent"].notna().to_numpy()

    # per-tile derived: junction spacing + trunk mouth logA
    len_by_tile = branches.groupby("tile").len_m.sum()
    roots = branches[branches.parent.isna()]
    trunk_a = roots.groupby("tile").area_mouth_km2.max()
    nets = nets.copy()
    nets["junction_spacing_m"] = (
        len_by_tile.reindex(nets.tile).to_numpy(float)
        / np.maximum(nets.n_junctions.to_numpy(float) + 1.0, 1.0))
    nets["trunk_mouth_logA"] = np.log10(np.maximum(
        trunk_a.reindex(nets.tile).to_numpy(float), 1e-3))
    # spacing of junctions ON the trunk specifically (the max-area root):
    # its in-window length over its direct-children count — the statistic the
    # sampler's junction walk actually needs (the network-wide figure above
    # mixes every branch's length into the numerator)
    trunk_rows = roots.sort_values("area_mouth_km2") \
        .groupby("tile").tail(1)[["tile", "branch", "len_m"]]
    nchild = kids.groupby(["tile", "parent"]).size()
    trunk_rows["n_child"] = [
        int(nchild.get((t, b), 0))
        for t, b in zip(trunk_rows.tile, trunk_rows.branch)]
    tjs = (trunk_rows.len_m / (trunk_rows.n_child + 1.0))
    tjs.index = trunk_rows.tile
    nets["trunk_junction_spacing_m"] = tjs.reindex(nets.tile).to_numpy(float)

    rid = params[(params.kind == "ridge") & params.ok] \
        .sort_values(["tile", "branch"]).reset_index(drop=True)
    rid = rid.assign(flank=(rid.wall_l + rid.wall_r) / 2.0)
    blf = params[(params.kind == "bluff") & params.ok] \
        .sort_values(["tile", "branch"]).reset_index(drop=True)
    bwl = params[(params.kind == "bowl") & params.ok] \
        .sort_values(["tile", "branch"]).reset_index(drop=True)

    def w_of(tiles, j) -> np.ndarray:
        if j is None:
            return np.ones(len(tiles))
        return np.array([tile_resp.get(t, [0.0] * k_arch)[j] for t in tiles])

    def _wfrac(flags: np.ndarray, w: np.ndarray) -> float | None:
        sw = float(w.sum())
        if sw <= 0:
            return None
        return round(float((flags.astype(float) * w).sum() / sw), 4)

    def _get(d: dict | None, ks: tuple):
        for k in ks:
            if d is None:
                return None
            d = d.get(k)
        return d

    # ---- one block (pooled when j is None, else archetype j shrunk) -----
    def block(j: int | None, P: dict | None) -> dict:
        wv = w_of(val.tile, j)
        wb = w_of(natural.tile, j)
        wn = w_of(nets.tile, j)

        def T(vals, w, *ks):
            v = np.asarray(vals, float)
            m = np.isfinite(v) & (np.asarray(w, float) > 0)
            own = _dec(vals, w)
            if P is None:
                return own
            return _shrink_tab(own, _get(P, ks),
                               _kish(np.asarray(w, float)[m]))

        out: dict = {"n_eff": {
            "reaches": round(_kish(wv), 1),
            "branches": round(_kish(wb), 1),
            "tiles": round(_kish(wn), 1),
        }}

        # hydraulic: regressions on ln A + residual deciles
        hyd: dict = {}
        resid: dict[str, np.ndarray] = {}
        for name, xs, ys, ws in (
                ("floor_halfwidth", val.area_km2, val.hw, wv),
                ("incision", val.area_km2, val.incision, wv),
                ("top_width", natural.area_mouth_km2, natural.top_width_m,
                 wb)):
            r = _wls_loglog(xs, ys, ws)
            if r is None:
                hyd[name] = _get(P, ("hydraulic", name))
                continue
            b0, b1, res = r
            n_eff = _kish(np.asarray(ws, float)[np.isfinite(res)])
            lam = 1.0 if P is None else n_eff / (n_eff + SHRINK_N0)
            p0 = _get(P, ("hydraulic", name, "b0"))
            p1 = _get(P, ("hydraulic", name, "b1"))
            hyd[name] = {
                "b0": round(lam * b0 + (1 - lam) * (p0 if p0 is not None
                                                    else b0), 4),
                "b1": round(lam * b1 + (1 - lam) * (p1 if p1 is not None
                                                    else b1), 4),
                "resid_deciles": T(res, ws, "hydraulic", name,
                                   "resid_deciles"),
            }
            resid[name] = res
        # slope-area law S = k A^-theta  (branch fall_raw vs mouth area)
        r = _wls_loglog(natural.area_mouth_km2, natural.fall_raw, wb)
        if r is not None:
            b0, b1, res = r
            n_eff = _kish(np.asarray(wb, float)[np.isfinite(res)])
            lam = 1.0 if P is None else n_eff / (n_eff + SHRINK_N0)
            p0 = _get(P, ("hydraulic", "fall_gradient", "ln_k"))
            pt = _get(P, ("hydraulic", "fall_gradient", "theta"))
            hyd["fall_gradient"] = {
                "ln_k": round(lam * b0 + (1 - lam) * (p0 if p0 is not None
                                                      else b0), 4),
                "theta": round(lam * (-b1) + (1 - lam) * (pt if pt is not None
                                                          else -b1), 4),
                "resid_deciles": T(res, wb, "hydraulic", "fall_gradient",
                                   "resid_deciles"),
            }
        else:
            hyd["fall_gradient"] = _get(P, ("hydraulic", "fall_gradient"))
        hyd["wall_grad_mean_deciles"] = T(val.wall_mean, wv, "hydraulic",
                                          "wall_grad_mean_deciles")
        hyd["wall_asym_logratio_deciles"] = T(val.wall_asym, wv, "hydraulic",
                                              "wall_asym_logratio_deciles")
        hyd["floor_round_deciles"] = T(val.floor_round, wv, "hydraulic",
                                       "floor_round_deciles")
        out["hydraulic"] = hyd

        # copula over reach residual vector (rank corr -> Cholesky)
        if "incision" in resid and "floor_halfwidth" in resid:
            M = np.stack([resid["incision"], resid["floor_halfwidth"],
                          val.wall_mean.to_numpy(float),
                          val.wall_asym.to_numpy(float),
                          val.floor_round.to_numpy(float)], 1)
            rows = np.isfinite(M).all(1)
            if j is not None:
                rows &= wv >= HARD_RESP
            if int(rows.sum()) >= COPULA_MIN_N or P is None:
                from scipy.stats import norm, rankdata
                Mv = M[rows]
                U = np.stack([rankdata(Mv[:, c]) / (len(Mv) + 1.0)
                              for c in range(Mv.shape[1])], 1)
                Z = norm.ppf(U)
                C = np.corrcoef(Z.T) + np.eye(M.shape[1]) * 1e-6
                out["copula"] = {
                    "fields": COPULA_FIELDS,
                    "chol": [[round(float(x), 4) for x in row]
                             for row in np.linalg.cholesky(C)],
                    "source": "own", "n": int(rows.sum()),
                }
            else:
                out["copula"] = {**_get(P, ("copula",)), "source": "pooled"}
        elif P is not None:
            out["copula"] = {**_get(P, ("copula",)), "source": "pooled"}

        # meander (branch level, precomputed wavelength_mult etc.)
        wm = w_of(meand.tile, j)
        out["meander"] = {
            "fit_frac": _wfrac(np.isfinite(natural.wavelength_mult
                                           .to_numpy(float)), wb),
            "wavelength_mult_deciles": T(meand.wavelength_mult, wm,
                                         "meander", "wavelength_mult_deciles"),
            "intensity_deciles": T(meand.meander_intensity, wm,
                                   "meander", "intensity_deciles"),
            "sinuosity_deciles": T(meand.sinuosity, wm,
                                   "meander", "sinuosity_deciles"),
        }

        # network + tilt (tile / junction level)
        wk = w_of(kids.tile, j)
        ja = natural[np.isfinite(natural.junction_angle_deg)]
        wj = w_of(ja.tile, j)
        out["network"] = {
            "drainage_density_deciles": T(nets.drainage_density_km, wn,
                                          "network",
                                          "drainage_density_deciles"),
            "junction_angle_deg_deciles": T(ja.junction_angle_deg, wj,
                                            "network",
                                            "junction_angle_deg_deciles"),
            "junction_area_logratio_deciles": T(
                kids.area_logratio, wk,
                "network", "junction_area_logratio_deciles"),
            "junction_spacing_m_deciles": T(nets.junction_spacing_m, wn,
                                            "network",
                                            "junction_spacing_m_deciles"),
            "trunk_junction_spacing_m_deciles": T(
                nets.trunk_junction_spacing_m, wn,
                "network", "trunk_junction_spacing_m_deciles"),
            "n_junctions_deciles": T(nets.n_junctions, wn, "network",
                                     "n_junctions_deciles"),
            "trunk_mouth_logA_deciles": T(nets.trunk_mouth_logA, wn,
                                          "network",
                                          "trunk_mouth_logA_deciles"),
            "depth2_frac": _wfrac(depth2, wk),
            # in-WINDOW branch length — the right statistic for a 3 km
            # generator (real branches are window-truncated the same way)
            "branch_len_m_deciles": T(natural.len_m, wb, "network",
                                      "branch_len_m_deciles"),
            # side of trunk a tributary joins on — not recorded by the
            # extractor in v0.1; uninformative Beta(1,1) documented default
            "side_balance_beta": [1.0, 1.0],
        }
        out["tilt"] = {"grade_deciles": T(nets.tilt_grade, wn,
                                          "tilt", "grade_deciles")}

        # ridges / bluffs / bowls
        wr = w_of(rid.tile, j)
        out["ridges"] = {
            "per_km2_deciles": T(nets.n_ridges / 9.0, wn, "ridges",
                                 "per_km2_deciles"),
            "prominence_deciles": T(rid.incision, wr, "ridges",
                                    "prominence_deciles"),
            "crest_halfwidth_deciles": T(rid.hw, wr, "ridges",
                                         "crest_halfwidth_deciles"),
            "flank_grad_deciles": T(rid.flank, wr, "ridges",
                                    "flank_grad_deciles"),
            "len_m_deciles": T(rid.len_m, wr, "ridges", "len_m_deciles"),
            "spacing_m_deciles": T(nets.ridge_spacing_m, wn, "ridges",
                                   "spacing_m_deciles"),
        }
        wbl = w_of(blf.tile, j)
        out["bluffs"] = {
            "per_km2_deciles": T(nets.n_bluffs / 9.0, wn, "bluffs",
                                 "per_km2_deciles"),
            "height_deciles": T(blf.incision, wbl, "bluffs",
                                "height_deciles"),
            "face_grad_deciles": T(blf.wall_l, wbl, "bluffs",
                                   "face_grad_deciles"),
            "len_m_deciles": T(blf.len_m, wbl, "bluffs", "len_m_deciles"),
        }
        wbo = w_of(bwl.tile, j)
        out["bowls"] = {
            "per_km2_deciles": T(nets.n_bowls / 9.0, wn, "bowls",
                                 "per_km2_deciles"),
            "depth_deciles": T(bwl.incision, wbo, "bowls", "depth_deciles"),
            "radius_deciles": T(bwl.hw, wbo, "bowls", "radius_deciles"),
            "inner_grad_deciles": T(bwl.wall_l, wbo, "bowls",
                                    "inner_grad_deciles"),
            "wobble_deciles": T(bwl.wobble, wbo, "bowls", "wobble_deciles"),
            "cycles_deciles": T(bwl.cycles, wbo, "bowls", "cycles_deciles"),
            "lake_frac": _wfrac((bwl.source == "lake").to_numpy(), wbo),
        }
        out["noise_defaults"] = noise_mid
        return out

    pooled = block(None, None)
    archetypes = []
    tiles_sorted = sorted(tile_resp)
    for j in range(k_arch):
        b = block(j, pooled)
        b["name"] = arch["names"][j]
        b["weight"] = round(float(np.mean(
            [tile_resp[t][j] for t in tiles_sorted])), 4)
        archetypes.append(b)

    return {
        "version": 1,
        "prior_version": LANDFORM_PRIOR_VERSION,
        "source": {
            "n_tiles": int(nets.tile.nunique()),
            "n_valley_reaches": int(len(val)),
            "n_branches": int(len(natural)),
            "n_ridges": int(len(rid)),
            "n_bluffs": int(len(blf)),
            "n_bowls": int(len(bwl)),
            "n_meander": int(len(meand)),
            "n_junctions": int(len(kids)),
        },
        "archetype_names": arch["names"],
        "pooled": pooled,
        "archetypes": archetypes,
    }


def run() -> int:
    prior = fit()
    os.makedirs(OUT, exist_ok=True)
    out_p = os.path.join(OUT, "landform_prior.json")
    repo_p = os.path.join(REPO, "golf-landform", "data",
                          "landform_prior.json")
    os.makedirs(os.path.dirname(repo_p), exist_ok=True)
    for p in (out_p, repo_p):
        with open(p, "w") as f:
            json.dump(prior, f, indent=1, sort_keys=True)
            f.write("\n")
    print(f"landform_prior v{prior['prior_version']}  "
          f"{json.dumps(prior['source'])}")
    for a in prior["archetypes"]:
        h = a["hydraulic"]
        print(f"  {a['name'][:36]:36s} w={a['weight']:.3f} "
              f"n_eff(r/b/t)={a['n_eff']['reaches']:.0f}/"
              f"{a['n_eff']['branches']:.0f}/{a['n_eff']['tiles']:.0f} "
              f"hw~A^{h['floor_halfwidth']['b1']:.3f} "
              f"theta={h['fall_gradient']['theta']:.3f} "
              f"copula={a['copula']['source']}")
    print(f"wrote {out_p}\n  and {repo_p}")
    return 0
