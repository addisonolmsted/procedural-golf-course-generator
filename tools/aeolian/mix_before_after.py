"""Before/after viewer for one fix over the mixed run: two screened dirs, stacked.

  python3 tools/aeolian/mix_before_after.py <before_dir> <after_dir> <out.html> \
      [--n 24] [--title ..] [--blurb ..] [--prefix m_] [--pick score|wet|lake|perched|...]

Both dirs must have been screened (`mix_screen.py` writes `screen/<seed>.json`
and the flag masks). The page: the screen's summary table for both builds side
by side, then the N tiles whose score (or the picked class) changed most,
each as the whole tile plus a 500 m detail at the before build's worst spot,
both builds stacked so flipping (hold `b`) swaps the ground, the water and
the flags with nothing redrawn.
"""
import sys, json, pathlib
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
import mix_screen as M

TITLE = "Mixed run, before and after"
BLURB = "Hold <kbd>b</kbd> to see the build before the change. Flags are the screen's, computed on each build."
DETAIL_M, TILE_PX = 500.0, 480


def largest_body_xy(w, line, c):
    """World (x, y) of the centroid of the largest standing body: wet cells not within 3 cells of the creek line."""
    from scipy import ndimage as ndi
    wet = np.isfinite(w)
    if line is not None and len(line):
        on = np.zeros_like(wet)
        ii = np.clip((line[:, 1] / c).astype(int), 0, wet.shape[0] - 1); jj = np.clip((line[:, 0] / c).astype(int), 0, wet.shape[1] - 1)
        on[ii, jj] = True
        wet = wet & ~ndi.binary_dilation(on, iterations=3)
    lab, n = ndi.label(wet)
    if n == 0: return None
    sizes = ndi.sum(wet, lab, range(1, n + 1)); k = int(np.argmax(sizes)) + 1
    if sizes[k - 1] * c * c < 500: return None
    iy, ix = ndi.center_of_mass(lab == k)
    return [float(ix * c), float(iy * c)]


def load_rows(d):
    sd = pathlib.Path(d) / "screen"
    return {p.stem: json.load(open(p)) for p in sd.glob("*.json")}


def agg(rows, seeds, mode):
    ss = [s for s in seeds if rows[s]["mode"] == mode] if mode != "all" else list(seeds)
    if not ss:
        return None
    r = {"n": len(ss), "flagged": sum(bool(rows[s]["notable"]) for s in ss)}
    for k in M.CLASSES:
        r[k] = f'{sum(rows[s]["cells"][k] > 0 for s in ss)} / {sum(k in rows[s]["notable"] for s in ss)}'
    r["ha_" + "perched"] = float(np.median([rows[s]["ha"]["perched"] for s in ss]))
    r["wet"] = float(np.median([rows[s]["wet_pct"] for s in ss]))
    r["slope_p99"] = float(np.median([rows[s]["slope_p99"] for s in ss]))
    cm = [rows[s]["creek"].get("cut_max", np.nan) for s in ss if rows[s]["creek"]]
    r["cut_max"] = float(np.nanmedian(cm)) if cm else float("nan")
    rv = [rows[s]["creek"].get("reversal_m", 0.0) for s in ss if rows[s]["creek"]]
    r["rise"] = float(np.median(rv)) if rv else 0.0
    r["score"] = float(np.median([rows[s]["score"] for s in ss]))
    return r


def main():
    a = sys.argv[1:]
    n_pick, prefix, pick = 24, "m_", "score"
    global TITLE, BLURB
    for k in ("--n", "--title", "--blurb", "--prefix", "--pick"):
        if k in a:
            i = a.index(k); v = a[i + 1]; a = a[:i] + a[i + 2:]
            if k == "--n": n_pick = int(v)
            elif k == "--title": TITLE = v
            elif k == "--blurb": BLURB = v
            elif k == "--prefix": prefix = v
            else: pick = v
    M.PREFIX = prefix
    db, da, out_html = a[0], a[1], a[2]
    rb, ra = load_rows(db), load_rows(da)
    seeds = sorted(set(rb) & set(ra), key=int)
    key = (lambda s: abs(rb[s]["score"] - ra[s]["score"])) if pick == "score" else \
          (lambda s: abs(rb[s]["wet_pct"] - ra[s]["wet_pct"])) if pick in ("wet", "lake") else \
          (lambda s: abs(rb[s]["ha"][pick] - ra[s]["ha"][pick]))
    chosen = sorted(seeds, key=key, reverse=True)[:n_pick]
    cards, total = [], 0
    for s in chosen:
        zb, wb, c, lb = M.load(db, s, prefix); za, wa, _, la = M.load(da, s, prefix)
        mb = np.load(pathlib.Path(db) / "screen" / f"{s}.npz")["mask"]
        ma = np.load(pathlib.Path(da) / "screen" / f"{s}.npz")["mask"]
        n = zb.shape[0]
        views = [("3 km tile", (slice(0, n), slice(0, n)), TILE_PX)]
        half = int(DETAIL_M / c / 2)
        if pick == "lake":
            # details centred on the largest standing body (off the creek line) of each build
            spots = []
            for label, w, line in (("before", wb, lb), ("after", wa, la)):
                xy = largest_body_xy(w, line, c)
                if xy is None: continue
                if any(abs(xy[0] - o[0]) < DETAIL_M / 2 and abs(xy[1] - o[1]) < DETAIL_M / 2 for _, o in spots):
                    continue
                spots.append((label, xy))
            for label, wx in spots:
                y0 = int(np.clip(wx[1] / c - half, 0, n - 2 * half)); x0 = int(np.clip(wx[0] / c - half, 0, n - 2 * half))
                views.append((f"{DETAIL_M:.0f} m at the {label} build's largest lake", (slice(y0, y0 + 2 * half), slice(x0, x0 + 2 * half)), 2 * half * 2))
        else:
            wx = rb[s]["worst_xy"] or ra[s]["worst_xy"] or [n * c / 2, n * c / 2]
            y0 = int(np.clip(wx[1] / c - half, 0, n - 2 * half)); x0 = int(np.clip(wx[0] / c - half, 0, n - 2 * half))
            views.append((f"{DETAIL_M:.0f} m at the before build's worst spot", (slice(y0, y0 + 2 * half), slice(x0, x0 + 2 * half)), 2 * half * 2))
        panels = []
        for cap, sl, px in views:
            bb, wbi, _ = M.view(zb, np.isfinite(wb), c, lb, sl, px)
            ba, wai, _ = M.view(za, np.isfinite(wa), c, la, sl, px)
            fb, fa = M.flag_layer(mb, sl, px), M.flag_layer(ma, sl, px)
            us = [M.uri(bb, quality=82), M.uri(wbi, lossless=True), M.uri(fb, lossless=True),
                  M.uri(ba, quality=82), M.uri(wai, lossless=True), M.uri(fa, lossless=True)]
            total += sum(u[1] for u in us)
            panels.append(
                f'<figure class="panel" style="width:{bb.width}px"><div class="stack" style="aspect-ratio:{bb.width}/{bb.height}">'
                f'<img class="before base" src="{us[0][0]}" alt="before"><img class="before water" src="{us[1][0]}" alt="water before">'
                f'<img class="before flags" src="{us[2][0]}" alt="flags before">'
                f'<img class="after base" src="{us[3][0]}" alt="after"><img class="after water" src="{us[4][0]}" alt="water after">'
                f'<img class="after flags" src="{us[5][0]}" alt="flags after"><span class="tag"></span></div><figcaption>{cap}</figcaption></figure>')
        b, af = rb[s], ra[s]
        meta = (f'score <b>{b["score"]:.0f}</b> → <b>{af["score"]:.0f}</b> · water <b>{b["wet_pct"]:.2f}</b> → <b>{af["wet_pct"]:.2f}</b> % · '
                + " · ".join(f'<i class="k {k}">{k}</i> {b["ha"][k]:.2f} → {af["ha"][k]:.2f} ha' for k in M.CLASSES if b["ha"][k] > 0.005 or af["ha"][k] > 0.005))
        if b["creek"]:
            meta += (f' · cut max {b["creek"]["cut_max"]:.1f} → {af["creek"]["cut_max"]:.1f} m · rise {b["creek"]["reversal_m"]:.2f} → {af["creek"]["reversal_m"]:.2f} m')
        cards.append(f'<section class="card {b["mode"]}"><h3>seed {s} <span class="mode">{b["mode"]}</span><span class="meta">{meta}</span></h3>'
                     f'<div class="row">{"".join(panels)}</div></section>')
        print(f"  {s}: score {b['score']:.0f} -> {af['score']:.0f} ({total/1e6:.1f} MB)", flush=True)
    # summary
    rows_t = []
    def cell(v, fmt):
        return fmt.format(v) if not isinstance(v, str) else v
    for mode in ("all", "aeolian", "fluvial"):
        A, B = agg(rb, seeds, mode), agg(ra, seeds, mode)
        if not A: continue
        rows_t.append(f'<tr class="hd"><th colspan="3">{mode}, {A["n"]} tiles</th></tr>')
        for label, k, fmt in [("tiles flagged", "flagged", "{}")] + [(f'<i class="k {k}">{k}</i>, any / notable', k, "{}") for k in M.CLASSES] + \
                             [("water, median % of tile", "wet", "{:.2f}"), ("perched, median ha", "ha_perched", "{:.2f}"),
                              ("creek level rise toward mouth, median m", "rise", "{:.2f}"), ("creek cut max, median m", "cut_max", "{:.1f}"),
                              ("ground slope p99, median %", "slope_p99", "{:.0f}"), ("score, median", "score", "{:.0f}")]:
            rows_t.append(f"<tr><td>{label}</td><td>{cell(A[k], fmt)}</td><td>{cell(B[k], fmt)}</td></tr>")
    table = f'<table><thead><tr><th></th><th>before</th><th>after</th></tr></thead><tbody>{"".join(rows_t)}</tbody></table>'
    legend = " ".join(f'<span class="lg"><i style="background:rgb{M.COLOUR[k]}"></i>{k}</span>' for k in M.CLASSES)
    css_k = "".join(f'.k.{k}{{color:rgb{M.COLOUR[k]}}}' for k in M.CLASSES)
    html = f"""<title>{TITLE}</title>
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
table{{border-collapse:collapse;font-size:12.5px;font-variant-numeric:tabular-nums;font-family:"IBM Plex Mono",ui-monospace,monospace}}
td,th{{border-bottom:1px solid var(--rule);padding:3px 10px;text-align:right}}td:first-child,th:first-child{{text-align:left;font-family:"IBM Plex Sans",system-ui,sans-serif}}
tr.hd th{{text-align:left;color:var(--accent);padding-top:10px}}
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
.lg i{{display:inline-block;width:11px;height:11px;margin-right:4px;vertical-align:-1px}}
main{{max-width:1700px;margin:0 auto;padding:0 26px;display:flex;flex-direction:column;gap:22px}}
.card{{background:var(--panel);border:1px solid var(--rule);border-radius:3px;padding:14px 16px 12px;margin-top:22px}}
.card h3{{margin:0 0 10px;font-size:.95rem;font-family:"IBM Plex Mono",ui-monospace,monospace;color:var(--accent);display:flex;gap:14px;align-items:baseline;flex-wrap:wrap}}
.mode{{color:var(--muted);font-weight:400}}
.meta{{color:var(--muted);font-size:.78rem;font-weight:400;flex-basis:100%}}.meta b{{color:var(--ink);font-weight:600}}
{css_k}
.row{{display:flex;gap:14px;flex-wrap:wrap;align-items:flex-start;overflow-x:auto}}
.panel{{margin:0;flex:none;max-width:100%}}
.stack{{position:relative;width:100%;background:var(--chip)}}
.stack img{{position:absolute;inset:0;width:100%;height:100%;display:block}}
.stack .tag{{position:absolute;left:8px;top:8px;background:var(--tag);color:var(--tagink);font:11px "IBM Plex Mono",ui-monospace,monospace;padding:2px 7px;border-radius:2px;letter-spacing:.04em;text-transform:uppercase}}
.stack .tag::after{{content:"after"}}
body.before .stack .tag::after{{content:"before"}}
figcaption{{margin-top:5px;font-size:11.5px;color:var(--muted);font-family:"IBM Plex Mono",ui-monospace,monospace}}
body.before .after{{display:none}}
body:not(.before) .before{{display:none}}
body.no-water .water{{display:none}}
body.no-flags .flags{{display:none}}
body.mode-aeolian .card.fluvial{{display:none}}
body.mode-fluvial .card.aeolian{{display:none}}
</style>
<header>
<div><h1>{TITLE}</h1><p>{BLURB}</p><p>{len(chosen)} tiles shown, the ones whose {pick} changed most; the table covers all {len(seeds)}.</p></div>
{table}
</header>
<div class="controls">
<span class="seg" role="group" aria-label="build"><button id="b-before" aria-pressed="false">before</button><button id="b-after" aria-pressed="true">after</button></span>
<label><input type="checkbox" id="t-flags" checked> flags</label>
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
document.getElementById('t-flags').onchange = e => b.classList.toggle('no-flags', !e.target.checked);
document.getElementById('t-water').onchange = e => b.classList.toggle('no-water', !e.target.checked);
document.getElementById('s-mode').onchange = e => {{ b.classList.remove('mode-aeolian','mode-fluvial'); if (e.target.value !== 'all') b.classList.add('mode-' + e.target.value); }};
</script>"""
    open(out_html, "w").write(html)
    print(f"wrote {out_html}: {len(chosen)} tiles, {pathlib.Path(out_html).stat().st_size/1e6:.1f} MB")


if __name__ == "__main__":
    main()
