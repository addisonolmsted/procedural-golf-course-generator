#!/usr/bin/env python3
"""Routing contact sheet: nine routed holes on the hillshade.

    python3 tools/golf/routing_sheet.py <dump_dir> <out_html> seed:mode ...

Render-before-trusting instrument for the routing prototype: spines colored
by par, five tee boxes, LZ circles at true radius, numbered greens, dotted
walks, red bridge ticks, yellow x at play-line crossings.
"""
from __future__ import annotations

import pathlib
import sys

import numpy as np

_TOOLS = pathlib.Path(__file__).resolve().parent.parent
for _p in ("macro_campaign", "golf"):
    sys.path.insert(0, str(_TOOLS / _p))

from macro_campaign import cgrid                      # noqa: E402
import siting                                         # noqa: E402
import greens as greens_mod                           # noqa: E402
import routing                                        # noqa: E402
from siting_sheet import hillshade_rgb, to_uri, draw_disc  # noqa: E402

PAR_COLORS = {3: (80, 200, 255), 4: (255, 255, 255), 5: (255, 160, 40)}


def draw_line(img, a, b, color, dash=None, thick=1):
    """Raster-stamp a segment in IMAGE coords (row, col)."""
    n = max(int(np.hypot(b[0] - a[0], b[1] - a[1])) * 2, 2)
    ts = np.linspace(0, 1, n)
    ys = (a[0] + ts * (b[0] - a[0])).astype(int)
    xs = (a[1] + ts * (b[1] - a[1])).astype(int)
    if dash:
        on, off = dash
        keep = (np.arange(n) % (on + off)) < on
        ys, xs = ys[keep], xs[keep]
    for dy in range(-thick + 1, thick):
        for dx in range(-thick + 1, thick):
            yy = np.clip(ys + dy, 0, img.shape[0] - 1)
            xx = np.clip(xs + dx, 0, img.shape[1] - 1)
            img[yy, xx] = color


def draw_route(img, route, f, i0, j0, scale):
    """Draw a Route onto a window-cropped image. scale = px per metre."""
    def P(ym, xm):
        return (img.shape[0] - 1 - int((ym - i0) * scale),
                int((xm - j0) * scale))
    for h in route.holes:
        col = PAR_COLORS[h.par]
        w = h.walk_from_prev
        draw_line(img, P(*w.path[0]), P(*w.path[1]), (200, 200, 200),
                  dash=(4, 4))
        for i in range(len(h.spine) - 1):
            draw_line(img, P(*h.spine[i]), P(*h.spine[i + 1]), col, thick=2)
        for (y, x, r) in h.lzs:
            rr = max(int(r * scale), 2)
            yy, xx = np.mgrid[-rr:rr + 1, -rr:rr + 1]
            ring = (yy**2 + xx**2 <= rr**2) & (yy**2 + xx**2 >= (rr - 1)**2)
            py, px = P(y, x)
            ys, xs = np.where(ring)
            img[np.clip(py + ys - rr, 0, img.shape[0] - 1),
                np.clip(px + xs - rr, 0, img.shape[1] - 1)] = col
        for b in h.tee_boxes:
            py, px = P(*b.yx)
            s = max(int(b.size_m * scale / 2), 1)
            img[max(0, py - s):py + s + 1, max(0, px - s):px + s + 1] = \
                (150, 240, 150) if not b.graded else (240, 210, 120)
        gy, gx = P(*h.green_yx)
        draw_disc(img, gy, gx, 4, col)
        draw_disc(img, gy, gx, 1, (20, 20, 20))
        for b in h.bridges + w.bridges:
            my = (b.a_yx[0] + b.b_yx[0]) / 2
            mx = (b.a_yx[1] + b.b_yx[1]) / 2
            draw_disc(img, *P(my, mx), 3, (235, 60, 60))
    for (_i, _j, (cy, cx)) in route.crossings:
        py, px = P(cy, cx)
        for d in range(-4, 5):
            img[np.clip(py + d, 0, img.shape[0] - 1),
                np.clip(px + d, 0, img.shape[1] - 1)] = (250, 230, 60)
            img[np.clip(py + d, 0, img.shape[0] - 1),
                np.clip(px - d, 0, img.shape[1] - 1)] = (250, 230, 60)
    draw_disc(img, *P(*route.clubhouse_yx), 5, (235, 50, 50))
    return img


def main():
    dump = pathlib.Path(sys.argv[1])
    out = pathlib.Path(sys.argv[2])
    entries = []
    for spec in sys.argv[3:]:
        seed, mode = spec.split(":")
        dims = (1450.0, 950.0) if mode == "aeolian" else (1000.0, 700.0)
        z2, (_, _, c2) = cgrid.read_f32(dump / f"{seed}.cgrid")
        z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(np.float64)
        wp = dump / f"{seed}.water.cgrid"
        wet2 = (~np.isnan(cgrid.read_f32(wp)[0]) if wp.exists()
                else np.zeros(z2.shape, bool))
        sit, f, m, p = siting.run_siting(z2, c2, wet2, *dims)
        pool = greens_mod.generate(z2, c2, wet2, sit, f, m, p)

        def pool_for(ch, _z=z2, _c=c2, _w=wet2, _sit=sit, _f=f, _m=m, _p=p):
            _sit.clubhouse = ch
            return greens_mod.generate(_z, _c, _w, _sit, _f, _m, _p)
        route = routing.run_routing(z2, c2, wet2, sit, f, pool,
                                    pool_fn=pool_for)
        if route is None:
            print(f"  {seed} ({mode}): NO ROUTE")
            continue
        wy, wx, hh, ww = sit.window_m
        halo = 150.0
        i0, j0 = max(0.0, wy - halo), max(0.0, wx - halo)
        i1 = min(z2.shape[0] * c2, wy + hh + halo)
        j1 = min(z2.shape[1] * c2, wx + ww + halo)
        a0, b0 = int(i0 / c2), int(j0 / c2)
        a1, b1 = int(i1 / c2), int(j1 / c2)
        zc = z2[a0:a1, b0:b1]
        wc = wet2[a0:a1, b0:b1]
        img = hillshade_rgb(zc, c2, wc)
        img = np.ascontiguousarray(np.flipud(img))     # draw in world-up coords
        scale = 1.0 / c2
        img = draw_route(img, route, f, i0, j0, scale)
        uri = to_uri(np.flipud(img), upscale=1)
        walk_med = float(np.median([h.walk_from_prev.length_m
                                    for h in route.holes]))
        net_dzs = [h.terms.get("net_dz_m", 0) for h in route.holes]
        dogs = []
        for h in route.holes:
            sp = h.spine
            for i in range(1, len(sp) - 1):
                u, v = sp[i] - sp[i-1], sp[i+1] - sp[i]
                nu, nv = np.hypot(*u), np.hypot(*v)
                if nu > 20 and nv > 20:
                    dogs.append(float(np.degrees(np.arccos(np.clip(
                        np.dot(u, v) / (nu * nv), -1, 1)))))
        aboves = [h.terms.get("above_chord_m", 0) for h in route.holes]
        n_bridge = sum(len(h.bridges) + len(h.walk_from_prev.bridges)
                       for h in route.holes)
        entries.append(dict(
            seed=seed, mode=mode, img=uri,
            pars="-".join(str(p_) for p_ in route.par_sequence),
            lens=" ".join(f"{h.length_m:.0f}" for h in route.holes),
            total=route.total_length_m, walk=route.total_walk_m,
            walk_med=walk_med, ncross=len(route.crossings),
            nbridge=n_bridge, score=route.score,
            dz=" ".join(f"{v:+.0f}" for v in net_dzs),
            dog_med=float(np.median(dogs)) if dogs else 0.0,
            dog_max=float(max(dogs)) if dogs else 0.0,
            abmax=max(aboves),
            terms={k: round(v, 2) for k, v in route.terms.items()}))
        print(f"  {seed} ({mode}): pars {entries[-1]['pars']} "
              f"total {route.total_length_m:.0f} m walk-med {walk_med:.0f} m "
              f"cross {len(route.crossings)} bridges {n_bridge} "
              f"score {route.score:.2f}")

    cards = ""
    for e in entries:
        cards += f"""<section>
<h2>seed {e['seed']} — {e['mode']} · par {e['pars']} · {e['total']:.0f} m</h2>
<p>hole lengths {e['lens']} m · net dz {e['dz']} m (max above-chord
{e['abmax']:.1f} m; real p90 1.7) · walk total {e['walk']:.0f} m (median
{e['walk_med']:.0f}) · play crossings {e['ncross']} · bridges {e['nbridge']}
· doglegs med {e['dog_med']:.0f}° max {e['dog_max']:.0f}° (real par-4/5
p50 18-20°, p90 44-46°)
· worst clearance intrusion {e['terms'].get('worst_clear', 0):.2f}
(0 = full measured spacing kept) · score {e['score']:.2f} · {e['terms']}</p>
<figure><img src="{e['img']}"></figure>
</section><hr>"""

    html = f"""<title>Routing sheet — nine-hole routes</title>
<style>
:root{{--bg:#f4f2ec;--ink:#211f1b;--dim:#6a655c;--line:#d9d4c8;--card:#fffdf9}}
@media (prefers-color-scheme:dark){{:root:not([data-theme=light]){{--bg:#141410;--ink:#e8e4db;--dim:#989287;--line:#312d26;--card:#1d1c17}}}}
:root[data-theme=dark]{{--bg:#141410;--ink:#e8e4db;--dim:#989287;--line:#312d26;--card:#1d1c17}}
*{{box-sizing:border-box}}
body{{background:var(--bg);color:var(--ink);margin:0;
padding:2rem 1.4rem 4rem;font:15px/1.55 ui-sans-serif,system-ui}}
main{{max-width:1150px;margin:0 auto}}
h1{{font-size:1.5rem}} h2{{font-size:1.05rem;margin:.2rem 0}}
p{{color:var(--dim);margin:.2rem 0 .8rem;font-size:12.5px}}
hr{{border:0;border-top:1px solid var(--line);margin:1.6rem 0}}
figure{{margin:0;background:var(--card);border:1px solid var(--line);
border-radius:3px;overflow:hidden}}
figure img{{width:100%;display:block}}
</style>
<main><h1>Routing sheet — shot-based nine-hole routes</h1>
<p>Spines by par (blue 3 / white 4 / orange 5) · green tee boxes (amber =
graded) · LZ rings · dotted grey walks · red dots = bridges · yellow x =
play-line crossings · red disc = clubhouse.</p><hr>
{cards}</main>"""
    out.write_text(html)
    print(f"wrote {out} ({out.stat().st_size // 1024} KB)")


if __name__ == "__main__":
    main()
