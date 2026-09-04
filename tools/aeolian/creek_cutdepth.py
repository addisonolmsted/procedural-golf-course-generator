"""Cut depth along a creek: how far the ground beside the water stands above it.

  python3 tools/aeolian/creek_cutdepth.py <dir> <seed>... [--vs <dir2>] [--prefix a_] [--json out.json]

Per creek node (resampled at 2 m): level = the water surface at the node,
ground = the mean of the surface 12 m either side of the line, cut = ground
minus level. This is the number that caught the mound on Nebraska 600032
(17 m: the descending water level held flat under a dune train the gorge had
not cut through). Also reported: the water grade over 10 m and the steepest
ground slope within 30 m of the creek, both as percentiles along the creek.

With `--vs` the same seeds are measured in a second directory and the two are
printed side by side (before | after). Measurement only; importable.
"""
import sys, json, pathlib
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "macro_campaign"))
from macro_campaign import cgrid

SIDE_M, CORRIDOR_M, GRADE_M = 12.0, 30.0, 10.0


def load(d, seed, prefix):
    d = pathlib.Path(d)
    z, (_, _, c) = cgrid.read_f32(d / f"{prefix}{seed}.cgrid")
    w, _ = cgrid.read_f32(d / f"{prefix}{seed}.water.cgrid")
    line = np.loadtxt(d / f"{prefix}{seed}.creek.txt")
    return z.astype(float), w.astype(float), float(c), line


def resample(line, step=2.0):
    seg = np.hypot(*np.diff(line, axis=0).T)
    cum = np.r_[0.0, np.cumsum(seg)]
    ss = np.arange(0.0, cum[-1], step)
    P = np.stack([np.interp(ss, cum, line[:, 0]), np.interp(ss, cum, line[:, 1])], 1)
    return ss, P


def sample(z, c, x, y):
    """Bilinear sample of z[y, x] at metres (x, y); clamped to the grid."""
    n_y, n_x = z.shape
    fx = np.clip(x / c, 0, n_x - 1.001); fy = np.clip(y / c, 0, n_y - 1.001)
    ix = fx.astype(int); iy = fy.astype(int); tx = fx - ix; ty = fy - iy
    return (z[iy, ix] * (1 - tx) * (1 - ty) + z[iy, ix + 1] * tx * (1 - ty)
            + z[iy + 1, ix] * (1 - tx) * ty + z[iy + 1, ix + 1] * tx * ty)


def profile(z, w, c, line):
    """-> dict of per-node arrays: arc, level, ground, cut, grade, and the
    corridor slope percentiles."""
    ss, P = resample(line)
    T = np.gradient(P, axis=0); T /= np.hypot(*T.T)[:, None] + 1e-12
    N = np.stack([-T[:, 1], T[:, 0]], 1)
    # the water level at the node: the wet surface, taken at the nearest wet
    # cell within a few cells (the line sits on the wet ribbon)
    wet = np.isfinite(w)
    ix = np.clip((P[:, 0] / c).round().astype(int), 0, z.shape[1] - 1)
    iy = np.clip((P[:, 1] / c).round().astype(int), 0, z.shape[0] - 1)
    level = np.full(len(P), np.nan)
    for r in range(0, 4):
        miss = ~np.isfinite(level)
        if not miss.any():
            break
        for dy in range(-r, r + 1):
            for dx in range(-r, r + 1):
                jy = np.clip(iy + dy, 0, z.shape[0] - 1); jx = np.clip(ix + dx, 0, z.shape[1] - 1)
                ok = miss & wet[jy, jx]
                level[ok] = w[jy[ok], jx[ok]]
                miss = ~np.isfinite(level)
    gl = sample(z, c, P[:, 0] + N[:, 0] * SIDE_M, P[:, 1] + N[:, 1] * SIDE_M)
    gr = sample(z, c, P[:, 0] - N[:, 0] * SIDE_M, P[:, 1] - N[:, 1] * SIDE_M)
    ground = 0.5 * (gl + gr)
    ok = np.isfinite(level)
    cut = np.where(ok, ground - level, np.nan)
    k = int(GRADE_M / 2.0)
    lv = np.where(ok, level, np.nan)
    grade = np.full(len(P), np.nan)
    grade[k:] = np.abs(lv[k:] - lv[:-k]) / GRADE_M * 100.0
    # ground slope in the corridor
    gy, gx = np.gradient(z, c)
    slope = np.hypot(gx, gy) * 100.0
    yy, xx = np.mgrid[0:z.shape[0], 0:z.shape[1]]
    corr = np.zeros(z.shape, bool)
    rc = int(CORRIDOR_M / c)
    for (x, y) in zip(ix, iy):
        corr[max(0, y - rc):y + rc + 1, max(0, x - rc):x + rc + 1] = True
    cs = slope[corr & ~wet]
    return dict(arc=ss, level=lv, ground=ground, cut=cut, grade=grade,
                corridor_slope=cs, xy=P)


def stats(pr):
    cut = pr["cut"][np.isfinite(pr["cut"])]
    gr = pr["grade"][np.isfinite(pr["grade"])]
    cs = pr["corridor_slope"]
    q = lambda v, f: float(np.percentile(v, f)) if len(v) else float("nan")
    i_max = int(np.nanargmax(pr["cut"])) if np.isfinite(pr["cut"]).any() else 0
    return dict(cut_p50=q(cut, 50), cut_p90=q(cut, 90), cut_max=q(cut, 100),
                grade_p90=q(gr, 90), grade_max=q(gr, 100),
                slope_p90=q(cs, 90), slope_p99=q(cs, 99),
                deepest_xy=[float(pr["xy"][i_max, 0]), float(pr["xy"][i_max, 1])],
                deepest_arc=float(pr["arc"][i_max]), length_m=float(pr["arc"][-1]))


def measure(d, seed, prefix="a_"):
    return stats(profile(*load(d, seed, prefix)))


def main():
    a = sys.argv[1:]
    prefix, vs, js = "a_", None, None
    for k in ("--prefix", "--vs", "--json"):
        if k in a:
            i = a.index(k); v = a[i + 1]; a = a[:i] + a[i + 2:]
            if k == "--prefix": prefix = v
            elif k == "--vs": vs = v
            else: js = v
    d, seeds = a[0], a[1:]
    out = {}
    cols = ["cut_p50", "cut_p90", "cut_max", "grade_p90", "grade_max", "slope_p90", "slope_p99"]
    hdr = "seed    " + "".join(f"{c:>22}" for c in cols)
    print(hdr)
    for s in seeds:
        try:
            b = measure(d, s, prefix)
        except Exception as e:
            print(f"{s}: {e}"); continue
        row = {"before": b} if vs else {"after": b}
        if vs:
            try:
                row["after"] = measure(vs, s, prefix)
            except Exception as e:
                print(f"{s}: {e}"); continue
        out[s] = row
        line = f"{s:<8}"
        for c in cols:
            if vs:
                line += f"{row['before'][c]:>10.1f} |{row['after'][c]:>9.1f} "
            else:
                line += f"{row['after'][c]:>22.1f}"
        print(line)
    if out:
        agg = {}
        for key in out[next(iter(out))]:
            agg[key] = {c: float(np.median([r[key][c] for r in out.values()])) for c in cols}
            agg[key]["cut_max_worst"] = float(max(r[key]["cut_max"] for r in out.values()))
            print(f"median over {len(out)} seeds, {key}: " +
                  " ".join(f"{c} {agg[key][c]:.1f}" for c in cols) +
                  f"  worst cut_max {agg[key]['cut_max_worst']:.1f}")
    if js:
        json.dump({"seeds": out, "units": "m and % ; cut = ground 12 m off the line minus the water level"},
                  open(js, "w"), indent=1)


if __name__ == "__main__":
    main()
