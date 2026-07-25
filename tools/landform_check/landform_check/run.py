"""The closure loop: manifest → render (xtask, cached) → extract (cached)
→ S1 → compare (energy/KS/variety vs split-half baseline) → report."""

from __future__ import annotations

import hashlib
import json
import os
import subprocess
import sys

import numpy as np
import pandas as pd

from . import (CELL_M, K10, LANDFORM_CHECK_VERSION, N_PANEL_PER_ARCH, OUT,
               PANEL_SEED_BASE, POP_SEED_SALT, REPO)

REACH_FIELDS = ["ln_incision", "ln_hw", "wall_mean", "wall_asym",
                "floor_round"]
NET_FIELDS = ["drainage_density_km", "junction_angle_med", "n_junctions",
              "n_ridges", "n_bluffs", "n_bowls", "tilt_grade",
              "geo_ridge_frac", "geo_valley_frac", "geo_flat_frac"]
ENERGY_BAR_X = 2.0        # generated-vs-real energy <= 2x split-half
VARIETY_MIN = 0.6         # generated IQR >= 60% of tile IQR


def _tile_resp():
    arch = json.load(open(os.path.join(REPO, "tools", "tile_scout", "out",
                                       "archetypes.json")))
    man = json.load(open(os.path.join(REPO, "tools", "dtm_atlas",
                                      "tiles_us.json")))
    resp_by_course = {c: arch["resp"][i] for i, c in enumerate(arch["keys"])}
    k = len(arch["names"])
    uniform = [1.0 / k] * k
    return ({t: resp_by_course.get(v.get("best_course"), uniform)
             for t, v in man.items()}, arch["names"])


def _u64(data: bytes) -> int:
    return int.from_bytes(hashlib.blake2b(data, digest_size=8).digest(),
                          "little")


def fleet() -> list[dict]:
    """The deterministic fleet: 205 population-matched + 5x41 panel."""
    tile_resp, names = _tile_resp()
    k = len(names)
    items = []
    for t in sorted(tile_resp):
        h = _u64(POP_SEED_SALT + t.encode())
        u = (h >> 11) / float(1 << 53)
        cum = 0.0
        arch = k - 1
        for j, w in enumerate(tile_resp[t]):
            cum += w
            if u < cum:
                arch = j
                break
        items.append({"key": f"pop_{t}", "seed": h, "archetype": arch,
                      "group": "pop", "match_tile": t})
    for a in range(k):
        for j in range(N_PANEL_PER_ARCH):
            items.append({"key": f"pnl_a{a}_{j:02d}",
                          "seed": PANEL_SEED_BASE + a * 1000 + j,
                          "archetype": a, "group": "panel",
                          "match_tile": ""})
    return items


def render(force: bool = False) -> int:
    grids = os.path.join(OUT, "grids")
    reports = os.path.join(OUT, "reports")
    os.makedirs(grids, exist_ok=True)
    os.makedirs(reports, exist_ok=True)
    items = fleet()
    manifest = []
    for it in items:
        hg = os.path.join(grids, f"{it['key']}.hg01")
        rp = os.path.join(reports, f"{it['key']}.json")
        if not force and os.path.exists(hg) and os.path.exists(rp):
            continue
        manifest.append({"out": hg, "seed": it["seed"], "cell": CELL_M,
                         "archetype": it["archetype"], "composed": True,
                         "report": rp})
    if not manifest:
        print(f"render: all {len(items)} grids cached")
        return 0
    mp = os.path.join(OUT, "render_manifest.json")
    with open(mp, "w") as f:
        json.dump(manifest, f)
    print(f"render: {len(manifest)} of {len(items)} grids to build")
    r = subprocess.run(["cargo", "run", "-p", "xtask", "--release", "-q",
                        "--", "landform-sample-grid", mp], cwd=REPO)
    return r.returncode


def extract(workers: int = 0) -> int:
    from concurrent.futures import ProcessPoolExecutor
    from .synthx import safe_worker
    ex_dir = os.path.join(OUT, "extract")
    os.makedirs(ex_dir, exist_ok=True)
    todo = []
    for it in fleet():
        oj = os.path.join(ex_dir, f"{it['key']}.json")
        if os.path.exists(oj):
            continue
        todo.append({"key": it["key"],
                     "hg01": os.path.join(OUT, "grids", f"{it['key']}.hg01"),
                     "out_json": oj})
    if not todo:
        print("extract: all cached")
        return 0
    workers = workers or max(1, (os.cpu_count() or 4) - 2)
    print(f"extract: {len(todo)} grids on {workers} workers")
    fails = 0
    with ProcessPoolExecutor(max_workers=workers) as pool:
        for k, res in enumerate(pool.map(safe_worker, todo)):
            if not res["ok"]:
                fails += 1
                print(f"  FAIL {res['key']}: {res['err']}")
            if (k + 1) % 25 == 0 or k + 1 == len(todo):
                print(f"  ... {k + 1}/{len(todo)} ({fails} failed)")
    return 1 if fails else 0


def _s1_of_grid(nat: np.ndarray, cell: float) -> dict:
    from tile_scout.f2_terrain import s1_vector
    k = max(1, int(round(10.0 / cell)))
    ny, nx = nat.shape[0] // k, nat.shape[1] // k
    z10 = np.nanmean(nat[: ny * k, : nx * k].reshape(ny, k, nx, k),
                     axis=(1, 3))
    return s1_vector(z10, 10.0)


def s1(force: bool = False) -> int:
    """S1 vectors for BOTH populations with the same estimator."""
    from dtm_metrics.sandbox.campaign import read_hg01
    out_p = os.path.join(OUT, "s1_gen.parquet")
    if force or not os.path.exists(out_p):
        rows = []
        for it in fleet():
            hg = os.path.join(OUT, "grids", f"{it['key']}.hg01")
            if not os.path.exists(hg):
                continue
            h, cell = read_hg01(hg)
            rows.append({"tile": it["key"], "group": it["group"],
                         "archetype": it["archetype"],
                         **_s1_of_grid(h[::-1].astype(float), cell)})
            if len(rows) % 50 == 0:
                print(f"  s1 gen ... {len(rows)}")
        pd.DataFrame(rows).to_parquet(out_p)
        print(f"s1: wrote {out_p} ({len(rows)} rows)")
    real_p = os.path.join(OUT, "s1_real.parquet")
    if force or not os.path.exists(real_p):
        from dtm_atlas import config as atlas_config
        from dtm_atlas import grids as agrids
        atlas_config.select_dataset("tiles")
        man = json.load(open(os.path.join(REPO, "tools", "dtm_atlas",
                                          "tiles_us.json")))
        rows = []
        for t in sorted(man):
            p = os.path.join(atlas_config.STORE, t, "naturalized.tif")
            if not os.path.exists(p):
                continue
            arr, _tf, _e = agrids.read_gtiff(p)
            nat = np.where(arr == atlas_config.NODATA, np.nan,
                           arr.astype(float))
            rows.append({"tile": t, **_s1_of_grid(nat, 2.0)})
            if len(rows) % 50 == 0:
                print(f"  s1 real ... {len(rows)}")
        pd.DataFrame(rows).to_parquet(real_p)
        print(f"s1: wrote {real_p} ({len(rows)} rows)")
    return 0


def _load_gen():
    reaches, nets, branches = [], [], []
    for it in fleet():
        oj = os.path.join(OUT, "extract", f"{it['key']}.json")
        if not os.path.exists(oj):
            continue
        d = json.load(open(oj))
        row = dict(d["tile"])
        row.update(group=it["group"], archetype=it["archetype"])
        nets.append(row)
        for r in d["reaches"]:
            r2 = dict(r)
            r2.update(group=it["group"], archetype=it["archetype"])
            reaches.append(r2)
        for b in d["branches"]:
            b2 = dict(b)
            b2.update(group=it["group"], archetype=it["archetype"])
            branches.append(b2)
    return (pd.DataFrame(reaches), pd.DataFrame(nets),
            pd.DataFrame(branches))


def _reach_matrix(df: pd.DataFrame) -> np.ndarray:
    v = df[(df.kind == "valley") & df.ok].copy()
    m = np.stack([
        np.log(np.maximum(v.incision.to_numpy(float), 1e-2)),
        np.log(np.maximum(v.hw.to_numpy(float), 1e-2)),
        (v.wall_l.to_numpy(float) + v.wall_r.to_numpy(float)) / 2.0,
        np.log(np.maximum(v.wall_l.to_numpy(float), 1e-3)
               / np.maximum(v.wall_r.to_numpy(float), 1e-3)),
        v.floor_round.to_numpy(float),
    ], 1)
    return m[np.isfinite(m).all(1)]


def _net_matrix(df: pd.DataFrame) -> np.ndarray:
    m = np.stack([df[c].to_numpy(float) for c in NET_FIELDS], 1)
    # junction_angle_med is NaN on junction-free tiles — fill with the
    # column median so the tile still participates in the other axes
    for c in range(m.shape[1]):
        col = m[:, c]
        med = np.nanmedian(col)
        col[~np.isfinite(col)] = med
    return m[np.isfinite(m).all(1)]


def _energy(a: np.ndarray, b: np.ndarray, l_from: np.ndarray) -> float:
    from tile_scout import scoring
    lw = scoring.whitener(l_from)
    return float(scoring.energy_distance(a @ lw.T, b @ lw.T))


def _split_half(m: np.ndarray) -> float:
    a, b = m[0::2], m[1::2]
    return _energy(a, b, m)


def _ks(a: np.ndarray, b: np.ndarray) -> float:
    from scipy.stats import ks_2samp
    return float(ks_2samp(a, b).statistic)


def compare() -> int:
    gen_r, gen_n, gen_b = _load_gen()
    prim = os.path.join(REPO, "tools", "dtm_primitives", "out")
    real_r = pd.read_parquet(os.path.join(prim, "params_tiles.parquet"))
    real_n = pd.read_parquet(os.path.join(prim, "networks_tiles.parquet"))
    tile_resp, names = _tile_resp()
    real_n = real_n.assign(archetype=[
        int(np.argmax(tile_resp.get(t, [1.0]))) for t in real_n.tile])
    s1g = pd.read_parquet(os.path.join(OUT, "s1_gen.parquet"))
    s1r = pd.read_parquet(os.path.join(OUT, "s1_real.parquet"))
    s1_cols = sorted(c for c in s1r.columns
                     if c != "tile" and c in s1g.columns)

    rep: dict = {"version": LANDFORM_CHECK_VERSION, "cell_m": CELL_M,
                 "n_gen": int(gen_n.tile.nunique()),
                 "n_real": int(real_n.tile.nunique()),
                 "families": {}, "variety": {}, "per_archetype": {},
                 "bars": {"energy_x": ENERGY_BAR_X,
                          "variety_min": VARIETY_MIN}}
    pop_keys = {it["key"] for it in fleet() if it["group"] == "pop"}

    def fam(name, gm, rm):
        e = _energy(gm, rm, rm)
        sh = _split_half(rm)
        rep["families"][name] = {
            "energy": round(e, 4), "split_half": round(sh, 4),
            "ratio": round(e / max(sh, 1e-9), 2),
            "pass": bool(e <= ENERGY_BAR_X * sh),
            "n_gen": int(len(gm)), "n_real": int(len(rm)),
        }

    # population-matched subset drives the closure comparison
    fam("reach", _reach_matrix(gen_r[gen_r.tile.isin(pop_keys)]),
        _reach_matrix(real_r[(real_r.source == "nhd")
                             | (real_r.kind != "valley")]))
    fam("network", _net_matrix(gen_n[gen_n.tile.isin(pop_keys)]),
        _net_matrix(real_n))
    g1 = s1g[s1g.tile.isin(pop_keys)][s1_cols].to_numpy(float)
    r1 = s1r[s1_cols].to_numpy(float)
    g1 = g1[np.isfinite(g1).all(1)]
    r1 = r1[np.isfinite(r1).all(1)]
    fam("s1", g1, r1)

    # KS per field (population-matched)
    ks_out = {}
    gv = gen_r[(gen_r.kind == "valley") & gen_r.ok
               & gen_r.tile.isin(pop_keys)]
    rv = real_r[(real_r.kind == "valley") & real_r.ok
                & (real_r.source == "nhd")]
    for col in ("incision", "hw", "wall_l", "floor_round"):
        ks_out[f"reach_{col}"] = round(
            _ks(gv[col].dropna().to_numpy(float),
                rv[col].dropna().to_numpy(float)), 3)
    for col in NET_FIELDS:
        a = gen_n[gen_n.tile.isin(pop_keys)][col].dropna().to_numpy(float)
        b = real_n[col].dropna().to_numpy(float)
        if len(a) > 5 and len(b) > 5:
            ks_out[f"net_{col}"] = round(_ks(a, b), 3)
    rep["ks"] = ks_out

    # variety: generated IQR / real IQR (pooled + per archetype panels)
    def iqr(x):
        x = x[np.isfinite(x)]
        return float(np.percentile(x, 75) - np.percentile(x, 25)) \
            if len(x) > 4 else float("nan")

    for col in NET_FIELDS:
        rr = iqr(real_n[col].to_numpy(float))
        gg = iqr(gen_n[col].to_numpy(float))
        if np.isfinite(rr) and rr > 1e-9 and np.isfinite(gg):
            rep["variety"][col] = {
                "ratio": round(gg / rr, 2),
                "pass": bool(gg / rr >= VARIETY_MIN)}
    for col in ("incision", "hw"):
        rr = iqr(np.log(np.maximum(
            rv[col].dropna().to_numpy(float), 1e-2)))
        gg = iqr(np.log(np.maximum(
            gv[col].dropna().to_numpy(float), 1e-2)))
        if np.isfinite(rr) and rr > 1e-9:
            rep["variety"][f"ln_{col}"] = {
                "ratio": round(gg / rr, 2),
                "pass": bool(gg / rr >= VARIETY_MIN)}

    # per-archetype energy on the network family (the axis with per-tile n)
    for a, name in enumerate(names):
        ga = _net_matrix(gen_n[(gen_n.archetype == a)
                               & gen_n.tile.isin(pop_keys)])
        ra = _net_matrix(real_n[real_n.archetype == a])
        if len(ga) >= 8 and len(ra) >= 8:
            e = _energy(ga, ra, _net_matrix(real_n))
            sh = _split_half(_net_matrix(real_n))
            rep["per_archetype"][name] = {
                "n_gen": int(len(ga)), "n_real": int(len(ra)),
                "energy": round(e, 4),
                "ratio_vs_pooled_split": round(e / max(sh, 1e-9), 2)}

    passes = [f["pass"] for f in rep["families"].values()] \
        + [v["pass"] for v in rep["variety"].values()]
    rep["gate"] = {"all_energy_pass": all(
        f["pass"] for f in rep["families"].values()),
        "all_variety_pass": all(v["pass"] for v in rep["variety"].values()),
        "n_checks": len(passes)}

    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "closure_report.json"), "w") as f:
        json.dump(rep, f, indent=1, sort_keys=True)
    print(json.dumps(rep["families"], indent=1))
    print("variety fails:", [k for k, v in rep["variety"].items()
                             if not v["pass"]] or "none")
    print("gate:", rep["gate"])
    print(f"wrote {os.path.join(OUT, 'closure_report.json')}")
    return 0


def all_steps(force: bool = False) -> int:
    if force:
        # extract caches by output presence — purge so re-rendered grids
        # are actually re-measured
        ex = os.path.join(OUT, "extract")
        if os.path.isdir(ex):
            for f in os.listdir(ex):
                if f.startswith(("pop_", "pnl_")):
                    os.remove(os.path.join(ex, f))
    for step in (lambda: render(force), lambda: extract(),
                 lambda: s1(force), compare):
        rc = step()
        if rc:
            return rc
    return 0
