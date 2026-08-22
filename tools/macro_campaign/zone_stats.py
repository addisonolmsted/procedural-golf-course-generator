"""ZONE-RESOLVED transect statistics — the answer to parameter degeneracy.

Aggregate slope/relief cannot tell a 10 m tread from a 1000 m tread. This
segments real cross-valley profiles into RUNS — flat (tread), steep (riser),
ramp (slope) — and measures the distributions of their widths and heights
directly. Those ARE the section-zone dials.

Per biome, at trunk-class channels (acc >= 1e6 m2), perpendicular transects
out to 1000 m: local slope over 24 m windows classifies each sample
(<3% flat, >15% steep, else ramp); runs >= 24 m are recorded.
"""
import sys, pathlib, json
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT/"tools/macro_campaign")); sys.path.insert(0, str(ROOT/"tools/metrics"))
from macro_campaign import cgrid, extract_v2, flow
from metrics import core as mcore

CELL=8.0; TRUNK=1.0e6; OUT_M=1000.0
BIOMES=["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]
NT=int(sys.argv[1]) if len(sys.argv)>1 else 12

def runs_of(prof):
    """prof: z at 8 m steps from channel. -> list of (kind, width_m, dz)"""
    if len(prof)<6: return []
    sl=np.abs(np.gradient(prof, CELL))
    # smooth slope over 3 samples (24 m)
    sl=np.convolve(sl,[1/3]*3,mode='same')
    kind=np.where(sl<0.03,0,np.where(sl>0.15,2,1))
    out=[]; s0=0
    for i in range(1,len(kind)+1):
        if i==len(kind) or kind[i]!=kind[s0]:
            w=(i-s0)*CELL
            if w>=24:
                out.append((int(kind[s0]), w, float(prof[min(i,len(prof)-1)]-prof[s0])))
            s0=i
    return out

rows={}
print(f"{'biome':14s} | {'tread w p10/50/90':>20s} {'n':>5s} | {'riser h p10/50/90':>19s} {'grade50':>8s} {'n':>5s} | {'risers/km':>9s}")
for b in BIOMES:
    treads=[]; risers=[]; grades=[]; n_tr=0; total_transect_km=0.0
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
        ny,nx=z8.shape; recf=rec.ravel()
        ch=np.argwhere(acc>=TRUNK)
        if len(ch)<10: continue
        for (y,x) in ch[::max(1,len(ch)//40)]:
            i=y*nx+x; r=recf[i]
            if r<0: continue
            dy,dx=(r//nx)-y,(r%nx)-x
            n=np.hypot(dx,dy)
            if n<1e-9: continue
            px,py=-dy/n,dx/n
            for sgn in (1,-1):
                prof=[]
                for k in range(int(OUT_M/CELL)):
                    yy=int(round(y+py*sgn*k)); xx=int(round(x+px*sgn*k))
                    if not(0<=yy<ny and 0<=xx<nx): break
                    prof.append(z8[yy,xx])
                rs=runs_of(np.array(prof))
                total_transect_km+=len(prof)*CELL/1000.0
                first_flat=True
                for kind,w,dz in rs:
                    if kind==0:
                        if first_flat: first_flat=False; continue  # the floor
                        if w<1500: treads.append(w)
                    elif kind==2 and dz>0.5:
                        risers.append(dz); grades.append(dz/w); n_tr+=1
    q=lambda v,p: float(np.percentile(v,p)) if len(v) else float('nan')
    rows[b]=dict(tread_w=[q(treads,10),q(treads,50),q(treads,90)], n_tread=len(treads),
                 riser_h=[q(risers,10),q(risers,50),q(risers,90)], grade50=q(grades,50),
                 n_riser=len(risers), risers_per_km=n_tr/max(total_transect_km,1e-9))
    r=rows[b]
    print(f"{b:14s} | {r['tread_w'][0]:5.0f}/{r['tread_w'][1]:5.0f}/{r['tread_w'][2]:5.0f} {r['n_tread']:>5d} |"
          f" {r['riser_h'][0]:5.1f}/{r['riser_h'][1]:5.1f}/{r['riser_h'][2]:5.1f} {r['grade50']*100:7.1f}% {r['n_riser']:>5d} |"
          f" {r['risers_per_km']:9.2f}", flush=True)
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent/"zone_stats.json","w"), indent=1)
