"""Honest co-location: only real greens INSIDE the tested window count.
Plus: does our window chooser agree with the architect's ground?"""
import sys, os, json, pathlib
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
for p in ("macro_campaign", "golf", "dtm_primitives"):
    sys.path.insert(0, str(ROOT / "tools" / p))
from macro_campaign import cgrid
import siting, greens as G

SC = pathlib.Path(os.environ["SCRATCH"])
data = json.load(open(SC / "real_greens.json"))
PICK = {"nebraska": ["Wild Horse Golf Club", "Frederick Peak Golf Club",
                     "Pelican Beach Golf Course", "Bayside Golf Club",
                     "Sand Hills Golf Club", "Dismal River Club"],
        "carolina": ["Lake Course", "Mid South Club", "Anderson Creek Club",
                     "Grey Fox", "North Course"]}
rows = []
for region, names in PICK.items():
    play_m = 1200.0 if region == "nebraska" else 800.0
    for c in data[region]:
        if c["name"] not in names: continue
        z, (_, _, cell) = cgrid.read_f32(c["tile"])
        z = np.where(np.isfinite(z), z, np.nanmean(z)).astype(np.float64)
        wp = pathlib.Path(c["tile"]).with_suffix(".water.npy")
        wet = np.load(wp) if wp.exists() else np.zeros(z.shape, bool)
        gs = np.array(c["greens"])
        f = siting.build_fields(z, cell, wet)
        m = siting.build_morphology(f); p = siting.build_persistence(f)

        # window fitted over the DENSEST cluster of real greens (a single course)
        best = None
        for (gy, gx) in gs:
            n = ((np.abs(gs[:,0]-gy) < play_m/2) & (np.abs(gs[:,1]-gx) < play_m/2)).sum()
            if best is None or n > best[0]: best = (n, gy, gx)
        _, cy, cx = best
        wy = float(np.clip(cy-play_m/2, 750, 2250-play_m))
        wx = float(np.clip(cx-play_m/2, 750, 2250-play_m))
        inw = ((gs[:,0]>=wy)&(gs[:,0]<=wy+play_m)&(gs[:,1]>=wx)&(gs[:,1]<=wx+play_m))
        gin = gs[inw]
        if len(gin) < 6: continue
        class S: pass
        s2=S(); s2.window_ij=(int(wy/f.cell),int(wx/f.cell)); s2.window_m=(wy,wx,play_m)
        class CH: pass
        ch=CH(); ch.yx=(wy+play_m/2,wx+play_m/2); ch.reserved_green=ch.yx
        ch.reserved_tee=ch.yx; ch.score=1.0; s2.clubhouse=ch
        pool = G.generate(z, cell, wet, s2, f, m, p, n_target=110)
        q = np.array([a.yx for a in pool]) if pool else np.zeros((0,2))
        d = np.array([np.hypot(q[:,0]-y, q[:,1]-x).min() for (y,x) in gin]) if len(q) else None

        # window agreement: our own choice vs the architect's ground
        sit,_,_,_ = siting.run_siting(z, cell, wet, play_m)
        def terms(y0,x0):
            i0,j0=int(y0/f.cell),int(x0/f.cell); w=int(play_m/f.cell)
            sl=f.slope[i0:i0+w,j0:j0+w]; zz=f.z8[i0:i0+w,j0:j0+w]
            return ((sl<=0.08).mean(), np.percentile(zz,95)-np.percentile(zz,5),
                    f.relief_pos[i0:i0+w,j0:j0+w].mean())
        rc, ro = terms(wy,wx), terms(sit.window_m[0],sit.window_m[1])
        off = np.hypot(sit.window_m[0]-wy, sit.window_m[1]-wx)
        rows.append(dict(region=region, name=c["name"], n_in=int(len(gin)),
                         n_pool=len(pool),
                         med=float(np.median(d)) if d is not None else -1,
                         w60=float((d<=60).mean()*100) if d is not None else -1,
                         w100=float((d<=100).mean()*100) if d is not None else -1,
                         real_calm=rc[0], real_rel=rc[1], real_pos=rc[2],
                         our_calm=ro[0], our_rel=ro[1], our_pos=ro[2],
                         offset=float(off)))
        r=rows[-1]
        print(f"  {c['name'][:28]:30s} in-win {r['n_in']:2d} pool {r['n_pool']:3d}  "
              f"med {r['med']:4.0f}m  <60m {r['w60']:3.0f}%  <100m {r['w100']:3.0f}%  "
              f"| calm real {rc[0]:.2f} ours {ro[0]:.2f}  off {off:.0f}m")
json.dump(rows, open(SC/"overlay_stats.json","w"))
import numpy as np
print("\nMEDIANS")
for reg in ("nebraska","carolina"):
    rr=[r for r in rows if r["region"]==reg]
    if not rr: continue
    print(f"  {reg}: <60m {np.median([r['w60'] for r in rr]):.0f}%  "
          f"<100m {np.median([r['w100'] for r in rr]):.0f}%  "
          f"median dist {np.median([r['med'] for r in rr]):.0f} m")
    print(f"    window calm: architect {np.median([r['real_calm'] for r in rr]):.2f}  "
          f"ours {np.median([r['our_calm'] for r in rr]):.2f}   "
          f"relief {np.median([r['real_rel'] for r in rr]):.0f} vs {np.median([r['our_rel'] for r in rr]):.0f} m")
