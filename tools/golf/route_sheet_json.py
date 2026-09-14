"""Contact sheet of routed courses from batch records (Rust or Python).

  python3 tools/golf/route_sheet_json.py <dump_dir> <records.jsonl> <out.html> [--n 24] [--prefix m_] [--title ..]

Each card: the tile's hillshade cropped to the route (+60 m), water in
blue, spines per hole (colour by par: 3 green, 4 blue, 5 orange), tee
boxes as small squares (graded ones amber), landing zones as circles,
greens as dots, bridges red, crossings yellow, the clubhouse a red disc,
and the walk between holes as a dotted grey line. Caption: seed, mode,
pars, total length, walk, crossings, seconds. Same drawing conventions as
routing_sheet.py, read from the JSON instead of live Route objects.
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


def card(dump, prefix, rec, px=640):
    s = rec["seed"]
    z, (_, _, c) = cgrid.read_f32(str(dump / f"{prefix}{s}.cgrid")); z = z.astype(float)
    wp = dump / f"{prefix}{s}.water.cgrid"
    wet = ~np.isnan(cgrid.read_f32(str(wp))[0]) if wp.exists() else np.zeros(z.shape, bool)
    n = z.shape[0]
    ys, xs = [], []
    wy, wx, hh, ww = rec["window_m"]; ys += [wy, wy + hh]; xs += [wx, wx + ww]
    for h in rec["holes"]:
        for (py, pxx) in h["spine"]: ys.append(py); xs.append(pxx)
        for (py, pxx) in h["tees"]: ys.append(py); xs.append(pxx)
        for (ly, lx, lr) in h["lzs"]: ys += [ly - lr, ly + lr]; xs += [lx - lr, lx + lr]
    ys.append(rec["clubhouse"][0]); xs.append(rec["clubhouse"][1])
    pad = 60.0
    i0, j0 = max(0.0, min(ys) - pad), max(0.0, min(xs) - pad)
    i1, j1 = min(n * c, max(ys) + pad), min(n * c, max(xs) + pad)
    a0, b0, a1, b1 = int(i0 / c), int(j0 / c), int(i1 / c), int(j1 / c)
    zc, wc = z[a0:a1, b0:b1], wet[a0:a1, b0:b1]
    img = np.stack([shade(zc, c) * 225 + 25] * 3, -1); img[wc] = (70, 120, 200)
    im = Image.fromarray(np.flipud(img.astype(np.uint8)))
    scale = px / max(im.width, im.height)
    im = im.resize((max(1, int(im.width * scale)), max(1, int(im.height * scale))), Image.LANCZOS)
    dr = ImageDraw.Draw(im)
    H = im.height
    def P(y, x):
        return ((x - j0) / c * scale, H - 1 - (y - i0) / c * scale)
    prev_green = None
    for h in rec["holes"]:
        col = PAR_COL.get(h["par"], (200, 200, 200))
        pts = [P(y, x) for (y, x) in h["spine"]]
        if prev_green is not None and pts:
            dr.line([P(*prev_green), pts[0]], fill=(120, 120, 120), width=1)
        if len(pts) > 1:
            dr.line(pts, fill=col, width=3)
        for (ly, lx, lr) in h["lzs"]:
            cx, cy = P(ly, lx); r = lr / c * scale
            dr.ellipse([cx - r, cy - r, cx + r, cy + r], outline=col, width=2)
        for k, (ty, tx) in enumerate(h["tees"]):
            cx, cy = P(ty, tx); r = max(2, 7 / c * scale / 2)
            dr.rectangle([cx - r, cy - r, cx + r, cy + r], fill=(150, 240, 150) if k < 2 else (240, 210, 120))
        gx, gy = P(*h["green"]); dr.ellipse([gx - 5, gy - 5, gx + 5, gy + 5], fill=col, outline=(20, 20, 20))
        dr.text((gx + 6, gy - 6), str(h.get("index", 0) + 1) if "index" in h else "", fill=(20, 20, 20))
        prev_green = tuple(h["green"])
    cx, cy = P(*rec["clubhouse"]); dr.ellipse([cx - 6, cy - 6, cx + 6, cy + 6], fill=(235, 50, 50))
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
