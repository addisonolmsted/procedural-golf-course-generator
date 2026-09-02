"""Creek PLACEMENT sheet: the ground WITHOUT the creek, the creek's line
drawn over it, toggleable.

  python3 tools/aeolian/creek_overlay_sheet.py <nocarve_dir> <carved_dir> <out.html> <seed>...

Per seed: the whole tile (1 px per 2 m cell) and three 600 m crops along the
creek at 2x, each a stack of layers the reader toggles with checkboxes:
  base        tinted hillshade of the tile built with CREEK_CARVE=none
              (ponds still present; the creek neither cut nor wetted)
  carved      the same tile with the channel carve, swapped in for the base
  creek line  the planform centre-line (`Water::river`)
  floor edge  the u = 0.34 contour -- the room the planform is confined to

Everything is rendered as PNG/WebP layers at identical pixel size and stacked
with CSS, so the toggles are instant and nothing is re-drawn in the browser.
Hillshade is the branch standard (`hillshade_rgb`, tools/golf/siting_sheet.py).
"""
import sys, io, base64, pathlib
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "golf")); sys.path.insert(0, str(HERE.parent / "macro_campaign"))
from macro_campaign import cgrid
from siting_sheet import hillshade_rgb
from scipy import ndimage
from PIL import Image, ImageDraw

def uri(im, fmt="WEBP", **kw):
    b = io.BytesIO(); im.save(b, fmt, **kw)
    return f"data:image/{fmt.lower()};base64," + base64.b64encode(b.getvalue()).decode()

def load(d, seed):
    z, (_, _, c) = cgrid.read_f32(pathlib.Path(d) / f"tex_{seed}.cgrid")
    w, _ = cgrid.read_f32(pathlib.Path(d) / f"tex_{seed}.water.cgrid")
    return z, np.isfinite(w), c

def floor_edge(u8, c8, shape, c):
    """u = 0.34 boundary at the 2 m grid, as a boolean line mask."""
    zoom = (shape[0] / u8.shape[0], shape[1] / u8.shape[1])
    u = ndimage.zoom(u8, zoom, order=1)
    inside = u < 0.34
    return inside & ~ndimage.binary_erosion(inside, iterations=1)

def layer_line(shape, pts_px, color, width):
    im = Image.new("RGBA", (shape[1], shape[0]), (0, 0, 0, 0))
    ImageDraw.Draw(im).line([tuple(p) for p in pts_px], fill=color, width=width, joint="curve")
    return im

def layer_mask(mask, color):
    rgba = np.zeros(mask.shape + (4,), np.uint8)
    rgba[mask] = color
    return Image.fromarray(rgba)

def panel(base_rgb, carved_rgb, creek_px, edge_mask, scale, cap):
    """One layered panel. Arrays are row = y (north-up after flipud)."""
    h, w = base_rgb.shape[:2]
    def up(im):
        return im if scale == 1 else im.resize((w * scale, h * scale), Image.LANCZOS if im.mode == "RGB" else Image.NEAREST)
    base = up(Image.fromarray(np.flipud(base_rgb)))
    carved = up(Image.fromarray(np.flipud(carved_rgb)))
    # line layer drawn at the final pixel size (y flipped)
    pts = [((x) * scale, (h - 1 - y) * scale) for x, y in creek_px]
    line = layer_line((h * scale, w * scale), pts, (214, 48, 36, 235), max(2, scale))
    edge = up(layer_mask(np.flipud(edge_mask), (255, 236, 90, 200)))
    W, H = base.size
    return (f'<div class="panel" style="width:{W}px;height:{H}px">'
            f'<img class="base" src="{uri(base, quality=86)}">'
            f'<img class="carved" src="{uri(carved, quality=86)}">'
            f'<img class="edge" src="{uri(edge, "PNG")}">'
            f'<img class="creek" src="{uri(line, "PNG")}">'
            f'<div class="cap">{cap}</div></div>')

def main():
    d_no, d_cv, out_html = sys.argv[1:4]; seeds = sys.argv[4:]
    parts = []
    for seed in seeds:
        z0, wet0, c = load(d_no, seed)
        z1, wet1, _ = load(d_cv, seed)
        u8, (_, _, c8) = cgrid.read_f32(pathlib.Path(d_no) / f"tex_{seed}.u.cgrid")
        line = np.loadtxt(pathlib.Path(d_no) / f"tex_{seed}.creek.txt")
        px = line / c                       # (x, y) in cell units
        edge = floor_edge(u8, c8, z0.shape, c)
        base = hillshade_rgb(z0, c, wet0)
        carved = hillshade_rgb(z1, c, wet1)
        parts.append(f"<h3>seed {seed}</h3>")
        parts.append('<div class="row">' + panel(base, carved, px, edge, 1, "whole tile, 3 km, 1 px = 2 m") + "</div>")
        # three crops along the creek, at 2x
        n = len(px); half = int(300 / c)
        crops = []
        for f in (0.2, 0.5, 0.8):
            cx, cy = px[int(f * (n - 1))]
            x0 = int(np.clip(cx - half, 0, z0.shape[1] - 2 * half)); y0 = int(np.clip(cy - half, 0, z0.shape[0] - 2 * half))
            sl = (slice(y0, y0 + 2 * half), slice(x0, x0 + 2 * half))
            sub = px - np.array([x0, y0])
            crops.append(panel(base[sl], carved[sl], sub, edge[sl], 2, f"600 m crop at {int(f*100)} % along the creek, 2x"))
        parts.append('<div class="row">' + "".join(crops) + "</div>")
    html = f"""<title>Creek Placement Overlay</title>
<style>
:root{{--bg:#f6f4ee;--ink:#22231f;--muted:#5f5e57;--rule:#d9d5c9;--accent:#8a3b2a;--panel:#fffdf8}}
@media (prefers-color-scheme: dark){{:root:not([data-theme="light"]){{--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420}}}}
:root[data-theme="dark"]{{--bg:#1b1c19;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--panel:#232420}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:20px 24px;line-height:1.45}}
h2{{font-size:1.45rem;margin:0 0 6px}}h3{{font-size:1.05rem;margin:26px 0 6px;color:var(--accent)}}
p{{max-width:70ch;color:var(--muted);margin:4px 0 10px}}
.controls{{position:sticky;top:0;z-index:5;background:var(--bg);padding:8px 0 10px;border-bottom:1px solid var(--rule);display:flex;gap:18px;flex-wrap:wrap;font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:13px}}
.controls label{{display:flex;align-items:center;gap:6px;cursor:pointer}} .controls input:focus-visible{{outline:2px solid var(--accent)}}
.row{{display:flex;gap:10px;flex-wrap:wrap;align-items:flex-start;overflow-x:auto}}
.panel{{position:relative;background:var(--panel);flex:none}} .panel img{{position:absolute;left:0;top:0;width:100%;height:100%;image-rendering:auto}}
.panel .carved{{display:none}} body.show-carved .panel .carved{{display:block}}
body.hide-creek .panel .creek{{display:none}} body.hide-edge .panel .edge{{display:none}}
.panel .edge{{opacity:.85}}
.cap{{position:absolute;left:6px;bottom:4px;font-size:11px;color:#fff;text-shadow:0 0 3px #000,0 0 3px #000;font-family:"IBM Plex Mono",ui-monospace,monospace}}
</style>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap">
<h2>Where the creek sits: the ground without it, the line over it</h2>
<p>Built with <code>CREEK_CARVE=none</code>: the planform is computed but nothing is cut or wetted, so what you see is the valley the earlier stages made. Ponds are still there. Toggle the creek line and the valley-floor edge (u = 0.34, the room the planform is confined to); "carved ground" swaps in the tile with the channel cut, for reference only.</p>
<div class="controls">
<label><input type="checkbox" id="t-creek" checked> creek line (red)</label>
<label><input type="checkbox" id="t-edge"> floor edge u = 0.34 (yellow)</label>
<label><input type="checkbox" id="t-carved"> carved ground instead of bare ground</label>
</div>
{''.join(parts)}
<script>
const b=document.body;
document.getElementById('t-creek').onchange=e=>b.classList.toggle('hide-creek',!e.target.checked);
document.getElementById('t-edge').onchange=e=>b.classList.toggle('hide-edge',!e.target.checked);
document.getElementById('t-carved').onchange=e=>b.classList.toggle('show-carved',e.target.checked);
b.classList.add('hide-edge');
</script>"""
    open(out_html, "w").write(html)
    print(f"wrote {out_html} ({len(html)//1024} KB)")

if __name__ == "__main__":
    main()
