"""Creek planform measurement — the ROUTING metric, on finished tiles.

MEASUREMENT ONLY. Nothing here affects generation; these are the instruments
behind docs/calibration/creek-planform.md, kept so the numbers can be
re-derived rather than trusted.

  SCRATCH=<base dir> python3 creek_crossings.py <subdir> <seeds_file> [label] [prefix=tex_]

Reads <SCRATCH>/<subdir>/<prefix><seed>.water.cgrid. Two numbers per seed:

  multi-crossing  share of 350 m chords, ANGLED >= 35 deg to the local creek
                  axis, that cross the creek more than once. Random-bearing
                  chords read 21.9% where angled ones read 2.8% (the doc's
                  measurement trap 2): a chord nearly parallel to a creek
                  re-crosses whatever the creek's shape, and no router places
                  a hole that way.
  amplitude p95   lateral amplitude about the creek's own axis, detrended at
                  8% of length so the VALLEY's wander is not counted.
"""
import sys, os, pathlib
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "macro_campaign"))
import numpy as np
from scipy import ndimage
from macro_campaign import cgrid

def creek_mask(path, pond_w_m=10.0):
    """(union, thin, cell): `union` is every wet body that carries a creek --
    the creek AND any pond it runs through, for the body count; `thin` is the
    creek alone (bodies wider than pond_w_m removed by morphological opening),
    which is what the planform metrics are computed on. An intact creek that
    runs through its own impoundments is ONE union body; measuring the union
    would read the pond's width as meander amplitude."""
    w, (_, _, c) = cgrid.read_f32(path)
    wet = np.isfinite(w)
    if not wet.any(): return None, None, c
    k = max(int(round(pond_w_m / c)), 3)
    ponds = ndimage.binary_opening(wet, structure=np.ones((k, k)))
    ponds = ndimage.binary_dilation(ponds, structure=np.ones((3, 3)), iterations=2)
    thin_all = wet & ~ponds
    lab, n = ndimage.label(wet, structure=np.ones((3, 3)))
    union = np.zeros_like(wet); thin = np.zeros_like(wet)
    for i in range(1, n + 1):
        m = lab == i
        t = m & thin_all
        ys, xs = np.where(t)
        if len(ys) == 0: continue
        span = max(np.ptp(ys), np.ptp(xs)) * c
        if span < 400 or len(ys) * c * c < 400: continue
        union |= m; thin |= t
    if not thin.any(): return None, None, c
    return union, thin, c

def creek_bodies(wet):
    """How many connected pieces the creek is in (8-connected). 1 = intact."""
    _, n = ndimage.label(wet, structure=np.ones((3, 3)))
    return n

def local_axis_angles(wet, cell, r_m=30.0):
    """Per creek cell, the bearing of the creek's local axis (PCA of wet cells
    within r_m). Vectorised with a KD-tree-free trick: smooth the wet mask's
    structure tensor."""
    ys, xs = np.where(wet)
    # structure tensor of the wet mask, blurred at r_m
    sig = r_m / cell / 2.0
    W = wet.astype(float)
    yy, xx = np.indices(wet.shape)
    m00 = ndimage.gaussian_filter(W, sig)
    m10 = ndimage.gaussian_filter(W * xx, sig); m01 = ndimage.gaussian_filter(W * yy, sig)
    m20 = ndimage.gaussian_filter(W * xx * xx, sig); m02 = ndimage.gaussian_filter(W * yy * yy, sig)
    m11 = ndimage.gaussian_filter(W * xx * yy, sig)
    mu_x = m10 / np.maximum(m00, 1e-9); mu_y = m01 / np.maximum(m00, 1e-9)
    cxx = m20 / np.maximum(m00, 1e-9) - mu_x ** 2
    cyy = m02 / np.maximum(m00, 1e-9) - mu_y ** 2
    cxy = m11 / np.maximum(m00, 1e-9) - mu_x * mu_y
    ang = 0.5 * np.arctan2(2 * cxy, cxx - cyy)   # principal axis angle
    return ang[ys, xs], ys, xs

def multi_cross_rate(wet, cell, hole_m=350.0, min_angle_deg=35.0, n=4000, seed=3):
    """Fraction of creek-crossing chords (angled >= min_angle_deg to the local
    creek axis) that cross MORE than once."""
    rng = np.random.default_rng(seed)
    ang, ys, xs = local_axis_angles(wet, cell)
    if len(ys) < 50: return None
    ny, nx = wet.shape
    once = multi = 0
    t = np.linspace(-hole_m / 2, hole_m / 2, int(hole_m / 2.0))
    for _ in range(n):
        k = rng.integers(len(ys))
        cy, cx = ys[k] * cell, xs[k] * cell
        rel = np.radians(rng.uniform(min_angle_deg, 180.0 - min_angle_deg))
        th = ang[k] + rel
        py = cy + np.sin(th) * t; px = cx + np.cos(th) * t
        yi = (py / cell).astype(int); xi = (px / cell).astype(int)
        ok = (yi >= 0) & (yi < ny) & (xi >= 0) & (xi < nx)
        if ok.sum() < len(t) * 0.8: continue
        v = wet[yi[ok], xi[ok]].astype(int)
        runs = int(((np.diff(v) == 1).sum()) + (1 if v[0] else 0))
        if runs >= 1: once += 1
        if runs >= 2: multi += 1
    return (multi / once * 100) if once else None

def amplitude(wet, cell):
    """Lateral half-amplitude: p95 distance from the creek's own smoothed axis."""
    ys, xs = np.where(wet)
    P = np.stack([ys * cell, xs * cell], 1).astype(float)
    Pc = P - P.mean(0)
    u = np.linalg.svd(Pc, full_matrices=False)[2][0]
    t = Pc @ u; nvec = np.array([-u[1], u[0]]); off = Pc @ nvec
    o = np.argsort(t); t2, o2 = t[o], off[o]
    win = max(int(len(t2) * 0.08), 5)
    base = np.convolve(o2, np.ones(win) / win, mode="same")
    return float(np.percentile(np.abs(o2 - base), 95))

def measure_dir(base, subdir, seeds, prefix="tex_"):
    rows = []
    for s in seeds:
        p = pathlib.Path(base) / subdir / f"{prefix}{s}.water.cgrid"
        if not p.exists(): continue
        union, wet, c = creek_mask(p)
        if wet is None: continue
        r = multi_cross_rate(wet, c); a = amplitude(wet, c); b = creek_bodies(union)
        rows.append((s, r, a, b))
    return rows

if __name__ == "__main__":
    SC = pathlib.Path(os.environ["SCRATCH"])
    d = sys.argv[1]; seeds = pathlib.Path(sys.argv[2]).read_text().split()
    label = sys.argv[3] if len(sys.argv) > 3 else d
    prefix = sys.argv[4] if len(sys.argv) > 4 else "tex_"
    rows = measure_dir(SC, d, seeds, prefix)
    print(f"{label}:")
    mc, am = [], []
    for s, r, a, b in rows:
        if r is not None:
            mc.append(r); am.append(a)
        print(f"  {s}: multi-crossing(>=35deg) {r if r is None else round(r,1)}%   "
              f"lateral amplitude p95 {a:5.1f} m   creek bodies {b}")
    if mc:
        print(f"  MEDIAN: multi-crossing {np.median(mc):.1f}%   amplitude {np.median(am):.1f} m")
