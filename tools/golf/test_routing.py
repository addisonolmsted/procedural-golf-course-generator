"""Synthetic gates for the routing prototype. Run: python3 test_routing.py"""
import sys

import numpy as np

sys.path.insert(0, ".")
import routing  # noqa: E402
import siting   # noqa: E402
import greens as G  # noqa: E402


def make_tile(creek: bool = False):
    """Gentle rolling tile at 2 m: 1 m sinusoid + plane, optional wet strip."""
    yy, xx = np.mgrid[0:1504, 0:1504].astype(float) * 2.0
    z = (0.9 * np.sin(xx / 210.0) + 0.9 * np.sin(yy / 260.0)
         + 0.002 * xx + 4.0 * np.sin(xx / 900.0) * np.sin(yy / 800.0))
    wet = np.zeros(z.shape, bool)
    if creek:
        wet[:, (xx[0] > 1490) & (xx[0] < 1510)] = True
    return z, 2.0, wet


def route_for(creek: bool):
    z2, cell2, wet2 = make_tile(creek)
    sit, f, m, p = siting.run_siting(z2, cell2, wet2, 1000.0, 700.0)
    pool = G.generate(z2, cell2, wet2, sit, f, m, p, n_target=110)
    assert pool, "empty pool on a gentle tile"
    r = routing.run_routing(z2, cell2, wet2, sit, f, pool)
    return r, sit, f, pool, wet2, cell2


def test_gentle():
    r, sit, f, pool, wet2, cell2 = route_for(False)
    assert r is not None, "no route on a gentle tile"
    assert len(r.holes) == 9
    assert sum(r.par_sequence) == 36
    n3 = r.par_sequence.count(3)
    n4 = r.par_sequence.count(4)
    n5 = r.par_sequence.count(5)
    assert (n3, n4, n5) in routing.ALLOWED_MIXES, (n3, n4, n5)
    # returning nine
    t1 = np.asarray(r.holes[0].tee_boxes[0].yx)
    g9 = np.asarray(r.holes[8].green_yx)
    home = np.asarray(r.clubhouse_yx)
    assert np.hypot(*(t1 - home)) <= siting.CLUBHOUSE_RADIUS_M + 1e-6
    assert np.hypot(*(g9 - home)) <= siting.CLUBHOUSE_RADIUS_M + 1e-6
    # five boxes, non-increasing length back->forward
    for h in r.holes:
        assert len(h.tee_boxes) == 5
        lens = [b.length_m for b in h.tee_boxes]
        assert all(lens[i] >= lens[i + 1] - 1.0 for i in range(4)), lens
    # LZ ground within the fair gate
    for h in r.holes:
        for (y, x, _r) in h.lzs:
            yi, xi = int(y / f.cell), int(x / f.cell)
            assert f.slope[yi, xi] <= 0.10, f.slope[yi, xi]
    # no walk x play crossings
    walk_cross = 0
    for i in range(9):
        for j in range(9):
            if j in (i, i - 1):
                continue
            if routing.polyline_crossings(r.holes[i].walk_from_prev.path,
                                          r.holes[j].spine):
                walk_cross += 1
    assert walk_cross == 0, walk_cross
    assert 2200 <= r.total_length_m <= 3400, r.total_length_m
    print(f"  gentle: pars {r.par_sequence} total {r.total_length_m:.0f} m "
          f"walk {r.total_walk_m:.0f} m crossings {len(r.crossings)} "
          f"score {r.score:.2f}")
    return r


def test_creek():
    r, sit, f, pool, wet2, cell2 = route_for(True)
    assert r is not None, "no route on the creek tile (penalties must saturate)"
    strip_x = (2980.0, 3020.0)
    n_bridges = 0
    for h in r.holes:
        for b in h.bridges + h.walk_from_prev.bridges:
            n_bridges += 1
            # span >= strip width always; oblique crossings run longer
            # (an 80 m bridge over a 20 m strip is a 14-degree crossing)
            assert 15.0 <= b.span_m <= 150.0, b
    # every spine leg that spans the strip must carry a bridge
    missing = 0
    for h in r.holes:
        pts = h.spine
        for i in range(len(pts) - 1):
            x0, x1 = sorted((pts[i][1], pts[i + 1][1]))
            if x0 < strip_x[0] and x1 > strip_x[1]:
                if not any(b.kind == "spine" for b in h.bridges):
                    missing += 1
    assert missing == 0, f"{missing} crossings without bridges"
    print(f"  creek: pars {r.par_sequence} bridges {n_bridges} "
          f"score {r.score:.2f}")


def test_determinism():
    r1, *_ = route_for(False)
    r2, *_ = route_for(False)
    assert repr(r1) == repr(r2)
    print("  determinism: identical")


def test_kernels():
    si = routing.seg_intersect
    assert si((0, 0), (2, 2), (0, 2), (2, 0))
    assert not si((0, 0), (1, 1), (2, 2), (3, 3))       # collinear disjoint
    assert not si((0, 0), (2, 2), (2, 2), (4, 0))       # shared endpoint
    wet = np.zeros((100, 100), bool)
    wet[:, 40:50] = True
    br = routing.wet_spans(wet, 1.0, (50.0, 0.0), (50.0, 99.0), "spine", 0)
    assert len(br) == 1 and 8 <= br[0].span_m <= 12, br
    # par-mix prune vs brute force
    from itertools import product
    legal = set()
    for seq in product((3, 4, 5), repeat=9):
        c = (seq.count(3), seq.count(4), seq.count(5))
        if c in routing.ALLOWED_MIXES:
            legal.add(seq)
    for seq in product((3, 4, 5), repeat=4):
        counts = [0, 0, 0]
        ok = True
        for h, p in enumerate(seq):
            if p not in routing.legal_pars(tuple(counts), h):
                ok = False
                break
            counts[{3: 0, 4: 1, 5: 2}[p]] += 1
        brute = any(s[:4] == seq for s in legal)
        assert ok == brute, (seq, ok, brute)
    print("  kernels: PASS")


if __name__ == "__main__":
    test_kernels()
    test_gentle()
    test_creek()
    test_determinism()
    print("ALL PASS")
