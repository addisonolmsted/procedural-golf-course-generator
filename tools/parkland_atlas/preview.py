"""Render a cached course's map to PNG (server-side twin of the viewer's canvas).

Lets us eyeball collected courses without a browser — same hillshade + tree +
water + hole overlays the HTML draws.

    python3 preview.py oakmont muirfieldvillage ...      # -> out/preview/<key>.png
"""

import json
import math
import os
import sys

import numpy as np
from PIL import Image, ImageDraw

from common import N, decode_elev, decode_mask

HERE = os.path.dirname(os.path.abspath(__file__))
CACHE = os.path.join(HERE, "out", "cache")
PREV = os.path.join(HERE, "out", "preview")

RAMP = [(0.0, (26, 46, 36)), (.18, (38, 66, 44)), (.38, (74, 96, 52)),
        (.58, (122, 120, 66)), (.76, (168, 146, 92)), (.9, (204, 180, 132)), (1.0, (232, 214, 178))]
HOLECOL = [(232, 214, 143), (217, 164, 107), (201, 224, 162), (159, 203, 155), (143, 191, 180),
           (136, 174, 204), (166, 149, 198), (197, 143, 180), (217, 141, 134), (232, 176, 127),
           (235, 211, 126), (185, 215, 126), (147, 199, 156), (132, 185, 185), (138, 163, 206),
           (183, 155, 203), (208, 147, 181), (223, 160, 138)]


def ramp(t):
    for i in range(1, len(RAMP)):
        if t <= RAMP[i][0]:
            (a, ca), (b, cb) = RAMP[i - 1], RAMP[i]
            f = (t - a) / (b - a) if b > a else 0
            return tuple(ca[j] + (cb[j] - ca[j]) * f for j in range(3))
    return RAMP[-1][1]


def hillshade_rgb(c):
    g = decode_elev(c["b64"], c["emin"])
    cw, ch = c["wm"] / N, c["hm"] / N
    az, alt = math.radians(315), math.radians(45)
    lx, ly, lz = math.cos(alt) * math.cos(az), math.cos(alt) * math.sin(az), math.sin(alt)
    rng = max(1e-6, c["emax"] - c["emin"])
    gx = np.zeros((N, N)); gy = np.zeros((N, N))
    gx[:, 1:-1] = (g[:, 2:] - g[:, :-2]) / (2 * cw)
    gy[1:-1, :] = (g[2:, :] - g[:-2, :]) / (2 * ch)
    nx, ny, nz = -gx, gy, np.ones((N, N))
    nl = np.sqrt(nx * nx + ny * ny + nz * nz)
    sh = np.clip((nx * lx + ny * ly + nz * lz) / nl, 0, None)
    lum = 0.35 + 0.75 * sh
    img = np.zeros((N, N, 3))
    tn = (g - c["emin"]) / rng
    for y in range(N):
        for x in range(N):
            col = ramp(tn[y, x])
            img[y, x] = [col[k] * lum[y, x] for k in range(3)]
    return np.clip(img, 0, 255).astype(np.uint8)


def render(key, scale=4):
    with open(os.path.join(CACHE, f"{key}.json")) as f:
        c = json.load(f)
    base = hillshade_rgb(c)
    im = Image.fromarray(base, "RGB").resize((N * scale, N * scale), Image.BILINEAR).convert("RGBA")

    def overlay(mask_b64, rgb, a):
        m = decode_mask(mask_b64)
        ov = np.zeros((N, N, 4), np.uint8)
        ov[m == 1] = (*rgb, a)
        im.alpha_composite(Image.fromarray(ov, "RGBA").resize((N * scale, N * scale), Image.NEAREST))

    overlay(c["tb64"], (20, 52, 30), 150)
    overlay(c["wb64"], (70, 128, 176), 205)

    W = H = N * scale
    la0, lo0, la1, lo1 = c["bbox"]
    drw = ImageDraw.Draw(im, "RGBA")

    def P(lat, lon):
        return ((lon - lo0) / (lo1 - lo0) * W, (la1 - lat) / (la1 - la0) * H)
    for i, h in enumerate(c["holes"]):
        pts = [P(p[0], p[1]) for p in h["pts"]]
        drw.line(pts, fill=(12, 19, 16, 200), width=5, joint="curve")
        drw.line(pts, fill=(*HOLECOL[i % 18], 255), width=2, joint="curve")
        tx, ty = pts[0]; gx, gy = pts[-1]
        drw.rectangle([tx - 3, ty - 3, tx + 3, ty + 3], fill=(230, 224, 204, 255))
        drw.ellipse([gx - 4, gy - 4, gx + 4, gy + 4], outline=(228, 87, 61, 255), width=2)
    for m, sym in ((c.get("min"), "v"), (c.get("max"), "^")):
        if not m:
            continue
        x = (m["gx"] + 0.5) / N * W
        y = (m["gy"] + 0.5) / N * H
        col = (127, 183, 222, 255) if sym == "v" else (228, 87, 61, 255)
        drw.ellipse([x - 4, y - 4, x + 4, y + 4], fill=col)
    return im


def main():
    os.makedirs(PREV, exist_ok=True)
    keys = sys.argv[1:] or ["oakmont"]
    for k in keys:
        out = os.path.join(PREV, f"{k}.png")
        render(k).save(out)
        print(out)


if __name__ == "__main__":
    main()
