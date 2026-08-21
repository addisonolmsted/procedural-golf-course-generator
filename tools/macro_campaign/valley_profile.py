"""What does a trunk-valley cross-section actually look like, per biome?

For each tile: extract trunk-class channel cells (acc >= 1e6 m2), walk
transects perpendicular to flow, record height-above-channel at fixed
distances. Aggregate per biome: the empirical rise(d) profile, its power-law
exponent, floor half-width, and wall slope. These become the T1 catena record
ranges, with provenance = corpus.
"""
import sys, pathlib, json
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT/"tools/macro_campaign")); sys.path.insert(0, str(ROOT/"tools/metrics"))
from macro_campaign import cgrid, extract_v2, flow
from metrics import core as mcore

CELL=8.0; TRUNK=1.0e6
DISTS=[24, 48, 100, 160, 240, 400, 600]
BIOMES=["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]
NT=int(sys.argv[1]) if len(sys.argv)>1 else 12

print(f"{'biome':14s} {'n_tr':>5s} | rise above channel (m) at d=24/48/100/160/240/400/600 | {'exp':>5s} {'floor_hw':>8s}")
rows={}
for b in BIOMES:
    prof={d:[] for d in DISTS}
    for tid in [t for a,t in extract_v2.kept_tiles() if a==b][:NT]:
        p=extract_v2.OUT/"tiles"/b/f"{tid}.cgrid"
        if not p.exists(): continue
        z,(_,_,c)=cgrid.read_f32(p); z=z.astype(np.float64); m=np.isfinite(z)
        if m.mean()<0.95: continue
        z=np.where(m,z,np.nanmean(z[m]))
        step=max(int(round(CELL/c)),1); z8=z[::step,::step]
        zf=mcore.fill_depressions(z8,CELL)
        rec,_=flow.receivers(zf,CELL)
        acc=flow.accumulate(rec).astype(float)*CELL*CELL
        ny,nx=z8.shape
        ch=np.argwhere(acc>=TRUNK)
        if len(ch)<10: continue
        # subsample channel cells; flow dir from receiver
        recf=rec.ravel()
        for (y,x) in ch[::max(1,len(ch)//60)]:
            i=y*nx+x; r=recf[i]
            if r<0: continue
            dy,dx=(r//nx)-y,(r%nx)-x
            n=np.hypot(dx,dy)
            if n<1e-9: continue
            # perpendicular
            px,py=-dy/n,dx/n
            z0=z8[y,x]
            for sgn in (1,-1):
                for d in DISTS:
                    yy=int(round(y+py*sgn*d/CELL)); xx=int(round(x+px*sgn*d/CELL))
                    if 0<=yy<ny and 0<=xx<nx:
                        prof[d].append(z8[yy,xx]-z0)
    med={d:(float(np.median(prof[d])) if prof[d] else np.nan) for d in DISTS}
    # power exponent from 100->400 m
    if med[100]>0.3 and med[400]>med[100]:
        expo=float(np.log(med[400]/med[100])/np.log(4.0))
    else:
        expo=float('nan')
    # floor half-width: first d where median rise > 1.0 m
    fhw=next((d for d in DISTS if med[d]>1.0), DISTS[-1])
    rows[b]=dict(prof=med, exp=expo, floor_hw=fhw, n=len(prof[100]))
    pr="/".join(f"{med[d]:.1f}" for d in DISTS)
    print(f"{b:14s} {len(prof[100]):5d} | {pr:47s} | {expo:5.2f} {fhw:8d}", flush=True)
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent/"valley_profile.json","w"), indent=1)
