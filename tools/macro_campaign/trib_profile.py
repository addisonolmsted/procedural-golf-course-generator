"""What does a TRIBUTARY-valley cross-section look like, per biome? (T2)

Same rig as valley_profile.py, but transects channels in the tributary
accumulation band (channel threshold 6e4 m^2 up to 6e5 m^2 — below the
1e6 trunk class) and reads at short distances. These are the streams that
link up to trunk-class valleys, i.e. exactly the tier the T2 catena cut
models. Reports the empirical rise(d), an effective side slope over the
rising limb, the floor half-width, and the depth at which the profile
flattens into the surrounding slope (valley depth ~ where rise stops
growing).
"""
import sys, pathlib, json
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT/"tools/macro_campaign")); sys.path.insert(0, str(ROOT/"tools/metrics"))
from macro_campaign import cgrid, extract_v2, flow
from metrics import core as mcore

CELL=8.0
TRIB_LO=6.0e4; TRIB_HI=6.0e5
DISTS=[8, 16, 24, 40, 60, 90, 130, 180, 260, 360]
BIOMES=["piedmont","great_plains","river_valley","hill_country","heathland"]
NT=int(sys.argv[1]) if len(sys.argv)>1 else 12

print(f"{'biome':14s} {'n_tr':>6s} | rise above channel (m) at d=8/16/24/40/60/90/130/180/260/360")
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
        ch=np.argwhere((acc>=TRIB_LO)&(acc<TRIB_HI))
        if len(ch)<10: continue
        recf=rec.ravel()
        for (y,x) in ch[::max(1,len(ch)//80)]:
            i=y*nx+x; r=recf[i]
            if r<0: continue
            dy,dx=(r//nx)-y,(r%nx)-x
            n=np.hypot(dx,dy)
            if n<1e-9: continue
            px,py=-dy/n,dx/n
            z0=z8[y,x]
            for sgn in (1,-1):
                for d in DISTS:
                    yy=int(round(y+py*sgn*d/CELL)); xx=int(round(x+px*sgn*d/CELL))
                    if 0<=yy<ny and 0<=xx<nx:
                        prof[d].append(z8[yy,xx]-z0)
    med={d:(float(np.median(prof[d])) if prof[d] else np.nan) for d in DISTS}
    q75={d:(float(np.percentile(prof[d],75)) if prof[d] else np.nan) for d in DISTS}
    # floor half-width: first d where median rise > 0.8 m
    fhw=next((d for d in DISTS if med[d]>0.8), DISTS[-1])
    # depth: where the rise curve flattens (gain over the next step < 15%
    # of the running rise) -- the notch has merged into the hillslope
    depth=med[DISTS[-1]]; wall_end=DISTS[-1]
    for a,bb in zip(DISTS[2:],DISTS[3:]):
        if med[a]>1.0 and (med[bb]-med[a])<0.15*med[a]:
            depth=med[a]; wall_end=a; break
    # effective side slope over the rising limb (fhw -> wall_end)
    slope=(depth-med.get(fhw,0.0))/max(wall_end-fhw,1) if depth>0 else float('nan')
    rows[b]=dict(prof=med, q75=q75, floor_hw=fhw, depth=depth, wall_end=wall_end,
                 slope=slope, n=len(prof[60]))
    pr="/".join(f"{med[d]:.1f}" for d in DISTS)
    print(f"{b:14s} {len(prof[60]):6d} | {pr}  | floor_hw {fhw:3d} m  depth {depth:4.1f} m @ {wall_end} m  side {100*slope:4.1f}%", flush=True)
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent/"trib_profile.json","w"), indent=1)
