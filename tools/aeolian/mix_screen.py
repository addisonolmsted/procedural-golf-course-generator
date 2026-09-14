"""Screen a mixed run for suspicious terrain and build the gallery.

  python3 tools/aeolian/mix_screen.py <dir> <out.html> [--prefix m_] [--details 24] [--rescreen]

Reads every `<prefix><seed>.cgrid` in <dir> (mode from `index_*.txt`),
flags what a reviewer would call suspicious, and writes one page: every
tile as a thumbnail with the water and the flags as toggleable layers, a
summary table, filters by mode / flagged, sort by score, and a detail crop at
the worst spot of the most-flagged tiles. Per seed the screen is cached in
`<dir>/screen/<seed>.json` (+ the flag mask as .npz); `--rescreen` redoes it.

Flag classes (colours in the legend):
  step      ground departs from its 3x3 mean by > STEP_M -- a kink or a seam
            no natural 2 m surface has (window clipping, min-compose seams)
  steep     slope above the mode's natural limit (dune slip faces rest near
            62 %, so aeolian is flagged above 80 %, fluvial above 50 %)
  wall      slope above 150 % anywhere -- a cliff
  perched   a wet cell standing more than PERCH_M above a dry neighbour's
            ground, water that would spill
  reversal  the creek's water level rising toward the mouth by > REV_M / 10 m
  deepcut   ground 12 m off the creek line more than DEEP_M above the water
  flat      a dry region below 0.3 % slope larger than FLAT_HA hectares

None of this is a fix: the point is to see. Measurement only.
"""
import sys, io, json, base64, pathlib, glob
import numpy as np
from scipy import ndimage as ndi
HERE = pathlib.Path(__file__).resolve().parent
for p in (HERE, HERE.parent / "golf", HERE.parent / "macro_campaign"):
    sys.path.insert(0, str(p))
from macro_campaign import cgrid
from siting_sheet import hillshade_rgb
from PIL import Image, ImageDraw
import creek_cutdepth as C

STEP_M, PERCH_M, REV_M, DEEP_M, FLAT_HA = 0.8, 0.3, 0.25, 5.0, 1.0
STEEP = {"aeolian": 80.0, "fluvial": 50.0}
WALL = 150.0
THUMB_PX, DETAIL_M = 376, 500.0
CLASSES = ["step", "steep", "wall", "perched", "reversal", "deepcut", "flat"]
COLOUR = {"step": (255, 40, 40), "steep": (255, 150, 0), "wall": (170, 0, 220), "perched": (0, 220, 255),
          "reversal": (255, 0, 160), "deepcut": (255, 230, 0), "flat": (0, 200, 90)}
WEIGHT = {"step": 30.0, "steep": 5.0, "wall": 60.0, "perched": 20.0, "reversal": 40.0, "deepcut": 25.0, "flat": 3.0}
# hectares above which a class makes the tile "flagged" (steep never does on
# its own: every aeolian tile carries slip faces above 80 %)
NOTABLE = {"step": 0.5, "steep": 1e9, "wall": 2.0, "perched": 0.1, "reversal": 0.0, "deepcut": 0.0, "flat": 0.0}
WET_RGB = np.array([38, 86, 140])


def uri(im, lossless=False, quality=80):
    b = io.BytesIO()
    im.save(b, "WEBP", lossless=lossless, quality=quality, method=4)
    return "data:image/webp;base64," + base64.b64encode(b.getvalue()).decode(), b.getbuffer().nbytes


def load(d, seed, prefix):
    z, (_, _, c) = cgrid.read_f32(pathlib.Path(d) / f"{prefix}{seed}.cgrid")
    w, _ = cgrid.read_f32(pathlib.Path(d) / f"{prefix}{seed}.water.cgrid")
    lp = pathlib.Path(d) / f"{prefix}{seed}.creek.txt"
    line = np.loadtxt(lp) if lp.exists() and lp.stat().st_size else None
    return z.astype(float), w.astype(float), float(c), line


def screen(z, w, c, line, mode):
    """-> (mask uint8 [class bit], stats dict)."""
    wet = np.isfinite(w)
    n = z.shape[0]
    gy, gx = np.gradient(z, c)
    slope = np.hypot(gx, gy) * 100.0
    near_wet = ndi.binary_dilation(wet, iterations=2)
    m = {}
    # step: departure from the 3x3 mean, away from water (the crease's own edge is intended)
    box = ndi.uniform_filter(z, 3, mode="nearest")
    m["step"] = (np.abs(z - box) > STEP_M) & ~near_wet
    m["steep"] = (slope > STEEP[mode]) & ~wet
    m["wall"] = (slope > WALL) & ~wet
    # perched: wet cell above a dry neighbour's ground
    lvl = np.where(wet, w, -np.inf)
    dry_ground = np.where(wet, np.inf, z)
    lowest_dry_nb = ndi.minimum_filter(dry_ground, 3, mode="nearest")
    m["perched"] = wet & (lvl - lowest_dry_nb > PERCH_M)
    # flat: dry, below 0.3 %, connected, larger than FLAT_HA
    flat = (slope < 0.3) & ~wet
    lab, nlab = ndi.label(flat)
    if nlab:
        sizes = ndi.sum(flat, lab, index=np.arange(1, nlab + 1))
        big = np.zeros(nlab + 1, bool); big[1:] = sizes * c * c > FLAT_HA * 1e4
        m["flat"] = big[lab]
    else:
        m["flat"] = np.zeros_like(flat)
    # along the creek
    m["reversal"] = np.zeros(z.shape, bool); m["deepcut"] = np.zeros(z.shape, bool)
    creek = {}
    if line is not None and len(line) > 10:
        pr = C.profile(z, w, c, line)
        lv, xy = pr["level"], pr["xy"]
        ok = np.isfinite(lv)
        if ok.sum() > 10:
            # mouth = the lower end; walking toward the mouth the level must not rise
            head_first = np.nanmean(lv[:len(lv) // 4]) > np.nanmean(lv[-len(lv) // 4:])
            k = 5   # 10 m at 2 m nodes
            # downstream minus upstream: with the head at index 0 the downstream
            # node is the later one
            rise = (lv[k:] - lv[:-k]) if head_first else (lv[:-k] - lv[k:])
            rev = np.zeros(len(lv), bool); rev[k:] = np.nan_to_num(rise) > REV_M
            deep = np.nan_to_num(pr["cut"]) > DEEP_M
            for cls, sel in (("reversal", rev), ("deepcut", deep)):
                ix = np.clip((xy[sel, 0] / c).round().astype(int), 0, n - 1)
                iy = np.clip((xy[sel, 1] / c).round().astype(int), 0, n - 1)
                mm = np.zeros(z.shape, bool); mm[iy, ix] = True
                m[cls] = ndi.binary_dilation(mm, iterations=4)
            creek = dict(cut_max=float(np.nanmax(pr["cut"])), cut_p90=float(np.nanpercentile(pr["cut"], 90)),
                         reversal_m=float(np.nanmax(rise)) if len(rise) else 0.0,
                         grade_max=float(np.nanmax(pr["grade"])), length_m=float(pr["arc"][-1]))
    mask = np.zeros(z.shape, np.uint8)
    stats = {"cells": {}, "ha": {}}
    for b, cls in enumerate(CLASSES):
        mask |= (m[cls].astype(np.uint8) << b)
        stats["cells"][cls] = int(m[cls].sum())
        stats["ha"][cls] = float(m[cls].sum() * c * c / 1e4)
    stats["creek"] = creek
    stats["slope_p99"] = float(np.percentile(slope[~wet], 99))
    stats["slope_max"] = float(slope[~wet].max())
    stats["relief"] = float(np.percentile(z, 99) - np.percentile(z, 1))
    stats["wet_pct"] = float(100.0 * wet.mean())
    # score: hectares per class, weighted; steps / walls / reversals count hardest
    stats["score"] = float(sum(WEIGHT[k] * stats["ha"][k] for k in CLASSES))
    stats["notable"] = [k for k in CLASSES if stats["ha"][k] > NOTABLE[k]]
    # worst spot: the densest flagged neighbourhood, weighted the same way
    wsum = np.zeros(z.shape)
    for b, cls in enumerate(CLASSES):
        if stats["cells"][cls]:
            wsum += WEIGHT[cls] * m[cls]
    if wsum.any():
        dens = ndi.uniform_filter(wsum, int(DETAIL_M / c / 2), mode="constant")
        iy, ix = np.unravel_index(int(dens.argmax()), dens.shape)
        stats["worst_xy"] = [float(ix * c), float(iy * c)]
    else:
        stats["worst_xy"] = None
    return mask, stats


def flag_layer(mask, sl, out_px):
    """RGBA of the flag classes over a window, dilated so single cells survive the thumbnail."""
    mm = mask[sl]
    h, w = mm.shape
    rgba = np.zeros((h, w, 4), np.uint8)
    scale = out_px / w
    grow = max(1, int(round(1.5 / scale)))       # ~1.5 output px
    for b, cls in enumerate(CLASSES):
        sel = (mm >> b) & 1
        if not sel.any():
            continue
        sel = ndi.binary_dilation(sel.astype(bool), iterations=grow)
        rgba[sel, :3] = COLOUR[cls]; rgba[sel, 3] = 210
    im = Image.fromarray(np.flipud(rgba))
    return im.resize((out_px, int(h * scale)), Image.NEAREST)


def view(z, wet, c, line, sl, out_px, with_line=False):
    zz, ww = z[sl], wet[sl]
    h, w = zz.shape
    base = hillshade_rgb(zz, c)
    tinted = base.copy(); tinted[ww] = tinted[ww] * 0.3 + WET_RGB * 0.7
    wl = np.zeros((h, w, 4), np.uint8); wl[..., :3] = tinted; wl[..., 3] = np.where(ww, 255, 0)
    scale = out_px / w
    b_im = Image.fromarray(np.flipud(base)).resize((out_px, int(h * scale)), Image.LANCZOS)
    w_im = Image.fromarray(np.flipud(wl)).resize((out_px, int(h * scale)), Image.NEAREST)
    c_im = None
    if with_line and line is not None and len(line) > 1:
        c_im = Image.new("RGBA", w_im.size, (0, 0, 0, 0))
        x0, y0 = sl[1].start, sl[0].start
        px = [((x / c - x0) * scale, (h - 1 - (y / c - y0)) * scale) for x, y in line]
        ImageDraw.Draw(c_im).line(px, fill=(214, 48, 36, 225), width=max(2, int(scale * 1.2)), joint="curve")
    return b_im, w_im, c_im


def main():
    a = sys.argv[1:]
    prefix, n_details, rescreen = "m_", 24, False
    if "--prefix" in a: i = a.index("--prefix"); prefix = a[i + 1]; a = a[:i] + a[i + 2:]
    if "--details" in a: i = a.index("--details"); n_details = int(a[i + 1]); a = a[:i] + a[i + 2:]
    if "--rescreen" in a: rescreen = True; a.remove("--rescreen")
    d, out_html = pathlib.Path(a[0]), a[1]
    modes, waters = {}, {}
    for f in d.glob("index_*.txt"):
        for l in open(f):
            p = l.split()
            if len(p) >= 2: modes[p[0]] = p[1]
            waters[p[0]] = p[3] if len(p) >= 4 else "river"
    seeds = sorted(s for s in modes if (d / f"{prefix}{s}.cgrid").exists())
    sd = d / "screen"; sd.mkdir(exist_ok=True)
    rows = {}
    for s in seeds:
        jp, mp = sd / f"{s}.json", sd / f"{s}.npz"
        if jp.exists() and mp.exists() and not rescreen:
            rows[s] = json.load(open(jp)); continue
        try:
            z, w, c, line = load(d, s, prefix)
            mask, st = screen(z, w, c, line, modes[s])
        except Exception as e:
            print(f"  {s}: {e}"); continue
        st["mode"] = modes[s]; st["water_class"] = waters.get(s, "river")
        json.dump(st, open(jp, "w")); np.savez_compressed(mp, mask=mask)
        rows[s] = st
        print(f"  screened {s} ({modes[s]}): score {st['score']:.1f}", flush=True)
    # --- render ---
    order = sorted(rows, key=lambda s: -rows[s]["score"])
    detail_seeds = [s for s in order if rows[s]["worst_xy"]][:n_details]
    cards, total = [], 0
    for s in seeds:
        st = rows[s]
        z, w, c, line = load(d, s, prefix); wet = np.isfinite(w)
        mask = np.load(sd / f"{s}.npz")["mask"]
        n = z.shape[0]
        b_im, w_im, _ = view(z, wet, c, line, (slice(0, n), slice(0, n)), THUMB_PX)
        f_im = flag_layer(mask, (slice(0, n), slice(0, n)), THUMB_PX)
        (bu, bs), (wu, ws), (fu, fs) = uri(b_im), uri(w_im, lossless=True), uri(f_im, lossless=True)
        total += bs + ws + fs
        counts = " ".join(f'<i class="k {k}">{st["ha"][k]:.2f}</i>' for k in CLASSES if st["cells"][k])
        cr = st["creek"]
        ck = (f'cut max {cr["cut_max"]:.1f} m · rise {cr["reversal_m"]:.2f} m' if cr else "")
        flagged = "flagged" if st["notable"] else "clean"
        cards.append(
            f'<figure class="card {st["mode"]} {st.get("water_class", waters.get(s, "river"))} {flagged}" data-seed="{s}" data-score="{st["score"]:.2f}" data-mode="{st["mode"]}">'
            f'<div class="stack" style="aspect-ratio:1"><img class="base" src="{bu}" alt="terrain"><img class="water" src="{wu}" alt="water">'
            f'<img class="flags" src="{fu}" alt="flags"></div>'
            f'<figcaption><b>{s}</b> <span class="mode">{st["mode"]}</span> <span class="score">score {st["score"]:.0f}</span>'
            f'<span class="counts">{counts}</span><span class="ck">{ck}</span></figcaption></figure>')
    details = []
    for s in detail_seeds:
        st = rows[s]; z, w, c, line = load(d, s, prefix); wet = np.isfinite(w)
        mask = np.load(sd / f"{s}.npz")["mask"]; n = z.shape[0]
        cx, cy = st["worst_xy"]; half = int(DETAIL_M / c / 2)
        y0 = int(np.clip(cy / c - half, 0, n - 2 * half)); x0 = int(np.clip(cx / c - half, 0, n - 2 * half))
        sl = (slice(y0, y0 + 2 * half), slice(x0, x0 + 2 * half))
        b_im, w_im, c_im = view(z, wet, c, line, sl, 2 * half * 2, with_line=True)
        f_im = flag_layer(mask, sl, 2 * half * 2)
        (bu, bs), (wu, ws), (fu, fs) = uri(b_im, quality=84), uri(w_im, lossless=True), uri(f_im, lossless=True)
        total += bs + ws + fs
        cl = ""
        if c_im is not None:
            cu, cs = uri(c_im, lossless=True); total += cs
            cl = f'<img class="creek" src="{cu}" alt="creek line">'
        sub = np.zeros(len(CLASSES), int)
        mm = mask[sl]
        for b, k in enumerate(CLASSES): sub[b] = ((mm >> b) & 1).sum()
        here = " ".join(f'<i class="k {k}">{k} {sub[b] * c * c / 1e4:.2f} ha</i>' for b, k in enumerate(CLASSES) if sub[b])
        details.append(
            f'<figure class="detail {st["mode"]}" data-seed="{s}"><div class="stack" style="aspect-ratio:1"><img class="base" src="{bu}" alt="terrain">'
            f'<img class="water" src="{wu}" alt="water">{cl}<img class="flags" src="{fu}" alt="flags"></div>'
            f'<figcaption><b>{s}</b> <span class="mode">{st["mode"]}</span> · {DETAIL_M:.0f} m at ({cx:.0f}, {cy:.0f}) m, 1 px = 1 m'
            f'<span class="counts">{here}</span></figcaption></figure>')
    # --- summary ---
    def agg(mode):
        ss = [s for s in seeds if rows[s]["mode"] == mode] if mode != "all" else list(seeds)
        if not ss: return None
        r = {"n": len(ss), "flagged": sum(bool(rows[s]["notable"]) for s in ss)}
        for k in CLASSES:
            r[k] = f'{sum(rows[s]["cells"][k] > 0 for s in ss)} / {sum(k in rows[s]["notable"] for s in ss)}'
        r["slope_p99"] = float(np.median([rows[s]["slope_p99"] for s in ss]))
        r["cut_max"] = float(np.median([rows[s]["creek"].get("cut_max", np.nan) for s in ss if rows[s]["creek"]])) if any(rows[s]["creek"] for s in ss) else float("nan")
        return r
    A = {m: agg(m) for m in ("all", "aeolian", "fluvial")}
    head = "".join(f"<th>{m}</th>" for m in A if A[m])
    def tr(label, key, fmt="{}"):
        return f"<tr><td>{label}</td>" + "".join(f"<td>{fmt.format(A[m][key])}</td>" for m in A if A[m]) + "</tr>"
    table = (f'<table><thead><tr><th></th>{head}</tr></thead><tbody>' + tr("tiles", "n") + tr("tiles flagged (a class above its notable size)", "flagged")
             + "".join(tr(f'tiles with <i class="k {k}">{k}</i>, any / notable', k) for k in CLASSES)
             + tr("ground slope p99, median %", "slope_p99", "{:.0f}") + tr("creek cut depth max, median m", "cut_max", "{:.1f}")
             + "</tbody></table>")
    legend = " ".join(f'<span class="lg"><i style="background:rgb{COLOUR[k]}"></i>{k}</span>' for k in CLASSES)
    n_a = A["aeolian"]["n"] if A["aeolian"] else 0; n_f = A["fluvial"]["n"] if A["fluvial"] else 0
    css_k = "".join(f'.k.{k}{{color:rgb{COLOUR[k]}}}' for k in CLASSES)
    html = f"""<title>Mixed Run Screen</title>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=IBM+Plex+Sans:wght@400;600&family=IBM+Plex+Mono&display=swap">
<style>
:root{{--bg:#f6f4ee;--panel:#fffdf8;--ink:#22231f;--muted:#5f5e57;--rule:#ddd8c9;--accent:#8a3b2a;--chip:#efeadc}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28}}}}
:root[data-theme="dark"]{{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72;--chip:#2c2d28}}
*{{box-sizing:border-box}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:0 0 48px;line-height:1.45}}
header{{padding:26px 26px 14px;max-width:1700px;margin:0 auto;display:flex;gap:32px;flex-wrap:wrap;align-items:flex-start}}
header>div{{flex:1 1 40ch}}
h1{{font-size:1.5rem;margin:0 0 6px;letter-spacing:-.01em;text-wrap:balance}}
h2{{font-size:1.1rem;margin:34px 0 10px;color:var(--accent)}}
header p{{max-width:70ch;color:var(--muted);margin:0 0 8px}}
table{{border-collapse:collapse;font-size:12.5px;font-variant-numeric:tabular-nums;font-family:"IBM Plex Mono",ui-monospace,monospace}}
td,th{{border-bottom:1px solid var(--rule);padding:3px 10px;text-align:right}}td:first-child,th:first-child{{text-align:left;font-family:"IBM Plex Sans",system-ui,sans-serif}}
.controls{{position:sticky;top:0;z-index:10;background:color-mix(in srgb,var(--bg) 92%,transparent);backdrop-filter:blur(6px);
  border-block:1px solid var(--rule);padding:10px 26px;display:flex;gap:20px;flex-wrap:wrap;align-items:center;
  font-family:"IBM Plex Mono",ui-monospace,monospace;font-size:13px}}
.controls label{{display:flex;align-items:center;gap:6px;cursor:pointer;user-select:none}}
.controls input{{accent-color:var(--accent);width:15px;height:15px}}
.controls select{{font:inherit;background:var(--panel);color:var(--ink);border:1px solid var(--rule);padding:2px 6px}}
.controls input:focus-visible,.controls select:focus-visible{{outline:2px solid var(--accent);outline-offset:2px}}
.legend{{display:flex;gap:12px;flex-wrap:wrap;margin-left:auto}}
.lg i{{display:inline-block;width:11px;height:11px;margin-right:4px;vertical-align:-1px}}
main{{max-width:1700px;margin:0 auto;padding:0 26px}}
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(376px,1fr));gap:16px}}
.card,.detail{{margin:0;background:var(--panel);border:1px solid var(--rule);border-radius:3px;padding:8px}}
.detail{{width:518px;max-width:100%}}
.stack{{position:relative;width:100%;background:var(--chip)}}
.stack img{{position:absolute;inset:0;width:100%;height:100%;display:block;image-rendering:auto}}
.detail .stack img{{image-rendering:auto}}
figcaption{{margin-top:6px;font-size:11.5px;color:var(--muted);font-family:"IBM Plex Mono",ui-monospace,monospace;display:flex;gap:8px;flex-wrap:wrap;align-items:baseline}}
figcaption b{{color:var(--ink)}}.mode{{color:var(--accent)}}.counts{{display:flex;gap:6px;flex-wrap:wrap}}.counts .k::after{{content:" ha"}}
.ck{{flex-basis:100%}}
{css_k}
body.no-water .water{{display:none}}
body.no-flags .flags{{display:none}}
body.no-creek .creek{{display:none}}
body.only-flagged .card.clean{{display:none}}
body.mode-aeolian .card.fluvial,body.mode-aeolian .detail.fluvial{{display:none}}
body.mode-fluvial .card.aeolian,body.mode-fluvial .detail.aeolian{{display:none}}
body.water-river .card.dry{{display:none}}
body.water-dry .card.river{{display:none}}
</style>
<header>
<div><h1>Mixed run, {len(seeds)} tiles with running water</h1>
<p>{n_a} aeolian and {n_f} fluvial tiles, each seed's mode from its own coin{", " + str(sum(1 for s in seeds if rows[s].get("water_class") == "river")) + " with a creek or river and " + str(sum(1 for s in seeds if rows[s].get("water_class") == "dry")) + " without" if any(rows[s].get("water_class") == "dry" for s in seeds) else ", kept only when it carries a creek or river"}. Every tile is screened for what a reviewer would call suspicious and the finds are drawn as a layer you can switch off. Nothing here is a fix.</p>
<p>Scores weight flagged hectares by class: steps, walls and level reversals count hardest. Thumbnails are 1 px = 8 m; the flag layer is grown so single cells stay visible, so read the hectares, not the paint.</p></div>
{table}
</header>
<div class="controls">
<label><input type="checkbox" id="t-flags" checked> flags</label>
<label><input type="checkbox" id="t-water" checked> water</label>
<label><input type="checkbox" id="t-creek" checked> creek line (details)</label>
<label><input type="checkbox" id="t-only"> flagged only</label>
<span class="hint" title="hectares: step 0.5, wall 2, perched 0.1, reversal / deepcut / flat any; steep never on its own">notable = above a per-class size</span>
<label>mode <select id="s-mode"><option value="all">all</option><option value="aeolian">aeolian</option><option value="fluvial">fluvial</option></select></label>
<label>water <select id="s-water"><option value="all">all</option><option value="river">river / creek</option><option value="dry">none</option></select></label>
<label>sort <select id="s-sort"><option value="score">score</option><option value="seed">seed</option></select></label>
<span class="legend">{legend}</span>
</div>
<main>
<h2>Worst spots, {len(details)} tiles by score</h2>
<div class="grid" id="details">{"".join(details)}</div>
<h2>All tiles</h2>
<div class="grid" id="tiles">{"".join(cards)}</div>
</main>
<script>
const b = document.body;
document.getElementById('t-flags').onchange = e => b.classList.toggle('no-flags', !e.target.checked);
document.getElementById('t-water').onchange = e => b.classList.toggle('no-water', !e.target.checked);
document.getElementById('t-creek').onchange = e => b.classList.toggle('no-creek', !e.target.checked);
document.getElementById('t-only').onchange = e => b.classList.toggle('only-flagged', e.target.checked);
document.getElementById('s-mode').onchange = e => {{ b.classList.remove('mode-aeolian','mode-fluvial'); if (e.target.value !== 'all') b.classList.add('mode-' + e.target.value); }};
const grid = document.getElementById('tiles');
function sortBy(k) {{
  const cards = [...grid.children];
  cards.sort((x, y) => k === 'seed' ? (+x.dataset.seed - +y.dataset.seed) : (+y.dataset.score - +x.dataset.score));
  cards.forEach(c => grid.appendChild(c));
}}
document.getElementById('s-sort').onchange = e => sortBy(e.target.value);
document.getElementById('s-water').onchange = e => {{ b.classList.remove('water-river','water-dry'); if (e.target.value !== 'all') b.classList.add('water-' + e.target.value); }};
sortBy('score');
</script>"""
    open(out_html, "w").write(html)
    json.dump({s: rows[s] for s in seeds}, open(d / "screen_summary.json", "w"), indent=1)
    print(f"wrote {out_html}: {len(seeds)} tiles, {len(details)} details, {pathlib.Path(out_html).stat().st_size/1e6:.1f} MB "
          f"(images {total/1e6:.1f} MB)")
    for m in A:
        if A[m]: print(m, A[m])


if __name__ == "__main__":
    main()
