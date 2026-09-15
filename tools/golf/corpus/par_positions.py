"""Real par-by-hole positions for par-36 nines, from the cached OSM hole ways.

  python3 tools/golf/corpus/par_positions.py            # writes out/par_positions.json

Source: every `golf=hole` way in the Overpass cache (`out/fetch_cache/overpass/`)
carrying both a numeric `ref` (hole number) and a one-digit `par` tag, with
geometry. Holes are clustered into courses greedily by proximity (within
1.5 km on both axes of a seed hole); a cluster contributes a FRONT nine when
refs 1-9 are each present exactly once, a BACK nine when refs 10-18 are, and a
nine counts only when its pars are all 3/4/5 and sum to 36 (the router's
par-36 mixes). Measured 2026-09-15: 412 par-36 nines (220 front, 192 back).

Output (`out/par_positions.json`): for `all`, `front`, `back`:
  n, share[9][3] (par 3/4/5 share per hole slot, 0-based), mixes (n3,n4,n5 ->
  share), b2b3 / b2b5 (share of nines with consecutive 3s / 5s),
  open_pairs / close_pairs (hole 1-2 and 8-9 par pairs -> share).
`route.rs::PAR_POS` is transcribed from `all.share`.
"""
import collections, glob, json, math, pathlib, re

HERE = pathlib.Path(__file__).resolve().parent
OUT = HERE / "out"
CLUSTER_M = 1500.0


def load_holes():
    els = {}
    for p in glob.glob(str(OUT / "fetch_cache" / "overpass" / "*.json")):
        try:
            d = json.load(open(p))
        except Exception:
            continue
        for e in d.get("elements", []):
            t = e.get("tags", {})
            if t.get("golf") != "hole":
                continue
            m = re.match(r"^\s*(\d+)\s*$", t.get("ref", ""))
            par = t.get("par", "").strip()
            g = e.get("geometry") or ([e["center"]] if "center" in e else None)
            if not m or not re.match(r"^\d$", par) or not g:
                continue
            lat = sum(q["lat"] for q in g) / len(g)
            lon = sum(q["lon"] for q in g) / len(g)
            els[e["id"]] = (lat, lon, int(m.group(1)), int(par))
    return list(els.values())


def cluster(items):
    used = [False] * len(items)
    out = []
    for i, (la, lo, _, _) in enumerate(items):
        if used[i]:
            continue
        cl = [i]
        used[i] = True
        for j in range(i + 1, len(items)):
            if used[j]:
                continue
            lb, ob = items[j][:2]
            if abs(la - lb) * 111e3 < CLUSTER_M and abs(lo - ob) * 111e3 * math.cos(math.radians(la)) < CLUSTER_M:
                cl.append(j)
                used[j] = True
        out.append(cl)
    return out


def nines(items, clusters):
    res = []
    for cl in clusters:
        refs = collections.defaultdict(list)
        for j in cl:
            refs[items[j][2]].append(items[j][3])
        for lo in (1, 10):
            if all(len(refs[i]) == 1 for i in range(lo, lo + 9)):
                hs = [refs[i][0] for i in range(lo, lo + 9)]
                if sum(hs) == 36 and all(p in (3, 4, 5) for p in hs):
                    res.append(("front" if lo == 1 else "back", hs))
    return res


def table(N):
    n = len(N)
    share = [[sum(h[i] == par for h in N) / n for par in (3, 4, 5)] for i in range(9)]
    mixes = collections.Counter((h.count(3), h.count(4), h.count(5)) for h in N)
    return dict(
        n=n, share=share,
        mixes={f"{k[0]},{k[1]},{k[2]}": v / n for k, v in mixes.most_common()},
        b2b3=sum(any(a == b == 3 for a, b in zip(h, h[1:])) for h in N) / n,
        b2b5=sum(any(a == b == 5 for a, b in zip(h, h[1:])) for h in N) / n,
        open_pairs={f"{k[0]}{k[1]}": v / n for k, v in collections.Counter((h[0], h[1]) for h in N).most_common()},
        close_pairs={f"{k[0]}{k[1]}": v / n for k, v in collections.Counter((h[7], h[8]) for h in N).most_common()},
    )


def main():
    items = load_holes()
    N = nines(items, cluster(items))
    groups = {"all": [h for _, h in N], "front": [h for k, h in N if k == "front"], "back": [h for k, h in N if k == "back"]}
    out = {k: table(v) for k, v in groups.items() if v}
    out["_meta"] = dict(holes_with_ref_par=len(items), cluster_m=CLUSTER_M, measured="2026-09-15")
    (OUT / "par_positions.json").write_text(json.dumps(out, indent=1))
    for k in ("all", "front", "back"):
        t = out[k]
        print(f"{k}: {t['n']} par-36 nines")
        for j, par in enumerate((3, 4, 5)):
            print(f"  par {par} by hole:", " ".join(f"{100 * t['share'][i][j]:4.0f}" for i in range(9)))
        print(f"  mixes {dict(list(t['mixes'].items())[:3])}  b2b3 {100 * t['b2b3']:.0f} %  b2b5 {100 * t['b2b5']:.0f} %")
    print("wrote", OUT / "par_positions.json")


if __name__ == "__main__":
    main()
