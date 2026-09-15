"""BLINDNESS on real golf holes: what can you not see from where you stand.

Reuses the join in `shot_profiles.joined` (cached golf=hole ways -> keeper
course -> 2 m 3DEP tile -> polyline in tile-local metres, oriented tee ->
green).  For every par 3/4/5 at least 100 m long it walks the centreline and
asks, from an eye 1.7 m above the ground at the tee (and again from where
the last full shot is played), which ground ahead is hidden behind the
terrain in between.

Sight lines are straight in PLAN -- the line a player actually looks along,
not the bent centreline -- sampled every 2 m against the RAW DEM, and a
target counts as blind when any intermediate ground rises more than
BLIND_TOL_M above the observer-eye-to-target line.

Caveats: 3DEP is a bare-earth DEM, so trees, hedges, buildings and grandstands
are absent -- these numbers are terrain blindness only, a lower bound on what
a player cannot see.  Real tees and greens are graded pads that survive in the
DEM (a built-up tee is a real 1-2 m of extra eye height, and a raised green
is real relief), whereas a generated hole placed on bare synthetic ground has
neither; so a generator matching these numbers on unpadded terrain will, once
it builds its pads, come out slightly LESS blind than the corpus.

Run from the repo root:  python3 tools/golf/corpus/blindness.py
Writes:                  tools/golf/corpus/out/blindness.json
"""

from __future__ import annotations

import json
import pathlib
import sys
import time

import numpy as np

_GOLF = pathlib.Path(__file__).resolve().parents[1]
if str(_GOLF) not in sys.path:
    sys.path.insert(0, str(_GOLF))
from corpus import config  # noqa: E402
from corpus.shot_profiles import joined  # noqa: E402

EYE_M = 1.7           # eye height above ground at the observer
BLIND_TOL_M = 0.3     # ground must beat the sight line by this to blind it
LOS_STEP_M = 2.0      # sampling along the plan sight line
SAMPLE_M = 5.0        # centreline target spacing
MIN_LEN_M = 100.0
DRIVE_M = 220.0
DRIVE_FRAC = 0.63
APPROACH_RESERVE_M = 60.0


# --------------------------------------------------------------------------
# geometry helpers
# --------------------------------------------------------------------------
def arc_cum(pts: np.ndarray) -> np.ndarray:
    seg = np.diff(pts, axis=0)
    return np.r_[0.0, np.cumsum(np.hypot(seg[:, 0], seg[:, 1]))]


def at_arc(pts: np.ndarray, cum: np.ndarray, s):
    """(y, x) on the polyline at arc position(s) s -- the same np.interp
    resampling shot_profiles.smoothed_profile uses for its z(s)."""
    y = np.interp(s, cum, pts[:, 0])
    x = np.interp(s, cum, pts[:, 1])
    return np.stack([y, x], axis=-1)


def z_at(z2: np.ndarray, cell2: float, p: np.ndarray) -> np.ndarray:
    """Ground z by nearest cell of the RAW grid (clipped to the tile)."""
    yi = np.clip(np.rint(p[..., 0] / cell2).astype(int), 0, z2.shape[0] - 1)
    xi = np.clip(np.rint(p[..., 1] / cell2).astype(int), 0, z2.shape[1] - 1)
    return z2[yi, xi]


def blocked(z2, cell2, p_obs, z_obs, p_tgt, z_tgt) -> np.ndarray:
    """Is each target hidden from the observer?  Straight plan line between
    them, sampled every LOS_STEP_M, ground vs. the eye-to-target line.

    p_tgt may be (M, 2); returns a bool array of shape (M,)."""
    p_tgt = np.atleast_2d(p_tgt)
    d = np.hypot(p_tgt[:, 0] - p_obs[0], p_tgt[:, 1] - p_obs[1])
    # each line gets its OWN step count, so the spacing is LOS_STEP_M on
    # every line rather than only on the longest one
    ni = np.maximum(np.ceil(d / LOS_STEP_M), 1.0)      # (M,) steps per line
    k = np.arange(int(ni.max()) + 1)[None, :]          # (1, K)
    t = k / ni[:, None]                                # (M, K), 1.0 = target
    inside = (k >= 1) & (t < 1.0 - 1e-12)              # strictly between
    t = np.clip(t, 0.0, 1.0)
    ys = p_obs[0] + (p_tgt[:, 0:1] - p_obs[0]) * t
    xs = p_obs[1] + (p_tgt[:, 1:2] - p_obs[1]) * t
    g = z_at(z2, cell2, np.stack([ys, xs], axis=-1))   # (M, K) ground
    eye = z_obs + EYE_M
    sight = eye + (z_tgt[:, None] - eye) * t           # (M, K) sight line
    over = np.where(inside, g - sight, -np.inf)
    return (over > BLIND_TOL_M).any(axis=1)


def sample_arcs(s0: float, L: float) -> np.ndarray:
    """Arc positions every SAMPLE_M over the half-open interval (s0, L]."""
    s = np.arange(s0 + SAMPLE_M, L + 1e-9, SAMPLE_M)
    if len(s) == 0 or s[-1] < L - 1e-9:
        s = np.r_[s, L]
    return s


# --------------------------------------------------------------------------
# one hole
# --------------------------------------------------------------------------
def measure(j: dict) -> dict:
    pts, z2, cell2 = j["pts_loc"], j["z2"], j["cell2"]
    L = j["length_m"]
    par = j["par"]
    cum = arc_cum(pts)
    # the resampled polyline runs 0..cum[-1]; ss from smoothed_profile shares
    # that arc, so scale in case of float drift
    cum = cum * (L / cum[-1]) if cum[-1] > 0 else cum

    s_drive = min(DRIVE_M, DRIVE_FRAC * L)
    s_second = min(s_drive + DRIVE_M, L - APPROACH_RESERVE_M)
    if par == 3:
        s_last = 0.0
    elif par == 4:
        s_last = s_drive
    else:
        s_last = s_second if s_second > s_drive else s_drive

    p_tee = at_arc(pts, cum, 0.0)
    z_tee = float(z_at(z2, cell2, p_tee))
    p_last = at_arc(pts, cum, s_last)
    z_last = float(z_at(z2, cell2, p_last))

    s_a = sample_arcs(0.0, L)
    p_a = at_arc(pts, cum, s_a)
    z_a = z_at(z2, cell2, p_a).astype(float)
    blind_tee = float(blocked(z2, cell2, p_tee, z_tee, p_a, z_a).mean())

    if par == 3:
        blind_appr = blind_tee
    else:
        s_b = sample_arcs(s_last, L)
        p_b = at_arc(pts, cum, s_b)
        z_b = z_at(z2, cell2, p_b).astype(float)
        blind_appr = float(
            blocked(z2, cell2, p_last, z_last, p_b, z_b).mean())

    lz_seen = None
    if par in (4, 5):
        p_lz = at_arc(pts, cum, s_drive)
        z_lz = z_at(z2, cell2, p_lz).astype(float).reshape(1)
        lz_seen = not bool(
            blocked(z2, cell2, p_tee, z_tee, p_lz, z_lz)[0])

    p_g = at_arc(pts, cum, L)
    z_g = z_at(z2, cell2, p_g).astype(float).reshape(1)
    green_seen = not bool(blocked(z2, cell2, p_last, z_last, p_g, z_g)[0])

    return dict(
        way_id=j["way_id"], course_id=j["course_id"], region=j["region"],
        par=par, length_m=L, blind_tee=blind_tee, blind_appr=blind_appr,
        lz_seen=lz_seen, green_seen=green_seen,
    )


def run() -> list[dict]:
    t0 = time.time()
    rows: list[dict] = []
    stats: dict = {}
    n_skip = 0
    for j in joined(stats):
        stats["n_rows"] = len(rows)
        if j["par"] not in (3, 4, 5) or j["length_m"] < MIN_LEN_M:
            n_skip += 1
            continue
        rows.append(measure(j))
        stats["n_rows"] = len(rows)
    out = config.OUT / "blindness.json"
    out.write_text(json.dumps(rows))
    print(f"  joined {stats['n_join']} ways on {stats['n_tiles']} tiles; "
          f"skipped {n_skip} without par 3/4/5 or < {MIN_LEN_M:.0f} m")
    print(f"  wrote {out} ({len(rows)} holes)  [{time.time() - t0:.0f} s]")
    return rows


# --------------------------------------------------------------------------
# summary
# --------------------------------------------------------------------------
def _p(v, q):
    v = np.asarray(v, float)
    return float(np.percentile(v, q)) if len(v) else float("nan")


def _mean(v):
    v = np.asarray(v, float)
    return float(v.mean()) if len(v) else float("nan")


def _share_false(v):
    v = [x for x in v if x is not None]
    return 100.0 * sum(not x for x in v) / len(v) if v else float("nan")


def _band(label: str, vals: list[float]) -> None:
    print(f"    {label:8s} n={len(vals):5d}  p25 {_p(vals, 25):.3f}  "
          f"p50 {_p(vals, 50):.3f}  p75 {_p(vals, 75):.3f}  "
          f"p90 {_p(vals, 90):.3f}  mean {_mean(vals):.3f}")


def summarize(rows: list[dict]) -> None:
    by = {p: [r for r in rows if r["par"] == p] for p in (3, 4, 5)}
    print("\n=== blindness: summary ===")
    print(f"  eye {EYE_M} m, tolerance {BLIND_TOL_M} m, sight step "
          f"{LOS_STEP_M} m, targets every {SAMPLE_M} m")
    print(f"  n holes {len(rows)}  " + "  ".join(
        f"(par {p}: {len(by[p])})" for p in (3, 4, 5)))
    print("  blind_tee  = share of centreline hidden from the tee:")
    for p in (3, 4, 5):
        _band(f"par {p}", [r["blind_tee"] for r in by[p]])
    _band("all", [r["blind_tee"] for r in rows])
    print("  blind_appr = share of the last leg hidden from the last station:")
    for p in (3, 4, 5):
        _band(f"par {p}", [r["blind_appr"] for r in by[p]])
    _band("all", [r["blind_appr"] for r in rows])
    print("  blind landing zone (lz_seen false), per par:")
    for p in (4, 5):
        v = [r["lz_seen"] for r in by[p]]
        print(f"    par {p}    n={len(v):5d}  {_share_false(v):5.1f} %")
    v = [r["lz_seen"] for r in rows if r["par"] in (4, 5)]
    print(f"    par 4+5  n={len(v):5d}  {_share_false(v):5.1f} %")
    print("  blind green (green_seen false), per par:")
    for p in (3, 4, 5):
        v = [r["green_seen"] for r in by[p]]
        print(f"    par {p}    n={len(v):5d}  {_share_false(v):5.1f} %")
    print(f"    all      n={len(rows):5d}  "
          f"{_share_false([r['green_seen'] for r in rows]):5.1f} %")
    print("  by region:")
    for reg in sorted({r["region"] for r in rows}):
        rr = [r for r in rows if r["region"] == reg]
        print(f"    {reg:14s} n={len(rr):5d}  blind_tee p50 "
              f"{_p([r['blind_tee'] for r in rr], 50):.3f}  blind_appr p50 "
              f"{_p([r['blind_appr'] for r in rr], 50):.3f}  lz blind "
              f"{_share_false([r['lz_seen'] for r in rr]):5.1f} %  "
              f"green blind "
              f"{_share_false([r['green_seen'] for r in rr]):5.1f} %")
    ref = config.OUT / "hole_profiles.json"
    if ref.exists():
        mac = {r["way_id"]: r["max_above_chord"]
               for r in json.loads(ref.read_text())}
        pair = [(r["blind_tee"], mac[r["way_id"]])
                for r in rows if r["way_id"] in mac]
        if len(pair) > 2:
            a = np.array(pair, float)
            c = float(np.corrcoef(a[:, 0], a[:, 1])[0, 1])
            rk = float(np.corrcoef(
                np.argsort(np.argsort(a[:, 0])).astype(float),
                np.argsort(np.argsort(a[:, 1])).astype(float))[0, 1])
            print(f"  sanity: corr(blind_tee, max_above_chord) over "
                  f"{len(pair)} shared ways: pearson {c:+.3f}, "
                  f"spearman {rk:+.3f}")
            print(f"          max_above_chord p50 "
                  f"{_p(a[:, 1], 50):.2f} m  p90 {_p(a[:, 1], 90):.2f} m")


if __name__ == "__main__":
    summarize(run())
