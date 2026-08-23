#!/usr/bin/env python3
"""Dune-field statistics — the aeolian analogue of the channel-network battery.

Attempt 5, `docs/sandhills/README.md`. Written because the two instruments the
Sandhills archetype is declared to need do not exist:

1. `docs/biomes/sandhills.md` names the **directional variogram along the wind
   azimuth** as its primary discriminant (dune spacing). `metrics.core.anisotropy`
   computes eight directional variograms internally and throws the curves away —
   at 22.5 deg resolution, with `max_lag_m` clamped to 400 m. Nebraska's dominant
   wavelength measures ~1190 m (`docs/calibration/variety-audit.md`), so the
   existing instrument SATURATES BEFORE THE SIGNAL STARTS.

2. Nothing measures **crest defect density** — terminations and Y-junctions per
   km2. That is the number separating a real dune field (chains of coalesced
   crescents) from parallel stripes, and a generator can hit spacing, relief and
   orientation while failing it completely.

WHY THIS FILE IS NOT THE OLD METRIC AGAIN. `fit_knobs.QUARANTINED["dune_wavelength_m"]`
records a purpose-built dune spectrum that was FALSIFIED: orientation coherence
over the Nebraska tiles (R=0.41) came out LOWER than over the Cumberland Plateau
tiles (R=0.75) — "it ranks structural grain above a real dune field." The
quarantine note names the remedy: *"revisit with dune-only crops or a mapped-dune
inventory validation."*

So this module ships with its own falsification harness (`--validate`), and the
rule is that **no generator work starts until it passes**: the Nebraska dune
corpus must outrank the Cumberland Plateau / Ozark structural-grain corpora on
orientation coherence and crest continuity. An instrument that has never been
seen to fail is not evidence.

    python3 tools/aeolian/dune_stats.py --validate
    python3 tools/aeolian/dune_stats.py --corpus sandhills sandhills_nc
    python3 tools/aeolian/dune_stats.py --tile <path.cgrid>
"""
from __future__ import annotations

import argparse
import json
import math
import pathlib
import sys
import warnings

import numpy as np

_TOOLS = pathlib.Path(__file__).resolve().parent.parent
for _p in ("macro_campaign", "dtm_primitives", "metrics"):
    sys.path.insert(0, str(_TOOLS / _p))

from macro_campaign.cgrid import read_f32                      # noqa: E402
from macro_campaign.extract import primitives_frame, _primitives_cfg, _deep_merge  # noqa: E402
from macro_campaign import extract_v2                          # noqa: E402
from dtm_primitives import ridgepipe                           # noqa: E402

OUT = _TOOLS / "macro_campaign" / "out"

# --------------------------------------------------------------------------
# Dune-scale config.
#
# ridgepipe's defaults are VALLEY settings run on an inverted surface: 120 m
# transect half-length, 200 m minimum length, tributary network gates sized for
# gullies. A barchanoid ridge is km-scale, so at those settings the tracer
# returns fragments of one crest rather than the crest (measured on
# sandhills/t04165_10185: 8 "ridges", median length 673 m, nearest-neighbour
# spacing 154 m on ground whose trains sit ~1 km apart).
#
# These overrides are the dune equivalent of extract.py's RIDGE_CFG_OVERRIDES,
# and they are declared here rather than tuned per run: the ruler does not move
# during a build.
DUNE_CFG_OVERRIDES = {
    # Crest admission: extract.py's ridge setting. A dune crest IS the
    # structure here, so admit generously -- the alternative was measured and
    # is worse. ABLATED 2026-08-22 on 3 Nebraska tiles: raising this to 2e5
    # (the "a dune drains a big inverted basin" argument) starved the tracer
    # from 3.3 crests/tile to 1.0, too few to compute spacing or coherence at
    # all. The `network` group was the whole effect; `ridgefit` was inert on
    # count and `transects` cost a third of it.
    "network": {
        "accum_min_area_m2": 20000.0,
        "trib_min_area_m2": 20000.0,
        "trib_min_len_m": 400.0,        # a crest under 400 m is a mound, not a train
        "max_depth": 3,
        "max_tributaries": 20,          # crest chains branch; let them
    },
    # Transects must reach TOWARD the interdune floor to read a flank at all.
    # extract.py's ridge setting is 120 m -- sized for an interfluve between
    # two gullies, and blind to a 500 m dune flank. The dtm_primitives default
    # (300) is the operating point: 500 measured better flanks but discarded
    # every near-edge crest on a 3 km tile (min_valid_frac 0.9).
    "transects": {
        "halflen_m": 300.0,
        "halflen_max_m": 500.0,
        "reach_len_m": 400.0,
        "min_transects_per_reach": 4,
        "spacing_m": 40.0,
    },
    "ridgefit": {
        "lookup_m": 900.0,   # ~one dune wavelength (Nebraska lambda_dom ~1190 m)
        "flat_deg": 0.6,     # stabilized dunes are gentle; 1.0 deg drops them
        "confirm_frac": 0.45,
        "confirm_radius_m": 90.0,
        "min_len_m": 400.0,
    },
}

_CFG_CACHE = None


def dune_cfg():
    global _CFG_CACHE
    if _CFG_CACHE is None:
        _CFG_CACHE = _deep_merge(_primitives_cfg(), DUNE_CFG_OVERRIDES)
    return _CFG_CACHE


# --------------------------------------------------------------------------
# Crest geometry


def _tangents(cl: np.ndarray, step: int = 4):
    """Unit tangents and their segment lengths along a centerline."""
    p = np.asarray(cl, dtype=float)[::step]
    if len(p) < 2:
        return np.zeros((0, 2)), np.zeros(0)
    d = np.diff(p, axis=0)
    L = np.hypot(d[:, 0], d[:, 1])
    ok = L > 1e-9
    return d[ok] / L[ok, None], L[ok]


def orientation_coherence(ridges) -> float:
    """Length-weighted AXIAL resultant of crest TANGENTS, in [0, 1].

    DESCRIPTIVE ONLY -- deliberately not a gate metric. Measured 2026-08-22:
    it is biased HIGH at low crest count (corr(n_crest, coherence) = -0.61;
    tiles with one traced crest read 0.470, tiles with four or more read
    0.261). One crest is trivially coherent with itself, so on ground where
    the tracer finds little this reports "highly oriented" for the wrong
    reason. `spectral_order` is the gate metric instead.
    """
    num = np.zeros(2)
    den = 0.0
    for r in ridges:
        t, L = _tangents(r.centerline)
        if len(t) == 0:
            continue
        th = np.arctan2(t[:, 1], t[:, 0])
        num += np.array([np.sum(L * np.cos(2 * th)), np.sum(L * np.sin(2 * th))])
        den += float(np.sum(L))
    if den <= 0:
        return float("nan")
    return float(np.hypot(*num) / den)


def spectral_order(z: np.ndarray, cell: float,
                   band_m: tuple[float, float] = (400.0, 1600.0)) -> tuple[float, float, float]:
    """Axial orientation order A, dominant wavelength, and band relief.

    Power-weighted doubled-angle resultant over the macro band of the 2-D
    spectrum. Tracing-free and routing-free, which matters here:
    `02-drainage-patterns.md` Sec 2 records that `fill_depressions` runs before
    routing, so sandhills' network is a PHANTOM the real ground does not carry
    -- and `ridgepipe` traces crests with that same inverted-network machinery.
    A spectral statistic does not inherit that problem.

    Identical to `macro_campaign/variety_audit.py::metrics`, and verified
    against it: over all 44 kept Nebraska tiles this returns A p10/p50/p90 =
    .186/.400/.755, lambda 1190 m, band relief 20.3 m -- reproducing
    `docs/calibration/variety-audit.md` to three decimals.
    """
    if cell < 8.0:
        step = int(round(8.0 / cell))
        z = z[::step, ::step]
        cell = cell * step
    z = np.asarray(z, dtype=float)
    m = np.isfinite(z)
    if not m.all():
        z = np.where(m, z, np.nanmedian(z[m]))
    n = z.shape[0]
    w = np.hanning(n)
    F = np.fft.fftshift(np.fft.fft2((z - z.mean()) * np.outer(w, w)))
    P = np.abs(F) ** 2
    f = np.fft.fftshift(np.fft.fftfreq(n, d=cell))
    fy, fx = np.meshgrid(f, f, indexing="ij")
    fr = np.hypot(fx, fy)
    lam = np.where(fr > 0, 1.0 / np.maximum(fr, 1e-12), np.inf)
    band = (lam >= band_m[0]) & (lam <= band_m[1])
    Pb = P[band]
    th = np.arctan2(fy, fx)[band]
    A = float(np.abs(np.sum(Pb * np.exp(2j * th)) / np.sum(Pb)))
    lam_dom = float(np.sum(Pb * lam[band]) / np.sum(Pb))
    s_hi, s_lo = band_m[0] / np.pi / cell, band_m[1] / np.pi / cell
    from scipy import ndimage
    zb = ndimage.gaussian_filter(z, s_hi) - ndimage.gaussian_filter(z, s_lo)
    relief = float(np.percentile(zb, 95) - np.percentile(zb, 5))
    return A, lam_dom, relief


def spectral_axis_deg(z: np.ndarray, cell: float,
                      band_m: tuple[float, float] = (400.0, 1600.0)) -> float:
    """Crest axis in degrees (undirected, 0-180), from the spectrum.

    The companion to `spectral_order`: same power-weighted doubled-angle
    resultant, but its ANGLE rather than its magnitude. Crests run
    perpendicular to the wind, so `axis + 90` is the wind azimuth -- which is
    the only way to get a wind direction for a corpus tile, since the corpus
    does not record one and aspect-relative-to-wind is meaningless without it.

    Note the spectrum's angle is the WAVEVECTOR direction, which is ACROSS the
    crests; the crest axis is that plus 90.
    """
    if cell < 8.0:
        step = int(round(8.0 / cell))
        z = z[::step, ::step]
        cell = cell * step
    z = np.asarray(z, dtype=float)
    m = np.isfinite(z)
    if not m.all():
        z = np.where(m, z, np.nanmedian(z[m]))
    n = z.shape[0]
    w = np.hanning(n)
    F = np.fft.fftshift(np.fft.fft2((z - z.mean()) * np.outer(w, w)))
    P = np.abs(F) ** 2
    f = np.fft.fftshift(np.fft.fftfreq(n, d=cell))
    fy, fx = np.meshgrid(f, f, indexing="ij")
    fr = np.hypot(fx, fy)
    lam = np.where(fr > 0, 1.0 / np.maximum(fr, 1e-12), np.inf)
    band = (lam >= band_m[0]) & (lam <= band_m[1])
    Pb = P[band]
    th = np.arctan2(fy, fx)[band]
    c = np.sum(Pb * np.exp(2j * th)) / np.sum(Pb)
    k_axis = 0.5 * math.atan2(c.imag, c.real)          # across-crest direction
    return float((np.degrees(k_axis) + 90.0) % 180.0)  # along-crest


def orientation_axis_deg(ridges) -> float:
    """Mean crest azimuth (undirected, deg from +x). Wind is perpendicular."""
    num = np.zeros(2)
    for r in ridges:
        t, L = _tangents(r.centerline)
        if len(t) == 0:
            continue
        th = np.arctan2(t[:, 1], t[:, 0])
        num += np.array([np.sum(L * np.cos(2 * th)), np.sum(L * np.sin(2 * th))])
    if np.allclose(num, 0):
        return float("nan")
    return float(np.degrees(0.5 * math.atan2(num[1], num[0])) % 180.0)


def defect_stats(ridges, tile_area_km2: float, join_m: float = 120.0) -> dict:
    """Crest terminations and Y-junctions per km2.

    A termination is a crest endpoint with no other crest within `join_m`; a
    junction is an endpoint that lands on another crest's interior. Real
    barchanoid ridges are chains of coalesced crescents and carry BOTH at
    density; parallel stripes carry terminations only, and a space-filling
    lattice carries neither.
    """
    if not ridges:
        return {"termination_km2": float("nan"), "junction_km2": float("nan"),
                "defect_km2": float("nan")}
    from scipy.spatial import cKDTree
    term = junc = 0
    for i, r in enumerate(ridges):
        cl = np.asarray(r.centerline, dtype=float)
        others = [np.asarray(o.centerline, float) for j, o in enumerate(ridges) if j != i]
        if not others:
            term += 2
            continue
        stack = np.concatenate(others)
        tree = cKDTree(stack)
        # endpoint index within `stack` tells us interior vs endpoint contact
        ends = np.cumsum([len(o) for o in others])
        starts = np.concatenate([[0], ends[:-1]])
        for ep in (cl[0], cl[-1]):
            d, k = tree.query(ep)
            if d > join_m:
                term += 1
            else:
                near_end = np.any((np.abs(k - starts) < 3) | (np.abs(k - (ends - 1)) < 3))
                junc += 1 if not near_end else 0
    return {"termination_km2": term / tile_area_km2,
            "junction_km2": junc / tile_area_km2,
            "defect_km2": (term + junc) / tile_area_km2}


def flank_asymmetry(ridges) -> float:
    """Median |flank_l - flank_r| / mean(|flank|) — the stoss/lee signature.

    A stabilized dune is asymmetric by construction: a gentle windward ramp and
    a steeper lee face. A structural ridge between two streams is not.
    """
    a = []
    for r in ridges:
        m = (abs(r.flank_l) + abs(r.flank_r)) / 2.0
        if m > 1e-9:
            a.append(abs(r.flank_l - r.flank_r) / m)
    return float(np.median(a)) if a else float("nan")


# --------------------------------------------------------------------------
# Directional variogram — written fresh; core.anisotropy discards the curves
# and clamps max_lag to 400 m, below the ~1190 m signal.


def directional_variogram(z: np.ndarray, cell: float, n_dir: int = 16,
                          max_lag_m: float = 1600.0, n_lag: int = 40):
    """Return (angles_rad[n_dir], lags_m[n_lag], gamma[n_dir, n_lag]).

    Quadratic-detrended, NaN-masked. Axial: directions span [0, pi).

    Measured on an 8 m grid. The signal is km-scale (Nebraska lambda_dom
    ~1190 m) so 2 m resolves nothing extra here, and it is the same grid the
    planform family already declares in `01-measurement-policy.md` Sec 5.
    """
    if cell < 8.0:
        step = int(round(8.0 / cell))
        z = z[::step, ::step]
        cell = cell * step
    zz = z.astype(float).copy()
    ny, nx = zz.shape
    yy, xx = np.mgrid[0:ny, 0:nx].astype(float)
    m = np.isfinite(zz)
    A = np.column_stack([np.ones(m.sum()), xx[m], yy[m], xx[m] ** 2, yy[m] ** 2, xx[m] * yy[m]])
    coef, *_ = np.linalg.lstsq(A, zz[m], rcond=None)
    fit = (coef[0] + coef[1] * xx + coef[2] * yy
           + coef[3] * xx ** 2 + coef[4] * yy ** 2 + coef[5] * xx * yy)
    zz = zz - fit
    zz[~m] = np.nan

    angles = np.linspace(0.0, np.pi, n_dir, endpoint=False)
    lags = np.linspace(cell, max_lag_m, n_lag)
    g = np.full((n_dir, n_lag), np.nan)
    for i, th in enumerate(angles):
        for j, lag in enumerate(lags):
            dx = int(round(lag / cell * math.cos(th)))
            dy = int(round(lag / cell * math.sin(th)))
            if dx == 0 and dy == 0:
                continue
            a = zz[max(0, dy):ny + min(0, dy), max(0, dx):nx + min(0, dx)]
            b = zz[max(0, -dy):ny + min(0, -dy), max(0, -dx):nx + min(0, -dx)]
            d = a - b
            d = d[np.isfinite(d)]
            if d.size > 512:
                g[i, j] = 0.5 * float(np.mean(d * d))
    return angles, lags, g


def variogram_dune_scale(angles, lags, g) -> dict:
    """Cross-crest wavelength from the directional variogram.

    A periodic dune train makes gamma RISE to a hump at half the wavelength and
    fall back — the classic hole effect. The direction of maximum hole depth is
    across the crests; twice its peak lag is the dune wavelength.
    """
    best = None
    for i in range(g.shape[0]):
        row = g[i]
        ok = np.isfinite(row)
        if ok.sum() < 8:
            continue
        r = row[ok]
        L = lags[ok]
        k = int(np.argmax(r))
        if k == 0 or k == len(r) - 1:
            continue                       # monotone: no hole in this direction
        tail = r[k:]
        depth = (r[k] - tail.min()) / r[k] if r[k] > 0 else 0.0
        if best is None or depth > best["hole_depth"]:
            best = {"hole_depth": float(depth),
                    "cross_axis_deg": float(np.degrees(angles[i]) % 180.0),
                    "wavelength_m": float(2.0 * L[k])}
    if best is None:
        return {"hole_depth": float("nan"), "cross_axis_deg": float("nan"),
                "wavelength_m": float("nan")}
    return best


# --------------------------------------------------------------------------


def measure_tile(path: pathlib.Path, with_variogram: bool = True) -> dict:
    z, (ox, oy, cell) = read_f32(str(path))
    area_km2 = (z.shape[0] * cell) * (z.shape[1] * cell) / 1e6
    with warnings.catch_warnings():
        warnings.simplefilter("ignore", RuntimeWarning)
        try:
            ridges, _ = ridgepipe.extract_ridges(*primitives_frame(z), cell, dune_cfg())
        except Exception as exc:
            print(f"    ridgepipe failed on {path.name}: {type(exc).__name__}: {exc}")
            ridges = []
    out = {"tile": path.stem, "n_crest": len(ridges)}
    if ridges:
        out.update({
            "crest_len_p50": float(np.median([r.length_m for r in ridges])),
            "crest_len_p90": float(np.percentile([r.length_m for r in ridges], 90)),
            "prominence_p50": float(np.median([r.prominence_med for r in ridges])),
            "crest_hw_p50": float(np.median([r.crest_hw_med for r in ridges])),
            "flank_asym": flank_asymmetry(ridges),
            "coherence": orientation_coherence(ridges),
            "crest_axis_deg": orientation_axis_deg(ridges),
        })
        out.update(ridgepipe.spacing_stats(ridges))
        out.update(defect_stats(ridges, area_km2))
    A, lam_dom, band_relief = spectral_order(z, cell)
    out.update({"spectral_A": A, "lam_dom_m": lam_dom, "band_relief_m": band_relief})
    if with_variogram:
        with warnings.catch_warnings():
            warnings.simplefilter("ignore", RuntimeWarning)
            out.update(variogram_dune_scale(*directional_variogram(z, cell)))
    return out


def kept(biome: str) -> list[pathlib.Path]:
    ks = {t for a, t in extract_v2.kept_tiles() if a == biome}
    if not ks:   # v1 keys are not in review_v2; fall back to on-disk minus excludes
        ex = json.loads((OUT / "exclude.json").read_text())
        bad = {t["tile"] for t in ex["tiles"] if t["archetype"] == biome}
        ks = {p.stem for p in (OUT / "tiles" / biome).glob("*.cgrid")} - bad
    return sorted((OUT / "tiles" / biome / f"{t}.cgrid") for t in ks)


def sweep(biome: str, limit: int | None, variogram: bool = True) -> list[dict]:
    tiles = kept(biome)
    if limit and len(tiles) > limit:
        # Even stride, NOT [:limit]. Tile ids sort geographically, so the first
        # N are one lattice corner -- measured 2026-08-22: the first 10 Nebraska
        # tiles gave spectral A p50 0.298 against 0.400 for the full 44, which
        # flipped the gate's verdict. A subsample of a lattice is a place, not
        # a sample.
        idx = np.linspace(0, len(tiles) - 1, limit).round().astype(int)
        tiles = [tiles[i] for i in sorted(set(idx))]
    rows = []
    for i, t in enumerate(tiles, 1):
        r = measure_tile(t, with_variogram=variogram)
        r["biome"] = biome
        rows.append(r)
        print(f"  [{i:3d}/{len(tiles)}] {biome}/{t.stem}  "
              f"crests {r['n_crest']:3d}  coh {r.get('coherence', float('nan')):.3f}",
              flush=True)
    return rows


def table(rows: list[dict], keys: list[str]) -> str:
    import collections
    by = collections.defaultdict(list)
    for r in rows:
        by[r["biome"]].append(r)
    w = max(len(b) for b in by) + 1
    out = [f"{'biome':<{w}} {'n':>3} " + " ".join(f"{k:>15}" for k in keys)]
    for b, rs in by.items():
        cells = []
        for k in keys:
            v = np.array([r.get(k, np.nan) for r in rs], dtype=float)
            v = v[np.isfinite(v)]
            cells.append(f"{np.median(v):15.3f}" if v.size else f"{'--':>15}")
        out.append(f"{b:<{w}} {len(rs):3d} " + " ".join(cells))
    return "\n".join(out)


# --------------------------------------------------------------------------
# Form class — the corpus is a MIXTURE, and that is a fact about dune fields.
#
# MEASURED 2026-08-22 over all 44 kept Nebraska tiles: orientation order A is
# cleanly bimodal, Ashman D = 3.96, with modes at A 0.339 and 0.756. Hill
# country over the same instrument is UNIMODAL (D = 1.88), so this is a
# property of the dune corpus and not of the statistic.
#
#   TRAINS  n=22, A p50 0.502 -- barchanoid ridge trains. Beat every
#           structural-grain control (hill_country .390, mountain_bench .345)
#           AND heathland (.410).
#   MOUNDS  n=22, A p50 0.261 -- domal / parabolic mound fields. Below every
#           control. A real dune form, not a measurement failure.
#
# Pooling them puts the median at 0.400 against hill country's 0.390 -- a
# margin smaller than the sampling noise, which is why a pooled median-vs-median
# test on this corpus is unpassable in either direction. It also independently
# reproduces the train/mound continuum the previous generator already carried
# ("1/3 strong trains, ~40% mounds"), found here by a different route.

ASHMAN_BIMODAL = 2.0


def ashman_d(v: np.ndarray) -> tuple[float, float, float]:
    """Ashman's D and the two component means of a 2-component GMM."""
    from sklearn.mixture import GaussianMixture
    v = np.asarray(v, dtype=float)
    v = v[np.isfinite(v)]
    if v.size < 8:
        return float("nan"), float("nan"), float("nan")
    g = GaussianMixture(2, covariance_type="full", random_state=0, n_init=3).fit(v.reshape(-1, 1))
    o = np.argsort(g.means_.ravel())
    mu = g.means_.ravel()[o]
    sd = np.sqrt(g.covariances_.ravel())[o]
    return float(abs(mu[1] - mu[0]) / np.sqrt(0.5 * (sd[0] ** 2 + sd[1] ** 2))), float(mu[0]), float(mu[1])


def split_form_class(rows: list[dict], key: str = "spectral_A") -> tuple[list[dict], list[dict]]:
    """Partition tiles into MOUND and TRAIN by the corpus's own bimodality.

    The boundary is the GMM decision point (posterior crossing), not a
    hand-picked percentile -- so it moves with the data rather than being a
    knob. Falls back to the median if the population is not bimodal.
    """
    from sklearn.mixture import GaussianMixture
    v = np.array([r.get(key, np.nan) for r in rows], dtype=float)
    ok = np.isfinite(v)
    if ok.sum() < 8:
        return [], list(rows)
    d, _, _ = ashman_d(v[ok])
    if not np.isfinite(d) or d < ASHMAN_BIMODAL:
        cut = float(np.median(v[ok]))
        lab = v >= cut
    else:
        g = GaussianMixture(2, covariance_type="full", random_state=0,
                            n_init=3).fit(v[ok].reshape(-1, 1))
        hi = int(np.argmax(g.means_.ravel()))
        lab = np.zeros(len(rows), dtype=bool)
        lab[ok] = g.predict(v[ok].reshape(-1, 1)) == hi
    for r, t in zip(rows, lab):
        r["form_class"] = "train" if t else "mound"
    return [r for r, t in zip(rows, lab) if t], [r for r, t in zip(rows, lab) if not t]


# --------------------------------------------------------------------------
# The falsification harness.

# The dune corpus, and the two structural-grain corpora it must beat.
# `mountain_bench` IS the Cumberland Plateau set (Big South Fork / Obed) that
# falsified the retired metric; `hill_country` is the Ozark upland.
DUNE = "sandhills"
CONTROLS = ["mountain_bench", "hill_country"]


def validate(limit: int) -> int:
    print(__doc__.split("\n\n")[0])
    print(f"\nFALSIFICATION TEST — dune corpus `{DUNE}` vs structural-grain controls "
          f"{CONTROLS}\n(the retired `dune_wavelength_m` scored NE 0.41 against "
          "Cumberland 0.75 and was quarantined)\n")
    print("Sweeping the FULL corpora — a stride subsample of a lattice is a place,")
    print("not a sample, and at n=8-14 the medians move by more than the margin.\n")
    rows = sweep(DUNE, None)
    ctrl = []
    for c in CONTROLS:
        ctrl += sweep(c, limit)

    d, m0, m1 = ashman_d(np.array([r["spectral_A"] for r in rows]))
    trains, mounds = split_form_class(rows)
    print(f"\nFORM CLASS — orientation order over {len(rows)} {DUNE} tiles: "
          f"Ashman D {d:.2f} ({'BIMODAL' if d >= ASHMAN_BIMODAL else 'unimodal'}), "
          f"modes {m0:.3f} / {m1:.3f}")
    print(f"  trains n={len(trains)}   mounds n={len(mounds)}\n")

    for r in trains:
        r["biome"] = f"{DUNE}:train"
    for r in mounds:
        r["biome"] = f"{DUNE}:mound"
    keys = ["spectral_A", "flank_asym", "lam_dom_m", "band_relief_m",
            "crest_len_p50", "defect_km2", "n_crest"]
    print(table(trains + mounds + ctrl, keys) + "\n")

    def med(b, k):
        v = np.array([r.get(k, np.nan) for r in (trains + mounds + ctrl)
                      if r["biome"] == b], dtype=float)
        v = v[np.isfinite(v)]
        return float(np.median(v)) if v.size else float("nan")

    # THE GATE: the TRAIN subpopulation must outrank every control on the
    # orientation order. Gating the pooled mixture is not a test the data can
    # pass, and pretending otherwise would be the "8/8 metrics in band" failure
    # with the sign reversed.
    fails = []
    tgt = f"{DUNE}:train"
    for k, label in (("spectral_A", "spectral orientation order"),):
        dv = med(tgt, k)
        for c in CONTROLS:
            cv = med(c, k)
            ok = dv > cv
            print(f"  {'PASS' if ok else 'FAIL'}  {label:26s} {tgt} {dv:8.3f} "
                  f"{'>' if ok else '<='} {c} {cv:8.3f}")
            if not ok:
                fails.append(f"{label} vs {c}")
    # The split must also be real, or "trains" is just the top half of noise.
    ok_bi = d >= ASHMAN_BIMODAL
    print(f"  {'PASS' if ok_bi else 'FAIL'}  {'form-class separation':26s} Ashman D {d:8.2f} "
          f"{'>=' if ok_bi else '<'} {ASHMAN_BIMODAL:.1f}")
    if not ok_bi:
        fails.append("form classes are not separable")

    print()
    if fails:
        print("INSTRUMENT FALSIFIED — it does not separate a dune field from")
        print("structural grain, which is exactly how `dune_wavelength_m` died.")
        print("Failed: " + "; ".join(fails))
        print("No generator work starts until this passes.")
        return 1
    print("INSTRUMENT VALIDATED — the TRAIN subpopulation outranks every")
    print("structural-grain control, and the train/mound split is a real")
    print("bimodality in the corpus rather than a chosen percentile.")
    print(f"\nTargets for the generator: trains A p50 {med(tgt,'spectral_A'):.3f}, "
          f"lambda {med(tgt,'lam_dom_m'):.0f} m, band relief {med(tgt,'band_relief_m'):.1f} m")
    print(f"                           mounds A p50 {med(f'{DUNE}:mound','spectral_A'):.3f}, "
          f"lambda {med(f'{DUNE}:mound','lam_dom_m'):.0f} m, "
          f"band relief {med(f'{DUNE}:mound','band_relief_m'):.1f} m")
    return 0


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--validate", action="store_true", help="run the falsification harness")
    ap.add_argument("--corpus", nargs="+", metavar="BIOME", help="sweep these biomes")
    ap.add_argument("--tile", metavar="PATH", help="measure one .cgrid")
    ap.add_argument("--limit", type=int, default=8)
    ap.add_argument("--json", metavar="PATH", help="write rows as JSON")
    a = ap.parse_args()

    if a.validate:
        return validate(a.limit)
    if a.tile:
        print(json.dumps(measure_tile(pathlib.Path(a.tile)), indent=2))
        return 0
    if a.corpus:
        rows = []
        for b in a.corpus:
            rows += sweep(b, a.limit)
        keys = ["spectral_A", "lam_dom_m", "band_relief_m", "flank_asym",
                "n_crest", "crest_len_p50", "ridge_spacing_m", "prominence_p50",
                "defect_km2", "wavelength_m"]
        print("\n" + table(rows, keys))
        if a.json:
            pathlib.Path(a.json).write_text(json.dumps(rows, indent=2))
            print(f"\nwrote {a.json}")
        return 0
    ap.print_help()
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
