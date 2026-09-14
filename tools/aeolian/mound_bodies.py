"""Sand-body coverage on aeolian tiles: are the dunes a field, or a few
lumps on a flat sheet?

  python3 tools/aeolian/mound_bodies.py <dir> [--prefix m_] [--seeds a,b,c] [--form Mound|Train]
  python3 tools/aeolian/mound_bodies.py --real            # the 54 real Nebraska tiles, split by orientation order

Per tile, on the 100 m belt (gaussian) above a 1.2 km rolling floor:
  cover        share of the tile where the belt stands above 30 % of its p95 relief
  bodies_km2   8-connected bodies per km^2
  floor_std    std of the belt minus its 1.2 km floor over the ground BETWEEN bodies, m
  edge_slope   median belt slope on the bodies' margins, %
  sparse       the style the owner called out: cover < 0.40 and bodies_km2 > 1.2
Water is masked; the river corridor (500 m either side of the creek line) too.

Reference (2026-09-14, tools/macro_campaign/out/tiles/sandhills, split at
orientation proxy A < 0.5 = mound-like, n = 39):
  cover p10/50/90 0.39 / 0.54 / 0.87   bodies/km2 0.22 / 0.56 / 1.13
  floor std m     1.3 / 2.5 / 4.8       edge slope % 4.5 / 6.3 / 8.1
  sparse-style share 2/39
Ours before the mound round (final250, Mound dry, n = 50): cover 0.30/0.45/0.63,
bodies 0.32/1.00/1.90, floor std 0.9/1.3/1.9, edge slope 2.8/3.9/4.9, sparse 15/50.
"""
import sys, glob, pathlib, re, subprocess
import numpy as np
from scipy import ndimage as ndi
HERE = pathlib.Path(__file__).resolve().parent
ROOT = HERE.parents[1]
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
from macro_campaign import cgrid                      # noqa: E402

REAL = ROOT / "tools/macro_campaign/out/tiles/sandhills"


def orient_a(z, c):
    n = min(z.shape); z = z[:n, :n].astype(float); z = z - ndi.gaussian_filter(z, 1600 / c / 2.355)
    z = z * np.hanning(n)[:, None] * np.hanning(n)[None, :]
    P = np.abs(np.fft.fftshift(np.fft.fft2(z))) ** 2
    fr = np.fft.fftshift(np.fft.fftfreq(n, c)); fy, fx = np.meshgrid(fr, fr, indexing="ij"); f = np.hypot(fx, fy)
    band = (f > 1 / 1600) & (f < 1 / 400); th = np.arctan2(fy, fx)[band]; p = P[band]
    return float(abs((p * np.exp(2j * th)).sum()) / p.sum())


def measure(z, c, mask=None):
    z = z.astype(float)
    if mask is not None:
        z = np.where(mask, z, np.nan); z = np.where(np.isfinite(z), z, np.nanmedian(z))
    belt = ndi.gaussian_filter(z, 100 / c / 2.355)
    floor = ndi.gaussian_filter(ndi.minimum_filter(belt, int(1200 / c)), 300 / c)
    h = belt - floor; p95 = float(np.percentile(h, 95))
    body = h > 0.30 * p95
    gy, gx = np.gradient(belt, c); sl = np.hypot(gx, gy) * 100
    edge = ndi.binary_dilation(body, iterations=3) & ~ndi.binary_erosion(body, iterations=3)
    _, nb = ndi.label(body, structure=np.ones((3, 3)))
    km2 = (z.shape[0] * c / 1000) * (z.shape[1] * c / 1000)
    r = dict(cover=float(body.mean()), bodies_km2=nb / km2,
             # lowland relief at the belt scale, detrended: the belt minus its 1.2 km floor
             floor_std=float(h[~body].std()) if (~body).any() else 0.0,
             edge_slope=float(np.median(sl[edge])) if edge.any() else 0.0, p95=p95)
    r["sparse"] = r["cover"] < 0.40 and r["bodies_km2"] > 1.2
    return r


def forms(seeds):
    exe = ROOT / "target/release/examples/desc_dump"
    if not exe.exists():
        exe = ROOT / "target/debug/examples/desc_dump"
    out = subprocess.run([str(exe)] + list(seeds), capture_output=True, text=True).stdout
    return {m.group(1): m.group(2) for m in re.finditer(r"(\d+) .*?form: (\w+)", out)}


def report(name, rows):
    if not rows:
        print(name, "n=0"); return
    q = lambda k: np.percentile([r[k] for r in rows], [10, 50, 90]).round(2).tolist()
    print(f"{name:34s} n={len(rows):3d} cover {q('cover')} bodies/km2 {q('bodies_km2')} floor std {q('floor_std')} "
          f"edge slope {q('edge_slope')} sparse {sum(r['sparse'] for r in rows)}/{len(rows)}")


def main():
    a = sys.argv[1:]
    if "--real" in a:
        rows = {"mound-like": [], "train-like": []}
        for p in sorted(REAL.glob("*.cgrid")):
            z, (_, _, c) = cgrid.read_f32(str(p)); A = orient_a(z, c)
            rows["mound-like" if A < 0.5 else "train-like"].append(measure(z, c))
        for k, v in rows.items(): report(f"real NE {k}", v)
        return
    prefix, only, form = "m_", None, None
    if "--prefix" in a: i = a.index("--prefix"); prefix = a[i + 1]; a = a[:i] + a[i + 2:]
    if "--seeds" in a: i = a.index("--seeds"); only = set(a[i + 1].split(",")); a = a[:i] + a[i + 2:]
    if "--form" in a: i = a.index("--form"); form = a[i + 1]; a = a[:i] + a[i + 2:]
    d = pathlib.Path(a[0])
    idx = {}
    for f in d.glob("index_*.txt"):
        for l in open(f):
            p = l.split(); idx[p[0]] = (p[1], p[3] if len(p) > 3 else "river")
    seeds = sorted(s for s in idx if idx[s][0] == "aeolian" and (only is None or s in only))
    fm = forms(seeds) if form else {}
    rows = {}
    for s in seeds:
        if form and fm.get(s) != form: continue
        z, (_, _, c) = cgrid.read_f32(str(d / f"{prefix}{s}.cgrid")); w, _ = cgrid.read_f32(str(d / f"{prefix}{s}.water.cgrid"))
        mask = ~np.isfinite(w)
        lp = d / f"{prefix}{s}.creek.txt"
        if lp.exists() and lp.stat().st_size:
            line = np.loadtxt(lp); on = np.zeros(z.shape, bool)
            ii = np.clip((line[:, 1] / c).astype(int), 0, z.shape[0] - 1); jj = np.clip((line[:, 0] / c).astype(int), 0, z.shape[1] - 1); on[ii, jj] = True
            mask &= ndi.distance_transform_edt(~on) * c > 500
        r = measure(z, c, mask); rows[s] = r
        print(f"  {s} {fm.get(s, '')} cover {r['cover']:.2f} bodies/km2 {r['bodies_km2']:.2f} floor std {r['floor_std']:.1f} edge {r['edge_slope']:.1f}{'  SPARSE' if r['sparse'] else ''}")
    report(f"{d.name} {form or 'aeolian'}", list(rows.values()))


if __name__ == "__main__":
    main()
