"""Creek gallery: many finished fluvial tiles, with the water as a LAYER.

  python3 tools/aeolian/creek_gallery.py <dir> <out.html> <seed>...

Each view is rendered twice and stacked with CSS, so the reader can take the
water off and see the ground the creek cut, at full render quality, with no
re-drawing in the browser:

  base    tinted hillshade of the finished tile, NO water tint
  water   the wet cells only, as they are tinted in every other sheet
  creek   the planform centre-line (`Water::river`), off by default

Per seed: the whole 3 km tile and a 700 m detail crop on the creek.
Hillshade is the branch standard (`hillshade_rgb`, tools/golf/siting_sheet.py).
"""
import sys, io, base64, pathlib
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
for p in (HERE, HERE.parent / "golf", HERE.parent / "macro_campaign"):
    sys.path.insert(0, str(p))
from macro_campaign import cgrid
from siting_sheet import hillshade_rgb
from PIL import Image, ImageDraw

TILE_PX, CROP_M, CROP_ZOOM = 900, 700.0, 3
WET_RGB = np.array([38, 86, 140])

def uri(im, lossless=False, quality=88):
    b = io.BytesIO()
    im.save(b, "WEBP", lossless=lossless, quality=quality, method=4)
    return "data:image/webp;base64," + base64.b64encode(b.getvalue()).decode(), b.getbuffer().nbytes

def load(d, seed):
    z, (_, _, c) = cgrid.read_f32(pathlib.Path(d) / f"tex_{seed}.cgrid")
    w, _ = cgrid.read_f32(pathlib.Path(d) / f"tex_{seed}.water.cgrid")
    line = None
    lp = pathlib.Path(d) / f"tex_{seed}.creek.txt"
    if lp.exists() and lp.stat().st_size:
        line = np.loadtxt(lp)
    return z.astype(float), np.isfinite(w), c, line

def view(z, wet, cell, line, sl, out_px):
    """-> (base, water, creek) images at out_px, north-up."""
    zz, ww = z[sl], wet[sl]
    h, w = zz.shape
    base = hillshade_rgb(zz, cell)                    # untinted
    tinted = base.copy()
    tinted[ww] = tinted[ww] * 0.3 + WET_RGB * 0.7     # the standard water tint
    wl = np.zeros((h, w, 4), np.uint8)
    wl[..., :3] = tinted
    wl[..., 3] = np.where(ww, 255, 0)
    scale = out_px / w
    b_im = Image.fromarray(np.flipud(base)).resize((out_px, int(h * scale)), Image.LANCZOS)
    w_im = Image.fromarray(np.flipud(wl)).resize((out_px, int(h * scale)), Image.NEAREST)
    c_im = Image.new("RGBA", w_im.size, (0, 0, 0, 0))
    if line is not None and len(line) > 1:
        x0, y0 = sl[1].start, sl[0].start
        px = [((x / cell - x0) * scale, (h - 1 - (y / cell - y0)) * scale) for x, y in line]
        ImageDraw.Draw(c_im).line([(a, b) for a, b in px], fill=(214, 48, 36, 225),
                                  width=max(2, int(scale * 1.2)), joint="curve")
    return b_im, w_im, c_im

def creek_centre(wet, line, shape):
    if line is not None and len(line) > 1:
        p = line[len(line) // 2]
        return p[1], p[0]                              # (y_m, x_m)
    ys, xs = np.where(wet)
    return float(np.median(ys)), float(np.median(xs))

def main():
    d, out_html = sys.argv[1], sys.argv[2]
    seeds = sys.argv[3:]
    cards, total = [], 0
    for seed in seeds:
        try:
            z, wet, c, line = load(d, seed)
        except Exception as e:
            print(f"  {seed}: {e}"); continue
        n = z.shape[0]
        views = []
        # whole tile
        views.append(("3 km tile · 1 px = 4 m", view(z, wet, c, line, (slice(0, n), slice(0, n)), TILE_PX)))
        # detail crop on the creek
        cy_m, cx_m = creek_centre(wet, line, z.shape)
        half = int(CROP_M / c / 2)
        y0 = int(np.clip(cy_m / c - half, 0, n - 2 * half)); x0 = int(np.clip(cx_m / c - half, 0, n - 2 * half))
        sl = (slice(y0, y0 + 2 * half), slice(x0, x0 + 2 * half))
        views.append((f"{CROP_M:.0f} m detail · 1 px = {c/CROP_ZOOM:.0f} m", view(z, wet, c, line, sl, 2 * half * CROP_ZOOM)))
        panels = []
        for cap, (b_im, w_im, c_im) in views:
            (bu, bs), (wu, ws), (cu, cs) = uri(b_im), uri(w_im, lossless=True), uri(c_im, lossless=True)
            total += bs + ws + cs
            panels.append(f'<figure class="panel" style="width:{b_im.width}px"><div class="stack" style="aspect-ratio:{b_im.width}/{b_im.height}">'
                          f'<img class="base" src="{bu}" alt="terrain"><img class="water" src="{wu}" alt="water">'
                          f'<img class="creek" src="{cu}" alt="creek line"></div><figcaption>{cap}</figcaption></figure>')
        wet_pct = 100.0 * wet.sum() / wet.size
        cards.append(f'<section class="card"><h3>seed {seed}<span class="meta">water {wet_pct:.2f} % of the tile</span></h3>'
                     f'<div class="row">{"".join(panels)}</div></section>')
        print(f"  {seed}: ok ({total/1e6:.1f} MB so far)", flush=True)
    html = f"""<title>Carolina Creek Gallery</title>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap">
<style>
:root{{--bg:#f6f4ee;--panel:#fffdf8;--ink:#22231f;--muted:#5f5e57;--rule:#ddd8c9;--accent:#8a3b2a;--chip:#efeadc}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28}}}}
:root[data-theme="dark"]{{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28}}
*{{box-sizing:border-box}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:0 0 48px;line-height:1.45}}
header{{padding:26px 26px 14px;max-width:1600px;margin:0 auto}}
h1{{font-size:1.5rem;margin:0 0 6px;letter-spacing:-.01em;text-wrap:balance}}
header p{{max-width:74ch;color:var(--muted);margin:0}}
.controls{{position:sticky;top:0;z-index:10;background:color-mix(in srgb,var(--bg) 92%,transparent);backdrop-filter:blur(6px);
  border-block:1px solid var(--rule);padding:10px 26px;display:flex;gap:22px;flex-wrap:wrap;align-items:center;
  font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:13px}}
.controls label{{display:flex;align-items:center;gap:7px;cursor:pointer;user-select:none}}
.controls input{{accent-color:var(--accent);width:15px;height:15px}}
.controls input:focus-visible{{outline:2px solid var(--accent);outline-offset:2px}}
.controls .hint{{color:var(--muted);margin-left:auto}}
main{{max-width:1600px;margin:0 auto;padding:0 26px;display:flex;flex-direction:column;gap:26px}}
.card{{background:var(--panel);border:1px solid var(--rule);border-radius:3px;padding:14px 16px 12px;margin-top:22px}}
.card h3{{margin:0 0 10px;font-size:.95rem;font-family:"IBM Plex Mono",ui-monospace,monospace;color:var(--accent);
  display:flex;gap:14px;align-items:baseline}}
.meta{{color:var(--muted);font-size:.8rem;font-weight:400}}
.row{{display:flex;gap:14px;flex-wrap:wrap;align-items:flex-start;overflow-x:auto}}
.panel{{margin:0;flex:none;max-width:100%}}
.stack{{position:relative;width:100%;background:var(--chip)}}
.stack img{{position:absolute;inset:0;width:100%;height:100%;display:block}}
figcaption{{margin-top:5px;font-size:11.5px;color:var(--muted);font-family:"IBM Plex Mono",ui-monospace,monospace}}
body.no-water .water{{display:none}}
body.no-creek .creek{{display:none}}
</style>
<header>
<h1>Carolina creeks, {len(cards)} tiles</h1>
<p>Finished fluvial tiles with the corpus-shaped incision. Take the water off to see the ground the creek cut: the channel keeps the 2&nbsp;m texture across its banks and rejoins the untouched surface at the rim. Each seed shows the whole 3&nbsp;km tile and a 700&nbsp;m detail on the creek.</p>
</header>
<div class="controls">
<label><input type="checkbox" id="t-water" checked> water</label>
<label><input type="checkbox" id="t-creek"> creek centre-line</label>
<span class="hint">tinted hillshade, north up, sun from the north-west</span>
</div>
<main>
{"".join(cards)}
</main>
<script>
const b = document.body;
document.getElementById('t-water').onchange = e => b.classList.toggle('no-water', !e.target.checked);
document.getElementById('t-creek').onchange = e => b.classList.toggle('no-creek', !e.target.checked);
b.classList.add('no-creek');
</script>"""
    open(out_html, "w").write(html)
    print(f"wrote {out_html}: {len(cards)} tiles, {pathlib.Path(out_html).stat().st_size/1e6:.1f} MB")

if __name__ == "__main__":
    main()
