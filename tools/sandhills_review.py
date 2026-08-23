#!/usr/bin/env python3
"""Build the Carolina Sandhills human-review sheet.

`extract-v2` only runs over tiles a person has marked kept in `review_v2.json`,
so the corpus cannot close without a human looking at every tile. This renders
the OSM-screened survivors as tinted hillshades with the numbers and the
advisory flags that inform a keep/cull, and emits a self-contained HTML page.

    python3 tools/sandhills_review.py <out_dir>

Rendering is `course-lab::render_terrain` verbatim (green low -> tan high,
lambert^1.15, Z_EXAG 2.4 on the shading normals only) so these read the same as
every other height visual on the branch.
"""
from __future__ import annotations

import base64
import json
import pathlib
import sys
import warnings

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent.parent
for p in ("macro_campaign", "metrics", "golf_proxy", "aeolian"):
    sys.path.insert(0, str(ROOT / "tools" / p))
warnings.simplefilter("ignore")

from macro_campaign import cgrid                      # noqa: E402
from proxy import quantities, load_thresholds         # noqa: E402
import dune_stats as ds                               # noqa: E402

BIOME = "sandhills_nc"
OUT_TILES = ROOT / "tools" / "macro_campaign" / "out" / "tiles" / BIOME
OUT_DEV = ROOT / "tools" / "macro_campaign" / "out" / "develop" / BIOME
EXCLUDE = ROOT / "tools" / "macro_campaign" / "out" / "exclude.json"

# course-lab::render_terrain, ported verbatim.
Z_EXAG = 2.4
RAMP = [(0.00, (0.34, 0.50, 0.29)), (0.40, (0.62, 0.62, 0.38)),
        (0.75, (0.78, 0.68, 0.44)), (1.00, (0.87, 0.79, 0.60))]


def tint(t):
    t = np.clip(t, 0.0, 1.0)
    out = np.zeros(t.shape + (3,))
    for i in range(len(RAMP) - 1):
        a, ca = RAMP[i]
        b, cb = RAMP[i + 1]
        m = (t >= a) & (t <= b)
        f = np.zeros_like(t)
        f[m] = (t[m] - a) / (b - a)
        for c in range(3):
            out[..., c][m] = ca[c] + (cb[c] - ca[c]) * f[m]
    return out


def render(z, cell, px=560):
    step = max(1, z.shape[0] // px)
    zz = z[::step, ::step].astype(float)
    c = cell * step
    az, alt = np.radians(315.0), np.radians(45.0)
    L = np.array([np.cos(az) * np.cos(alt), np.sin(az) * np.cos(alt), np.sin(alt)])
    gy, gx = np.gradient(zz * Z_EXAG, c)
    lam = ((-gx) * L[0] + (-gy) * L[1] + L[2]) / np.sqrt(gx ** 2 + gy ** 2 + 1.0)
    shade = np.clip(lam, 0.0, None) ** 1.15
    lo, hi = np.percentile(zz, 2), np.percentile(zz, 98)
    t = tint((zz - lo) / max(hi - lo, 1e-6))
    rgb = np.clip((t * 0.85 + 0.15) * (0.25 + 0.95 * shade)[..., None] * 235.0, 0, 255)
    return np.flipud(rgb).astype(np.uint8)      # row 0 is SOUTH; images are north-up


def road_score(z, cell):
    """p95 of local straight-orientation dominance — high means linear cuts
    (forest roads, firebreaks, powerline corridors, tank trails).
    Ported from tools/dictionary/p2_session.py:60."""
    gy, gx = np.gradient(z, cell)
    mag = np.hypot(gx, gy)
    bins = (np.mod(np.degrees(np.arctan2(gy, gx)), 180.0) / 15.0).astype(int) % 12
    w = max(1, int(256 / cell))
    s = []
    for y0 in range(0, z.shape[0] - w, w // 2):
        for x0 in range(0, z.shape[1] - w, w // 2):
            m = mag[y0:y0 + w, x0:x0 + w].ravel()
            tot = m.sum()
            if tot <= 1e-9:
                continue
            s.append((np.bincount(bins[y0:y0 + w, x0:x0 + w].ravel(),
                                  weights=m, minlength=12) / tot).max())
    return float(np.percentile(s, 95)) if s else 1.0


def leveled_frac(z, cell):
    """Share of the tile with near-zero fine-band texture — laser-levelled
    fields read this way even when OSM has no farmland polygon.
    The physical screen from extract_v2.py."""
    from scipy import ndimage
    sig = 64.0 / np.pi / cell
    fine = z - ndimage.gaussian_filter(z.astype(float), sig)
    w = 15
    m2 = ndimage.uniform_filter(fine ** 2, w)
    m1 = ndimage.uniform_filter(fine, w)
    return float((np.sqrt(np.maximum(m2 - m1 ** 2, 0)) < 0.04).mean())


def main():
    out = pathlib.Path(sys.argv[1] if len(sys.argv) > 1 else "/tmp/sand_review")
    out.mkdir(parents=True, exist_ok=True)
    ex = {t["tile"] for t in json.loads(EXCLUDE.read_text())["tiles"]
          if t["archetype"] == BIOME}
    th = load_thresholds()

    rows = []
    for p in sorted(OUT_TILES.glob("*.cgrid")):
        if p.stem in ex:
            continue
        dev = OUT_DEV / f"{p.stem}.json"
        z, (_, _, cell) = cgrid.read_f32(str(p))
        q = quantities(z, cell)
        A, lam, band = ds.spectral_order(z, cell)
        gy, gx = np.gradient(z.astype(float), cell)
        slope = np.degrees(np.arctan(np.hypot(gx, gy)))
        r = {
            "tile": p.stem,
            "lat": int(p.stem[1:6]) / 100.0,
            "lon": -int(p.stem[7:12]) / 100.0,
            "developed": json.loads(dev.read_text())["developed_frac"] if dev.exists() else None,
            "relief": q["relief_p95_p5"],
            "cap": q["frac_under_cap"],
            "steep": q["frac_under_steep"],
            "contig": q["largest_contig_ha"],
            "A": A, "lam": lam, "band": band,
            "slope_p50": float(np.percentile(slope, 50)),
            "slope_p95": float(np.percentile(slope, 95)),
            "road": road_score(z, cell),
            "leveled": leveled_frac(z, cell),
        }
        T = th["thresholds"]
        r["proxy_pass"] = bool(
            q["frac_under_cap"] >= T["frac_under_cap"]["floor"]
            and q["frac_under_steep"] >= T["frac_under_steep"]["floor"]
            and q["largest_contig_ha"] >= T["largest_contig_ha"]["floor"]
            and T["relief_p95_p5"]["band"][0] <= q["relief_p95_p5"] <= T["relief_p95_p5"]["band"][1])
        img = render(z, cell)
        try:
            from PIL import Image
            f = out / f"{p.stem}.jpg"
            Image.fromarray(img).save(f, quality=82)
        except ImportError:
            import imageio.v3 as iio
            f = out / f"{p.stem}.jpg"
            iio.imwrite(f, img, quality=82)
        r["img"] = f.name
        rows.append(r)
        print(f"  {p.stem}  relief {r['relief']:5.1f}  road {r['road']:.2f}  "
              f"level {r['leveled']*100:4.1f}%  A {A:.3f}", flush=True)

    (out / "rows.json").write_text(json.dumps(rows, indent=2))
    print(f"\n{len(rows)} tiles -> {out}")

    # advisory-flag summary
    hi_road = sum(1 for r in rows if r["road"] > 0.34)
    hi_lvl = sum(1 for r in rows if r["leveled"] > 0.15)
    print(f"  flagged linear-cut (road>0.34): {hi_road}")
    print(f"  flagged levelled  (>15% flat) : {hi_lvl}")


if __name__ == "__main__":
    main()
