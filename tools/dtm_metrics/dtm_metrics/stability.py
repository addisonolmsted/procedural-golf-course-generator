"""G2 (mask/clip stability), G3 real-side (split-half ICC), and the
L-sensitivity table for the primitive-proxy cutoff.

All variants recompute the battery on modified Course views and compare
across courses; results feed gates.py.
"""

from __future__ import annotations

import dataclasses
import os
from concurrent.futures import ProcessPoolExecutor

import numpy as np
import pandas as pd
from scipy import stats as sstats

from . import candidates, store

VARIANTS = ("masks_off", "interior_only", "half_w", "half_e", "half_n", "half_s")


def _crop(c: store.Course, sl_y: slice, sl_x: slice) -> store.Course:
    kw = {}
    for f in dataclasses.fields(c):
        v = getattr(c, f.name)
        kw[f.name] = v[sl_y, sl_x] if isinstance(v, np.ndarray) else v
    return store.Course(**kw)


def _variant(c: store.Course, name: str) -> store.Course:
    h, w = c.naturalized.shape
    if name == "masks_off":
        return dataclasses.replace(c, clean=c.valid.copy(),
                                   inpaint=np.zeros_like(c.inpaint))
    if name == "interior_only":
        v = c.valid & c.interior
        return dataclasses.replace(c, valid=v, clean=c.clean & c.interior,
                                   exterior=np.zeros_like(c.exterior))
    if name == "half_w":
        return _crop(c, slice(None), slice(0, w // 2))
    if name == "half_e":
        return _crop(c, slice(None), slice(w // 2, w))
    if name == "half_n":
        return _crop(c, slice(0, h // 2), slice(None))
    if name == "half_s":
        return _crop(c, slice(h // 2, h), slice(None))
    raise KeyError(name)


def _worker(args) -> dict:
    key, cfg, which = args
    c = store.load_course(key)
    out = {"key": key}
    for name in which:
        try:
            row = candidates.compute_course(_variant(c, name), cfg)
            for k, v in row.items():
                if k.startswith(("s1_", "s2_", "s3_")) and isinstance(v, float):
                    out[f"{k}@{name}"] = v
        except Exception as e:
            out[f"ctx_error@{name}"] = f"{type(e).__name__}: {e}"
    return out


def _l_worker(args) -> dict:
    key, cfg, L = args
    cfg = dict(cfg)
    cfg["surfaces"] = dict(cfg["surfaces"], lowpass_L_m=float(L))
    c = store.load_course(key)
    row = candidates.compute_course(c, cfg)
    return {"key": key,
            **{f"{k}@L{int(L)}": v for k, v in row.items()
               if k.startswith(("s2_", "s3_")) and isinstance(v, float)}}


def run(workers: int) -> int:
    cfg = store.load_config()
    mdf = pd.read_parquet(os.path.join(store.OUT, "metrics.parquet"))
    conf = mdf[~mdf["ctx_low_confidence"].astype(bool)]
    keys = list(conf.index)
    print(f"G2/G3 stability over {len(keys)} confident courses")

    rows = []
    with ProcessPoolExecutor(max_workers=workers) as ex:
        for i, row in enumerate(ex.map(
                _worker, [(k, cfg, VARIANTS) for k in keys], chunksize=1)):
            rows.append(row)
            if (i + 1) % 20 == 0:
                print(f"  {i + 1}/{len(keys)}", flush=True)
    vdf = pd.DataFrame(rows).set_index("key")
    os.makedirs(os.path.join(store.OUT, "stability"), exist_ok=True)
    vdf.to_parquet(os.path.join(store.OUT, "stability", "variants.parquet"))

    metrics = sorted({c.split("@")[0] for c in vdf.columns if "@" in c
                      and not c.startswith("ctx_")})
    rep = []
    for m in metrics:
        base = mdf.loc[vdf.index, m] if m in mdf else pd.Series(np.nan, index=vdf.index)
        pop_std = max(float(mdf[m].std()), 1e-9) if m in mdf else 1e-9
        row = {"metric": m}
        # G2: agreement of masks_off / interior_only with the baseline
        for name in ("masks_off", "interior_only"):
            v = vdf.get(f"{m}@{name}")
            if v is None:
                continue
            pair = pd.concat([base, v], axis=1).dropna()
            if len(pair) >= 20:
                row[f"g2_{name}_rho"] = float(
                    sstats.spearmanr(pair.iloc[:, 0], pair.iloc[:, 1]).statistic)
                row[f"g2_{name}_dmed"] = float(
                    (pair.iloc[:, 1] - pair.iloc[:, 0]).abs().median() / pop_std)
        # all_valid vs __clean agreement (from the main parquet)
        if f"{m}__clean" in mdf.columns:
            pair = mdf[[m, f"{m}__clean"]].dropna()
            if len(pair) >= 20:
                row["g2_cleanvar_rho"] = float(
                    sstats.spearmanr(pair.iloc[:, 0], pair.iloc[:, 1]).statistic)
        # G3: split-half ICC pooled over both axes
        icc_vals = []
        for a, b in (("half_w", "half_e"), ("half_n", "half_s")):
            va, vb = vdf.get(f"{m}@{a}"), vdf.get(f"{m}@{b}")
            if va is None or vb is None:
                continue
            pair = pd.concat([va, vb], axis=1).dropna()
            if len(pair) >= 20:
                x, y = pair.iloc[:, 0].to_numpy(), pair.iloc[:, 1].to_numpy()
                within = float(np.mean((x - y) ** 2) / 2.0)
                between = float(np.var((x + y) / 2.0))
                icc_vals.append((between - within) / (between + within)
                                if between + within > 0 else np.nan)
        if icc_vals:
            row["g3_icc"] = float(np.nanmean(icc_vals))
        rep.append(row)
    pd.DataFrame(rep).set_index("metric").to_parquet(
        os.path.join(store.OUT, "stability", "g2g3.parquet"))
    print("wrote stability/g2g3.parquet")

    # L-sensitivity on a seeded 20-course subset
    rng = np.random.default_rng(cfg["seed"])
    sub = sorted(rng.choice(keys, size=min(20, len(keys)), replace=False))
    ltasks = [(k, cfg, L) for L in cfg["surfaces"]["L_sensitivity_m"] for k in sub]
    lrows: dict[str, dict] = {}
    with ProcessPoolExecutor(max_workers=workers) as ex:
        for row in ex.map(_l_worker, ltasks, chunksize=1):
            lrows.setdefault(row["key"], {}).update(row)
    ldf = pd.DataFrame(list(lrows.values())).set_index("key")
    base = mdf.loc[ldf.index]
    lrep = []
    for m in sorted({c.split("@")[0] for c in ldf.columns}):
        r = {"metric": m}
        for L in cfg["surfaces"]["L_sensitivity_m"]:
            v = ldf.get(f"{m}@L{int(L)}")
            if v is None or m not in base:
                continue
            pair = pd.concat([base[m], v], axis=1).dropna()
            if len(pair) >= 10:
                r[f"rho_L{int(L)}"] = float(
                    sstats.spearmanr(pair.iloc[:, 0], pair.iloc[:, 1]).statistic)
        lrep.append(r)
    pd.DataFrame(lrep).set_index("metric").to_parquet(
        os.path.join(store.OUT, "stability", "l_sensitivity.parquet"))
    print("wrote stability/l_sensitivity.parquet")
    return 0
