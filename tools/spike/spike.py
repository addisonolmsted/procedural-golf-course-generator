"""M3.5 dictionary spike — fit, reconstruct, score (gate G-SPIKE).

Two-level conditioned patch dictionary fitted from 5 clean piedmont tiles,
evaluated by reconstructing the residual of a held-out 6th tile over that
tile's own lowpass:

  levels:  MID  = band 64-400 m, on the 8 m grid, 32x32 patches (256 m)
           FINE = band  <64 m,  on the 2 m grid, 32x32 patches ( 64 m)
  conditioning: lp_slope x tpi x relief_pos (terciles -> up to 27 buckets),
                all computed from the L=400 m macro surface
  basis:   per-bucket mean + PCA (adaptive rank to >=90% variance, cap 32),
           coefficient std per component; draws are POSITION-SEEDED
  blend:   periodic-Hann overlap-add at 50% (COLA: weights sum to 1);
           stochastic part variance-corrected by 1/sqrt(sum w^2)

Outputs:
  out/metrics.json            full scoring data (gitignored working dir)
  report/REPORT.md            the gate report table
  report/pair_*.png           labeled side-by-side hillshade crops
  report/blind/blind_*.png    shuffled crops for the P2 blind A/B
  report/blind/answer_key.json  (do not open until after the test)

Run:  python3 tools/spike/spike.py
"""
from __future__ import annotations

import json
import os
import sys

import numpy as np

REPO = os.path.normpath(os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", ".."))
for p in ("tools/macro_campaign", "tools/metrics", "tools/dtm_metrics"):
    sys.path.insert(0, os.path.join(REPO, p))

import yaml  # noqa: E402
from scipy import ndimage  # noqa: E402
from PIL import Image  # noqa: E402
from macro_campaign import cgrid  # noqa: E402
from metrics import features  # noqa: E402
from metrics import core as v1core  # noqa: E402

TILE_DIR = os.path.join(REPO, "tools/macro_campaign/out/tiles/piedmont")
TRAIN = ["t03520_07988", "t03525_07978", "t03528_07995", "t03538_07999", "t03544_07992"]
HOLDOUT = "t03550_08002"

SEED = 1
L_MACRO = 400.0     # everything above this is the macro surface (not spike scope)
L_MID = 64.0        # mid band: 64-400 m; fine band: < 64 m
PATCH = 32          # patch side, cells (both levels)
STRIDE = PATCH // 2 # 50% overlap (COLA for periodic Hann)
MAX_RANK = 32
VAR_TARGET = 0.90
MIN_BUCKET = 8

SIGMA_PER_L = float(np.sqrt(np.log(2.0) / (2.0 * np.pi ** 2)))

# Shape metrics are scale-invariant -- the dictionary must EARN them.
# Amplitude metrics are set by the per-band closer (as in the production
# pipeline, where S0 draws the site amplitude descriptor) -- they verify
# self-consistency, not synthesis quality. Mixed = shape-given-amplitude.
SHAPE_METRICS = ["spectral_slope_beta", "variogram_range", "anisotropy_ratio"]
MIXED_METRICS = ["mean_abs_profile_curv", "plan_curv_p90"]
AMPL_METRICS = ["variogram_sill", "rms_roughness_small", "rms_roughness_mid"]
KEY_METRICS = SHAPE_METRICS + MIXED_METRICS + AMPL_METRICS

OUT = os.path.join(REPO, "tools/spike/out")
REPORT = os.path.join(REPO, "tools/spike/report")


# ---------------------------------------------------------------- decompose

def load(name):
    z, (_ox, _oy, cell) = cgrid.read_f32(os.path.join(TILE_DIR, name + ".cgrid"))
    return z.astype(np.float64), cell


def decompose(z, cell):
    """-> dict of surfaces + conditioning fields (all at 2 m except mid8)."""
    valid = np.isfinite(z)
    filled = v1core._fill_nearest(z, valid)
    lp400 = ndimage.gaussian_filter(filled, SIGMA_PER_L * L_MACRO / cell)
    lp64 = ndimage.gaussian_filter(filled, SIGMA_PER_L * L_MID / cell)
    fine = filled - lp64            # < 64 m band, 2 m grid
    mid = lp64 - lp400              # 64-400 m band, band-limited -> safe at 8 m
    mid8 = mid[::4, ::4].copy()

    gy, gx = np.gradient(lp400, cell)
    lp_slope = np.hypot(gx, gy)
    w = max(3, int(round(L_MACRO / cell)) | 1)
    tpi = lp400 - ndimage.uniform_filter(lp400, w)
    lv = lp400[valid]
    order = np.argsort(lv, kind="stable")
    ranks = np.empty_like(order, dtype=np.float64)
    ranks[order] = np.linspace(0.0, 1.0, lv.size)
    relief_pos = np.zeros_like(lp400)
    relief_pos[valid] = ranks

    cond = np.stack([lp_slope, tpi, relief_pos], axis=-1)  # (H, W, 3) at 2 m
    return dict(valid=valid, filled=filled, lp400=lp400, fine=fine,
                mid8=mid8, cond=cond, cond8=cond[::4, ::4].copy())


# ------------------------------------------------------------------- fitting

def patch_grid(n, stride=STRIDE):
    """Patch origins covering [0, n) with the given stride, edge-clamped."""
    xs = list(range(0, n - PATCH + 1, stride))
    if xs[-1] != n - PATCH:
        xs.append(n - PATCH)
    return xs


def extract(band, cond):
    """-> (patches (n, PATCH*PATCH) f32, cond vectors (n, 3))."""
    H, W = band.shape
    ys, xs = patch_grid(H), patch_grid(W)
    P, C = [], []
    for y in ys:
        for x in xs:
            P.append(band[y:y + PATCH, x:x + PATCH].ravel())
            C.append(cond[y:y + PATCH, x:x + PATCH].reshape(-1, 3).mean(axis=0))
    return np.asarray(P, dtype=np.float32), np.asarray(C, dtype=np.float64)


def fit_level(patches, cond, edges=None):
    """Bucket by conditioning terciles; per-bucket mean + PCA.

    -> dict(edges, buckets={id: (mean, basis, std)}, global_=(mean, basis, std))
    """
    if edges is None:
        edges = [np.quantile(cond[:, k], [1 / 3, 2 / 3]) for k in range(3)]
    ids = np.zeros(len(patches), dtype=np.int64)
    for k in range(3):
        ids = ids * 3 + np.digitize(cond[:, k], edges[k])

    def pca(X):
        mean = X.mean(axis=0)
        Xc = (X - mean).astype(np.float64)
        _u, s, vt = np.linalg.svd(Xc, full_matrices=False)
        var = s ** 2
        cum = np.cumsum(var) / max(var.sum(), 1e-30)
        rank = int(np.searchsorted(cum, VAR_TARGET) + 1)
        rank = min(rank, MAX_RANK, len(s))
        std = s[:rank] / np.sqrt(max(len(X) - 1, 1))
        return mean.astype(np.float32), vt[:rank].astype(np.float32), std.astype(np.float32)

    global_ = pca(patches)
    buckets = {}
    members = {}
    for b in np.unique(ids):
        sel = ids == b
        members[int(b)] = np.flatnonzero(sel)
        if sel.sum() >= MIN_BUCKET:
            buckets[int(b)] = pca(patches[sel])
    lib_std = patches.std(axis=1)
    return dict(edges=edges, buckets=buckets, global_=global_,
                members=members, lib=patches, lib_std=lib_std,
                n_patches=len(patches), n_buckets=len(buckets))


# -------------------------------------------------------------- reconstruct

def hann2d():
    w = 0.5 * (1.0 - np.cos(2.0 * np.pi * np.arange(PATCH) / PATCH))  # periodic
    return np.outer(w, w)


def bucket_of(cvec, edges):
    b = 0
    for k in range(3):
        b = b * 3 + int(np.digitize(cvec[k], edges[k]))
    return b


def stable_seed(level, y, x):
    return (SEED * 0x9E3779B9 ^ (level * 0x85EBCA6B) ^ (y * 0xC2B2AE35) ^ x) & 0x7FFFFFFF


# Synthesis method. Two failed variants taught us the shape of the problem:
#   "pca" + independent draws  -> spectra in band, structure incoherent
#                                 (crumpled paper; no connected ravines)
#   "pca" + correlated fields  -> reinforces the patch lattice (checkerboard)
# "quilt" copies REAL patch content chosen by conditioning bucket + boundary
# compatibility with the canvas laid so far -- real within-patch structure,
# continuity from the boundary match.
METHOD = "quilt"
COEFF_CORR = 0.0
QUILT_CAP = 300     # candidate subset per site (speed)
QUILT_TOPK = 3      # seeded pick among the best-matching candidates
SYNTH_STRIDE = 24   # 25% overlap at synthesis: less cross-patch cancellation
                    # than 50% (acc_w-normalized, so COLA is not required)
QUILT_STD_FLOOR = 0.5  # candidates must carry >= this x bucket-median std --
                       # kills the L2 bias toward flat patches
QUILT_GRADIENT = True  # composite patch GRADIENTS along the seams and Poisson-
                       # integrate back to height: a seam through a height
                       # offset is a visible crease in hillshade; in the
                       # gradient domain offsets cannot exist by construction


def coeff_fields(ny, nx, n_comp, level_id):
    """One smooth standard-normal field per PCA component over the patch grid.

    Independent per-patch draws cannot produce structure that spans patches --
    the first spike run proved it visually (metrics in band, hillshade
    incoherent). Drawing each coefficient as a correlated FIELD preserves the
    marginals while letting adjacent patches agree, so ravines and spurs can
    continue across patch boundaries. Position-seeded per node, so still
    deterministic and order-independent.
    """
    F = np.empty((n_comp, ny, nx))
    for iy in range(ny):
        for ix in range(nx):
            rng = np.random.default_rng(stable_seed(level_id, iy, ix))
            F[:, iy, ix] = rng.standard_normal(n_comp)
    if COEFF_CORR > 0:
        F = ndimage.gaussian_filter(F, sigma=(0, COEFF_CORR, COEFF_CORR))
        sd = F.std(axis=(1, 2), keepdims=True)
        F /= np.maximum(sd, 1e-9)
    return F


def reconstruct_level(shape, cond, model, level_id):
    """Overlap-add reconstruction with correlated coefficient fields.

    Mean part COLA-normalized. With correlated neighbours the stochastic parts
    of overlapping patches reinforce like the mean (variance ~ (sum w)^2), so
    it is normalized the same way; the per-band closer owns final amplitude.
    """
    H, W = shape
    win = hann2d()
    acc_mean = np.zeros(shape); acc_sto = np.zeros(shape)
    acc_w = np.zeros(shape)
    ys, xs = patch_grid(H), patch_grid(W)
    n_comp = max(len(model["global_"][2]),
                 max((len(b[2]) for b in model["buckets"].values()), default=0))
    F = coeff_fields(len(ys), len(xs), n_comp, level_id)
    for iy, y in enumerate(ys):
        for ix, x in enumerate(xs):
            cvec = cond[y:y + PATCH, x:x + PATCH].reshape(-1, 3).mean(axis=0)
            mean, basis, std = model["buckets"].get(
                bucket_of(cvec, model["edges"]), model["global_"])
            coeff = F[:len(std), iy, ix] * std
            sto = (coeff[None, :] @ basis).reshape(PATCH, PATCH)
            acc_mean[y:y + PATCH, x:x + PATCH] += win * mean.reshape(PATCH, PATCH)
            acc_sto[y:y + PATCH, x:x + PATCH] += win * sto
            acc_w[y:y + PATCH, x:x + PATCH] += win
    eps = 1e-9
    return (acc_mean + acc_sto) / np.maximum(acc_w, eps)


def _seam_mask(err_v, err_h):
    """Take-patch mask for the overlap regions via min-error boundary cuts
    (Efros-Freeman). err_v: (P, OV) left strip errors or None; err_h:
    (OV, P) top strip errors or None. Returns (P, P) float mask, 1 = patch."""
    m = np.ones((PATCH, PATCH))
    if err_v is not None:
        ov = err_v.shape[1]
        cost = err_v.copy()
        for r in range(1, PATCH):
            prev = cost[r - 1]
            best = np.minimum(np.minimum(np.roll(prev, 1), prev), np.roll(prev, -1))
            best[0] = min(prev[0], prev[1]); best[-1] = min(prev[-1], prev[-2])
            cost[r] += best
        c = int(np.argmin(cost[-1]))
        seam = np.empty(PATCH, np.int64); seam[-1] = c
        for r in range(PATCH - 2, -1, -1):
            lo, hi = max(c - 1, 0), min(c + 2, ov)
            c = lo + int(np.argmin(cost[r, lo:hi])); seam[r] = c
        for r in range(PATCH):
            m[r, :seam[r]] = 0.0      # left of the seam keeps the canvas
    if err_h is not None:
        ov = err_h.shape[0]
        cost = err_h.copy()
        for cc in range(1, PATCH):
            prev = cost[:, cc - 1]
            best = np.minimum(np.minimum(np.roll(prev, 1), prev), np.roll(prev, -1))
            best[0] = min(prev[0], prev[1]); best[-1] = min(prev[-1], prev[-2])
            cost[:, cc] += best
        r = int(np.argmin(cost[:, -1]))
        seam = np.empty(PATCH, np.int64); seam[-1] = r
        for cc in range(PATCH - 2, -1, -1):
            lo, hi = max(r - 1, 0), min(r + 2, ov)
            r = lo + int(np.argmin(cost[lo:hi, cc])); seam[cc] = r
        for cc in range(PATCH):
            m[:seam[cc], cc] = 0.0    # above the seam keeps the canvas
    return m


def radial_power(img, cell, nbins=64):
    """Radially-binned power spectrum -> (bin_centers 1/m, mean power)."""
    F = np.fft.fft2(img)
    P = (F * np.conj(F)).real
    H, W = img.shape
    K = np.hypot(np.fft.fftfreq(H, cell)[:, None], np.fft.fftfreq(W, cell)[None, :])
    edges = np.linspace(0.0, float(K.max()) + 1e-12, nbins + 1)
    idx = np.clip(np.digitize(K.ravel(), edges) - 1, 0, nbins - 1)
    power = np.bincount(idx, P.ravel(), minlength=nbins)
    count = np.maximum(np.bincount(idx, minlength=nbins), 1)
    return 0.5 * (edges[:-1] + edges[1:]), power / count


def spectral_equalize(img, cell, tgt_centers, tgt_power, clip=(0.25, 4.0)):
    """Radial-gain filter matching img's band spectrum to the training-average
    shape. Deterministic; cannot reintroduce seams (pure radial filter). The
    production cousin of the calibrated sub-patch spectral fill."""
    F = np.fft.fft2(img)
    H, W = img.shape
    K = np.hypot(np.fft.fftfreq(H, cell)[:, None], np.fft.fftfreq(W, cell)[None, :])
    own_centers, own_power = radial_power(img, cell)
    own_on_tgt = np.interp(tgt_centers, own_centers, own_power)
    gain_r = np.sqrt(np.maximum(tgt_power, 0) / np.maximum(own_on_tgt, 1e-30))
    gain_r = np.clip(gain_r, clip[0], clip[1])
    G = np.interp(K, tgt_centers, gain_r)
    G[0, 0] = 1.0
    return np.fft.ifft2(F * G).real


def poisson_integrate(gx, gy, cell):
    """Solve lap(h) = div(g) with Neumann BC via DCT (mean-free)."""
    from scipy.fft import dctn, idctn
    div = np.gradient(gx, cell, axis=1) + np.gradient(gy, cell, axis=0)
    H, W = div.shape
    D = dctn(div, type=2, norm="ortho")
    iy = np.arange(H)[:, None]; ix = np.arange(W)[None, :]
    lam = (2.0 * np.cos(np.pi * iy / H) - 2.0 +
           2.0 * np.cos(np.pi * ix / W) - 2.0) / (cell * cell)
    lam[0, 0] = 1.0
    D /= lam
    D[0, 0] = 0.0
    h = idctn(D, type=2, norm="ortho")
    return h - h.mean()


def quilt_level(shape, cond, model, level_id, cell):
    """Exemplar quilting with min-error-boundary compositing: per site, copy a
    REAL training patch from the conditioning bucket (best boundary match among
    QUILT_TOPK, seeded), cut in along the minimum-error seam, small feather.
    Raster order, deterministic."""
    H, W = shape
    ov = PATCH - SYNTH_STRIDE
    canvas = np.zeros(shape)
    cgx = np.zeros(shape); cgy = np.zeros(shape)
    covered = np.zeros(shape, bool)
    lib = model["lib"]; lib_std = model["lib_std"]
    ys, xs = patch_grid(H, SYNTH_STRIDE), patch_grid(W, SYNTH_STRIDE)
    for iy, y in enumerate(ys):
        for ix, x in enumerate(xs):
            cvec = cond[y:y + PATCH, x:x + PATCH].reshape(-1, 3).mean(axis=0)
            b = bucket_of(cvec, model["edges"])
            cand = model["members"].get(b, np.empty(0, np.int64))
            if len(cand) < MIN_BUCKET:
                cand = np.arange(len(lib))
            med = float(np.median(lib_std[cand]))
            keep = cand[lib_std[cand] >= QUILT_STD_FLOOR * med]
            if len(keep) >= MIN_BUCKET:
                cand = keep
            rng = np.random.default_rng(stable_seed(level_id, y, x))
            if len(cand) > QUILT_CAP:
                cand = cand[rng.choice(len(cand), QUILT_CAP, replace=False)]
            reg = canvas[y:y + PATCH, x:x + PATCH]
            cov = covered[y:y + PATCH, x:x + PATCH]
            mflat = cov.ravel().astype(np.float64)
            if mflat.sum() > 0:
                t = reg.ravel()
                C = lib[cand].astype(np.float64)
                ssd = (C * C) @ mflat - 2.0 * (C @ (mflat * t))
                order = np.argsort(ssd, kind="stable")[:QUILT_TOPK]
                pick = cand[order[rng.integers(len(order))]]
            else:
                pick = cand[rng.integers(len(cand))]
            patch = lib[pick].reshape(PATCH, PATCH).astype(np.float64)
            err = (patch - reg) ** 2
            err_v = err[:, :ov] if (ix > 0 and cov[:, 0].any()) else None
            err_h = err[:ov, :] if (iy > 0 and cov[0, :].any()) else None
            m = _seam_mask(err_v, err_h)
            m = np.where(cov, m, 1.0)          # uncovered cells always take patch
            m = ndimage.gaussian_filter(m, 1.0)  # ~2 px feather over the cut
            canvas[y:y + PATCH, x:x + PATCH] = reg * (1.0 - m) + patch * m
            if QUILT_GRADIENT:
                gpy, gpx = np.gradient(patch, cell)
                rgx = cgx[y:y + PATCH, x:x + PATCH]
                rgy = cgy[y:y + PATCH, x:x + PATCH]
                cgx[y:y + PATCH, x:x + PATCH] = rgx * (1.0 - m) + gpx * m
                cgy[y:y + PATCH, x:x + PATCH] = rgy * (1.0 - m) + gpy * m
            covered[y:y + PATCH, x:x + PATCH] = True
    if QUILT_GRADIENT:
        return poisson_integrate(cgx, cgy, cell)
    return canvas


def synth_level(shape, cond, model, level_id, cell):
    if METHOD == "quilt":
        return quilt_level(shape, cond, model, level_id, cell)
    return reconstruct_level(shape, cond, model, level_id)


# ------------------------------------------------------------------ scoring

def battery(z, cell, cfg, axes):
    valid = np.isfinite(z)
    scalars, dists = features.compute(z.astype(np.float64), cell, valid, cfg, axes)
    return scalars, dists


def hillshade(z, cell, az_deg=315.0, alt_deg=45.0):
    gy, gx = np.gradient(z, cell)
    slope = np.arctan(np.hypot(gx, gy))
    aspect = np.arctan2(-gx, gy)
    az, alt = np.radians(az_deg), np.radians(alt_deg)
    hs = np.sin(alt) * np.cos(slope) + np.cos(alt) * np.sin(slope) * np.cos(az - aspect)
    hs = np.clip(hs, 0, 1)
    lo, hi = np.percentile(hs, [1, 99])
    return (np.clip((hs - lo) / max(hi - lo, 1e-9), 0, 1) * 255).astype(np.uint8)


def save_png(arr, path):
    Image.fromarray(arr).save(path)


# --------------------------------------------------------------------- main

def main():
    os.makedirs(OUT, exist_ok=True)
    os.makedirs(os.path.join(REPORT, "blind"), exist_ok=True)
    cfg = yaml.safe_load(open(os.path.join(REPO, "tools/metrics/metrics/config.yaml")))
    axes = features.build_axes(cfg)

    # ---- A1: extract from training tiles
    print("== A1: extracting training patches ==")
    fine_P, fine_C, mid_P, mid_C = [], [], [], []
    fine_spec, mid_spec = [], []
    cell = None
    for name in TRAIN:
        z, cell = load(name)
        d = decompose(z, cell)
        P, C = extract(d["fine"], d["cond"])
        fine_P.append(P); fine_C.append(C)
        P, C = extract(d["mid8"], d["cond8"])
        mid_P.append(P); mid_C.append(C)
        fine_spec.append(radial_power(d["fine"], cell))
        mid_spec.append(radial_power(d["mid8"], cell * 4))
        print(f"  {name}: fine {len(fine_P[-1])} patches, mid {len(mid_P[-1])}")
    fine_tgt = (fine_spec[0][0], np.mean([s[1] for s in fine_spec], axis=0))
    mid_tgt = (mid_spec[0][0], np.mean([s[1] for s in mid_spec], axis=0))
    fine_P = np.concatenate(fine_P); fine_C = np.concatenate(fine_C)
    mid_P = np.concatenate(mid_P); mid_C = np.concatenate(mid_C)

    # ---- A2: fit
    print("== A2: fitting ==")
    fine_model = fit_level(fine_P, fine_C)
    mid_model = fit_level(mid_P, mid_C)
    for tag, m, P in (("fine", fine_model, fine_P), ("mid", mid_model, mid_P)):
        ranks = [len(b[2]) for b in m["buckets"].values()]
        print(f"  {tag}: {m['n_patches']} patches, {m['n_buckets']}/27 buckets "
              f"(min {MIN_BUCKET}), rank med {int(np.median(ranks))} "
              f"max {max(ranks)}, global rank {len(m['global_'][2])}")

    # ---- A3: reconstruct held-out
    print("== A3: reconstructing held-out ==")
    z_h, cell = load(HOLDOUT)
    d_h = decompose(z_h, cell)
    fine_rec = synth_level(d_h["fine"].shape, d_h["cond"], fine_model, 1, cell)
    mid_rec8 = synth_level(d_h["mid8"].shape, d_h["cond8"], mid_model, 2, cell * 4)
    if METHOD == "quilt" and QUILT_GRADIENT:
        # Poisson integration invents long-wavelength swell from seam gradient
        # inconsistencies (1/lambda amplifies low frequencies). An integrated
        # BAND must stay in its band: high-pass each back to its own range.
        fine_rec -= ndimage.gaussian_filter(fine_rec, SIGMA_PER_L * L_MID / cell)
        mid_rec8 -= ndimage.gaussian_filter(mid_rec8, SIGMA_PER_L * L_MACRO / (cell * 4))
        # ...then true up the within-band spectrum to the training average --
        # integration + band-limiting reshape it, and a scalar closer cannot
        # fix a SHAPE drift.
        fine_rec = spectral_equalize(fine_rec, cell, *fine_tgt)
        mid_rec8 = spectral_equalize(mid_rec8, cell * 4, *mid_tgt)
    mid_rec = ndimage.zoom(mid_rec8, 4, order=3)
    assert mid_rec.shape == d_h["fine"].shape
    real_resid = d_h["filled"] - d_h["lp400"]
    raw_std = float((mid_rec + fine_rec).std())

    # The relief-budget CLOSER (stage-03 responsibility 6), PER BAND -- the
    # production design: each band is scaled to the site's amplitude target.
    # In the pipeline the targets are S0's drawn descriptors; for the held-out
    # test the honest stand-in is the held-out tile's own per-band std.
    real_mid = ndimage.zoom(d_h["mid8"], 4, order=3)
    real_fine = d_h["fine"]
    c_mid = float(real_mid.std()) / max(float(mid_rec.std()), 1e-9)
    c_fine = float(real_fine.std()) / max(float(fine_rec.std()), 1e-9)
    recon_resid = mid_rec * c_mid + fine_rec * c_fine
    recon_full = d_h["lp400"] + recon_resid
    closer = dict(mid=c_mid, fine=c_fine)
    print(f"  residual std: real {real_resid.std():.3f} m, raw {raw_std:.3f} m, "
          f"closer mid x{c_mid:.3f} fine x{c_fine:.3f} "
          f"-> {recon_resid.std():.3f} m")

    # ---- A4: score
    print("== A4: scoring ==")
    rows = {}
    real_r_s, real_r_d = battery(real_resid, cell, cfg, axes)
    rec_r_s, rec_r_d = battery(recon_resid, cell, cfg, axes)
    real_f_s, _ = battery(d_h["filled"], cell, cfg, axes)
    rec_f_s, _ = battery(recon_full, cell, cfg, axes)

    # 6-tile real residual spread = the band
    band = {}
    for name in TRAIN + [HOLDOUT]:
        z, c2 = load(name)
        dd = decompose(z, c2)
        s, _ = battery(dd["filled"] - dd["lp400"], c2, cfg, axes)
        for k in KEY_METRICS:
            band.setdefault(k, []).append(s.get(k))
    for k in KEY_METRICS:
        vals = [v for v in band[k] if v is not None and np.isfinite(v)]
        lo, hi = (min(vals), max(vals)) if vals else (np.nan, np.nan)
        rows[k] = dict(recon=rec_r_s.get(k), holdout=real_r_s.get(k),
                       band_lo=lo, band_hi=hi,
                       in_band=bool(vals and lo <= (rec_r_s.get(k) or np.nan) <= hi))

    # PSD seam check: recon/real ratio, flag pitch wavelengths
    wl = axes.get("psd_wavelength_m")
    psd_real = real_r_d["radial_psd"]; psd_rec = rec_r_d["radial_psd"]
    with np.errstate(divide="ignore", invalid="ignore"):
        ratio = np.where(psd_real > 0, psd_rec / psd_real, np.nan)
    seam = dict(ratio=ratio.tolist())
    if wl is not None:
        seam["wavelength_m"] = np.asarray(wl).tolist()
        for pitch in (STRIDE * 2.0, STRIDE * 8.0):  # 32 m fine, 128 m mid pitch
            i = int(np.nanargmin(np.abs(np.asarray(wl) - pitch)))
            nb = [j for j in (i - 2, i - 1, i + 1, i + 2) if 0 <= j < len(ratio)]
            local = np.nanmedian([ratio[j] for j in nb])
            seam[f"peak_at_{int(pitch)}m"] = dict(
                ratio=float(ratio[i]), neighborhood=float(local),
                excess=float(ratio[i] / local) if local else None)

    # ---- A5: renders
    print("== A5: rendering ==")
    hs_real = hillshade(d_h["filled"], cell)
    hs_rec = hillshade(recon_full, cell)
    # Variant B: the dictionary supplies ONLY the <64 m band; the 64-400 m
    # band is the real tile's own -- evidence for where the S2/S3 ownership
    # boundary should sit (if B looks real, "S2 owns 64-400" is viable).
    vb_full = d_h["lp400"] + real_mid + fine_rec * c_fine
    hs_vb = hillshade(vb_full, cell)
    for i, (cy, cx) in enumerate([(100, 100), (900, 900)]):
        r = hs_real[cy:cy + 512, cx:cx + 512]
        g = hs_vb[cy:cy + 512, cx:cx + 512]
        pair = np.concatenate([r, np.full((512, 8), 255, np.uint8), g], axis=1)
        save_png(pair, os.path.join(REPORT, f"variantB_{i}_real_left.png"))
    crops = [(100, 100), (100, 900), (900, 100), (900, 900)]
    entries = []
    for i, (cy, cx) in enumerate(crops):
        r = hs_real[cy:cy + 512, cx:cx + 512]
        g = hs_rec[cy:cy + 512, cx:cx + 512]
        pair = np.concatenate([r, np.full((512, 8), 255, np.uint8), g], axis=1)
        save_png(pair, os.path.join(REPORT, f"pair_{i}_real_left.png"))
        entries.append((f"crop{i}", "real", r))
        entries.append((f"crop{i}", "recon", g))
    rng = np.random.default_rng(SEED + 99)
    order = rng.permutation(len(entries))
    key = {}
    for j, idx in enumerate(order):
        cid, kind, img = entries[idx]
        name = f"blind_{chr(ord('a') + j)}.png"
        save_png(img, os.path.join(REPORT, "blind", name))
        key[name] = dict(crop=cid, kind=kind)
    json.dump(key, open(os.path.join(REPORT, "blind", "answer_key.json"), "w"), indent=1)

    # ---- report
    result = dict(
        train=TRAIN, holdout=HOLDOUT, seed=SEED, cell_m=cell, method=METHOD,
        levels=dict(mid=dict(band_m=[L_MID, L_MACRO], grid_m=cell * 4,
                             patch=PATCH, model_buckets=mid_model["n_buckets"]),
                    fine=dict(band_m=[0, L_MID], grid_m=cell,
                              patch=PATCH, model_buckets=fine_model["n_buckets"])),
        residual_std=dict(real=float(real_resid.std()), recon_raw=raw_std,
                          closer=closer, recon=float(recon_resid.std())),
        metric_classes=dict(shape=SHAPE_METRICS, mixed=MIXED_METRICS,
                            amplitude=AMPL_METRICS),
        metrics=rows, seam=seam,
        full_surface=dict(
            real={k: real_f_s.get(k) for k in KEY_METRICS},
            recon={k: rec_f_s.get(k) for k in KEY_METRICS}),
    )
    json.dump(result, open(os.path.join(OUT, "metrics.json"), "w"), indent=1,
              default=float)

    cls = {**{k: "shape" for k in SHAPE_METRICS},
           **{k: "mixed" for k in MIXED_METRICS},
           **{k: "amplitude*" for k in AMPL_METRICS}}
    lines = [f"# G-SPIKE report — method: {METHOD}", "",
             f"Train: {', '.join(TRAIN)}  ·  Held out: **{HOLDOUT}**  ·  seed {SEED}",
             "",
             f"Residual std: real **{real_resid.std():.3f} m**, recon raw "
             f"**{raw_std:.3f} m**; per-band budget closer mid ×{c_mid:.3f}, "
             f"fine ×{c_fine:.3f} (the stage-03 closer — in production the "
             f"target is S0's drawn site-amplitude descriptor).", "",
             "**Reading the table:** *shape* metrics are scale-invariant — the "
             "dictionary must earn them; *amplitude\** metrics are set by the "
             "closer and verify self-consistency only; *mixed* = shape given "
             "correct amplitude.", "",
             "| metric (residual) | class | recon | held-out real | 6-tile band | in band |",
             "|---|---|---|---|---|---|"]
    for k in KEY_METRICS:
        r = rows[k]
        fmt = lambda v: "—" if v is None or not np.isfinite(v) else f"{v:.4g}"
        lines.append(f"| {k} | {cls[k]} | {fmt(r['recon'])} | {fmt(r['holdout'])} | "
                     f"[{fmt(r['band_lo'])}, {fmt(r['band_hi'])}] | "
                     f"{'**yes**' if r['in_band'] else 'NO'} |")
    lines += ["", "## Seam check (PSD ratio recon/real at patch pitch)", ""]
    for pk in ("peak_at_32m", "peak_at_128m"):
        if pk in seam:
            s = seam[pk]
            lines.append(f"- {pk}: ratio {s['ratio']:.2f} vs neighborhood "
                         f"{s['neighborhood']:.2f} → excess {s['excess']:.2f} "
                         f"({'OK' if s['excess'] and s['excess'] < 1.5 else 'CHECK'})")
    lines += ["", "## Blind A/B", "",
              "`report/blind/blind_*.png` — sort each as real or generated;",
              "then check `answer_key.json`. Gate: reviewer accuracy ≤ 75%.", ""]
    open(os.path.join(REPORT, "REPORT.md"), "w").write("\n".join(lines))
    n_shape = sum(1 for k in SHAPE_METRICS + MIXED_METRICS if rows[k]["in_band"])
    n_all = sum(1 for r in rows.values() if r["in_band"])
    print(f"\nDONE: shape+mixed {n_shape}/{len(SHAPE_METRICS + MIXED_METRICS)} "
          f"in band (the earned ones); all {n_all}/{len(rows)}. "
          f"Report: tools/spike/report/REPORT.md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
