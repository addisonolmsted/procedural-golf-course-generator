#!/usr/bin/env python3
"""X0 — the HAND pack, and the leave-one-out reconstruction test.

The attempt-2 representation is z = B + H: a graded bed field plus a
height-above-nearest-drainage field synthesized from real material. This
script builds that material (the corpus extraction run over the kept NC
tiles → a pooled median curve H0(dist, acc) + a pack of HAND-residual
patches), and runs the KILL TEST the plan demands before any Rust exists:

    leave-one-out reconstruction — rebuild a real tile from its OWN
    extracted network but with H synthesized from the OTHER 29 tiles.

If the reconstruction cannot reproduce a Carolina surface given a perfect
network, the approach dies here, for the cost of this script.

    python3 tools/aeolian/build_handpack.py build          # pack from all 30
    python3 tools/aeolian/build_handpack.py reconstruct t03455_08020 ...

Policy: the corpus extraction exactly (8 m, fill_depressions, D8,
channels at acc >= 6e4 m^2), HAND/dist by euclidean attribution — the same
derivations as tools/macro_campaign/valley_compare.py.
"""
from __future__ import annotations

import json
import pathlib
import sys

import numpy as np
from scipy import ndimage

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
sys.path.insert(0, str(ROOT / "tools" / "metrics"))
from macro_campaign import cgrid, flow                      # noqa: E402
from metrics import core as mcore                           # noqa: E402

CELL = 8.0
THRESH = 6.0e4
PATCH = 48          # cells = 384 m
STRIDE = 24
# conditioning: distance band at the patch centre, attributed channel size,
# axial grain bin
DIST_EDGES = [60.0, 140.0, 240.0, 380.0, 560.0]            # -> 6 bands
ACC_EDGES = [10 ** 5.3, 10 ** 6.0]                          # -> 3 bands
GRAIN_BINS = 4                                              # axial, mod pi
# the H0 curve: median HAND on a dist grid per acc band
H0_DIST = np.concatenate([[0.0], np.geomspace(12.0, 900.0, 17)])

BIOME = "sandhills_nc"
TILES_DIR = ROOT / "tools/macro_campaign/out/tiles" / BIOME
OUT_PACK = ROOT / "assets" / "sandhills_handpack.npz"


def kept_tiles():
    rv = json.loads((ROOT / "tools/macro_campaign/out/review_v2.json").read_text())
    return [e["tile"] for e in rv["kept"] if e["archetype"] == BIOME]


def analyze(z8):
    """The corpus extraction + HAND/dist/attribution/grain fields."""
    zf = mcore.fill_depressions(z8, CELL)
    rec, _ = flow.receivers(zf, CELL)
    acc = flow.accumulate(rec).astype(np.float64) * CELL * CELL
    chan = acc >= THRESH
    dist, (iy, ix) = ndimage.distance_transform_edt(
        ~chan, sampling=CELL, return_indices=True)
    hand = z8 - z8[iy, ix]
    acc_att = acc[iy, ix]
    dsm = ndimage.gaussian_filter(dist, 3.0)
    gy, gx = np.gradient(dsm, CELL)
    grain = np.arctan2(gx, -gy) % np.pi                     # axial
    return {"chan": chan, "acc": acc, "dist": dist, "hand": hand,
            "acc_att": acc_att, "grain": grain, "bed": z8[iy, ix]}


def h0_fit(samples):
    """Pooled median HAND per (acc band, dist grid point)."""
    dist, acc, hand = samples
    ab = np.digitize(acc, ACC_EDGES)
    curve = np.zeros((len(ACC_EDGES) + 1, len(H0_DIST)))
    for a in range(len(ACC_EDGES) + 1):
        m = ab == a
        for i, d0 in enumerate(H0_DIST):
            lo = H0_DIST[i - 1] if i > 0 else -1.0
            hi = H0_DIST[i + 1] if i + 1 < len(H0_DIST) else 1e9
            mm = m & (dist > (d0 + lo) / 2) & (dist <= (d0 + hi) / 2)
            curve[a, i] = np.median(hand[mm]) if mm.sum() > 40 else np.nan
    # fill gaps by nearest valid along the dist axis
    for a in range(curve.shape[0]):
        v = curve[a]
        ok = np.isfinite(v)
        if ok.any():
            curve[a] = np.interp(np.arange(len(v)), np.nonzero(ok)[0], v[ok])
        else:
            curve[a] = 0.0
    return curve


def h0_eval(curve, dist, acc):
    ab = np.clip(np.digitize(acc, ACC_EDGES), 0, curve.shape[0] - 1)
    out = np.empty_like(dist)
    for a in range(curve.shape[0]):
        m = ab == a
        if m.any():
            out[m] = np.interp(dist[m], H0_DIST, curve[a])
    return out


def bucket_of(dist, acc, grain):
    db = int(np.digitize(dist, DIST_EDGES))
    ab = int(np.digitize(acc, ACC_EDGES))
    gb = int(grain / np.pi * GRAIN_BINS) % GRAIN_BINS
    return (db * (len(ACC_EDGES) + 1) + ab) * GRAIN_BINS + gb


N_BUCKET = (len(DIST_EDGES) + 1) * (len(ACC_EDGES) + 1) * GRAIN_BINS


def load_z8(tile):
    z, (_, _, cell) = cgrid.read_f32(str(TILES_DIR / f"{tile}.cgrid"))
    step = max(1, int(round(CELL / cell)))
    return z[::step, ::step].astype(np.float64)


def collect(tiles, verbose=True):
    """Extraction over `tiles` -> (H0 curve, residual patch buckets)."""
    samples_d, samples_a, samples_h = [], [], []
    per_tile = []
    for t in tiles:
        z8 = load_z8(t)
        A = analyze(z8)
        per_tile.append((t, A))
        samples_d.append(A["dist"].ravel())
        samples_a.append(A["acc_att"].ravel())
        samples_h.append(A["hand"].ravel())
        if verbose:
            print(f"  extracted {t}", flush=True)
    curve = h0_fit((np.concatenate(samples_d), np.concatenate(samples_a),
                    np.concatenate(samples_h)))
    buckets = [[] for _ in range(N_BUCKET)]
    rng = np.random.default_rng(20260828)
    seen = [0] * N_BUCKET
    CAP = 240
    for t, A in per_tile:
        resid = A["hand"] - h0_eval(curve, A["dist"], A["acc_att"])
        ny, nx = resid.shape
        for y in range(0, ny - PATCH, STRIDE):
            for x in range(0, nx - PATCH, STRIDE):
                cy, cx = y + PATCH // 2, x + PATCH // 2
                b = bucket_of(A["dist"][cy, cx], A["acc_att"][cy, cx],
                              A["grain"][cy, cx])
                p = resid[y:y + PATCH, x:x + PATCH].astype(np.float32)
                if not np.isfinite(p).all():
                    continue
                bl = buckets[b]
                if len(bl) < CAP:
                    bl.append(p)
                else:
                    seen[b] += 1
                    j = rng.integers(0, seen[b])
                    if j < CAP:
                        bl[j] = p
    return curve, buckets


def quilt_residual(buckets, dist, acc_att, grain, seed=7):
    """Overlap-add residual synthesis over the target fields."""
    ny, nx = dist.shape
    acc_grid = np.zeros((ny, nx))
    wsum = np.zeros((ny, nx))
    w1 = np.hanning(PATCH + 2)[1:-1]
    win = np.outer(w1, w1)
    counts = np.array([len(b) for b in buckets])
    tile_i = 0
    for y0 in range(-STRIDE, ny, STRIDE):
        for x0 in range(-STRIDE, nx, STRIDE):
            cy = np.clip(y0 + PATCH // 2, 0, ny - 1)
            cx = np.clip(x0 + PATCH // 2, 0, nx - 1)
            b = bucket_of(dist[cy, cx], acc_att[cy, cx], grain[cy, cx])
            # fallback: grain ring, then acc ring within the dist band
            if counts[b] == 0:
                base = b - b % GRAIN_BINS
                ring = [base + g for g in range(GRAIN_BINS)]
                db = b // ((len(ACC_EDGES) + 1) * GRAIN_BINS)
                for a in range(len(ACC_EDGES) + 1):
                    ring += [(db * (len(ACC_EDGES) + 1) + a) * GRAIN_BINS + g
                             for g in range(GRAIN_BINS)]
                for rb in ring:
                    if counts[rb] > 0:
                        b = rb
                        break
                else:
                    tile_i += 1
                    continue
            k = (seed * 2654435761 + tile_i * 40503) % counts[b]
            p = buckets[b][k]
            ys, xs = max(0, y0), max(0, x0)
            ye, xe = min(ny, y0 + PATCH), min(nx, x0 + PATCH)
            py, px = ys - y0, xs - x0
            acc_grid[ys:ye, xs:xe] += (win[py:py + ye - ys, px:px + xe - xs]
                                       * p[py:py + ye - ys, px:px + xe - xs])
            wsum[ys:ye, xs:xe] += win[py:py + ye - ys, px:px + xe - xs]
            tile_i += 1
    out = np.where(wsum > 1e-9, acc_grid / np.maximum(wsum, 1e-9), 0.0)
    return out


def soft_fields(z8, A, k=8, tau=140.0):
    """v2: SOFT attribution — B and H0 blended over the k nearest channel
    cells with exp(-d/tau) weights. Kills the Voronoi facets and medial-axis
    creases the hard-nearest reconstruction printed."""
    from scipy.spatial import cKDTree
    ny, nx = z8.shape
    cy, cx = np.nonzero(A["chan"])
    tree = cKDTree(np.c_[cy, cx] * CELL)
    py, px = np.mgrid[0:ny, 0:nx]
    dists, idxs = tree.query(np.c_[py.ravel() * CELL, px.ravel() * CELL], k=k)
    beds = z8[cy[idxs], cx[idxs]]
    accs = A["acc"][cy[idxs], cx[idxs]]
    w = np.exp(-dists / tau)
    w /= w.sum(axis=1, keepdims=True)
    B = (w * beds).sum(axis=1).reshape(ny, nx)
    return B, dists, accs, w


def resid_stats(tiles_data, curve):
    """Residual std per (dist band, acc band), pooled over the pack tiles."""
    n_d, n_a = len(DIST_EDGES) + 1, len(ACC_EDGES) + 1
    acc_s = np.zeros((n_d, n_a))
    acc_n = np.zeros((n_d, n_a))
    for _, A in tiles_data:
        resid = A["hand"] - h0_eval(curve, A["dist"], A["acc_att"])
        db = np.digitize(A["dist"], DIST_EDGES)
        ab = np.digitize(A["acc_att"], ACC_EDGES)
        for d in range(n_d):
            for a in range(n_a):
                m = (db == d) & (ab == a)
                if m.sum() > 100:
                    acc_s[d, a] += resid[m].std() * m.sum()
                    acc_n[d, a] += m.sum()
    return np.where(acc_n > 0, acc_s / np.maximum(acc_n, 1), 0.5)


def synth_residual(shape, std_map, seed=11):
    """Calibrated procedural residual: two smoothed white-noise octaves,
    unit-normalized, scaled by the measured per-band std envelope."""
    rng = np.random.default_rng(seed)
    r = (0.62 * ndimage.gaussian_filter(rng.standard_normal(shape), 3.0)
         + 0.38 * ndimage.gaussian_filter(rng.standard_normal(shape), 9.0))
    r /= r.std()
    return r * std_map


def reconstruct2(target, out_dir):
    """v2: soft attribution + curve + calibrated procedural residual."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas = []
    samples = [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B, dists, accs, w = soft_fields(z8, A)
    ny, nx = z8.shape
    # H0 blended over the same neighbors (medial-axis kinks smooth out)
    H = (w * h0_eval(curve, dists, accs)).sum(axis=1).reshape(ny, nx)
    db = np.digitize(A["dist"], DIST_EDGES)
    ab = np.digitize(A["acc_att"], ACC_EDGES)
    std_map = stds[db, ab]
    R = synth_residual((ny, nx), std_map)
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon2_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    for name, z in [("real", z8), ("soft", B + H), ("recon2", z_rec)]:
        fine = z - ndimage.gaussian_filter(z, (64.0 / np.pi) / CELL)
        print(f"  {target} {name:6s}: relief "
              f"{np.percentile(z, 99) - np.percentile(z, 1):5.1f} m  "
              f"band64 {float(fine.std()):.3f}")
    return z8, B + H, z_rec


def harmonic_bed(z8, chan, iters=260):
    """B as a CLAMPED DIFFUSION of the channel beds: channel cells hold
    their elevation, everything else relaxes toward the harmonic
    interpolation. C-infinity away from channels — no attribution facets,
    no k-nearest fans (both printed as crease filaments in v1/v2)."""
    B = ndimage.grey_erosion(z8, size=3).astype(np.float64)
    bed_vals = z8[chan]
    B[chan] = bed_vals
    for _ in range(iters):
        B = ndimage.uniform_filter(B, size=3)
        B[chan] = bed_vals
    return B


def h0_eval_cont(curve, dist, log_acc):
    """H0 continuous in log-acc: linear blend between the band curves."""
    centers = np.array([5.05, 5.65, 6.4])
    la = np.clip(log_acc, centers[0], centers[-1])
    hi = np.clip(np.searchsorted(centers, la), 1, len(centers) - 1)
    lo = hi - 1
    t = (la - centers[lo]) / (centers[hi] - centers[lo])
    vlo = np.empty_like(dist)
    vhi = np.empty_like(dist)
    for a in range(curve.shape[0]):
        m_lo = lo == a
        m_hi = hi == a
        if m_lo.any():
            vlo[m_lo] = np.interp(dist[m_lo], H0_DIST, curve[a])
        if m_hi.any():
            vhi[m_hi] = np.interp(dist[m_hi], H0_DIST, curve[a])
    return vlo * (1 - t) + vhi * t


def reconstruct3(target, out_dir):
    """v3: harmonic B + continuous H0 + long-octave calibrated residual."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas = []
    samples = [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B = harmonic_bed(z8, A["chan"])
    log_acc = ndimage.gaussian_filter(np.log10(A["acc_att"]), 2.0)
    H = h0_eval_cont(curve, A["dist"], log_acc)
    db = np.digitize(A["dist"], DIST_EDGES)
    ab = np.digitize(A["acc_att"], ACC_EDGES)
    std_map = ndimage.gaussian_filter(stds[db, ab], 4.0) * 0.8
    rng = np.random.default_rng(11)
    R = (0.55 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 14.0)
         + 0.45 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 36.0))
    R = R / R.std() * std_map
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon3_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    for name, z in [("real", z8), ("BH", B + H), ("recon3", z_rec)]:
        fine = z - ndimage.gaussian_filter(z, (64.0 / np.pi) / CELL)
        print(f"  {target} {name:6s}: relief "
              f"{np.percentile(z, 99) - np.percentile(z, 1):5.1f} m  "
              f"band64 {float(fine.std()):.3f}")
    return z8, B + H, z_rec


def soft_distance(chan, k=6, tau=90.0):
    """SOFT-MIN distance to the k nearest channel cells, with the matching
    soft-attributed accumulation weights.

    The hard EDT distance has a gradient KINK at the medial axis (the
    divide), and H0(dist) inherits it as a crisp ridge line — measured at
    2.9x the curvature of the surrounding ground, where REAL divides carry
    exactly the same curvature as open ground (0.093 vs 0.093). Soft-min
    rounds the divide by construction: both channels contribute there.

        d_soft = -tau * log( sum_i exp(-d_i / tau) )

    Returns (d_soft, weights, indices) — the weights also drive the
    attributed accumulation, which removes the acc-jump seam (a true C0
    step, measured at 3.4x) in the same stroke.
    """
    from scipy.spatial import cKDTree
    ny, nx = chan.shape
    cy, cx = np.nonzero(chan)
    tree = cKDTree(np.c_[cy, cx] * CELL)
    py, px = np.mgrid[0:ny, 0:nx]
    d, idx = tree.query(np.c_[py.ravel() * CELL, px.ravel() * CELL],
                        k=min(k, len(cy)))
    d = np.atleast_2d(d)
    idx = np.atleast_2d(idx)
    dmin = d.min(axis=1, keepdims=True)
    e = np.exp(-(d - dmin) / tau)
    d_soft = (dmin[:, 0] - tau * np.log(e.sum(axis=1))).reshape(ny, nx)
    w = e / e.sum(axis=1, keepdims=True)
    return d_soft, w, idx, (cy, cx)


def soft_fields_conv(chan, acc, dist_hard, tau=90.0):
    """Soft-min distance and soft-attributed log-acc, aggregated over ALL
    channel cells by convolution.

        S(p)    = sum_j exp(-|p-j|/tau)          (j over channel cells)
        d_soft  = -tau * log S
        la_soft = sum_j la_j exp(-|p-j|/tau) / S

    The k-nearest form does NOT work here: the k nearest channel CELLS all
    belong to the same channel (they are adjacent along its line), so the
    soft-min never blends across a divide — measured, it made the crease
    slightly worse (2.9x -> 3.5x). Summing over every channel cell is what
    makes both sides of a divide contribute, which rounds the medial axis
    by construction and removes the acc-jump step in the same stroke.
    """
    from scipy.signal import fftconvolve
    rad = int(np.ceil(6.0 * tau / CELL))
    yy, xx = np.mgrid[-rad:rad + 1, -rad:rad + 1]
    K = np.exp(-np.hypot(yy, xx) * CELL / tau)
    ch = chan.astype(np.float64)
    S = fftconvolve(ch, K, mode="same")
    la = np.zeros_like(ch)
    la[chan] = np.log10(acc[chan])
    num = fftconvolve(la, K, mode="same")
    ok = S > 1e-9
    d_soft = np.where(ok, -tau * np.log(np.maximum(S, 1e-300)), dist_hard)
    d_soft = np.minimum(d_soft, dist_hard)          # soft-min <= hard min
    la_soft = np.where(ok, num / np.maximum(S, 1e-300), np.log10(6e4))
    return d_soft, la_soft


def _kernel(tau):
    rad = int(np.ceil(6.0 * tau / CELL))
    yy, xx = np.mgrid[-rad:rad + 1, -rad:rad + 1]
    return np.exp(-np.hypot(yy, xx) * CELL / tau)


def _line_reference(tau):
    """S(d) for a single straight channel — the normalisation that makes
    the soft distance EXACT next to an isolated channel."""
    from scipy.signal import fftconvolve
    n = int(np.ceil(12.0 * tau / CELL)) * 2 + 41
    g = np.zeros((n, n))
    g[n // 2, :] = 1.0
    S = fftconvolve(g, _kernel(tau), mode="same")[:, n // 2]
    d = np.abs(np.arange(n) - n // 2) * CELL
    half = slice(n // 2, n)
    return d[half], S[half]


def soft_fields_norm(chan, acc, dist_hard, tau=60.0):
    """Divide-rounded distance that PRESERVES the valley profile.

    Plain soft-min (v5) shrank distance everywhere — a channel is a LINE,
    so many of its own cells contribute — and the tile washed out
    (dissection gone, though the crease gate passed). Here S is inverted
    through the straight-line reference S_ref(d): next to an isolated
    channel the result is d_hard by construction, and only where a second
    channel contributes does S exceed the reference and pull the distance
    down — which is exactly the medial axis, and exactly the crest that
    needs rounding.
    """
    from scipy.signal import fftconvolve
    K = _kernel(tau)
    ch = chan.astype(np.float64)
    S = fftconvolve(ch, K, mode="same")
    d_ref, S_ref = _line_reference(tau)
    order = np.argsort(S_ref)
    d_soft = np.interp(S, S_ref[order], d_ref[order])
    d_soft = np.clip(d_soft, 0.0, None)
    d_soft = np.minimum(d_soft, dist_hard)
    la = np.zeros_like(ch)
    la[chan] = np.log10(acc[chan])
    num = fftconvolve(la, K, mode="same")
    la_soft = np.where(S > 1e-9, num / np.maximum(S, 1e-300), np.log10(6e4))
    return d_soft, la_soft


def soft_bed(chan, z8, tau_b=80.0):
    """Bed field as the exp-weighted mean of nearby CHANNEL bed elevations.

    Replaces the harmonic diffusion: harmonic interpolation averages in
    UPSTREAM beds, which are higher, so every divide floated up (measured:
    relief 30% over real, the excess concentrated at the crests). The
    weighted mean is consistent with how HAND is defined — height above
    the nearby channel bed — and stays smooth, so the facets the diffusion
    was introduced to kill do not come back.
    """
    from scipy.signal import fftconvolve
    K = _kernel(tau_b)
    ch = chan.astype(np.float64)
    zz = np.zeros_like(ch)
    zz[chan] = z8[chan]
    S = fftconvolve(ch, K, mode="same")
    N = fftconvolve(zz, K, mode="same")
    B = np.where(S > 1e-9, N / np.maximum(S, 1e-300), np.nan)
    if np.isnan(B).any():                      # far corners: nearest bed
        m = np.isnan(B)
        _, (iy, ix) = ndimage.distance_transform_edt(
            ~chan, sampling=CELL, return_indices=True)
        B[m] = z8[iy[m], ix[m]]
    return B


def round_crests(H, dist, sigma=5.0, passes=3, width=0.55):
    """Round the divide crest, and ONLY the divide crest.

    H0(dist) inherits the distance field's gradient kink at the medial
    axis. Blending distances instead (soft-min, any radius wide enough to
    bridge a divide) also flattens everything else, because the divide
    spacing IS the channel spacing — measured: the tile washed out.

    |grad d| is the exact indicator of the medial axis: 1 on open ground,
    collapsing to 0 on the crest. Use it as the blend weight for a local
    smooth of H, and the treatment lands on the crease alone.
    """
    gy, gx = np.gradient(dist, CELL)
    w = np.clip(1.0 - np.hypot(gx, gy) / width, 0.0, 1.0)
    w = ndimage.gaussian_filter(w, 2.0)
    out = H
    for _ in range(passes):
        out = out * (1 - w) + ndimage.gaussian_filter(out, sigma) * w
    return out


def channel_links(chan, acc):
    """Label channel cells by LINK — the reach between confluences.

    Needed because the divide crease must be rounded between DISTINCT
    channels, and a drainage network is one connected component: cell-wise
    neighbourhoods (k-nearest, convolution) never separate two banks of the
    same divide, which is why every distance-blending variant either failed
    to round the crest or flattened the whole tile.
    """
    lbl = np.zeros(chan.shape, np.int32)
    nb = ndimage.convolve(chan.astype(np.int32),
                          np.ones((3, 3), np.int32), mode="constant")
    junction = chan & (nb >= 4)          # 3+ channel neighbours + self
    links, n = ndimage.label(chan & ~junction,
                             structure=np.ones((3, 3), np.int32))
    lbl[:] = links
    # junction cells join their largest-accumulation neighbouring link
    jy, jx = np.nonzero(junction)
    for y, x in zip(jy, jx):
        best, bl = -1.0, 0
        for dy in (-1, 0, 1):
            for dx in (-1, 0, 1):
                yy, xx = y + dy, x + dx
                if 0 <= yy < chan.shape[0] and 0 <= xx < chan.shape[1] \
                        and links[yy, xx] > 0 and acc[yy, xx] > best:
                    best, bl = acc[yy, xx], links[yy, xx]
        lbl[y, x] = bl
    return lbl, n


def softmin_hand(lbl, n_links, acc, curve, shape, kh=2.2, min_cells=6):
    """H = soft-min over links of H0(d_link, acc_link).

    Away from a divide one link dominates and the soft-min IS that link's
    profile — exact, no flattening. Where two links compete the blend
    rounds the crest by ~kh*log(2) metres. The rounding scale is set in
    HEIGHT units, which is the quantity that was wrong.
    """
    acc_sm = np.zeros(shape)
    first = True
    for li in range(1, n_links + 1):
        m = lbl == li
        c = int(m.sum())
        if c < min_cells:
            continue
        d = ndimage.distance_transform_edt(~m, sampling=CELL)
        la = float(np.log10(np.median(acc[m])))
        h = h0_eval_cont(curve, d, np.full(shape, la))
        e = np.exp(-h / kh)
        acc_sm = e if first else acc_sm + e
        first = False
    return -kh * np.log(np.maximum(acc_sm, 1e-300))


def reconstruct9(target, out_dir, tau_b=80.0, kh=2.2, resid_k=1.0, h_gain=1.0):
    """v9 — soft bed + per-link soft-min HAND + calibrated residual."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas, samples = [], [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B = soft_bed(A["chan"], z8, tau_b=tau_b)
    lbl, n = channel_links(A["chan"], A["acc"])
    H = softmin_hand(lbl, n, A["acc"], curve, z8.shape, kh=kh) * h_gain
    db = np.digitize(A["dist"], DIST_EDGES)
    ab = np.digitize(A["acc_att"], ACC_EDGES)
    std_map = ndimage.gaussian_filter(stds[db, ab], 4.0) * resid_k
    rng = np.random.default_rng(11)
    R = (0.55 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 14.0)
         + 0.45 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 36.0))
    R = R / R.std() * std_map
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon9_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    return z8, B + H, z_rec, A, n


def reconstruct8(target, out_dir, tau_b=80.0, acc_sigma=2.0, resid_k=1.0,
                 h_gain=1.0, crest_sigma=5.0, crest_w=0.55, crest_passes=3):
    """v8 — the X0b candidate: soft bed (kills the harmonic upstream bias)
    + measured H0 on the HARD distance (valley profile intact) + crest
    rounding at the medial axis only + calibrated residual."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas, samples = [], [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B = soft_bed(A["chan"], z8, tau_b=tau_b)
    log_acc = ndimage.gaussian_filter(np.log10(A["acc_att"]), acc_sigma * 3)
    H = h0_eval_cont(curve, A["dist"], log_acc) * h_gain
    H = round_crests(H, A["dist"], sigma=crest_sigma,
                     passes=int(crest_passes), width=crest_w)
    db = np.digitize(A["dist"], DIST_EDGES)
    ab = np.digitize(A["acc_att"], ACC_EDGES)
    std_map = ndimage.gaussian_filter(stds[db, ab], 4.0) * resid_k
    rng = np.random.default_rng(11)
    R = (0.55 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 14.0)
         + 0.45 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 36.0))
    R = R / R.std() * std_map
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon8_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    return z8, B + H, z_rec, A


def reconstruct7(target, out_dir, tau=200.0, tau_b=80.0, acc_sigma=3.0,
                 resid_k=1.0, h_gain=1.0):
    """v7 — the X0b candidate: soft bed + reference-normalised divide
    rounding + measured H0 + calibrated residual. Everything derives from
    the same exponential weighting over channel cells."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas, samples = [], [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B = soft_bed(A["chan"], z8, tau_b=tau_b)
    d_soft, la_soft = soft_fields_norm(A["chan"], A["acc"], A["dist"], tau=tau)
    la_soft = ndimage.gaussian_filter(la_soft, acc_sigma)
    H = h0_eval_cont(curve, d_soft, la_soft) * h_gain
    db = np.digitize(d_soft, DIST_EDGES)
    ab = np.clip(np.digitize(10 ** la_soft, ACC_EDGES), 0, len(ACC_EDGES))
    std_map = ndimage.gaussian_filter(stds[db, ab], 4.0) * resid_k
    rng = np.random.default_rng(11)
    R = (0.55 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 14.0)
         + 0.45 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 36.0))
    R = R / R.std() * std_map
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon7_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    return z8, B + H, z_rec, A


def reconstruct6(target, out_dir, tau=60.0, acc_sigma=3.0, resid_k=1.0):
    """v6 = v3 + reference-normalised divide rounding (the X0b fix)."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas, samples = [], [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B = harmonic_bed(z8, A["chan"])
    d_soft, la_soft = soft_fields_norm(A["chan"], A["acc"], A["dist"], tau=tau)
    la_soft = ndimage.gaussian_filter(la_soft, acc_sigma)
    H = h0_eval_cont(curve, d_soft, la_soft)
    db = np.digitize(d_soft, DIST_EDGES)
    ab = np.clip(np.digitize(10 ** la_soft, ACC_EDGES), 0, len(ACC_EDGES))
    std_map = ndimage.gaussian_filter(stds[db, ab], 4.0) * resid_k
    rng = np.random.default_rng(11)
    R = (0.55 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 14.0)
         + 0.45 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 36.0))
    R = R / R.std() * std_map
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon6_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    return z8, B + H, z_rec, A


def reconstruct5(target, out_dir, tau=90.0, acc_sigma=3.0, resid_k=1.0):
    """v5 = v3 + convolutional soft-min distance and soft log-acc."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas, samples = [], [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B = harmonic_bed(z8, A["chan"])
    d_soft, la_soft = soft_fields_conv(A["chan"], A["acc"], A["dist"], tau=tau)
    la_soft = ndimage.gaussian_filter(la_soft, acc_sigma)
    H = h0_eval_cont(curve, d_soft, la_soft)
    db = np.digitize(d_soft, DIST_EDGES)
    ab = np.clip(np.digitize(10 ** la_soft, ACC_EDGES), 0, len(ACC_EDGES))
    std_map = ndimage.gaussian_filter(stds[db, ab], 4.0) * resid_k
    rng = np.random.default_rng(11)
    R = (0.55 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 14.0)
         + 0.45 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 36.0))
    R = R / R.std() * std_map
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon5_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    return z8, B + H, z_rec, A


def reconstruct4(target, out_dir, tau=90.0, acc_sigma=6.0):
    """v4 = v3 + soft-min distance and soft-attributed accumulation."""
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    datas, samples = [], [[], [], []]
    for t in others:
        z8o = load_z8(t)
        Ao = analyze(z8o)
        datas.append((t, Ao))
        samples[0].append(Ao["dist"].ravel())
        samples[1].append(Ao["acc_att"].ravel())
        samples[2].append(Ao["hand"].ravel())
    curve = h0_fit(tuple(np.concatenate(x) for x in samples))
    stds = resid_stats(datas, curve)

    z8 = load_z8(target)
    A = analyze(z8)
    B = harmonic_bed(z8, A["chan"])
    d_soft, w, idx, (cy, cx) = soft_distance(A["chan"], tau=tau)
    ny, nx = z8.shape
    # soft-attributed accumulation, then smoothed hard: the acc seam is the
    # one true C0 discontinuity in the H field
    la_ch = np.log10(A["acc"][cy, cx])
    log_acc = (w * la_ch[idx]).sum(axis=1).reshape(ny, nx)
    log_acc = ndimage.gaussian_filter(log_acc, acc_sigma)
    H = h0_eval_cont(curve, d_soft, log_acc)
    db = np.digitize(d_soft, DIST_EDGES)
    ab = np.clip(np.digitize(10 ** log_acc, ACC_EDGES), 0, len(ACC_EDGES))
    std_map = ndimage.gaussian_filter(stds[db, ab], 4.0) * 0.8
    rng = np.random.default_rng(11)
    R = (0.55 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 14.0)
         + 0.45 * ndimage.gaussian_filter(rng.standard_normal(z8.shape), 36.0))
    R = R / R.std() * std_map
    z_rec = B + H + R
    np.savez_compressed(out_dir / f"recon4_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=(B + H).astype(np.float32))
    return z8, B + H, z_rec, A


def divide_gate(real, rec, A):
    """THE GATE (X0b): curvature on divide lines relative to open ground.
    Real Carolina divides are as smooth as their surroundings (ratio ~1.0);
    the hard-EDT reconstruction ran 2.9x."""
    gy, gx = np.gradient(A["dist"], CELL)
    medial = (np.hypot(gx, gy) < 0.55) & (A["dist"] > 40)
    open_g = (~medial) & (A["dist"] > 40)

    def lap(z):
        return ndimage.laplace(ndimage.gaussian_filter(z, 1.0))
    lr, lc = lap(real), lap(rec)
    out = {}
    for nm, z, l in [("real", real, lr), ("recon", rec, lc)]:
        out[nm] = (float(np.abs(l[medial]).mean()),
                   float(np.abs(l[open_g]).mean()))
        out[nm + "_ratio"] = out[nm][0] / out[nm][1]
        out[nm + "_relief"] = float(np.percentile(z, 99) - np.percentile(z, 1))
    return out


def reconstruct(target, out_dir):
    tiles = kept_tiles()
    others = [t for t in tiles if t != target]
    print(f"pack from {len(others)} tiles (leave out {target})")
    curve, buckets = collect(others)
    counts = np.array([len(b) for b in buckets])
    print(f"buckets: {int((counts == 0).sum())} empty of {N_BUCKET}, "
          f"median {int(np.median(counts))}")

    z8 = load_z8(target)
    A = analyze(z8)
    # B: the real tile's own bed attribution, softened across divides
    B = ndimage.gaussian_filter(A["bed"], 2.0)
    H0 = h0_eval(curve, A["dist"], A["acc_att"])
    R = quilt_residual(buckets, A["dist"], A["acc_att"], A["grain"])
    z_curve = B + H0
    z_rec = B + H0 + R

    np.savez_compressed(out_dir / f"recon_{target}.npz",
                        z_real=z8.astype(np.float32),
                        z_rec=z_rec.astype(np.float32),
                        z_curve=z_curve.astype(np.float32))
    for name, z in [("real", z8), ("curve", z_curve), ("recon", z_rec)]:
        fine = z - ndimage.gaussian_filter(z, (64.0 / np.pi) / CELL)
        print(f"  {target} {name:6s}: relief p99-p1 "
              f"{np.percentile(z, 99) - np.percentile(z, 1):5.1f} m   "
              f"band64 std {float(fine.std()):.3f}")
    return z8, z_curve, z_rec


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "build"
    if mode == "build":
        curve, buckets = collect(kept_tiles())
        packed, index = [], []
        for b in buckets:
            index.append((len(packed), len(b)))
            packed.extend(b)
        arr = (np.stack(packed) if packed
               else np.zeros((0, PATCH, PATCH), np.float32))
        np.savez_compressed(
            OUT_PACK, patches=arr.astype(np.float16),
            index=np.array(index, np.int32), curve=curve,
            h0_dist=H0_DIST, dist_edges=np.array(DIST_EDGES),
            acc_edges=np.array(ACC_EDGES), grain_bins=GRAIN_BINS,
            patch=PATCH)
        print(f"wrote {OUT_PACK.name}: {len(packed)} patches, "
              f"{int((np.array([c for _, c in index]) == 0).sum())} empty "
              f"of {N_BUCKET} buckets")
    elif mode == "reconstruct":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        targets = [a for a in sys.argv[2:] if a.startswith("t0")]
        for t in targets:
            reconstruct(t, out)
    elif mode == "reconstruct9":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        def arg(k, dv):
            return float(next((a.split("=")[1] for a in sys.argv
                               if a.startswith(k + "=")), dv))
        kh, hg, taub = arg("kh", 2.2), arg("hg", 1.0), arg("taub", 80.0)
        rk = arg("rk", 1.0)
        for t in [a for a in sys.argv[2:] if a.startswith("t0")]:
            real, bh, rec, A, n = reconstruct9(t, out, kh=kh, h_gain=hg,
                                               tau_b=taub, resid_k=rk)
            g = divide_gate(real, rec, A)
            fine = rec - ndimage.gaussian_filter(rec, (64.0 / np.pi) / CELL)
            print(f"  {t} kh={kh:.1f} hg={hg:.2f} links={n}: crest "
                  f"{g['recon'][0] / g['real'][0]:.2f}x real | open "
                  f"{g['recon'][1] / g['real'][1]:.2f}x | relief "
                  f"{g['real_relief']:.1f} vs {g['recon_relief']:.1f} | "
                  f"band64 {float(fine.std()):.3f}", flush=True)
    elif mode == "reconstruct8":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        def arg(k, dv):
            return float(next((a.split("=")[1] for a in sys.argv
                               if a.startswith(k + "=")), dv))
        cs, hg, taub = arg("cs", 5.0), arg("hg", 1.0), arg("taub", 80.0)
        cw, cp = arg("cw", 0.55), arg("cp", 3)
        for t in [a for a in sys.argv[2:] if a.startswith("t0")]:
            real, bh, rec, A = reconstruct8(t, out, crest_sigma=cs, h_gain=hg,
                                            tau_b=taub, crest_w=cw,
                                            crest_passes=cp)
            g = divide_gate(real, rec, A)
            fine = rec - ndimage.gaussian_filter(rec, (64.0 / np.pi) / CELL)
            # THE GATE: absolute crest curvature against the real tile's own
            # (the ratio-to-open-ground is inflated while texture is absent)
            print(f"  {t} cs={cs:.0f} cw={cw:.2f} cp={cp:.0f} hg={hg:.2f}: "
                  f"crest {g['recon'][0] / g['real'][0]:.2f}x real | "
                  f"relief {g['real_relief']:.1f} vs {g['recon_relief']:.1f} | "
                  f"band64 {float(fine.std()):.3f}", flush=True)
    elif mode == "reconstruct7":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        def arg(k, dv):
            return float(next((a.split("=")[1] for a in sys.argv
                               if a.startswith(k + "=")), dv))
        tau, tau_b, hg = arg("tau", 200.0), arg("taub", 80.0), arg("hg", 1.0)
        for t in [a for a in sys.argv[2:] if a.startswith("t0")]:
            real, bh, rec, A = reconstruct7(t, out, tau=tau, tau_b=tau_b, h_gain=hg)
            g = divide_gate(real, rec, A)
            fine = rec - ndimage.gaussian_filter(rec, (64.0 / np.pi) / CELL)
            print(f"  {t} tau={tau:.0f} taub={tau_b:.0f} hg={hg:.2f}: "
                  f"divide/open real {g['real_ratio']:.2f}x recon "
                  f"{g['recon_ratio']:.2f}x | relief {g['real_relief']:.1f} vs "
                  f"{g['recon_relief']:.1f} | band64 {float(fine.std()):.3f}",
                  flush=True)
    elif mode == "reconstruct6":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        tau = float(next((a.split("=")[1] for a in sys.argv if a.startswith("tau=")), 60.0))
        for t in [a for a in sys.argv[2:] if a.startswith("t0")]:
            real, bh, rec, A = reconstruct6(t, out, tau=tau)
            g = divide_gate(real, rec, A)
            fine = rec - ndimage.gaussian_filter(rec, (64.0 / np.pi) / CELL)
            print(f"  {t} tau={tau:.0f}: divide/open  real {g['real_ratio']:.2f}x  "
                  f"recon {g['recon_ratio']:.2f}x  |  relief real "
                  f"{g['real_relief']:.1f} recon {g['recon_relief']:.1f}  |  "
                  f"band64 {float(fine.std()):.3f}", flush=True)
    elif mode == "reconstruct5":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        tau = float(next((a.split("=")[1] for a in sys.argv if a.startswith("tau=")), 90.0))
        for t in [a for a in sys.argv[2:] if a.startswith("t0")]:
            real, bh, rec, A = reconstruct5(t, out, tau=tau)
            g = divide_gate(real, rec, A)
            fine = rec - ndimage.gaussian_filter(rec, (64.0 / np.pi) / CELL)
            print(f"  {t} tau={tau:.0f}: divide/open  real {g['real_ratio']:.2f}x  "
                  f"recon {g['recon_ratio']:.2f}x  |  relief real "
                  f"{g['real_relief']:.1f} recon {g['recon_relief']:.1f}  |  "
                  f"band64 {float(fine.std()):.3f}", flush=True)
    elif mode == "reconstruct4":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        for t in [a for a in sys.argv[2:] if a.startswith("t0")]:
            real, bh, rec, A = reconstruct4(t, out)
            g = divide_gate(real, rec, A)
            fine = rec - ndimage.gaussian_filter(rec, (64.0 / np.pi) / CELL)
            print(f"  {t}: divide/open curvature  real {g['real_ratio']:.2f}x   "
                  f"recon {g['recon_ratio']:.2f}x  |  relief real "
                  f"{g['real_relief']:.1f} recon {g['recon_relief']:.1f} m  |  "
                  f"band64 {float(fine.std()):.3f}", flush=True)
    elif mode == "reconstruct3":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        for t in [a for a in sys.argv[2:] if a.startswith("t0")]:
            reconstruct3(t, out)
    elif mode == "reconstruct2":
        out = pathlib.Path(sys.argv[-1]) if pathlib.Path(sys.argv[-1]).is_dir() \
            else pathlib.Path(".")
        targets = [a for a in sys.argv[2:] if a.startswith("t0")]
        for t in targets:
            reconstruct2(t, out)
    else:
        raise SystemExit(f"unknown mode {mode}")


if __name__ == "__main__":
    main()
