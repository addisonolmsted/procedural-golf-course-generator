"""Real outlet spacing as a function of how BIG an outlet has to be.

Our step-2 trunks are the major systems only. The 4e5 m2 threshold catches
quite small ones, so the tight 16-64 m pairs it reports may be minor systems
that would be step-4 tributaries for us, not trunks. Sweep the threshold.
"""
import sys, pathlib
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT/"tools/macro_campaign")); sys.path.insert(0, str(ROOT/"tools/metrics"))
from macro_campaign import cgrid, extract_v2, flow
from metrics import core as mcore
CELL=8.0
THS=[4e5, 1e6, 2e6, 4e6]
print(f"{'thresh m2':>10} {'per edge':>9} {'gap p10':>8} {'p50':>7} {'p90':>7} {'min':>6} {'n':>5}")
acc_by_th={t:[] for t in THS}; pe_by_th={t:[] for t in THS}
for b in ["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]:
    for tid in [t for a,t in extract_v2.kept_tiles() if a==b][:14]:
        p=extract_v2.OUT/"tiles"/b/f"{tid}.cgrid"
        if not p.exists(): continue
        z,(_,_,c)=cgrid.read_f32(p); z=z.astype(np.float64); m=np.isfinite(z)
        if m.mean()<0.95: continue
        z=np.where(m,z,np.nanmean(z[m]))
        step=max(int(round(CELL/c)),1); z8=z[::step,::step]
        zf=mcore.fill_depressions(z8,CELL); rec,_=flow.receivers(zf,CELL)
        acc=flow.accumulate(rec).astype(float)*CELL*CELL
        for TH in THS:
            for border in (acc[0,:],acc[-1,:],acc[:,0],acc[:,-1]):
                idx=np.flatnonzero(border>=TH)
                if not len(idx): pe_by_th[TH].append(0); continue
                runs,start=[],idx[0]
                for a_,b_ in zip(idx,idx[1:]):
                    if b_!=a_+1: runs.append((start+a_)/2.0); start=b_
                runs.append((start+idx[-1])/2.0)
                pe_by_th[TH].append(len(runs))
                for x,y in zip(runs,runs[1:]): acc_by_th[TH].append((y-x)*CELL)
for TH in THS:
    g=acc_by_th[TH]; q=lambda p: np.percentile(g,p) if g else float('nan')
    print(f"{TH:>10.0e} {np.mean(pe_by_th[TH]):>9.2f} {q(10):>8.0f} {q(50):>7.0f} {q(90):>7.0f} "
          f"{(min(g) if g else float('nan')):>6.0f} {len(g):>5}", flush=True)
