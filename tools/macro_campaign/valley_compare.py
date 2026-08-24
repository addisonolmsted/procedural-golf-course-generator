#!/usr/bin/env python3
"""Valley-scale diagnostic: run the corpus extraction policy (8 m, fill, D8,
acc>=6e4) on real NC tiles AND our carved tiles; overlay the extracted
network, the main trunk, and HAND-based valley outlines; measure trunk
valley width and depth with the same ruler on both."""
import base64, io, json, pathlib, sys
import numpy as np
from PIL import Image
from scipy import ndimage

ROOT = pathlib.Path("/Users/davisolmsted/Documents/GitHub/GolfProceduralGenerator")
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
sys.path.insert(0, str(ROOT / "tools" / "metrics"))
sys.path.insert(0, str(pathlib.Path(__file__).parent))
from macro_campaign import cgrid, flow
from metrics import core as mcore
from net_viewer import render

CELL = 8.0
THRESH = 6.0e4

def analyze(z8):
    zf = mcore.fill_depressions(z8, CELL)
    rec, _ = flow.receivers(zf, CELL)
    acc = flow.accumulate(rec).astype(float) * CELL * CELL
    ny, nx = z8.shape
    chan = acc >= THRESH
    # HAND: height above nearest channel cell (euclidean attribution)
    dist, (iy, ix) = ndimage.distance_transform_edt(~chan, sampling=CELL, return_indices=True)
    hand = z8 - z8[iy, ix]
    # main trunk: from the max-acc cell walk upstream along max-acc donor
    recf, accf = rec.ravel(), acc.ravel()
    donors = [[] for _ in range(nx * ny)]
    for i, r in enumerate(recf):
        if r >= 0:
            donors[r].append(i)
    cur = int(np.argmax(accf))
    trunk = [cur]
    while True:
        ups = [d for d in donors[cur] if accf[d] >= 5 * THRESH]
        if not ups:
            break
        cur = max(ups, key=lambda d: accf[d])
        trunk.append(cur)
    ty, tx = np.array(trunk) // nx, np.array(trunk) % nx
    # measurements at trunk stations (every 6 cells, skipping the ends)
    widths_f, widths_v, depths = [], [], []
    for k in range(4, len(trunk) - 4, 6):
        y, x = int(ty[k]), int(tx[k])
        dyv = float(ty[min(k+3, len(trunk)-1)] - ty[max(k-3, 0)])
        dxv = float(tx[min(k+3, len(trunk)-1)] - tx[max(k-3, 0)])
        nrm = np.array([-dxv, dyv]); n = np.hypot(*nrm)
        if n < 1e-6:
            continue
        nrm /= n
        # width: run of HAND below threshold along the perpendicular
        def run_width(hthr):
            w = 0
            for sgn in (1, -1):
                for step in range(1, 60):
                    yy = int(round(y + nrm[1] * step * sgn))
                    xx = int(round(x + nrm[0] * step * sgn))
                    if not (0 <= yy < ny and 0 <= xx < nx):
                        break
                    if hand[yy, xx] > hthr:
                        break
                    w += 1
            return (w + 1) * CELL
        widths_f.append(run_width(2.0))
        widths_v.append(run_width(5.0))
        # depth: local upland (p85 within 300 m) minus bed
        y0, y1 = max(0, y-38), min(ny, y+38)
        x0, x1 = max(0, x-38), min(nx, x+38)
        depths.append(float(np.percentile(z8[y0:y1, x0:x1], 85) - z8[y, x]))
    dens = chan.sum() * CELL / (ny * nx * CELL * CELL / 1e6) / 1000.0
    return {
        "chan": chan, "acc": acc, "hand": hand, "trunk": (ty, tx),
        "stats": {
            "density": round(float(dens), 2),
            "floor_w_p50": round(float(np.median(widths_f)), 0) if widths_f else None,
            "valley_w_p50": round(float(np.median(widths_v)), 0) if widths_v else None,
            "depth_p50": round(float(np.median(depths)), 1) if depths else None,
            "depth_p90": round(float(np.percentile(depths, 90)), 1) if depths else None,
        },
    }

def card(z8, label):
    a = analyze(z8)
    rgb, extent = render(z8, CELL)
    img = rgb.astype(float)
    ny, nx = z8.shape
    # overlays are in image space (flipud applied by render)
    def put(mask, color, alpha):
        m = np.flipud(mask)
        for c in range(3):
            img[..., c][m] = img[..., c][m] * (1 - alpha) + color[c] * alpha
    put(a["hand"] < 5.0, (255, 165, 40), 0.28)     # valley walls outline zone
    put(a["hand"] < 2.0, (230, 60, 40), 0.38)      # valley floor
    put(a["chan"], (30, 70, 220), 0.9)             # extracted channels
    tmask = np.zeros_like(a["chan"])
    ty, tx = a["trunk"]
    tmask[ty, tx] = True
    tmask = ndimage.binary_dilation(tmask, iterations=1)
    put(tmask, (10, 20, 120), 1.0)                 # the main trunk
    b = io.BytesIO()
    Image.fromarray(np.clip(img, 0, 255).astype(np.uint8)).save(b, "PNG", optimize=True)
    print(label, a["stats"])
    return {"label": label, "img": base64.b64encode(b.getvalue()).decode(), "stats": a["stats"]}

def main():
    out = []
    kept = [e["tile"] for e in json.loads((ROOT/"tools/macro_campaign/out/review_v2.json").read_text())["kept"]
            if e["archetype"] == "sandhills_nc"]
    for t in [kept[2], kept[8], kept[14], kept[20], kept[26], kept[29]]:
        z, (_, _, cell) = cgrid.read_f32(ROOT / f"tools/macro_campaign/out/tiles/sandhills_nc/{t}.cgrid")
        step = max(1, int(round(CELL / cell)))
        out.append(card(z[::step, ::step].astype(float), f"REAL {t}"))
    S = pathlib.Path(sys.argv[1])
    for seed in [101, 102, 103, 104, 105, 106, 107, 108, 109, 110]:
        z, (_, _, cell) = cgrid.read_f32(S / f"carve_{seed}.cgrid")
        out.append(card(z.astype(float), f"GEN seed {seed}"))
    (S / "compare.json").write_text(json.dumps(out))

if __name__ == "__main__":
    main()
