#!/usr/bin/env python3
"""Measure texture amplitude as a function of RELIEF POSITION on the NC tiles.

The existing texture profile (TPROF1) is keyed on the valley coordinate
u = d/(d+m) — distance to the nearest channel against distance to the divide.
Review 2026-08-26 asks a different question: does the fabric differ between
the BOTTOM THIRD and the TOP THIRD of the relief, with the high ground finer
and smoother and the low ground carrying the bumps and pocks?

That is a genuinely different axis. `u` is a planform coordinate; a point can
sit at u = 0.9 (far from any channel) and still be low, on a broad flat
interfluve between two shallow valleys. So this measures the corpus against
normalised height directly and emits a table the generator can interpolate.

    python3 tools/aeolian/texture_by_relief.py measure       # print the curve
    python3 tools/aeolian/texture_by_relief.py export        # write the asset
"""
import sys
import pathlib
import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "tools/macro_campaign"))
from macro_campaign import cgrid  # noqa: E402

TILES = ROOT / "tools/macro_campaign/out/tiles/sandhills_nc"
OUT = ROOT / "assets" / "sandhills_relief_profile.txt"

# Bands, metres. The first is the grain the eye reads as "fine", the last is
# landform-scale and is carried by the macro stage, not the quilt.
BANDS = [(2, 4), (4, 8), (8, 16), (16, 32), (32, 64)]
NBIN = 7                      # relief bins, 0 = valley floor, 1 = hilltop


def bandpass(z, c, lo, hi):
    return (ndimage.gaussian_filter(z, lo / (2.355 * c))
            - ndimage.gaussian_filter(z, hi / (2.355 * c)))


def tile_curve(z, c):
    """Per-band RMS in each relief bin, normalised to the tile's own mean."""
    z = z.astype(float)
    lo, hi = np.percentile(z, 2), np.percentile(z, 98)
    if hi - lo < 1e-6:
        return None
    h = np.clip((z - lo) / (hi - lo), 0.0, 1.0)
    binid = np.minimum((h * NBIN).astype(int), NBIN - 1)
    out = np.zeros((len(BANDS), NBIN))
    for bi, (a, b) in enumerate(BANDS):
        f = bandpass(z, c, a, b)
        f2 = f * f
        tot = np.sqrt(f2.mean())
        if tot < 1e-9:
            continue
        for k in range(NBIN):
            m = binid == k
            out[bi, k] = np.sqrt(f2[m].mean()) / tot if m.sum() > 50 else np.nan
    return out


def collect(paths):
    acc = []
    for t in paths:
        z, (_, _, c) = cgrid.read_f32(t)
        cur = tile_curve(z, c)
        if cur is not None:
            acc.append(cur)
    return np.nanmedian(np.array(acc), axis=0), len(acc)


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "measure"
    paths = sorted(TILES.glob("*.cgrid"))
    curve, n = collect(paths)
    centres = [(k + 0.5) / NBIN for k in range(NBIN)]

    print(f"{n} NC tiles — band RMS by relief position, "
          f"normalised to each tile's own mean")
    print(f"{'band':>10s} " + " ".join(f"{c:6.2f}" for c in centres))
    print(f"{'':>10s} " + " ".join(f"{'low' if i == 0 else ('high' if i == NBIN - 1 else ''):>6s}"
                                   for i in range(NBIN)))
    for bi, (a, b) in enumerate(BANDS):
        print(f"{a:4d}-{b:<5d} " + " ".join(f"{v:6.3f}" for v in curve[bi]))
    print()
    for bi, (a, b) in enumerate(BANDS):
        lo3 = np.nanmean(curve[bi, :NBIN // 3 + 1])
        hi3 = np.nanmean(curve[bi, -(NBIN // 3 + 1):])
        print(f"  {a:2d}-{b:<3d} m: bottom third {lo3:.3f}  top third {hi3:.3f}  "
              f"ratio {hi3 / lo3:.2f}x")

    if mode == "calibrate":
        # One multiplicative correction step. The stored table is the TARGET
        # curve, but applying a target is not the same as hitting it: part of
        # each band's energy is macro relief the quilt cannot touch, and our
        # valley floors start rougher than the corpus (bin 0 measured 1.43
        # against a target 1.15, because the floors carry macro roughness the
        # texture gain then multiplies). So the table is pre-distorted by
        # target/achieved, measured on generated tiles.
        gen = pathlib.Path(sys.argv[2])
        gacc = []
        for t in sorted(q for q in gen.glob("tex_*.cgrid") if ".water." not in q.name):
            z, (_, _, c) = cgrid.read_f32(t)
            cur = tile_curve(z, c)
            if cur is not None:
                gacc.append(cur)
        got = np.nanmedian(np.array(gacc), axis=0)
        old = np.loadtxt(OUT, skiprows=3)
        corr = np.clip(curve / got, 0.55, 1.8)
        new = old * corr
        with OUT.open("w") as f:
            f.write("RPROF1\n")
            f.write(f"{len(BANDS)} {NBIN}\n")
            f.write(" ".join(f"{a}-{b}" for a, b in BANDS) + "\n")
            for bi in range(len(BANDS)):
                f.write(" ".join(f"{v:.4f}" for v in new[bi]) + "\n")
        print(f"\ncorrection applied to {OUT} (from {len(gacc)} generated tiles)")
        for bi, (a, b) in enumerate(BANDS):
            print(f"  {a:2d}-{b:<3d}: " + " ".join(f"{v:5.2f}" for v in corr[bi]))
        return

    if mode == "export":
        with OUT.open("w") as f:
            f.write("RPROF1\n")
            f.write(f"{len(BANDS)} {NBIN}\n")
            f.write(" ".join(f"{a}-{b}" for a, b in BANDS) + "\n")
            for bi in range(len(BANDS)):
                row = np.nan_to_num(curve[bi], nan=1.0)
                f.write(" ".join(f"{v:.4f}" for v in row) + "\n")
        print(f"\nwrote {OUT}")


if __name__ == "__main__":
    main()
