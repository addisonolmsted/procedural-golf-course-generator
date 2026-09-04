"""Before/after viewer for a creek-terrain change, many seeds, layers swapped in CSS.

  python3 tools/aeolian/creek_before_after.py <before_dir> <after_dir> <out.html> <seed>... \
      [--prefix a_] [--title ..] [--blurb ..] [--before-label ..] [--after-label ..]

Every view is rendered for both builds and stacked, so flipping between them
is a layer swap with nothing re-drawn: the eye can hold one spot and see what
moved. Per seed: the whole tile and a detail crop centred on the DEEPEST cut
of the before build (the number from creek_cutdepth.py), with that number and
the after build's beside the seed. Hillshade is the branch standard.
"""
import sys, pathlib
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import creek_gallery as G
import creek_cutdepth as C

TILE_PX, CROP_M, CROP_ZOOM = 720, 600.0, 2
TITLE = "Nebraska river: cut to grade, {n} seeds"
BLURB = ("The gorge's inner trench now carries the river to its graded bed where the valley cut saturates "
         "under a dune train; before, the water level held flat under ground it had not cut. "
         "Flip between the builds (or hold <kbd>b</kbd>); the detail is centred on the deepest cut of the before build.")
LABELS = ["before", "after"]


def main():
    global TITLE, BLURB
    a = sys.argv[1:]
    prefix = "a_"
    for k in ("--prefix", "--title", "--blurb", "--before-label", "--after-label"):
        if k in a:
            i = a.index(k); v = a[i + 1]; a = a[:i] + a[i + 2:]
            if k == "--prefix": prefix = v
            elif k == "--title": TITLE = v
            elif k == "--blurb": BLURB = v
            elif k == "--before-label": LABELS[0] = v
            else: LABELS[1] = v
    G.PREFIX = prefix
    db, da, out_html = a[0], a[1], a[2]
    seeds = a[3:]
    cards, total, rows = [], 0, []
    for seed in seeds:
        try:
            zb, wb, c, lb = G.load(db, seed)
            za, wa, _, la = G.load(da, seed)
            pb = C.profile(*C.load(db, seed, prefix)); sb = C.stats(pb)
            pa = C.profile(*C.load(da, seed, prefix)); sa = C.stats(pa)
        except Exception as e:
            print(f"  {seed}: {e}"); continue
        n = zb.shape[0]
        views = [("3 km tile", (slice(0, n), slice(0, n)), TILE_PX)]
        cx_m, cy_m = sb["deepest_xy"]
        half = int(CROP_M / c / 2)
        y0 = int(np.clip(cy_m / c - half, 0, n - 2 * half)); x0 = int(np.clip(cx_m / c - half, 0, n - 2 * half))
        views.append((f"{CROP_M:.0f} m at the deepest cut of the {LABELS[0]} build",
                      (slice(y0, y0 + 2 * half), slice(x0, x0 + 2 * half)), 2 * half * CROP_ZOOM))
        panels = []
        for cap, sl, px in views:
            b_b, w_b, c_b = G.view(zb, wb, c, lb, sl, px)
            b_a, w_a, c_a = G.view(za, wa, c, la, sl, px)
            (u1, s1), (u2, s2), (u3, s3), (u4, s4), (u5, s5) = (
                G.uri(b_b, quality=84), G.uri(w_b, lossless=True), G.uri(b_a, quality=84),
                G.uri(w_a, lossless=True), G.uri(c_a, lossless=True))
            total += s1 + s2 + s3 + s4 + s5
            panels.append(
                f'<figure class="panel" style="width:{b_b.width}px"><div class="stack" style="aspect-ratio:{b_b.width}/{b_b.height}">'
                f'<img class="before base" src="{u1}" alt="{LABELS[0]}"><img class="before water" src="{u2}" alt="water {LABELS[0]}">'
                f'<img class="after base" src="{u3}" alt="{LABELS[1]}"><img class="after water" src="{u4}" alt="water {LABELS[1]}">'
                f'<img class="creek" src="{u5}" alt="creek line"><span class="tag"></span></div><figcaption>{cap}</figcaption></figure>')
        rows.append((seed, sb, sa))
        meta = (f'cut p90 <b>{sb["cut_p90"]:.1f}</b> → <b>{sa["cut_p90"]:.1f}</b> m · '
                f'max <b>{sb["cut_max"]:.1f}</b> → <b>{sa["cut_max"]:.1f}</b> m · '
                f'bank slope p99 <b>{sb["slope_p99"]:.0f}</b> → <b>{sa["slope_p99"]:.0f}</b> %')
        cards.append(f'<section class="card"><h3>seed {seed}<span class="meta">{meta}</span></h3>'
                     f'<div class="row">{"".join(panels)}</div></section>')
        print(f"  {seed}: cut max {sb['cut_max']:.1f} -> {sa['cut_max']:.1f} m ({total/1e6:.1f} MB so far)", flush=True)
    med = lambda k, j: np.median([r[j][k] for r in rows])
    summary = (f'<table><thead><tr><th>median over {len(rows)} seeds</th><th>{LABELS[0]}</th><th>{LABELS[1]}</th></tr></thead><tbody>'
               f'<tr><td>cut depth p90, m</td><td>{med("cut_p90",1):.1f}</td><td>{med("cut_p90",2):.1f}</td></tr>'
               f'<tr><td>cut depth max, m</td><td>{med("cut_max",1):.1f}</td><td>{med("cut_max",2):.1f}</td></tr>'
               f'<tr><td>worst cut on any seed, m</td><td>{max(r[1]["cut_max"] for r in rows):.1f}</td><td>{max(r[2]["cut_max"] for r in rows):.1f}</td></tr>'
               f'<tr><td>ground slope within 30 m, p99 %</td><td>{med("slope_p99",1):.0f}</td><td>{med("slope_p99",2):.0f}</td></tr>'
               f'<tr><td>water grade over 10 m, max %</td><td>{med("grade_max",1):.1f}</td><td>{med("grade_max",2):.1f}</td></tr>'
               f'</tbody></table>')
    html = f"""<title>Nebraska Cut to Grade</title>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap">
<style>
:root{{--bg:#f6f4ee;--panel:#fffdf8;--ink:#22231f;--muted:#5f5e57;--rule:#ddd8c9;--accent:#8a3b2a;--chip:#efeadc;--tag:#22231f;--tagink:#f6f4ee}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28;--tag:#e8e5dc;--tagink:#191a17}}}}
:root[data-theme="dark"]{{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28;--tag:#e8e5dc;--tagink:#191a17}}
*{{box-sizing:border-box}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:0 0 48px;line-height:1.45}}
header{{padding:26px 26px 14px;max-width:1600px;margin:0 auto;display:flex;gap:32px;flex-wrap:wrap;align-items:flex-start}}
header div{{flex:1 1 40ch}}
h1{{font-size:1.5rem;margin:0 0 6px;letter-spacing:-.01em;text-wrap:balance}}
header p{{max-width:70ch;color:var(--muted);margin:0}}
kbd{{font-family:"IBM Plex Mono",ui-monospace,monospace;border:1px solid var(--rule);border-radius:3px;padding:0 4px;font-size:.85em}}
table{{border-collapse:collapse;font-size:12.5px;font-variant-numeric:tabular-nums;font-family:"IBM Plex Mono",ui-monospace,monospace}}
td,th{{border-bottom:1px solid var(--rule);padding:4px 12px;text-align:right}}td:first-child,th:first-child{{text-align:left;font-family:"IBM Plex Sans",system-ui,sans-serif}}
.controls{{position:sticky;top:0;z-index:10;background:color-mix(in srgb,var(--bg) 92%,transparent);backdrop-filter:blur(6px);
  border-block:1px solid var(--rule);padding:10px 26px;display:flex;gap:22px;flex-wrap:wrap;align-items:center;
  font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:13px}}
.controls label{{display:flex;align-items:center;gap:7px;cursor:pointer;user-select:none}}
.controls input{{accent-color:var(--accent);width:15px;height:15px}}
.controls input:focus-visible{{outline:2px solid var(--accent);outline-offset:2px}}
.seg{{display:inline-flex;border:1px solid var(--rule);border-radius:3px;overflow:hidden}}
.seg button{{background:var(--panel);color:var(--ink);border:0;padding:4px 12px;font:inherit;cursor:pointer}}
.seg button[aria-pressed="true"]{{background:var(--accent);color:var(--tagink)}}
.seg button:focus-visible{{outline:2px solid var(--accent);outline-offset:-2px}}
.controls .hint{{color:var(--muted);margin-left:auto}}
main{{max-width:1600px;margin:0 auto;padding:0 26px;display:flex;flex-direction:column;gap:26px}}
.card{{background:var(--panel);border:1px solid var(--rule);border-radius:3px;padding:14px 16px 12px;margin-top:22px}}
.card h3{{margin:0 0 10px;font-size:.95rem;font-family:"IBM Plex Mono",ui-monospace,monospace;color:var(--accent);
  display:flex;gap:14px;align-items:baseline;flex-wrap:wrap}}
.meta{{color:var(--muted);font-size:.8rem;font-weight:400}}.meta b{{color:var(--ink);font-weight:600}}
.row{{display:flex;gap:14px;flex-wrap:wrap;align-items:flex-start;overflow-x:auto}}
.panel{{margin:0;flex:none;max-width:100%}}
.stack{{position:relative;width:100%;background:var(--chip)}}
.stack img{{position:absolute;inset:0;width:100%;height:100%;display:block}}
.stack .tag{{position:absolute;left:8px;top:8px;background:var(--tag);color:var(--tagink);font:11px "IBM Plex Mono",ui-monospace,monospace;
  padding:2px 7px;border-radius:2px;letter-spacing:.04em;text-transform:uppercase}}
.stack .tag::after{{content:"{LABELS[1]}"}}
body.before .stack .tag::after{{content:"{LABELS[0]}"}}
figcaption{{margin-top:5px;font-size:11.5px;color:var(--muted);font-family:"IBM Plex Mono",ui-monospace,monospace}}
body.before .after{{display:none}}
body:not(.before) .before{{display:none}}
body.no-water .water{{display:none}}
body.no-creek .creek{{display:none}}
</style>
<header>
<div><h1>{TITLE.format(n=len(cards))}</h1><p>{BLURB}</p></div>
{summary}
</header>
<div class="controls">
<span class="seg" role="group" aria-label="build"><button id="b-before" aria-pressed="false">{LABELS[0]}</button><button id="b-after" aria-pressed="true">{LABELS[1]}</button></span>
<label><input type="checkbox" id="t-water" checked> water</label>
<label><input type="checkbox" id="t-creek"> creek centre-line</label>
<span class="hint">hold <kbd>b</kbd> to see {LABELS[0]} · tinted hillshade, north up</span>
</div>
<main>
{"".join(cards)}
</main>
<script>
const b = document.body, bb = document.getElementById('b-before'), ba = document.getElementById('b-after');
let sticky = false;
function show(before) {{ b.classList.toggle('before', before); bb.setAttribute('aria-pressed', before); ba.setAttribute('aria-pressed', !before); }}
bb.onclick = () => {{ sticky = true; show(true); }};
ba.onclick = () => {{ sticky = false; show(false); }};
document.addEventListener('keydown', e => {{ if (e.key === 'b' && !e.repeat) show(!sticky); }});
document.addEventListener('keyup', e => {{ if (e.key === 'b') show(sticky); }});
document.getElementById('t-water').onchange = e => b.classList.toggle('no-water', !e.target.checked);
document.getElementById('t-creek').onchange = e => b.classList.toggle('no-creek', !e.target.checked);
b.classList.add('no-creek');
</script>"""
    open(out_html, "w").write(html)
    print(f"wrote {out_html}: {len(cards)} seeds, {pathlib.Path(out_html).stat().st_size/1e6:.1f} MB")


if __name__ == "__main__":
    main()
