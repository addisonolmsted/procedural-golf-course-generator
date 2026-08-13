"""Does meander wavelength scale with discharge?

Leopold & Wolman: meander wavelength ~ 10-14 x channel width, and width
scales with the square root of discharge — so a big river should wander at
a LONG wavelength and a small stream at a short one. This measures that
scaling identically on real corpus tiles and on generated carved surfaces
(dumped by course-skeleton's carve_dump example).

Method: trace every channel path downstream through the D8 receivers, bin
each path by the drained area it carries, and for each bin report
sinuosity measured over several window lengths. A population whose
sinuosity survives to long windows is meandering at long wavelength; one
whose sinuosity collapses by 800 m is only wiggling locally.

    python tools/macro_campaign/meander_scaling.py <generated_dump_dir>
"""
import sys
import pathlib

import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT.parent / "metrics"))
from macro_campaign import cgrid, extract_v2, flow  # noqa: E402
from metrics import core as mcore  # noqa: E402

WINDOWS = [200.0, 400.0, 800.0, 1600.0]
# discharge classes, m^2 of drained area
BINS = [(1.0e5, 4.0e5, "small"), (4.0e5, 2.0e6, "medium"), (2.0e6, 1e12, "large")]
CELL = 8.0


def trace_paths(rec, acc, nx, ny, thresh=1.0e5):
    """Downstream paths from every channel head, with carried area."""
    ch = acc >= thresh
    flat_rec = rec.ravel()
    ch_flat = ch.ravel()
    donors = np.zeros(nx * ny, dtype=np.int32)
    idx = np.nonzero(ch_flat)[0]
    tgt = flat_rec[idx]
    ok = tgt >= 0
    np.add.at(donors, tgt[ok][ch_flat[tgt[ok]]], 1)
    visited = np.zeros(nx * ny, dtype=bool)
    out = []
    for s in idx:
        if donors[s] != 0:
            continue
        path = []
        i = s
        while i >= 0 and ch_flat[i] and not visited[i]:
            visited[i] = True
            path.append(i)
            i = flat_rec[i]
        if len(path) >= 20:
            ys, xs = np.divmod(np.array(path), nx)
            pts = np.stack([xs * CELL, ys * CELL], axis=1).astype(float)
            out.append((pts, float(acc.ravel()[path[-1]])))
    return out


def sinuosity(pts, win_m):
    k = max(3, int(round(50.0 / CELL)) | 1)
    if len(pts) <= k:
        return []
    sm = np.stack([ndimage.uniform_filter1d(pts[:, 0], k, mode="nearest"),
                   ndimage.uniform_filter1d(pts[:, 1], k, mode="nearest")], axis=1)
    d = np.hypot(*np.diff(sm, axis=0).T)
    arc = np.concatenate([[0.0], np.cumsum(d)])
    if arc[-1] < win_m * 1.2:
        return []
    t = np.arange(0.0, arc[-1], 10.0)
    rs = np.stack([np.interp(t, arc, sm[:, 0]), np.interp(t, arc, sm[:, 1])], axis=1)
    n = int(win_m / 10.0)
    out = []
    for i in range(0, len(rs) - n, 5):
        chord = float(np.hypot(*(rs[i + n] - rs[i])))
        out.append(win_m / max(chord, 1e-9))
    return out


def collect(paths):
    acc = {b[2]: {w: [] for w in WINDOWS} for b in BINS}
    for pts, area in paths:
        label = next((b[2] for b in BINS if b[0] <= area < b[1]), None)
        if label is None:
            continue
        for w in WINDOWS:
            acc[label][w] += sinuosity(pts, w)
    return acc


def report(title, acc):
    print(f"\n{title}")
    print(f"  {'class':7s} " + " ".join(f"{int(w):>6d}m" for w in WINDOWS) + "   (median sinuosity)")
    for _, _, label in BINS:
        row = []
        for w in WINDOWS:
            v = acc[label][w]
            row.append(f"{np.median(v):7.3f}" if len(v) > 30 else "      -")
        print(f"  {label:7s} " + " ".join(row))


def main():
    gen_dir = pathlib.Path(sys.argv[1])
    for biome in ["river_valley", "piedmont", "hill_country"]:
        # ---- real ----
        racc = {b[2]: {w: [] for w in WINDOWS} for b in BINS}
        for tid in [t for a, t in extract_v2.kept_tiles() if a == biome][:8]:
            z, (_, _, cell) = cgrid.read_f32(extract_v2.OUT / "tiles" / biome / f"{tid}.cgrid")
            z = z.astype(np.float64)
            m = np.isfinite(z)
            if m.mean() < 0.95:
                continue
            z8 = np.where(m, z, np.nanmean(z[m]))[::4, ::4]
            zf = mcore.fill_depressions(z8, CELL)
            rec, _ = flow.receivers(zf, CELL)
            acc = flow.accumulate(rec).astype(float) * CELL * CELL
            ny, nx = z8.shape
            got = collect(trace_paths(rec, acc, nx, ny))
            for lab in racc:
                for w in WINDOWS:
                    racc[lab][w] += got[lab][w]
        report(f"REAL {biome}", racc)

        # ---- generated ----
        gacc = {b[2]: {w: [] for w in WINDOWS} for b in BINS}
        for f in sorted(gen_dir.glob(f"{biome}_*.z.f32")):
            raw = np.frombuffer(f.read_bytes(), dtype=np.float32)
            side = int(round(len(raw) ** 0.5))
            z8 = raw.reshape(side, side).astype(float)
            a = np.frombuffer((gen_dir / f.name.replace(".z.", ".a.")).read_bytes(),
                              dtype=np.float32).reshape(side, side).astype(float)
            zf = mcore.fill_depressions(z8, CELL)
            rec, _ = flow.receivers(zf, CELL)
            ny, nx = z8.shape
            got = collect(trace_paths(rec, a, nx, ny))
            for lab in gacc:
                for w in WINDOWS:
                    gacc[lab][w] += got[lab][w]
        report(f"GEN  {biome}", gacc)


if __name__ == "__main__":
    main()
