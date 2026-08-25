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
