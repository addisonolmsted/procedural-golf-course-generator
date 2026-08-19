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

BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland",
          "sandhills"]
N_TILES = 8
WINDOWS = [300.0, 600.0]
STRAIGHT_S = 1.01
PAR_BAND = (40.0, 200.0)
# NEAR-parallel, the tight companion to par_frac. Same constants as
# crates/course-skeleton/examples/planform.rs, and — unlike par_frac — the
# same TRACING on both sides: reach-based, so the two numbers are comparable
# cell for cell. par_frac's 40-200 m band with a 400 m run minimum scores a
# channel 150 m from its neighbour the same as one 30 m away, which is why
# it never caught the defect this measures.
NEAR_SAMPLE_M = 20.0
NEAR_BAND_M = 60.0
NEAR_END_EXEMPT_M = 90.0


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


def trace_reaches(z, cell, developed, grid_m=8.0):
    """Maximal constant-order reaches, exactly as carve.rs `trace` cuts them.

    Two things have to match for a near-parallel measure to compare:

    `trace_paths` walks head-to-terminal and claims cells first-come, which
    leaves one long trunk path and tributaries that stop where they meet it.
    The generator emits REACHES — split at every confluence — and reach
    tracing makes more endpoints, which are exempt.

    And the GRID. The skeleton routes at 8 m; tracing the corpus at its
    native 2 m resolves the same catchment into four times the cells and
    (measured) 464 reaches against our ~100, which changes both the pair
    count and the exemption rate. Downsample first, then trace.
    """
    step = max(int(round(grid_m / cell)), 1)
    if step > 1:
        z = z[::step, ::step]
        if developed is not None:
            developed = developed[::step, ::step]
        cell = cell * step
    zfill = mcore.fill_depressions(z, cell)
    rec, _ = flow.receivers(zfill, cell)
    acc = flow.accumulate(rec).astype(float) * cell * cell
    lake = zfill > z + 0.01
    ch = acc >= extract_v2.CHANNEL_AREA_M2
    ch &= ~lake | (acc >= 4.0 * extract_v2.CHANNEL_AREA_M2)
    if developed is not None:
        ch &= ~developed
    ny, nx = z.shape
    r = rec.ravel()
    cf = ch.ravel()
    idx = np.nonzero(cf)[0]
    donors = np.zeros(ny * nx, dtype=np.int32)
    tgt = r[idx]
    ok = (tgt >= 0) & cf[np.where(tgt >= 0, tgt, 0)]
    np.add.at(donors, tgt[ok], 1)
    out = []
    for s in idx:
        if donors[s] == 1:                    # mid-reach cell, not a start
            continue
        path = [s]
        cur = s
        while True:
            nxt = r[cur]
            if nxt < 0 or not cf[nxt]:
                break
            path.append(nxt)
            if donors[nxt] >= 2:              # the confluence starts its own
                break
            cur = nxt
        if len(path) >= 2:
            ys, xs = np.divmod(np.array(path), nx)
            out.append(np.stack([xs * cell, ys * cell], axis=1).astype(float))
    return out


def near_parallel(reaches, cell):
    """(near-parallel length, total length) in metres on the 20 m grid."""
    s = [r for r in (smooth_resample(p, cell, NEAR_SAMPLE_M) for p in reaches)
         if r is not None and len(r)]
    if len(s) < 2:
        return 0.0, sum(len(x) for x in s) * NEAR_SAMPLE_M
    pts = np.concatenate(s)
    ids = np.concatenate([np.full(len(x), i) for i, x in enumerate(s)])
    tree = cKDTree(pts)
    ends = np.array([[x[0], x[-1]] for x in s])          # (nreach, 2, 2)
    near = 0.0
    for i, si in enumerate(s):
        # every sample within the band, then drop own-reach hits and the
        # ones sitting in another reach's junction neighbourhood
        hits = tree.query_ball_point(si, NEAR_BAND_M)
        for k, hs in enumerate(hits):
            if not hs:
                continue
            other = np.unique(ids[np.asarray(hs, dtype=int)])
            other = other[other != i]
            if not len(other):
                continue
            d_end = np.hypot(*(ends[other] - si[k]).T).min(axis=0)
            if (d_end > NEAR_END_EXEMPT_M).any():
                near += NEAR_SAMPLE_M
    return near, sum(len(x) for x in s) * NEAR_SAMPLE_M


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
        near_len = near_total = 0.0
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
            reaches = trace_reaches(z, cell, dev)
            nl, nt = near_parallel(reaches, 8.0)
            near_len += nl
            near_total += nt
            print(f"  [{biome}] {tid}: {len(paths)} paths, {len(reaches)} reaches, "
                  f"near_par {100 * nl / max(nt, 1e-9):.1f}%", file=sys.stderr)
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
        rec["near_par_frac"] = near_len / near_total if near_total else 0.0
        rec["net_km"] = near_total / 1000.0 / max(len(tiles), 1)
        rec["n_tiles"] = len(tiles)
        out[biome] = rec
    print(json.dumps(out, indent=1))


if __name__ == "__main__":
    main()
