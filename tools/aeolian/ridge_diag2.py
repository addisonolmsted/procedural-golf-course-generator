#!/usr/bin/env python3
"""Ridge lines by ASPECT DIVERGENCE — where the slope direction flips.

The curvature detector missed what the eye sees. A hillshade shows a ridge
as a shadow-highlight pair because the surface ASPECT reverses across it,
however gently. The scale-free test for that is the divergence of the unit
gradient field: positive where the surface sheds in both directions (a
ridge), negative where it gathers (a valley). Magnitude does not matter,
which is why this catches the faint forked pattern the eye reads and the
curvature threshold did not.
"""
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

def aspect_ridges(z8, cell=8.0, smooth_m=40.0):
    zs = ndimage.gaussian_filter(z8, (smooth_m/np.pi)/cell)
    gy, gx = np.gradient(zs, cell)
    n = np.hypot(gx, gy) + 1e-9
    ux, uy = gx/n, gy/n
    _, dux = np.gradient(ux, cell)
    duy, _ = np.gradient(uy, cell)
    div = dux + duy                      # >0 ridge, <0 valley
    return ndimage.gaussian_filter(div, 1.0)

def analyse(z8, cell=8.0):
    div = aspect_ridges(z8, cell)
    high = z8 > np.median(z8)
    thr = np.percentile(div[high], 90)
    m = (div > thr) & high
    m = ndimage.binary_closing(m, np.ones((3,3)))
    # the divide skeleton, for the coincidence test
    rec,_ = flow.receivers(mcore.fill_depressions(z8, cell), cell)
    acc = flow.accumulate(rec).astype(float)*cell*cell
    d = ndimage.distance_transform_edt(~(acc >= 1.2e4), sampling=cell)
    gy,gx = np.gradient(ndimage.gaussian_filter(d,1.0), cell)
    med = (np.hypot(gx,gy) < 0.6) & (d > 24)
    coin = float((m & ndimage.binary_dilation(med, iterations=4)).sum()/max(m.sum(),1))
    # how ORGANISED is the pattern: total ridge length vs how much of it is
    # in long connected runs (forks and bends chain up; noise does not)
    lab,nl = ndimage.label(m, np.ones((3,3)))
    sizes = ndimage.sum(m, lab, range(1, nl+1)) if nl else np.array([0.0])
    longshare = float(sizes[sizes > 60].sum()/max(sizes.sum(),1))
    return m, med, float(div[high].std()), coin, longshare

def card(z8, label, cell=8.0):
    m, med, sd, coin, ls = analyse(z8, cell)
    rgb,_ = render(z8, cell, px=620)
    img = rgb.astype(float)
    step = max(1, z8.shape[0]//620)
    def put(mask, col, a):
        mm = np.flipud(mask[::step,::step])[:img.shape[0],:img.shape[1]]
        for c,v in enumerate(col):
            img[...,c][mm] = img[...,c][mm]*(1-a) + v*a
    put(med, (60,120,255), 0.40)
    put(m, (230,40,40), 0.80)
    b = io.BytesIO(); Image.fromarray(np.clip(img,0,255).astype(np.uint8)).save(b,"WEBP",quality=90,method=6)
    print(f"  {label}: aspect-divergence std {sd:.5f}  on-divide {100*coin:.0f}%  "
          f"in long runs {100*ls:.0f}%")
    return {"label":label,"img":base64.b64encode(b.getvalue()).decode(),
            "sd":round(sd,5),"coin":round(100*coin),"ls":round(100*ls)}

out={"gen":[],"real":[]}
kept=[e["tile"] for e in json.loads((ROOT/"tools/macro_campaign/out/review_v2.json").read_text())["kept"] if e["archetype"]=="sandhills_nc"]
print("REAL:")
for t in [kept[2],kept[14],kept[20]]:
    z,(_,_,c)=cgrid.read_f32(str(ROOT/f"tools/macro_campaign/out/tiles/sandhills_nc/{t}.cgrid"))
    st=max(1,int(round(8.0/c))); out["real"].append(card(z[::st,::st].astype(float), f"REAL {t}"))
print("OURS:")
for seed in [105,101,104,109]:
    z,_=cgrid.read_f32(str(S/"asmround"/f"asm_{seed}.cgrid"))
    out["gen"].append(card(z.astype(float), f"seed {seed}"))
(S/"ridgediag2.json").write_text(json.dumps(out))
