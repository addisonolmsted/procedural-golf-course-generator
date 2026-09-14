"""Route a batch of dumped tiles with the Python prototype and record what it did.

  python3 tools/golf/batch_route.py <dump_dir> <out.jsonl> [--prefix m_] [--seeds a,b,c] [--stride k --offset j]

Reads <dump_dir>/index_*.txt (seed mode lake_frac [water]) for the mode of each
seed, routes each tile (siting -> greens -> routing, the routing_sheet driver),
and appends one JSON line per seed: routed, pars, total_length_m, total_walk_m,
n_crossings, clubhouse, per-hole green/tees/lzs/spine, score terms, seconds.
The reference the Rust port is accepted against (statistical parity, 2026-09-14).
"""
import sys, json, time, pathlib, glob
import numpy as np
HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parents[0] / "macro_campaign"))
from macro_campaign import cgrid                      # noqa: E402
import siting, greens as greens_mod, routing         # noqa: E402


def route_one(dump, prefix, seed, mode):
    dims = (1450.0, 950.0) if mode == "aeolian" else (1000.0, 700.0)
    z2, (_, _, c2) = cgrid.read_f32(str(dump / f"{prefix}{seed}.cgrid"))
    z2 = np.where(np.isfinite(z2), z2, np.nanmean(z2)).astype(np.float64)
    wp = dump / f"{prefix}{seed}.water.cgrid"
    wet2 = (~np.isnan(cgrid.read_f32(str(wp))[0]) if wp.exists() else np.zeros(z2.shape, bool))
    t0 = time.time()
    sit, f, m, p = siting.run_siting(z2, c2, wet2, *dims)
    pool = greens_mod.generate(z2, c2, wet2, sit, f, m, p)

    def pool_for(ch, _z=z2, _c=c2, _w=wet2, _sit=sit, _f=f, _m=m, _p=p):
        _sit.clubhouse = ch
        return greens_mod.generate(_z, _c, _w, _sit, _f, _m, _p)
    route = routing.run_routing(z2, c2, wet2, sit, f, pool, pool_fn=pool_for)
    dt = time.time() - t0
    rec = {"seed": seed, "mode": mode, "seconds": round(dt, 2), "routed": route is not None,
           "window_m": [float(v) for v in sit.window_m], "pool": len(pool)}
    if route is None:
        return rec
    rec.update(pars=list(route.par_sequence), total_length_m=round(route.total_length_m, 1),
               total_walk_m=round(route.total_walk_m, 1), n_crossings=len(route.crossings),
               clubhouse=[float(v) for v in route.clubhouse_yx], score=round(route.score, 3),
               terms={k: round(float(v), 4) for k, v in route.terms.items()},
               holes=[{"par": h.par, "green": [float(v) for v in h.green_yx], "length_m": round(h.length_m, 1),
                       "tees": [[float(v) for v in b.yx] for b in h.tee_boxes],
                       "lzs": [[float(v) for v in l] for l in h.lzs],
                       "spine": [[float(v) for v in pt] for pt in h.spine],
                       "walk_m": round(h.walk_from_prev.length_m, 1), "bridges": len(h.bridges)}
                      for h in route.holes])
    return rec


def main():
    a = sys.argv[1:]
    prefix, only, stride, offset = "m_", None, 1, 0
    if "--prefix" in a: i = a.index("--prefix"); prefix = a[i + 1]; a = a[:i] + a[i + 2:]
    if "--seeds" in a: i = a.index("--seeds"); only = a[i + 1].split(","); a = a[:i] + a[i + 2:]
    if "--stride" in a: i = a.index("--stride"); stride = int(a[i + 1]); a = a[:i] + a[i + 2:]
    if "--offset" in a: i = a.index("--offset"); offset = int(a[i + 1]); a = a[:i] + a[i + 2:]
    dump, outp = pathlib.Path(a[0]), pathlib.Path(a[1])
    modes = {}
    for f in glob.glob(str(dump / "index_*.txt")):
        for l in open(f):
            p = l.split()
            if len(p) >= 2: modes[p[0]] = p[1]
    seeds = only or sorted(modes, key=int)
    seeds = [s for k, s in enumerate(seeds) if k % stride == offset]
    with open(outp, "a") as out:
        for s in seeds:
            try:
                rec = route_one(dump, prefix, s, modes[s])
            except Exception as e:                       # noqa: BLE001
                rec = {"seed": s, "mode": modes.get(s), "routed": False, "error": repr(e)}
            out.write(json.dumps(rec) + "\n"); out.flush()
            print(f"{s} {modes.get(s)} routed={rec.get('routed')} pars={rec.get('pars')} len={rec.get('total_length_m')} x={rec.get('n_crossings')} {rec.get('seconds')}s", flush=True)


if __name__ == "__main__":
    main()
