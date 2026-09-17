"""Cut the corridors: the finalized canopy.

  python3 tools/golf/canopy_clear.py out/final250_v2 out/canopy250 out/canopy250_final

Round 2 grew the natural canopy and left the fluvial seeds correct and
unplayable -- the median fluvial hole runs a corridor that is 98 % treed. This
removes the trees a course would remove, and leaves the ones a course would
leave, which is not none: 8.5 % of a real playing corridor is still tree
against 38.9 % in the woods beside it.

The two things the owner asked for -- the odd specimen standing in a fairway,
and trees near a green working as an aerial hazard -- are not built here. They
fall out of matching the measured retention, the same way the lone dune-country
trees fell out of the coverage distribution last round. The corpus is explicit
that this is the honest construction: on 720 bent holes, trees left in play show
no preference for the inside of the dogleg, 50.9 % against 50. They are what the
clearing did not take, not what an architect placed.

Writes a NEW directory. `out/canopy250/` stays as the natural layer, because it
is what round 2's acceptance is built against and it is the denominator of every
statistic here.
"""
from __future__ import annotations

import argparse
import json
import pathlib
import sys
import time

import numpy as np
import scipy.ndimage as ndi
from scipy.special import ndtri

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                                   # noqa: E402
import clearing_geom as G                                          # noqa: E402
import clearing_model as model                                     # noqa: E402
import canopy_gen as cg                                            # noqa: E402
import canopy_model as cmod                                        # noqa: E402

CLASS_TREE, CLASS_OTHER, CLASS_WATER = 10, 30, 80
OFFS = np.arange(-160.0, 160.0 + 5.0, 5.0)
ZONES = model.ZONES
EPS = 1e-4
# probability bands for every quantile solve. Finer near zero, because that is
# where the corridor lives and where a band's width turns straight into a
# profile error.
_E = np.concatenate([np.linspace(0.0, 0.2, 9), np.linspace(0.25, 1.0, 16)])
BANDS = list(zip(_E[:-1], _E[1:]))
MULT_CAP = 2.0      # how many times its zone mean a hole's retention may reach


def read_routes(path):
    out = {}
    for ln in pathlib.Path(path).open():
        r = json.loads(ln)
        out[r["seed"]] = r
    return out


def retention_draw(recs, params, open_ground=frozenset(), treed_seeds=frozenset()):
    """(seed, hole, zone) -> retention, by correlated stratified inverse-CDF.

    Two properties have to hold at once and neither is free.

    The MARGINAL must reproduce the measured per-hole distribution, which is
    wildly over-dispersed with a hard atom at zero -- no parametric family fits
    it, so it is an inverse-CDF from the empirical sample, and read off the step
    function (`inverted_cdf`) so every retention we generate is one that was
    actually observed on a real hole. That is `canopy_gen.coverage_for`'s
    argument and its reason.

    The DEPENDENCE between a hole's three zones must also hold. Drawing them at
    one shared rank makes a hole uniformly bare or uniformly treed and leaves
    39 % of holes with no corridor tree at all; drawing them independently
    leaves 7 %. The measured answer is 22 %, with rank correlations 0.64, 0.63
    and 0.44. So the three zones are drawn as correlated Gaussians and then
    RANKED within the cohort -- ranking keeps the copula exactly and hands each
    zone back an exact marginal.

    The tee zone, and only the tee zone, is warped. That is the owner's
    departure: some tees tight and some open, erring open. A monotone
    reweighting of quantiles, so the tightest tee in the corpus stays reachable.
    """
    L = np.linalg.cholesky(np.asarray(model.ZONE_CORR, float)
                           + 1e-9 * np.eye(len(ZONES)))
    cohorts, n_open = {}, {}
    for seed, r in recs.items():
        for i, h in enumerate(r.get("holes", [])):
            par = int(h["par"])
            if (seed, i) in open_ground:
                # a corridor with no natural tree is bare whatever it draws:
                # it fills one of the distribution's bare slots rather than
                # drawing again on top of them
                n_open[par] = n_open.get(par, 0) + 1
                for zone in ZONES:
                    out_open = (seed, i, zone)
                    cohorts.setdefault("_open", []).append(out_open)
                continue
            cohorts.setdefault(par, []).append((seed, i))
    out = {k: 0.0 for k in cohorts.pop("_open", [])}
    for par, members in cohorts.items():
        z = np.array([L @ np.random.default_rng(
            [int(s), int(i), model.TAG_ZONE]).standard_normal(len(ZONES))
            for s, i in members])
        n = len(members)
        # The open-ground holes have already spent part of the corpus's bare
        # share, so the holes that DO have trees must draw bare less often:
        # solve for the share of each zone's zero atom to skip so that the
        # cohort's drawn-bare rate lands where the corpus's realised rate says,
        # net of the open-ground holes. Only the atom is compressed -- sliding
        # the whole range lifted every quantile (green p75 0.34 -> 0.41).
        n_treed = sum(1 for s, i in members if s in treed_seeds)
        n_o = n_open.get(par, 0)
        open_share = n_o / max(n_treed + n_o, 1)
        want = max((model.SIDE.get("no_tree", 0.2) - open_share) / max(1 - open_share, 1e-9), 0.0) \
            if getattr(model, "OPEN_GROUND_FILLS_ATOM", False) and n_treed else None
        samples = {zone: np.asarray(model.retention_for(par, zone), float) for zone in ZONES}
        base_u = {}
        for zi, zone in enumerate(ZONES):
            e = params["TEE_OPEN_EXP"] if zone == "tee" else 1.0
            order = np.argsort(np.argsort(z[:, zi], kind="stable"), kind="stable")
            base_u[zone] = ((order + 0.5) / n) ** e
        treed_mask = np.array([s in treed_seeds for s, i in members])

        def draw(u0):
            rr = {}
            for zone in ZONES:
                sample, u = samples[zone], base_u[zone]
                atom = float(np.mean(sample < 0.02))
                if 0.0 < u0 < atom:
                    # the bottom u0 of the atom is lifted to sit JUST ABOVE it,
                    # on the smallest non-zero retentions; the rest of the atom
                    # stays bare and nothing above it moves. Compressing within
                    # the atom, the first attempt, left those holes on zero.
                    u = np.where(u < u0, atom + u, u)
                rr[zone] = np.quantile(sample, np.clip(u, 0, 1), method="inverted_cdf")
            return rr

        u0 = 0.0
        if want is not None:
            lo, hi = 0.0, 0.6
            for _ in range(30):
                mid = 0.5 * (lo + hi)
                rr = draw(mid)
                bare = np.mean(np.all([rr[zn][treed_mask] < 0.02 for zn in ZONES], axis=0))
                if bare > want:
                    lo = mid
                else:
                    hi = mid
            u0 = 0.5 * (lo + hi)
        drawn = draw(u0)
        for zone in ZONES:
            r = drawn[zone]
            # Bound the tail, then restore the mean. The measured distribution
            # runs to nine times its own mean, and those holes are real: their
            # corridor carried more tree than the ground around it. Our canopy
            # is statistically homogeneous, so a corridor has about what
            # surrounds it and a multiplier of nine is simply unreachable -- it
            # saturates against 1 and takes 0.1 out of the corridor edge with
            # it. Capping and rescaling keeps the mean exact and the spread as
            # wide as the ground can actually carry.
            mu = max(float(r.mean()), 1e-9)
            r = np.minimum(r, MULT_CAP * mu)
            r *= mu / max(float(r.mean()), 1e-9)
            for j, (s, i) in enumerate(members):
                out[(s, i, zone)] = float(r[j])
    return out


def untouched_rate(tree, lines):
    """The tile's tree rate on ground no hole has touched.

    Not the old per-hole band 120-160 m out along the normals: on a real course
    that band is the next fairway and holds only 0.64 of untouched cover, and on
    our own tiles 0.80, so the two sides would be normalised by differently
    contaminated denominators. Beyond 500 m from every line there is nothing to
    contaminate -- the corpus shows cover flat within 3 % of untouched from
    100 m all the way out.
    """
    ys, xs = G.cell_centres(tree.shape)
    d = np.full(tree.shape, np.inf)
    for p in lines:
        dd, _s, _f = G.project_polyline(np.asarray(p, float), ys, xs)
        d = np.minimum(d, dd)
    m = d > model.UNTOUCHED_M
    if m.sum() < 2000:
        m = d > 300.0
    return (float(tree[m].mean()) if m.sum() >= 500 else float(tree.mean())), d


def prepare(seed, rec, natural_dir):
    """Everything about a tile that no dial can change: geometry and octaves.

    Split out so the calibrator and the generator run ONE code path -- the
    branch's standing rule that the two sides of a comparison must not drift --
    and so a 42-cell search is a reweighting rather than 42 rasterisations.
    """
    grid, (ox, oy, cell) = cgrid.read_u8(natural_dir / f"m_{seed}.canopy.cgrid")
    tree = grid == CLASS_TREE
    n = grid.shape[0]
    P = np.ones((n, n), float)
    holes = rec.get("holes", [])
    per, zall = [], np.zeros((n, n), bool)
    for i, h in enumerate(holes):
        keep, d, side, frac = G.hole_keep(h, (n, n), model)
        P = np.minimum(P, keep)
        zm = G.corridor_zone_masks(h["spine"], (n, n))
        zall |= zm["tee"] | zm["landing"] | zm["green"]
        per.append((i, zm, side))
    unt, _d = untouched_rate(tree, [h["spine"] for h in holes])
    octs = cg.octave_fields(int(seed) ^ model.TAG_CLEAR, n)
    return dict(seed=seed, grid=grid, tree=tree, n=n, P=P, per=per, zall=zall,
                unt=unt, octs=octs, origin=(ox, oy, cell),
                pars=[int(h["par"]) for h in holes])


def keep_by_rank(field, pool, prob):
    """Keep each cell with exactly its own probability, in clumps.

    The textbook move is a Gaussian copula -- threshold `field - ndtri(p)` at a
    constant -- and it is wrong twice over here. It keeps a cell with
    probability Phi(thr + ndtri(p)), which equals p only if the field is exactly
    standard normal, and the 320 m and 640 m octaves have about nine and two
    independent samples across a 3 km tile, so its deviation cannot be trusted.
    Measured, it bent the profile up near the line (0.064 against 0.042) and
    down at the corridor edge (0.348 against 0.424). Solving in probability
    bands fixes that but shreds the clumping, because each band then picks its
    own survivors: patches per hole went 2.54 to 3.18 and one-sided holes 39 %
    to 23 %.

    Ranking the field inside the pool gives an exactly uniform variable that is
    still spatially correlated, because rank is monotone. Comparing it to the
    cell's own probability is then exact at every level AND clumped, with no
    distribution assumed anywhere.
    """
    v = field[pool]
    n = len(v)
    u = (np.argsort(np.argsort(v, kind="stable"), kind="stable") + 0.5) / n
    keep = np.zeros(pool.shape, bool)
    keep[pool] = u < prob[pool]
    return keep


def scaled_prob(P, pool, mult):
    """A hole's own share of the profile: scale it, and do not renormalise.

    Renormalising each hole so its POOL MEAN equals the draw is the tempting
    move and it is wrong, because the acceptance reads the profile level by
    level, not pooled. A hole drawn high lifts its near-line cells a long way in
    relative terms while its outer cells have no room, a hole drawn low cannot
    push the near-line cells below zero, and averaged over holes that left the
    landing line at 0.083 where the profile said 0.041 and the corridor edge at
    0.343 where it said 0.425.

    Multiplying the odds instead has no cap, but the odds map is concave in the
    multiplier, so Jensen pulls every level down: it cost 0.15 at the corridor
    edge and broke the retention rows too.

    A plain multiplier with mean one is unbiased at every level, which is the
    thing being gated. Its only cost is at the top, where `p * mult` would
    exceed one, and that is why the draw's tail is bounded upstream rather than
    truncated here.
    """
    out = np.zeros(P.shape)
    out[pool] = np.clip(P[pool] * mult, 0.0, 1.0)
    return out


def solve(ctx, params, ret):
    """Which trees fall, given the dials. Everything else is already decided."""
    n, tree, P = ctx["n"], ctx["tree"], ctx["P"]
    base = cg.zscore(sum((L ** params["H_CLEAR"]) * f
                         for L, f in zip(cmod.OCTAVES_M, ctx["octs"])))
    cleared = np.zeros((n, n), bool)
    kept = []
    pars = ctx["pars"]
    for i, zm, side in ctx["per"]:
        S = base
        if params["SIDE_SIGMA"] > 0:
            b = float(np.random.default_rng(
                [int(ctx["seed"]), int(i), model.TAG_SIDE]
            ).normal(0.0, params["SIDE_SIGMA"]))
            S = base + b * side          # side is +-1: one flank thins harder
        hole_pool, hole_drawn = np.zeros((n, n), bool), 0.0
        for z in ZONES:
            m = zm[z]
            pool = m & tree
            if not m.any() or not pool.any():
                continue
            # The per-hole draw is a MULTIPLIER on the profile's own anchor, not
            # an absolute target. Used absolutely it loses a fifth of itself to
            # clipping: the measured distribution has a long tail above 1.0 --
            # real corridors that carried more tree than the ground beside them
            # -- and our canopy is homogeneous, so a corridor has about 1.0x
            # what surrounds it and those draws are unreachable.
            r = ret[(ctx["seed"], i, z)]
            mult = r / model.mean_retention(pars[i], z)
            keep = keep_by_rank(S, pool, scaled_prob(P, pool, mult))
            cleared |= pool & ~keep
            hole_pool |= pool
            hole_drawn = max(hole_drawn, r)
        if hole_drawn > 0.02 and hole_pool.any() and not (hole_pool & ~cleared).any():
            # drawn with some retention: at least one tree stands somewhere in
            # the corridor. Small pools (a tee zone is ~25 cells) otherwise
            # round a positive draw away to nothing on 3.7 % of holes
            v = np.where(hole_pool, S, np.inf)
            cleared[np.unravel_index(int(np.argmin(v)), v.shape)] = False
            kept.append((i, z, r, int(keep.sum()), int(m.sum())))
    # everything the profile reaches that the instrument's +-35 m stamp does
    # not: the 35-80 m flanks and the caps past the green and behind the tee.
    # No hole owns these, so they take the profile straight, with no draw.
    rest = tree & (P < 1.0 - 1e-9) & ~ctx["zall"]
    if rest.any():
        cleared |= rest & ~keep_by_rank(base, rest, P)
    out = ctx["grid"].copy()
    out[cleared & tree] = CLASS_OTHER
    return out, cleared, kept


def clear_tile(seed, rec, natural_dir, params, ret):
    ctx = prepare(seed, rec, natural_dir)
    out, cleared, kept = solve(ctx, params, ret)
    return out, ctx["grid"], cleared, kept, ctx["origin"]


def tcc_for(cleared_grid, natural_grid, cover_key):
    """Canopy percent on the cleared mask, at the NATURAL tile's density scale.

    Re-solving the scale would let the interior of an untouched wood thicken to
    make up for a thinned corridor edge, which is backwards: felling a fairway
    does not grow canopy half a kilometre away. So the scale is frozen off the
    natural mask and only the edge ramp is recomputed, which lets new corridor
    edges fade exactly as natural stand edges do. The tile's density then falls
    a little, and that is the right direction -- wooded COURSE tiles measure
    0.611 against 0.677 on comparable natural land.
    """
    dens = cmod.DENSITY[cover_key]
    r_edge = cmod.PARAMS[cover_key]["r_edge"]
    nat = natural_grid == CLASS_TREE
    if not nat.any():
        return np.zeros(natural_grid.shape, np.uint8)
    f_nat = np.clip(ndi.distance_transform_edt(nat, sampling=G.CELL) / r_edge, 0, 1)
    k = dens * nat.mean() * 100.0 / max(f_nat.mean(), 1e-12)
    cl = cleared_grid == CLASS_TREE
    if not cl.any():
        return np.zeros(natural_grid.shape, np.uint8)
    f = np.clip(ndi.distance_transform_edt(cl, sampling=G.CELL) / r_edge, 0, 1)
    return np.clip(np.rint(k * f), 0, 100).astype(np.uint8)


def run(dump, natural_dir, out_dir, routes="out/route_rs/rs.jsonl", only=None):
    recs = read_routes(routes)
    nat_rows = {r["seed"]: r for r in json.loads(
        (natural_dir / "index_canopy.json").read_text())["rows"]}
    params = dict(model.PARAMS)
    # only holes on tiles that are wooded enough to be in the acceptance
    # cohort take a bare slot: a hole on bare prairie has nothing to clear and
    # nothing to draw, and counting it here emptied the atom for everyone
    # else -- bare corridors fell to 7 % and retention doubled
    open_ground = set()
    for s, r in recs.items():
        if nat_rows[s]["tree_frac"] < 0.25:
            continue
        g, _ = cgrid.read_u8(natural_dir / f"m_{s}.canopy.cgrid")
        tree = g == CLASS_TREE
        for i, h in enumerate(r.get("holes", [])):
            zm = G.corridor_zone_masks(h["spine"], tree.shape)
            if not (tree & (zm["tee"] | zm["landing"] | zm["green"])).any():
                open_ground.add((s, i))
    treed = frozenset(s for s in recs if nat_rows[s]["tree_frac"] >= 0.25)
    ret = retention_draw(recs, params, frozenset(open_ground), treed)
    out_dir.mkdir(parents=True, exist_ok=True)
    seeds = sorted(recs) if not only else [s for s in sorted(recs) if s in set(only)]
    rows = []
    for s in seeds:
        g, nat, cleared, kc, (ox, oy, cell) = clear_tile(s, recs[s], natural_dir, params, ret)
        cover = nat_rows[s]["cover"]
        tcc = tcc_for(g, nat, cover)
        cgrid.write_u8(out_dir / f"m_{s}.canopy.cgrid", g, ox, oy, cell)
        cgrid.write_u8(out_dir / f"m_{s}.tcc.cgrid", tcc, ox, oy, cell)
        rows.append(dict(seed=int(s), mode=recs[s]["mode"], cover=cover,
                         nat_tree=float((nat == CLASS_TREE).mean()),
                         cut_tree=float((g == CLASS_TREE).mean()),
                         removed=int(cleared.sum()), tcc_mean=float(tcc.mean()),
                         zones=[list(x) for x in kc]))
    (out_dir / "index_clear.json").write_text(json.dumps(
        dict(params=params, natural=str(natural_dir), rows=rows), indent=1))
    return rows


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("dump")
    ap.add_argument("natural")
    ap.add_argument("out")
    ap.add_argument("--routes", default="out/route_rs/rs.jsonl")
    ap.add_argument("--seeds")
    a = ap.parse_args()
    only = [int(x) for x in a.seeds.split(",")] if a.seeds else None
    t0 = time.time()
    rows = run(pathlib.Path(a.dump), pathlib.Path(a.natural), pathlib.Path(a.out),
               a.routes, only)
    print(f"{len(rows)} tiles in {time.time() - t0:.0f} s")
    for mode in ("aeolian", "fluvial"):
        rs = [r for r in rows if r["mode"] == mode]
        if not rs:
            continue
        print(f"  {mode:8s} {len(rs):3d} seeds   treed {100 * np.mean([r['nat_tree'] for r in rs]):5.2f} % "
              f"-> {100 * np.mean([r['cut_tree'] for r in rs]):5.2f} %   "
              f"cells removed/tile {np.mean([r['removed'] for r in rs]):6.0f}")


if __name__ == "__main__":
    main()
