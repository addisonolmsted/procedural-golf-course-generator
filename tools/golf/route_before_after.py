"""Before/after viewer for one routing change: two batch jsonl on the same tiles, stacked.

  python3 tools/golf/route_before_after.py <dump_dir> <before.jsonl> <after.jsonl> <out.html> \\
      [--pick metric] [--n 24] [--prefix m_] [--title ..]

Both records are audited in-process (`route_audit.audit`, terrain loaded
once per seed); the page header is the audit's table for both builds, then
the N courses whose picked per-course metric changed most (|after - before|,
seed as the tie-break), each as one crop shared by both builds -- the union
of the two routes' bounding boxes plus 60 m -- with the ground and the
water as fixed layers and the route overlay swapped by the before/after
toggle: hold `b` to see the build before the change (the mechanism of
tools/aeolian/mix_before_after.py).

The overlay is `route_sheet_json.draw_route` plus the audit's marks: the
play window as a thin rectangle, red rings on greens lying in another
hole's play (within 50 m; thick within 35 m), thick red segments for the
forced carries (v2 records), blue rings on landing zones and greens within
60 m of water.

--pick (per course, from the audit's course record):
  score          the router's score
  coverage       share of the window within 100 m of play
  edge           greens within 60 m of the window edge (count)
  above          the course's largest above-chord rise, m
  water          holes whose spine passes within 40 m of water (count)
  green_in_play  green-in-play cases within 50 m (count)
  length         sum over holes of |length - real median for its par|, m
  sequence       hole 2 a par 3 + hole 9 a par 5 (0, 1, 2)
  total          total length, m
  tee            tee boxes on > 15 % ground (count)
  setting        median green surround relief, m
"""
import sys, io, json, base64, pathlib
import numpy as np
from PIL import Image, ImageDraw
HERE = pathlib.Path(__file__).resolve().parent
for p in (HERE, HERE.parents[0] / "macro_campaign"):
    sys.path.insert(0, str(p))
import route_audit as RA                              # noqa: E402
from route_sheet_json import draw_route, route_bbox   # noqa: E402
from siting_sheet import hillshade_rgb                # noqa: E402

TITLE = "Routing, before and after"
BLURB = ("Hold <kbd>b</kbd> to see the routes before the change; the ground and the water are the same tile. "
         "Red rings: a green in another hole's play (thick: within 35 m). Blue rings: a landing zone or green within 60 m "
         "of water. Thick red: a forced carry. Thin rectangle: the play window.")
PAD_M, CARD_PX = 60.0, 640
WET_RGB = np.array([38, 86, 140])
PICKS = {
    "score": lambda c: c["score"], "coverage": lambda c: c["cov100"], "edge": lambda c: c["edge60"],
    "above": lambda c: c["above_max"], "water": lambda c: c["near_water"], "green_in_play": lambda c: c["n_gip50"],
    "length": lambda c: c["len_fit"], "sequence": lambda c: c["sequence"], "total": lambda c: c["total"],
    "tee": lambda c: c["tee15"], "setting": lambda c: c["g_sur"],
}


def uri(im, lossless=False, quality=82):
    b = io.BytesIO()
    im.save(b, "WEBP", lossless=lossless, quality=quality, method=4)
    return "data:image/webp;base64," + base64.b64encode(b.getvalue()).decode(), b.getbuffer().nbytes


def layers(t, recs, audits, px=CARD_PX):
    """(base RGB, water RGBA, [route RGBA per rec]) over the union crop of `recs`, `px` on the long side."""
    n, c = t.z.shape[0], t.c
    bb = np.array([route_bbox(r) for r in recs])
    y0, x0 = max(0.0, bb[:, 0].min() - PAD_M), max(0.0, bb[:, 1].min() - PAD_M)
    y1, x1 = min(n * c, bb[:, 2].max() + PAD_M), min(n * c, bb[:, 3].max() + PAD_M)
    a0, b0, a1, b1 = int(y0 / c), int(x0 / c), int(y1 / c), int(x1 / c)
    zc, wc = t.z[a0:a1, b0:b1], t.wet[a0:a1, b0:b1]
    base = hillshade_rgb(zc, c)
    scale = px / max(zc.shape)
    size = (max(1, int(zc.shape[1] * scale)), max(1, int(zc.shape[0] * scale)))
    b_im = Image.fromarray(np.flipud(base)).resize(size, Image.LANCZOS)
    wl = np.zeros(zc.shape + (4,), np.uint8); wl[..., :3] = base * 0.3 + WET_RGB * 0.7; wl[..., 3] = np.where(wc, 255, 0)
    w_im = Image.fromarray(np.flipud(wl)).resize(size, Image.NEAREST)
    H = size[1]
    def P(y, x):
        return ((x - x0) / c * scale, H - 1 - (y - y0) / c * scale)
    routes = []
    for rec, holes in zip(recs, audits):
        im = Image.new("RGBA", size, (0, 0, 0, 0))
        draw_route(ImageDraw.Draw(im), rec, P, {"mpp": scale / c, "holes": holes})
        routes.append(im)
    return b_im, w_im, routes


def main():
    a = sys.argv[1:]
    n_pick, prefix, pick, title = 24, "m_", "score", TITLE
    for k in ("--n", "--prefix", "--pick", "--title"):
        if k in a:
            i = a.index(k); v = a[i + 1]; a = a[:i] + a[i + 2:]
            if k == "--n": n_pick = int(v)
            elif k == "--prefix": prefix = v
            elif k == "--pick": pick = v
            else: title = v
    if len(a) < 4 or pick not in PICKS:
        print(__doc__); sys.exit(1)
    dump, pb, pa, out_html = pathlib.Path(a[0]), a[1], a[2], a[3]
    rb, ra = RA.load(pb), RA.load(pa)
    print(f"auditing {len(rb)} + {len(ra)} records", flush=True)
    (hb, cb), (ha, ca) = RA.audit(dump, prefix, [rb, ra], verbose=True)
    names = [pathlib.Path(pb).stem, pathlib.Path(pa).stem]
    table = RA.html_table(names, [RA.columns(rb, hb, cb), RA.columns(ra, ha, ca)])
    CB, CA = {c["seed"]: c for c in cb}, {c["seed"]: c for c in ca}
    HB, HA = {}, {}
    for h in hb: HB.setdefault(h["seed"], []).append(h)
    for h in ha: HA.setdefault(h["seed"], []).append(h)
    RB, RAf = {r["seed"]: r for r in rb if r.get("routed")}, {r["seed"]: r for r in ra if r.get("routed")}
    seeds = sorted(set(CB) & set(CA))
    f = PICKS[pick]
    chosen = sorted(seeds, key=lambda s: (-abs(f(CA[s]) - f(CB[s])), s))[:n_pick]
    cards, total = [], 0
    for s in chosen:
        t = RA.Terrain(dump, prefix, s)
        b_im, w_im, (o_b, o_a) = layers(t, [RB[s], RAf[s]], [HB[s], HA[s]])
        us = [uri(b_im), uri(w_im, lossless=True), uri(o_b, lossless=True), uri(o_a, lossless=True)]
        total += sum(u[1] for u in us)
        b, af = CB[s], CA[s]
        pars = lambda c: "".join(str(p) for p in c["pars"])
        meta = (f'pars <b>{pars(b)}</b> → <b>{pars(af)}</b> · total <b>{b["total"]:.0f}</b> → <b>{af["total"]:.0f}</b> m · '
                f'coverage <b>{100 * b["cov100"]:.0f}</b> → <b>{100 * af["cov100"]:.0f}</b> % · score <b>{b["score"]:.1f}</b> → <b>{af["score"]:.1f}</b> · '
                f'{pick} {f(b):.2f} → {f(af):.2f}')
        panel = (f'<figure class="panel" style="width:{b_im.width}px"><div class="stack" style="aspect-ratio:{b_im.width}/{b_im.height}">'
                 f'<img class="base" src="{us[0][0]}" alt="ground"><img class="water" src="{us[1][0]}" alt="water">'
                 f'<img class="before route" src="{us[2][0]}" alt="route before"><img class="after route" src="{us[3][0]}" alt="route after">'
                 f'<span class="tag"></span></div><figcaption>crop = both routes + {PAD_M:.0f} m · walk {b["walk"]:.0f} → {af["walk"]:.0f} m · '
                 f'crossings {b["crossings"]} → {af["crossings"]} · {b["seconds"]:.2f} → {af["seconds"]:.2f} s</figcaption></figure>')
        cards.append(f'<section class="card {b["mode"]}"><h3>seed {s} <span class="mode">{b["mode"]}</span><span class="meta">{meta}</span></h3>'
                     f'<div class="row">{panel}</div></section>')
        print(f"  {s}: {pick} {f(b):.2f} -> {f(af):.2f} ({total / 1e6:.1f} MB)", flush=True)
    legend = ('<span class="lg"><i style="background:rgb(60,170,80)"></i>par 3</span><span class="lg"><i style="background:rgb(60,110,220)"></i>par 4</span>'
              '<span class="lg"><i style="background:rgb(235,140,40)"></i>par 5</span><span class="lg"><i style="border:2px solid rgb(225,30,30)"></i>green in play</span>'
              '<span class="lg"><i style="border:2px solid rgb(40,110,230)"></i>near water</span><span class="lg"><i style="background:rgb(220,30,30)"></i>carry</span>')
    html = f"""<title>{title}</title>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap">
<style>
:root{{--bg:#f6f4ee;--panel:#fffdf8;--ink:#22231f;--muted:#5f5e57;--rule:#ddd8c9;--accent:#8a3b2a;--chip:#efeadc;--tag:#22231f;--tagink:#f6f4ee}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28;--tag:#e8e5dc;--tagink:#191a17}}}}
:root[data-theme="dark"]{{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28;--tag:#e8e5dc;--tagink:#191a17}}
*{{box-sizing:border-box}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:0 0 48px;line-height:1.45}}
header{{padding:26px 26px 14px;max-width:1700px;margin:0 auto;display:flex;gap:32px;flex-wrap:wrap;align-items:flex-start}}
header>div{{flex:1 1 40ch}}
h1{{font-size:1.5rem;margin:0 0 6px;letter-spacing:-.01em;text-wrap:balance}}
header p{{max-width:70ch;color:var(--muted);margin:0 0 8px}}
kbd{{font-family:"IBM Plex Mono",ui-monospace,monospace;border:1px solid var(--rule);border-radius:3px;padding:0 4px;font-size:.85em}}
.tbl{{overflow-x:auto;max-width:100%}}
table{{border-collapse:collapse;font-size:12.5px;font-variant-numeric:tabular-nums;font-family:"IBM Plex Mono",ui-monospace,monospace}}
td,th{{border-bottom:1px solid var(--rule);padding:3px 10px;text-align:right;white-space:nowrap}}td:first-child,th:first-child{{text-align:left;font-family:"IBM Plex Sans",system-ui,sans-serif}}
thead th{{color:var(--accent)}}
.controls{{position:sticky;top:0;z-index:10;background:color-mix(in srgb,var(--bg) 92%,transparent);backdrop-filter:blur(6px);
  border-block:1px solid var(--rule);padding:10px 26px;display:flex;gap:20px;flex-wrap:wrap;align-items:center;
  font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:13px}}
.controls label{{display:flex;align-items:center;gap:6px;cursor:pointer;user-select:none}}
.controls input{{accent-color:var(--accent);width:15px;height:15px}}
.controls input:focus-visible{{outline:2px solid var(--accent);outline-offset:2px}}
.seg{{display:inline-flex;border:1px solid var(--rule);border-radius:3px;overflow:hidden}}
.seg button{{background:var(--panel);color:var(--ink);border:0;padding:4px 12px;font:inherit;cursor:pointer}}
.seg button[aria-pressed="true"]{{background:var(--accent);color:var(--tagink)}}
.seg button:focus-visible{{outline:2px solid var(--accent);outline-offset:-2px}}
.legend{{display:flex;gap:12px;flex-wrap:wrap;margin-left:auto}}
.lg i{{display:inline-block;width:11px;height:11px;margin-right:4px;vertical-align:-1px;border-radius:50%}}
main{{max-width:1700px;margin:0 auto;padding:0 26px;display:grid;grid-template-columns:repeat(auto-fill,minmax(680px,1fr));gap:22px}}
.card{{background:var(--panel);border:1px solid var(--rule);border-radius:3px;padding:14px 16px 12px;margin-top:22px}}
.card h3{{margin:0 0 10px;font-size:.95rem;font-family:"IBM Plex Mono",ui-monospace,monospace;color:var(--accent);display:flex;gap:14px;align-items:baseline;flex-wrap:wrap}}
.mode{{color:var(--muted);font-weight:400}}
.meta{{color:var(--muted);font-size:.78rem;font-weight:400;flex-basis:100%}}.meta b{{color:var(--ink);font-weight:600}}
.row{{display:flex;gap:14px;flex-wrap:wrap;align-items:flex-start;overflow-x:auto}}
.panel{{margin:0;flex:none;max-width:100%;image-rendering:auto}}
.stack{{position:relative;width:100%;background:var(--chip)}}
.stack img{{position:absolute;inset:0;width:100%;height:100%;display:block}}
.stack .tag{{position:absolute;left:8px;top:8px;background:var(--tag);color:var(--tagink);font:11px "IBM Plex Mono",ui-monospace,monospace;padding:2px 7px;border-radius:2px;letter-spacing:.04em;text-transform:uppercase}}
.stack .tag::after{{content:"after"}}
body.before .stack .tag::after{{content:"before"}}
figcaption{{margin-top:5px;font-size:11.5px;color:var(--muted);font-family:"IBM Plex Mono",ui-monospace,monospace}}
body.before .after{{display:none}}
body:not(.before) .before{{display:none}}
body.no-water .water{{display:none}}
body.no-route .route{{display:none}}
body.mode-aeolian .card.fluvial{{display:none}}
body.mode-fluvial .card.aeolian{{display:none}}
</style>
<header>
<div><h1>{title}</h1><p>{BLURB}</p><p>{len(chosen)} courses shown, the ones whose <b>{pick}</b> changed most; the table covers all {len(seeds)}. Before: <code>{pb}</code>; after: <code>{pa}</code>.</p></div>
<div class="tbl">{table}</div>
</header>
<div class="controls">
<span class="seg" role="group" aria-label="build"><button id="b-before" aria-pressed="false">before</button><button id="b-after" aria-pressed="true">after</button></span>
<label><input type="checkbox" id="t-route" checked> routes</label>
<label><input type="checkbox" id="t-water" checked> water</label>
<label>mode <select id="s-mode"><option value="all">all</option><option value="aeolian">aeolian</option><option value="fluvial">fluvial</option></select></label>
<span class="legend">{legend}</span>
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
document.getElementById('t-route').onchange = e => b.classList.toggle('no-route', !e.target.checked);
document.getElementById('t-water').onchange = e => b.classList.toggle('no-water', !e.target.checked);
document.getElementById('s-mode').onchange = e => {{ b.classList.remove('mode-aeolian','mode-fluvial'); if (e.target.value !== 'all') b.classList.add('mode-' + e.target.value); }};
</script>"""
    open(out_html, "w").write(html)
    print(f"wrote {out_html}: {len(chosen)} courses, {pathlib.Path(out_html).stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
