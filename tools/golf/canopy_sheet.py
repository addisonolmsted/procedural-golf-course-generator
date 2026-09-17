"""Our generated canopy beside real land, by eye and by number.

  python3 tools/golf/canopy_sheet.py out/final250_v2 out/canopy250 <out.html>

The branch rule is render before trusting metrics. A coverage number and a
patch count cannot tell you whether a tile reads as pine savanna or as static,
so every panel here puts one of our tiles next to a REAL tile of the same
archetype at the SAME coverage, through the same render function, and lets the
eye do what the statistics cannot.

Matching on coverage is the point. Pairing our 76 percent tile with a real 76
percent tile asks the only question left after `coverage_for` has already made
the amount right by construction: does that much cover come as the right SHAPE?

The routed holes are drawn on top with the corridor round 1 measured on real
courses. They are not cleared here -- clearing is the next round -- so the
corridor outline is a preview of the work, and the treed fraction inside it is
that work's size.
"""
from __future__ import annotations

import argparse
import html
import json
import pathlib
import sys

import numpy as np
import scipy.ndimage as ndi

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                                   # noqa: E402
import canopy_metrics as cm                                        # noqa: E402
import canopy_model as model                                       # noqa: E402
import canopy_gen as g                                             # noqa: E402
from siting_sheet import hillshade_rgb, to_uri                     # noqa: E402
from routing_sheet import draw_line                                # noqa: E402
from hole_metrics import arc_cum, at_arc                           # noqa: E402

TILES = HERE.parents[0] / "macro_campaign" / "out" / "tiles"
CELL = model.CELL
# The terrain ramp's low end is already a mid green, so the paint has to be
# both darker and cooler than it or canopy and low ground read as one thing.
CANOPY_RGB = np.array([22, 58, 48], float)
# round 1, 4813 holes on 651 real course tiles: the cleared corridor is tapered,
# and treating it as a constant-width rectangle would be wrong by nearly a
# factor of two end to end
CORRIDOR = (("tee", 0.0, 0.25, 18.0), ("landing", 0.25, 0.7, 30.0),
            ("green", 0.7, 1.0, 35.0))


def block_mean(a, k):
    n = a.shape[0] // k
    return a[:n * k, :n * k].reshape(n, k, n, k).mean(axis=(1, 3))


def render(z10, tree, tcc10, wet10, upscale=2):
    """One function for both sides, so the comparison cannot cheat.

    The density that sets the paint's opacity is read at 30 m on BOTH sides.
    Ours is native 10 m and the real corpus's is native 30 m, so showing ours at
    its own resolution would give our tiles a finer-grained canopy purely as an
    artefact of which raster each side came from.
    """
    img = hillshade_rgb(z10, CELL, wet=wet10).astype(float)
    d = np.repeat(np.repeat(block_mean(tcc10, 3), 3, 0), 3, 1)
    d = np.pad(d, ((0, tcc10.shape[0] - d.shape[0]),
                   (0, tcc10.shape[1] - d.shape[1])), mode="edge")
    a = np.where(tree, 0.45 + 0.50 * np.clip(d / 100.0, 0, 1), 0.0)[..., None]
    return np.clip(img * (1 - a) + CANOPY_RGB * a, 0, 255).astype(np.uint8)


def our_tile(seed, dump, cdir):
    z2, c2, cls, wet10, _ = g.tile_inputs(seed, dump)
    grid, _ = cgrid.read_u8(cdir / f"m_{seed}.canopy.cgrid")
    tcc, _ = cgrid.read_u8(cdir / f"m_{seed}.tcc.cgrid")
    n = grid.shape[0]
    return (block_mean(z2, int(round(CELL / c2)))[:n, :n], grid == g.CLASS_TREE,
            tcc.astype(float), wet10[:n, :n])


def real_tile(arch, tile):
    base = TILES / arch / tile
    cls, _ = cgrid.read_u8(base.with_suffix(".canopy.cgrid"))
    z2, (_, _, c2) = cgrid.read_f32(base.with_suffix(".cgrid"))
    z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(float)
    n = min(z2.shape[0] // int(round(CELL / float(c2))), cls.shape[0])
    z10 = block_mean(z2, int(round(CELL / float(c2))))[:n, :n]
    tp = base.with_suffix(".tcc.cgrid")
    if tp.exists():
        t, _ = cgrid.read_u8(tp)
        k = max(1, int(round(n / t.shape[0])))
        tcc = np.repeat(np.repeat(t.astype(float), k, 0), k, 1)[:n, :n]
        if tcc.shape[0] < n:
            tcc = np.pad(tcc, ((0, n - tcc.shape[0]), (0, n - tcc.shape[1])), mode="edge")
    else:
        tcc = np.full((n, n), 50.0)
    return z10, cls[:n, :n] == 10, tcc, cls[:n, :n] == 80


def corridor_mask(spine, shape, cell=CELL):
    """The tapered cleared corridor round 1 measured, as a mask on the raster."""
    p = np.asarray(spine, float)
    if len(p) < 2:
        return np.zeros(shape, bool)
    cum = arc_cum(p)
    L = float(cum[-1])
    if L < 50.0:
        return np.zeros(shape, bool)
    m = np.zeros(shape, bool)
    ss = np.arange(0.0, L + 1e-9, cell / 2.0)
    q = at_arc(p, cum, ss)
    frac = ss / max(L, 1.0)
    hw = np.zeros_like(ss)
    for _name, a, b, w in CORRIDOR:
        sel = (frac >= a) & (frac <= b)
        hw[sel] = w
    offs = np.arange(-35.0, 35.0 + 1e-9, cell / 2.0)
    fwd = at_arc(p, cum, np.clip(ss + 10, 0, L)) - at_arc(p, cum, np.clip(ss - 10, 0, L))
    nrm = np.stack([-fwd[:, 1], fwd[:, 0]], -1)
    nrm /= np.maximum(np.hypot(nrm[:, 0], nrm[:, 1]), 1e-9)[:, None]
    for o in offs:
        sel = np.abs(o) <= hw
        if not sel.any():
            continue
        ys = (q[sel, 0] + nrm[sel, 0] * o) / cell
        xs = (q[sel, 1] + nrm[sel, 1] * o) / cell
        yi = np.clip(ys.astype(int), 0, shape[0] - 1)
        xi = np.clip(xs.astype(int), 0, shape[1] - 1)
        m[yi, xi] = True
    return m


def overlay_route(img, rec, pars=True):
    from routing_sheet import PAR_COLORS
    for h in rec.get("holes", []):
        sp = np.asarray(h["spine"], float) / CELL
        col = PAR_COLORS.get(h["par"], (255, 255, 255)) if pars else (255, 255, 255)
        for i in range(len(sp) - 1):
            draw_line(img, (sp[i][0], sp[i][1]), (sp[i + 1][0], sp[i + 1][1]), col, thick=1)
    return img


def shaped(rows, arch_of):
    """Our rows in the exact shape canopy_metrics.summarize reads.

    Pushing our tiles back through the SAME summariser that produced the real
    table is the strongest check available that the two columns mean the same
    thing.
    """
    return [dict(archetype=arch_of(r), tile=str(r["seed"]), n=r["n"],
                 tree_frac=r["tree_frac"], tcc_mean=r["tcc_mean"],
                 patch={k: r[k] for k in model.PATCH_KEYS}, curves={})
            for r in rows]


# ---------------------------------------------------------------- acceptance

ACCEPT = [
    ("treed land %", "treed_land_pct", "abs", 0.3, "%.2f"),
    ("canopy %", "canopy_pct", "abs", 0.5, "%.2f"),
    ("density", "density", "abs", 0.02, "%.3f"),
    ("patches / km2", "patches_per_km2", "ratio", 1.5, "%.2f"),
    ("p50 patch cells", "p50_patch_cells", "abs", 3.0, "%.1f"),
    ("lone / km2", "lone_per_km2", "ratio", 2.0, "%.2f"),
    ("largest patch %", "largest_patch_pct", "abs", None, "%.1f"),
    ("treed p10", "treed_p10", "abs", 0.3, "%.2f"),
    ("treed p50", "treed_p50", "abs", 0.3, "%.2f"),
    ("treed p90", "treed_p90", "abs", 0.3, "%.2f"),
]


def verdict(kind, tol, ours, real):
    if tol is None or not np.isfinite(ours) or not np.isfinite(real):
        return "n/a", True
    if kind == "abs":
        ok = abs(ours - real) <= tol
        return f"±{tol:g}", ok
    r = (ours + 1e-9) / (real + 1e-9)
    return f"×÷{tol:g}", (1.0 / tol) <= r <= tol


def accept_rows(arch, ours_sum):
    out = []
    real = model.SUMMARY[arch]
    for label, key, kind, tol, fmt in ACCEPT:
        if key == "largest_patch_pct":
            tol = 15.0 if arch != "sandhills_nc" else 3.0
        o, r = float(ours_sum.get(key, float("nan"))), float(real[key])
        t, ok = verdict(kind, tol, o, r)
        out.append((label, fmt % o, fmt % r, t, ok))
    return out


# ---------------------------------------------------------------------- SVG

def svg_cdf(series, logx=False, w=520, h=190):
    """Coverage distribution, ours against real, as a cumulative curve."""
    pad = dict(l=44, r=12, t=10, b=28)
    iw, ih = w - pad["l"] - pad["r"], h - pad["t"] - pad["b"]
    allv = np.concatenate([np.asarray(v, float) for _n, v, _c in series])
    lo = max(allv[allv > 0].min() * 0.7, 1e-3) if logx and (allv > 0).any() else 0.0
    hi = max(allv.max() * 1.05, lo * 2 if logx else 1.0)

    def X(v):
        if logx:
            v = max(v, lo)
            return pad["l"] + iw * (np.log10(v / lo) / max(np.log10(hi / lo), 1e-9))
        return pad["l"] + iw * (v - lo) / max(hi - lo, 1e-9)

    p = [f'<svg viewBox="0 0 {w} {h}" role="img" aria-label="coverage distribution">']
    ticks = ([0.01, 0.1, 1, 10, 100] if logx
             else np.linspace(lo, hi, 5))
    for t in ticks:
        if not (lo <= t <= hi):
            continue
        p.append(f'<line x1="{X(t):.1f}" y1="{pad["t"]}" x2="{X(t):.1f}" '
                 f'y2="{pad["t"] + ih}" class="grid"/>')
        p.append(f'<text x="{X(t):.1f}" y="{h - 9}" class="tick" '
                 f'text-anchor="middle">{t:g}</text>')
    for f in (0.0, 0.5, 1.0):
        y = pad["t"] + ih * (1 - f)
        p.append(f'<text x="{pad["l"] - 8}" y="{y + 4:.1f}" class="tick" '
                 f'text-anchor="end">{int(100 * f)}%</text>')
    for name, vals, col in series:
        v = np.sort(np.asarray(vals, float))
        pts = []
        for i, x in enumerate(v):
            y = pad["t"] + ih * (1 - (i + 1) / len(v))
            pts.append(f"{X(x):.1f},{y:.1f}")
        p.append(f'<polyline points="{" ".join(pts)}" fill="none" stroke="{col}" '
                 f'stroke-width="2.2" stroke-linejoin="round"/>')
    p.append("</svg>")
    return "".join(p)


def svg_profile(offsets, curves, w=520, h=200):
    """Tree rate against distance from the hole centre line."""
    pad = dict(l=44, r=12, t=10, b=30)
    iw, ih = w - pad["l"] - pad["r"], h - pad["t"] - pad["b"]
    hi = max(0.05, max(float(np.max(c)) for _n, c, _col in curves) * 1.15)
    X = lambda o: pad["l"] + iw * (o + 160.0) / 320.0          # noqa: E731
    Y = lambda v: pad["t"] + ih * (1 - v / hi)                 # noqa: E731
    p = [f'<svg viewBox="0 0 {w} {h}" role="img" aria-label="corridor profile">']
    for o in (-160, -80, 0, 80, 160):
        p.append(f'<line x1="{X(o):.1f}" y1="{pad["t"]}" x2="{X(o):.1f}" '
                 f'y2="{pad["t"] + ih}" class="grid"/>')
        p.append(f'<text x="{X(o):.1f}" y="{h - 10}" class="tick" '
                 f'text-anchor="middle">{o:+d}</text>')
    for f in (0, 0.5, 1.0):
        p.append(f'<text x="{pad["l"] - 8}" y="{Y(f * hi) + 4:.1f}" class="tick" '
                 f'text-anchor="end">{100 * f * hi:.0f}%</text>')
    p.append(f'<rect x="{X(-30):.1f}" y="{pad["t"]}" width="{X(30) - X(-30):.1f}" '
             f'height="{ih}" class="band"/>')
    for name, c, col in curves:
        pts = " ".join(f"{X(o):.1f},{Y(v):.1f}" for o, v in zip(offsets, c))
        p.append(f'<polyline points="{pts}" fill="none" stroke="{col}" stroke-width="2.2"/>')
    p.append(f'<text x="{X(0):.1f}" y="{pad["t"] + 12}" class="tick" '
             f'text-anchor="middle">corridor</text></svg>')
    return "".join(p)


# --------------------------------------------------------------------- page

CSS = """
:root{
  --ground:#F4F6F3; --surface:#FFFFFF; --ink:#16211A; --muted:#5E6D62;
  --line:#D9E0D8; --pine:#2F4F34; --sand:#A8834A; --ok:#2F6B3C; --warn:#9A5B22;
  --ours:#2F4F34; --real:#A8834A;
}
@media (prefers-color-scheme: dark){
  :root:not([data-theme="light"]){
    --ground:#0F1411; --surface:#161E18; --ink:#E2E8E1; --muted:#96A499;
    --line:#28332A; --pine:#8FBF92; --sand:#D3A868; --ok:#7FBE8B; --warn:#D9A063;
    --ours:#8FBF92; --real:#D3A868;
  }
}
:root[data-theme="dark"]{
  --ground:#0F1411; --surface:#161E18; --ink:#E2E8E1; --muted:#96A499;
  --line:#28332A; --pine:#8FBF92; --sand:#D3A868; --ok:#7FBE8B; --warn:#D9A063;
  --ours:#8FBF92; --real:#D3A868;
}
*{box-sizing:border-box}
body{background:var(--ground);color:var(--ink);
  font:16px/1.62 "Public Sans",-apple-system,BlinkMacSystemFont,"Segoe UI",sans-serif;
  margin:0;padding:0 20px 90px}
.wrap{max-width:1080px;margin:0 auto}
h1,h2,h3{font-family:"Zilla Slab",Georgia,serif;font-weight:600;text-wrap:balance;margin:0}
h1{font-size:clamp(34px,5.2vw,52px);letter-spacing:-.015em;line-height:1.06}
h2{font-size:26px;margin:54px 0 6px;padding-top:22px;border-top:2px solid var(--line)}
h3{font-size:17px;margin:26px 0 8px}
header{padding:54px 0 8px}
.eyebrow{font:600 11px/1 "IBM Plex Mono",monospace;letter-spacing:.17em;
  text-transform:uppercase;color:var(--sand);margin-bottom:16px}
.lede{font-size:19px;color:var(--muted);max-width:64ch;margin:16px 0 0}
p{max-width:68ch}
.sub{color:var(--muted);font-size:14.5px;max-width:70ch;margin:4px 0 16px}
table{border-collapse:collapse;width:100%;font-size:14px;margin:10px 0 4px;
  font-variant-numeric:tabular-nums}
th,td{text-align:right;padding:7px 10px;border-bottom:1px solid var(--line)}
th:first-child,td:first-child{text-align:left}
th{font:600 11px/1.3 "IBM Plex Mono",monospace;letter-spacing:.08em;
  text-transform:uppercase;color:var(--muted);white-space:nowrap}
td.n{font-family:"IBM Plex Mono",monospace}
.v{font:600 11px/1 "IBM Plex Mono",monospace;letter-spacing:.06em}
.pass{color:var(--ok)} .fail{color:var(--warn)}
.scroll{overflow-x:auto}
.card{background:var(--surface);border:1px solid var(--line);border-radius:5px;
  padding:16px 18px;margin:16px 0}
.pair{display:grid;grid-template-columns:1fr 1fr;gap:14px;margin:12px 0 26px}
.pair figure{margin:0}
.pair img{width:100%;display:block;border-radius:3px;border:1px solid var(--line)}
figcaption{font:12.5px/1.5 "IBM Plex Mono",monospace;color:var(--muted);
  margin-top:7px;white-space:pre-line}
.tag{font:600 11px/1 "IBM Plex Mono",monospace;letter-spacing:.09em;
  text-transform:uppercase;padding:3px 7px;border-radius:3px;margin-right:7px}
.tag.o{background:var(--ours);color:var(--ground)}
.tag.r{background:var(--real);color:var(--ground)}
.charts{display:grid;grid-template-columns:repeat(auto-fit,minmax(330px,1fr));gap:20px}
svg{width:100%;height:auto;display:block}
.grid{stroke:var(--line);stroke-width:1}
.tick{fill:var(--muted);font:11px "IBM Plex Mono",monospace}
.band{fill:var(--pine);opacity:.09}
.key{display:flex;gap:16px;flex-wrap:wrap;font:12px "IBM Plex Mono",monospace;
  color:var(--muted);margin-top:8px}
.key i{display:inline-block;width:15px;height:3px;vertical-align:middle;margin-right:6px}
@media (max-width:720px){ .pair{grid-template-columns:1fr} }
"""

HEAD = """<title>Our Own Canopy</title>
<link rel="preconnect" href="https://fonts.googleapis.com">
<link rel="preconnect" href="https://fonts.gstatic.com" crossorigin>
<link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Zilla+Slab:wght@500;600&family=Public+Sans:wght@400;600&family=IBM+Plex+Mono:wght@400;600&display=swap">
<style>%s</style>"""


def table(head, body):
    h = "".join(f"<th>{html.escape(c)}</th>" for c in head)
    r = []
    for row in body:
        cells = []
        for i, c in enumerate(row):
            if isinstance(c, tuple):
                cls = "pass" if c[1] else "fail"
                cells.append(f'<td class="v {cls}">{html.escape(str(c[0]))}</td>')
            else:
                cells.append(f'<td class="{"" if i == 0 else "n"}">{html.escape(str(c))}</td>')
        r.append("<tr>" + "".join(cells) + "</tr>")
    return f'<div class="scroll"><table><thead><tr>{h}</tr></thead><tbody>' \
           + "".join(r) + "</tbody></table></div>"


def build(dump, cdir, out_html, route_jsonl="out/route_rs/rs.jsonl"):
    rows = json.loads((cdir / "index_canopy.json").read_text())["rows"]
    by = {r["seed"]: r for r in rows}
    recs = {}
    rp = pathlib.Path(route_jsonl)
    if rp.exists():
        for ln in rp.open():
            r = json.loads(ln)
            recs[r["seed"]] = r
    prev = {}
    pp = cdir / "corridor_preview.json"
    if pp.exists():
        prev = json.loads(pp.read_text())

    import io as _io
    import contextlib as _ct
    ours = {}
    for key in model.ARCHETYPES:
        rs = [r for r in rows if r["cover"] == key]
        if not rs:
            continue
        with _ct.redirect_stdout(_io.StringIO()):
            ours[key] = cm.summarize(shaped(rs, lambda _r, k=key: k))[key]

    P = [f"<!-- generated by tools/golf/canopy_sheet.py -->", HEAD % CSS, '<div class="wrap">']
    P.append(f"""<header>
<div class="eyebrow">Canopy round 2 &middot; 250 frozen seeds</div>
<h1>Our Own Canopy</h1>
<p class="lede">Round one measured where trees grow on land no golf has touched.
This grows them on ours, and puts every tile next to real ground of the same
country at the same tree cover, so the only question left is whether that much
cover arrives in the right shape.</p>
</header>""")

    P.append("<h2>What had to match</h2>")
    P.append("""<p class="sub">Coverage is right by construction: each seed is
handed the tree cover of one actual tile from that archetype's corpus. What the
fitted parameters had to buy is structure &mdash; how that cover breaks into
stands, how many trees stand alone, how much of the treed area sits in the
single largest wood. Every figure in both columns is produced by the same
function, <span class="v">canopy_metrics.summarize</span>, that produced the
published numbers for real land.</p>""")
    NAMES = {"sandhills": "Nebraska dune country, dry",
             "sandhills_nc": "Carolina longleaf savanna",
             "sandhills_river": "Nebraska dune country, with a river"}
    npass = ntot = 0
    for key in model.ARCHETYPES:
        if key not in ours:
            continue
        rs = [r for r in rows if r["cover"] == key]
        P.append(f"<h3>{html.escape(NAMES[key])} &mdash; {len(rs)} of our seeds "
                 f"against {model.SUMMARY[key]['tiles']} real tiles</h3>")
        body = []
        for label, val, real, tol, ok in accept_rows(key, ours[key]):
            body.append([label, val, real, tol, ("pass" if ok else "off", ok)])
            npass += ok
            ntot += 1
        # edge density is not in summarize's row, so it is measured here
        eo = float(np.median([r["edge_density"] for r in rs]))
        er = float(np.median([t["edge_density"] for t in model.TILES[key]]))
        ok = (1 / 1.5) <= ((eo + 1e-9) / (er + 1e-9)) <= 1.5
        body.append(["edge density", f"{eo:.3f}", f"{er:.3f}", "×÷1.5",
                     ("pass" if ok else "off", ok)])
        npass += ok
        ntot += 1
        P.append(table(["statistic", "ours", "real land", "tolerance", ""], body))
    P.append(f'<p class="sub"><strong>{npass} of {ntot} rows inside tolerance.</strong></p>')

    P.append("<h2>How much cover each seed gets</h2>")
    P.append("""<p class="sub">The median dune-country tile is bare. Sixty-one
percent of real sandhills tiles carry under one percent tree cover, and drawing
each seed from that distribution rather than from its average is what keeps
trees off most seeds and gives the occasional one a real grove. The curve is
cumulative: the share of tiles at or below each cover.</p>""")
    P.append('<div class="charts">')
    for key, logx in (("sandhills", True), ("sandhills_nc", False),
                      ("sandhills_river", True)):
        rs = [r for r in rows if r["cover"] == key]
        if not rs:
            continue
        P.append(f'<div class="card"><h3>{html.escape(NAMES[key])}</h3>'
                 + svg_cdf([("ours", [100 * r["tree_frac"] for r in rs], "var(--ours)"),
                            ("real", [100 * v for v in model.COVERAGE[key]], "var(--real)")],
                           logx=logx)
                 + '<div class="key"><span><i style="background:var(--ours)"></i>our seeds</span>'
                   '<span><i style="background:var(--real)"></i>real tiles</span></div>'
                   '<div class="key">tree cover, percent of tile</div></div>')
    P.append("</div>")
    return P, rows, by, recs, prev, ours


def pick_pairs(rows, key, quants):
    """Our seeds at chosen coverage quantiles, each matched to the nearest real tile."""
    rs = sorted([r for r in rows if r["cover"] == key], key=lambda r: r["tree_frac"])
    real = model.TILES[key]
    out = []
    for q, label in quants:
        i = int(np.clip(round(q * (len(rs) - 1)), 0, len(rs) - 1))
        r = rs[i]
        t = min(real, key=lambda t: abs(t["tree_frac"] - r["tree_frac"]))
        out.append((r, t, label))
    return out


def panel_pairs(P, rows, key, quants, dump, cdir, recs, names):
    for r, t, label in pick_pairs(rows, key, quants):
        seed = r["seed"]
        z10, tree, tcc, wet = our_tile(seed, dump, cdir)
        img = render(z10, tree, tcc, wet)
        rec = recs.get(seed)
        corr = None
        if rec:
            tf = []
            for h in rec.get("holes", []):
                m = corridor_mask(h["spine"], tree.shape)
                if m.any():
                    tf.append(float(tree[m].mean()))
            corr = float(np.mean(tf)) if tf else None
            img = overlay_route(img, rec)
        rz, rtree, rtcc, rwet = real_tile(key, t["tile"])
        rimg = render(rz, rtree, rtcc, rwet)
        km2r = (t["n"] * CELL / 1000.0) ** 2
        cap_o = (f"seed {seed} · {names[key]}\n"
                 f"{100 * r['tree_frac']:.2f} % treed · {r['n_patches'] / 9.0:.1f} patches/km² · "
                 f"{r['isolated_per_km2']:.2f} lone/km² · largest {100 * r['largest_frac']:.0f} %")
        if corr is not None:
            cap_o += f"\ncorridor {100 * corr:.0f} % treed — the next round's work"
        cap_r = (f"{t['tile']} · real {key}\n"
                 f"{100 * t['tree_frac']:.2f} % treed · {t['n_patches'] / km2r:.1f} patches/km² · "
                 f"{t['isolated_per_km2']:.2f} lone/km² · largest {100 * t['largest_frac']:.0f} %")
        P.append(f'<h3>{html.escape(label)}</h3><div class="pair">'
                 f'<figure><img src="{render_uri(img)}" alt="our generated tile {seed}">'
                 f'<figcaption><span class="tag o">ours</span>{html.escape(cap_o)}</figcaption></figure>'
                 f'<figure><img src="{render_uri(rimg)}" alt="real tile {t["tile"]}">'
                 f'<figcaption><span class="tag r">real</span>{html.escape(cap_r)}</figcaption></figure>'
                 f"</div>")


def render_uri(img):
    return to_uri(img, upscale=2)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dump")
    ap.add_argument("canopy")
    ap.add_argument("out")
    ap.add_argument("--routes", default="out/route_rs/rs.jsonl")
    a = ap.parse_args()
    dump, cdir = pathlib.Path(a.dump), pathlib.Path(a.canopy)
    P, rows, by, recs, prev, ours = build(dump, cdir, a.out, a.routes)
    names = {"sandhills": "dune country", "sandhills_nc": "Carolina savanna",
             "sandhills_river": "dune country, river"}

    P.append("<h2>Side by side, at the same cover</h2>")
    P.append("""<p class="sub">Both panels go through one render function. The
hillshade is the branch's standard tint, trees are painted dark green, and the
paint's opacity is canopy density read at thirty metres on <em>both</em> sides
&mdash; ours is natively finer, and showing it at its own resolution would give
our tiles a subtler canopy purely because of which raster each side came from.
White and coloured lines are the routed holes, drawn on ours only. They are not
cleared: that is the next round.</p>""")
    if TILES.exists():
        P.append("<h3 style='border-top:1px solid var(--line);padding-top:18px'>Nebraska dune country</h3>")
        panel_pairs(P, rows, "sandhills",
                    [(0.02, "A bare seed — the dune-country median is 0.5 % cover"),
                     (0.55, "One percent: scattered specimens, the Old Macdonald case"),
                     (0.88, "A seed that drew a real grove"),
                     (0.99, "The heavy tail: dune country can be woodland")],
                    dump, cdir, recs, names)
        panel_pairs(P, rows, "sandhills_river",
                    [(0.75, "Dune country with a creek through it")],
                    dump, cdir, recs, names)
        P.append("<h3 style='border-top:1px solid var(--line);padding-top:18px'>Carolina longleaf savanna</h3>")
        panel_pairs(P, rows, "sandhills_nc",
                    [(0.10, "The open end of the savanna"),
                     (0.50, "The median Carolina tile, 76 % treed at 50 % canopy"),
                     (0.92, "Closed pine wood")],
                    dump, cdir, recs, names)
    else:
        P.append('<p class="sub">The natural tile corpus is not on this machine, '
                 'so the paired panels are omitted and only the numbers stand.</p>')

    if prev:
        P.append("<h2>The corridor that is not cut yet</h2>")
        P.append("""<p class="sub">Round one measured the cleared corridor on
4,813 holes across 651 real courses: tree cover falls to 8 % on the centre line
against 39 % in the untouched far field, over a tapered corridor 18 m either
side at the tee, 30 m through the landing zone and 35 m at the green. Our holes
were routed on bare earth, so their centre line carries the same cover as the
country around it. The gap between those two curves is the next round.</p>""")
        offs = np.arange(-160.0, 160.0 + 5.0, 5.0)
        P.append('<div class="charts">')
        for mode, key in (("aeolian", "sandhills"), ("fluvial", "sandhills_nc")):
            if mode not in prev:
                continue
            c = np.asarray(prev[mode]["corr"], float)
            pr = np.asarray(prev[mode]["prof"], float)
            ctr = pr[np.abs(offs) <= 10].mean()
            far = pr[np.abs(offs) >= 120].mean()
            P.append(f'<div class="card"><h3>{mode}</h3>'
                     + svg_profile(offs, [("ours", pr, "var(--ours)")])
                     + f'<div class="key">tree rate against metres from the centre line</div>'
                       f'<p class="sub" style="margin:10px 0 0">centre line '
                       f'<strong>{100 * ctr:.0f} %</strong>, far field '
                       f'<strong>{100 * far:.0f} %</strong>, ratio '
                       f'<strong>{ctr / max(far, 1e-9):.2f}</strong> against '
                       f'<strong>0.21</strong> on real courses.<br>Mean cover inside '
                       f'the tapered corridor <strong>{100 * c.mean():.0f} %</strong>, '
                       f'median <strong>{100 * np.median(c):.0f} %</strong> over '
                       f'{len(c)} holes.</p></div>')
        P.append("</div>")

    lw = prev.get("lone_window") if prev else None
    if lw:
        P.append("<h2>The lone tree, counted</h2>")
        P.append("""<p class="sub">The ask was occasional single trees for the
look of the thing, the way one tree carries a hole at Old Macdonald, and
explicitly not on every seed. Nothing in the generator places them. They fall
out of drawing each seed's cover from the real distribution, where the median
dune tile is half a percent treed: at that much cover the fractal field
naturally leaves one- and two-cell specks standing clear of everything. Counted
inside the routed play window, per seed:</p>""")
        body = []
        for mode, area_label in (("aeolian", "1.38"), ("fluvial", "0.70")):
            ks = np.array([v["k"] for v in lw.values() if v["mode"] == mode])
            ar = float(np.median([v["area_km2"] for v in lw.values() if v["mode"] == mode]))
            if not len(ks):
                continue
            body.append([mode, f"{len(ks)}", f"{ar:.2f}", f"{ks.mean():.2f}",
                         f"{100 * (ks == 0).mean():.0f} %",
                         f"{100 * ((ks >= 1) & (ks <= 2)).mean():.0f} %",
                         f"{100 * (ks >= 3).mean():.0f} %",
                         f"{ks.mean() / ar:.2f}"])
        P.append(table(["seeds", "n", "window km²", "mean trees", "none",
                        "one or two", "three or more", "per km²"], body))
        P.append("""<p class="sub">Two thirds of dune-country seeds carry no
specimen tree at all, a sixth carry one or two, and the rest carry a real
scatter. The rate inside the window, 0.96 per square kilometre, sits just above
the real dune-country median of 0.61 and inside its 0.2 to 1.1 quartile range.
An explicit specimen pass was designed and then dropped: it would have added
trees on top of a process already delivering them at the measured rate.</p>""")

    P.append("<h2>How it works, and what it does not do</h2>")
    P.append("""<p>The fitted logistic model says where trees are likelier from
eight terrain quantities the router already computes. Its skill is small, 0.09
to 0.13 against always guessing the base rate, so it biases placement and
nothing more. Thresholding it alone gives smooth blobs of one size, and real
land is not that: a dune-country tile carries one dominant stand holding half
the treed area plus a long tail of specks. So the score adds a fractal noise
field over seven octaves from 10 m to 640 m, and the threshold is the quantile
that hits the drawn coverage exactly.</p>""")
    P.append("""<p>A single smoothing length cannot do this. Sweeping one
Gaussian across every width and every mix never produced a largest stand above
13 % of the treed area, against 40 to 50 % on real tiles, because clumps of one
size give you one patch per <em>n</em>. That is structural, not a tuning miss,
and it is why the field is fractal.</p>""")
    P.append("""<p>Parameters were fitted against every real tile of each
archetype, on half our seeds, and chosen on the other half. The holdout earns
its keep: an earlier search over sixteen tiles handed back a setting that scored
0.138 on its own tiles and 0.248 on tiles it had not seen, while the runner-up
scored 0.165 and 0.168. The winner is the one that survived the split.</p>""")
    P.append("<h3>Named shortfalls</h3>")
    P.append("""<p><strong>Carolina wood edges are too clean.</strong> Our edge
density is 0.53 times the real figure. The dial that fixes it exists, and
turning it far enough drives the miss to nothing &mdash; while multiplying the
patch count fourfold and the lone-tree rate eightfold, taking the overall error
from 0.22 to 1.07. At 76 % cover the real edge density is largely
classification speckle in a 10 m satellite product rather than the geometry of a
wood, and reproducing it means generating that speckle. Left alone, and
recorded.</p>""")
    P.append("""<p><strong>Canopy density is a tile-level scalar, not a
per-cell truth.</strong> The measured density is whole-tile canopy percent over
treed percent, across two rasters at different resolutions, and real ground
carries canopy on cells the classifier does not call tree land at all. We match
the scalar exactly. Our zero bin is therefore heavier than the real one, which
is a limitation of deriving density from a binary mask, not something to tune
away.</p>""")
    P.append("""<p><strong>Trees stay out of visibility.</strong> Our blindness
numbers and the corpus's both measure bare earth. Adding tree occlusion now
would correct against a reference that excludes the thing being added.</p>""")
    P.append("""<p><strong>Nothing was written into the terrain.</strong> The
canopy is a sidecar in its own directory, in the same class codes the real
corpus uses, so the corridor measurement reads it with no change at all. The 250
frozen heightfields hash the same before and after, and a second run of the
generator is byte-identical to the first.</p>""")
    P.append("</div>")
    pathlib.Path(a.out).write_text("\n".join(P))
    print(f"wrote {a.out} ({pathlib.Path(a.out).stat().st_size / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()
