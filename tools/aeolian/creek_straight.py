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

R_MIN = [150.0, 300.0]

def curv_profile(P, resample=10.0, smooth=30.0):
    seg = np.hypot(*np.diff(P, axis=0).T); L = float(seg.sum())
    if L < 400: return None, None
    cum = np.r_[0.0, np.cumsum(seg)]
    ss = np.arange(0, L, resample)
    Q = np.stack([np.interp(ss,cum,P[:,0]), np.interp(ss,cum,P[:,1])],1)
    k = max(int(smooth/resample),1)
    if k > 1:
        ker = np.ones(k)/k
        Q = np.stack([np.convolve(Q[:,0],ker,"valid"), np.convolve(Q[:,1],ker,"valid")],1)
    if len(Q) < 12: return None, None
    v1 = np.diff(Q,axis=0)[:-1]; v2 = np.diff(Q,axis=0)[1:]
    cr = v1[:,0]*v2[:,1]-v1[:,1]*v2[:,0]
    n1 = np.hypot(*v1.T); n2 = np.hypot(*v2.T); n3 = np.hypot(*(v1+v2).T)
    den = n1*n2*n3
    c = np.where(den>1e-9, 2*np.abs(cr)/np.maximum(den,1e-9), 0.0)
    s2 = np.hypot(*np.diff(Q,axis=0).T)
    return c, s2[:len(c)]

def straight_stats(P):
    c, ds = curv_profile(P)
    if c is None: return None
    out = {}
    for rmin in R_MIN:
        flat = c < (1.0/rmin)
        frac = float((ds*flat).sum()/ds.sum())
        runs = []
        i = 0
        while i < len(flat):
            if flat[i]:
                j = i
                while j < len(flat) and flat[j]: j += 1
                runs.append(float(ds[i:j].sum())); i = j
            else: i += 1
        runs = [r for r in runs if r > 20]
        out[rmin] = (frac, float(np.percentile(runs,90)) if runs else 0.0,
                     float(np.max(runs)) if runs else 0.0)
    return out

def agg(rows, label):
    if not rows: print(f"  {label}: none"); return
    print(f"  {label} (n={len(rows)})")
    for rmin in R_MIN:
        fr = np.median([r[rmin][0] for r in rows])*100
        p90 = np.median([r[rmin][1] for r in rows])
        mx = np.median([r[rmin][2] for r in rows])
        print(f"    R>{rmin:.0f} m: {fr:4.1f}% of length straight · "
              f"typical longest-runs p90 {p90:5.0f} m · max {mx:5.0f} m")

mode = sys.argv[1]
if mode == "real":
    from corpus import config, net, geo
    Q = '[out:json][timeout:180];(way["waterway"="stream"]({b}););out geom;'
    rows = []
    for (tag, la, lo, h) in config.SEARCH_BOXES:
        for (qa,qo) in ((la-h/2,lo-h/2),(la-h/2,lo+h/2),(la+h/2,lo-h/2),(la+h/2,lo+h/2)):
            try: d = net.overpass(Q.format(b=f"{qa-h/2},{qo-h/2},{qa+h/2},{qo+h/2}"))
            except Exception: continue
            for el in d.get("elements", []):
                g = el.get("geometry") or []
                if len(g) < 8: continue
                lat = np.array([q["lat"] for q in g]); lon = np.array([q["lon"] for q in g])
                P = geo.ll_to_m(lat, lon, geo.utm_zone(float(lon.mean())))
                st = straight_stats(P)
                if st: rows.append(st)
    agg(rows, "REAL creeks")
else:
    from macro_campaign import cgrid
    SC = pathlib.Path(os.environ["SCRATCH"])
    d = sys.argv[2]; seeds = pathlib.Path(sys.argv[3]).read_text().split()
    src = open("/tmp/crossings.py").read(); ns = {}
    exec(src.split('for tag, d, seeds in')[0], ns)
    rows = []
    for s in seeds:
        try: wet, c = ns["creek_mask"](d, s)
        except Exception: continue
        if wet is None: continue
        ys,xs = np.where(wet)
        idx = {(int(y),int(x)):i for i,(y,x) in enumerate(zip(ys,xs))}
        def bfs(src_i):
            dd = np.full(len(ys),-1.0); dd[src_i]=0; par={src_i:None}
            dq = collections.deque([(int(ys[src_i]),int(xs[src_i]))])
            while dq:
                y,x = dq.popleft(); k0=idx[(y,x)]
                for dy in (-1,0,1):
                    for dx in (-1,0,1):
                        if dy==0 and dx==0: continue
                        k = idx.get((y+dy,x+dx))
                        if k is not None and dd[k]<0:
                            dd[k]=dd[k0]+c*(1.414 if dy and dx else 1.0)
                            par[k]=k0; dq.append((y+dy,x+dx))
            return dd,par
        d0,_ = bfs(0); a=int(np.argmax(np.where(d0>=0,d0,-1)))
        d1,par = bfs(a); b=int(np.argmax(np.where(d1>=0,d1,-1)))
        path=[]; cur=b
        while cur is not None: path.append((ys[cur]*c, xs[cur]*c)); cur=par[cur]
        st = straight_stats(np.array(path[::-1]))
        if st: rows.append(st)
    agg(rows, sys.argv[4] if len(sys.argv)>4 else d)
