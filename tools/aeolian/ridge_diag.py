#!/usr/bin/env python3
"""Ridge diagnosis on the high ground: draw every local ridge line above the
median elevation, on our tiles and on real ones, with the same detector."""
import sys, json, base64, io, pathlib
import numpy as np
from scipy import ndimage
ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
S = pathlib.Path(sys.argv[1])
sys.path.insert(0, str(S)); sys.path.insert(0, str(ROOT/"tools/macro_campaign"))
sys.path.insert(0, str(ROOT/"tools/metrics"))
from net_viewer import render
from macro_campaign import cgrid, flow
from metrics import core as mcore
from PIL import Image

def ridges(z8, cell=8.0, smooth_m=48.0):
    """Ridge strength = the most NEGATIVE principal curvature; ridge lines are
    its ridges. Reported only above the median elevation."""
    zs = ndimage.gaussian_filter(z8, (smooth_m/np.pi)/cell)
    gy, gx = np.gradient(zs, cell)
    gyy, gyx = np.gradient(gy, cell); gxy, gxx = np.gradient(gx, cell)
    tr = gxx + gyy
    det = gxx*gyy - gxy*gyx
    disc = np.sqrt(np.maximum(tr*tr/4 - det, 0))
    lmin = tr/2 - disc                      # most negative principal curvature
    strength = -lmin                        # positive on ridges
    high = z8 > np.median(z8)
    thr = np.percentile(strength[high], 88)
    m = (strength > thr) & high
    m = ndimage.binary_closing(m, np.ones((3,3)))
    m = ndimage.binary_opening(m, np.ones((2,2)))
    # how ridge-y: mean strength on the detected set, and how much of the
    # high ground is flagged as ridge
    return m, float(strength[high].mean()), float(np.percentile(strength[high], 99))

def medial(z8, cell=8.0):
    rec,_ = flow.receivers(mcore.fill_depressions(z8, cell), cell)
    acc = flow.accumulate(rec).astype(float)*cell*cell
    d = ndimage.distance_transform_edt(~(acc >= 1.2e4), sampling=cell)
    gy,gx = np.gradient(ndimage.gaussian_filter(d,1.0), cell)
    return (np.hypot(gx,gy) < 0.6) & (d > 24)

def card(z8, label, cell=8.0):
    m, mean_s, p99 = ridges(z8, cell)
    med = medial(z8, cell)
    rgb, _ = render(z8, cell, px=620)
    img = rgb.astype(float)
    step = max(1, z8.shape[0]//620)
    def put(mask, col, a):
        mm = np.flipud(mask[::step,::step])[:img.shape[0], :img.shape[1]]
        for c,v in enumerate(col):
            img[...,c][mm] = img[...,c][mm]*(1-a) + v*a
    put(med, (60,120,255), 0.45)     # where the drainage divide runs
    put(m,   (230,40,40), 0.85)      # detected ridge lines on high ground
    # coincidence: what share of detected ridge sits on the divide?
    coin = float((m & ndimage.binary_dilation(med, iterations=3)).sum()/max(m.sum(),1))
    b = io.BytesIO(); Image.fromarray(np.clip(img,0,255).astype(np.uint8)).save(b,"WEBP",quality=90,method=6)
    print(f"  {label}: ridge cells {int(m.sum()):6d} ({100*m.mean():.1f}% of tile)  "
          f"mean strength {mean_s:.5f}  p99 {p99:.5f}  on-divide {100*coin:.0f}%")
    return {"label":label,"img":base64.b64encode(b.getvalue()).decode(),
            "n":int(m.sum()),"mean":round(mean_s,5),"p99":round(p99,5),
            "coin":round(100*coin)}

out={"gen":[],"real":[]}
kept=[e["tile"] for e in json.loads((ROOT/"tools/macro_campaign/out/review_v2.json").read_text())["kept"] if e["archetype"]=="sandhills_nc"]
print("REAL:")
for t in [kept[2],kept[14],kept[20]]:
    z,(_,_,c)=cgrid.read_f32(str(ROOT/f"tools/macro_campaign/out/tiles/sandhills_nc/{t}.cgrid"))
    step=max(1,int(round(8.0/c))); out["real"].append(card(z[::step,::step].astype(float), f"REAL {t}"))
print("OURS:")
for seed in [105,101,104,109]:
    z,_=cgrid.read_f32(str(S/"asmround"/f"asm_{seed}.cgrid"))
    out["gen"].append(card(z.astype(float), f"seed {seed}"))
(S/"ridgediag.json").write_text(json.dumps(out))
