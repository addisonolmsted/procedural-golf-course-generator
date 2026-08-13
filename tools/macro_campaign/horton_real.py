"""What Horton ratios does the REAL corpus give under our extraction rules?

The S2 acceptance band (Rb 3-5, Rl 1.5-3) was inherited from the authored
engine, where the hierarchy was constructed top-down and the ratios held
by design. A DERIVED network's ratios are an outcome, and they depend on
the extraction threshold and on how a "reach" is defined — so the band has
to be re-based on the corpus measured the same way, not assumed.

Reaches are maximal constant-order runs between confluences, exactly as
course-skeleton's carve::trace defines them.

    python tools/macro_campaign/horton_real.py [threshold_m2]
"""
import sys
import pathlib

import numpy as np

ROOT = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
sys.path.insert(0, str(ROOT.parent / "metrics"))
from macro_campaign import cgrid, extract_v2, flow  # noqa: E402
from metrics import core as mcore  # noqa: E402

CELL = 8.0
THRESH = float(sys.argv[1]) if len(sys.argv) > 1 else 1.2e5


def strahler(rec, is_ch, n):
    order = np.zeros(n, dtype=np.int32)
    indeg = np.zeros(n, dtype=np.int32)
    for i in np.nonzero(is_ch)[0]:
        r = rec[i]
        if r >= 0 and is_ch[r]:
            indeg[r] += 1
    best = np.zeros(n, dtype=np.int32)
    bcount = np.zeros(n, dtype=np.int32)
    pending = indeg.copy()
    stack = [int(i) for i in np.nonzero(is_ch & (indeg == 0))[0]]
    while stack:
        i = stack.pop()
        o = best[i] + 1 if bcount[i] >= 2 else max(best[i], 1)
        order[i] = o
        r = rec[i]
        if r >= 0 and is_ch[r]:
            if o > best[r]:
                best[r], bcount[r] = o, 1
            elif o == best[r]:
                bcount[r] += 1
            pending[r] -= 1
            if pending[r] == 0:
                stack.append(int(r))
    return order


def reaches(rec, is_ch, order, n):
    """(order, length_m) per maximal reach, split at confluences."""
    donors = np.zeros(n, dtype=np.int32)
    for i in np.nonzero(is_ch)[0]:
        r = rec[i]
        if r >= 0 and is_ch[r]:
            donors[r] += 1
    out = []
    for s in np.nonzero(is_ch & (donors != 1))[0]:
        cur, cells = int(s), 1
        while True:
            r = rec[cur]
            if r < 0 or not is_ch[r]:
                break
            cells += 1
            if donors[r] >= 2:
                break
            cur = int(r)
        if cells >= 2:
            out.append((int(order[s]) or 1, cells * CELL))
    return out


def ratios(rs):
    if not rs:
        return None, None
    mo = max(o for o, _ in rs)
    if mo < 2:
        return None, None
    cnt = {o: 0 for o in range(1, mo + 1)}
    ln = {o: 0.0 for o in range(1, mo + 1)}
    for o, l in rs:
        cnt[o] += 1
        ln[o] += l
    rb, rl = [], []
    for o in range(1, mo):
        if cnt[o + 1] > 0 and cnt[o] > 0:
            rb.append(cnt[o] / cnt[o + 1])
            a, b = ln[o + 1] / cnt[o + 1], ln[o] / cnt[o]
            if b > 0:
                rl.append(a / b)
    med = lambda v: float(np.median(v)) if v else None
    return med(rb), med(rl)


def main():
    print(f"REAL corpus Horton ratios at threshold {THRESH:.1e} m2")
    print(f"  {'biome':14s} {'n':>2s} {'Rb med':>7s} {'Rb p10-p90':>14s} {'Rl med':>7s} {'Omega':>6s}")
    for b in ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]:
        rbs, rls, omegas = [], [], []
        for tid in [t for a, t in extract_v2.kept_tiles() if a == b][:10]:
            z, (_, _, cell) = cgrid.read_f32(extract_v2.OUT / "tiles" / b / f"{tid}.cgrid")
            z = z.astype(np.float64)
            m = np.isfinite(z)
            if m.mean() < 0.95:
                continue
            z8 = np.where(m, z, np.nanmean(z[m]))[::4, ::4]
            zf = mcore.fill_depressions(z8, CELL)
            rec, _ = flow.receivers(zf, CELL)
            acc = flow.accumulate(rec).astype(float) * CELL * CELL
            n = z8.size
            rec = rec.ravel()
            is_ch = (acc.ravel() >= THRESH)
            order = strahler(rec, is_ch, n)
            rs = reaches(rec, is_ch, order, n)
            rb, rl = ratios(rs)
            if rb:
                rbs.append(rb)
            if rl:
                rls.append(rl)
            omegas.append(int(order.max()))
        if rbs:
            rl_lo = np.percentile(rls, 10) if rls else float('nan')
            rl_hi = np.percentile(rls, 90) if rls else float('nan')
            print(f"  {b:14s} {len(rbs):2d} {np.median(rbs):7.2f} "
                  f"{np.percentile(rbs,10):6.2f}-{np.percentile(rbs,90):<7.2f} "
                  f"{(np.median(rls) if rls else float('nan')):7.2f} "
                  f"{rl_lo:5.2f}-{rl_hi:<5.2f} {int(np.median(omegas)):6d}")


if __name__ == "__main__":
    main()
