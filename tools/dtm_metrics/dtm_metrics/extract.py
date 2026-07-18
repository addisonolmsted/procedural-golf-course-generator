"""Extraction CLI: run the candidate battery over the frozen store ->
out/metrics.parquet (one wide row per course; S1 metrics carry all_valid +
__clean variant columns).

ProcessPool notes (the documented v1 gotcha): the worker is MODULE-LEVEL
(spawn-picklable) and the CLI entry has a __main__ guard — running this from a
heredoc/stdin will BrokenProcessPool by design of macOS spawn.
"""

from __future__ import annotations

import os
import time
from concurrent.futures import ProcessPoolExecutor

import pandas as pd

from . import candidates, store


def _worker(args) -> dict | None:
    key, cfg = args
    try:
        t0 = time.time()
        c = store.load_course(key)
        row = candidates.compute_course(c, cfg)
        row["ctx_extract_s"] = round(time.time() - t0, 2)
        return row
    except Exception as e:  # never kill the batch
        return {"key": key, "ctx_error": f"{type(e).__name__}: {e}"}


def run(keys: list[str] | None, workers: int, out_path: str | None = None) -> pd.DataFrame:
    cfg = store.load_config()
    if cfg["store"].get("verify_manifest", True):
        store.verify_store_or_die()
    if keys is None:
        keys = store.course_keys()
    tasks = [(k, cfg) for k in keys]
    rows: list[dict] = []
    with ProcessPoolExecutor(max_workers=workers) as ex:
        for i, row in enumerate(ex.map(_worker, tasks, chunksize=1)):
            rows.append(row)
            err = row.get("ctx_error")
            took = row.get("ctx_extract_s", 0)
            msg = f"FAIL {err}" if err else f"ok ({took} s)"
            print(f"[{i + 1}/{len(tasks)}] {row['key']}: {msg}", flush=True)
    df = pd.DataFrame(rows).set_index("key").sort_index()
    if out_path:
        os.makedirs(os.path.dirname(out_path), exist_ok=True)
        df.to_parquet(out_path)
        print(f"wrote {out_path} ({len(df)} courses x {len(df.columns)} columns)")
    return df
