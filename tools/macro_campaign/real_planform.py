"""Real-corpus planform twin of crates/course-skeleton/examples/planform.rs.

Traces D8 flow paths over the kept lidar tiles (same router + channel
acceptance as E5's extract_v2.skeleton) and computes the SAME statistics:
window sinuosity (300/600 m), straight-run lengths, parallel-run lengths.
"""
import json
import sys
import pathlib

import numpy as np
from scipy import ndimage
from scipy.spatial import cKDTree

ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
from macro_campaign import cgrid, develop, flow, extract_v2  # noqa: E402
from metrics import core as mcore  # noqa: E402

BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland"]
N_TILES = 8
WINDOWS = [300.0, 600.0]
STRAIGHT_S = 1.01
PAR_BAND = (40.0, 200.0)


def trace_paths(z, cell, developed):
    zfill = mcore.fill_depressions(z, cell)
    rec, _ = flow.receivers(zfill, cell)
    acc = flow.accumulate(rec).astype(float) * cell * cell
    lake = zfill > z + 0.01
    channels = acc >= extract_v2.CHANNEL_AREA_M2
    channels &= ~lake | (acc >= 4.0 * extract_v2.CHANNEL_AREA_M2)
    if developed is not None:
        channels &= ~developed
    ny, nx = z.shape
    flat_rec = rec.ravel()
    ch_flat = channels.ravel()
    # donors within the channel mask -> sources are channel cells nothing
    # channelized drains into
    donors = np.zeros(ny * nx, dtype=np.int32)
    src_idx = np.nonzero(ch_flat)[0]
    tgt = flat_rec[src_idx]
    ok = tgt >= 0
    np.add.at(donors, tgt[ok][ch_flat[tgt[ok]]], 1)
    sources = [i for i in src_idx if donors[i] == 0]
    visited = np.zeros(ny * nx, dtype=bool)
    paths = []
    for s in sources:
        path = []
        i = s
        while i >= 0 and ch_flat[i] and not visited[i]:
            visited[i] = True
            path.append(i)
            i = flat_rec[i]
        # include the junction/terminal cell so paths touch where they meet
        if i >= 0 and ch_flat[i]:
            path.append(i)
        if len(path) >= 5:
            ys, xs = np.divmod(np.array(path), nx)
            paths.append(np.stack([xs * cell, ys * cell], axis=1).astype(float))
    return paths


def smooth_resample(p, cell, step):
    # ~50 m moving average kills the D8 staircase before arc is measured
    k = max(3, int(round(50.0 / cell)) | 1)
    if len(p) <= k:
        return None
    sm = np.stack(
        [ndimage.uniform_filter1d(p[:, 0], k, mode="nearest"),
         ndimage.uniform_filter1d(p[:, 1], k, mode="nearest")], axis=1)
    d = np.hypot(*np.diff(sm, axis=0).T)
    arc = np.concatenate([[0.0], np.cumsum(d)])
    if arc[-1] < step * 2:
        return None
    t = np.arange(0.0, arc[-1], step)
    return np.stack([np.interp(t, arc, sm[:, 0]), np.interp(t, arc, sm[:, 1])], axis=1)


def main():
    out = {}
    for biome in BIOMES:
        tiles = [t for a, t in extract_v2.kept_tiles() if a == biome][:N_TILES]
        sin = {w: [] for w in WINDOWS}
        straight_runs, par_runs = [], []
        total_len = 0.0
        par_len = 0.0
        for tid in tiles:
            src = extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid"
            z, (ox, oy, cell) = cgrid.read_f32(src)
            z = z.astype(np.float64)
            dev = develop.load_mask(biome, tid)
            if dev is not None and dev.shape != z.shape:
                dev = None
            paths = trace_paths(z, cell, dev)
            # sinuosity + straight runs on 10 m resample
            for p in paths:
                s10 = smooth_resample(p, cell, 10.0)
                if s10 is None:
                    continue
                total_len += (len(s10) - 1) * 10.0
                for wm in WINDOWS:
                    n = int(wm / 10.0)
                    if len(s10) <= n:
                        continue
                    run = 0.0
                    for i in range(0, len(s10) - n, 5):
                        chord = float(np.hypot(*(s10[i + n] - s10[i])))
                        s = wm / max(chord, 1e-9)
                        sin[wm].append(s)
                        if wm == 600.0:
                            if s < STRAIGHT_S:
                                run += 50.0
                            elif run > 0.0:
                                straight_runs.append(run + 600.0)
                                run = 0.0
                    if wm == 600.0 and run > 0.0:
                        straight_runs.append(run + 600.0)
            # parallel runs on 30 m resample
            sampled = []
            for pi, p in enumerate(paths):
                s30 = smooth_resample(p, cell, 30.0)
                if s30 is not None:
                    sampled.append((pi, s30))
            if len(sampled) >= 2:
                all_pts = np.concatenate([s for _, s in sampled])
                all_ids = np.concatenate(
                    [np.full(len(s), pi) for pi, s in sampled])
                tree = cKDTree(all_pts)
                for pi, s30 in sampled:
                    run = 0.0
                    dd, ii = tree.query(s30, k=24, distance_upper_bound=PAR_BAND[1] + 1)
                    for row_d, row_i in zip(dd, ii):
                        dmin = np.inf
                        for d, j in zip(row_d, row_i):
                            if not np.isfinite(d) or j >= len(all_ids):
                                break
                            if all_ids[j] != pi:
                                dmin = d
                                break
                        if PAR_BAND[0] <= dmin <= PAR_BAND[1]:
                            run += 30.0
                        else:
                            if run > 400.0:
                                par_runs.append(run)
                                par_len += run
                            run = 0.0
                    if run > 400.0:
                        par_runs.append(run)
                        par_len += run
            print(f"  [{biome}] {tid}: {len(paths)} paths", file=sys.stderr)
        q = lambda v, p: float(np.quantile(v, p)) if len(v) else float("nan")
        rec = {}
        for wm in WINDOWS:
            v = sin[wm]
            rec[f"w{int(wm)}_all"] = {
                "n": len(v), "p10": q(v, 0.10), "p50": q(v, 0.50), "p90": q(v, 0.90),
                "frac_straight": (float(np.mean(np.array(v) < STRAIGHT_S)) if v else float("nan")),
            }
        rec["straight_run_p50"] = q(straight_runs, 0.5)
        rec["straight_run_p90"] = q(straight_runs, 0.9)
        rec["straight_run_max"] = max(straight_runs) if straight_runs else float("nan")
        rec["par_run_p50"] = q(par_runs, 0.5)
        rec["par_run_p90"] = q(par_runs, 0.9)
        rec["par_run_max"] = max(par_runs) if par_runs else float("nan")
        rec["par_frac"] = par_len / total_len if total_len else 0.0
        rec["n_tiles"] = len(tiles)
        out[biome] = rec
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
