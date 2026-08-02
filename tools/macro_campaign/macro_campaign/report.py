"""`progress.html` — the generated-vs-real gap, drawn.

`compare` already writes the same numbers as a markdown table, but a table
cannot show that a generated tile is a plane with objects on it while the
real one is dissected everywhere. This renders the gap two ways:

- **Side by side terrain.** Both sides go through ONE renderer
  (`dtm_atlas.qa.hillshade_rgb`) from the `.cgrid` heightfields, so nothing
  in the comparison comes from the drawing. The Rust snapshot examples were
  deliberately not used: they draw overlays and normalise elevation
  differently per example, which would make the two sides incomparable for
  reasons that have nothing to do with the terrain.
- **The diagnostic charts.** Real distribution as a band, generated median
  as a marker, against the gate's accepted ratio zone.

It measures NOTHING. Everything is read from what `extract`/`compare` have
already written, and the gate bands and noise floor come from `compare`
itself, so this page and `compare --gate` cannot disagree.

Re-run it after each milestone: the same charts, moving.
"""

from __future__ import annotations

import base64
import html
import json
import math
import pathlib
import sys

import numpy as np

from . import cgrid, compare
from .fit_knobs import load_excluded

_ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(_ROOT / "dtm_atlas"))

from dtm_atlas.qa import hillshade_rgb  # noqa: E402

OUT = pathlib.Path(__file__).resolve().parent.parent / "out"
REPORT = OUT / "report"
IMG = REPORT / "img"

# Rendered edge, px. 420 keeps the page under a couple of MB with 20 images
# (5 archetypes x 2 sides x 2 elevation-scale modes) inlined as data URIs.
IMG_PX = 420
JPEG_Q = 82

ARCH_ORDER = ["florida_lowland", "sandhills", "piedmont", "glacial_moraine", "mountain_bench"]

# The metric that picks the representative tile: whichever tile's value is
# nearest its OWN side's median is shown, so neither side is cherry-picked.
PICK_BY = "local_relief_200m"

CAPTION_METRICS = [
    ("local_relief_200m", "relief @200 m", "m"),
    ("dist_to_channel_p50_m", "dist to channel", "m"),
    ("plane_residual_frac", "plane residual", ""),
    ("core_relief_m", "core relief", "m"),
]


def _title(arch: str) -> str:
    return arch.replace("_", " ")


def _fmt(v: float, unit: str = "") -> str:
    if v is None or not np.isfinite(v):
        return "—"
    s = f"{v:.3g}" if abs(v) < 1000 else f"{v:.0f}"
    return f"{s} {unit}".strip()


# ---------------------------------------------------------------- data ----


def _real_records() -> dict[str, list[tuple[str, dict]]]:
    """(tile name, flattened metrics) per archetype, after the cull."""
    excluded, _ = load_excluded()
    per: dict[str, list[tuple[str, dict]]] = {}
    ex_dir = OUT / "extract"
    for arch_dir in sorted(ex_dir.iterdir()) if ex_dir.exists() else []:
        if not arch_dir.is_dir():
            continue
        rows = []
        for p in sorted(arch_dir.glob("*.json")):
            if p.name.endswith(".regions.json") or (arch_dir.name, p.stem) in excluded:
                continue
            rows.append((p.stem, compare._collect(json.loads(p.read_text()))))
        per[arch_dir.name] = rows
    return per


def _gen_records(seeds: int | None) -> dict[str, list[tuple[str, dict]]]:
    """Cached generated measurements. Never re-measures — `compare --force`
    is where that happens."""
    per: dict[str, list[tuple[str, dict]]] = {}
    base = OUT / "generated_measured"
    for arch_dir in sorted(base.iterdir()) if base.exists() else []:
        if not arch_dir.is_dir():
            continue
        rows = [
            (p.stem, compare._collect(json.loads(p.read_text())))
            for p in sorted(arch_dir.glob("*.json"))
        ]
        per[arch_dir.name] = rows[:seeds] if seeds else rows
    return per


def _series(recs: list[tuple[str, dict]], key: str) -> list[float]:
    return [r[key] for _n, r in recs if key in r]


def _stats(vals: list[float]) -> dict | None:
    if not vals:
        return None
    q25, med, q75 = (float(x) for x in np.percentile(vals, [25, 50, 75]))
    return {"q25": q25, "med": med, "q75": q75, "n": len(vals)}


def _representative(recs: list[tuple[str, dict]]) -> str | None:
    """Tile whose PICK_BY value is nearest that side's own median; ties go
    to the lowest name, so the choice is reproducible run to run."""
    vals = [(n, r[PICK_BY]) for n, r in recs if PICK_BY in r]
    if not vals:
        return recs[0][0] if recs else None
    med = float(np.median([v for _n, v in vals]))
    return min(vals, key=lambda nv: (abs(nv[1] - med), nv[0]))[0]


def _gate_rows(real, gen) -> list[dict]:
    """Exactly `compare.run`'s gate loop, so the two can never disagree."""
    rows = []
    for arch in ARCH_ORDER:
        for key, (lo, hi) in compare.GATES.items():
            rv = _series(real.get(arch, []), key)
            gv = _series(gen.get(arch, []), key)
            if len(rv) < 3 or not gv:
                continue
            rmed = float(np.median(rv))
            gmed = float(np.median(gv))
            ratio = compare._ratio(gmed, rmed)
            rows.append({
                "arch": arch, "key": key, "ratio": ratio, "band": (lo, hi),
                "pass": bool(np.isfinite(ratio) and lo <= ratio <= hi),
            })
    return rows


# -------------------------------------------------------------- images ----


def _render(path: pathlib.Path, zrange, dest: pathlib.Path) -> None:
    from PIL import Image

    z, (_ox, _oy, cell) = cgrid.read_f32(path)
    z = z.astype(float)
    valid = np.isfinite(z)
    if not valid.all():
        z = np.where(valid, z, np.nanmedian(z[valid]))
    rgb = hillshade_rgb(z, valid, cell, zrange=zrange)
    # cgrid row 0 is the SOUTH edge (y-up world); PNG row 0 is the top, so
    # north must go up here or the hillshade reads as inverted terrain.
    im = Image.fromarray(np.flipud(rgb))
    im = im.resize((IMG_PX, IMG_PX), Image.Resampling.LANCZOS)
    dest.parent.mkdir(parents=True, exist_ok=True)
    im.save(dest, quality=JPEG_Q, optimize=True)


def _z_percentiles(path: pathlib.Path) -> tuple[float, float]:
    z, _ = cgrid.read_f32(path)
    z = z.astype(float)
    z = z[np.isfinite(z)]
    return float(np.percentile(z, 1)), float(np.percentile(z, 99))


def _data_uri(p: pathlib.Path) -> str:
    return "data:image/jpeg;base64," + base64.b64encode(p.read_bytes()).decode("ascii")


def _pair_images(arch: str, real_tile: str, gen_tile: str, force: bool) -> dict | None:
    """Four JPEGs for one archetype: {real,gen} x {self, shared} scale.

    Self-scale shows each side's structure; shared scale puts both on one
    hypsometric range, which is the only way the relief deficit shows up as
    a picture rather than a number."""
    src = {
        "real": OUT / "tiles" / arch / f"{real_tile}.cgrid",
        "gen": OUT / "generated" / arch / f"{gen_tile}.cgrid",
    }
    if not all(p.exists() for p in src.values()):
        return None
    lo_r, hi_r = _z_percentiles(src["real"])
    lo_g, hi_g = _z_percentiles(src["gen"])
    # Shared scale = the union SPAN, re-centred on each tile's own median.
    # A literal union of absolute elevations would compare a 900 m mountain
    # tile against a 40 m generated one and paint each a single flat color:
    # what matters is how much relief each has, not where it sits in the
    # datum.
    span = max(hi_r - lo_r, hi_g - lo_g, 1e-6)
    shared = {
        "real": ((lo_r + hi_r) / 2 - span / 2, (lo_r + hi_r) / 2 + span / 2),
        "gen": ((lo_g + hi_g) / 2 - span / 2, (lo_g + hi_g) / 2 + span / 2),
    }
    out = {}
    for side in ("real", "gen"):
        for mode in ("self", "shared"):
            dest = IMG / f"{arch}_{side}_{mode}.jpg"
            if force or not dest.exists():
                _render(src[side], None if mode == "self" else shared[side], dest)
            out[f"{side}_{mode}"] = _data_uri(dest)
    out["span_m"] = span
    out["real_span_m"] = hi_r - lo_r
    out["gen_span_m"] = hi_g - lo_g
    return out


# --------------------------------------------------------------- charts ----

PAL = {"real": "var(--series-1)", "gen": "var(--series-2)"}


def _svg_open(w: int, h: int, label: str) -> list[str]:
    return [f'<svg viewBox="0 0 {w} {h}" role="img" aria-label="{html.escape(label)}" '
            f'preserveAspectRatio="xMidYMid meet">']


def _tip(text: str) -> str:
    return f' data-tip="{html.escape(text)}"'


def _lin(v, lo, hi, p0, p1):
    if hi == lo:
        return p0
    return p0 + (p1 - p0) * (v - lo) / (hi - lo)


def _log(v, lo, hi, p0, p1):
    v = max(float(v), lo)
    return p0 + (p1 - p0) * (math.log10(v) - math.log10(lo)) / (
        math.log10(hi) - math.log10(lo))


def _nice_ticks(hi: float, n: int = 4) -> list[float]:
    raw = hi / n
    mag = 10 ** math.floor(math.log10(raw)) if raw > 0 else 1
    step = min([m * mag for m in (1, 2, 2.5, 5, 10)], key=lambda s: abs(s - raw))
    ticks, t = [], 0.0
    while t <= hi + step * 0.01:
        ticks.append(round(t, 6))
        t += step
    return ticks


def chart_dist_to_channel(real, gen) -> str:
    """Dumbbell per archetype: real median + IQR against generated, with the
    gate's accepted zone shaded behind.

    The y axis is LOG. On a linear axis florida's 570 m generated median sets
    the top and squashes every real value — which sit in a 20 m band across
    all five archetypes — into an invisible smear at the bottom, hiding the
    finding the chart exists to show. Log also makes the gate zone a
    constant-height band, so columns are directly comparable."""
    W, H = 900, 300
    L, R, T, B = 58, 16, 22, 54
    key = "dist_to_channel_p50_m"
    cols, vals = [], []
    for arch in ARCH_ORDER:
        rs = _stats(_series(real.get(arch, []), key))
        gs = _stats(_series(gen.get(arch, []), key))
        if not rs or not gs:
            continue
        cols.append((arch, rs, gs))
        vals += [rs["q25"], rs["q75"], gs["q25"], gs["q75"], rs["med"], gs["med"]]
    if not cols:
        return ""
    lo, hi = min(vals) * 0.82, max(vals) * 1.25
    ticks = [t for t in (50, 75, 100, 150, 200, 300, 400, 600, 800, 1200) if lo <= t <= hi]
    s = _svg_open(W, H, "Median distance to the nearest channel, real versus generated")

    def y(v):
        return _log(v, lo, hi, H - B, T)

    for t in ticks:
        s.append(f'<line class="grid" x1="{L}" x2="{W - R}" y1="{y(t):.1f}" y2="{y(t):.1f}"/>')
        s.append(f'<text class="tick" x="{L - 8}" y="{y(t) + 4:.1f}" text-anchor="end">{t:g}</text>')
    s.append(f'<text class="axis-title" transform="translate(14,{(T + H - B) / 2}) rotate(-90)" '
             f'text-anchor="middle">metres (log)</text>')

    step = (W - L - R) / len(cols)
    lo_b, hi_b = compare.GATES[key]
    for i, (arch, rs, gs) in enumerate(cols):
        cx = L + step * (i + 0.5)
        zt, zb = y(rs["med"] * hi_b), y(rs["med"] * lo_b)
        s.append(f'<rect class="zone" x="{cx - 58:.1f}" y="{zt:.1f}" width="116" '
                 f'height="{max(zb - zt, 1):.1f}" rx="4"'
                 + _tip(f"gate zone {lo_b}–{hi_b}x the real median "
                        f"({rs['med'] * lo_b:.0f}–{rs['med'] * hi_b:.0f} m)") + "/>")
        # the gap itself, as a segment between the two medians
        s.append(f'<line class="gap" x1="{cx - 16:.1f}" y1="{y(rs["med"]):.1f}" '
                 f'x2="{cx + 16:.1f}" y2="{y(gs["med"]):.1f}"/>')
        for side, st, dx in (("real", rs, -16), ("gen", gs, 16)):
            lab = "real" if side == "real" else "generated"
            s.append(f'<line class="whisker" x1="{cx + dx:.1f}" x2="{cx + dx:.1f}" '
                     f'y1="{y(st["q25"]):.1f}" y2="{y(st["q75"]):.1f}" '
                     f'stroke="{PAL[side]}"/>')
            s.append(f'<circle class="mark dot" cx="{cx + dx:.1f}" cy="{y(st["med"]):.1f}" '
                     f'r="6" fill="{PAL[side]}"'
                     + _tip(f"{lab} {_title(arch)}: median {st['med']:.0f} m "
                            f"(IQR {st['q25']:.0f}–{st['q75']:.0f}, n={st['n']})") + "/>")
        ratio = gs["med"] / rs["med"]
        ok = lo_b <= ratio <= hi_b
        above = gs["med"] >= rs["med"]
        s.append(f'<text class="ratio {"ok" if ok else "bad"}" x="{cx + 16:.1f}" '
                 f'y="{y(gs["med"]) + (-13 if above else 20):.1f}" '
                 f'text-anchor="middle">{ratio:.1f}x</text>')
        s.append(f'<text class="cat" x="{cx:.1f}" y="{H - B + 20}" text-anchor="middle">'
                 f'{html.escape(_title(arch))}</text>')
        s.append(f'<text class="cat sub" x="{cx:.1f}" y="{H - B + 36}" text-anchor="middle">'
                 f'real {rs["med"]:.0f} · gen {gs["med"]:.0f} m</text>')
    s.append("</svg>")
    return "".join(s)


def _panels(real, gen, keys, xlabels, title, logscale, ylab, unit) -> str:
    """Five small multiples on ONE shared y scale — the point is comparing
    archetypes, which per-panel scales would quietly destroy."""
    PW, PH = 176, 210
    L, R, T, B = 46, 8, 14, 40
    W, H = L + PW * len(ARCH_ORDER) + R, PH + T + B
    vals = []
    for arch in ARCH_ORDER:
        for key in keys:
            vals += _series(real.get(arch, []), key) + _series(gen.get(arch, []), key)
    vals = [v for v in vals if np.isfinite(v) and (v > 0 or not logscale)]
    if not vals:
        return ""
    if logscale:
        lo = 10 ** math.floor(math.log10(max(min(vals), 1e-3)))
        hi = 10 ** math.ceil(math.log10(max(vals)))
        ticks = [10 ** e for e in range(int(math.log10(lo)), int(math.log10(hi)) + 1)]
        def y(v):
            return _log(v, lo, hi, T + PH, T)
    else:
        hi = max(vals) * 1.08
        ticks = _nice_ticks(hi)
        hi = max(hi, ticks[-1])
        def y(v):
            return _lin(v, 0, hi, T + PH, T)

    s = _svg_open(W, H, title)
    for t in ticks:
        s.append(f'<line class="grid" x1="{L}" x2="{W - R}" y1="{y(t):.1f}" y2="{y(t):.1f}"/>')
        s.append(f'<text class="tick" x="{L - 8}" y="{y(t) + 4:.1f}" text-anchor="end">'
                 f'{t:g}</text>')
    s.append(f'<text class="axis-title" transform="translate(12,{T + PH / 2}) rotate(-90)" '
             f'text-anchor="middle">{html.escape(ylab)}</text>')

    for i, arch in enumerate(ARCH_ORDER):
        x0 = L + PW * i
        inner = PW - 34
        xs = [x0 + 22 + inner * (j / max(len(keys) - 1, 1)) for j in range(len(keys))]
        if i:
            s.append(f'<line class="sep" x1="{x0}" x2="{x0}" y1="{T}" y2="{T + PH}"/>')
        for side, recs in (("real", real), ("gen", gen)):
            pts, band_hi, band_lo = [], [], []
            for j, key in enumerate(keys):
                st = _stats(_series(recs.get(arch, []), key))
                if not st:
                    pts = []
                    break
                pts.append((xs[j], y(st["med"]), st, key))
                band_hi.append(f"{xs[j]:.1f},{y(st['q75']):.1f}")
                band_lo.append(f"{xs[j]:.1f},{y(st['q25']):.1f}")
            if not pts:
                continue
            s.append(f'<polygon class="band" points="{" ".join(band_hi + band_lo[::-1])}" '
                     f'fill="{PAL[side]}" opacity="0.16"/>')
            s.append(f'<polyline class="series" points="'
                     + " ".join(f"{p[0]:.1f},{p[1]:.1f}" for p in pts)
                     + f'" stroke="{PAL[side]}"/>')
            for x, yy, st, key in pts:
                s.append(f'<circle class="mark dot" cx="{x:.1f}" cy="{yy:.1f}" r="4.5" '
                         f'fill="{PAL[side]}"'
                         + _tip(f"{'real' if side == 'real' else 'generated'} "
                                f"{_title(arch)} · {key}: median {_fmt(st['med'], unit)} "
                                f"(IQR {_fmt(st['q25'])}–{_fmt(st['q75'], unit)}, "
                                f"n={st['n']})") + "/>")
        for j, lab in enumerate(xlabels):
            s.append(f'<text class="tick" x="{xs[j]:.1f}" y="{T + PH + 16}" '
                     f'text-anchor="middle">{html.escape(lab)}</text>')
        s.append(f'<text class="cat" x="{x0 + PW / 2:.1f}" y="{T + PH + 34}" '
                 f'text-anchor="middle">{html.escape(_title(arch))}</text>')
    s.append("</svg>")
    return "".join(s)


def chart_bars(real, gen, key, title, ylab) -> str:
    """Paired bars, real vs generated, for a 0–1 fraction."""
    W, H = 440, 240
    L, R, T, B = 44, 10, 14, 46
    s = _svg_open(W, H, title)

    def y(v):
        return _lin(v, 0, 1, H - B, T)

    for t in (0, 0.25, 0.5, 0.75, 1.0):
        s.append(f'<line class="grid" x1="{L}" x2="{W - R}" y1="{y(t):.1f}" y2="{y(t):.1f}"/>')
        s.append(f'<text class="tick" x="{L - 8}" y="{y(t) + 4:.1f}" text-anchor="end">'
                 f'{t:g}</text>')
    s.append(f'<text class="axis-title" transform="translate(12,{(T + H - B) / 2}) rotate(-90)" '
             f'text-anchor="middle">{html.escape(ylab)}</text>')
    step = (W - L - R) / len(ARCH_ORDER)
    for i, arch in enumerate(ARCH_ORDER):
        cx = L + step * (i + 0.5)
        for side, recs, dx in (("real", real, -15), ("gen", gen, 3)):
            st = _stats(_series(recs.get(arch, []), key))
            if not st:
                continue
            hgt = max(y(0) - y(st["med"]), 1.5)
            s.append(f'<rect class="mark bar" x="{cx + dx:.1f}" y="{y(st["med"]):.1f}" '
                     f'width="12" height="{hgt:.1f}" rx="4" fill="{PAL[side]}"'
                     + _tip(f"{'real' if side == 'real' else 'generated'} {_title(arch)}: "
                            f"median {st['med']:.3f} (IQR {st['q25']:.3f}–{st['q75']:.3f}, "
                            f"n={st['n']})") + "/>")
            s.append(f'<text class="val" x="{cx + dx + 6:.1f}" y="{y(st["med"]) - 6:.1f}" '
                     f'text-anchor="middle">{st["med"]:.2f}</text>')
        s.append(f'<text class="cat" x="{cx:.1f}" y="{H - B + 18}" text-anchor="middle">'
                 f'{html.escape(_title(arch).split()[0])}</text>')
    s.append("</svg>")
    return "".join(s)


def _table(real, gen, keys, unit="") -> str:
    rows = ["<table><thead><tr><th>archetype</th><th>metric</th>"
            "<th>real median [IQR]</th><th>generated median [IQR]</th>"
            "<th>ratio</th></tr></thead><tbody>"]
    for arch in ARCH_ORDER:
        for key in keys:
            rs = _stats(_series(real.get(arch, []), key))
            gs = _stats(_series(gen.get(arch, []), key))
            if not rs or not gs:
                continue
            ratio = compare._ratio(gs["med"], rs["med"])
            rows.append(
                f"<tr><td>{html.escape(_title(arch))}</td><td>{html.escape(key)}</td>"
                f"<td>{_fmt(rs['med'], unit)} [{_fmt(rs['q25'])}–{_fmt(rs['q75'])}]</td>"
                f"<td>{_fmt(gs['med'], unit)} [{_fmt(gs['q25'])}–{_fmt(gs['q75'])}]</td>"
                f"<td>{ratio:.2f}x</td></tr>")
    rows.append("</tbody></table>")
    return "".join(rows)


# ----------------------------------------------------------------- page ----

CSS = """
:root { color-scheme: light dark; }
.viz-root {
  --surface-1:#fcfcfb; --page:#f9f9f7; --text-primary:#0b0b0b;
  --text-secondary:#52514e; --muted:#898781; --grid:#e1e0d9; --axis:#c3c2b7;
  --border:rgba(11,11,11,0.10); --series-1:#2a78d6; --series-2:#eb6834;
  --series-3:#1baf7a; --good:#0ca30c; --critical:#d03b3b; --zone:rgba(137,135,129,0.13);
}
@media (prefers-color-scheme: dark) {
  :root:where(:not([data-theme="light"])) .viz-root {
    --surface-1:#1a1a19; --page:#0d0d0d; --text-primary:#ffffff;
    --text-secondary:#c3c2b7; --muted:#898781; --grid:#2c2c2a; --axis:#383835;
    --border:rgba(255,255,255,0.10); --series-1:#3987e5; --series-2:#d95926;
    --series-3:#199e70; --zone:rgba(137,135,129,0.18);
  }
}
:root[data-theme="dark"] .viz-root {
  --surface-1:#1a1a19; --page:#0d0d0d; --text-primary:#ffffff;
  --text-secondary:#c3c2b7; --muted:#898781; --grid:#2c2c2a; --axis:#383835;
  --border:rgba(255,255,255,0.10); --series-1:#3987e5; --series-2:#d95926;
  --series-3:#199e70; --zone:rgba(137,135,129,0.18);
}
* { box-sizing:border-box; }
body { margin:0; }
.viz-root {
  background:var(--page); color:var(--text-primary); min-height:100vh;
  font-family:system-ui,-apple-system,"Segoe UI",sans-serif;
  font-size:15px; line-height:1.55; padding:28px 20px 80px;
}
.wrap { max-width:1000px; margin:0 auto; }
h1 { font-size:26px; margin:0 0 6px; letter-spacing:-0.01em; }
h2 { font-size:19px; margin:44px 0 4px; letter-spacing:-0.005em; }
h3 { font-size:15px; margin:22px 0 2px; }
p.lede { color:var(--text-secondary); margin:0 0 4px; max-width:74ch; }
p.note { color:var(--text-secondary); font-size:13.5px; margin:6px 0 0; max-width:82ch; }
.stamp { color:var(--muted); font-size:13px; }
.card {
  background:var(--surface-1); border:1px solid var(--border); border-radius:12px;
  padding:16px 18px; margin-top:14px; overflow-x:auto;
}
.tiles { display:grid; grid-template-columns:repeat(auto-fit,minmax(190px,1fr)); gap:12px; margin:18px 0 6px; }
.tile { background:var(--surface-1); border:1px solid var(--border); border-radius:12px; padding:14px 16px; }
.tile .k { font-size:12.5px; color:var(--text-secondary); text-transform:uppercase; letter-spacing:0.04em; }
.tile .v { font-size:30px; font-weight:600; letter-spacing:-0.02em; margin-top:2px; }
.tile .s { font-size:13px; color:var(--text-secondary); }
.hero { font-size:52px; font-weight:650; letter-spacing:-0.03em; line-height:1.05; }
svg { display:block; width:100%; height:auto; max-width:100%; }
.grid { stroke:var(--grid); stroke-width:1; }
.sep { stroke:var(--grid); stroke-width:1; stroke-dasharray:2 3; }
.zone { fill:var(--zone); }
.series { fill:none; stroke-width:2; }
.whisker { stroke-width:3; opacity:0.38; stroke-linecap:round; }
.gap { stroke:var(--muted); stroke-width:1.5; stroke-dasharray:3 3; }
.dot { stroke:var(--surface-1); stroke-width:2; }
.bar { stroke:var(--surface-1); stroke-width:2; }
.tick { fill:var(--muted); font-size:11px; font-variant-numeric:tabular-nums; }
.cat { fill:var(--text-secondary); font-size:12px; }
.cat.sub { fill:var(--muted); font-size:10.5px; font-variant-numeric:tabular-nums; }
.axis-title { fill:var(--muted); font-size:11px; }
.val { fill:var(--text-secondary); font-size:10.5px; font-variant-numeric:tabular-nums; }
.ratio { font-size:11.5px; font-weight:600; font-variant-numeric:tabular-nums; }
.ratio.ok { fill:var(--good); } .ratio.bad { fill:var(--critical); }
.mark { transition:opacity .12s; } .mark:hover { opacity:0.78; }
.legend { display:flex; gap:18px; flex-wrap:wrap; align-items:center; margin:2px 0 10px;
  font-size:13px; color:var(--text-secondary); }
.legend i { width:12px; height:12px; border-radius:3px; display:inline-block;
  vertical-align:-1px; margin-right:6px; }
.pair { display:grid; grid-template-columns:1fr 1fr; gap:14px; }
.pair figure { margin:0; }
.pair img { width:100%; border-radius:8px; border:1px solid var(--border); display:block; }
.pair figcaption { font-size:12.5px; color:var(--text-secondary); margin-top:6px; }
.pair .id { color:var(--muted); font-variant-numeric:tabular-nums; }
.controls { display:flex; gap:10px; flex-wrap:wrap; align-items:center; margin:14px 0 0; }
button { font:inherit; font-size:13px; padding:6px 12px; border-radius:8px;
  border:1px solid var(--border); background:var(--surface-1); color:var(--text-primary);
  cursor:pointer; }
button[aria-pressed="true"] { background:var(--series-1); color:#fff; border-color:transparent; }
details { margin-top:10px; } summary { cursor:pointer; color:var(--text-secondary); font-size:13px; }
table { border-collapse:collapse; font-size:12.5px; margin-top:8px; width:100%; }
th, td { text-align:left; padding:4px 10px 4px 0; border-bottom:1px solid var(--grid);
  font-variant-numeric:tabular-nums; }
th { color:var(--text-secondary); font-weight:600; }
.badge { display:inline-block; font-size:12px; padding:2px 8px; border-radius:999px;
  border:1px solid var(--border); }
.badge.bad { color:var(--critical); } .badge.ok { color:var(--good); }
#tip { position:fixed; pointer-events:none; opacity:0; transition:opacity .1s;
  background:var(--surface-1); color:var(--text-primary); border:1px solid var(--border);
  border-radius:8px; padding:6px 9px; font-size:12.5px; max-width:280px;
  box-shadow:0 4px 14px rgba(0,0,0,0.18); z-index:9; }
@media (max-width:720px) { .pair { grid-template-columns:1fr; } }
"""

JS = """
const tip = document.getElementById('tip');
document.addEventListener('mouseover', e => {
  const t = e.target.closest('[data-tip]');
  if (!t) return;
  tip.textContent = t.getAttribute('data-tip');
  tip.style.opacity = 1;
});
document.addEventListener('mousemove', e => {
  if (tip.style.opacity == 0) return;
  const pad = 14, w = tip.offsetWidth, h = tip.offsetHeight;
  let x = e.clientX + pad, y = e.clientY + pad;
  if (x + w > innerWidth - 8) x = e.clientX - w - pad;
  if (y + h > innerHeight - 8) y = e.clientY - h - pad;
  tip.style.left = x + 'px'; tip.style.top = y + 'px';
});
document.addEventListener('mouseout', e => {
  if (e.target.closest('[data-tip]')) tip.style.opacity = 0;
});
const scaleBtn = document.getElementById('scale-toggle');
scaleBtn.addEventListener('click', () => {
  const on = scaleBtn.getAttribute('aria-pressed') !== 'true';
  scaleBtn.setAttribute('aria-pressed', on);
  document.querySelectorAll('img[data-self]').forEach(img => {
    img.src = on ? img.dataset.shared : img.dataset.self;
  });
  document.querySelectorAll('.scale-note').forEach(n => {
    n.textContent = on ? n.dataset.shared : n.dataset.self;
  });
});
const themeBtn = document.getElementById('theme-toggle');
themeBtn.addEventListener('click', () => {
  const dark = matchMedia('(prefers-color-scheme: dark)').matches;
  const cur = document.documentElement.getAttribute('data-theme') || (dark ? 'dark' : 'light');
  document.documentElement.setAttribute('data-theme', cur === 'dark' ? 'light' : 'dark');
});
"""


def _legend(real_lab="real exemplar tiles", gen_lab="generated courses",
            zone=False) -> str:
    parts = [f'<span><i style="background:{PAL["real"]}"></i>{html.escape(real_lab)}</span>',
             f'<span><i style="background:{PAL["gen"]}"></i>{html.escape(gen_lab)}</span>']
    if zone:
        parts.append('<span><i style="background:var(--zone)"></i>gate zone (accepted ratio)</span>')
    parts.append('<span class="stamp">bands = interquartile range</span>')
    return '<div class="legend">' + "".join(parts) + "</div>"


def _spike_table(real_recs, path: pathlib.Path) -> str:
    """The spike measured by the REAL extractor, against real medians."""
    from .extract import measure_tile

    res = measure_tile(path)
    if res is None:
        return '<p class="note">spike tile could not be measured</p>'
    m = {**(res[0].get("knobs") or {}), **(res[0].get("metrics") or {})}
    rows = ["<table><thead><tr><th>gated metric</th><th>real piedmont</th>"
            "<th>M5 spike</th><th>ratio</th><th></th></tr></thead><tbody>"]
    n_pass = 0
    for key, (lo, hi) in compare.GATES.items():
        rv = _series(real_recs, key)
        if not rv or key not in m:
            continue
        rm = float(np.median(rv))
        ratio = m[key] / rm if rm else float("nan")
        ok = bool(np.isfinite(ratio) and lo <= ratio <= hi)
        n_pass += ok
        rows.append(
            f"<tr><td>{html.escape(key)}</td><td>{_fmt(rm)}</td>"
            f"<td>{_fmt(m[key])}</td><td>{ratio:.2f}x</td>"
            f'<td><span class="badge {"ok" if ok else "bad"}">'
            f'{"pass" if ok else "fail"}</span></td></tr>')
    rows.append("</tbody></table>")
    head = (f'<p class="note"><b>{n_pass}/{len(compare.GATES)} structural gates '
            f'pass</b> — including <code>slope_median</code>, added because every '
            f'other gate here is satisfiable by simply smoothing the surface.</p>')
    return head + "".join(rows)


def build(force: bool = False, seeds: int | None = None) -> pathlib.Path:
    real = _real_records()
    gen = _gen_records(seeds)
    if not any(gen.values()):
        raise SystemExit(
            "no generated measurements — run:\n"
            "  cargo run -p course-macro --example export_tiles --release\n"
            "  python3 -m macro_campaign compare")

    gates = _gate_rows(real, gen)
    n_pass = sum(g["pass"] for g in gates)
    noise = compare.baseline(compare._real_halves())

    # Hero: the single number that says why the redesign is happening.
    rel = []
    for arch in ARCH_ORDER:
        rv = _series(real.get(arch, []), "local_relief_100m")
        gv = _series(gen.get(arch, []), "local_relief_100m")
        if rv and gv:
            rel.append(float(np.median(gv)) / float(np.median(rv)))
    hero = float(np.median(rel)) if rel else float("nan")

    body: list[str] = ['<div class="viz-root"><div class="wrap">']
    body.append('<div class="controls" style="justify-content:flex-end;margin:0 0 8px">'
                '<button id="theme-toggle">theme</button></div>')
    body.append("<h1>Macro landform — generated vs real</h1>")
    body.append(
        '<p class="lede">The generator draws terrain from a prior fitted to real '
        'topography. This page asks one question: does the result belong to the '
        'same population as the landscapes it was fitted from? Every number here '
        'is read from what <code>extract</code> and <code>compare</code> already '
        'wrote — this page measures nothing of its own.</p>')

    body.append('<div class="tiles">')
    body.append(
        f'<div class="tile"><div class="k">relief at hole scale</div>'
        f'<div class="v">{hero * 100:.0f}%</div>'
        f'<div class="s">of real, median across archetypes (100 m window)</div></div>')
    body.append(
        f'<div class="tile"><div class="k">structural gates</div>'
        f'<div class="v">{n_pass}/{len(gates)}</div>'
        f'<div class="s">generated median inside the accepted ratio band</div></div>')
    n_arch_ok = len({g["arch"] for g in gates}) - len({g["arch"] for g in gates if not g["pass"]})
    body.append(
        f'<div class="tile"><div class="k">archetypes clean</div>'
        f'<div class="v">{n_arch_ok}/{len({g["arch"] for g in gates})}</div>'
        f'<div class="s">no failing gate</div></div>')
    body.append(
        f'<div class="tile"><div class="k">corpus</div>'
        f'<div class="v">{sum(len(v) for v in real.values())}</div>'
        f'<div class="s">real tiles after the cull, vs '
        f'{sum(len(v) for v in gen.values())} generated</div></div>')
    body.append("</div>")

    # ---------------------------------------------------------- terrain --
    body.append("<h2>The terrain, side by side</h2>")
    body.append(
        '<p class="lede">Both columns are the same 3 km box at 2 m, rendered by the '
        'same hillshade code from the raw heightfields. Each archetype shows the '
        'tile whose relief at 200 m is nearest its own side\'s median, so neither '
        'side is cherry-picked.</p>')
    body.append(
        '<div class="controls"><button id="scale-toggle" aria-pressed="false">'
        'shared elevation scale</button>'
        '<span class="stamp">off: each image stretched to its own relief (shows '
        'structure) · on: both stretched to the same span (shows the deficit)</span>'
        '</div>')

    for arch in ARCH_ORDER:
        rr, gr = real.get(arch, []), gen.get(arch, [])
        if not rr or not gr:
            continue
        rt, gt = _representative(rr), _representative(gr)
        imgs = _pair_images(arch, rt, gt, force)
        if not imgs:
            continue
        rmap = dict(rr)[rt]
        gmap = dict(gr)[gt]

        def cap(m):
            return " · ".join(
                f"{lab} {_fmt(m.get(k), u)}" for k, lab, u in CAPTION_METRICS if k in m)

        body.append(f"<h3>{html.escape(_title(arch))}</h3>")
        body.append('<div class="card"><div class="pair">')
        for side, tile, m, lab in (("real", rt, rmap, "real exemplar"),
                                   ("gen", gt, gmap, "generated course")):
            body.append(
                f'<figure><img alt="{html.escape(lab)} hillshade, {html.escape(_title(arch))}" '
                f'data-self="{imgs[f"{side}_self"]}" data-shared="{imgs[f"{side}_shared"]}" '
                f'src="{imgs[f"{side}_self"]}"/>'
                f'<figcaption><b style="color:{PAL[side]}">{html.escape(lab)}</b> '
                f'<span class="id">{html.escape(tile)}</span><br>{html.escape(cap(m))}'
                f'</figcaption></figure>')
        body.append("</div>")
        body.append(
            f'<p class="note scale-note" data-self="Own scale: each image uses its own '
            f'1st–99th percentile elevation range — real spans '
            f'{imgs["real_span_m"]:.0f} m, generated {imgs["gen_span_m"]:.0f} m." '
            f'data-shared="Shared scale: both images stretched over '
            f'{imgs["span_m"]:.0f} m of elevation. The flatter image is flatter '
            f'terrain, not a different colour choice.">'
            f'Own scale: each image uses its own 1st–99th percentile elevation range '
            f'— real spans {imgs["real_span_m"]:.0f} m, generated '
            f'{imgs["gen_span_m"]:.0f} m.</p>')
        body.append("</div>")

    # ------------------------------------------------------- M5 spike ----
    spike = OUT / "generated_spike" / "piedmont" / "current.cgrid"
    if spike.exists():
        body.append("<h2>M5 spike — the network construction, measured</h2>")
        body.append(
            '<p class="lede">Not the shipping generator: a hand-wired config with '
            'the grown drainage network, slope-area long profiles, convex '
            'hillslopes and <b>every ridge, bowl and bluff deleted</b>. It exists '
            'to answer whether the primitive engine can produce real macro '
            'character, or whether the approach needed replacing. Interfluves '
            'here are not placed — they are what is left between channels.</p>')
        img = IMG / "spike_piedmont.jpg"
        if force or not img.exists():
            _render(spike, None, img)
        rt = _representative(real.get("piedmont", []))
        rimg = IMG / "piedmont_real_self.jpg"
        body.append('<div class="card"><div class="pair">')
        body.append(
            f'<figure><img alt="real piedmont exemplar" src="{_data_uri(rimg)}"/>'
            f'<figcaption><b style="color:{PAL["real"]}">real exemplar</b> '
            f'<span class="id">{html.escape(rt or "")}</span></figcaption></figure>')
        body.append(
            f'<figure><img alt="M5 spike" src="{_data_uri(img)}"/>'
            f'<figcaption><b style="color:{PAL["gen"]}">M5 spike</b> '
            f'<span class="id">125 reaches, no ridges placed</span></figcaption></figure>')
        body.append("</div>")
        body.append(_spike_table(real.get("piedmont", []), spike) + "</div>")

    # ------------------------------------------------------------ charts --
    body.append("<h2>Where you are relative to a channel</h2>")
    body.append(
        '<p class="lede">Real ground is space-filling: no matter which archetype, '
        'the median cell is 105–125 m from the nearest drainage. A passing '
        'generator lands inside the shaded zone in every column.</p>')
    body.append('<div class="card">' + _legend(zone=True)
                + chart_dist_to_channel(real, gen)
                + '<details><summary>table</summary>'
                + _table(real, gen, ["dist_to_channel_p50_m"], "m") + "</details></div>")

    body.append("<h2>Relief at the scale a hole is played</h2>")
    body.append(
        '<p class="lede">Relief measured inside a moving window. Real terrain has '
        'most of its relief already present at 100 m; if the generated line only '
        'climbs at 400 m, the terrain is a smooth regional tilt with nothing '
        'inside a golf hole. Passing means the two lines overlap, especially at '
        'the left of each panel. Log scale, shared across all five panels.</p>')
    body.append('<div class="card">' + _legend()
                + _panels(real, gen,
                          ["local_relief_100m", "local_relief_200m", "local_relief_400m"],
                          ["100", "200", "400"],
                          "Local relief versus window size, real versus generated",
                          True, "relief (m, log)", "m")
                + '<p class="note">x: window size in metres.</p>'
                + '<details><summary>table</summary>'
                + _table(real, gen, ["local_relief_100m", "local_relief_200m",
                                     "local_relief_400m"], "m") + "</details></div>")

    body.append("<h2>Does the network keep branching?</h2>")
    body.append(
        '<p class="lede">Channel length per unit area as the accumulation threshold '
        'drops. Real networks keep finding new low-order tributaries as the '
        'threshold falls, so the line climbs steeply to the right. A generator '
        'that draws a few big valleys and nothing else produces a flat line. '
        'Passing means matching slopes, not just the endpoint.</p>')
    body.append('<div class="card">' + _legend()
                + _panels(real, gen,
                          ["drainage_density_4x", "drainage_density_1x",
                           "drainage_density_0p25x"],
                          ["4x", "1x", "0.25x"],
                          "Drainage density versus accumulation threshold",
                          False, "km / km²", "")
                + '<p class="note">x: accumulation threshold, as a multiple of the '
                  'nominal channel-initiation area (lower = finer tributaries).</p>'
                + '<details><summary>table</summary>'
                + _table(real, gen, ["drainage_density_4x", "drainage_density_1x",
                                     "drainage_density_0p25x"]) + "</details></div>")

    body.append("<h2>Plane, or landscape?</h2>")
    body.append(
        '<p class="lede">Left: the fraction of the tile explained by its own '
        'best-fit plane — high means "a tilted plane with a few objects on it". '
        'Right: the fraction of the tile the geomorphon classifier calls ridge. '
        'Passing means the left bars come down to the real level and the right '
        'bars come up to it.</p>')
    body.append('<div class="card">' + _legend()
                + '<div class="pair">'
                + chart_bars(real, gen, "plane_residual_frac",
                             "Fraction of the tile explained by its best-fit plane",
                             "plane fraction")
                + chart_bars(real, gen, "ridge_mask_area_frac",
                             "Fraction of the tile classified as ridge",
                             "ridge area fraction")
                + "</div>"
                + '<p class="note">Left: plane-explained fraction (lower is more '
                  'dissected). Right: ridge-mask coverage. Both 0–1.</p>'
                + '<details><summary>table</summary>'
                + _table(real, gen, ["plane_residual_frac", "ridge_mask_area_frac"])
                + "</details></div>")

    # ------------------------------------------------------------- gates --
    body.append("<h2>Gate status</h2>")
    body.append(
        '<p class="lede">The same bands <code>compare --gate</code> enforces. '
        '<em>Noise</em> is how far the ratio moves when one half of the REAL '
        'corpus is compared to the other half — a gate band only means something '
        'if it is wider than that.</p>')
    rows = ["<div class=\"card\"><table><thead><tr><th>archetype</th><th>metric</th>"
            "<th>ratio</th><th>band</th><th></th><th>real-vs-real noise (p50/max)</th>"
            "</tr></thead><tbody>"]
    for g in gates:
        nz = noise.get(g["key"]) or {}
        p50, mx = nz.get("ratio_spread_p50"), nz.get("ratio_spread_max")
        rows.append(
            f'<tr><td>{html.escape(_title(g["arch"]))}</td>'
            f'<td>{html.escape(g["key"])}</td>'
            f'<td>{g["ratio"]:.2f}x</td><td>{g["band"][0]}–{g["band"][1]}x</td>'
            f'<td><span class="badge {"ok" if g["pass"] else "bad"}">'
            f'{"pass" if g["pass"] else "fail"}</span></td>'
            f'<td>{"—" if p50 is None or not np.isfinite(p50) else f"{p50:.2f}x / {mx:.2f}x"}'
            f'</td></tr>')
    rows.append("</tbody></table></div>")
    body.append("".join(rows))

    body.append(
        '<h2>How to read this</h2>'
        '<p class="lede">Real values come from exemplar 3 km tiles on protected '
        'land (developed tiles are culled, so road ditches are not counted as '
        'channels). Generated values come from the same extractor run over the '
        'planner\'s <code>base_height</code> at the same resolution — identical '
        'code on both sides. Sample sizes are small on both sides, so a single '
        'ratio near 1 is not proof of anything; the shape of the curves is the '
        'evidence.</p>')
    body.append("</div></div><div id=\"tip\"></div>")

    REPORT.mkdir(parents=True, exist_ok=True)
    page = (f"<title>Macro landform — generated vs real</title>"
            f"<style>{CSS}</style>" + "".join(body) + f"<script>{JS}</script>")
    path = REPORT / "progress.html"
    path.write_text(page)
    return path


def run(force: bool = False, seeds: int | None = None) -> int:
    path = build(force=force, seeds=seeds)
    kb = path.stat().st_size / 1024
    print(f"wrote {path} ({kb:.0f} KB)")
    return 0
