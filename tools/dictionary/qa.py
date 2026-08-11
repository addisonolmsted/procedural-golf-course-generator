"""F3 — held-out QA: can the dictionary rebuild tiles it never saw?

For every held-out tile (recorded in the asset header, excluded from
harvest by construction): reconstruct the mid and fine residual bands by
position-seeded patch quilting — bucket lookup from the tile's OWN
conditioning, gradient-domain Hann overlap-add with best-of-K candidate
matching in the overlap, Poisson integration (DCT), per-bucket amplitude
close — then score the reconstruction against the tile's real band and
the biome's real corpus band on the E6 discriminants (mid_std, fine_std,
radial-PSD log-slope). Renders one recon-vs-real hillshade pair per biome.

    python tools/dictionary/qa.py    # -> report appended + out/qa/*.png
"""
import hashlib
import json
import pathlib
import struct
import sys

import numpy as np
from scipy import fft as sfft

ROOT = pathlib.Path(__file__).resolve().parent.parent.parent
sys.path.insert(0, str(ROOT / "tools" / "macro_campaign"))
from macro_campaign import extract_v2  # noqa: E402

ASSET = ROOT / "assets" / "dictionary_v2.bin"
OUT = ROOT / "tools" / "dictionary" / "out" / "qa"
REPORT = ROOT / "tools" / "dictionary" / "report.md"
PATCH, STRIDE, K_CAND = 32, 16, 3


def load_asset():
    b = ASSET.read_bytes()
    assert b[:4] == b"CDIC"
    hlen = struct.unpack("<I", b[8:12])[0]
    header = json.loads(b[12:12 + hlen])
    blob = b[12 + hlen:]
    return header, blob


def get_patch(blob, entry):
    n = PATCH * PATCH
    q = np.frombuffer(blob, dtype="<i2", count=n, offset=entry["offset"])
    return q.astype(np.float32).reshape(PATCH, PATCH) * entry["scale"]


def bucket_id(cv, edges):
    idx = 0
    for k, e in enumerate(edges):
        idx = idx * (len(e) + 1) + int(np.searchsorted(e, cv[k]))
    return idx


def nearest_bucket(bid, buckets, edges):
    if str(bid) in buckets:
        return str(bid)
    dims = [len(e) + 1 for e in edges]

    def unpack(i):
        out = []
        for d in reversed(dims):
            out.append(i % d)
            i //= d
        return list(reversed(out))

    me = unpack(bid)
    best, bd = None, 1e9
    for key in buckets:
        d = sum(abs(a - b) for a, b in zip(me, unpack(int(key))))
        if d < bd:
            bd, best = d, key
    return best


def hann2d():
    w = np.hanning(PATCH + 1)[:PATCH]
    return np.outer(w, w) + 1e-6


def stable_pick(seed_words, n, k):
    h = hashlib.sha256(("|".join(map(str, seed_words))).encode()).digest()
    picks = []
    for i in range(k):
        v = int.from_bytes(h[4 * i:4 * i + 4], "little")
        picks.append(v % n)
    return picks


def poisson_integrate(gx, gy):
    """Least-squares height field from PER-CELL gradients via DCT
    (Neumann). Everything stays in cell units: np.gradient gives dz per
    cell, the discrete Laplacian denominator is per cell, so no physical
    cell-size factor may appear anywhere — the first version divided the
    divergence by cell and multiplied the solve by cell**2, a net x8
    amplitude error at 8 m that failed every F3 reconstruction."""
    H, W = gx.shape
    div = np.zeros((H, W))
    div[:, 1:] += gx[:, 1:] - gx[:, :-1]
    div[1:, :] += gy[1:, :] - gy[:-1, :]
    d = sfft.dctn(div, norm="ortho")
    yy = np.arange(H)[:, None]
    xx = np.arange(W)[None, :]
    denom = (2 * np.cos(np.pi * yy / H) - 2) + (2 * np.cos(np.pi * xx / W) - 2)
    denom[0, 0] = 1.0
    z = sfft.idctn(d / denom, norm="ortho")
    return z - z.mean()


def _radial16(p):
    f = np.fft.fftshift(np.abs(np.fft.fft2(p)) ** 2)
    n = p.shape[0]
    yy, xx = np.mgrid[0:n, 0:n]
    r = np.hypot(yy - n / 2, xx - n / 2)
    bins = np.linspace(0, n / 2, 17)
    out = np.zeros(16)
    for i in range(16):
        m = (r >= bins[i]) & (r < bins[i + 1])
        out[i] = float(f[m].mean()) if m.any() else 0.0
    return out


def grid(n):
    xs = list(range(0, n - PATCH + 1, STRIDE))
    if xs[-1] != n - PATCH:
        xs.append(n - PATCH)
    return xs


def reconstruct(level_rec, blob, cond8, shape, level, seed):
    n = shape[0]
    cs, cw = (1, 32) if level == "mid" else (4, 8)
    gx = np.zeros(shape)
    gy = np.zeros(shape)
    wsum = np.zeros(shape)
    amp_t = np.zeros(shape)
    eq_t = np.zeros(16)
    eq_n = 0
    win = hann2d()
    edges = level_rec["edges"]
    buckets = level_rec["buckets"]
    for y in grid(n):
        for x in grid(n):
            c = cond8[y // cs:y // cs + cw, x // cs:x // cs + cw]
            m = np.nanmean(c.reshape(-1, 5), axis=0)
            cv = [m[0], m[1], m[2], np.log1p(max(m[4], 0.0))]
            key = nearest_bucket(bucket_id(cv, edges), buckets, edges)
            b = buckets[key]
            cands = stable_pick(["f3", level, seed, y, x], len(b["patches"]), K_CAND)
            # best-of-K: match the already-built canvas in the overlap
            best_p, best_score = None, -1e18
            cur = None
            wreg = wsum[y:y + PATCH, x:x + PATCH]
            if wreg.max() > 0:
                cur = gx[y:y + PATCH, x:x + PATCH] / np.maximum(wreg, 1e-9)
            for ci in cands:
                p = get_patch(blob, b["patches"][ci])
                pgy, pgx = np.gradient(p)
                if cur is None:
                    best_p = (pgx, pgy)
                    break
                mask = wreg > 0.05
                s = float((pgx * cur)[mask].sum())
                if s > best_score:
                    best_score, best_p = s, (pgx, pgy)
            pgx, pgy = best_p
            gx[y:y + PATCH, x:x + PATCH] += pgx * win
            gy[y:y + PATCH, x:x + PATCH] += pgy * win
            wsum[y:y + PATCH, x:x + PATCH] += win
            amp_t[y:y + PATCH, x:x + PATCH] += b["amp_p50"] * win
            eq_t += np.array(b["equalizer"])
            eq_n += 1
    gx /= np.maximum(wsum, 1e-9)
    gy /= np.maximum(wsum, 1e-9)
    amp_t /= np.maximum(wsum, 1e-9)
    z = poisson_integrate(gx, gy)
    # RE-BAND-LIMIT (the spike's step this QA first skipped): integrating
    # a patchwork of mutually inconsistent gradients pumps spurious power
    # into wavelengths ABOVE the band — the first run's reconstructions
    # had psd slopes of -4.6..-5.5 vs real -3.3..-4.3 and stds pinned to
    # the closer's clamp. High-pass at the band's upper cut with the same
    # half-amplitude Gaussian the extraction used.
    from scipy import ndimage
    sigma_per_l = float(np.sqrt(np.log(2.0) / (2.0 * np.pi ** 2)))
    lam_cut, cell = (400.0, 8.0) if level == "mid" else (64.0, 2.0)
    z = z - ndimage.gaussian_filter(z, sigma_per_l * lam_cut / cell)
    # RADIAL SPECTRAL EQUALIZER (the spike's second post-step): Hann
    # overlap-add of uncorrelated patches lowpasses the mosaic (first
    # equalizer-less run: psd slopes ~1 too steep everywhere). Target =
    # the asset's per-bucket radial-PSD means; actual = the mosaic's own
    # patch-window PSD in the identical 16-bin convention; per-bin gain
    # applied in the tile FFT with radius mapped to patch bins.
    tgt = eq_t / max(eq_n, 1)
    fy = np.fft.fftfreq(n)[:, None]
    fx = np.fft.fftfreq(n)[None, :]
    rbin = np.clip((np.hypot(fy, fx) * PATCH).astype(int), 0, 15)
    # Two measured passes with a CUMULATIVE gain clamp. The naive 3x
    # iteration diverged 60-250x: the act-measurement removes each
    # window's mean, so the lowest bins re-earn max gain every pass and
    # compound. Bins 0-1 are excluded outright (a mean-removed 32-cell
    # window cannot measure them; the band-limit owns that end), and the
    # TOTAL applied gain is clamped to [0.4, 5] relative to the original.
    g_tot = np.ones(16)
    for _ in range(2):
        act = np.zeros(16)
        cnt = 0
        for yy in range(0, n - PATCH, 64):
            for xx in range(0, n - PATCH, 64):
                w32 = z[yy:yy + PATCH, xx:xx + PATCH]
                act += _radial16(w32 - w32.mean())
                cnt += 1
        act /= max(cnt, 1)
        gains = np.sqrt(np.maximum(tgt, 1e-20) / np.maximum(act, 1e-20))
        gains[:2] = 1.0
        allowed = np.clip(g_tot * gains, 0.4, 5.0) / g_tot
        g_tot *= allowed
        z = np.real(np.fft.ifft2(np.fft.fft2(z) * allowed[rbin]))
        if float(np.abs(np.log(allowed)).max()) < 0.1:
            break
    # amplitude close: local std (over ~4 patch widths) driven to the
    # bucket target — the same rule S3 will apply
    w = PATCH * 2
    mu = ndimage.uniform_filter(z, w)
    var = ndimage.uniform_filter(z * z, w) - mu * mu
    local = np.sqrt(np.maximum(var, 1e-12))
    # real patches already carry real amplitude; the closer only corrects
    # the overlap-averaging loss, so the gain stays near 1
    gain = np.clip(amp_t / np.maximum(local, 1e-6), 0.5, 2.5)
    return z * gain


def psd_slope(z, cell, lam_band):
    """Radial log-log PSD slope fitted INSIDE the level's own band —
    outside it both fields are near the numeric floor and the fit is
    noise (symmetric for real and recon either way; in-band is the claim
    that matters)."""
    f = np.abs(np.fft.fft2(z - z.mean())) ** 2
    n = z.shape[0]
    fr = np.fft.fftfreq(n, d=cell)
    fy, fx = np.meshgrid(fr, fr, indexing="ij")
    r = np.hypot(fx, fy).ravel()
    p = f.ravel()
    lo, hi = lam_band
    m = (r > 1.0 / hi) & (r < 1.0 / lo)
    lr, lp = np.log(r[m]), np.log(p[m] + 1e-20)
    return float(np.polyfit(lr, lp, 1)[0])


def hillshade(z, cell, ex=1.0):
    gy, gx = np.gradient(z * ex, cell)
    az, alt = np.radians(315), np.radians(45)
    sl = np.arctan(np.hypot(gx, gy))
    asp = np.arctan2(-gx, gy)
    hs = np.sin(alt) * np.cos(sl) + np.cos(alt) * np.sin(sl) * np.cos(az - asp)
    return np.clip((hs + 0.15) / 1.15 * 255, 0, 255).astype(np.uint8)


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    header, blob = load_asset()
    # per-biome real bands from every kept tile's scalars
    bands = {}
    for a, t in extract_v2.kept_tiles():
        j = json.loads((extract_v2.OUT / "extract_v2" / a / f"{t}.json").read_text())
        bands.setdefault(a, {"fine": [], "mid": []})
        bands[a]["fine"].append(j["fine_std_m"])
        bands[a]["mid"].append(j["mid_std_m"])
    lines = ["\n## F3 held-out QA (reconstruction vs real)\n",
             "| biome | tile | mid_std real/recon | fine_std real/recon | "
             "psd-slope real/recon (fine) | in-band? |",
             "|---|---|---|---|---|---|"]
    n_pass = n_tot = 0
    for biome, holds in header["holdout_tiles"].items():
        brec = header["biomes"][biome]
        fb = np.array(bands[biome]["fine"])
        mb = np.array(bands[biome]["mid"])
        f_lo, f_hi = np.percentile(fb, 10), np.percentile(fb, 90)
        m_lo, m_hi = np.percentile(mb, 10), np.percentile(mb, 90)
        rendered = False
        for tid in holds:
            npz = np.load(extract_v2.OUT / "extract_v2" / biome / f"{tid}.npz")
            cond8 = npz["cond8"].astype(np.float64)
            clean = npz["clean"]
            recon_m = reconstruct(brec["mid"], blob, cond8, (375, 375), "mid", tid)
            recon_f = reconstruct(brec["fine"], blob, cond8, (1500, 1500), "fine", tid)
            real_m = npz["mid8"].astype(np.float64)
            real_f = npz["fine"].astype(np.float64)
            cl8 = clean[::4, ::4]
            ms_r, ms_g = float(np.std(real_m[cl8])), float(np.std(recon_m[cl8]))
            fs_r, fs_g = float(np.std(real_f[clean])), float(np.std(recon_f[clean]))
            ps_r, ps_g = (psd_slope(real_f, 2.0, (5.0, 55.0)),
                          psd_slope(recon_f, 2.0, (5.0, 55.0)))
            ok = (m_lo * 0.8 <= ms_g <= m_hi * 1.2) and (f_lo * 0.8 <= fs_g <= f_hi * 1.2) \
                and abs(ps_g - ps_r) < 0.8
            n_pass += ok
            n_tot += 1
            lines.append(f"| {biome} | {tid} | {ms_r:.2f}/{ms_g:.2f} | "
                         f"{fs_r:.2f}/{fs_g:.2f} | {ps_r:.2f}/{ps_g:.2f} | "
                         f"{'PASS' if ok else 'FAIL'} |")
            print(lines[-1], flush=True)
            if not rendered:
                try:
                    from PIL import Image
                    pair = np.concatenate([hillshade(real_f, 2.0),
                                           np.full((1500, 8), 255, np.uint8),
                                           hillshade(recon_f, 2.0)], axis=1)
                    Image.fromarray(pair).resize((1130, 562)).save(OUT / f"{biome}_{tid}_fine.png")
                    pairm = np.concatenate([hillshade(real_m, 8.0, 2.0),
                                            np.full((375, 4), 255, np.uint8),
                                            hillshade(recon_m, 8.0, 2.0)], axis=1)
                    Image.fromarray(pairm).resize((1130, 562)).save(OUT / f"{biome}_{tid}_mid.png")
                    rendered = True
                except ImportError:
                    pass
    lines.append(f"\n**{n_pass}/{n_tot} held-out reconstructions in-band.** "
                 "(band = biome p10–p90 ±20%, psd-slope within 0.8)")
    with open(REPORT, "a") as f:
        f.write("\n".join(lines) + "\n")
    print(f"\n{n_pass}/{n_tot} PASS -> report appended, renders in {OUT}")


if __name__ == "__main__":
    main()
