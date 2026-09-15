"""Contact sheet of routed courses from batch records (Rust or Python).

  python3 tools/golf/route_sheet_json.py <dump_dir> <records.jsonl> <out.html> [--n 24] [--prefix m_] [--title ..]

Each card: the tile's hillshade cropped to the route (+60 m), water in
blue, spines per hole (colour by par: 3 green, 4 blue, 5 orange), tee
boxes as small squares (graded ones amber), landing zones as circles,
greens as dots, bridges red, crossings yellow, the clubhouse a red disc,
and the walk between holes as a dotted grey line. Caption: seed, mode,
pars, total length, walk, crossings, seconds. Same drawing conventions as
routing_sheet.py, read from the JSON instead of live Route objects.

The drawing itself is `draw_route(draw, rec, P, opts)` on a transparent
RGBA layer (window rectangle included), which `route_before_after.py`
reuses as its route overlay with the audit's marks.
"""
import sys, json, base64, io, pathlib
import numpy as np
from PIL import Image, ImageDraw
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                      # noqa: E402

PAR_COL = {3: (60, 170, 80), 4: (60, 110, 220), 5: (235, 140, 40)}


def shade(z, c):
    gy, gx = np.gradient(z, c); az, al = np.deg2rad(315), np.deg2rad(45)
    s = np.arctan(np.hypot(gx, gy)); a = np.arctan2(-gx, gy)
    return np.clip(np.sin(al) * np.cos(s) + np.cos(al) * np.sin(s) * np.cos(az - a), 0, 1)


def _ring(dr, cx, cy, r, col, width):
    dr.ellipse([cx - r, cy - r, cx + r, cy + r], outline=col, width=width)


def draw_route(draw, rec, P, opts=None):
    """Draw one routed course on `draw`, an ImageDraw over a transparent RGBA image.

    P(y, x) -> (px, py) maps world metres to pixels. opts (all optional):
      mpp      pixels per metre (default 1.0), scales the metre-sized marks
      window   draw the play-window rectangle (default True)
      holes    route_audit per-hole records for this course, same order as
               rec["holes"]: red rings on green-in-play offenders (thick when a
               case is within 35 m), blue rings on LZs / greens within 60 m of
               water, thick red segments for carries
      labels   number the greens (default True)
    v2 record keys (bridges as a list, tee_graded, crossings, walk.path) are
    drawn when present; a v1 record draws without them.
    """
    o = {"mpp": 1.0, "window": True, "holes": None, "labels": True}
    o.update(opts or {})
    mpp, dr = o["mpp"], draw
    if o["window"] and rec.get("window_m"):
        wy, wx, hh, ww = rec["window_m"]
        dr.rectangle([P(wy + hh, wx), P(wy, wx + ww)], outline=(40, 40, 40, 170), width=1)
    prev_green = None
    audit = o["holes"] or []
    for k, h in enumerate(rec["holes"]):
        col = PAR_COL.get(h["par"], (200, 200, 200))
        pts = [P(y, x) for (y, x) in h["spine"]]
        walk = h.get("walk") if isinstance(h.get("walk"), dict) else None
        if walk and walk.get("path"):
            dr.line([P(y, x) for (y, x) in walk["path"]], fill=(120, 120, 120, 255), width=1)
        elif prev_green is not None and pts:
            dr.line([P(*prev_green), pts[0]], fill=(120, 120, 120, 255), width=1)
        if len(pts) > 1:
            dr.line(pts, fill=col, width=3)
        # bridges: v2 lists of {a, b, span_m, kind} on the hole and under walk.bridges;
        # a spine span of 15-70 m is a forced carry (thick red), any other bridge a thin red segment
        spine_b = [b for b in (h["bridges"] if isinstance(h.get("bridges"), list) else []) if b.get("kind", "spine") != "walk"]
        walk_b = [b for b in (h["bridges"] if isinstance(h.get("bridges"), list) else []) if b.get("kind", "spine") == "walk"]
        walk_b += walk["bridges"] if walk and isinstance(walk.get("bridges"), list) else []
        for b in spine_b + walk_b:
            carry = b in spine_b and 15.0 <= float(b.get("span_m", 0)) <= 70.0
            dr.line([P(*b["a"]), P(*b["b"])], fill=(220, 30, 30, 255), width=6 if carry else 2)
        for (ly, lx, lr) in h["lzs"]:
            cx, cy = P(ly, lx); r = lr * mpp
            dr.ellipse([cx - r, cy - r, cx + r, cy + r], outline=col, width=2)
        graded = h.get("tee_graded") if isinstance(h.get("tee_graded"), list) else None
        for t, (ty, tx) in enumerate(h["tees"]):
            cx, cy = P(ty, tx); r = max(2, 7 * mpp / 2)
            amber = graded[t] if graded and t < len(graded) else t >= 2
            dr.rectangle([cx - r, cy - r, cx + r, cy + r], fill=(240, 210, 120, 255) if amber else (150, 240, 150, 255))
        gx, gy = P(*h["green"]); dr.ellipse([gx - 5, gy - 5, gx + 5, gy + 5], fill=col, outline=(20, 20, 20))
        if o["labels"]:
            dr.text((gx + 6, gy - 6), str(h.get("index", k) + 1), fill=(20, 20, 20, 255))
        if k < len(audit) and audit[k]:
            a = audit[k]; rr = max(8, 14 * mpp)
            for (ly, lx, lr), wet in zip(h["lzs"], a.get("water_lz", [])):
                if wet:
                    cx, cy = P(ly, lx); _ring(dr, cx, cy, lr * mpp + 4, (40, 110, 230, 255), 2)
            if a.get("water_green"):
                _ring(dr, gx, gy, rr, (40, 110, 230, 255), 2)
            if a.get("gip50"):
                _ring(dr, gx, gy, rr + 4, (225, 30, 30, 255), 4 if a.get("gip35") else 2)
        prev_green = tuple(h["green"])
    for x in (rec["crossings"] if isinstance(rec.get("crossings"), list) else []):
        cx, cy = P(x[2], x[3]); dr.ellipse([cx - 5, cy - 5, cx + 5, cy + 5], fill=(250, 220, 40, 255), outline=(20, 20, 20))
    cx, cy = P(*rec["clubhouse"]); dr.ellipse([cx - 6, cy - 6, cx + 6, cy + 6], fill=(235, 50, 50, 255))


def route_bbox(rec):
    """(ymin, xmin, ymax, xmax) of everything the route draws, in metres."""
    ys, xs = [], []
    wy, wx, hh, ww = rec["window_m"]; ys += [wy, wy + hh]; xs += [wx, wx + ww]
    for h in rec["holes"]:
        for (py, pxx) in h["spine"]: ys.append(py); xs.append(pxx)
        for (py, pxx) in h["tees"]: ys.append(py); xs.append(pxx)
        for (ly, lx, lr) in h["lzs"]: ys += [ly - lr, ly + lr]; xs += [lx - lr, lx + lr]
    ys.append(rec["clubhouse"][0]); xs.append(rec["clubhouse"][1])
    return min(ys), min(xs), max(ys), max(xs)


def card(dump, prefix, rec, px=640):
    s = rec["seed"]
    z, (_, _, c) = cgrid.read_f32(str(dump / f"{prefix}{s}.cgrid")); z = z.astype(float)
    wp = dump / f"{prefix}{s}.water.cgrid"
    wet = ~np.isnan(cgrid.read_f32(str(wp))[0]) if wp.exists() else np.zeros(z.shape, bool)
    n = z.shape[0]
    y0, x0, y1, x1 = route_bbox(rec)
    pad = 60.0
    i0, j0 = max(0.0, y0 - pad), max(0.0, x0 - pad)
    i1, j1 = min(n * c, y1 + pad), min(n * c, x1 + pad)
    a0, b0, a1, b1 = int(i0 / c), int(j0 / c), int(i1 / c), int(j1 / c)
    zc, wc = z[a0:a1, b0:b1], wet[a0:a1, b0:b1]
    img = np.stack([shade(zc, c) * 225 + 25] * 3, -1); img[wc] = (70, 120, 200)
    im = Image.fromarray(np.flipud(img.astype(np.uint8)))
    scale = px / max(im.width, im.height)
    im = im.resize((max(1, int(im.width * scale)), max(1, int(im.height * scale))), Image.LANCZOS).convert("RGBA")
    H = im.height
    def P(y, x):
        return ((x - j0) / c * scale, H - 1 - (y - i0) / c * scale)
    layer = Image.new("RGBA", im.size, (0, 0, 0, 0))
    draw_route(ImageDraw.Draw(layer), rec, P, {"mpp": scale / c})
    im = Image.alpha_composite(im, layer).convert("RGB")
    buf = io.BytesIO(); im.save(buf, "WEBP", quality=82)
    return "data:image/webp;base64," + base64.b64encode(buf.getvalue()).decode()


def main():
    a = sys.argv[1:]
    n_max, prefix, title = 24, "m_", "Routed courses"
    for k in ("--n", "--prefix", "--title"):
        if k in a:
            i = a.index(k); v = a[i + 1]; a = a[:i] + a[i + 2:]
            if k == "--n": n_max = int(v)
            elif k == "--prefix": prefix = v
            else: title = v
    dump, recs, out = pathlib.Path(a[0]), [json.loads(l) for l in open(a[1])], a[2]
    recs = [r for r in recs if r.get("routed")][:n_max]
    cards = []
    for r in recs:
        uri = card(dump, prefix, r)
        cap = (f'<b>{r["seed"]}</b> <span class="mode">{r["mode"]}</span> · pars {"".join(str(p) for p in r["pars"])} '
               f'({sum(r["pars"])}) · {r["total_length_m"]:.0f} m · walk {r["total_walk_m"]:.0f} m · '
               f'crossings {r["n_crossings"]} · score {r["score"]:.1f} · {r["seconds"]:.2f} s')
        cards.append(f'<figure class="card {r["mode"]}"><img src="{uri}" alt="route"><figcaption>{cap}</figcaption></figure>')
    html = f"""<title>{title}</title>
<style>
:root{{--bg:#f6f4ee;--panel:#fffdf8;--ink:#22231f;--muted:#5f5e57;--rule:#ddd8c9;--accent:#8a3b2a}}
@media (prefers-color-scheme:dark){{:root:not([data-theme="light"]){{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72}}}}
:root[data-theme="dark"]{{--bg:#191a17;--panel:#22231f;--ink:#e8e5dc;--muted:#a7a49a;--rule:#3a3b35;--accent:#e08a72}}
body{{background:var(--bg);color:var(--ink);font-family:"IBM Plex Sans",system-ui,sans-serif;margin:0;padding:20px 26px 48px}}
h1{{font-size:1.4rem;margin:0 0 4px}} p{{color:var(--muted);max-width:80ch;margin:0 0 16px}}
.grid{{display:grid;grid-template-columns:repeat(auto-fill,minmax(640px,1fr));gap:16px}}
.card{{margin:0;background:var(--panel);border:1px solid var(--rule);border-radius:3px;padding:8px}}
.card img{{width:100%;display:block}} figcaption{{margin-top:6px;font-size:12px;color:var(--muted);font-family:ui-monospace,monospace}} figcaption b{{color:var(--ink)}} .mode{{color:var(--accent)}}
</style>
<h1>{title}</h1>
<p>Spines by par (3 green, 4 blue, 5 orange); tee boxes as squares (amber = graded), landing zones as circles, greens as dots, walks dotted grey, clubhouse red. Crop = the route plus 60 m.</p>
<div class="grid">{"".join(cards)}</div>"""
    open(out, "w").write(html)
    print(f"wrote {out}: {len(cards)} cards, {pathlib.Path(out).stat().st_size / 1e6:.1f} MB")


if __name__ == "__main__":
    main()
