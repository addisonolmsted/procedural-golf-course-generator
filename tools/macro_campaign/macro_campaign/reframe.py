"""`reframe.html` — what the macro reframe actually changed, in pictures.

`progress.html` tracks the generated-vs-real gap over time. This page answers
a narrower question that the gap charts cannot: *what did the construction
change from, and to?* Before the reframe the macro surface was a tilted plane
with independently placed objects on it — ridges drawn at four random
candidate centres, keeping whichever fell farthest from a drain, with no
structural relation to the drainage at all. After it, the surface is the
lower envelope of a grown drainage network, and the interfluves are not
placed: they are what is left between channels.

It also carries the stages BETWEEN those two, because the route mattered.
Three separate levers each drove the gate set to a pass while making the
terrain visibly worse, which is why the shape-sensitive `slope_median` gate
exists and why a human look is part of the acceptance check.

Everything is read from tiles already on disk and measured with the same
`extract.measure_tile` that measures the real exemplars. Self-contained: no
external assets, images inlined.
"""

from __future__ import annotations

import html
import pathlib

import numpy as np

from . import compare, report

OUT = report.OUT
IMG = report.REPORT / "img" / "reframe"

# (file, short label, what it demonstrates). Ordered as the work happened.
STAGES = [
    ("out/generated/piedmont/seed1.cgrid", "before",
     "The shipping generator: a tilted plane, one trunk plus a few tributaries "
     "joined in a star, and ridges placed at random centres. This is the state "
     "that prompted the question."),
    ("out/generated_spike/piedmont/seed1_c0.cgrid", "network",
     "The drainage network grown to a space-filling dendritic form, its lower "
     "envelope becoming the surface. Every placed ridge deleted — the "
     "interfluves here are the network's complement."),
    ("out/generated_spike/piedmont/seed1_c90.cgrid", "over-smoothed",
     "A cautionary frame, not progress. Widening the divide blend band drove "
     "distance-to-channel to 1.01x and drainage density to 1.03x of real — "
     "near-perfect numbers on a surface of smooth blobs and hairline slashes."),
    ("out/generated_spike/piedmont/seed1_d430h140.cgrid", "after",
     "Convex hillslopes, channels that wander to the measured sinuosity, and "
     "growth to the FINE drainage-density threshold so low-order tributaries "
     "reach into the interfluves. 125 reaches, Strahler 4."),
]
REAL = "out/tiles/piedmont/t03520_07988.cgrid"

HEADLINE = [
    ("dist_to_channel_p50_m", "distance to a channel", "m"),
    ("local_relief_100m", "relief at hole scale (100 m)", "m"),
    ("ridge_mask_area_frac", "ridge coverage", ""),
    ("plane_residual_frac", "explained by its own best-fit plane", ""),
    ("slope_median", "median slope", ""),
]


def _measure(path: pathlib.Path) -> dict:
    from .extract import measure_tile

    res = measure_tile(path)
    if res is None:
        return {}
    return {**(res[0].get("knobs") or {}), **(res[0].get("metrics") or {})}


def build(force: bool = False) -> pathlib.Path:
    real_recs = report._real_records().get("piedmont", [])
    rmed = {}
    for key, _lab, _u in HEADLINE:
        v = report._series(real_recs, key)
        rmed[key] = float(np.median(v)) if v else float("nan")
    for key in compare.GATES:
        v = report._series(real_recs, key)
        if v:
            rmed[key] = float(np.median(v))

    IMG.mkdir(parents=True, exist_ok=True)
    real_img = IMG / "real.jpg"
    if force or not real_img.exists():
        report._render(OUT.parent / REAL, None, real_img)

    stages = []
    for rel, label, blurb in STAGES:
        src = OUT.parent / rel
        if not src.exists():
            continue
        img = IMG / f"{label.replace(' ', '_')}.jpg"
        if force or not img.exists():
            report._render(src, None, img)
        stages.append({
            "label": label, "blurb": blurb, "uri": report._data_uri(img),
            "m": _measure(src),
        })

    body = ['<div class="viz-root"><div class="wrap">']
    body.append('<div class="controls" style="justify-content:flex-end;margin:0 0 8px">'
                '<button id="theme-toggle">theme</button></div>')
    body.append("<h1>The macro reframe — before and after</h1>")
    body.append(
        '<p class="lede">Step 03 used to build a tilted plane and composite '
        'independent objects onto it. Ridges in particular were drawn at four '
        'random candidate centres, keeping whichever landed farthest from a '
        'drain — no structural relation to the drainage whatever, which is '
        'exactly how they read. The reframe grows a drainage network instead '
        'and takes the surface as its lower envelope, so interfluves are not '
        'placed at all: they are what is left between channels. Every tile '
        'below is piedmont at 2 m, rendered by one hillshade path, and '
        'measured by the same extractor that measures the real exemplar.</p>')

    # ---- the pictures -------------------------------------------------
    body.append('<div class="card"><h3 style="margin-top:0">Reference</h3>'
                f'<img class="wide" alt="real piedmont exemplar" src="{report._data_uri(real_img)}"/>'
                '<p class="note">A real piedmont tile (Uwharrie NF). Everything '
                'below is trying to be this.</p></div>')

    for i, st in enumerate(stages):
        cls = "bad" if st["label"] == "over-smoothed" else ("ok" if st["label"] == "after" else "")
        body.append('<div class="card">')
        body.append(f'<h3 style="margin-top:0"><span class="step">{i + 1}</span> '
                    f'{html.escape(st["label"])}'
                    + (' <span class="badge bad">numbers passed, terrain worse</span>'
                       if cls == "bad" else "") + "</h3>")
        body.append(f'<img class="wide" alt="{html.escape(st["label"])}" src="{st["uri"]}"/>')
        body.append(f'<p class="note">{html.escape(st["blurb"])}</p>')
        body.append(_row_table(st["m"], rmed))
        body.append("</div>")

    # ---- the numbers ---------------------------------------------------
    body.append("<h2>The headline numbers</h2>")
    body.append(
        '<p class="lede">Ratios are to the real piedmont median. The last row '
        'is the one that matters most: relief at the scale a golf hole is '
        'played was 0.15x of real for the whole life of the project.</p>')
    body.append('<div class="card">' + _summary_table(stages, rmed) + "</div>")

    body.append("<h2>Why one of these frames is a warning</h2>")
    body.append(
        '<p class="lede">Most gated metrics can be satisfied by SMOOTHING. A '
        'blurred surface still has its channels where they were, so '
        'distance-to-channel, drainage density, local relief and plane '
        'residual all improve as it blurs — the over-smoothed frame reaches '
        '1.01x on distance-to-channel and 1.03x on drainage density, better '
        'than the frame shipped as "after".</p>')
    body.append(
        '<p class="lede"><code>slope_median</code> was added to the gate set '
        'for exactly this reason, and it is worth being precise about how far '
        'it goes: it <b>does</b> catch over-smoothing driven by hillslope '
        'convexity (0.41x and 0.21x of real at convexity 0.70 and 0.85, both '
        'failing), and it <b>does not</b> catch this one — the frame above '
        'measures 0.77x, comfortably inside the band, and passes 7 of 8 '
        'gates. Only looking at it caught this. That is the honest state of '
        'the acceptance check: the gate set is necessary and not sufficient, '
        'and a human look at the render is part of the procedure rather than '
        'a courtesy.</p>')

    body.append("</div></div><div id=\"tip\"></div>")
    page = ("<title>Macro reframe — before and after</title><style>"
            + report.CSS + EXTRA_CSS + "</style>" + "".join(body)
            + f"<script>{THEME_JS}</script>")
    path = report.REPORT / "reframe.html"
    path.write_text(page)
    return path


def _row_table(m: dict, rmed: dict) -> str:
    if not m:
        return '<p class="note">not measured</p>'
    cells = []
    for key, lab, unit in HEADLINE:
        v, r = m.get(key), rmed.get(key)
        if v is None or r is None or not np.isfinite(r):
            continue
        band = compare.GATES.get(key)
        ok = band and band[0] <= v / r <= band[1]
        # ONE class attribute. Emitting a second silently loses the colour:
        # browsers keep the first and drop the rest.
        mark = "" if band is None else (" ok" if ok else " bad")
        cells.append(f"<div class='stat'><div class='k'>{html.escape(lab)}</div>"
                     f"<div class='v{mark}'>{report._fmt(v, unit)}</div>"
                     f"<div class='s'>{v / r:.2f}x real</div></div>")
    return "<div class='stats'>" + "".join(cells) + "</div>"


def _summary_table(stages: list[dict], rmed: dict) -> str:
    heads = "".join(f"<th>{html.escape(s['label'])}</th>" for s in stages)
    rows = [f"<table><thead><tr><th>metric</th><th>real</th>{heads}</tr></thead><tbody>"]
    for key, lab, unit in HEADLINE:
        r = rmed.get(key)
        if r is None or not np.isfinite(r):
            continue
        tds = []
        for s in stages:
            v = s["m"].get(key)
            if v is None:
                tds.append("<td>—</td>")
                continue
            band = compare.GATES.get(key)
            ok = band and band[0] <= v / r <= band[1]
            cls = "" if band is None else (" class='ok'" if ok else " class='bad'")
            tds.append(f"<td{cls}>{report._fmt(v, unit)} <span class='sub'>"
                       f"{v / r:.2f}x</span></td>")
        rows.append(f"<tr><td>{html.escape(lab)}</td><td>{report._fmt(r, unit)}</td>"
                    + "".join(tds) + "</tr>")
    # gate count per stage
    tds = []
    for s in stages:
        n = sum(1 for k, (lo, hi) in compare.GATES.items()
                if k in s["m"] and rmed.get(k) and lo <= s["m"][k] / rmed[k] <= hi)
        tds.append(f"<td><b>{n}/{len(compare.GATES)}</b></td>")
    rows.append("<tr><td><b>structural gates passing</b></td><td>—</td>"
                + "".join(tds) + "</tr></tbody></table>")
    return "".join(rows)


EXTRA_CSS = """
img.wide { width:100%; max-width:520px; border-radius:10px;
  border:1px solid var(--border); display:block; margin:0 auto; }
.step { display:inline-block; width:24px; height:24px; line-height:24px;
  text-align:center; border-radius:999px; background:var(--series-1);
  color:#fff; font-size:13px; margin-right:8px; }
.stats { display:grid; grid-template-columns:repeat(auto-fit,minmax(150px,1fr));
  gap:10px; margin-top:12px; }
.stat { border:1px solid var(--border); border-radius:10px; padding:10px 12px; }
.stat .k { font-size:11.5px; color:var(--text-secondary); text-transform:uppercase;
  letter-spacing:0.04em; }
.stat .v { font-size:21px; font-weight:600; letter-spacing:-0.01em; }
.stat .v.ok { color:var(--good); } .stat .v.bad { color:var(--critical); }
.stat .s { font-size:12px; color:var(--muted); font-variant-numeric:tabular-nums; }
td.ok { color:var(--good); } td.bad { color:var(--critical); }
td .sub { color:var(--muted); font-size:11.5px; }
"""

THEME_JS = """
const themeBtn = document.getElementById('theme-toggle');
themeBtn.addEventListener('click', () => {
  const dark = matchMedia('(prefers-color-scheme: dark)').matches;
  const cur = document.documentElement.getAttribute('data-theme') || (dark ? 'dark' : 'light');
  document.documentElement.setAttribute('data-theme', cur === 'dark' ? 'light' : 'dark');
});
"""


def run(force: bool = False) -> int:
    path = build(force=force)
    print(f"wrote {path} ({path.stat().st_size / 1024:.0f} KB)")
    return 0
