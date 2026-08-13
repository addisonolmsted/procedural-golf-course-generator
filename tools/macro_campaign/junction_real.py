"""What junction angles does the REAL corpus give under our measurement?

The S2 gate allows at most 3% of confluences to exceed 80 degrees (a
"T-junction"). That band came from the authored engine, which enforced a
30-62 degree mouth angle by construction. A DERIVED network's angles are
whatever the flow field gives, and on a D8 grid every step is a multiple
of 45 degrees, so the measured angle is quantised — especially over the
2-cell tangent baseline the battery uses.

This measures real corpus tiles with the SAME extraction, the same reach
definition, and the same 2-cell tangent baseline, so the two numbers mean
the same thing. It also reports a longer (6-cell) baseline, which is what
a viewer's eye actually integrates.

    python tools/macro_campaign/junction_real.py [threshold_m2]
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


def angles_for(rec, is_ch, acc, nx, ny, baseline):
    """Confluence angles: for each cell with >=2 channel donors, the angle
    between the SMALLER donor's incoming direction and the continuation of
    the larger one — the battery's child-vs-parent convention."""
    n = nx * ny
    donors = [[] for _ in range(n)]
    for i in np.nonzero(is_ch)[0]:
        r = rec[i]
        if r >= 0 and is_ch[r]:
            donors[r].append(int(i))

    def upstream_dir(cell, k):
        """direction from cell back upstream k steps (unit)."""
        cur, path = cell, [cell]
        for _ in range(k):
            ds = [d for d in donors[cur] if is_ch[d]]
            if not ds:
                break
            cur = max(ds, key=lambda d: acc[d])
            path.append(cur)
        a, b = path[0], path[-1]
        dy = (b // nx) - (a // nx)
        dx = (b % nx) - (a % nx)
        norm = np.hypot(dx, dy)
        return (dx / norm, dy / norm) if norm > 0 else None

    def downstream_dir(cell, k):
        cur, path = cell, [cell]
        for _ in range(k):
            r = rec[cur]
            if r < 0 or not is_ch[r]:
                break
            cur = int(r)
            path.append(cur)
        a, b = path[0], path[-1]
        dy = (b // nx) - (a // nx)
        dx = (b % nx) - (a % nx)
        norm = np.hypot(dx, dy)
        return (dx / norm, dy / norm) if norm > 0 else None

    out = []
    for j in range(n):
        ds = donors[j]
        if len(ds) < 2:
            continue
        ds = sorted(ds, key=lambda d: acc[d], reverse=True)
        main, trib = ds[0], ds[1]
        # parent's downstream direction at the junction, reversed = "upstream"
        dd = downstream_dir(j, baseline)
        # tributary's own incoming direction
        td = upstream_dir(trib, baseline)
        if dd is None or td is None:
            continue
        # angle between the tributary's upstream direction and the
        # parent's upstream direction (both point away from the junction)
        dot = td[0] * -dd[0] + td[1] * -dd[1]
        out.append(np.degrees(np.arccos(np.clip(dot, -1, 1))))
    return out


def main():
    print(f"REAL corpus junction angles at threshold {THRESH:.1e} m2")
    print(f"  {'biome':14s} {'n':>5s} | {'2-cell baseline':>22s} | {'6-cell baseline':>22s}")
    print(f"  {'':14s} {'':5s} | {'p50':>6s} {'>80deg':>7s} {'>=88':>6s} | {'p50':>6s} {'>80deg':>7s} {'>=88':>6s}")
    for b in ["piedmont", "great_plains", "river_valley", "hill_country", "heathland", "sandhills"]:
        a2, a6 = [], []
        for tid in [t for a, t in extract_v2.kept_tiles() if a == b][:8]:
            z, (_, _, cell) = cgrid.read_f32(extract_v2.OUT / "tiles" / b / f"{tid}.cgrid")
            z = z.astype(np.float64)
            m = np.isfinite(z)
            if m.mean() < 0.95:
                continue
            z8 = np.where(m, z, np.nanmean(z[m]))[::4, ::4]
            zf = mcore.fill_depressions(z8, CELL)
            rec, _ = flow.receivers(zf, CELL)
            acc = flow.accumulate(rec).astype(float) * CELL * CELL
            ny, nx = z8.shape
            rec = rec.ravel()
            accf = acc.ravel()
            is_ch = accf >= THRESH
            a2 += angles_for(rec, is_ch, accf, nx, ny, 2)
            a6 += angles_for(rec, is_ch, accf, nx, ny, 6)
        if a2:
            f = lambda v, t: 100.0 * np.mean(np.array(v) > t)
            print(f"  {b:14s} {len(a2):5d} | {np.median(a2):6.0f} {f(a2,80):6.0f}% {f(a2,87.9):5.0f}% |"
                  f" {np.median(a6):6.0f} {f(a6,80):6.0f}% {f(a6,87.9):5.0f}%")


if __name__ == "__main__":
    main()
