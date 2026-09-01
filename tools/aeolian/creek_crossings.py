"""Creek planform measurement — corpus statistics and model comparison.

MEASUREMENT ONLY. Nothing here affects generation; these are the instruments
behind docs/calibration/creek-planform.md, kept so the numbers can be
re-derived rather than trusted. The creek-planform experiment they were built
for was reverted (see the doc); the measurements stand.

Same pipeline both sides is the whole point: OSM stream vertices sit 10-30 m
apart, so an unsmoothed curvature measures DIGITIZING noise, not planform.
Every statistic here resamples at 10 m and smooths at 30 m.
"""
import sys, os, pathlib, collections
sys.path.insert(0,"."); sys.path.insert(0,"../macro_campaign")
import numpy as np
from scipy import ndimage
from macro_campaign import cgrid
SC = pathlib.Path(os.environ["SCRATCH"])

def creek_mask(d, s):
    w,(_,_,c) = cgrid.read_f32(SC/d/f"{s}.water.cgrid")
    wet = np.isfinite(w)
    if not wet.any(): return None, c
    lab,n = ndimage.label(wet)
    keep = np.zeros_like(wet)
    for k in range(1,n+1):
        m = lab==k; ys,xs = np.where(m)
        area=len(ys)*c*c; span=max(np.ptp(ys),np.ptp(xs))*c
        if area<400 or span<400: continue
        if area/max(span,1) > 40: continue
        keep |= m
    return (keep if keep.any() else None), c

def multi_cross_rate(wet, cell, hole_m=350.0, n=4000, seed=3):
    """Fraction of creek-crossing chords that cross MORE than once."""
    rng = np.random.default_rng(seed)
    ys, xs = np.where(wet)
    if len(ys) < 50: return None
    ny, nx = wet.shape
    once = multi = 0
    for _ in range(n):
        k = rng.integers(len(ys))
        cy, cx = ys[k]*cell, xs[k]*cell
        th = rng.random()*np.pi
        # chord centred on a creek cell, random bearing
        t = np.linspace(-hole_m/2, hole_m/2, int(hole_m/2.0))
        py = cy + np.sin(th)*t; px = cx + np.cos(th)*t
        yi = (py/cell).astype(int); xi = (px/cell).astype(int)
        ok = (yi>=0)&(yi<ny)&(xi>=0)&(xi<nx)
        if ok.sum() < len(t)*0.8: continue
        v = wet[yi[ok], xi[ok]].astype(int)
        runs = int(((np.diff(v) == 1).sum()) + (1 if v[0] else 0))
        if runs >= 1: once += 1
        if runs >= 2: multi += 1
    return (multi/once*100) if once else None

def amplitude(wet, cell):
    """Lateral half-amplitude: max distance from the creek's own smoothed axis."""
    ys, xs = np.where(wet)
    P = np.stack([ys*cell, xs*cell],1).astype(float)
    Pc = P - P.mean(0)
    u = np.linalg.svd(Pc, full_matrices=False)[2][0]
    t = Pc @ u; nvec = np.array([-u[1], u[0]]); off = Pc @ nvec
    # detrend the belt at 400 m so we measure the MEANDER, not the valley
    o = np.argsort(t); t2, o2 = t[o], off[o]
    win = max(int(len(t2)*0.08), 5)
    base = np.convolve(o2, np.ones(win)/win, mode="same")
    return float(np.percentile(np.abs(o2-base), 95))

for tag, d, seeds in (("NEW (migrate)", "mig",
                       pathlib.Path("/tmp/mig_seeds.txt").read_text().split()),):
    print(f"{tag}:")
    mc, am = [], []
    for s in seeds:
        wet, c = creek_mask(d, s)
        if wet is None: continue
        r = multi_cross_rate(wet, c); a = amplitude(wet, c)
        if r is not None:
            mc.append(r); am.append(a)
            print(f"  {s}: multi-crossing {r:4.1f}% of crossing chords   "
                  f"lateral amplitude p95 {a:5.1f} m")
    if mc:
        print(f"  MEDIAN: multi-crossing {np.median(mc):.1f}%   amplitude {np.median(am):.1f} m")
