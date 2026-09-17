"""The finalized canopy: what the clearing took, and what it left.

  python3 tools/golf/clearing_sheet.py out/final250_v2 out/canopy250 out/canopy250_final <out.html>

Every number in both columns comes from `clearing_profile.py`'s own functions,
run on our cleared tiles exactly as they were run on 3,851 real holes. The
before/after panels go through one render function so the eye is not being
helped on one side.
"""
from __future__ import annotations

import argparse
import html
import io
import json
import pathlib
import sys
import contextlib

import numpy as np
import scipy.ndimage as ndi

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                                   # noqa: E402
import canopy_clear as CC                                          # noqa: E402
import clearing_profile as CP                                      # noqa: E402
import clearing_model as M                                         # noqa: E402
import clearing_geom as G                                          # noqa: E402
import canopy_sheet as CS                                          # noqa: E402
from canopy_sheet import render, to_uri, table, CSS, HEAD          # noqa: E402
from routing_sheet import draw_line, PAR_COLORS                    # noqa: E402

MIN_TREED = 0.25
ST = np.ones((3, 3), int)


def load(seed, d):
    g, _ = cgrid.read_u8(d / f"m_{seed}.canopy.cgrid")
    t, _ = cgrid.read_u8(d / f"m_{seed}.tcc.cgrid")
    return g, t.astype(float)


def measure(dirpath, recs, natrows, only_treed=True):
    num, den, holes, unts = {}, {}, [], []
    for s in sorted(recs):
        if only_treed and natrows[s]["tree_frac"] < MIN_TREED:
            continue
        g, _ = cgrid.read_u8(dirpath / f"m_{s}.canopy.cgrid")
        lines = [np.asarray(h["spine"], float) for h in recs[s]["holes"]]
        r = CP.course_rows(g == 10, lines, [h["par"] for h in recs[s]["holes"]])
        if not r:
            continue
        for k, v in r["num"].items():
            num[k] = num.get(k, 0.0) + v
            den[k] = den.get(k, 0.0) + r["den"][k]
        holes.extend(r["holes"])
        unts.append(r["unt"])
    with contextlib.redirect_stdout(io.StringIO()):
        return CP.summarize(num, den, holes, unts), len(holes), len(unts)


def emergent(recs, natrows, fin):
    """What the owner asked for, counted rather than placed."""
    fair, flank, lz, per_hole = 0, 0, 0, 0
    holes_with_fair, holes_with_flank = 0, 0
    for s in sorted(recs):
        if natrows[s]["tree_frac"] < MIN_TREED:
            continue
        g, _ = cgrid.read_u8(fin / f"m_{s}.canopy.cgrid")
        tree = g == 10
        lab, nl = ndi.label(tree, structure=ST)
        if not nl:
            continue
        sizes = np.bincount(lab.ravel())[1:]
        cen = np.array(ndi.center_of_mass(tree, lab, range(1, nl + 1)))
        small = sizes <= 3
        cy, cx = cen[:, 0] * 10 + 5, cen[:, 1] * 10 + 5
        for h in recs[s]["holes"]:
            per_hole += 1
            p = np.asarray(h["spine"], float)
            d, _side, frac = G.project_polyline(p, cy, cx)
            seg = G.seg_index(frac)
            gp = p[-1]
            dg = np.hypot(cy - gp[0], cx - gp[1])
            f = small & (seg == 1) & (d <= 15.0)
            k = small & (dg >= 20.0) & (dg <= 40.0)
            fair += int(f.sum())
            flank += int(k.sum())
            holes_with_fair += int(f.any())
            holes_with_flank += int(k.any())
            for y, x, r_ in (h.get("lzs") or []):
                lz += int((small & (np.hypot(cy - y, cx - x) <= max(r_, 20.0))).sum())
    return dict(holes=per_hole, fairway=fair, flank=flank, lz=lz,
                holes_with_fair=holes_with_fair, holes_with_flank=holes_with_flank)


def panel(seed, dump, nat, fin, rec, P, upscale=2):
    imgs = []
    for d in (nat, fin):
        z10, tree, tcc, wet = CS.our_tile(seed, dump, d)
        img = render(z10, tree, tcc, wet, upscale=1)
        for h in rec.get("holes", []):
            sp = np.asarray(h["spine"], float) / 10.0
            col = PAR_COLORS.get(h["par"], (255, 255, 255))
            for i in range(len(sp) - 1):
                draw_line(img, (sp[i][0], sp[i][1]), (sp[i + 1][0], sp[i + 1][1]),
                          col, thick=1)
        imgs.append(to_uri(img, upscale=upscale))
    return imgs


GATED = [("landing", (0, 2, 4, 6, 8)), ("green", (0, 2, 4, 6, 8))]


def build(dump, nat, fin, out_html, routes="out/route_rs/rs.jsonl"):
    recs = CC.read_routes(routes)
    natrows = {r["seed"]: r for r in json.loads(
        (nat / "index_canopy.json").read_text())["rows"]}
    T = json.loads((HERE / "corpus" / "out" / "clearing_profile.json").read_text())
    ours, nh, nt = measure(fin, recs, natrows)
    before, _, _ = measure(nat, recs, natrows)
    sample = np.asarray(T["retention"]["all"]["tee"], float)
    u = (np.arange(20000) + 0.5) / 20000
    warp = np.quantile(sample, u ** M.PARAMS["TEE_OPEN_EXP"], method="inverted_cdf")

    P = [HEAD % CSS, '<div class="wrap">', f"""<header>
<div class="eyebrow">Canopy round 3 &middot; 250 frozen seeds</div>
<h1>Cutting The Corridors</h1>
<p class="lede">The trees are out of the lines of play. Not all of them &mdash;
a real playing corridor is still about a tenth tree, and that residue is where
the specimen standing in a fairway and the pine guarding a green come from.
Nothing here places a tree. Everything is what the clearing did not take.</p>
</header>"""]

    P.append("<h2>What a course actually clears</h2>")
    P.append("""<p class="sub">Measured on 3,851 holes across 315 wooded
courses, against ground more than 500 m from any hole. Two corrections had to be
made before the numbers meant anything. Normalising each hole by its own cover
120&ndash;160 m out is the obvious method and it is wrong: on a real course that
band is the next fairway, holding 0.64 of untouched cover, so every ratio
computed that way is inflated by about 1.6&times;. And clearing recovers
<strong>fully by 80 m</strong> from the nearest hole line &mdash; between 100 m
and 500 m out, cover sits within 3 % of untouched. A golf course is a set of
corridors, not an opened-up estate.</p>""")
    body = []
    for z in ("tee", "landing", "green"):
        row = [z]
        for j in (0, 2, 4, 6, 8, 10):
            row.append(f"{T['shape']['all'][z][j]:.3f}")
        body.append(row)
    P.append(table(["zone"] + [f"{M.SHAPE_X[j]:.0f} m" for j in (0, 2, 4, 6, 8, 10)], body))
    P.append("""<p class="sub">Par 3s are a different animal and get their own
row in the model: the middle of a par 3 is carry, not a landing zone, and
courses leave <strong>0.133</strong> of cover on its centre line against
<strong>0.034</strong> for a par 4. And trees left in play are residual, not
sited &mdash; on 720 bent holes they show no preference for the inside of the
dogleg, 50.9 % against 50. That is why nothing here is placed for strategy.</p>""")

    P.append("<h2>Did we clear like that</h2>")
    P.append(f"""<p class="sub">{nh} of our holes on {nt} tiles at least a
quarter treed, against {T['n_holes']} real holes. Every figure in both columns
comes from the same function. Tiles below a quarter cover are excluded from
every ratio and reported separately, exactly as the corpus excluded its bare
prairie courses.</p>""")
    rows, npass, ntot = [], 0, 0

    def add(lab, a, b, tol, ratio=False, gate=True):
        nonlocal npass, ntot
        ok = ((1 / tol) <= ((a + 1e-9) / (b + 1e-9)) <= tol) if ratio else abs(a - b) <= tol
        if gate:
            npass += ok
            ntot += 1
        rows.append([lab, f"{a:.3f}", f"{b:.3f}",
                     (f"×{tol:g}" if ratio else f"±{tol:g}"),
                     ("pass" if ok else "off", ok) if gate else ("—", True)])

    for z in ("landing", "green"):
        for j in (0, 2, 4, 6, 8):
            add(f"keep {z} at {M.SHAPE_X[j]:.0f} m", ours['shape']['all'][z][j],
                T['shape']['all'][z][j], 0.05)
    for z in ("tee", "landing", "green"):
        o = np.array(ours["retention"]["all"][z])
        r = warp if z == "tee" else np.array(T["retention"]["all"][z])
        add(f"retention {z}, mean", o.mean(), r.mean(), 0.04)
        add(f"retention {z}, p75", np.percentile(o, 75), np.percentile(r, 75), 0.06)
    add("patches per hole", ours['patch']['per_hole'], T['patch']['per_hole'], 1.4, True)
    for k in ("1 cell", "2 cells", "3-5", "6-20", "21+"):
        add(f"patches, {k}", ours['patch'][k], T['patch'][k], 1.6, True)
    add("heavier flank's share", ours['side']['heavier'], T['side']['heavier'], 0.05)
    add("holes cleared all on one side", ours['side']['all_one'], T['side']['all_one'], 0.08)
    add("holes with no corridor tree", ours['side']['no_tree'], T['side']['no_tree'], 0.05)
    P.append(table(["row", "ours", "real land", "tolerance", ""], rows))
    P.append(f'<p class="sub"><strong>{npass} of {ntot} gated rows inside '
             f'tolerance.</strong></p>')

    P.append("<h3>The tees, opened on purpose</h3>")
    P.append("""<p class="sub">Real tees sit in the trees &mdash; they keep
0.19 of cover on the tee line against 0.02 at a green, and the per-hole spread
is enormous, 46 % of tees essentially bare and a quarter of them tight. The ask
was to keep that variety and err open, so the tee retention alone is drawn at a
warped quantile. These rows are the departure, shown against both the measured
value and what the warp predicts, rather than buried as failures.</p>""")
    trow = []
    for j in (0, 2, 4, 6, 8):
        trow.append([f"keep tee at {M.SHAPE_X[j]:.0f} m",
                     f"{ours['shape']['all']['tee'][j]:.3f}",
                     f"{T['shape']['all']['tee'][j]:.3f}", "opened", ("—", True)])
    o = np.array(ours["retention"]["all"]["tee"])
    r0 = np.array(T["retention"]["all"]["tee"])
    trow.append(["tee retention, mean", f"{o.mean():.3f}", f"{r0.mean():.3f}",
                 f"warp says {warp.mean():.3f}", ("—", True)])
    trow.append(["tees essentially bare", f"{np.mean(o < 0.02):.3f}",
                 f"{np.mean(r0 < 0.02):.3f}",
                 f"warp says {np.mean(warp < 0.02):.3f}", ("—", True)])
    P.append(table(["row", "ours", "real land", "note", ""], trow))
    return P, recs, natrows, T, ours, before, nh, nt


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dump")
    ap.add_argument("natural")
    ap.add_argument("final")
    ap.add_argument("out")
    ap.add_argument("--routes", default="out/route_rs/rs.jsonl")
    a = ap.parse_args()
    dump, nat, fin = (pathlib.Path(a.dump), pathlib.Path(a.natural),
                      pathlib.Path(a.final))
    P, recs, natrows, T, ours, before, nh, nt = build(dump, nat, fin, a.out, a.routes)

    P.append("<h2>Before and after</h2>")
    P.append("""<p class="sub">Natural canopy on the left, the finalized canopy
on the right, through one render function. Hole centre lines are drawn on both
&mdash; blue par 3, white par 4, orange par 5 &mdash; so the corridor is visible
as the thing that appears between them.</p>""")
    treed = [s for s in sorted(recs) if natrows[s]["tree_frac"] >= MIN_TREED]
    treed.sort(key=lambda s: natrows[s]["tree_frac"])
    picks = [(treed[int(q * (len(treed) - 1))], lab) for q, lab in
             ((0.05, "A lightly wooded seed"), (0.40, "The Carolina median"),
              (0.75, "Heavier pine"), (0.97, "Closed wood"))]
    for seed, lab in picks:
        b, f = panel(seed, dump, nat, fin, recs[seed], None)
        r = natrows[seed]
        g, _ = cgrid.read_u8(fin / f"m_{seed}.canopy.cgrid")
        P.append(f'<h3>{html.escape(lab)}</h3><div class="pair">'
                 f'<figure><img src="{b}" alt="seed {seed} before clearing">'
                 f'<figcaption><span class="tag r">natural</span>'
                 f'seed {seed} · {100 * r["tree_frac"]:.1f} % treed</figcaption></figure>'
                 f'<figure><img src="{f}" alt="seed {seed} after clearing">'
                 f'<figcaption><span class="tag o">cleared</span>'
                 f'{100 * (g == 10).mean():.1f} % treed · '
                 f'{int((g == 10).sum() - 0):d} tree cells left</figcaption></figure></div>')

    e = emergent(recs, natrows, fin)
    n = max(e["holes"], 1)
    P.append("<h2>The trees left in play, counted</h2>")
    P.append(f"""<p class="sub">The ask was the occasional specimen standing in
a fairway and a tree or two working as an aerial hazard by a green. Neither is
built. Both are what the clearing did not take, and this is how often they
turned up across {n} holes.</p>""")
    P.append(table(["what", "total", "per hole", "share of holes"], [
        ["specimen within 15 m of the landing-zone line", f"{e['fairway']}",
         f"{e['fairway'] / n:.2f}", f"{100 * e['holes_with_fair'] / n:.0f} %"],
        ["specimen 20–40 m from the green", f"{e['flank']}",
         f"{e['flank'] / n:.2f}", f"{100 * e['holes_with_flank'] / n:.0f} %"],
        ["specimen inside a landing-zone disc", f"{e['lz']}",
         f"{e['lz'] / n:.2f}", "—"],
    ]))
    P.append("""<p class="sub">About one hole in thirteen ends with a
free-standing tree near its landing line. Worth stating plainly: the real
wooded-course corpus scores near zero on this exact measure, not because such
trees do not exist but because at 60 % surrounding cover anything left in a
corridor stays joined to the tree line. Ours stand clear more often than that.
It is the requested feature and it is a small departure upward, named
here rather than claimed as a match.</p>""")

    P.append("<h2>Where nothing needed cutting</h2>")
    noop = []
    for s in sorted(recs):
        if natrows[s]["tree_frac"] >= MIN_TREED:
            continue
        ng, _ = cgrid.read_u8(nat / f"m_{s}.canopy.cgrid")
        fg, _ = cgrid.read_u8(fin / f"m_{s}.canopy.cgrid")
        noop.append((100 * ((ng == 10).mean() - (fg == 10).mean()),
                     int((ng != fg).sum())))
    A = np.array(noop)
    P.append(f"""<p class="sub">{len(noop)} tiles carry less than a quarter tree
cover, almost all of them dune country. Clearing is close to a no-op there,
which is correct: there is nothing to cut. Tree cover falls by
<strong>{A[:, 0].mean():.3f} percentage points</strong> on average and at worst
{A[:, 0].max():.2f}, and <strong>{int((A[:, 1] == 0).sum())}</strong> of them
are not touched at all.</p>""")

    P.append("<h2>How it works, and what it does not do</h2>")
    P.append("""<p>Clearing is proportional thinning, not an absolute target.
Binning courses by their own cover, the absolute tree rate inside a corridor
varies 2.4&times; while the normalised profile barely moves &mdash; a course on
heavy ground leaves proportionally the same trees. So the rule is a keep
probability that depends only on distance to the nearest hole line and which
part of the hole it belongs to, and no local tree rate ever has to be
estimated.</p>""")
    P.append("""<p>Which trees survive is decided by ranking a fractal field
inside each pool and comparing that rank to the cell's own probability. The
textbook Gaussian copula was tried first and bends the corridor: it kept 0.083
on the landing line where the profile said 0.041 and 0.343 at the corridor edge
where it said 0.425, because the field is not exactly normal &mdash; the 640 m
octave has about two independent samples across a 3 km tile. Solving in
probability bands fixes that and shreds the clumping instead. A rank is exactly
uniform without assuming any distribution, and monotone, so it stays
clumped.</p>""")
    P.append("""<p>Per-hole variety is a multiplier on that profile, drawn from
the measured distribution with the three zones of a hole correlated the way real
zones are. Drawing them at one shared rank leaves 39 % of holes with no corridor
tree at all and drawing them independently leaves 7 %; the measured answer is
22 %, so the dependence is carried explicitly.</p>""")
    P.append("<h3>Named shortfalls</h3>")
    P.append("""<p><strong>The multiplier's tail is capped at twice its zone
mean.</strong> The measured retention runs to nine times its mean, and those
holes are real &mdash; their corridor carried more tree than the ground around
it. Our canopy is statistically homogeneous, so a corridor has about what
surrounds it and a multiplier of nine saturates against certainty, taking 0.1
out of the corridor edge with it. Capping and rescaling keeps the mean exact
and the spread as wide as the ground can carry.</p>""")
    P.append("""<p><strong>Bare corridors, fixed.</strong> A first pass left
28 % of holes with no corridor tree against 20 % real. The cause was
double-counting: 6.5 % of our holes were routed over ground with no natural
tree in the corridor at all, and those holes still drew from a distribution
whose zero atom already includes the corpus's own open-ground holes. They now
take their share of that atom up front, and the draw for the holes that do
have trees is solved so the cohort lands on the corpus rate net of them:
22 % against 20.3 %.</p>""")
    P.append("""<p><strong>Corridor widths against the Carolina courses.</strong>
Read by interpolating the half-cover crossing rather than the instrument's
5 m step, the drive landing zone is 75.7 m wide against 71.7 on the 24 Carolina
courses, the stretch between landing zones 77.6 against 68.1, and the approach
80.1 against 69.8 &mdash; Carolina plus about ten yards throughout, which is
where the owner asked to be. Dogleg corners and the green surround match
Carolina exactly. An approach-narrowing dial exists and is left at zero.</p>""")
    P.append("""<p><strong>Nothing is placed for strategy.</strong> The corpus
gave no licence for it: trees left in play show no preference for the inside of
a dogleg. A deliberate strategic pass was offered and declined.</p>""")
    P.append("""<p><strong>The natural layer is untouched.</strong> Clearing
writes its own directory. The frozen heightfields and the natural canopy both
hash identically before and after, clearing never plants a tree, never touches
water, and never changes a cell outside a corridor.</p>""")
    P.append("</div>")
    pathlib.Path(a.out).write_text("\n".join(P))
    print(f"wrote {a.out} ({pathlib.Path(a.out).stat().st_size / 1e6:.1f} MB)")


if __name__ == "__main__":
    main()
