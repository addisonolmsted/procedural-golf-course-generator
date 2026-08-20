"""Spacing of MAJOR outlets only -- the fair comparison for our trunks.

The all-outlets measurement counts every order-1 rill that happens to cross a
border; our trunks are only the 1-3 big systems. Restrict to outlets carrying
>= 4e5 m2 (heartland's "large" discharge class) and re-measure.
"""
import sys, pathlib, json
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT/"tools/macro_campaign")); sys.path.insert(0, str(ROOT/"tools/metrics"))
from macro_campaign import cgrid, extract_v2, flow
from metrics import core as mcore
CELL=8.0; MAJOR=4.0e5
print(f"{'biome':14s} {'n':>3s} | {'major/edge':>10s} {'gap p10':>8s} {'p50':>7s} {'p90':>7s} {'min':>6s}")
rows={}
for b in ["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]:
    gaps, per_edge = [], []
    for tid in [t for a,t in extract_v2.kept_tiles() if a==b]:
        p=extract_v2.OUT/"tiles"/b/f"{tid}.cgrid"
        if not p.exists(): continue
        z,(_,_,c)=cgrid.read_f32(p); z=z.astype(np.float64); m=np.isfinite(z)
        if m.mean()<0.95: continue
        z=np.where(m,z,np.nanmean(z[m]))
        step=max(int(round(CELL/c)),1); z8=z[::step,::step]
        zf=mcore.fill_depressions(z8,CELL); rec,_=flow.receivers(zf,CELL)
        acc=flow.accumulate(rec).astype(float)*CELL*CELL
        for border in (acc[0,:],acc[-1,:],acc[:,0],acc[:,-1]):
            idx=np.flatnonzero(border>=MAJOR)
            if not len(idx): per_edge.append(0); continue
            runs,start=[],idx[0]
            for a_,b_ in zip(idx,idx[1:]):
                if b_!=a_+1: runs.append((start+a_)/2.0); start=b_
            runs.append((start+idx[-1])/2.0)
            per_edge.append(len(runs))
            for x,y in zip(runs,runs[1:]): gaps.append((y-x)*CELL)
    q=lambda v,p: float(np.percentile(v,p)) if len(v) else float('nan')
    rows[b]=dict(per_edge=float(np.mean(per_edge)), g10=q(gaps,10), g50=q(gaps,50),
                 g90=q(gaps,90), gmin=min(gaps) if gaps else float('nan'), n=len(gaps))
    r=rows[b]
    print(f"{b:14s} {r['n']:3d} | {r['per_edge']:10.2f} {r['g10']:8.0f} {r['g50']:7.0f} {r['g90']:7.0f} {r['gmin']:6.0f}", flush=True)
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent/"major_outlets.json","w"), indent=1)
