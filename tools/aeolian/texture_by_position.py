#!/usr/bin/env python3
"""Measure fine-band texture as a function of position across the valley.

Review 2026-08-25: the pasted fabric reads as uniform fuzz, while on real
tiles the texture plainly differs between valley floor, wall and ridge. The
gate in use was a guessed linear ramp. This measures the real thing: for
each wavelength band, the amplitude profile against the normalised valley
coordinate u = d/(d+m) (0 on the channel, 1 on the divide), pooled over the
kept Carolina tiles, and emits it as a shipped table.
"""
import json, pathlib, sys
import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
sys.path.insert(0, str(ROOT / "tools" / "metrics"))
from macro_campaign import cgrid, flow                      # noqa
from metrics import core as mcore                           # noqa

CELL8, THRESH = 8.0, 1.2e4
BANDS = [(4, 12), (12, 24), (24, 48), (48, 96)]
U_BINS = 12
BIOME = "sandhills_nc"


def valley_u(z8):
    zf = mcore.fill_depressions(z8, CELL8)
    rec, _ = flow.receivers(zf, CELL8)
    acc = flow.accumulate(rec).astype(float) * CELL8 * CELL8
    chan = acc >= THRESH
    dist = ndimage.distance_transform_edt(~chan, sampling=CELL8)
    gy, gx = np.gradient(ndimage.gaussian_filter(dist, 1.0), CELL8)
    medial = (np.hypot(gx, gy) < 0.6) & (dist > 24.0)
    if not medial.any():
        medial = dist > np.percentile(dist, 99)
    m = ndimage.distance_transform_edt(~medial, sampling=CELL8)
    return np.clip(dist / np.maximum(dist + m, 1e-6), 0, 1), dist


def main():
    kept = [e["tile"] for e in json.loads(
        (ROOT / "tools/macro_campaign/out/review_v2.json").read_text())["kept"]
        if e["archetype"] == BIOME]
    acc = np.zeros((len(BANDS), U_BINS))
    cnt = np.zeros((len(BANDS), U_BINS))
    for t in kept:
        z, (_, _, cell) = cgrid.read_f32(
            str(ROOT / f"tools/macro_campaign/out/tiles/{BIOME}/{t}.cgrid"))
        z = z.astype(float)
        step = max(1, int(round(CELL8 / cell)))
        u8, _ = valley_u(z[::step, ::step])
        u = ndimage.zoom(u8, z.shape[0] / u8.shape[0], order=1)
        u = u[:z.shape[0], :z.shape[1]]
        ub = np.clip((u * U_BINS).astype(int), 0, U_BINS - 1)
        for bi, (lo, hi) in enumerate(BANDS):
            r = (ndimage.gaussian_filter(z, (lo / np.pi) / cell)
                 - ndimage.gaussian_filter(z, (hi / np.pi) / cell))
            for k in range(U_BINS):
                mm = ub == k
                if mm.sum() > 500:
                    acc[bi, k] += r[mm].std() * mm.sum()
                    cnt[bi, k] += mm.sum()
        print(f"  {t}", flush=True)
    prof = np.where(cnt > 0, acc / np.maximum(cnt, 1), np.nan)
    for bi in range(len(BANDS)):
        v = prof[bi]
        ok = np.isfinite(v)
        prof[bi] = np.interp(np.arange(U_BINS), np.nonzero(ok)[0], v[ok])
    print("\nband      " + "  ".join(f"u{k / U_BINS:.2f}" for k in range(U_BINS)))
    for bi, (lo, hi) in enumerate(BANDS):
        print(f"{lo:3d}-{hi:3d}m  " + "  ".join(f"{v:.3f}" for v in prof[bi]))
    print("\nnormalised to each band's tile mean (the GAIN profile):")
    lines = ["TPROF1",
             f"# fine-band texture amplitude vs valley position u, {len(kept)} "
             f"kept {BIOME} tiles",
             f"# u = d/(d+m); bands are (lo,hi) m; values are gain vs the "
             f"band's own tile mean",
             f"u_bins {U_BINS}",
             "bands " + " ".join(f"{lo}:{hi}" for lo, hi in BANDS)]
    for bi, (lo, hi) in enumerate(BANDS):
        g = prof[bi] / prof[bi].mean()
        print(f"{lo:3d}-{hi:3d}m  " + "  ".join(f"{v:.3f}" for v in g))
        lines.append("g " + " ".join(f"{v:.4f}" for v in g))
    out = ROOT / "assets" / "sandhills_texture_profile.txt"
    out.write_text("\n".join(lines) + "\n")
    print(f"\nwrote {out.name}")


if __name__ == "__main__":
    main()
