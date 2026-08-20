"""Golf proxy over the CORPUS tiles, per biome, against the 64-real-course floors.

Answers: how much routable ground does each archetype's real land actually
offer, and therefore how much macro intervention golf requires.
"""
import sys, pathlib, json
import numpy as np
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT/"tools/macro_campaign")); sys.path.insert(0, str(ROOT/"tools/metrics"))
sys.path.insert(0, str(ROOT/"tools/golf_proxy"))
from macro_campaign import cgrid, extract_v2
import proxy as gp

th = gp.load_thresholds()["thresholds"]
CAPF, STEEPF = th["frac_under_cap"]["floor"], th["frac_under_steep"]["floor"]
CONTF = th["largest_contig_ha"]["floor"]
RB = th["relief_p95_p5"]["band"]
print(f"floors from 64 real courses: cap>={CAPF:.3f} steep>={STEEPF:.3f} contig>={CONTF:.0f}ha relief {RB[0]:.1f}-{RB[1]:.1f}m\n")
print(f"{'biome':14s} {'n':>3s} {'cap p50':>8s} {'steep p50':>10s} {'contig_ha p50':>14s} {'relief p50':>11s} {'PASS all':>9s}")
rows={}
for b in ["piedmont","great_plains","river_valley","hill_country","heathland","sandhills"]:
    C,S,H,R,ok = [],[],[],[],0
    tids=[t for a,t in extract_v2.kept_tiles() if a==b]
    for tid in tids:
        p=extract_v2.OUT/"tiles"/b/f"{tid}.cgrid"
        if not p.exists(): continue
        z,(_,_,c)=cgrid.read_f32(p); z=z.astype(np.float64)
        if np.isfinite(z).mean()<0.95: continue
        q=gp.quantities(z,c)
        C.append(q["frac_under_cap"]); S.append(q["frac_under_steep"])
        H.append(q["largest_contig_ha"]); R.append(q["relief_p95_p5"])
        if (q["frac_under_cap"]>=CAPF and q["frac_under_steep"]>=STEEPF
            and q["largest_contig_ha"]>=CONTF and RB[0]<=q["relief_p95_p5"]<=RB[1]): ok+=1
    m=lambda v: float(np.median(v)) if v else float('nan')
    rows[b]=dict(n=len(C),cap=m(C),steep=m(S),contig=m(H),relief=m(R),pass_frac=100.0*ok/max(len(C),1))
    print(f"{b:14s} {len(C):3d} {m(C):8.3f} {m(S):10.3f} {m(H):14.1f} {m(R):11.1f} {100.0*ok/max(len(C),1):8.0f}%", flush=True)
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent/"proxy_corpus.json","w"), indent=1)
