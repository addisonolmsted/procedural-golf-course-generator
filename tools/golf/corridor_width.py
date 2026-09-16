"""How wide a real golf course clears its trees.

  python3 tools/golf/corridor_width.py [--min-treed 25] [--limit 200]

The natural tiles say where trees grow. This says what a course does to them:
canopy against perpendicular distance from the hole's centre line, on the 651
course tiles, using the hole geometry `corpus/shot_profiles.py::joined` already
assembles and the stations `tools/golf/hole_metrics.py` already defines.

Only wooded courses can answer the question. A Nebraska course on bare prairie
has no corridor because there was nothing to clear, so tiles under
`--min-treed` percent tree cover are excluded and counted separately rather
than averaged in as zero-width corridors.

Two rasters, two meanings, both reported: WorldCover at 10 m says whether the
ground is treed, NLCD at 30 m says what fraction is under canopy. The corridor
edge is sharper in the first and the density profile is truer in the second.

Writes `out/corridor_width.json`.
"""
import argparse
import json
import pathlib
import sys

import numpy as np

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                                   # noqa: E402
from corpus.shot_profiles import joined                            # noqa: E402
from hole_metrics import arc_cum, at_arc                           # noqa: E402

OUT = HERE / "corpus" / "out" / "corridor_width.json"
CLASS_TREE = 10
OFF_MAX_M = 160.0          # far enough out to reach untouched woods
OFF_STEP_M = 5.0
ARC_STEP_M = 10.0
SEGMENTS = (("tee", 0.0, 0.25), ("landing", 0.25, 0.7), ("green", 0.7, 1.0))


def _sample(grid, cell, ys, xs):
    yi = np.clip((np.asarray(ys) / cell).astype(int), 0, grid.shape[0] - 1)
    xi = np.clip((np.asarray(xs) / cell).astype(int), 0, grid.shape[1] - 1)
    return grid[yi, xi]


def profile_for_hole(pts, tree, cell_t, tcc, cell_c):
    """Tree rate and canopy percent at each offset, and per segment."""
    p = np.asarray(pts, float)
    cum = arc_cum(p)
    L = float(cum[-1])
    if L < 100.0:
        return None
    offs = np.arange(-OFF_MAX_M, OFF_MAX_M + OFF_STEP_M, OFF_STEP_M)
    ss = np.arange(0.0, L + 1e-9, ARC_STEP_M)
    q = at_arc(p, cum, ss)
    # local direction at each station, and its normal
    fwd = at_arc(p, cum, np.clip(ss + 10.0, 0, L)) - at_arc(p, cum, np.clip(ss - 10.0, 0, L))
    nrm = np.stack([-fwd[:, 1], fwd[:, 0]], axis=-1)
    ln = np.hypot(nrm[:, 0], nrm[:, 1])
    nrm = nrm / np.maximum(ln, 1e-9)[:, None]
    ys = q[:, 0][:, None] + nrm[:, 0][:, None] * offs[None, :]
    xs = q[:, 1][:, None] + nrm[:, 1][:, None] * offs[None, :]
    t = (_sample(tree, cell_t, ys, xs) == CLASS_TREE).astype(float)
    c = _sample(tcc, cell_c, ys, xs).astype(float) if tcc is not None else None
    out = {"L": L, "all_tree": t.mean(axis=0).tolist()}
    if c is not None:
        out["all_tcc"] = c.mean(axis=0).tolist()
    frac = ss / max(L, 1.0)
    for name, a, b in SEGMENTS:
        m = (frac >= a) & (frac < b) if b < 1.0 else (frac >= a)
        if m.sum():
            out[f"{name}_tree"] = t[m].mean(axis=0).tolist()
    return out


def half_width(offsets, rate, far_frac=0.5):
    """Where the tree rate first reaches `far_frac` of its far-field value.

    The far field is the mean beyond 120 m, which is untouched woodland. The
    crossing is read outward from the centre line on each side and averaged,
    so an asymmetric corridor still yields one number.
    """
    r = np.asarray(rate, float)
    o = np.asarray(offsets, float)
    far = r[np.abs(o) >= 120.0]
    if far.size == 0 or far.mean() < 0.15:
        return None, None
    thr = far_frac * far.mean()
    widths = []
    for sign in (1, -1):
        m = np.argsort(sign * o)
        oo, rr = o[m], r[m]
        sel = (sign * oo) >= 0
        oo, rr = oo[sel], rr[sel]
        hit = np.where(rr >= thr)[0]
        widths.append(abs(oo[hit[0]]) if hit.size else np.nan)
    w = np.nanmean(widths)
    return (float(w) if np.isfinite(w) else None), float(far.mean())


def tile_paths():
    """course_id -> the elevation cgrid, honouring tiles shared between courses."""
    from corpus import registry
    out = {}
    for rec in registry.all_records():
        t = rec.get("tile") or {}
        if t.get("path"):
            out[rec["course_id"]] = pathlib.Path(t["path"])
    return out


def run(min_treed=25.0, limit=None):
    paths = tile_paths()
    rows, skipped = [], {"bare": 0, "no_canopy": 0, "no_tile": 0, "short": 0}
    cache = {}
    for w in joined():
        cid = w["course_id"]
        base = paths.get(cid)
        if base is None:
            skipped["no_tile"] += 1
            continue
        if cid not in cache:
            wc = base.with_suffix(".canopy.cgrid")
            if not wc.exists():
                cache[cid] = None
            else:
                tree, (_, _, cell_t) = cgrid.read_u8(wc)
                if 100.0 * float((tree == CLASS_TREE).mean()) < min_treed:
                    cache[cid] = "bare"
                else:
                    tc = base.with_suffix(".tcc.cgrid")
                    tcc, cell_c = (None, None)
                    if tc.exists():
                        tcc, (_, _, cell_c) = cgrid.read_u8(tc)
                    cache[cid] = (tree, float(cell_t), tcc,
                                  float(cell_c) if cell_c else None)
        ent = cache[cid]
        if ent is None:
            skipped["no_canopy"] += 1
            continue
        if ent == "bare":
            skipped["bare"] += 1
            continue
        tree, cell_t, tcc, cell_c = ent
        pr = profile_for_hole(w["pts_loc"], tree, cell_t, tcc, cell_c)
        if pr is None:
            skipped["short"] += 1
            continue
        pr.update(way_id=w["way_id"], course_id=cid, region=w["region"], par=w["par"])
        rows.append(pr)
        if limit and len(rows) >= limit:
            break
    return rows, skipped


def summarize(rows):
    offs = np.arange(-OFF_MAX_M, OFF_MAX_M + OFF_STEP_M, OFF_STEP_M)
    print(f"{len(rows)} holes on wooded courses\n")
    if not rows:
        return {}
    allt = np.array([r["all_tree"] for r in rows]).mean(axis=0)
    w, far = half_width(offs, allt)
    print("tree rate against distance from the centre line, all holes:")
    for o, v in zip(offs, allt):
        if abs(o) % 20 == 0 and abs(o) <= 140:
            print(f"   {o:+6.0f} m  {100 * v:5.1f} %  {'#' * int(60 * v)}")
    print(f"\ncorridor half-width, tree rate at half its far-field value: "
          f"{w:.0f} m" if w else "\ncorridor half-width: undetermined")
    print(f"far-field tree rate beyond 120 m: {100 * far:.0f} %" if far else "")
    out = {"n_holes": len(rows), "offsets": offs.tolist(),
           "all_tree": allt.tolist(), "half_width_m": w, "far_field": far}
    for name, _a, _b in SEGMENTS:
        k = f"{name}_tree"
        have = [r[k] for r in rows if k in r]
        if not have:
            continue
        m = np.array(have).mean(axis=0)
        ww, _ = half_width(offs, m)
        out[k] = m.tolist()
        out[f"{name}_half_width_m"] = ww
        print(f"   {name:8s} half-width {ww:.0f} m" if ww else f"   {name:8s} undetermined")
    if "all_tcc" in rows[0]:
        c = np.array([r["all_tcc"] for r in rows if "all_tcc" in r]).mean(axis=0)
        out["all_tcc"] = c.tolist()
        print(f"\ncanopy percent on the centre line {c[len(c) // 2]:.0f} %, "
              f"beyond 120 m {c[np.abs(offs) >= 120].mean():.0f} %")
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--min-treed", type=float, default=25.0)
    ap.add_argument("--limit", type=int)
    a = ap.parse_args()
    rows, skipped = run(a.min_treed, a.limit)
    print(f"skipped: {skipped}")
    summary = summarize(rows)
    OUT.write_text(json.dumps(dict(summary=summary, min_treed=a.min_treed,
                                   skipped=skipped), indent=1))
    print(f"\nwrote {OUT}")


if __name__ == "__main__":
    main()
