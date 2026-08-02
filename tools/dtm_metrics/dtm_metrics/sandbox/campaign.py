"""Sandbox campaigns: Sobol design -> xtask noise-grid -> score with the SAME
battery -> out/sandbox/campaign_{a,b,c}.parquet.

Grids are generated and scored in BATCHES and deleted after scoring (a full
campaign would otherwise hold ~6 GB of HG01 on disk; peak is ~1 batch).

Campaign A: flat-base identifiability core (dims 1-15 + dummy, 3 seeds).
Campaign B: skeleton-on modulation study (all 19 dims, Stage-2 presets,
            conditional metrics scored against BOTH the lowpass proxy and the
            true skeleton surface -> the s3 vs s3t proxy-validation pair).
Campaign C: fit-recovery set (40 known configs).
"""

from __future__ import annotations

import dataclasses
import json
import os
import shutil
import subprocess
import sys
from concurrent.futures import ProcessPoolExecutor

import numpy as np
import pandas as pd
from scipy.stats import qmc

from .. import candidates, store, surfaces
from ..store import Course
from . import params as P

REPO = os.path.normpath(os.path.join(store.ROOT, "..", ".."))
SBOX = os.path.join(store.OUT, "sandbox")


def read_hg01(path: str):
    """HG01 -> (heights float64 row0=SOUTH to match our Grid convention, cell)."""
    import struct
    with open(path, "rb") as f:
        assert f.read(4) == b"HG01"
        ny, nx = struct.unpack("<II", f.read(8))
        (cell,) = struct.unpack("<f", f.read(4))
        h = np.frombuffer(f.read(4 * ny * nx), dtype="<f4").astype(np.float64)
        h = h.reshape(ny, nx)[::-1]     # HG01 row0=north -> row0=south
    return h, float(cell)


def _fake_course(h: np.ndarray, cell: float, key: str) -> Course:
    ones = np.ones(h.shape, bool)
    zeros = np.zeros(h.shape, bool)
    meta = {"grid": {"w": h.shape[1], "h": h.shape[0], "res_m": cell},
            "margin": {"exterior_inpaint_frac": 1.0}, "masks": {}, "inpaint": {}}
    return Course(key=key, meta=meta, cell=cell, naturalized=h, raw=h,
                  valid=ones, clean=ones.copy(), inpaint=zeros,
                  interior=ones.copy(), exterior=zeros.copy(), water=zeros.copy())


_SKEL_CACHE: dict[str, np.ndarray] = {}


def _score_worker(args) -> dict:
    """Module-level (spawn-safe). Scores one grid; adds s3t_* truth-conditioned
    metrics when a skeleton_only grid path is supplied."""
    path, meta_row, skel_path, cfg = args
    try:
        h, cell = read_hg01(path)
        c = _fake_course(h, cell, meta_row["uid"])
        row = dict(meta_row)
        row.update({k: v for k, v in candidates.compute_course(c, cfg).items()
                    if k.startswith(("s1_", "s2_", "s3_", "ctx_"))})
        if skel_path:
            if skel_path not in _SKEL_CACHE:
                _SKEL_CACHE[skel_path] = read_hg01(skel_path)[0]
            sk = _SKEL_CACHE[skel_path]
            s = surfaces.build(h, c.valid, cell, cfg["surfaces"]["lowpass_L_m"],
                               cfg["surfaces"]["tpi_window_m"],
                               macro_override=sk)
            row.update(candidates.modulation_metrics(
                s, c.clean, cell, cfg, prefix="s3t_"))
        return row
    except Exception as e:
        return {**meta_row, "ctx_error": f"{type(e).__name__}: {e}"}


def _run_batches(items: list[dict], meta_rows: list[dict], skel_paths: list,
                 cfg: dict, workers: int, batch: int = 96) -> pd.DataFrame:
    rows = []
    for b0 in range(0, len(items), batch):
        chunk = items[b0:b0 + batch]
        mpath = os.path.join(SBOX, "manifest.json")
        with open(mpath, "w") as f:
            json.dump(chunk, f)
        subprocess.run(["cargo", "run", "-q", "-p", "xtask", "--release", "--",
                        "noise-grid", mpath], cwd=REPO, check=True)
        tasks = [(it["out"], meta_rows[b0 + i], skel_paths[b0 + i], cfg)
                 for i, it in enumerate(chunk)]
        with ProcessPoolExecutor(max_workers=workers) as ex:
            for row in ex.map(_score_worker, tasks, chunksize=2):
                rows.append(row)
        for it in chunk:
            try:
                os.remove(it["out"])
            except OSError:
                pass
        print(f"  batch {b0 // batch + 1}/{(len(items) + batch - 1) // batch}: "
              f"{len(rows)} scored", flush=True)
    return pd.DataFrame(rows)


def _design(n: int, dims: list[str], seed: int) -> np.ndarray:
    eng = qmc.Sobol(d=len(dims), scramble=True, seed=seed)
    m = int(np.ceil(np.log2(max(n, 2))))
    return eng.random_base2(m=m)[:n]


def _skeleton_dump(preset: str, cell: float, extent: float) -> str:
    """Emit the bare skeleton grid once (for s3t truth conditioning)."""
    out = os.path.join(SBOX, "skeletons", f"{preset}_{int(cell*10)}.bin")
    if not os.path.exists(out):
        mpath = os.path.join(SBOX, "skel_manifest.json")
        os.makedirs(os.path.dirname(out), exist_ok=True)
        item = {"out": out, "cell": cell, "extent": extent, "skeleton": preset,
                "skeleton_only": True,
                "params": P.unit_row_to_config([], 0, [])}
        with open(mpath, "w") as f:
            json.dump([item], f)
        subprocess.run(["cargo", "run", "-q", "-p", "xtask", "--release", "--",
                        "noise-grid", mpath], cwd=REPO, check=True)
    return out


def run(which: str, workers: int) -> int:
    cfg = store.load_config()
    cc = cfg["campaigns"]
    os.makedirs(SBOX, exist_ok=True)
    gdir = os.path.join(SBOX, "grids")
    os.makedirs(gdir, exist_ok=True)

    if which == "a":
        dims = [n for n in P.NAMES if n not in ("floor_damp", "slope_gain", "grain_align")]
        U = _design(cc["a"]["n"], dims, cfg["seed"])
        np.save(os.path.join(SBOX, "design_a.npy"), U)
        with open(os.path.join(SBOX, "design_a_dims.json"), "w") as f:
            json.dump(dims, f)
        items, metas, skels = [], [], []
        for ti, u in enumerate(U):
            for seed in cc["a"]["seeds"]:
                pcfg = P.unit_row_to_config(u, seed, dims)
                uid = f"a_t{ti:04d}_s{seed}"
                items.append({"out": os.path.join(gdir, uid + ".bin"),
                              "cell": cc["cell_m"], "extent": cc["extent_flat_m"],
                              "skeleton": None, "params": pcfg})
                metas.append({"uid": uid, "theta_id": ti, "seed": seed,
                              **{n: pcfg[n] for n in P.NAMES}})
                skels.append(None)
        df = _run_batches(items, metas, skels, cfg, workers)
        df.to_parquet(os.path.join(SBOX, "campaign_a.parquet"))
        print(f"campaign A: {len(df)} rows -> campaign_a.parquet")
        return 0

    if which == "b":
        dims = list(P.NAMES)
        U = _design(cc["b"]["n"], dims, cfg["seed"] + 1)
        np.save(os.path.join(SBOX, "design_b.npy"), U)
        items, metas, skels = [], [], []
        for skel in cc["b"]["skeletons"]:
            spath = _skeleton_dump(skel, cc["cell_m"], cc["extent_skel_m"])
            for ti, u in enumerate(U):
                for seed in cc["b"]["seeds"]:
                    pcfg = P.unit_row_to_config(u, seed, dims)
                    uid = f"b_{skel}_t{ti:04d}_s{seed}"
                    items.append({"out": os.path.join(gdir, uid + ".bin"),
                                  "cell": cc["cell_m"],
                                  "extent": cc["extent_skel_m"],
                                  "skeleton": skel, "params": pcfg})
                    metas.append({"uid": uid, "theta_id": ti, "seed": seed,
                                  "skeleton": skel,
                                  **{n: pcfg[n] for n in P.NAMES}})
                    skels.append(spath)
        df = _run_batches(items, metas, skels, cfg, workers)
        df.to_parquet(os.path.join(SBOX, "campaign_b.parquet"))
        print(f"campaign B: {len(df)} rows -> campaign_b.parquet")
        return 0

    if which == "c":
        items, metas, skels = [], [], []
        # flat half: base-parameter recovery
        dims_flat = [n for n in P.NAMES
                     if n not in ("floor_damp", "slope_gain", "grain_align")]
        U1 = _design(cc["c"]["n"], dims_flat, cfg["seed"] + 2)
        np.save(os.path.join(SBOX, "design_c_flat.npy"), U1)
        for ti, u in enumerate(U1):
            for seed in cc["c"]["seeds"]:
                pcfg = P.unit_row_to_config(u, seed, dims_flat)
                uid = f"cf_t{ti:04d}_s{seed}"
                items.append({"out": os.path.join(gdir, uid + ".bin"),
                              "cell": cc["cell_m"], "extent": cc["extent_flat_m"],
                              "skeleton": None, "params": pcfg})
                metas.append({"uid": uid, "mode": "flat", "theta_id": ti,
                              "seed": seed, **{n: pcfg[n] for n in P.NAMES}})
                skels.append(None)
        # skeleton half: all 19 dims over river_confluence
        dims_all = list(P.NAMES)
        U2 = _design(cc["c"]["n"], dims_all, cfg["seed"] + 3)
        np.save(os.path.join(SBOX, "design_c_skel.npy"), U2)
        spath = _skeleton_dump("river_confluence", cc["cell_m"], cc["extent_skel_m"])
        for ti, u in enumerate(U2):
            for seed in cc["c"]["seeds"][:2]:
                pcfg = P.unit_row_to_config(u, seed, dims_all)
                uid = f"cs_t{ti:04d}_s{seed}"
                items.append({"out": os.path.join(gdir, uid + ".bin"),
                              "cell": cc["cell_m"], "extent": cc["extent_skel_m"],
                              "skeleton": "river_confluence", "params": pcfg})
                metas.append({"uid": uid, "mode": "skel", "theta_id": ti,
                              "seed": seed, **{n: pcfg[n] for n in P.NAMES}})
                skels.append(spath)
        df = _run_batches(items, metas, skels, cfg, workers)
        df.to_parquet(os.path.join(SBOX, "campaign_c.parquet"))
        print(f"campaign C: {len(df)} rows -> campaign_c.parquet")
        return 0
    print(f"unknown campaign {which}")
    return 1
