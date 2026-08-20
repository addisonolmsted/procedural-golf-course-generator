"""Is river_valley's trunk dominant, given main_share reads only 37.5%?

HYPOTHESIS: yes, but the dominance is in DISCHARGE and is sourced OUTSIDE the
3 km window. A tile-local D8 cannot see the trunk's upstream catchment, so it
scores the trunk as just another channel carrying only the area it gathers
inside the tile. main_share therefore measures how the tile's OWN drainage
partitions -- which is a different question from "does a trunk dominate".

Two measurements settle it:
  max_acc_frac   largest accumulation at any channel cell / tile area.
                 If river_valley ~= the others, no system gathers more than
                 the tile itself provides -> tile-internal catchments are
                 comparable everywhere, and any trunk dominance MUST be
                 external.
  flat_floor     fraction of tile area that is low AND flat AND connected --
                 a floodplain. Positive evidence that a trunk valley exists
                 even where main_share says the catchment is shared.
"""
import sys, pathlib, json
import numpy as np
from scipy import ndimage

ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
sys.path.insert(0, str(ROOT / "tools" / "metrics"))
from macro_campaign import cgrid, extract_v2, flow      # noqa
from metrics import core as mcore                        # noqa

BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]
CELL = 8.0
NTILE = int(sys.argv[1]) if len(sys.argv) > 1 else 12

print(f"{'biome':14s} {'n':>3s} {'max_acc/area':>13s} {'flat_floor%':>12s} {'relief_m':>9s}")
rows={}
for b in BIOMES:
    ma, ff, rl = [], [], []
    for tid in [t for a, t in extract_v2.kept_tiles() if a == b][:NTILE]:
        p = extract_v2.OUT / "tiles" / b / f"{tid}.cgrid"
        if not p.exists(): continue
        z,(_,_,cell) = cgrid.read_f32(p); z=z.astype(np.float64)
        m=np.isfinite(z)
        if m.mean()<0.95: continue
        z=np.where(m,z,np.nanmean(z[m]))
        step=max(int(round(CELL/cell)),1); z8=z[::step,::step]
        area = z8.size * CELL * CELL
        zf=mcore.fill_depressions(z8,CELL)
        rec,_=flow.receivers(zf,CELL)
        acc=flow.accumulate(rec).astype(float)*CELL*CELL
        ma.append(float(acc.max())/area)
        # flat floor: low (bottom 25% of relief) AND gentle (<1.5%) AND in the
        # largest connected component of that set
        gy,gx=np.gradient(z8,CELL); slope=np.hypot(gx,gy)
        lo = z8 <= np.percentile(z8,5)+0.25*(np.percentile(z8,95)-np.percentile(z8,5))
        cand = lo & (slope < 0.015)
        lab,n = ndimage.label(cand)
        if n:
            sizes=ndimage.sum(cand,lab,range(1,n+1))
            ff.append(100.0*sizes.max()/z8.size)
        else:
            ff.append(0.0)
        rl.append(float(np.percentile(z8,95)-np.percentile(z8,5)))
    med=lambda v: float(np.median(v)) if v else float('nan')
    rows[b]=dict(n=len(ma),max_acc=med(ma),flat=med(ff),relief=med(rl))
    print(f"{b:14s} {len(ma):3d} {med(ma):13.3f} {med(ff):12.1f} {med(rl):9.1f}", flush=True)
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent/"trunk_probe.json","w"), indent=1)
