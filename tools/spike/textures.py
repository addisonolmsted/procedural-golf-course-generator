"""Texture-isolation renders for the G-SPIKE blind test.

The first blind set was confounded: real crops carry dendritic drainage
STRUCTURE, which in production comes from S2's skeleton, not the dictionary --
a reviewer can sort real from generated on evidence about the wrong stage.

These renders isolate TEXTURE (the hillslope fabric between drainage lines,
the thing the dictionary actually owns) by giving both sides identical
structure, and by using visuals that show fabric more directly than a single
B&W hillshade:

  fineblind/    THE FAIR BLIND TEST: same macro + same real mid band on both
                sides; only the <64 m band differs (real vs dictionary).
                8 shuffled crops + answer key.
  ramp_*        each band pasted onto a uniform 3.5% planar slope -- pure
                fabric, no structure of either origin to key on. Real left.
  curv_*        Laplacian curvature maps (diverging colormap, +/-p98 clip) --
                curvature is where the remaining metric miss lives, so this
                is the visual where real-vs-dict differences are most honest.
                Real left.
  tint_*        hypsometric tint x multidirectional hillshade composites of
                the full surfaces (real vs full reconstruction) -- a richer
                overall view than single-source hillshade. Real left.

Run after spike.py (reads out/bands.npz):  python3 tools/spike/textures.py
"""
import json
import os

import numpy as np
from PIL import Image
from matplotlib import colormaps
from scipy import ndimage

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "out")
REPORT = os.path.join(HERE, "report")
CELL = 2.0
SEED = 7


def hillshade(z, cell, az_deg=315.0, alt_deg=45.0):
    gy, gx = np.gradient(z, cell)
    slope = np.arctan(np.hypot(gx, gy))
    aspect = np.arctan2(-gx, gy)
    az, alt = np.radians(az_deg), np.radians(alt_deg)
    hs = np.sin(alt) * np.cos(slope) + np.cos(alt) * np.sin(slope) * np.cos(az - aspect)
    return np.clip(hs, 0, 1)


def to_u8(x):
    lo, hi = np.percentile(x, [1, 99])
    return (np.clip((x - lo) / max(hi - lo, 1e-9), 0, 1) * 255).astype(np.uint8)


def multishade(z, cell):
    """Mean of three azimuths — softer, more shape cues than one source."""
    hs = sum(hillshade(z, cell, az) for az in (315.0, 15.0, 270.0)) / 3.0
    return hs


def tint_composite(z, cell):
    """Hypsometric tint × multidirectional hillshade → RGB u8."""
    hs = multishade(z, cell)
    lo, hi = np.percentile(z, [1, 99])
    t = np.clip((z - lo) / max(hi - lo, 1e-9), 0, 1)
    rgb = colormaps["gist_earth"](t)[..., :3]
    shade = (0.35 + 0.65 * (hs - hs.min()) / max(hs.max() - hs.min(), 1e-9))
    return (rgb * shade[..., None] * 255).astype(np.uint8)


def curv_map(band, cell):
    """Laplacian curvature, diverging colormap, ±p98 clip → RGB u8."""
    lap = ndimage.laplace(band) / (cell * cell)
    s = np.percentile(np.abs(lap), 98)
    t = np.clip(lap / max(s, 1e-12), -1, 1) * 0.5 + 0.5
    return (colormaps["RdBu_r"](t)[..., :3] * 255).astype(np.uint8)


def pair(a, b, gap=8):
    pad = np.full((a.shape[0], gap) + a.shape[2:], 255, np.uint8)
    return np.concatenate([a, pad, b], axis=1)


def save(arr, name):
    Image.fromarray(arr).save(os.path.join(REPORT, name))
    print("  ", name)


def main():
    d = np.load(os.path.join(OUT, "bands.npz"))
    lp400 = d["lp400"].astype(np.float64)
    real_mid = d["real_mid"].astype(np.float64)
    real_fine = d["real_fine"].astype(np.float64)
    mid_dict = d["mid_dict"].astype(np.float64)
    fine_dict = d["fine_dict"].astype(np.float64)
    recon_full = d["recon_full"].astype(np.float64)
    filled = d["filled"].astype(np.float64)

    crops = [(100, 100), (100, 900), (900, 100), (900, 900)]

    # ---- 1. THE FAIR BLIND TEST: identical structure, only fine band differs
    base = lp400 + real_mid
    hs_real = to_u8(multishade(base + real_fine, CELL))
    hs_dict = to_u8(multishade(base + fine_dict, CELL))
    bdir = os.path.join(REPORT, "fineblind")
    os.makedirs(bdir, exist_ok=True)
    entries = []
    for i, (cy, cx) in enumerate(crops):
        entries.append((f"crop{i}", "real", hs_real[cy:cy + 512, cx:cx + 512]))
        entries.append((f"crop{i}", "dict", hs_dict[cy:cy + 512, cx:cx + 512]))
    rng = np.random.default_rng(SEED)
    key = {}
    for j, idx in enumerate(rng.permutation(len(entries))):
        cid, kind, img = entries[idx]
        name = f"fine_{chr(ord('a') + j)}.png"
        Image.fromarray(img).save(os.path.join(bdir, name))
        key[name] = dict(crop=cid, kind=kind)
    json.dump(key, open(os.path.join(bdir, "answer_key.json"), "w"), indent=1)
    print("   fineblind/fine_a..h.png (+ answer_key.json)")

    # ---- 2. Bands on a uniform ramp: pure fabric, no structure to key on
    H, W = real_fine.shape
    ramp = 0.035 * CELL * np.arange(W)[None, :] * np.ones((H, 1))
    cy, cx = 400, 400
    for tag, r_band, d_band in (("fine", real_fine, fine_dict),
                                ("mid", real_mid, mid_dict)):
        a = to_u8(multishade(ramp + r_band, CELL))[cy:cy + 512, cx:cx + 512]
        b = to_u8(multishade(ramp + d_band, CELL))[cy:cy + 512, cx:cx + 512]
        save(pair(a, b), f"ramp_{tag}_real_left.png")

    # ---- 3. Curvature maps: the fabric's sharpness, directly
    for tag, r_band, d_band in (("fine", real_fine, fine_dict),
                                ("mid", real_mid, mid_dict)):
        a = curv_map(r_band, CELL)[cy:cy + 512, cx:cx + 512]
        b = curv_map(d_band, CELL)[cy:cy + 512, cx:cx + 512]
        save(pair(a, b), f"curv_{tag}_real_left.png")

    # ---- 4. Full-surface tinted composites (context view)
    for i, (cy2, cx2) in enumerate(crops[:2]):
        a = tint_composite(filled, CELL)[cy2:cy2 + 512, cx2:cx2 + 512]
        b = tint_composite(recon_full, CELL)[cy2:cy2 + 512, cx2:cx2 + 512]
        save(pair(a, b), f"tint_{i}_real_left.png")

    print("done")


if __name__ == "__main__":
    main()
