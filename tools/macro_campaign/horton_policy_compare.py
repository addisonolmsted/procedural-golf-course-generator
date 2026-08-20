"""M0d: measure BOTH Horton policies on the SAME corpus tiles.

Policy A = tools/metrics/metrics/core.py::horton_ratios
           threshold 6e4 m2, NATIVE cell (2 m), N_w = stream HEADS per order,
           Rb/Rl by log-linear REGRESSION over orders.
           -> this is the one wired into the 52-key ED feature vector.

Policy B = tools/macro_campaign/horton_real.py
           threshold 1.2e5 m2, downsampled to 8 m, reaches = maximal
           constant-order runs, Rb/Rl = MEDIAN of successive order-pair ratios.
           -> this is the one the S2 acceptance band (1.7-4.5) came from.

Four differences at once: threshold, resolution, counting unit, fitting rule.
"""
import sys, pathlib, time, json
import numpy as np

ROOT = pathlib.Path("tools/macro_campaign").resolve()
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT.parent / "metrics"))
from macro_campaign import cgrid, extract_v2, flow          # noqa
from metrics import core as mcore                            # noqa
sys.path.insert(0, str(ROOT))
import importlib.util
spec = importlib.util.spec_from_file_location("hreal", ROOT / "horton_real.py")
hreal = importlib.util.module_from_spec(spec); spec.loader.exec_module(hreal)

BIOMES = ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]
NTILE = int(sys.argv[1]) if len(sys.argv) > 1 else 8

def policy_b(z8):
    zf = mcore.fill_depressions(z8, 8.0)
    rec, _ = flow.receivers(zf, 8.0)
    acc = flow.accumulate(rec).astype(float) * 64.0
    n = z8.size
    rec = rec.ravel()
    is_ch = acc.ravel() >= 1.2e5
    order = hreal.strahler(rec, is_ch, n)
    rs = hreal.reaches(rec, is_ch, order, n)
    rb, rl = hreal.ratios(rs)
    return rb, rl, int(order.max()) if order.size else 0

rows = {}
for b in BIOMES:
    tids = [t for a, t in extract_v2.kept_tiles() if a == b][:NTILE]
    A_rb, A_rl, A_om, B_rb, B_rl, B_om = [], [], [], [], [], []
    for tid in tids:
        p = extract_v2.OUT / "tiles" / b / f"{tid}.cgrid"
        if not p.exists():
            continue
        z, (_, _, cell) = cgrid.read_f32(p)
        z = z.astype(np.float64)
        m = np.isfinite(z)
        if m.mean() < 0.95:
            continue
        zfill = np.where(m, z, np.nanmean(z[m]))
        # ---- Policy A: native cell, metrics battery
        t0 = time.time()
        ha = mcore.horton_ratios(zfill, cell)
        ta = time.time() - t0
        if np.isfinite(ha["horton_bifurcation_ratio"]):
            A_rb.append(ha["horton_bifurcation_ratio"])
        if np.isfinite(ha["horton_length_ratio"]):
            A_rl.append(ha["horton_length_ratio"])
        if np.isfinite(ha["strahler_max"]):
            A_om.append(ha["strahler_max"])
        # ---- Policy B: 8 m, horton_real
        rb, rl, om = policy_b(zfill[::4, ::4])
        if rb: B_rb.append(rb)
        if rl: B_rl.append(rl)
        B_om.append(om)
    q = lambda v, p: float(np.percentile(v, p)) if v else float("nan")
    rows[b] = dict(n=len(A_rb), cell=cell,
                   A_rb=q(A_rb,50), A_rb10=q(A_rb,10), A_rb90=q(A_rb,90),
                   A_rl=q(A_rl,50), A_om=q(A_om,50),
                   B_rb=q(B_rb,50), B_rb10=q(B_rb,10), B_rb90=q(B_rb,90),
                   B_rl=q(B_rl,50), B_om=q(B_om,50))
    r = rows[b]
    print(f"{b:14s} n={r['n']:2d} | A: Rb {r['A_rb']:5.2f} [{r['A_rb10']:.2f}-{r['A_rb90']:.2f}] "
          f"Rl {r['A_rl']:5.2f} W {r['A_om']:.0f} | B: Rb {r['B_rb']:5.2f} "
          f"[{r['B_rb10']:.2f}-{r['B_rb90']:.2f}] Rl {r['B_rl']:5.2f} W {r['B_om']:.0f}", flush=True)

allA_rb = [r["A_rb"] for r in rows.values() if np.isfinite(r["A_rb"])]
allB_rb = [r["B_rb"] for r in rows.values() if np.isfinite(r["B_rb"])]
allA_rl = [r["A_rl"] for r in rows.values() if np.isfinite(r["A_rl"])]
allB_rl = [r["B_rl"] for r in rows.values() if np.isfinite(r["B_rl"])]
print()
print(f"POLICY A across biomes: Rb {min(allA_rb):.2f}-{max(allA_rb):.2f}  Rl {min(allA_rl):.2f}-{max(allA_rl):.2f}")
print(f"POLICY B across biomes: Rb {min(allB_rb):.2f}-{max(allB_rb):.2f}  Rl {min(allB_rl):.2f}-{max(allB_rl):.2f}")
json.dump(rows, open(pathlib.Path(sys.argv[0]).parent / "horton_policy.json", "w"), indent=1)
