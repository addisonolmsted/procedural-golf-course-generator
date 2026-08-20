"""How far apart are real drainage outlets on a tile border?

Our trunk mouths are placed at >=420 m separation, scored on relief lows. The
review question is whether they cluster. That is only answerable against how
real networks leave a 3 km window, so: extract channels at the declared policy
(8 m, 6e4 m2), find where they cross each border, group contiguous crossings
into outlets, and measure the gap between adjacent outlets on the same edge.

Also reports slope percentiles, which set how wide a bench TREAD is for a
given bed thickness: tread ~= thickness / slope.
"""
import sys, pathlib, json
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT/"tools/macro_campaign")); sys.path.insert(0, str(ROOT/"tools/metrics"))
from macro_campaign import cgrid, extract_v2, flow
from metrics import core as mcore

CELL = 8.0
TH = extract_v2.CHANNEL_AREA_M2
BIOMES = ["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]

def outlets_on(mask1d):
    """Start index of each contiguous run of channel cells along a border."""
    idx = np.flatnonzero(mask1d)
    if not len(idx): return []
    runs, start = [], idx[0]
    for a, b in zip(idx, idx[1:]):
        if b != a + 1:
            runs.append((start + a) / 2.0); start = b
    runs.append((start + idx[-1]) / 2.0)
    return runs

print(f"{'biome':14s} {'n':>3s} | {'outlets/edge':>12s} {'gap p10':>8s} {'p50':>7s} {'p90':>7s} | {'slope p50':>9s} {'p90':>7s}")
rows={}
for b in BIOMES:
    gaps, per_edge, sl50, sl90 = [], [], [], []
    for tid in [t for a,t in extract_v2.kept_tiles() if a==b]:
        p = extract_v2.OUT/"tiles"/b/f"{tid}.cgrid"
        if not p.exists(): continue
        z,(_,_,c) = cgrid.read_f32(p); z=z.astype(np.float64); m=np.isfinite(z)
        if m.mean()<0.95: continue
        z=np.where(m,z,np.nanmean(z[m]))
        step=max(int(round(CELL/c)),1); z8=z[::step,::step]
        zf=mcore.fill_depressions(z8,CELL)
        rec,_=flow.receivers(zf,CELL)
        acc=flow.accumulate(rec).astype(float)*CELL*CELL
        ch = acc >= TH
        for border in (ch[0,:], ch[-1,:], ch[:,0], ch[:,-1]):
            o = outlets_on(border)
            per_edge.append(len(o))
            for x,y in zip(o, o[1:]):
                gaps.append((y-x)*CELL)
        gy,gx=np.gradient(z8,CELL); s=np.hypot(gx,gy)
        sl50.append(float(np.percentile(s,50))); sl90.append(float(np.percentile(s,90)))
    q=lambda v,p: float(np.percentile(v,p)) if len(v) else float('nan')
    rows[b]=dict(n=len(sl50), per_edge=float(np.mean(per_edge)) if per_edge else 0,
                 g10=q(gaps,10), g50=q(gaps,50), g90=q(gaps,90),
                 s50=q(sl50,50), s90=q(sl90,50))
    r=rows[b]
    print(f"{b:14s} {r['n']:3d} | {r['per_edge']:12.1f} {r['g10']:8.0f} {r['g50']:7.0f} {r['g90']:7.0f} | "
          f"{r['s50']*100:8.1f}% {r['s90']*100:6.1f}%", flush=True)
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent/"outlet_probe.json","w"), indent=1)
