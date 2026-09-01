"""Creek planform measurement — corpus statistics and model comparison.

MEASUREMENT ONLY. Nothing here affects generation; these are the instruments
behind docs/calibration/creek-planform.md, kept so the numbers can be
re-derived rather than trusted. The creek-planform experiment they were built
for was reverted (see the doc); the measurements stand.

Same pipeline both sides is the whole point: OSM stream vertices sit 10-30 m
apart, so an unsmoothed curvature measures DIGITIZING noise, not planform.
Every statistic here resamples at 10 m and smooths at 30 m.
"""
import sys, os, pathlib, json
sys.path.insert(0, ".")
import numpy as np

def bend_skew(P, resample=10.0, smooth=30.0):
    seg = np.hypot(*np.diff(P, axis=0).T); L = float(seg.sum())
    if L < 400: return []
    cum = np.r_[0.0, np.cumsum(seg)]
    ss = np.arange(0, L, resample)
    Q = np.stack([np.interp(ss, cum, P[:,0]), np.interp(ss, cum, P[:,1])], 1)
    k = max(int(smooth/resample), 1)
    if k > 1:
        ker = np.ones(k)/k
        Q = np.stack([np.convolve(Q[:,0], ker, "valid"),
                      np.convolve(Q[:,1], ker, "valid")], 1)
    if len(Q) < 12: return []
    s2 = np.hypot(*np.diff(Q, axis=0).T); c2 = np.r_[0.0, np.cumsum(s2)]
    v1 = np.diff(Q, axis=0)[:-1]; v2 = np.diff(Q, axis=0)[1:]
    cr = v1[:,0]*v2[:,1] - v1[:,1]*v2[:,0]
    n1 = np.hypot(*v1.T); n2 = np.hypot(*v2.T)
    denom = n1*n2*np.hypot(*(v1+v2).T)
    curv = np.where(denom > 1e-9, 2*cr/np.maximum(denom,1e-9), 0.0)
    sg = np.sign(curv); idx = np.where(np.diff(sg) != 0)[0]
    out = []
    for a, b in zip(idx[:-1], idx[1:]):
        if b - a < 3: continue
        arc = c2[b] - c2[a]
        if not (25 < arc < 1500): continue
        seg_c = np.abs(curv[a:b])
        if seg_c.max() <= 0: continue
        pk = a + int(np.argmax(seg_c))
        out.append(float((c2[pk] - c2[a]) / arc))
    return out

if __name__ == "__main__":
    mode = sys.argv[1]
    if mode == "real":
        from corpus import config, net, geo
        QUERY = ('[out:json][timeout:180];(way["waterway"="stream"]({b}););out geom;')
        sk = []
        for (tag, la, lo, h) in config.SEARCH_BOXES:
            for (qa, qo) in ((la-h/2, lo-h/2), (la-h/2, lo+h/2),
                             (la+h/2, lo-h/2), (la+h/2, lo+h/2)):
                b = f"{qa-h/2},{qo-h/2},{qa+h/2},{qo+h/2}"
                try: d = net.overpass(QUERY.format(b=b))
                except Exception: continue
                for el in d.get("elements", []):
                    g = el.get("geometry") or []
                    if len(g) < 8: continue
                    lat = np.array([q["lat"] for q in g]); lon = np.array([q["lon"] for q in g])
                    P = geo.ll_to_m(lat, lon, geo.utm_zone(float(lon.mean())))
                    sk += bend_skew(P)
        sk = np.array(sk)
        print(f"REAL bend skew, n={len(sk)} bends")
        print(f"  p10/25/50/75/90 = " + "/".join(f"{v:.3f}" for v in
              np.percentile(sk,[10,25,50,75,90])))
        print(f"  |skew-0.5| median {np.median(np.abs(sk-0.5)):.3f}   "
              f"fraction beyond 0.30-0.70: {np.mean((sk<0.3)|(sk>0.7))*100:.0f}%")
        json.dump({"p":list(np.percentile(sk,[10,25,50,75,90])),
                   "asym":float(np.median(np.abs(sk-0.5))),
                   "tail":float(np.mean((sk<0.3)|(sk>0.7)))}, open("/tmp/skew_real.json","w"))
    else:
        SC = pathlib.Path(os.environ["SCRATCH"])
        groups, cur = {}, None
        for ln in (SC/"ladder.txt").read_text().splitlines():
            if not (ln[0].isdigit() or ln.startswith("-")):
                cur = ln.strip(); groups[cur] = []
            else:
                x,y = ln.split(); groups[cur].append((float(x), float(y)))
        real = json.load(open("/tmp/skew_real.json")) if pathlib.Path("/tmp/skew_real.json").exists() else None
        if real: print(f"REAL: median skew {real['p'][2]:.3f}, |asym| {real['asym']:.3f}, "
                       f"tail {real['tail']*100:.0f}%")
        for k in groups:
            if "_OX" in k or k == "AXIS": continue
            sk = np.array(bend_skew(np.array(groups[k])))
            if len(sk) < 5: continue
            print(f"  {k:16s} median skew {np.median(sk):.3f}  |asym| "
                  f"{np.median(np.abs(sk-0.5)):.3f}  tail "
                  f"{np.mean((sk<0.3)|(sk>0.7))*100:3.0f}%  (n={len(sk)})")
