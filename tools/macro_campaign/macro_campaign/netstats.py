"""Network statistics — the branching structure of the drainage.

The shape pass already traces a real branching network and then throws the
topology away: `valleypipe.extract_valleys` returns `(results, net)` and
`shape.py` binds `net` to `_`, while each `BranchResult.branch` carries
`parent`, `junction_frac` and `mouth_area_m2`. Everything here is a
RECORDING change, not a new estimator — the numbers were already computed.

These drive network-driven generation: how often channels branch, at what
angle, how far apart, how much drainage area a tributary brings relative to
its parent, and how reach geometry scales with area.

The two power laws are fitted PIVOT-CENTRED, `ln y = c0 + b1*ln(A/A0)`,
with `A0` a fixed corpus-scale pivot. That matters: with an uncentred fit
the intercept and exponent are strongly correlated, so a sampler drawing
them independently from separate quantile tables would produce absurd
combinations (a high intercept with a high exponent). Centring on a pivot
inside the data range makes them near-orthogonal, which is what lets the
pair ride in the existing flat `<section>.<knob>` prior schema instead of
needing a new block type.
"""

import numpy as np

# Pivot for the area power laws, km^2. Chosen near the corpus median reach
# drainage area so c0 reads as "the value at a typical reach" and is
# near-independent of b1. Changing it changes the meaning of the fitted
# intercept knob — it is a contract, not a tuning parameter.
AREA_PIVOT_KM2 = 0.5
# `_wls_loglog`-style fits need enough reaches to be meaningful.
MIN_FIT_ROWS = 8


def _fit_loglog(area_km2, y) -> tuple[float, float, int]:
    """Least-squares `ln y = c0 + b1*ln(A/A0)`; returns (c0, b1, n_used).

    c0 is the natural log of y AT THE PIVOT, so `exp(c0)` is directly
    interpretable and comparable across tiles.
    """
    a = np.asarray(area_km2, dtype=float)
    v = np.asarray(y, dtype=float)
    ok = np.isfinite(a) & np.isfinite(v) & (a > 0) & (v > 0)
    n = int(ok.sum())
    if n < MIN_FIT_ROWS:
        return float("nan"), float("nan"), n
    x = np.log(a[ok] / AREA_PIVOT_KM2)
    ly = np.log(v[ok])
    b1, c0 = np.polyfit(x, ly, 1)
    return float(c0), float(b1), n


def _strahler(parents: list) -> list[int]:
    """Strahler order from a parent-indexed forest (children have higher
    indices than their parent, so one reverse pass suffices)."""
    n = len(parents)
    kids: list[list[int]] = [[] for _ in range(n)]
    for i, p in enumerate(parents):
        if p is not None and 0 <= p < n:
            kids[p].append(i)
    order = [1] * n
    for i in range(n - 1, -1, -1):
        if not kids[i]:
            order[i] = 1
            continue
        child = sorted((order[c] for c in kids[i]), reverse=True)
        order[i] = child[0] + 1 if len(child) > 1 and child[0] == child[1] else child[0]
    return order


def _resolve_parents(results, net) -> list:
    """Map `branch.parent` (an index into the ORIGINAL network) onto indices
    into `results`.

    `extract_valleys` drops branches whose fit fails, so the two lists are
    not aligned — a 13-result tile was observed carrying parent indices 15
    and 19. Taking `branch.parent` at face value therefore links the wrong
    reaches together (and can index out of range). Where a parent was
    dropped, walk up the original chain to the nearest surviving ancestor so
    the topology stays connected rather than silently fragmenting.
    """
    if net is None:
        return [None] * len(results)
    orig = {id(b): j for j, b in enumerate(net.branches)}
    keep = {}
    for i, r in enumerate(results):
        j = orig.get(id(r.branch))
        if j is not None:
            keep[j] = i
    out = []
    for r in results:
        p = getattr(r.branch, "parent", None)
        guard = 0
        while p is not None and p not in keep and guard <= len(net.branches):
            p = getattr(net.branches[p], "parent", None) if 0 <= p < len(net.branches) else None
            guard += 1
        out.append(keep.get(p) if p is not None else None)
    return out


def network_stats(results, cell: float, net=None) -> tuple[dict, dict]:
    """(knob-ish scalars, extras) from one tile's `BranchResult` list.

    `results[i].branch` carries the topology; `crossfits` carry the
    cross-section fits already matched to reaches. `net` is the `Network`
    the results came from — required to translate parent indices (see
    `_resolve_parents`).
    """
    import sys, pathlib
    sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent.parent / "dtm_primitives"))
    from dtm_primitives.extract import _junction_angle
    from dtm_primitives.transects import arc_length

    cls = [np.asarray(r.centerline, dtype=float) for r in results]
    parents = _resolve_parents(results, net)
    order = _strahler(parents)

    angles, spacings, lengths, logratios = [], [], [], []
    hw_rows, fall_rows = [], []

    # junction spacing: consecutive mouths along the same parent
    by_parent: dict[int, list[float]] = {}
    for i, r in enumerate(results):
        p = parents[i]
        cl = cls[i]
        if len(cl) >= 2:
            lengths.append(float(arc_length(cl)))
        area = float(getattr(r.branch, "mouth_area_m2", float("nan"))) / 1e6
        if p is not None and 0 <= p < len(results):
            if len(cl) >= 3 and len(cls[p]) >= 3:
                a = _junction_angle(cl, cls[p])
                if np.isfinite(a):
                    angles.append(float(a))
            jf = float(getattr(r.branch, "junction_frac", float("nan")))
            if np.isfinite(jf) and len(cls[p]) >= 2:
                by_parent.setdefault(p, []).append(jf * arc_length(cls[p]))
            pa = float(getattr(results[p].branch, "mouth_area_m2", float("nan"))) / 1e6
            if np.isfinite(area) and np.isfinite(pa) and area > 0 and pa > 0:
                logratios.append(float(np.log10(area / pa)))
        # area -> geometry rows for the power laws
        if np.isfinite(area) and area > 0:
            for cf in r.crossfits:
                if getattr(cf, "ok", False) and np.isfinite(cf.hw) and cf.hw > 0:
                    hw_rows.append((area, float(cf.hw)))
            lf = getattr(r, "longfit", None)
            if lf is not None and np.isfinite(lf.fall_raw) and lf.fall_raw > 0:
                fall_rows.append((area, float(lf.fall_raw)))
    for _p, stations in by_parent.items():
        st = sorted(stations)
        spacings += [b - a for a, b in zip(st[:-1], st[1:]) if b > a]

    hw_c0, hw_b1, hw_n = _fit_loglog([a for a, _ in hw_rows], [v for _, v in hw_rows])
    # Slope-area: S = k * A^-theta, so the fitted b1 IS -theta.
    s_c0, s_b1, s_n = _fit_loglog([a for a, _ in fall_rows], [v for _, v in fall_rows])

    med = lambda v: float(np.median(v)) if v else float("nan")  # noqa: E731
    knobs = {
        "junction_angle_deg": med(angles),
        "junction_spacing_m": med(spacings),
        "branch_len_m": med(lengths),
        "junction_area_logratio": med(logratios),
        "chan_hw_at_a0_m": float(np.exp(hw_c0)) if np.isfinite(hw_c0) else float("nan"),
        "chan_hw_area_exp": hw_b1,
        "slope_area_theta": -s_b1 if np.isfinite(s_b1) else float("nan"),
    }
    extras = {
        "net_n_branches": float(len(results)),
        "net_n_junctions": float(sum(1 for p in parents if p is not None)),
        "net_strahler_max": float(max(order)) if order else float("nan"),
        "net_depth_max": float(_max_depth(parents)),
        "net_hw_fit_rows": float(hw_n),
        "net_fall_fit_rows": float(s_n),
        "net_fall_at_a0": float(np.exp(s_c0)) if np.isfinite(s_c0) else float("nan"),
    }
    return knobs, extras


def _max_depth(parents: list) -> int:
    best = 0
    for i in range(len(parents)):
        d, p, guard = 0, parents[i], 0
        while p is not None and 0 <= p < len(parents) and guard < len(parents):
            d += 1
            p = parents[p]
            guard += 1
        best = max(best, d)
    return best
