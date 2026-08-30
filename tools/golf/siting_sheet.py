#!/usr/bin/env python3
"""The siting contact sheet + approach roses. (Verification instruments 1+2.)

The project rule is render before trusting metrics, and the approach scores
have no numeric ground truth at all — a human looking at a hillshade can say
"that's blind from the north" in a second, so the rose must be drawn ON the
hillshade at a scale where that judgement is possible.

    python3 tools/golf/siting_sheet.py <dump_dir> <out_html> seed:mode ...

`mode` picks play_m: aeolian 1200, fluvial 800.
"""
from __future__ import annotations

import base64
import io
import pathlib
import sys

import numpy as np

_TOOLS = pathlib.Path(__file__).resolve().parent.parent
for _p in ("macro_campaign", "golf"):
    sys.path.insert(0, str(_TOOLS / _p))

from macro_campaign import cgrid                      # noqa: E402
import siting                                         # noqa: E402
import greens as greens_mod                           # noqa: E402

# course-lab::render_terrain, the branch-standard tinted hillshade
Z_EXAG = 2.4
RAMP = [(0.00, (0.34, 0.50, 0.29)), (0.40, (0.62, 0.62, 0.38)),
        (0.75, (0.78, 0.68, 0.44)), (1.00, (0.87, 0.79, 0.60))]
TYPE_COLORS = {
    "bench": (66, 135, 245), "spur": (245, 130, 48), "saddle": (145, 30, 180),
    "punchbowl": (60, 180, 75), "plateau": (240, 50, 230), "dell": (70, 240, 240),
    "valley_flat": (210, 245, 60), "knoll": (250, 190, 212), "generic": (128, 128, 128), "hot": (255, 64, 160),
}


def hillshade_rgb(z, cell, wet=None):
    az, alt = np.radians(315.0), np.radians(45.0)
    L = np.array([np.cos(az) * np.cos(alt), np.sin(az) * np.cos(alt), np.sin(alt)])
    gy, gx = np.gradient(z * Z_EXAG, cell)
    lam = ((-gx) * L[0] + (-gy) * L[1] + L[2]) / np.sqrt(gx ** 2 + gy ** 2 + 1.0)
    shade = np.clip(lam, 0.0, None) ** 1.15
    lo, hi = np.percentile(z, 2), np.percentile(z, 98)
    t = np.clip((z - lo) / max(hi - lo, 1e-6), 0, 1)
    rgb = np.zeros(z.shape + (3,))
    for i in range(len(RAMP) - 1):
        a, ca = RAMP[i]
        b, cb = RAMP[i + 1]
        m = (t >= a) & (t <= b)
        f = np.where(m, (t - a) / (b - a), 0)
        for c in range(3):
            rgb[..., c] = np.where(m, ca[c] + (cb[c] - ca[c]) * f, rgb[..., c])
    img = np.clip((rgb * 0.85 + 0.15) * (0.25 + 0.95 * shade)[..., None] * 235, 0, 255)
    if wet is not None:
        img[wet] = img[wet] * 0.3 + np.array([38, 86, 140]) * 0.7
    return img.astype(np.uint8)


def to_uri(img_arr, upscale=1):
    from PIL import Image
    im = Image.fromarray(np.flipud(img_arr))
    if upscale > 1:
        im = im.resize((im.width * upscale, im.height * upscale), Image.NEAREST)
    b = io.BytesIO()
    im.save(b, "WEBP", quality=88)
    return "data:image/webp;base64," + base64.b64encode(b.getvalue()).decode()


def draw_rect(img, y0, x0, hw, color, thick=2):
    h, w = hw if isinstance(hw, tuple) else (hw, hw)
    y1, x1 = y0 + h, x0 + w
    for t in range(thick):
        img[np.clip(y0 + t, 0, img.shape[0] - 1), np.clip(x0, 0, img.shape[1] - 1):np.clip(x1, 0, img.shape[1] - 1)] = color
        img[np.clip(y1 - t, 0, img.shape[0] - 1), np.clip(x0, 0, img.shape[1] - 1):np.clip(x1, 0, img.shape[1] - 1)] = color
        img[np.clip(y0, 0, img.shape[0] - 1):np.clip(y1, 0, img.shape[0] - 1), np.clip(x0 + t, 0, img.shape[1] - 1)] = color
        img[np.clip(y0, 0, img.shape[0] - 1):np.clip(y1, 0, img.shape[0] - 1), np.clip(x1 - t, 0, img.shape[1] - 1)] = color


def draw_disc(img, y, x, r, color):
    yy, xx = np.mgrid[-r:r + 1, -r:r + 1]
    m = yy * yy + xx * xx <= r * r
    ys, xs = np.where(m)
    py = np.clip(y + ys - r, 0, img.shape[0] - 1)
    px = np.clip(x + xs - r, 0, img.shape[1] - 1)
    img[py, px] = color


def contact_sheet(z2, c2, wet2, sit, f, pool):
    """Full-core hillshade with windows, clubhouse and typed candidates."""
    img = hillshade_rgb(f.z8, f.cell, f.wet8)
    # shortlisted windows, faint (shortlist ij are core-relative; the core
    # starts at cell 750/8)
    core0 = int(round(750.0 / f.cell))
    for (oij, ws, cs, loop) in sit.shortlist:
        oi, i, j = oij
        scan = sit.scans[oi]
        draw_rect(img, core0 + i, core0 + j, (scan.h, scan.w), (200, 200, 200), 1)
    # the chosen window, bold
    draw_rect(img, sit.window_ij[0], sit.window_ij[1],
              (int(round(sit.window_m[2] / f.cell)),
               int(round(sit.window_m[3] / f.cell))), (255, 255, 255), 2)
    # clubhouse
    cy, cx = int(sit.clubhouse.yx[0] / f.cell), int(sit.clubhouse.yx[1] / f.cell)
    draw_disc(img, cy, cx, 4, (230, 40, 40))
    # candidates by type, sized by rank
    ranked = sorted(pool, key=lambda c: -c.score)
    for rank, c in enumerate(ranked):
        y, x = int(c.yx[0] / f.cell), int(c.yx[1] / f.cell)
        r = 3 if rank < 20 else 2
        draw_disc(img, y, x, r, TYPE_COLORS[c.kind])
        if c.reserved:
            draw_disc(img, y, x, 1, (255, 255, 255))
    return to_uri(img, upscale=2)


def rose_card(z2, c2, wet2, cand, f):
    """200 m 2 m-hillshade crop with the 16-spoke approach rose."""
    r = int(100.0 / c2)
    y, x = int(cand.yx[0] / c2), int(cand.yx[1] / c2)
    y0, x0 = max(0, y - r), max(0, x - r)
    crop = z2[y0:y0 + 2 * r, x0:x0 + 2 * r]
    wcrop = wet2[y0:y0 + 2 * r, x0:x0 + 2 * r]
    img = hillshade_rgb(crop, c2, wcrop)
    cy, cx = y - y0, x - x0
    for k in range(greens_mod.N_BEARINGS):
        th = 2 * np.pi * k / greens_mod.N_BEARINGS
        vis = cand.approach[k, 0]
        rec = cand.approach[k, 1]
        ln = int((0.15 + 0.85 * vis) * 40)
        col = ((60, 220, 60) if rec > 0.005 else
               (230, 60, 60) if rec < -0.005 else (235, 220, 60))
        for s in range(8, ln):
            py = int(np.clip(cy + np.sin(th) * s, 0, img.shape[0] - 1))
            px = int(np.clip(cx + np.cos(th) * s, 0, img.shape[1] - 1))
            img[py, px] = col
    draw_disc(img, cy, cx, 3, (255, 255, 255))
    return to_uri(img, upscale=3)


def main():
    dump = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    entries = []
    for spec in sys.argv[3:]:
        seed, mode = spec.split(":")
        dims = (1450.0, 950.0) if mode == "aeolian" else (1000.0, 700.0)
        z2, (_, _, c2) = cgrid.read_f32(dump / f"{seed}.cgrid")
        wp = dump / f"{seed}.water.cgrid"
        wet2 = ~np.isnan(cgrid.read_f32(wp)[0]) if wp.exists() else np.zeros(z2.shape, bool)
        sit, f, m, p = siting.run_siting(z2, c2, wet2, *dims)
        pool = greens_mod.generate(z2, c2, wet2, sit, f, m, p)
        sheet = contact_sheet(z2, c2, wet2, sit, f, pool)
        ranked = sorted(pool, key=lambda c: -c.score)
        picks = ranked[:4] + ranked[len(ranked) // 2:len(ranked) // 2 + 2]
        roses = [(c.kind, f"{c.score:.2f}", rose_card(z2, c2, wet2, c, f))
                 for c in picks]
        from collections import Counter
        # relief_pos of the chosen window: the crest-flat trap detector
        # (d6w finding 5 -- real sandhills courses sit at ~0.43 on floor
        # systems at 0.18; generated crest-flats read ~0.56)
        wi, wj = sit.window_ij
        hh = int(round(sit.window_m[2] / f.cell))
        ww = int(round(sit.window_m[3] / f.cell))
        wpos = float(f.relief_pos[wi:wi + hh, wj:wj + ww].mean())
        entries.append(dict(seed=seed, mode=mode,
                            play_m=f"{dims[0]:.0f}x{dims[1]:.0f}", sheet=sheet,
                            roses=roses, n=len(pool), wpos=wpos,
                            types=dict(Counter(c.kind for c in pool)),
                            loop=max(r[3] for r in sit.shortlist)))
        print(f"  {seed} ({mode}): {len(pool)} candidates, "
              f"loop {entries[-1]['loop']}, wpos {wpos:.2f}")

    legend = " ".join(
        f'<span class="chip" style="background:rgb{TYPE_COLORS[t]}"></span>{t}'
        for t in TYPE_COLORS)
    body = ""
    for e in entries:
        roses = "".join(
            f'<figure class="rose"><img src="{u}">'
            f'<figcaption>{k} · {s}</figcaption></figure>'
            for k, s, u in e["roses"])
        body += f"""<section>
<h2>seed {e['seed']} — {e['mode']}, {e['play_m']} m window</h2>
<p>{e['n']} candidates · loop closure {e['loop']}/9 · window relief-pos
{e['wpos']:.2f} · {e['types']}</p>
<div class="row"><figure class="sheet"><img src="{e['sheet']}">
<figcaption>core · shortlisted windows grey, chosen white, clubhouse red</figcaption></figure>
<div class="roses">{roses}</div></div>
</section><hr>"""

    html = f"""<title>Siting contact sheet</title>
<style>
:root{{--bg:#f4f2ec;--ink:#211f1b;--dim:#6a655c;--line:#d9d4c8;--card:#fffdf9}}
@media (prefers-color-scheme:dark){{:root:not([data-theme=light]){{--bg:#141410;--ink:#e8e4db;--dim:#989287;--line:#312d26;--card:#1d1c17}}}}
:root[data-theme=dark]{{--bg:#141410;--ink:#e8e4db;--dim:#989287;--line:#312d26;--card:#1d1c17}}
*{{box-sizing:border-box}}
body{{background:var(--bg);color:var(--ink);margin:0;padding:2rem 1.4rem 4rem;font:15px/1.55 ui-sans-serif,system-ui}}
main{{max-width:1280px;margin:0 auto}}
h1{{font-size:1.6rem}} h2{{font-size:1.05rem;margin:.2rem 0}}
p{{color:var(--dim);margin:.2rem 0 .8rem}}
hr{{border:0;border-top:1px solid var(--line);margin:1.6rem 0}}
.row{{display:grid;grid-template-columns:minmax(380px,1.4fr) 1fr;gap:1rem;align-items:start}}
.sheet img{{width:100%;image-rendering:pixelated}}
.roses{{display:grid;grid-template-columns:repeat(3,1fr);gap:.6rem}}
.rose img{{width:100%;image-rendering:pixelated}}
figure{{margin:0;background:var(--card);border:1px solid var(--line);border-radius:3px;overflow:hidden}}
figcaption{{padding:.3rem .5rem;font-size:11.5px;color:var(--dim)}}
.chip{{display:inline-block;width:.85em;height:.85em;border-radius:2px;margin:0 .3em 0 .9em;vertical-align:-.1em}}
</style>
<main><h1>Siting contact sheet — windows, clubhouse, typed green candidates</h1>
<p>Rose spokes: length = visibility of the putting surface from that bearing
(60–200 m out, eye 1.6 m); colour = receptivity (green tilts toward the
golfer / red away / yellow flat). Legend: {legend}</p><hr>
{body}</main>"""
    out.write_text(html)
    print(f"wrote {out} ({out.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
